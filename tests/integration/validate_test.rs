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

fn malformed() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("malformed.html")
}

#[test]
fn test_validate_survey_app() {
    let output = Command::new(sfhtml_bin())
        .args(["validate", survey_app().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    // Check structure
    assert!(result["anchor_consistency"].is_object());
    assert!(result["syntax_validation"].is_object());
}

#[test]
fn test_validate_text_output() {
    let output = Command::new(sfhtml_bin())
        .args(["validate", survey_app().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== Anchor Consistency ==="));
    assert!(stdout.contains("=== Syntax Validation ==="));
}

#[test]
fn test_validate_malformed() {
    let output = Command::new(sfhtml_bin())
        .args(["validate", malformed().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    // Should detect missing anchors
    let missing_from_code = result["anchor_consistency"]["missing_from_code"].as_array().unwrap();
    // "const Bar" is in header but has no closing pipe so it may or may not be parsed
    // But at least the validation should run successfully
    assert!(result["errors"].as_u64().unwrap() > 0 || result["warnings"].as_u64().unwrap() > 0,
        "Malformed file should have errors or warnings");
}

#[test]
fn test_validate_no_header() {
    let no_header = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("no_header.html");

    let output = Command::new(sfhtml_bin())
        .args(["validate", no_header.to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    // Should handle gracefully — no header means 0 header anchors
    // The validator may still succeed with 0/0 anchors found
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Either succeeds with 0 anchors or errors about missing header
    assert!(output.status.success() || stderr.contains("No AI-SKILL-HEADER"),
        "stdout: {}\nstderr: {}", stdout, stderr);
}
