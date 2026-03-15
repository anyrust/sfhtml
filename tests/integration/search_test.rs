use std::process::Command;
use std::path::PathBuf;

fn sfhtml_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sfhtml"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures")
}

#[test]
fn test_search_finds_survey() {
    let output = Command::new(sfhtml_bin())
        .args(["search", "survey", "--dir", fixtures_dir().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Invalid JSON");

    assert!(!results.is_empty(), "Expected at least one search result");
    assert!(results[0]["path"].as_str().unwrap().contains("survey_app"));
}

#[test]
fn test_search_title_high_weight() {
    let output = Command::new(sfhtml_bin())
        .args(["search", "Legacy Measurement", "--dir", fixtures_dir().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Invalid JSON");

    assert!(!results.is_empty());
    // The file with the matching title should have high score
    let top_result = &results[0];
    assert!(top_result["score"].as_u64().unwrap() >= 10);
}

#[test]
fn test_search_no_results() {
    let output = Command::new(sfhtml_bin())
        .args(["search", "zzz_nonexistent_query_zzz", "--dir", fixtures_dir().to_str().unwrap()])
        .output()
        .expect("Failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("Invalid JSON");
    assert!(results.is_empty());
}
