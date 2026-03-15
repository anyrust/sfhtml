use std::process::Command;
use std::path::PathBuf;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

// ---------------------------------------------------------------------------
// debug start — graceful failure without browser
// ---------------------------------------------------------------------------

#[test]
fn test_debug_start_no_browser_graceful() {
    // When no browser is available (or file doesn't exist), should fail gracefully
    let output = Command::new(sfhtml_bin())
        .args(["debug", "start", "/nonexistent/file.html", "--port", "19222"])
        .output()
        .expect("Failed to execute");

    // Should exit with code 1 (graceful failure), not crash
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("debug start failed") || stderr.contains("File not found") || stderr.contains("No Chrome"),
        "Expected graceful error message, got: {}",
        stderr
    );
    // Confirm it mentions other commands still work
    assert!(
        stderr.contains("sfhtml commands remain available"),
        "Should tell user other commands still work, got: {}",
        stderr
    );
}

// ---------------------------------------------------------------------------
// debug list — works even with no sessions
// ---------------------------------------------------------------------------

#[test]
fn test_debug_list_empty() {
    let output = Command::new(sfhtml_bin())
        .args(["debug", "list"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Could be empty or show "No active browser sessions."
    assert!(
        stdout.contains("No active") || stdout.trim().is_empty() || stdout.contains("sessions"),
        "Unexpected output: {}",
        stdout
    );
}

#[test]
fn test_debug_list_json() {
    let output = Command::new(sfhtml_bin())
        .args(["debug", "list", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");
    assert!(v["sessions"].is_array());
}

// ---------------------------------------------------------------------------
// page commands — graceful failure without active session
// ---------------------------------------------------------------------------

#[test]
fn test_page_screenshot_no_session() {
    let output = Command::new(sfhtml_bin())
        .args(["page", "screenshot", "--port", "19999"])
        .output()
        .expect("Failed to execute");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("page command failed"),
        "Expected graceful error, got: {}",
        stderr
    );
}

#[test]
fn test_page_dom_no_session() {
    let output = Command::new(sfhtml_bin())
        .args(["page", "dom", "--port", "19999"])
        .output()
        .expect("Failed to execute");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("page command failed") || stderr.contains("No active session"));
}

#[test]
fn test_page_click_no_session() {
    let output = Command::new(sfhtml_bin())
        .args(["page", "click", "#btn", "--port", "19999"])
        .output()
        .expect("Failed to execute");

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn test_page_eval_no_session() {
    let output = Command::new(sfhtml_bin())
        .args(["page", "eval", "1+1", "--port", "19999"])
        .output()
        .expect("Failed to execute");

    assert_eq!(output.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// debug stop — graceful even if nothing to stop
// ---------------------------------------------------------------------------

#[test]
fn test_debug_stop_no_session() {
    let output = Command::new(sfhtml_bin())
        .args(["debug", "stop", "--port", "19999"])
        .output()
        .expect("Failed to execute");

    // Should either succeed (nothing to stop) or fail gracefully
    let code = output.status.code().unwrap_or(1);
    assert!(code == 0 || code == 1);
}

// ---------------------------------------------------------------------------
// CLI parsing — verify subcommand structure is correct
// ---------------------------------------------------------------------------

#[test]
fn test_debug_help() {
    let output = Command::new(sfhtml_bin())
        .args(["debug", "--help"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("start"));
    assert!(stdout.contains("stop"));
    assert!(stdout.contains("list"));
}

#[test]
fn test_page_help() {
    let output = Command::new(sfhtml_bin())
        .args(["page", "--help"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("screenshot"));
    assert!(stdout.contains("dom"));
    assert!(stdout.contains("console"));
    assert!(stdout.contains("click"));
    assert!(stdout.contains("type"));
    assert!(stdout.contains("scroll"));
    assert!(stdout.contains("touch"));
    assert!(stdout.contains("eval"));
    assert!(stdout.contains("pdf"));
    assert!(stdout.contains("close"));
}
