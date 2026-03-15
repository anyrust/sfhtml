use std::process::Command;
use std::path::PathBuf;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn survey_app() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("survey_app.html")
}

#[test]
fn test_header_full_extract() {
    let output = Command::new(sfhtml_bin())
        .args(["header", survey_app().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# SurveyApp"));
    assert!(stdout.contains("## 1. Overview"));
    assert!(stdout.contains("## 5. Key Internal Modules"));
}

#[test]
fn test_header_json_output() {
    let output = Command::new(sfhtml_bin())
        .args(["header", survey_app().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let header: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    assert_eq!(header["app_name"], "SurveyApp");
    assert!(header["sections"].as_array().unwrap().len() >= 5);
}

#[test]
fn test_header_section_extract() {
    let output = Command::new(sfhtml_bin())
        .args(["header", survey_app().to_str().unwrap(), "--section", "5", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let section: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    assert_eq!(section["number"], 5);
    assert_eq!(section["title"], "Key Internal Modules");
}

#[test]
fn test_header_no_header_file() {
    let no_header = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("no_header.html");

    let output = Command::new(sfhtml_bin())
        .args(["header", no_header.to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No AI-SKILL-HEADER found"));
}
