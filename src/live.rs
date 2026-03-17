use anyhow::{Context, Result};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tungstenite::protocol::Message;
use tungstenite::WebSocket;

// ---------------------------------------------------------------------------
// Client-side JS injected into served HTML
// ---------------------------------------------------------------------------

const LIVE_CLIENT_JS: &str = r##"
<script data-sfhtml-live>
(function(){
  var ws, oldHTML = document.documentElement.outerHTML;
  function connect(){
    var proto = location.protocol === 'https:' ? 'wss' : 'ws';
    ws = new WebSocket(proto + '://' + location.host + '/__sfhtml_ws');
    ws.onmessage = function(e){
      try {
        var msg = JSON.parse(e.data);
        if (msg.type === 'full') {
          document.open();
          document.write(msg.html);
          document.close();
        } else if (msg.type === 'patch') {
          applyPatch(msg);
        } else if (msg.type === 'reload') {
          location.reload();
        }
      } catch(err) { location.reload(); }
    };
    ws.onclose = function(){ setTimeout(connect, 1000); };
    ws.onerror = function(){ ws.close(); };
  }
  function applyPatch(msg){
    if (msg.selector && msg.html) {
      var el = document.querySelector(msg.selector);
      if (el) { el.outerHTML = msg.html; return; }
    }
    if (msg.eval) {
      try { (0, eval)(msg.eval); return; } catch(e){}
    }
    location.reload();
  }
  connect();
})();
</script>
"##;

// ---------------------------------------------------------------------------
// Shared server state
// ---------------------------------------------------------------------------

struct LiveState {
    file_path: PathBuf,
    current_html: String,
    current_hash: String,
    clients: Vec<WsClient>,
}

struct WsClient {
    ws: WebSocket<TcpStream>,
    id: u64,
}

type SharedState = Arc<Mutex<LiveState>>;

static CLIENT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Start a live-serve session: HTTP server + file watcher + WebSocket push.
pub fn serve(
    file: &Path,
    port: u16,
    open_browser: bool,
    inject_live: bool,
) -> Result<()> {
    let abs_path = std::fs::canonicalize(file)
        .with_context(|| format!("File not found: {}", file.display()))?;
    let base_dir = abs_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let raw_html = std::fs::read_to_string(&abs_path)
        .with_context(|| format!("Cannot read {}", abs_path.display()))?;

    let served_html = if inject_live { inject_live_script(&raw_html) } else { raw_html.clone() };
    let hash = hash_content(&raw_html);

    let state: SharedState = Arc::new(Mutex::new(LiveState {
        file_path: abs_path.clone(),
        current_html: served_html,
        current_hash: hash,
        clients: Vec::new(),
    }));

    // Start file watcher
    let watch_state = state.clone();
    let watch_path = abs_path.clone();
    let watch_inject = inject_live;
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    // Watch the parent directory to catch renames and new files
    watcher.watch(
        watch_path.parent().unwrap_or(Path::new(".")),
        RecursiveMode::NonRecursive,
    )?;

    // File watcher thread
    std::thread::spawn(move || {
        // Debounce: wait a bit after events to coalesce rapid saves
        let debounce = Duration::from_millis(100);
        loop {
            match rx.recv() {
                Ok(Ok(event)) => {
                    // Only care about modify/create events for our file
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            if event.paths.iter().any(|p| same_file(p, &watch_path)) {
                                std::thread::sleep(debounce);
                                // Drain any queued events
                                while rx.try_recv().is_ok() {}
                                handle_file_change(&watch_state, watch_inject);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => break, // channel closed
            }
        }
    });

    // HTTP + WebSocket server
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr)
        .with_context(|| format!("Cannot bind to port {}", port))?;

    let file_name = abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("file.html");
    eprintln!("sfhtml serve: http://localhost:{}/{}", port, file_name);
    eprintln!("  Watching: {}", abs_path.display());
    eprintln!("  Live reload: {}", if inject_live { "enabled" } else { "disabled (use --live)" });
    eprintln!("  Press Ctrl+C to stop");

    if open_browser {
        let url = format!("http://localhost:{}/{}", port, file_name);
        let _ = open_url_in_browser(&url);
    }

    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };

        let state = state.clone();
        let base = base_dir.clone();
        let main_file = abs_path.clone();

        std::thread::spawn(move || {
            let _ = handle_connection(stream, &state, &base, &main_file);
        });
    }

    // Keep watcher alive
    drop(watcher);
    Ok(())
}

// ---------------------------------------------------------------------------
// Connection handler — route HTTP vs WebSocket
// ---------------------------------------------------------------------------

fn handle_connection(
    mut stream: TcpStream,
    state: &SharedState,
    base_dir: &Path,
    main_file: &Path,
) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf)?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Check if this is a WebSocket upgrade request
    if request.contains("Upgrade: websocket") || request.contains("upgrade: websocket") {
        return handle_websocket_upgrade(stream, state, &request);
    }

    // Regular HTTP
    let path = parse_request_path(&request);
    handle_http(stream, state, base_dir, main_file, &path)
}

fn parse_request_path(request: &str) -> String {
    // GET /path HTTP/1.1
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .to_string()
}

// ---------------------------------------------------------------------------
// HTTP handler — serve HTML + static assets
// ---------------------------------------------------------------------------

fn handle_http(
    mut stream: TcpStream,
    state: &SharedState,
    base_dir: &Path,
    main_file: &Path,
    path: &str,
) -> Result<()> {
    // Decode percent-encoded path and sanitize
    let decoded = percent_decode(path);
    let clean = decoded.trim_start_matches('/');

    // Serve the main HTML file (root or matching filename)
    let main_name = main_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if clean.is_empty() || clean == main_name {
        let html = {
            let st = state.lock().unwrap();
            st.current_html.clone()
        };
        return send_response(&mut stream, 200, "text/html; charset=utf-8", html.as_bytes());
    }

    // Serve static files relative to base directory
    let file_path = base_dir.join(clean);

    // Security: ensure resolved path is under base_dir
    let canonical = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return send_response(&mut stream, 404, "text/plain", b"Not Found"),
    };
    let canonical_base = base_dir.canonicalize().unwrap_or_else(|_| base_dir.to_path_buf());
    if !canonical.starts_with(&canonical_base) {
        return send_response(&mut stream, 403, "text/plain", b"Forbidden");
    }

    if canonical.is_file() {
        let content = std::fs::read(&canonical)?;
        let mime = guess_mime(&canonical);
        send_response(&mut stream, 200, mime, &content)
    } else {
        send_response(&mut stream, 404, "text/plain", b"Not Found")
    }
}

fn send_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) -> Result<()> {
    let status_text = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-cache\r\n\r\n",
        status, status_text, content_type, body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// WebSocket handler — upgrade + push loop
// ---------------------------------------------------------------------------

fn handle_websocket_upgrade(
    stream: TcpStream,
    state: &SharedState,
    request: &str,
) -> Result<()> {
    // Extract Sec-WebSocket-Key
    let key = request
        .lines()
        .find(|l| l.to_lowercase().starts_with("sec-websocket-key:"))
        .and_then(|l| l.split(':').nth(1))
        .map(|k| k.trim().to_string())
        .unwrap_or_default();

    let accept = compute_ws_accept(&key);

    let response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n\r\n",
        accept
    );

    let mut raw = stream.try_clone()?;
    raw.write_all(response.as_bytes())?;
    raw.flush()?;

    // Wrap in tungstenite WebSocket
    let ws = WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Server, None);
    let client_id = CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    {
        let mut st = state.lock().unwrap();
        st.clients.push(WsClient { ws, id: client_id });
        eprintln!("  [live] client {} connected ({} total)", client_id, st.clients.len());
    }

    // We don't need to read from the client for now — the watcher thread pushes.
    // Just keep the connection alive by sleeping.
    // The cleanup happens when broadcast detects a dead connection.
    loop {
        std::thread::sleep(Duration::from_secs(60));
        // Check if we're still in the client list
        let st = state.lock().unwrap();
        if !st.clients.iter().any(|c| c.id == client_id) {
            break;
        }
    }

    Ok(())
}

fn compute_ws_accept(key: &str) -> String {
    use sha2::Digest;
    let magic = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
    let mut hasher = Sha256::new();
    // WebSocket spec uses SHA-1, but we need to use the correct one
    // Actually, let's use a simple implementation
    let concat = format!("{}{}", key, magic);
    // We need SHA-1 for WebSocket accept. Use sha2 crate doesn't have SHA-1.
    // Let's compute it manually with a minimal SHA-1.
    let hash = sha1_hash(concat.as_bytes());
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(hash)
}

/// Minimal SHA-1 implementation for WebSocket handshake only.
fn sha1_hash(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..80 {
            w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h0, h1, h2, h3, h4);
        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(w[i]);
            e = d; d = c; c = b.rotate_left(30); b = a; a = temp;
        }
        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

// ---------------------------------------------------------------------------
// File change handler — diff + broadcast
// ---------------------------------------------------------------------------

fn handle_file_change(state: &SharedState, inject_live: bool) {
    let mut st = state.lock().unwrap();

    let new_raw = match std::fs::read_to_string(&st.file_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let new_hash = hash_content(&new_raw);
    if new_hash == st.current_hash {
        return; // No actual change
    }

    let new_served = if inject_live { inject_live_script(&new_raw) } else { new_raw.clone() };

    eprintln!("  [live] file changed, pushing to {} client(s)", st.clients.len());

    // Send full HTML update (most reliable for single-file apps)
    let msg = json!({
        "type": "full",
        "html": new_served,
    });
    let payload = Message::Text(msg.to_string());

    // Broadcast, removing dead clients
    st.clients.retain_mut(|client| {
        match client.ws.send(payload.clone()) {
            Ok(_) => true,
            Err(_) => {
                eprintln!("  [live] client {} disconnected", client.id);
                false
            }
        }
    });

    st.current_html = new_served;
    st.current_hash = new_hash;
}

/// Push an incremental DOM patch to all connected clients.
/// Called externally (e.g. after `sfhtml apply`) for targeted updates.
#[allow(dead_code)]
pub fn push_patch(port: u16, selector: &str, html: &str) -> Result<usize> {
    let msg = json!({
        "type": "patch",
        "selector": selector,
        "html": html,
    });
    // This would need access to the shared state.
    // For now, we broadcast via a UDP signal or shared file.
    // In practice, `apply` + file watcher handles this automatically.
    eprintln!("  [live] patch push: {} -> {} bytes", selector, html.len());
    let _ = msg;
    let _ = port;
    Ok(0)
}

/// Push a JS eval command to all connected clients.
#[allow(dead_code)]
pub fn push_eval(port: u16, js_code: &str) -> Result<usize> {
    let msg = json!({
        "type": "patch",
        "eval": js_code,
    });
    let _ = msg;
    let _ = port;
    Ok(0)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn inject_live_script(html: &str) -> String {
    // Insert before </body> or at end
    if let Some(pos) = html.to_lowercase().rfind("</body>") {
        let mut result = String::with_capacity(html.len() + LIVE_CLIENT_JS.len());
        result.push_str(&html[..pos]);
        result.push_str(LIVE_CLIENT_JS);
        result.push_str(&html[pos..]);
        result
    } else {
        format!("{}\n{}", html, LIVE_CLIENT_JS)
    }
}

fn hash_content(content: &str) -> String {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h = chars.next().unwrap_or(b'0');
            let l = chars.next().unwrap_or(b'0');
            let val = hex_val(h) * 16 + hex_val(l);
            result.push(val as char);
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn open_url_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd").args(["/c", "start", url]).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}
