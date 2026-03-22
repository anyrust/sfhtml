use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileStats {
    pub calls: usize,
    pub last_call: u64,
}

/// Get the stats file path: ~/.sfhtml/stats.json
fn stats_path() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SFHTML_CACHE_DIR") {
        return Ok(PathBuf::from(dir).join("..").join("stats.json"));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(PathBuf::from(home).join(".sfhtml").join("stats.json"))
}

/// Load all stats from disk
pub fn load() -> HashMap<String, FileStats> {
    let path = match stats_path() {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Increment call count for a file (canonicalizes path)
pub fn increment(file: &Path) {
    let key = match file.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => file.to_string_lossy().to_string(),
    };
    let mut data = load();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let entry = data.entry(key).or_insert(FileStats { calls: 0, last_call: 0 });
    entry.calls += 1;
    entry.last_call = now;
    let _ = save(&data);
}

/// Get stats for a specific file
pub fn get(file: &Path) -> FileStats {
    let key = match file.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => file.to_string_lossy().to_string(),
    };
    load().get(&key).cloned().unwrap_or(FileStats { calls: 0, last_call: 0 })
}

/// Look up stats for a relative path by matching the suffix against canonical keys
pub fn get_by_suffix(rel_path: &str, data: &HashMap<String, FileStats>) -> FileStats {
    for (key, val) in data {
        if key.ends_with(rel_path) || key.ends_with(&rel_path.replace('/', std::path::MAIN_SEPARATOR_STR)) {
            return val.clone();
        }
    }
    FileStats { calls: 0, last_call: 0 }
}

fn save(data: &HashMap<String, FileStats>) -> Result<()> {
    let path = stats_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(data)?;
    fs::write(&path, json)?;
    Ok(())
}

/// Format seconds-ago as human-readable relative time
pub fn format_ago(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if ts == 0 || ts > now {
        return String::from("unknown");
    }
    let diff = now - ts;
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Compute relevance score: calls × recency_decay
/// recency_decay = 1 / (1 + hours_since_modified / 24)
pub fn relevance(calls: usize, modified_ts: u64) -> f64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let hours = if modified_ts > 0 && modified_ts <= now {
        (now - modified_ts) as f64 / 3600.0
    } else {
        9999.0
    };
    let recency = 1.0 / (1.0 + hours / 24.0);
    calls as f64 * recency
}
