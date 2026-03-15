use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};

static MSG_ID: AtomicU64 = AtomicU64::new(1);

/// A CDP (Chrome DevTools Protocol) connection to a browser tab.
pub struct CdpClient {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
}

/// Result of launching a browser process.
pub struct BrowserProcess {
    pub child: Child,
    pub ws_url: String,
    #[allow(dead_code)]
    pub port: u16,
}

// ---------------------------------------------------------------------------
// Browser detection
// ---------------------------------------------------------------------------

/// Find a Chrome/Chromium/Edge binary on the system.
pub fn find_browser() -> Option<PathBuf> {
    let candidates = [
        // Linux
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
        "microsoft-edge-stable",
        // macOS
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];

    for c in &candidates {
        let path = Path::new(c);
        if path.is_absolute() && path.exists() {
            return Some(path.to_path_buf());
        }
        // Search via `which`
        if let Ok(output) = Command::new("which").arg(c).output() {
            if output.status.success() {
                let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(PathBuf::from(p));
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Browser launch
// ---------------------------------------------------------------------------

/// Launch browser with CDP debugging on the given port.
/// If `headless` is true, runs in headless mode (no visible window).
pub fn launch_browser(
    file_url: &str,
    port: u16,
    headless: bool,
) -> Result<BrowserProcess> {
    let browser = find_browser().ok_or_else(|| {
        anyhow!(
            "No Chrome/Chromium/Edge found. Install one or use --port to connect to an existing browser.\n\
             Candidates: google-chrome, chromium, microsoft-edge"
        )
    })?;

    let mut args = vec![
        format!("--remote-debugging-port={}", port),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        format!("--user-data-dir={}", temp_profile_dir(port).display()),
    ];

    if headless {
        args.push("--headless=new".to_string());
    }

    args.push(file_url.to_string());

    let child = Command::new(&browser)
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to launch browser: {}", browser.display()))?;

    // Wait for CDP to become available (up to 10 seconds)
    let ws_url = wait_for_cdp(port, Duration::from_secs(10))?;

    Ok(BrowserProcess {
        child,
        ws_url,
        port,
    })
}

fn temp_profile_dir(port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("sfhtml-cdp-{}", port))
}

/// Poll the CDP HTTP endpoint until a WebSocket debugger URL is available.
fn wait_for_cdp(port: u16, timeout: Duration) -> Result<String> {
    let start = std::time::Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    loop {
        if start.elapsed() > timeout {
            bail!(
                "Timed out waiting for CDP on port {}. Is the browser running with --remote-debugging-port={}?",
                port, port
            );
        }

        if let Ok(mut stream) = TcpStream::connect_timeout(
            &addr.parse().unwrap(),
            Duration::from_millis(500),
        ) {
            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

            let req = format!(
                "GET /json HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                port
            );
            if stream.write_all(req.as_bytes()).is_ok() {
                let mut buf = Vec::new();
                let _ = stream.read_to_end(&mut buf);
                let body = String::from_utf8_lossy(&buf);

                // Find JSON body after \r\n\r\n
                if let Some(idx) = body.find("\r\n\r\n") {
                    let json_str = &body[idx + 4..];
                    if let Ok(pages) = serde_json::from_str::<Vec<Value>>(json_str) {
                        for page in &pages {
                            if page.get("type").and_then(|t| t.as_str()) == Some("page") {
                                if let Some(url) = page.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                                    return Ok(url.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(300));
    }
}

// ---------------------------------------------------------------------------
// Connect to existing CDP
// ---------------------------------------------------------------------------

/// Connect to an already-running browser's CDP port.
pub fn connect_to_port(port: u16) -> Result<String> {
    wait_for_cdp(port, Duration::from_secs(5))
}

/// Get a list of all open pages/tabs on the given CDP port.
#[allow(dead_code)]
pub fn list_targets(port: u16) -> Result<Vec<Value>> {
    let addr = format!("127.0.0.1:{}", port);
    let mut stream = TcpStream::connect_timeout(
        &addr.parse().unwrap(),
        Duration::from_secs(2),
    ).with_context(|| format!("Cannot connect to CDP on port {}", port))?;

    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

    let req = format!(
        "GET /json HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        port
    );
    stream.write_all(req.as_bytes())?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;
    let body = String::from_utf8_lossy(&buf);

    let idx = body.find("\r\n\r\n").ok_or_else(|| anyhow!("Invalid HTTP response"))?;
    let json_str = &body[idx + 4..];
    let pages: Vec<Value> = serde_json::from_str(json_str)
        .with_context(|| "Failed to parse CDP target list")?;
    Ok(pages)
}

// ---------------------------------------------------------------------------
// CDP Client
// ---------------------------------------------------------------------------

impl CdpClient {
    /// Create a new CDP client by connecting to a WebSocket debugger URL.
    pub fn new(ws_url: &str) -> Result<Self> {
        let (ws, _response) = connect(ws_url)
            .with_context(|| format!("Failed to connect to CDP WebSocket: {}", ws_url))?;
        Ok(CdpClient { ws })
    }

    /// Send a CDP command and wait for the matching response.
    pub fn send(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = MSG_ID.fetch_add(1, Ordering::SeqCst);
        let msg = json!({
            "id": id,
            "method": method,
            "params": params,
        });

        self.ws.send(Message::Text(msg.to_string())).with_context(|| format!("Failed to send CDP command: {}", method))?;

        // Read messages until we get our response
        loop {
            let raw = self.ws
                .read()
                .with_context(|| "Lost connection to browser")?;
            match raw {
                Message::Text(txt) => {
                    let v: Value = serde_json::from_str(&txt)?;
                    if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                        if let Some(err) = v.get("error") {
                            bail!("CDP error: {}", err);
                        }
                        return Ok(v.get("result").cloned().unwrap_or(json!({})));
                    }
                    // Otherwise it's an event — skip it
                }
                Message::Close(_) => bail!("Browser closed the connection"),
                _ => {}
            }
        }
    }

    /// Enable a CDP domain (e.g. "Runtime", "Page", "Network", "Console").
    pub fn enable_domain(&mut self, domain: &str) -> Result<()> {
        self.send(&format!("{}.enable", domain), json!({}))?;
        Ok(())
    }

    /// Collect events for a specified duration.
    pub fn collect_events(&mut self, duration: Duration) -> Result<Vec<Value>> {
        let start = std::time::Instant::now();
        let mut events = Vec::new();

        // Set a short read timeout so we don't block forever
        if let MaybeTlsStream::Plain(ref stream) = self.ws.get_ref() {
            stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
        }

        while start.elapsed() < duration {
            match self.ws.read() {
                Ok(Message::Text(txt)) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                        if v.get("method").is_some() {
                            events.push(v);
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    continue;
                }
                Err(_) => break,
                _ => {}
            }
        }

        Ok(events)
    }

    // -----------------------------------------------------------------------
    // High-level page commands
    // -----------------------------------------------------------------------

    /// Capture a full-page screenshot, returns base64 PNG.
    pub fn screenshot(&mut self, selector: Option<&str>) -> Result<String> {
        if let Some(sel) = selector {
            // Get element bounding box
            let node = self.send("Runtime.evaluate", json!({
                "expression": format!(
                    "JSON.stringify(document.querySelector({}).getBoundingClientRect())",
                    serde_json::to_string(sel)?
                ),
                "returnByValue": true,
            }))?;
            let rect_str = node
                .pointer("/result/value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Element not found: {}", sel))?;
            let rect: Value = serde_json::from_str(rect_str)?;

            let clip = json!({
                "x": rect["x"],
                "y": rect["y"],
                "width": rect["width"],
                "height": rect["height"],
                "scale": 1,
            });

            let result = self.send("Page.captureScreenshot", json!({
                "format": "png",
                "clip": clip,
            }))?;
            Ok(result["data"].as_str().unwrap_or("").to_string())
        } else {
            let result = self.send("Page.captureScreenshot", json!({
                "format": "png",
                "captureBeyondViewport": true,
            }))?;
            Ok(result["data"].as_str().unwrap_or("").to_string())
        }
    }

    /// Get the outer HTML of the document or a specific selector.
    pub fn get_dom(&mut self, selector: Option<&str>) -> Result<String> {
        let expr = if let Some(sel) = selector {
            format!(
                "(document.querySelector({}) || {{}}).outerHTML || 'Element not found'",
                serde_json::to_string(sel)?
            )
        } else {
            "document.documentElement.outerHTML".to_string()
        };

        let result = self.send("Runtime.evaluate", json!({
            "expression": expr,
            "returnByValue": true,
        }))?;

        Ok(result
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string())
    }

    /// Get console log messages (enables Console domain, collects for 1s).
    pub fn get_console_logs(&mut self) -> Result<Vec<Value>> {
        self.enable_domain("Console")?;

        // Also pull existing logs via Runtime
        let result = self.send("Runtime.evaluate", json!({
            "expression": r#"
                (function() {
                    if (!window.__sfhtml_logs) return '[]';
                    return JSON.stringify(window.__sfhtml_logs);
                })()
            "#,
            "returnByValue": true,
        }))?;

        let mut logs = Vec::new();

        // Check if we injected our logger before
        let existing = result
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("[]");
        if let Ok(arr) = serde_json::from_str::<Vec<Value>>(existing) {
            logs.extend(arr);
        }

        // Inject console interceptor for future calls
        self.send("Runtime.evaluate", json!({
            "expression": r#"
                if (!window.__sfhtml_logs) {
                    window.__sfhtml_logs = [];
                    const orig = {};
                    ['log','warn','error','info','debug'].forEach(m => {
                        orig[m] = console[m];
                        console[m] = function() {
                            window.__sfhtml_logs.push({
                                level: m,
                                text: Array.from(arguments).map(a =>
                                    typeof a === 'object' ? JSON.stringify(a) : String(a)
                                ).join(' '),
                                ts: Date.now()
                            });
                            orig[m].apply(console, arguments);
                        };
                    });
                }
                'ok'
            "#,
            "returnByValue": true,
        }))?;

        Ok(logs)
    }

    /// Get network request log (enables Network domain, collects for specified duration).
    pub fn get_network_logs(&mut self, wait_ms: u64) -> Result<Vec<Value>> {
        self.enable_domain("Network")?;
        let events = self.collect_events(Duration::from_millis(wait_ms))?;

        let mut requests: Vec<Value> = Vec::new();
        for ev in &events {
            if let Some(method) = ev.get("method").and_then(|m| m.as_str()) {
                if method == "Network.requestWillBeSent" || method == "Network.responseReceived" {
                    requests.push(ev.clone());
                }
            }
        }
        Ok(requests)
    }

    /// Click an element by CSS selector.
    pub fn click(&mut self, selector: &str) -> Result<Value> {
        let result = self.send("Runtime.evaluate", json!({
            "expression": format!(
                r#"(function() {{
                    var el = document.querySelector({sel});
                    if (!el) return 'not_found';
                    el.click();
                    return 'clicked';
                }})()"#,
                sel = serde_json::to_string(selector)?
            ),
            "returnByValue": true,
        }))?;

        let status = result
            .pointer("/result/value")
            .and_then(|v| v.as_str())
            .unwrap_or("error");

        if status == "not_found" {
            bail!("Element not found: {}", selector);
        }

        Ok(json!({ "status": status, "selector": selector }))
    }

    /// Type text into a focused element or a specified selector.
    pub fn type_text(&mut self, selector: &str, text: &str) -> Result<Value> {
        // Focus the element first
        self.send("Runtime.evaluate", json!({
            "expression": format!(
                "document.querySelector({}).focus()",
                serde_json::to_string(selector)?
            ),
            "returnByValue": true,
        }))?;

        // Type each character via Input.dispatchKeyEvent
        for ch in text.chars() {
            self.send("Input.dispatchKeyEvent", json!({
                "type": "keyDown",
                "text": ch.to_string(),
            }))?;
            self.send("Input.dispatchKeyEvent", json!({
                "type": "keyUp",
                "text": ch.to_string(),
            }))?;
        }

        Ok(json!({ "typed": text, "selector": selector }))
    }

    /// Scroll the page.
    pub fn scroll(&mut self, x: f64, y: f64) -> Result<Value> {
        self.send("Runtime.evaluate", json!({
            "expression": format!("window.scrollBy({}, {})", x, y),
            "returnByValue": true,
        }))?;
        Ok(json!({ "scrolled": { "x": x, "y": y } }))
    }

    /// Simulate a touch event at coordinates.
    pub fn touch(&mut self, x: f64, y: f64) -> Result<Value> {
        self.send("Input.dispatchTouchEvent", json!({
            "type": "touchStart",
            "touchPoints": [{ "x": x, "y": y }],
        }))?;
        self.send("Input.dispatchTouchEvent", json!({
            "type": "touchEnd",
            "touchPoints": [],
        }))?;
        Ok(json!({ "touched": { "x": x, "y": y } }))
    }

    /// Evaluate arbitrary JavaScript expression.
    pub fn eval(&mut self, expression: &str) -> Result<Value> {
        let result = self.send("Runtime.evaluate", json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        }))?;

        if let Some(exception) = result.get("exceptionDetails") {
            bail!("JS exception: {}", exception);
        }

        Ok(result.get("result").cloned().unwrap_or(json!(null)))
    }

    /// Export page as PDF (headless only). Returns base64 data.
    pub fn print_pdf(&mut self) -> Result<String> {
        let result = self.send("Page.printToPDF", json!({
            "printBackground": true,
        }))?;
        Ok(result["data"].as_str().unwrap_or("").to_string())
    }

    /// Navigate to a URL.
    #[allow(dead_code)]
    pub fn navigate(&mut self, url: &str) -> Result<Value> {
        self.send("Page.navigate", json!({ "url": url }))
    }

    /// Close the connection.
    pub fn close(mut self) -> Result<()> {
        let _ = self.ws.close(None);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Port file management — stores {port, pid, ws_url} on disk
// ---------------------------------------------------------------------------

fn state_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("sfhtml-pages");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Save a running session's info to disk.
pub fn save_session(port: u16, pid: u32, ws_url: &str) -> Result<()> {
    let path = state_dir().join(format!("{}.json", port));
    let data = json!({
        "port": port,
        "pid": pid,
        "ws_url": ws_url,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&data)?)?;
    Ok(())
}

/// Load a session by port.
pub fn load_session(port: u16) -> Result<Value> {
    let path = state_dir().join(format!("{}.json", port));
    let data = std::fs::read_to_string(&path)
        .with_context(|| format!("No active session on port {}. Use `sfhtml debug start <file> --port {}` first.", port, port))?;
    Ok(serde_json::from_str(&data)?)
}

/// Remove a session file.
pub fn remove_session(port: u16) {
    let path = state_dir().join(format!("{}.json", port));
    let _ = std::fs::remove_file(path);
}

/// List all active sessions.
pub fn list_sessions() -> Result<Vec<Value>> {
    let dir = state_dir();
    let mut sessions = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(data) = std::fs::read_to_string(entry.path()) {
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        sessions.push(v);
                    }
                }
            }
        }
    }
    Ok(sessions)
}
