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
const MAX_HTML_FULL: usize = 300;

// --- Sort ---

#[derive(Debug, Clone, Copy)]
pub enum SortKey {
    Modified,
    Created,
    Name,
    Size,
}

impl SortKey {
    pub fn from_str(s: &str) -> Self {
        match s {
            "created" => SortKey::Created,
            "name" => SortKey::Name,
            "size" => SortKey::Size,
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
    /// true = full scan; false = rough mode (over 300 HTML limit)
    #[serde(skip_serializing_if = "is_true")]
    pub rough: bool,
    /// modification time as unix timestamp (for sorting)
    #[serde(skip)]
    pub modified_ts: u64,
    /// creation time as unix timestamp
    #[serde(skip)]
    pub created_ts: u64,
    /// file size in bytes (for sorting)
    #[serde(skip)]
    pub size_bytes: u64,
}

fn is_true(v: &bool) -> bool { *v }

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
    pub html_rough: Vec<ScanResult>,
    pub dirs: Vec<DirEntry>,
    pub other_files: Vec<OtherFileEntry>,
    pub html_total: usize,
    pub html_scanned_full: usize,
    /// Combined count of rough + dirs + other (before truncation)
    pub misc_total: usize,
    /// True if misc_total exceeded the 1000 limit
    pub misc_truncated: bool,
}

/// Full workspace scan: HTML files (full + rough), folders, and other files.
/// Auto-deepens into subdirectories until 300 HTML files found or tree exhausted.
/// html_rough + dirs + other_files share a combined misc_limit.
pub fn scan_directory(
    dir: &Path,
    recursive: bool,
    jobs: usize,
    sort_key: SortKey,
    sort_order: SortOrder,
    match_keywords: &[String],
    misc_limit: usize,
) -> Result<FullScanResult> {
    let misc_limit = if misc_limit == 0 { usize::MAX } else { misc_limit };
    let mut html_paths: Vec<PathBuf> = Vec::new();
    let mut dir_entries: Vec<DirEntry> = Vec::new();
    let mut other_files: Vec<OtherFileEntry> = Vec::new();
    let mut misc_total: usize = 0;
    let mut misc_stopped = false;

    // Helper: check if we've hit the misc limit
    let misc_count = |dirs: &[DirEntry], others: &[OtherFileEntry], rough_count: usize| -> usize {
        dirs.len() + others.len() + rough_count
    };

    if recursive {
        // Recursive: walk entire tree, but respect misc limit for non-full-HTML items
        let walker = WalkDir::new(dir).follow_links(true);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
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
                misc_total += 1;
                if !misc_stopped {
                    let children = std::fs::read_dir(path).map(|rd| rd.count()).unwrap_or(0);
                    dir_entries.push(DirEntry { path: rel, children });
                    if misc_count(&dir_entries, &other_files, 0) >= misc_limit {
                        misc_stopped = true;
                    }
                }
            } else if path.is_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
                    html_paths.push(path.to_path_buf());
                } else {
                    misc_total += 1;
                    if !misc_stopped {
                        other_files.push(OtherFileEntry { path: rel });
                        if misc_count(&dir_entries, &other_files, 0) >= misc_limit {
                            misc_stopped = true;
                        }
                    }
                }
            }
        }
    } else {
        // Non-recursive with auto-deepen: BFS by depth until 300 HTML found
        let mut queue: Vec<PathBuf> = vec![dir.to_path_buf()];
        let mut _depth = 0;

        while !queue.is_empty() && html_paths.len() < MAX_HTML_FULL {
            let mut next_queue: Vec<PathBuf> = Vec::new();

            for current_dir in &queue {
                let entries = match std::fs::read_dir(current_dir) {
                    Ok(rd) => rd,
                    Err(_) => continue,
                };
                for entry in entries.filter_map(|e| e.ok()) {
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
                        misc_total += 1;
                        if !misc_stopped {
                            let children = std::fs::read_dir(&path).map(|rd| rd.count()).unwrap_or(0);
                            dir_entries.push(DirEntry { path: rel, children });
                        if misc_count(&dir_entries, &other_files, 0) >= misc_limit {
                            misc_stopped = true;
                        }
                    }
                } else if path.is_file() {
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
                            html_paths.push(path);
                        } else {
                            misc_total += 1;
                            if !misc_stopped {
                                other_files.push(OtherFileEntry { path: rel });
                                if misc_count(&dir_entries, &other_files, 0) >= misc_limit {
                                    misc_stopped = true;
                                }
                            }
                        }
                    }
                }
            }

            _depth += 1;
            // If we already have enough HTML, stop deepening
            if html_paths.len() >= MAX_HTML_FULL {
                break;
            }
            // If misc limit hit and no more HTML to find at this depth, stop
            if misc_stopped && next_queue.is_empty() {
                break;
            }
            queue = next_queue;
        }
    }

    let html_total = html_paths.len();

    // Sort HTML paths by the chosen key before splitting full vs rough
    sort_paths_by_key(&mut html_paths, sort_key, sort_order);

    // Split: first MAX_HTML_FULL get full scan, rest get rough scan
    let (full_paths, rough_paths) = if html_paths.len() > MAX_HTML_FULL {
        let (a, b) = html_paths.split_at(MAX_HTML_FULL);
        (a.to_vec(), b.to_vec())
    } else {
        (html_paths, Vec::new())
    };

    // Full scan (multi-threaded)
    let mut html_files = scan_batch(&full_paths, dir, jobs, false)?;
    sort_scan_results(&mut html_files, sort_key, sort_order);

    // Rough scan — also subject to the shared misc limit
    let rough_budget = if misc_stopped { 0 } else { misc_limit - misc_count(&dir_entries, &other_files, 0) };
    let rough_to_scan = if rough_paths.len() > rough_budget { &rough_paths[..rough_budget] } else { &rough_paths };
    let mut html_rough: Vec<ScanResult> = rough_to_scan
        .iter()
        .filter_map(|p| scan_rough(p, dir).ok())
        .collect();
    sort_scan_results(&mut html_rough, sort_key, sort_order);

    misc_total += rough_paths.len();
    let misc_truncated = misc_total > misc_limit || misc_stopped;

    Ok(FullScanResult {
        html_files,
        html_rough,
        dirs: dir_entries,
        other_files,
        html_total,
        html_scanned_full: std::cmp::min(html_total, MAX_HTML_FULL),
        misc_total,
        misc_truncated,
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
            SortKey::Modified => {
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
        };
        if order == SortOrder::Desc { cmp.reverse() } else { cmp }
    });
}

fn scan_batch(paths: &[PathBuf], dir: &Path, jobs: usize, rough: bool) -> Result<Vec<ScanResult>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let num_threads = if jobs == 0 {
        std::cmp::min(num_cpus::get(), 8)
    } else {
        jobs
    };

    if num_threads <= 1 || paths.len() <= 1 {
        let results: Vec<ScanResult> = paths
            .iter()
            .filter_map(|p| if rough { scan_rough(p, dir).ok() } else { scan_single_file(p, dir).ok() })
            .collect();
        return Ok(results);
    }

    let (sender, receiver) = crossbeam_channel::bounded::<PathBuf>(num_threads * 4);
    let (result_sender, result_receiver) = crossbeam_channel::unbounded::<ScanResult>();
    let dir_owned = dir.to_path_buf();

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let rx = receiver.clone();
            let tx = result_sender.clone();
            let d = dir_owned.clone();
            std::thread::spawn(move || {
                while let Ok(path) = rx.recv() {
                    if let Ok(result) = scan_single_file(&path, &d) {
                        let _ = tx.send(result);
                    }
                }
            })
        })
        .collect();

    drop(result_sender);

    for path in paths {
        let _ = sender.send(path.clone());
    }
    drop(sender);

    let results: Vec<ScanResult> = result_receiver.iter().collect();

    for handle in handles {
        let _ = handle.join();
    }

    Ok(results)
}

/// Rough scan: only path + has_header boolean + file_lines, no title/preview extraction
fn scan_rough(path: &Path, base_dir: &Path) -> Result<ScanResult> {
    let metadata = std::fs::metadata(path)?;
    let file_size = metadata.len();
    let modified_ts = metadata.modified().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
    let created_ts = metadata.created().map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()).unwrap_or(0);
    let rel_path = path.strip_prefix(base_dir).unwrap_or(path).to_string_lossy().to_string();

    // Quick header check: read first 8KB
    let has_header = {
        let mut f = File::open(path)?;
        let read_size = std::cmp::min(file_size as usize, READ_LIMIT_WITH_HEADER);
        let mut buf = vec![0u8; read_size];
        f.read_exact(&mut buf)?;
        memmem::find(&buf, HEADER_MARKER).is_some()
    };

    let file_lines = count_file_lines(path);

    Ok(ScanResult {
        path: rel_path,
        app_name: None,
        summary: None,
        has_header,
        file_lines,
        title_fallback: None,
        preview: None,
        rough: true,
        modified_ts,
        created_ts,
        size_bytes: file_size,
    })
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
                rough: false,
                modified_ts,
                created_ts,
                size_bytes: file_size,
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
            rough: false,
            modified_ts,
            created_ts,
            size_bytes: file_size,
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
            rough: false,
            modified_ts,
            created_ts,
            size_bytes: file_size,
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

/// Format full scan result as text output
pub fn format_text(result: &FullScanResult, top: usize) -> String {
    let mut output = String::new();

    // Section 1: HTML files (full scan)
    if !result.html_files.is_empty() {
        output.push_str(&format!("── HTML files ({} full-scanned) ──\n", result.html_scanned_full));
        let display = if top > 0 && top < result.html_files.len() {
            &result.html_files[..top]
        } else {
            &result.html_files
        };
        let rows: Vec<(String, String)> = display
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
                (r.path.clone(), right)
            })
            .collect();
        let max_left = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        let table: Vec<_> = rows.iter()
            .map(|(left, right)| format!("{:<width$}  \u{2192}  {}", left, right, width = max_left))
            .collect();
        output.push_str(&table.join("\n"));
        if top > 0 && result.html_files.len() > top {
            output.push_str(&format!("\n  ... {} more full-scanned", result.html_files.len() - top));
        }
        output.push('\n');
    }

    // Section 2: HTML files (rough scan, overflow)
    if !result.html_rough.is_empty() {
        output.push_str(&format!("\n── HTML rough ({} over limit) ──\n", result.html_rough.len()));
        for r in &result.html_rough {
            let mark = if r.has_header { "✓" } else { "·" };
            output.push_str(&format!("  {} {} ({} lines)\n", mark, r.path, r.file_lines));
        }
    }

    // Section 3: Directories
    if !result.dirs.is_empty() {
        output.push_str(&format!("\n── Directories ({}) ──\n", result.dirs.len()));
        for d in &result.dirs {
            output.push_str(&format!("  {}/ ({} children)\n", d.path, d.children));
        }
    }

    // Section 4: Other files
    if !result.other_files.is_empty() {
        output.push_str(&format!("\n── Other files ({}) ──\n", result.other_files.len()));
        for f in &result.other_files {
            output.push_str(&format!("  {}\n", f.path));
        }
    }

    // Misc truncation warning
    if result.misc_truncated {
        output.push_str(&format!(
            "\n⚠ Non-HTML output truncated ({}+ items, showing {}). Use --misc-limit {} to see more.\n",
            result.misc_total,
            result.dirs.len() + result.other_files.len() + result.html_rough.len(),
            result.misc_total.min(result.misc_total + 1000)
        ));
    }

    output
}

/// Format full scan result as a compact summary
pub fn format_summary(result: &FullScanResult) -> String {
    let all_html: Vec<&ScanResult> = result.html_files.iter().chain(result.html_rough.iter()).collect();
    let with_header = all_html.iter().filter(|r| r.has_header).count();
    let without_header = result.html_total - with_header;
    let total_lines: usize = all_html.iter().map(|r| r.file_lines).sum();

    // Group by top-level directory
    let mut dir_counts: std::collections::BTreeMap<String, (usize, usize)> = std::collections::BTreeMap::new();
    for r in &all_html {
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
        "Scan summary: {} HTML ({} with header, {} without, {} lines), {} dirs, {} other files{}\n\n",
        result.html_total, with_header, without_header, total_lines,
        result.dirs.len(), result.other_files.len(),
        if result.misc_truncated { format!(" (misc truncated at {})", result.misc_total) } else { String::new() }
    );
    if !dir_counts.is_empty() {
        output.push_str("HTML by directory:\n");
        for (dir, (count, headers)) in &dir_counts {
            output.push_str(&format!("  {:<30} {:>4} files ({} with header)\n", dir, count, headers));
        }
    }
    output.push_str(&format!("\nUse --top N to list specific files, or --summary=false to show all."));
    output
}
