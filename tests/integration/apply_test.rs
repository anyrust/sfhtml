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
fn test_apply_simple_diff() {
    let (dir, test_file) = create_test_copy();

    // Read the original to know what's there
    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();

    // Find the line with "return degrees * Math.PI / 180"
    let target_line = lines.iter().enumerate().find(|(_, l)| l.contains("return degrees * Math.PI / 180")).unwrap().0 + 1;

    // Create a simple diff
    let diff = format!(
        r#"--- a/test_app.html
+++ b/test_app.html
@@ -{0},3 +{0},4 @@
     toRadians: function(degrees) {{
-        return degrees * Math.PI / 180;
+        // Convert degrees to radians
+        return (degrees * Math.PI) / 180;
     }},
"#,
        target_line - 1
    );

    let diff_file = dir.path().join("test.diff");
    fs::write(&diff_file, &diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["apply", test_file.to_str().unwrap(), "--diff", diff_file.to_str().unwrap(), "--force"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success(), "apply failed: {}", String::from_utf8_lossy(&output.stderr));

    let new_content = fs::read_to_string(&test_file).unwrap();
    assert!(new_content.contains("// Convert degrees to radians"));
    assert!(new_content.contains("return (degrees * Math.PI) / 180;"));
}

#[test]
fn test_apply_dry_run() {
    let (dir, test_file) = create_test_copy();

    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();
    let target_line = lines.iter().enumerate().find(|(_, l)| l.contains("return degrees * Math.PI / 180")).unwrap().0 + 1;

    let diff = format!(
        r#"--- a/test_app.html
+++ b/test_app.html
@@ -{0},3 +{0},3 @@
     toRadians: function(degrees) {{
-        return degrees * Math.PI / 180;
+        return (degrees * Math.PI) / 180;
     }},
"#,
        target_line - 1
    );

    let diff_file = dir.path().join("test.diff");
    fs::write(&diff_file, &diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["apply", test_file.to_str().unwrap(), "--diff", diff_file.to_str().unwrap(), "--dry-run", "--force"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());

    // File should not have changed
    let after = fs::read_to_string(&test_file).unwrap();
    assert_eq!(original, after);
}

#[test]
fn test_apply_backup() {
    let (dir, test_file) = create_test_copy();

    let original = fs::read_to_string(&test_file).unwrap();
    let lines: Vec<&str> = original.lines().collect();
    let target_line = lines.iter().enumerate().find(|(_, l)| l.contains("return degrees * Math.PI / 180")).unwrap().0 + 1;

    let diff = format!(
        r#"--- a/test_app.html
+++ b/test_app.html
@@ -{0},3 +{0},3 @@
     toRadians: function(degrees) {{
-        return degrees * Math.PI / 180;
+        return (degrees * Math.PI) / 180;
     }},
"#,
        target_line - 1
    );

    let diff_file = dir.path().join("test.diff");
    fs::write(&diff_file, &diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["apply", test_file.to_str().unwrap(), "--diff", diff_file.to_str().unwrap(), "--backup", "--force"])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());

    // Check backup file exists
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    let backup_exists = entries.iter().any(|e| {
        e.as_ref().unwrap().file_name().to_string_lossy().contains(".bak.")
    });
    assert!(backup_exists, "Backup file should exist");
}

#[test]
fn test_apply_context_mismatch() {
    let (dir, test_file) = create_test_copy();

    // Create a diff with wrong context
    let diff = r#"--- a/test_app.html
+++ b/test_app.html
@@ -50,3 +50,3 @@
     wrongContext: function() {
-        this does not exist;
+        this is new;
     }
"#;

    let diff_file = dir.path().join("bad.diff");
    fs::write(&diff_file, diff).unwrap();

    let output = Command::new(sfhtml_bin())
        .args(["apply", test_file.to_str().unwrap(), "--diff", diff_file.to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(!output.status.success(), "Should fail on context mismatch");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("mismatch") || stderr.contains("Error"), "Should report context mismatch");
}
