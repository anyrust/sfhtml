use std::process::Command;
use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

#[test]
fn test_create_html_basic() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("new_app.html");

    let output = Command::new(sfhtml_bin())
        .args(["create", path.to_str().unwrap(), "--title", "My Test App"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("<title>My Test App</title>"));
    assert!(content.contains("<!DOCTYPE html>"));
    assert!(!content.contains("AI-SKILL-HEADER"));
}

#[test]
fn test_create_html_with_header() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("header_app.html");

    let output = Command::new(sfhtml_bin())
        .args(["create", path.to_str().unwrap(), "--title", "Header App", "--with-header"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("<title>Header App</title>"));
    assert!(content.contains("<!-- AI-SKILL-HEADER START"));
    assert!(content.contains("AI-SKILL-HEADER END -->"));
    assert!(content.contains("# Header App"));
}

#[test]
fn test_create_refuses_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.html");
    fs::write(&path, "existing content").unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["create", path.to_str().unwrap(), "--title", "Overwrite"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    // File should be unchanged
    assert_eq!(fs::read_to_string(&path).unwrap(), "existing content");
}

#[test]
fn test_create_force_overwrite() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("existing.html");
    fs::write(&path, "old content").unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["create", path.to_str().unwrap(), "--title", "NewApp", "--force"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("<title>NewApp</title>"));
}

#[test]
fn test_create_nested_directory() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sub").join("deep").join("app.html");

    let output = Command::new(sfhtml_bin())
        .args(["create", path.to_str().unwrap(), "--title", "Deep App"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(path.exists());
}

#[test]
fn test_create_json_output() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("json_test.html");

    let output = Command::new(sfhtml_bin())
        .args(["--json", "create", path.to_str().unwrap(), "--title", "JSON"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(val["with_header"], false);
}
