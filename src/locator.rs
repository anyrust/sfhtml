use anyhow::{bail, Result};
use serde::Serialize;

use crate::header;
use crate::js_scope;

#[derive(Debug, Serialize)]
pub struct LocateResult {
    pub anchor: String,
    pub matches: Vec<AnchorMatch>,
}

#[derive(Debug, Serialize)]
pub struct AnchorMatch {
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub context_preview: String,
}

/// Locate an anchor in the file
pub fn locate_anchor(content: &str, anchor: &str, context_lines: usize) -> Result<LocateResult> {
    let lines: Vec<&str> = content.lines().collect();
    let mut matches = Vec::new();

    // First, try to get a hint from the header
    let _header_hint = header::extract_header(content)
        .ok()
        .and_then(|h| {
            h.sections.iter().find(|s| s.number == 5).map(|s| {
                header::parse_anchor_table(&s.content)
            })
        })
        .unwrap_or_default();

    // Search in script regions
    let script_regions = header::find_script_regions(&lines);

    for (region_start, region_end) in &script_regions {
        let region_lines = &lines[*region_start..*region_end];

        for (idx, line) in region_lines.iter().enumerate() {
            if line.contains(anchor) {
                let abs_line = region_start + idx;
                let line_num = abs_line + 1; // 1-based

                // Try to detect scope end
                let end_line = js_scope::detect_scope_end(&lines, abs_line).map(|l| l + 1);

                // Build context preview
                let preview_start = abs_line;
                let preview_end = std::cmp::min(abs_line + 3, lines.len());
                let preview = lines[preview_start..preview_end].join("\n");

                matches.push(AnchorMatch {
                    line: line_num,
                    end_line,
                    context_preview: preview,
                });
            }
        }
    }

    // Also search in HTML for HTML element anchors (e.g., <div id="...">)
    if matches.is_empty() {
        for (idx, line) in lines.iter().enumerate() {
            if line.contains(anchor) {
                let line_num = idx + 1;

                let preview_start = idx;
                let preview_end = std::cmp::min(idx + 3, lines.len());
                let preview = lines[preview_start..preview_end].join("\n");

                matches.push(AnchorMatch {
                    line: line_num,
                    end_line: None,
                    context_preview: preview,
                });
            }
        }
    }

    if matches.is_empty() {
        // Suggest similar anchors using simple Levenshtein-like matching
        let suggestions = find_similar_anchors(content, anchor);
        if suggestions.is_empty() {
            bail!("Error: Anchor \"{}\" not found.", anchor);
        } else {
            bail!(
                "Error: Anchor \"{}\" not found. Did you mean: {}?",
                anchor,
                suggestions.join(", ")
            );
        }
    }

    // Apply context expansion
    if context_lines > 0 {
        for m in &mut matches {
            let start = (m.line as isize - 1 - context_lines as isize).max(0) as usize;
            let end_line = m.end_line.unwrap_or(m.line);
            let end = std::cmp::min(end_line + context_lines, lines.len());
            m.context_preview = lines[start..end].join("\n");
        }
    }

    Ok(LocateResult {
        anchor: anchor.to_string(),
        matches,
    })
}

fn find_similar_anchors(content: &str, query: &str) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    let script_regions = header::find_script_regions(&lines);
    let mut candidates = Vec::new();

    for (start, end) in &script_regions {
        let region = &lines[*start..*end];
        let decls = js_scope::extract_js_declarations(region);
        for (name, _, _) in decls {
            candidates.push(name);
        }
    }

    // Simple substring matching for suggestions
    let query_lower = query.to_lowercase();
    let mut suggestions: Vec<String> = candidates
        .into_iter()
        .filter(|c| {
            let c_lower = c.to_lowercase();
            c_lower.contains(&query_lower)
                || query_lower.contains(&c_lower)
                || levenshtein_distance(&c_lower, &query_lower) <= 5
        })
        .take(5)
        .collect();
    suggestions.dedup();
    suggestions
}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = std::cmp::min(
                std::cmp::min(dp[i - 1][j] + 1, dp[i][j - 1] + 1),
                dp[i - 1][j - 1] + cost,
            );
        }
    }

    dp[m][n]
}

/// List all anchors in a file (from header Section 5 + actual code)
#[derive(Debug, Serialize)]
pub struct AnchorListEntry {
    pub name: String,
    pub line: usize,
    #[serde(rename = "type")]
    pub anchor_type: String,
    pub in_header: bool,
}

pub fn list_anchors(content: &str) -> Vec<AnchorListEntry> {
    let lines: Vec<&str> = content.lines().collect();
    let script_regions = header::find_script_regions(&lines);

    // Get header anchors
    let header_anchors: Vec<String> = header::extract_header(content)
        .ok()
        .and_then(|h| h.sections.into_iter().find(|s| s.number == 5))
        .map(|s| {
            header::parse_anchor_table(&s.content)
                .into_iter()
                .map(|a| a.name)
                .collect()
        })
        .unwrap_or_default();

    // Get actual code declarations
    let mut entries = Vec::new();
    for (start, end) in &script_regions {
        let region = &lines[*start..*end];
        let decls = js_scope::extract_js_declarations(region);
        for (name, decl_type, rel_line) in decls {
            let abs_line = start + rel_line + 1; // 1-based
            let in_header = header_anchors.iter().any(|a| *a == name);
            entries.push(AnchorListEntry {
                name,
                line: abs_line,
                anchor_type: decl_type.to_string(),
                in_header,
            });
        }
    }

    entries
}
