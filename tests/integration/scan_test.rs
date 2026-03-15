use std::process::Command;
use std::path::PathBuf;

fn sfhtml_bin() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"));
    path
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

fn parse_scan_json(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).expect("Invalid JSON")
}

#[test]
fn test_scan_finds_header_files() {
    let output = Command::new(sfhtml_bin())
        .args(["scan", fixtures_dir().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success(), "scan failed: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = parse_scan_json(&stdout);
    let html_files = result["html_files"].as_array().expect("html_files should be array");

    // Should find HTML files
    assert!(!html_files.is_empty(), "Expected html_files to be non-empty");

    // At least some should have headers
    assert!(html_files.iter().any(|r| r["has_header"] == true));
}

#[test]
fn test_scan_extracts_app_name() {
    let output = Command::new(sfhtml_bin())
        .args(["scan", fixtures_dir().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = parse_scan_json(&stdout);
    let html_files = result["html_files"].as_array().expect("html_files should be array");

    // Find the survey_app result
    let survey = html_files.iter().find(|r| {
        r["path"].as_str().unwrap_or("").contains("survey_app")
    }).expect("survey_app.html not found in results");

    assert_eq!(survey["app_name"], "SurveyApp");
    assert!(survey["summary"].as_str().unwrap().contains("Total station"));
    assert_eq!(survey["has_header"], true);
}

#[test]
fn test_scan_fallback_for_no_header() {
    let output = Command::new(sfhtml_bin())
        .args(["scan", fixtures_dir().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result = parse_scan_json(&stdout);
    let html_files = result["html_files"].as_array().expect("html_files should be array");

    let no_header = html_files.iter().find(|r| {
        r["path"].as_str().unwrap_or("").contains("no_header")
    }).expect("no_header.html not found in results");

    assert_eq!(no_header["has_header"], false);
    assert_eq!(no_header["title_fallback"], "Legacy Measurement Tool");
}

#[test]
fn test_scan_text_output() {
    let output = Command::new(sfhtml_bin())
        .args(["scan", fixtures_dir().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("SurveyApp"));
    assert!(stdout.contains("[no header]"));
}
