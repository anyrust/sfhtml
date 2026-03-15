use std::process::Command;
use std::path::PathBuf;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn module_test_app() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("module_test")
        .join("app.html")
}

#[test]
fn test_module_finds_all_deps() {
    let output = Command::new(sfhtml_bin())
        .args(["--json", "module", module_test_app().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["total"], 9);
    assert_eq!(result["local"], 9);
    assert_eq!(result["remote"], 0);
}

#[test]
fn test_module_detects_missing() {
    let output = Command::new(sfhtml_bin())
        .args(["--json", "module", module_test_app().to_str().unwrap()])
        .output()
        .unwrap();

    // Exit code 1 when there are missing deps
    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(result["missing"], 6);
}

#[test]
fn test_module_header_preview() {
    let output = Command::new(sfhtml_bin())
        .args(["--json", "module", module_test_app().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    let deps = result["deps"].as_array().unwrap();

    // store.js has a header → preview should be the title line
    let store_dep = deps.iter().find(|d| d["source"] == "./modules/store.js").unwrap();
    assert_eq!(store_dep["has_header"], true);
    assert!(store_dep["preview"].as_str().unwrap().contains("DataStore"));

    // renderer.js has no header → preview should be first 100 bytes
    let renderer_dep = deps.iter().find(|d| d["source"] == "./modules/renderer.js").unwrap();
    assert_eq!(renderer_dep["has_header"], false);
    assert!(renderer_dep["preview"].as_str().unwrap().contains("Renderer module"));
}

#[test]
fn test_module_dep_types() {
    let output = Command::new(sfhtml_bin())
        .args(["--json", "module", module_test_app().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let deps = result["deps"].as_array().unwrap();

    let types: Vec<&str> = deps.iter()
        .map(|d| d["dep_type"].as_str().unwrap())
        .collect();

    assert!(types.contains(&"csslink"));
    assert!(types.contains(&"cssimport"));
    assert!(types.contains(&"jsmodule"));
    assert!(types.contains(&"jsscript"));
    assert!(types.contains(&"htmlref"));
}

#[test]
fn test_module_text_output() {
    let output = Command::new(sfhtml_bin())
        .args(["module", module_test_app().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("✓ header"));
    assert!(stdout.contains("✗ MISSING"));
    assert!(stdout.contains("✓ exists"));
    assert!(stdout.contains("# DataStore"));
    assert!(stdout.contains("missing dependency"));
}
