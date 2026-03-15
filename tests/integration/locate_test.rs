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
fn test_locate_const_anchor() {
    let output = Command::new(sfhtml_bin())
        .args(["locate", survey_app().to_str().unwrap(), "const DataFusion", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    assert_eq!(result["anchor"], "const DataFusion");
    let matches = result["matches"].as_array().unwrap();
    assert!(!matches.is_empty(), "Should find at least one match");
    assert!(matches[0]["line"].as_u64().unwrap() > 0);
}

#[test]
fn test_locate_function_anchor() {
    let output = Command::new(sfhtml_bin())
        .args(["locate", survey_app().to_str().unwrap(), "function adjustTraverseFromData", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    let matches = result["matches"].as_array().unwrap();
    assert!(!matches.is_empty());
    // Should detect scope end
    assert!(matches[0]["end_line"].as_u64().is_some());
}

#[test]
fn test_locate_class_anchor() {
    let output = Command::new(sfhtml_bin())
        .args(["locate", survey_app().to_str().unwrap(), "class TraverseRenderer", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("Invalid JSON");

    let matches = result["matches"].as_array().unwrap();
    assert!(!matches.is_empty());
}

#[test]
fn test_locate_not_found() {
    let output = Command::new(sfhtml_bin())
        .args(["locate", survey_app().to_str().unwrap(), "const NonExistent", "--json"])
        .output()
        .expect("Failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"));
}

#[test]
fn test_anchor_list() {
    let output = Command::new(sfhtml_bin())
        .args(["anchor-list", survey_app().to_str().unwrap(), "--json"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let anchors: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Invalid JSON");

    // Should find SurveyMath, DataFusion, adjustTraverseFromData, parseGsiData, TraverseRenderer
    assert!(anchors.len() >= 4, "Expected at least 4 anchors, got {}", anchors.len());

    // Check types
    let types: Vec<&str> = anchors.iter().map(|a| a["type"].as_str().unwrap()).collect();
    assert!(types.contains(&"js-const"));
    assert!(types.contains(&"js-function"));
    assert!(types.contains(&"js-class"));
}
