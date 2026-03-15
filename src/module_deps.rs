use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::header;

const PREVIEW_BYTES: usize = 100;

#[derive(Debug, Serialize)]
pub struct ModuleDep {
    /// The import/reference source text (e.g. "./utils.js")
    pub source: String,
    /// Resolved absolute path (if local)
    pub resolved_path: Option<String>,
    /// Type of reference
    pub dep_type: DepType,
    /// Line number where the import appears (1-based)
    pub line: usize,
    /// Whether the file exists on disk
    pub exists: bool,
    /// If the file has an AI-SKILL-HEADER, show the title line; otherwise first 100 bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Whether the dependent file has an AI-SKILL-HEADER
    pub has_header: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DepType {
    /// ES module: import ... from '...' / import('...')
    JsModule,
    /// Classic script: <script src="...">
    JsScript,
    /// CSS link: <link rel="stylesheet" href="...">
    CssLink,
    /// CSS @import: @import url('...')
    CssImport,
    /// HTML reference: <iframe src="...">, <object data="...">
    HtmlRef,
}

impl std::fmt::Display for DepType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepType::JsModule => write!(f, "js-module"),
            DepType::JsScript => write!(f, "js-script"),
            DepType::CssLink => write!(f, "css-link"),
            DepType::CssImport => write!(f, "css-import"),
            DepType::HtmlRef => write!(f, "html-ref"),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ModuleDepsResult {
    pub file: String,
    pub total: usize,
    pub local: usize,
    pub remote: usize,
    pub missing: usize,
    pub deps: Vec<ModuleDep>,
}

/// Scan an HTML file for all local module/resource dependencies
pub fn scan_deps(file_path: &Path) -> Result<ModuleDepsResult> {
    let content = std::fs::read_to_string(file_path)?;
    let base_dir = file_path.parent().unwrap_or(Path::new("."));
    let lines: Vec<&str> = content.lines().collect();

    let mut deps = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // ES Module: import ... from '...' / import ... from "..."
        scan_es_imports(trimmed, line_num, &mut deps);

        // Dynamic import: import('...')
        scan_dynamic_imports(trimmed, line_num, &mut deps);

        // <script src="...">
        scan_script_src(trimmed, line_num, &mut deps);

        // <link rel="stylesheet" href="...">
        scan_link_href(trimmed, line_num, &mut deps);

        // @import url('...')  or  @import '...'
        scan_css_import(trimmed, line_num, &mut deps);

        // <iframe src="...">, <object data="...">
        scan_html_refs(trimmed, line_num, &mut deps);
    }

    // Resolve paths and check existence
    let mut local_count = 0;
    let mut remote_count = 0;
    let mut missing_count = 0;

    for dep in &mut deps {
        if is_remote(&dep.source) {
            remote_count += 1;
            dep.exists = true; // We don't verify remote URLs
            continue;
        }

        local_count += 1;
        let resolved = resolve_path(base_dir, &dep.source);
        dep.resolved_path = Some(resolved.display().to_string());

        if resolved.exists() {
            dep.exists = true;
            dep.preview = Some(get_file_preview(&resolved));
            dep.has_header = check_has_header(&resolved);
        } else {
            dep.exists = false;
            missing_count += 1;
        }
    }

    Ok(ModuleDepsResult {
        file: file_path.display().to_string(),
        total: deps.len(),
        local: local_count,
        remote: remote_count,
        missing: missing_count,
        deps,
    })
}

/// Recursively scan dependencies up to `max_depth` levels deep.
/// Merges all discovered deps into a single result, avoiding cycles via visited set.
pub fn scan_deps_recursive(file_path: &Path, max_depth: usize) -> Result<ModuleDepsResult> {
    use std::collections::HashSet;

    let canonical = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
    let mut visited: HashSet<PathBuf> = HashSet::new();
    visited.insert(canonical.clone());

    let mut all_deps: Vec<ModuleDep> = Vec::new();
    let mut queue: Vec<(PathBuf, usize)> = vec![(file_path.to_path_buf(), 0)];

    while let Some((current_file, current_depth)) = queue.pop() {
        let result = scan_deps(&current_file)?;
        for mut dep in result.deps {
            // Tag depth for display
            if current_depth > 0 {
                dep.source = format!("[depth {}] {}", current_depth, dep.source);
            }
            // If local, existing, and within depth limit, enqueue for further scanning
            if dep.exists && current_depth < max_depth {
                if let Some(resolved) = &dep.resolved_path {
                    let resolved_path = PathBuf::from(resolved);
                    let canon = resolved_path.canonicalize().unwrap_or(resolved_path.clone());
                    if !visited.contains(&canon) {
                        visited.insert(canon);
                        queue.push((resolved_path, current_depth + 1));
                    }
                }
            }
            all_deps.push(dep);
        }
    }

    let local_count = all_deps.iter().filter(|d| !is_remote_from_source(&d.source)).count();
    let remote_count = all_deps.iter().filter(|d| is_remote_from_source(&d.source)).count();
    let missing_count = all_deps.iter().filter(|d| !d.exists && !is_remote_from_source(&d.source)).count();

    Ok(ModuleDepsResult {
        file: file_path.display().to_string(),
        total: all_deps.len(),
        local: local_count,
        remote: remote_count,
        missing: missing_count,
        deps: all_deps,
    })
}

/// Check if a source string (possibly with depth prefix) is remote
fn is_remote_from_source(source: &str) -> bool {
    // Strip "[depth N] " prefix if present
    let clean = if source.starts_with("[depth ") {
        if let Some(pos) = source.find("] ") {
            &source[pos + 2..]
        } else {
            source
        }
    } else {
        source
    };
    is_remote(clean)
}

/// Extract between quotes: accepts 'x' or "x", returns the inner content
fn extract_quoted(s: &str) -> Option<&str> {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

/// Find content between first quote pair in a substring
fn find_quoted_in(s: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        if let Some(start) = s.find(quote) {
            if let Some(end) = s[start + 1..].find(quote) {
                return Some(&s[start + 1..start + 1 + end]);
            }
        }
    }
    None
}

// --- Scanners ---

fn scan_es_imports(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    // import X from 'path'
    // import { X } from 'path'
    // import * as X from 'path'
    // import 'path'
    // export ... from 'path'
    if !(line.starts_with("import ") || line.starts_with("export ")) {
        return;
    }

    if let Some(from_pos) = line.find(" from ") {
        let rest = &line[from_pos + 6..];
        let rest = rest.trim().trim_end_matches(';');
        if let Some(path) = extract_quoted(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::JsModule,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
    } else if line.starts_with("import ") {
        // bare import: import 'path';
        let rest = line["import ".len()..].trim().trim_end_matches(';');
        if let Some(path) = extract_quoted(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::JsModule,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
    }
}

fn scan_dynamic_imports(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    // import('./path.js')  or  import("./path.js")
    let mut search = line;
    while let Some(pos) = search.find("import(") {
        let rest = &search[pos + 7..];
        if let Some(path) = find_quoted_in(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::JsModule,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
        search = &search[pos + 7..];
    }
}

fn scan_script_src(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    let lower = line.to_lowercase();
    if !lower.contains("<script") || !lower.contains("src=") {
        return;
    }
    if let Some(pos) = lower.find("src=") {
        let rest = &line[pos + 4..];
        if let Some(path) = find_quoted_in(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::JsScript,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
    }
}

fn scan_link_href(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    let lower = line.to_lowercase();
    if !lower.contains("<link") || !lower.contains("stylesheet") {
        return;
    }
    if let Some(pos) = lower.find("href=") {
        let rest = &line[pos + 5..];
        if let Some(path) = find_quoted_in(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::CssLink,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
    }
}

fn scan_css_import(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    let trimmed = line.trim();
    if !trimmed.starts_with("@import") {
        return;
    }
    let rest = &trimmed[7..].trim();
    // @import url('path') or @import url("path")
    if rest.starts_with("url(") {
        let inner = &rest[4..];
        if let Some(end) = inner.find(')') {
            let url_part = &inner[..end].trim();
            let path = url_part.trim_matches(|c| c == '\'' || c == '"');
            if !path.is_empty() {
                deps.push(ModuleDep {
                    source: path.to_string(),
                    resolved_path: None,
                    dep_type: DepType::CssImport,
                    line: line_num,
                    exists: false,
                    preview: None,
                    has_header: false,
                });
            }
        }
    } else {
        // @import 'path';  or  @import "path";
        let rest = rest.trim_end_matches(';').trim();
        if let Some(path) = extract_quoted(rest) {
            deps.push(ModuleDep {
                source: path.to_string(),
                resolved_path: None,
                dep_type: DepType::CssImport,
                line: line_num,
                exists: false,
                preview: None,
                has_header: false,
            });
        }
    }
}

fn scan_html_refs(line: &str, line_num: usize, deps: &mut Vec<ModuleDep>) {
    let lower = line.to_lowercase();

    // <iframe src="...">
    if lower.contains("<iframe") {
        if let Some(pos) = lower.find("src=") {
            let rest = &line[pos + 4..];
            if let Some(path) = find_quoted_in(rest) {
                deps.push(ModuleDep {
                    source: path.to_string(),
                    resolved_path: None,
                    dep_type: DepType::HtmlRef,
                    line: line_num,
                    exists: false,
                    preview: None,
                    has_header: false,
                });
            }
        }
    }

    // <object data="...">
    if lower.contains("<object") {
        if let Some(pos) = lower.find("data=") {
            let rest = &line[pos + 5..];
            if let Some(path) = find_quoted_in(rest) {
                deps.push(ModuleDep {
                    source: path.to_string(),
                    resolved_path: None,
                    dep_type: DepType::HtmlRef,
                    line: line_num,
                    exists: false,
                    preview: None,
                    has_header: false,
                });
            }
        }
    }
}

// --- Helpers ---

fn is_remote(source: &str) -> bool {
    source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("//")
        || source.starts_with("data:")
}

fn resolve_path(base_dir: &Path, source: &str) -> PathBuf {
    let p = Path::new(source);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

/// Get file preview: if has AI-SKILL-HEADER → title line; otherwise first 100 bytes
fn get_file_preview(path: &Path) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return "(unreadable)".to_string(),
    };

    // Try to extract header title
    if let Ok(h) = header::extract_header(&content) {
        if let Some(title) = &h.title_line {
            return title.clone();
        }
    }

    // Fallback: first PREVIEW_BYTES bytes, trimmed
    let preview: String = content.chars().take(PREVIEW_BYTES).collect();
    let preview = preview.replace('\n', " ").replace('\r', "");
    preview.trim().to_string()
}

fn check_has_header(path: &Path) -> bool {
    // Quick check: read first 8KB and look for marker
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = vec![0u8; 8192];
    let n = match std::io::Read::read(&mut file, &mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    buf.truncate(n);
    memchr::memmem::find(&buf, b"<!-- AI-SKILL-HEADER START").is_some()
}

/// Format as human readable text, optionally truncated to top N deps
pub fn format_text(result: &ModuleDepsResult, top: usize) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Dependencies for {} ({} total, {} local, {} remote, {} missing)\n\n",
        result.file, result.total, result.local, result.remote, result.missing
    ));

    let deps_display = if top > 0 && top < result.deps.len() {
        &result.deps[..top]
    } else {
        &result.deps
    };

    for dep in deps_display {
        let status = if is_remote(&dep.source) {
            "↗ remote".to_string()
        } else if dep.exists {
            if dep.has_header {
                "✓ header".to_string()
            } else {
                "✓ exists".to_string()
            }
        } else {
            "✗ MISSING".to_string()
        };

        output.push_str(&format!(
            "  [{}] {:<13} {:<40} (line {})\n",
            status, dep.dep_type, dep.source, dep.line
        ));

        if let Some(preview) = &dep.preview {
            let disp = if preview.len() > 72 {
                format!("{}...", &preview[..69])
            } else {
                preview.clone()
            };
            output.push_str(&format!("    → {}\n", disp));
        }
    }

    if top > 0 && result.deps.len() > top {
        output.push_str(&format!(
            "\n... and {} more (use --top 0 to show all)\n",
            result.deps.len() - top
        ));
    }

    if result.missing > 0 {
        output.push_str(&format!(
            "\n⚠ {} missing dependency file{}. AI should create or fix these references.\n",
            result.missing,
            if result.missing != 1 { "s" } else { "" }
        ));
    }

    output
}
