use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub path: String,
    pub score: u32,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Serialize)]
pub struct SearchMatch {
    pub line: usize,
    pub content: String,
    #[serde(rename = "type")]
    pub match_type: String,
}

/// Search HTML files for a query using TF-based scoring
pub fn search_files(dir: &Path, query: &str, top_n: usize, context_lines: usize) -> Result<Vec<SearchResult>> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    let walker = WalkDir::new(dir).follow_links(true);
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.eq_ignore_ascii_case("html") && !ext.eq_ignore_ascii_case("htm") {
            continue;
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            let rel_path = path
                .strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            if let Some(result) = score_file(&rel_path, &content, &query_lower, context_lines) {
                results.push(result);
            }
        }
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.cmp(&a.score));
    results.truncate(top_n);

    Ok(results)
}

fn score_file(path: &str, content: &str, query: &str, context_lines: usize) -> Option<SearchResult> {
    let _content_lower = content.to_lowercase();
    let lines: Vec<&str> = content.lines().collect();
    let lines_lower: Vec<String> = lines.iter().map(|l| l.to_lowercase()).collect();
    let mut score: u32 = 0;
    let mut matches = Vec::new();

    // Check filename
    let filename_lower = path.to_lowercase();
    if filename_lower.contains(query) {
        score += 10;
        matches.push(SearchMatch {
            line: 0,
            content: path.to_string(),
            match_type: "filename".to_string(),
        });
    }

    // Determine regions
    let mut in_header = false;
    let mut in_body = false;

    for (idx, line_lower) in lines_lower.iter().enumerate() {
        let is_title_line = line_lower.contains("<title>") || line_lower.contains("</title>");
        if line_lower.contains("<!-- ai-skill-header start") {
            in_header = true;
        }
        if line_lower.contains("ai-skill-header end -->") {
            in_header = false;
        }
        if line_lower.contains("<body") {
            in_body = true;
        }

        if line_lower.contains(query) {
            let weight = if is_title_line {
                10
            } else if in_header {
                5
            } else if in_body {
                1
            } else {
                1
            };
            score += weight;

            let match_type = if is_title_line {
                "title"
            } else if in_header {
                "header"
            } else {
                "body"
            };

            let mut match_content = lines[idx].to_string();
            if context_lines > 0 {
                let start = idx.saturating_sub(context_lines);
                let end = std::cmp::min(idx + context_lines + 1, lines.len());
                match_content = lines[start..end].join("\n");
            }

            matches.push(SearchMatch {
                line: idx + 1,
                content: match_content,
                match_type: match_type.to_string(),
            });
        }
    }

    if score > 0 {
        Some(SearchResult {
            path: path.to_string(),
            score,
            matches,
        })
    } else {
        None
    }
}
