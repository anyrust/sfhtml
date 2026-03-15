use std::process::Command;
use std::path::PathBuf;
use std::fs;
use tempfile::TempDir;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

#[test]
fn test_diff_two_files() {
    let dir = TempDir::new().unwrap();

    let old_content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
    let new_content = "line 1\nline 2 modified\nline 3\nnew line\nline 4\nline 5\n";

    let old_file = dir.path().join("old.html");
    let new_file = dir.path().join("new.html");
    fs::write(&old_file, old_content).unwrap();
    fs::write(&new_file, new_content).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["diff", new_file.to_str().unwrap(), old_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("---"));
    assert!(stdout.contains("+++"));
    assert!(stdout.contains("@@"));
}

#[test]
fn test_diff_identical_files() {
    let dir = TempDir::new().unwrap();

    let content = "line 1\nline 2\nline 3\n";
    let file1 = dir.path().join("a.html");
    let file2 = dir.path().join("b.html");
    fs::write(&file1, content).unwrap();
    fs::write(&file2, content).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["diff", file1.to_str().unwrap(), file2.to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Identical files should produce empty diff
    assert!(stdout.trim().is_empty() || !stdout.contains("@@"),
        "Identical files should have no hunks");
}
