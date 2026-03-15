use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::browser::{self, CdpClient};

// ---------------------------------------------------------------------------
// debug start — launch browser with CDP
// ---------------------------------------------------------------------------

pub fn debug_start(file: &Path, port: u16, headless: bool) -> Result<Value> {
    // Resolve file to absolute path, then to file:// URL
    let abs = std::fs::canonicalize(file)
        .with_context(|| format!("File not found: {}", file.display()))?;
    let file_url = format!("file://{}", abs.display());

    let proc = browser::launch_browser(&file_url, port, headless)?;
    let pid = proc.child.id();

    browser::save_session(port, pid, &proc.ws_url)?;

    // Detach — we intentionally leak the child handle so the browser stays alive
    std::mem::forget(proc.child);

    Ok(json!({
        "status": "started",
        "port": port,
        "pid": pid,
        "ws_url": proc.ws_url,
        "file": abs.display().to_string(),
        "headless": headless,
    }))
}

// ---------------------------------------------------------------------------
// debug stop — kill browser and clean up
// ---------------------------------------------------------------------------

pub fn debug_stop(port: u16) -> Result<Value> {
    let session = browser::load_session(port)?;
    let pid = session["pid"].as_u64().unwrap_or(0) as u32;

    if pid > 0 {
        // Send SIGTERM on Unix via kill command
        let _ = std::process::Command::new("kill")
            .arg(pid.to_string())
            .output();
    }

    browser::remove_session(port);

    // Clean up temp profile
    let profile = std::env::temp_dir().join(format!("sfhtml-cdp-{}", port));
    let _ = std::fs::remove_dir_all(profile);

    Ok(json!({
        "status": "stopped",
        "port": port,
        "pid": pid,
    }))
}

// ---------------------------------------------------------------------------
// debug list — show all active sessions
// ---------------------------------------------------------------------------

pub fn debug_list() -> Result<Value> {
    let sessions = browser::list_sessions()?;
    Ok(json!({ "sessions": sessions }))
}

// ---------------------------------------------------------------------------
// page open — connect to an existing CDP port
// ---------------------------------------------------------------------------

pub fn page_open(port: u16) -> Result<Value> {
    let ws_url = browser::connect_to_port(port)?;
    Ok(json!({
        "status": "connected",
        "port": port,
        "ws_url": ws_url,
    }))
}

// ---------------------------------------------------------------------------
// Connect helper — get a CdpClient from port
// ---------------------------------------------------------------------------

fn get_client(port: u16) -> Result<CdpClient> {
    let session = browser::load_session(port)?;
    let ws_url = session["ws_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid session data for port {}", port))?;

    // The stored ws_url might be stale; try to get a fresh one
    let fresh_url = browser::connect_to_port(port).unwrap_or_else(|_| ws_url.to_string());

    CdpClient::new(&fresh_url)
}

// ---------------------------------------------------------------------------
// page screenshot
// ---------------------------------------------------------------------------

pub fn page_screenshot(port: u16, selector: Option<&str>, output: Option<&Path>) -> Result<Value> {
    let mut client = get_client(port)?;
    let b64 = client.screenshot(selector)?;

    if let Some(path) = output {
        let bytes = base64_decode(&b64)?;
        std::fs::write(path, &bytes)?;
        Ok(json!({
            "saved": path.display().to_string(),
            "size_bytes": bytes.len(),
        }))
    } else {
        // Return base64 (AI can interpret or save)
        Ok(json!({
            "format": "png",
            "encoding": "base64",
            "length": b64.len(),
            "data": b64,
        }))
    }
}

// ---------------------------------------------------------------------------
// page dom
// ---------------------------------------------------------------------------

pub fn page_dom(port: u16, selector: Option<&str>) -> Result<Value> {
    let mut client = get_client(port)?;
    let html = client.get_dom(selector)?;

    let lines = html.lines().count();
    Ok(json!({
        "html": html,
        "lines": lines,
        "selector": selector.unwrap_or("document"),
    }))
}

// ---------------------------------------------------------------------------
// page console
// ---------------------------------------------------------------------------

pub fn page_console(port: u16) -> Result<Value> {
    let mut client = get_client(port)?;
    let logs = client.get_console_logs()?;
    Ok(json!({
        "logs": logs,
        "count": logs.len(),
    }))
}

// ---------------------------------------------------------------------------
// page network
// ---------------------------------------------------------------------------

pub fn page_network(port: u16, wait_ms: u64) -> Result<Value> {
    let mut client = get_client(port)?;
    let events = client.get_network_logs(wait_ms)?;
    Ok(json!({
        "events": events,
        "count": events.len(),
        "wait_ms": wait_ms,
    }))
}

// ---------------------------------------------------------------------------
// page click
// ---------------------------------------------------------------------------

pub fn page_click(port: u16, selector: &str) -> Result<Value> {
    let mut client = get_client(port)?;
    client.click(selector)
}

// ---------------------------------------------------------------------------
// page type
// ---------------------------------------------------------------------------

pub fn page_type(port: u16, selector: &str, text: &str) -> Result<Value> {
    let mut client = get_client(port)?;
    client.type_text(selector, text)
}

// ---------------------------------------------------------------------------
// page scroll
// ---------------------------------------------------------------------------

pub fn page_scroll(port: u16, x: f64, y: f64) -> Result<Value> {
    let mut client = get_client(port)?;
    client.scroll(x, y)
}

// ---------------------------------------------------------------------------
// page touch
// ---------------------------------------------------------------------------

pub fn page_touch(port: u16, x: f64, y: f64) -> Result<Value> {
    let mut client = get_client(port)?;
    client.touch(x, y)
}

// ---------------------------------------------------------------------------
// page eval
// ---------------------------------------------------------------------------

pub fn page_eval(port: u16, expression: &str) -> Result<Value> {
    let mut client = get_client(port)?;
    client.eval(expression)
}

// ---------------------------------------------------------------------------
// page pdf
// ---------------------------------------------------------------------------

pub fn page_pdf(port: u16, output: Option<&Path>) -> Result<Value> {
    let mut client = get_client(port)?;
    let b64 = client.print_pdf()?;

    if let Some(path) = output {
        let bytes = base64_decode(&b64)?;
        std::fs::write(path, &bytes)?;
        Ok(json!({
            "saved": path.display().to_string(),
            "size_bytes": bytes.len(),
        }))
    } else {
        Ok(json!({
            "format": "pdf",
            "encoding": "base64",
            "length": b64.len(),
            "data": b64,
        }))
    }
}

// ---------------------------------------------------------------------------
// page close — just disconnect (doesn't stop the browser)
// ---------------------------------------------------------------------------

pub fn page_close(port: u16) -> Result<Value> {
    let client = get_client(port)?;
    client.close()?;
    Ok(json!({ "status": "disconnected", "port": port }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base64_decode(b64: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .with_context(|| "Failed to decode base64 data")
}
