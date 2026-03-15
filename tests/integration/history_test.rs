use std::process::Command;
use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn survey_app() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("survey_app.html")
}

fn create_test_copy() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("test_app.html");
    fs::copy(survey_app(), &dest).unwrap();
    (dir, dest)
}

#[test]
fn test_apply_saves_history() {
    let (dir, test_file) = create_test_copy();

    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();

    // Create a simple diff to add a comment
    let target_line = lines.len().min(5); // pick a safe line
    let diff = format!(
        "--- a/test_app.html\n+++ b/test_app.html\n@@ -{0},1 +{0},2 @@\n {1}\n+<!-- history test comment -->\n",
        target_line,
        lines[target_line - 1]
    );

    let diff_path = dir.path().join("test.diff");
    fs::write(&diff_path, &diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["--json", "apply", test_file.to_str().unwrap(), "--diff", diff_path.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    assert!(output.status.success(), "apply failed: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(result["history_id"].is_string(), "Should have history_id");
    let history_id = result["history_id"].as_str().unwrap();

    // Verify history entry exists
    let list_output = Command::new(sfhtml_bin())
        .args(["history", "list"])
        .output()
        .unwrap();
    assert!(list_output.status.success());
    let list_text = String::from_utf8_lossy(&list_output.stdout);
    assert!(list_text.contains(history_id), "History should contain the new entry");

    // Show the entry
    let show_output = Command::new(sfhtml_bin())
        .args(["history", "show", history_id])
        .output()
        .unwrap();
    assert!(show_output.status.success());
    let show_text = String::from_utf8_lossy(&show_output.stdout);
    assert!(show_text.contains("Forward Diff"));
    assert!(show_text.contains("Reverse Diff"));

    // Clean up history
    let _ = Command::new(sfhtml_bin())
        .args(["history", "delete", history_id])
        .output()
        .unwrap();
}

#[test]
fn test_history_rollback() {
    let (dir, test_file) = create_test_copy();

    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();

    let target_line = lines.len().min(5);
    let diff = format!(
        "--- a/test_app.html\n+++ b/test_app.html\n@@ -{0},1 +{0},2 @@\n {1}\n+<!-- rollback test line -->\n",
        target_line,
        lines[target_line - 1]
    );

    let diff_path = dir.path().join("test.diff");
    fs::write(&diff_path, &diff).unwrap();

    // Apply the diff
    let apply_output = Command::new(sfhtml_bin())
        .args(["--json", "apply", test_file.to_str().unwrap(), "--diff", diff_path.to_str().unwrap(), "--force"])
        .output()
        .unwrap();
    assert!(apply_output.status.success());

    let apply_result: serde_json::Value = serde_json::from_str(
        &String::from_utf8_lossy(&apply_output.stdout)
    ).unwrap();
    let history_id = apply_result["history_id"].as_str().unwrap();

    // Verify the change exists
    let modified = fs::read_to_string(&test_file).unwrap();
    assert!(modified.contains("<!-- rollback test line -->"));

    // Rollback
    let rollback_output = Command::new(sfhtml_bin())
        .args(["history", "rollback", test_file.to_str().unwrap(), history_id])
        .output()
        .unwrap();
    assert!(rollback_output.status.success(), "rollback failed: {}",
        String::from_utf8_lossy(&rollback_output.stderr));

    // Verify rollback removed the added line
    let rolled_back = fs::read_to_string(&test_file).unwrap();
    assert!(!rolled_back.contains("<!-- rollback test line -->"));

    // Clean up
    let _ = Command::new(sfhtml_bin())
        .args(["history", "clean"])
        .output();
}

#[test]
fn test_history_status_and_clean() {
    // Status
    let status_output = Command::new(sfhtml_bin())
        .args(["history", "status"])
        .output()
        .unwrap();
    assert!(status_output.status.success());
    let text = String::from_utf8_lossy(&status_output.stdout);
    assert!(text.contains("Cache dir:"));
    assert!(text.contains("Entries:"));
}

#[test]
fn test_apply_validation_json_output() {
    let (dir, test_file) = create_test_copy();

    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();

    let target_line = lines.len().min(5);
    let diff = format!(
        "--- a/test_app.html\n+++ b/test_app.html\n@@ -{0},1 +{0},2 @@\n {1}\n+<!-- validation test -->\n",
        target_line,
        lines[target_line - 1]
    );

    let diff_path = dir.path().join("test.diff");
    fs::write(&diff_path, &diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["--json", "apply", test_file.to_str().unwrap(), "--diff", diff_path.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    // Should have validation field
    assert!(result["validation"].is_object(), "Should include validation");
    let validation = &result["validation"];
    assert!(validation["status"].is_string());
    assert!(validation["syntax_ok"].is_boolean());
    assert!(validation["anchor_ok"].is_boolean());

    // Clean up
    if let Some(id) = result["history_id"].as_str() {
        let _ = Command::new(sfhtml_bin())
            .args(["history", "delete", id])
            .output();
    }
}
