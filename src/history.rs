use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_CACHE_BYTES: u64 = 10 * 1024 * 1024; // 10MB

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub file_path: String,
    pub timestamp: u64,
    pub timestamp_human: String,
    pub diff_text: String,
    pub reverse_diff: String,
    pub hunks_applied: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct HistorySummary {
    pub id: String,
    pub file_path: String,
    pub timestamp_human: String,
    pub hunks_applied: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub size_bytes: u64,
    pub description: String,
}

/// Get the history cache directory
pub fn cache_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SFHTML_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Cannot determine home directory"))?;
    Ok(PathBuf::from(home).join(".sfhtml").join("history"))
}

/// Ensure cache directory exists
fn ensure_cache_dir() -> Result<PathBuf> {
    let dir = cache_dir()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Save a history entry
pub fn save_entry(entry: &HistoryEntry) -> Result<PathBuf> {
    let dir = ensure_cache_dir()?;
    let path = dir.join(format!("{}.json", entry.id));
    let json = serde_json::to_string_pretty(entry)?;
    fs::write(&path, &json)?;
    // Enforce cache limit
    enforce_cache_limit(&dir)?;
    Ok(path)
}

/// Create a new history entry from an apply operation
pub fn create_entry(
    file_path: &Path,
    original_content: &str,
    new_content: &str,
    diff_text: &str,
    hunks_applied: usize,
    lines_added: usize,
    lines_removed: usize,
) -> HistoryEntry {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    let secs = timestamp.as_secs();
    let sub = timestamp.subsec_nanos();

    let file_stem = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .replace(
            |c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_',
            "_",
        );

    let id = format!("{}_{}_{}", secs, sub, file_stem);
    let timestamp_human = format_timestamp(secs);

    // Generate reverse diff for rollback
    let reverse_diff = crate::differ::generate_diff(
        new_content,
        original_content,
        &format!("b/{}", file_stem),
        &format!("a/{}", file_stem),
        3,
    );

    HistoryEntry {
        id,
        file_path: file_path.to_string_lossy().to_string(),
        timestamp: secs,
        timestamp_human,
        diff_text: diff_text.to_string(),
        reverse_diff,
        hunks_applied,
        lines_added,
        lines_removed,
        description: format!(
            "{}: +{} -{} ({} hunks)",
            file_stem, lines_added, lines_removed, hunks_applied
        ),
    }
}

/// List all history entries
pub fn list_entries(filter_file: Option<&str>) -> Result<Vec<HistorySummary>> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let record: HistoryEntry = match serde_json::from_str(&content) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if let Some(filter) = filter_file {
            if !record.file_path.contains(filter) {
                continue;
            }
        }

        entries.push(HistorySummary {
            id: record.id,
            file_path: record.file_path,
            timestamp_human: record.timestamp_human,
            hunks_applied: record.hunks_applied,
            lines_added: record.lines_added,
            lines_removed: record.lines_removed,
            size_bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            description: record.description,
        });
    }

    entries.sort_by(|a, b| b.id.cmp(&a.id)); // newest first
    Ok(entries)
}

/// Show a specific history entry
pub fn show_entry(id: &str) -> Result<HistoryEntry> {
    let dir = cache_dir()?;
    let path = dir.join(format!("{}.json", id));
    if !path.exists() {
        bail!("History entry '{}' not found", id);
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Delete a specific history entry
pub fn delete_entry(id: &str) -> Result<u64> {
    let dir = cache_dir()?;
    let path = dir.join(format!("{}.json", id));
    if !path.exists() {
        bail!("History entry '{}' not found", id);
    }
    let size = path.metadata()?.len();
    fs::remove_file(&path)?;
    Ok(size)
}

/// Rollback a file using a specific history entry's reverse diff
pub fn rollback(file_path: &Path, id: &str, fuzz: usize, dry_run: bool) -> Result<String> {
    let entry = show_entry(id)?;

    if dry_run {
        return Ok(entry.reverse_diff.clone());
    }

    // Apply the reverse diff (force=true to skip validation-rollback loop, backup=true for safety)
    let result = crate::applier::apply_diff(file_path, &entry.reverse_diff, fuzz, false, true, true)?;

    Ok(format!(
        "Rollback applied: {} hunks, +{} -{} lines. Backup created.",
        result.hunks_applied, result.lines_added, result.lines_removed
    ))
}

/// Get total cache size in bytes
pub fn cache_size() -> Result<u64> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        total += entry.metadata()?.len();
    }
    Ok(total)
}

/// Enforce the 10MB cache limit by removing oldest entries
fn enforce_cache_limit(dir: &Path) -> Result<()> {
    let mut entries: Vec<(PathBuf, u64, u64)> = Vec::new(); // (path, size, timestamp)
    let mut total_size = 0u64;

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let size = entry.metadata()?.len();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("0");
        let ts: u64 = stem
            .split('_')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        entries.push((path, size, ts));
        total_size += size;
    }

    if total_size <= MAX_CACHE_BYTES {
        return Ok(());
    }

    // Sort by timestamp ascending (oldest first)
    entries.sort_by_key(|e| e.2);

    // Remove oldest entries until under limit
    for (path, size, _) in &entries {
        if total_size <= MAX_CACHE_BYTES {
            break;
        }
        fs::remove_file(path)?;
        total_size -= size;
    }

    Ok(())
}

/// Clean all cached history
pub fn clean_cache() -> Result<(usize, u64)> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok((0, 0));
    }
    let mut removed = 0;
    let mut freed = 0u64;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let size = entry.metadata()?.len();
        fs::remove_file(&path)?;
        removed += 1;
        freed += size;
    }
    Ok((removed, freed))
}

fn format_timestamp(secs: u64) -> String {
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let mut y: u64 = 1970;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0u64;
    for (i, &days) in month_days.iter().enumerate() {
        if remaining < days {
            m = (i + 1) as u64;
            break;
        }
        remaining -= days;
    }
    if m == 0 {
        m = 12;
    }
    let d = remaining + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y, m, d, hours, minutes, seconds
    )
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
