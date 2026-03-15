use std::process::Command;
use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn no_header_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("no_header.html")
}

fn survey_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("survey_app.html")
}

#[test]
fn test_save_as_basic_copy() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("copy.html");

    let output = Command::new(sfhtml_bin())
        .args(["save-as", survey_fixture().to_str().unwrap(), dest.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(output.status.success());
    let original = fs::read_to_string(survey_fixture()).unwrap();
    let copied = fs::read_to_string(&dest).unwrap();
    assert_eq!(original, copied);
}

#[test]
fn test_save_as_inject_header() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("with_header.html");

    let output = Command::new(sfhtml_bin())
        .args(["save-as", no_header_fixture().to_str().unwrap(), dest.to_str().unwrap(), "--inject-header"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(&dest).unwrap();
    assert!(content.contains("<!-- AI-SKILL-HEADER START"));
    assert!(content.contains("AI-SKILL-HEADER END -->"));
    // Original should still not have the header marker
    let original = fs::read_to_string(no_header_fixture()).unwrap();
    assert!(!original.contains("<!-- AI-SKILL-HEADER START"));
}

#[test]
fn test_save_as_refuses_overwrite() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("existing.html");
    fs::write(&dest, "existing").unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["save-as", survey_fixture().to_str().unwrap(), dest.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "existing");
}

#[test]
fn test_save_as_force_overwrite() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("overwrite.html");
    fs::write(&dest, "old").unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["save-as", survey_fixture().to_str().unwrap(), dest.to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let content = fs::read_to_string(&dest).unwrap();
    assert!(content.contains("AI-SKILL-HEADER"));
}
