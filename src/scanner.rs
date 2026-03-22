use anyhow::Result;
use memchr::memmem;
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const HEADER_MARKER: &[u8] = b"<!-- AI-SKILL-HEADER START";
const READ_LIMIT_WITH_HEADER: usize = 8 * 1024; // 8KB
const READ_LIMIT_FALLBACK: usize = 1024; // 1KB

// --- Sort ---

#[derive(Debug, Clone, Copy)]
pub enum SortKey {
    Modified,
    Created,
    Name,
    Size,
    Relevance,
}

impl SortKey {
    pub fn from_str(s: &str) -> Self {
        match s {
            "created" => SortKey::Created,
            "name" => SortKey::Name,
            "size" => SortKey::Size,
            "relevance" => SortKey::Relevance,
            _ => SortKey::Modified,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn from_str(s: &str) -> Self {
        match s {
            "asc" => SortOrder::Asc,
            _ => SortOrder::Desc,
        }
    }
}

// --- Result types ---

#[derive(Debug, Serialize, Clone)]
pub struct ScanResult {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub has_header: bool,
    pub file_lines: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_fallback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// modification time as unix timestamp (for sorting)
    pub modified_ts: u64,
    /// creation time as unix timestamp
    #[serde(skip)]
    pub created_ts: u64,
    /// file size in bytes (for sorting)
    #[serde(skip)]
    pub size_bytes: u64,
    /// sfhtml call count from usage stats
    #[serde(skip_serializing_if = "is_zero")]
    pub calls: usize,
    /// human-readable relative modification time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_ago: Option<String>,
}

fn is_zero(v: &usize) -> bool { *v == 0 }

#[derive(Debug, Serialize, Clone)]
pub struct DirEntry {
    pub path: String,
    pub children: usize,
}

#[derive(Debug, Serialize, Clone)]
pub struct OtherFileEntry {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct FullScanResult {
    pub html_files: Vec<ScanResult>,
    pub dirs: Vec<DirEntry>,
    pub other_files: Vec<OtherFileEntry>,
    pub html_total: usize,
    /// True if scan was terminated by timeout
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub timed_out: bool,
}

/// Full workspace scan: HTML files, folders, and other files.
/// When `deadline` is set, the scan will stop collecting HTML paths
/// once the deadline is reached and return partial results with `timed_out = true`.
pub fn scan_directory(
    dir: &Path,
    recursive: bool,
    sort_key: SortKey,
    sort_order: SortOrder,
    match_keywords: &[String],
    deadline: Option<std::time::Instant>,
) -> Result<FullScanResult> {
    let mut html_paths: Vec<PathBuf> = Vec::new();
    let mut dir_entries: Vec<DirEntry> = Vec::new();
    let mut other_files: Vec<OtherFileEntry> = Vec::new();
    let mut timed_out = false;

    let is_expired = |dl: Option<std::time::Instant>| -> bool {
        dl.map_or(false, |d| std::time::Instant::now() >= d)
    };

    if recursive {
        let walker = WalkDir::new(dir).follow_links(true);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if is_expired(deadline) { timed_out = true; break; }
            let path = entry.path();
            if path == dir { continue; }
            let rel = path.strip_prefix(dir).unwrap_or(path).to_string_lossy().to_string();

            if !match_keywords.is_empty() {
                let lower = rel.to_lowercase();
                if !match_keywords.iter().all(|k| lower.contains(&k.to_lowercase())) {
                    continue;
                }
            }

            if path.is_dir() {
                let children = std::fs::read_dir(path).map(|rd| rd.count()).unwrap_or(0);
                dir_entries.push(DirEntry { path: rel, children });
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
                    html_paths.push(path.to_path_buf());
                } else {
                    other_files.push(OtherFileEntry { path: rel });
                }
            }
        }
    } else {
        // Non-recursive with auto-deepen BFS
        let mut queue: Vec<PathBuf> = vec![dir.to_path_buf()];

        while !queue.is_empty() {
            if is_expired(deadline) { timed_out = true; break; }
            let mut next_queue: Vec<PathBuf> = Vec::new();

            for current_dir in &queue {
                if is_expired(deadline) { timed_out = true; break; }
                let entries = match std::fs::read_dir(current_dir) {
                    Ok(rd) => rd,
                    Err(_) => continue,
                };
                for entry in entries.filter_map(|e| e.ok()) {
                    if is_expired(deadline) { timed_out = true; break; }
                    let path = entry.path();
                    let rel = path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().to_string();

                    if !match_keywords.is_empty() {
                        let lower = rel.to_lowercase();
                        if !match_keywords.iter().all(|k| lower.contains(&k.to_lowercase())) {
                            continue;
                        }
                    }

                    if path.is_dir() {
                        next_queue.push(path.clone());
                        let children = std::fs::read_dir(&path).map(|rd| rd.count()).unwrap_or(0);
                        dir_entries.push(DirEntry { path: rel, children });
                    } else if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
                            html_paths.push(path);
                        } else {
                            other_files.push(OtherFileEntry { path: rel });
                        }
                    }
                }
            }
            if timed_out { break; }
            queue = next_queue;
        }
    }

    let html_total = html_paths.len();

    // Sort HTML paths before scanning
    sort_paths_by_key(&mut html_paths, sort_key, sort_order);

    // Full scan all HTML files (multi-threaded)
    let mut html_files = scan_batch(&html_paths, dir, deadline)?;
    sort_scan_results(&mut html_files, sort_key, sort_order);

    // If scan_batch timed out, some files may not have been scanned
    if html_files.len() < html_total {
        timed_out = true;
    }

    Ok(FullScanResult {
        html_files,
        dirs: dir_entries,
        other_files,
        html_total,
        timed_out,
    })
}

fn sort_paths_by_key(paths: &mut Vec<PathBuf>, key: SortKey, order: SortOrder) {
    paths.sort_by(|a, b| {
        let cmp = match key {
            SortKey::Name => a.file_name().cmp(&b.file_name()),
            SortKey::Size => {
                let sa = std::fs::metadata(a).map(|m| m.len()).unwrap_or(0);
                let sb = std::fs::metadata(b).map(|m| m.len()).unwrap_or(0);
                sa.cmp(&sb)
            }
            SortKey::Created => {
                let ca = std::fs::metadata(a).and_then(|m| m.created()).map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
                let cb = std::fs::metadata(b).and_then(|m| m.created()).map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
                ca.cmp(&cb)
            }
            SortKey::Modified | SortKey::Relevance => {
                let ma = std::fs::metadata(a).and_then(|m| m.modified()).map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
                let mb = std::fs::metadata(b).and_then(|m| m.modified()).map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
                ma.cmp(&mb)
            }
        };
        if order == SortOrder::Desc { cmp.reverse() } else { cmp }
    });
}

fn sort_scan_results(results: &mut Vec<ScanResult>, key: SortKey, order: SortOrder) {
    results.sort_by(|a, b| {
        let cmp = match key {
            SortKey::Name => a.path.cmp(&b.path),
            SortKey::Size => a.size_bytes.cmp(&b.size_bytes),
            SortKey::Created => a.created_ts.cmp(&b.created_ts),
            SortKey::Modified => a.modified_ts.cmp(&b.modified_ts),
            SortKey::Relevance => {
                let ra = crate::stats::relevance(a.calls, a.modified_ts);
                let rb = crate::stats::relevance(b.calls, b.modified_ts);
                ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
            }
        };
        if order == SortOrder::Desc { cmp.reverse() } else { cmp }
    });
}

fn scan_batch(paths: &[PathBuf], dir: &Path, deadline: Option<std::time::Instant>) -> Result<Vec<ScanResult>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let num_threads = std::cmp::min(num_cpus::get(), 8);

    if num_threads <= 1 || paths.len() <= 1 {
        let mut results = Vec::new();
        for p in paths {
            if deadline.map_or(false, |d| std::time::Instant::now() >= d) { break; }
            if let Ok(r) = scan_single_file(p, dir) {
                results.push(r);
            }
        }
        return Ok(results);
    }

    let (sender, receiver) = crossbeam_channel::bounded::<PathBuf>(num_threads * 4);
    let (result_sender, result_receiver) = crossbeam_channel::unbounded::<ScanResult>();
    let dir_owned = dir.to_path_buf();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let rx = receiver.clone();
            let tx = result_sender.clone();
            let d = dir_owned.clone();
            let cancel = cancelled.clone();
            std::thread::spawn(move || {
                while let Ok(path) = rx.recv() {
                    if cancel.load(std::sync::atomic::Ordering::Relaxed) { break; }
                    if let Ok(result) = scan_single_file(&path, &d) {
                        let _ = tx.send(result);
                    }
                }
            })
        })
        .collect();

    drop(result_sender);

    for path in paths {
        if deadline.map_or(false, |d| std::time::Instant::now() >= d) {
            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            break;
        }
        let _ = sender.send(path.clone());
    }
    drop(sender);

    let results: Vec<ScanResult> = result_receiver.iter().collect();

    for handle in handles {
        let _ = handle.join();
    }

    Ok(results)
}

fn count_file_lines(path: &Path) -> usize {
    match std::fs::read(path) {
        Ok(bytes) => memchr::memchr_iter(b'\n', &bytes).count() + 1,
        Err(_) => 0,
    }
}

fn scan_single_file(path: &Path, base_dir: &Path) -> Result<ScanResult> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();
    let modified_ts = metadata.modified().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
    let created_ts = metadata.created().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
    let file_lines = count_file_lines(path);
    let rel_path = path
        .strip_prefix(base_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let mut file = File::open(path)?;
    let read_size = std::cmp::min(file_size as usize, READ_LIMIT_WITH_HEADER);
    let mut buf = vec![0u8; read_size];
    file.read_exact(&mut buf)?;

    // Check UTF-8 validity
    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s,
        Err(_) => {
            return Ok(ScanResult {
                path: rel_path,
                app_name: None,
                summary: None,
                has_header: false,
                file_lines,
                title_fallback: None,
                preview: None,
                modified_ts,
                created_ts,
                size_bytes: file_size,
                calls: 0,
                modified_ago: None,
            });
        }
    };

    // Search for the header marker using memchr
    if let Some(pos) = memmem::find(buf.as_slice(), HEADER_MARKER) {
        let after_marker = &text[pos..];

        let mut app_name = None;
        let mut summary_text = None;
        for line in after_marker.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.starts_with("# ") {
                let title = &trimmed[2..];
                if let Some(dash_pos) = title.find(" — ") {
                    app_name = Some(title[..dash_pos].trim().to_string());
                    summary_text = Some(title[dash_pos + " — ".len()..].trim().to_string());
                } else if let Some(dash_pos) = title.find(" - ") {
                    app_name = Some(title[..dash_pos].trim().to_string());
                    summary_text = Some(title[dash_pos + 3..].trim().to_string());
                } else {
                    app_name = Some(title.trim().to_string());
                }
                break;
            }
        }

        Ok(ScanResult {
            path: rel_path,
            app_name,
            summary: summary_text,
            has_header: true,
            file_lines,
            title_fallback: None,
            preview: None,
            modified_ts,
            created_ts,
            size_bytes: file_size,
            calls: 0,
            modified_ago: None,
        })
    } else {
        let fallback_text = if text.len() > READ_LIMIT_FALLBACK {
            &text[..READ_LIMIT_FALLBACK]
        } else {
            text
        };

        let title = extract_title(fallback_text);
        let preview_text = if fallback_text.len() > 200 {
            format!("{}...", &fallback_text[..200])
        } else {
            fallback_text.to_string()
        };

        Ok(ScanResult {
            path: rel_path,
            app_name: None,
            summary: None,
            has_header: false,
            file_lines,
            title_fallback: title,
            preview: Some(preview_text),
            modified_ts,
            created_ts,
            size_bytes: file_size,
            calls: 0,
            modified_ago: None,
        })
    }
}

fn extract_title(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    if let Some(start) = lower.find("<title>") {
        let after = &text[start + 7..];
        if let Some(end) = after.to_lowercase().find("</title>") {
            let title = after[..end].trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

/// Inject usage stats (calls, modified_ago) into scan results
pub fn inject_stats(results: &mut Vec<ScanResult>, stats_data: &std::collections::HashMap<String, crate::stats::FileStats>) {
    for r in results.iter_mut() {
        let fs = crate::stats::get_by_suffix(&r.path, stats_data);
        r.calls = fs.calls;
        r.modified_ago = Some(crate::stats::format_ago(r.modified_ts));
    }
}

/// Format full scan result as text output
pub fn format_text(result: &FullScanResult, top: usize) -> String {
    let mut output = String::new();

    // Section 1: HTML files
    if !result.html_files.is_empty() {
        output.push_str(&format!("── HTML files ({}) ──\n", result.html_files.len()));
        let display = if top > 0 && top < result.html_files.len() {
            &result.html_files[..top]
        } else {
            &result.html_files
        };
        let rows: Vec<(String, String, String)> = display
            .iter()
            .map(|r| {
                let right = if r.has_header {
                    format!(
                        "{} — {}",
                        r.app_name.as_deref().unwrap_or("Unknown"),
                        r.summary.as_deref().unwrap_or("")
                    )
                } else if let Some(title) = &r.title_fallback {
                    format!("[no header] {}", title)
                } else {
                    format!("[no header] ({} lines)", r.file_lines)
                };
                let meta = format!("({}×, {})",
                    r.calls,
                    r.modified_ago.as_deref().unwrap_or("?"));
                (r.path.clone(), meta, right)
            })
            .collect();
        let max_left = rows.iter().map(|(l, _, _)| l.len()).max().unwrap_or(0);
        let max_meta = rows.iter().map(|(_, m, _)| m.len()).max().unwrap_or(0);
        let table: Vec<_> = rows.iter()
            .map(|(left, meta, right)| format!("{:<lw$}  {:<mw$}  \u{2192}  {}", left, meta, right, lw = max_left, mw = max_meta))
            .collect();
        output.push_str(&table.join("\n"));
        if top > 0 && result.html_files.len() > top {
            output.push_str(&format!("\n  ... {} more", result.html_files.len() - top));
        }
        output.push('\n');
    }

    // Section 2: Directories
    if !result.dirs.is_empty() {
        output.push_str(&format!("\n── Directories ({}) ──\n", result.dirs.len()));
        for d in &result.dirs {
            output.push_str(&format!("  {}/ ({} children)\n", d.path, d.children));
        }
    }

    // Section 3: Other files
    if !result.other_files.is_empty() {
        output.push_str(&format!("\n── Other files ({}) ──\n", result.other_files.len()));
        for f in &result.other_files {
            output.push_str(&format!("  {}\n", f.path));
        }
    }

    output
}

/// Format timeout summary: shows what was scanned and what remains
pub fn format_timeout_summary(result: &FullScanResult) -> String {
    let with_header = result.html_files.iter().filter(|r| r.has_header).count();
    let without_header = result.html_files.len() - with_header;
    let total_lines: usize = result.html_files.iter().map(|r| r.file_lines).sum();
    let remaining = result.html_total.saturating_sub(result.html_files.len());

    // Group by top-level directory
    let mut dir_counts: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    for r in &result.html_files {
        let dir = if let Some(sep) = r.path.find('/') {
            r.path[..sep].to_string()
        } else {
            ".".to_string()
        };
        let entry = dir_counts.entry(dir).or_insert((0, 0));
        entry.0 += 1;
        if r.has_header {
            entry.1 += 1;
        }
    }

    let mut output = format!(
        "\n⏱ Scan timed out. Scanned: {} HTML ({} with header, {} without, {} lines), {} dirs, {} other files\n",
        result.html_files.len(), with_header, without_header, total_lines,
        result.dirs.len(), result.other_files.len()
    );
    if remaining > 0 {
        output.push_str(&format!("Remaining: ~{} HTML files not scanned\n", remaining));
    }
    if !dir_counts.is_empty() {
        output.push_str("\nHTML by directory:\n");
        for (dir, (count, headers)) in &dir_counts {
            output.push_str(&format!("  {:<30} {:>4} files ({} with header)\n", dir, count, headers));
        }
    }
    output
}
