use anyhow::{bail, Result};
use serde::Serialize;

use crate::js_scope;

/// A script region with tag info and content boundaries (all 0-based)
#[derive(Debug, Clone)]
pub struct ScriptRegion {
    pub tag_line: usize,       // line of <script...>
    pub content_start: usize,  // first line of JS content
    pub content_end: usize,    // exclusive end of JS content
    pub close_line: usize,     // line of </script>
    pub tag_label: String,     // e.g. "<script type=\"module\">" or "<script>"
}

const HEADER_START: &str = "<!-- AI-SKILL-HEADER START";
const HEADER_END: &str = "AI-SKILL-HEADER END -->";

#[derive(Debug, Serialize)]
pub struct HeaderInfo {
    pub full_markdown: String,
    pub title_line: Option<String>,
    pub app_name: Option<String>,
    pub summary: Option<String>,
    pub sections: Vec<HeaderSection>,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_quality: Option<HeaderQuality>,
}

#[derive(Debug, Serialize)]
pub struct HeaderSection {
    pub number: usize,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct HeaderQuality {
    pub completeness: f32,
    pub stale_anchors: usize,
    pub missing_anchors: usize,
    pub section_5_coverage: String,
}

#[derive(Debug, Serialize)]
pub struct AnchorEntry {
    pub name: String,
    pub purpose: String,
}

/// Extract the full AI-SKILL-HEADER from a file
pub fn extract_header(content: &str) -> Result<HeaderInfo> {
    let lines: Vec<&str> = content.lines().collect();

    let mut start_line = 0;
    let mut end_line = 0;
    let mut found_start = false;

    for (idx, line) in lines.iter().enumerate() {
        if line.trim().starts_with(HEADER_START) {
            start_line = idx + 1;
            found_start = true;
        }
        if found_start && line.trim().contains(HEADER_END) {
            end_line = idx + 1;
            break;
        }
    }

    if !found_start || end_line == 0 {
        bail!("No AI-SKILL-HEADER found. Run `sfhtml init <file>` to add one.");
    }

    // Extract the markdown between markers
    let header_lines = &lines[start_line..end_line - 1]; // Exclude both marker lines
    let full_markdown = header_lines.join("\n");

    // Parse title
    let mut app_name = None;
    let mut summary = None;
    let mut title_line = None;

    for line in header_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            title_line = Some(trimmed.to_string());
            let title = &trimmed[2..];
            if let Some(dash_pos) = title.find(" — ") {
                app_name = Some(title[..dash_pos].trim().to_string());
                summary = Some(title[dash_pos + " — ".len()..].trim().to_string());
            } else if let Some(dash_pos) = title.find(" - ") {
                app_name = Some(title[..dash_pos].trim().to_string());
                summary = Some(title[dash_pos + 3..].trim().to_string());
            } else {
                app_name = Some(title.trim().to_string());
            }
            break;
        }
    }

    // Parse sections
    let sections = parse_sections(&full_markdown);

    Ok(HeaderInfo {
        full_markdown,
        title_line,
        app_name,
        summary,
        sections,
        start_line,
        end_line,
        header_quality: None,
    })
}

/// Parse markdown sections (## N. Title)
fn parse_sections(markdown: &str) -> Vec<HeaderSection> {
    let mut sections = Vec::new();
    let mut current_number = 0;
    let mut current_title = String::new();
    let mut current_content = String::new();
    let mut in_section = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## ") {
            // Save previous section
            if in_section {
                sections.push(HeaderSection {
                    number: current_number,
                    title: current_title.clone(),
                    content: current_content.trim().to_string(),
                });
            }

            // Parse "## N. Title" or "## N Title"
            let rest = &trimmed[3..];
            if let Some(dot_pos) = rest.find(". ") {
                if let Ok(num) = rest[..dot_pos].trim().parse::<usize>() {
                    current_number = num;
                    current_title = rest[dot_pos + 2..].trim().to_string();
                } else {
                    current_number = sections.len() + 1;
                    current_title = rest.to_string();
                }
            } else {
                current_number = sections.len() + 1;
                current_title = rest.to_string();
            }
            current_content = String::new();
            in_section = true;
        } else if in_section {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Save last section
    if in_section {
        sections.push(HeaderSection {
            number: current_number,
            title: current_title,
            content: current_content.trim().to_string(),
        });
    }

    sections
}

/// Extract a specific section by number
pub fn extract_section(content: &str, section_num: usize) -> Result<HeaderSection> {
    let header = extract_header(content)?;
    header
        .sections
        .into_iter()
        .find(|s| s.number == section_num)
        .ok_or_else(|| anyhow::anyhow!("Section {} not found in header", section_num))
}

/// Parse anchor entries from Section 5 content
/// Supports definition list (`- \`name\` — purpose`), table (`| Anchor | Purpose |`),
/// and legacy table (`| Name | Type | Line | Purpose |`) formats
pub fn parse_anchor_list(section_content: &str) -> Vec<AnchorEntry> {
    parse_anchor_list_with_issues(section_content).0
}

/// Parse anchor entries and also return lines that look like anchor entries but failed to parse.
pub fn parse_anchor_list_with_issues(section_content: &str) -> (Vec<AnchorEntry>, Vec<String>) {
    let mut anchors = Vec::new();
    let mut unparseable = Vec::new();

    for line in section_content.lines() {
        let trimmed = line.trim();

        // Skip empty lines, headings, and description-only lines
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('(') {
            continue;
        }

        let mut parsed = false;

        // Definition list format: - `name` — purpose
        if trimmed.starts_with("- `") {
            if let Some(rest) = trimmed.strip_prefix("- `") {
                if let Some(end_tick) = rest.find('`') {
                    let name = rest[..end_tick].to_string();
                    let after = rest[end_tick + 1..].trim();
                    let purpose = after.strip_prefix("—").or_else(|| after.strip_prefix("-")).unwrap_or(after).trim().to_string();
                    if !name.is_empty() {
                        anchors.push(AnchorEntry { name, purpose });
                        parsed = true;
                    }
                }
            }
        }

        // Table format fallback: | col1 | col2 | ...
        if !parsed && trimmed.starts_with('|') && !trimmed.starts_with("|---") {
            let parts: Vec<&str> = trimmed.split('|').collect();
            let is_legacy = parts.len() >= 5 && {
                let h = parts[1].trim();
                h != "Name" && h != "Anchor"
            };
            let (name, purpose) = if is_legacy && parts.len() >= 5 {
                (parts[1].trim().trim_matches('`').to_string(), parts[4].trim().to_string())
            } else if parts.len() >= 3 {
                (parts[1].trim().trim_matches('`').to_string(), parts[2].trim().to_string())
            } else {
                ("".to_string(), "".to_string())
            };
            if !name.is_empty() && name != "Name" && name != "Anchor" && !name.starts_with("---") {
                anchors.push(AnchorEntry { name, purpose });
                parsed = true;
            }
        }

        // Detect lines that look like they should be anchor entries but failed to parse
        if !parsed && (trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with('|')) {
            unparseable.push(trimmed.to_string());
        }
    }

    (anchors, unparseable)
}

/// Extract subsections (### 5.1, ### 5.2, etc.) from Section 5 content
fn extract_subsections(section_content: &str) -> Option<String> {
    let mut result = String::new();
    let mut in_subsection = false;

    for line in section_content.lines() {
        if line.trim().starts_with("### ") {
            in_subsection = true;
        }
        if in_subsection {
            result.push_str(line);
            result.push('\n');
        }
    }

    if result.is_empty() {
        None
    } else {
        Some(result.trim_end().to_string())
    }
}

/// Rebuild Section 5 of the header based on actual code blocks
pub fn rebuild_header(
    content: &str,
    preserve_descriptions: bool,
) -> Result<String> {
    let header = extract_header(content)?;

    // Get existing descriptions if preserving
    let old_section5 = header.sections.iter().find(|s| s.number == 5);
    let old_anchors = old_section5
        .map(|s| parse_anchor_list(&s.content))
        .unwrap_or_default();

    let lines: Vec<&str> = content.lines().collect();

    // Build new Section 5 from block-level anchors
    let section5 = build_section5_content(&lines, preserve_descriptions, &old_anchors);

    // Reconstruct header: preserve sections 1-4, replace 5
    let mut new_markdown = String::new();

    // Title line
    if let Some(title) = &header.title_line {
        new_markdown.push_str(title);
        new_markdown.push('\n');
        new_markdown.push('\n');
    }

    // Preserved sections (1-4)
    for section in &header.sections {
        if section.number >= 5 {
            continue;
        }
        new_markdown.push_str(&format!("## {}. {}\n", section.number, section.title));
        if !section.content.is_empty() {
            new_markdown.push_str(&section.content);
            new_markdown.push('\n');
        }
        new_markdown.push('\n');
    }

    // New Section 5
    new_markdown.push_str("## 5. Key Internal Modules\n\n");
    new_markdown.push_str(&section5);

    // Preserve subsections (### 5.1, etc.) from old header
    if let Some(old_s5) = old_section5 {
        if let Some(subs) = extract_subsections(&old_s5.content) {
            new_markdown.push('\n');
            new_markdown.push_str(&subs);
            new_markdown.push('\n');
        }
    }
    new_markdown.push('\n');

    // Now replace header in original content
    let start_marker_line = header.start_line - 1; // 0-based
    let end_marker_line = header.end_line - 1; // 0-based

    let mut result_lines: Vec<String> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx == start_marker_line {
            result_lines.push(HEADER_START.to_string());
            for md_line in new_markdown.lines() {
                result_lines.push(md_line.to_string());
            }
        } else if idx == end_marker_line {
            result_lines.push(format!("    {}", HEADER_END));
        } else if idx > start_marker_line && idx < end_marker_line {
            // Skip old header content
            continue;
        } else {
            result_lines.push(line.to_string());
        }
    }

    Ok(result_lines.join("\n"))
}

/// Generate an initial AI-SKILL-HEADER for a file without one
pub fn generate_init_header(content: &str) -> Result<String> {
    let lines: Vec<&str> = content.lines().collect();

    // Extract app name from <title>
    let app_name = extract_title_text(content).unwrap_or_else(|| "MyApp".to_string());

    // Build Section 5 from block-level anchors
    let section5 = build_section5_content(&lines, false, &[]);

    let header = format!(
        r#"<!-- AI-SKILL-HEADER START
# {} — (feature summary)

## 1. Overview
(App description, deployment method)

## 2. Public JavaScript API
(Functions exposed on window, parameters, return values, side effects)

## 3. Automation Example
(Puppeteer / Playwright examples)

## 4. Conventions
(Units, angle formats, state management rules)

## 5. Key Internal Modules

{}
    AI-SKILL-HEADER END -->"#,
        app_name, section5
    );

    // Insert after <head>
    let mut result = String::new();
    let mut inserted = false;

    for line in lines.iter() {
        result.push_str(line);
        result.push('\n');
        if !inserted && line.to_lowercase().contains("<head") {
            result.push_str(&header);
            result.push('\n');
            inserted = true;
        }
    }

    if !inserted {
        // No <head> found, prepend
        result = format!("{}\n{}", header, content);
    }

    Ok(result)
}

/// Build Section 5 content with block-level anchors:
/// - `<script>` blocks with their top-level declarations as description
/// - HTML elements with `id` attribute
fn build_section5_content(
    lines: &[&str],
    preserve_descriptions: bool,
    old_anchors: &[AnchorEntry],
) -> String {
    let script_regions = find_script_regions_full(lines);
    let id_elements = find_html_id_elements(lines);

    let mut section5 = String::new();

    // Script block anchors
    for region in &script_regions {
        let label = &region.tag_label;
        let region_lines = &lines[region.content_start..region.content_end];
        let decls = js_scope::extract_js_declarations(region_lines);

        // Build auto-description: list of declaration names
        let decl_names: Vec<String> = decls.iter().map(|(name, _, _)| {
            // Strip keyword prefix for brevity: "function initApp" -> "initApp"
            if let Some(rest) = name.strip_prefix("function ") {
                rest.to_string()
            } else if let Some(rest) = name.strip_prefix("class ") {
                rest.to_string()
            } else if let Some(rest) = name.strip_prefix("const ") {
                rest.to_string()
            } else if let Some(rest) = name.strip_prefix("let ") {
                rest.to_string()
            } else if let Some(rest) = name.strip_prefix("var ") {
                rest.to_string()
            } else {
                name.clone()
            }
        }).collect();

        let purpose = if preserve_descriptions {
            // Try to preserve existing description for this script block
            old_anchors
                .iter()
                .find(|a| a.name == *label)
                .map(|a| a.purpose.clone())
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| decl_names.join(", "))
        } else {
            decl_names.join(", ")
        };

        if purpose.is_empty() {
            section5.push_str(&format!("- `{}`\n", label));
        } else {
            section5.push_str(&format!("- `{}` — {}\n", label, purpose));
        }
    }

    // HTML element anchors (elements with id)
    for (id_val, _line_idx, tag_label) in &id_elements {
        let purpose = if preserve_descriptions {
            old_anchors
                .iter()
                .find(|a| a.name == *tag_label)
                .map(|a| a.purpose.clone())
                .filter(|p| !p.is_empty())
                .unwrap_or_default()
        } else {
            String::new()
        };

        // Skip common structural ids that are not interesting
        if id_val == "app" || id_val == "root" || id_val == "main" {
            if purpose.is_empty() {
                section5.push_str(&format!("- `{}`\n", tag_label));
            } else {
                section5.push_str(&format!("- `{}` — {}\n", tag_label, purpose));
            }
        } else if purpose.is_empty() {
            section5.push_str(&format!("- `{}`\n", tag_label));
        } else {
            section5.push_str(&format!("- `{}` — {}\n", tag_label, purpose));
        }
    }

    section5
}

/// Given a 1-based absolute line number, find the purpose comment from the line above the declaration.
fn find_purpose_from_comment(
    lines: &[&str],
    script_regions: &[(usize, usize)],
    abs_line_1based: usize,
) -> String {
    // Find which script region this line belongs to
    for (start, end) in script_regions {
        let region_lines = &lines[*start..*end];
        let abs_line_0based = abs_line_1based.saturating_sub(1);
        if abs_line_0based >= *start && abs_line_0based < *end {
            let rel_idx = abs_line_0based - start;
            if let Some(purpose) = js_scope::extract_purpose_comment(region_lines, rel_idx) {
                return purpose;
            }
        }
    }
    String::new()
}

/// Find <script> ... </script> regions, returning (start_line, end_line) pairs (0-based, exclusive end)
pub fn find_script_regions(lines: &[&str]) -> Vec<(usize, usize)> {
    find_script_regions_full(lines)
        .iter()
        .map(|r| (r.content_start, r.content_end))
        .collect()
}

/// Find <script> regions with full tag info
pub fn find_script_regions_full(lines: &[&str]) -> Vec<ScriptRegion> {
    let mut regions = Vec::new();
    let mut in_script = false;
    let mut tag_line = 0;
    let mut tag_label = String::new();

    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if !in_script && lower.contains("<script") {
            if !lower.contains("src=") {
                in_script = true;
                tag_line = idx;
                // Build label from the actual tag
                tag_label = extract_script_tag_label(line);
            }
        }
        if in_script && lower.contains("</script>") {
            regions.push(ScriptRegion {
                tag_line,
                content_start: tag_line + 1,
                content_end: idx,
                close_line: idx,
                tag_label: tag_label.clone(),
            });
            in_script = false;
        }
    }

    if in_script {
        regions.push(ScriptRegion {
            tag_line,
            content_start: tag_line + 1,
            content_end: lines.len(),
            close_line: lines.len().saturating_sub(1),
            tag_label,
        });
    }

    regions
}

/// Extract a clean label from a <script ...> tag line, e.g. `<script type="module">`
fn extract_script_tag_label(line: &str) -> String {
    let trimmed = line.trim();
    // Find the <script...> portion
    let lower = trimmed.to_lowercase();
    if let Some(start) = lower.find("<script") {
        let after = &trimmed[start..];
        if let Some(end) = after.find('>') {
            return after[..=end].to_string();
        }
    }
    "<script>".to_string()
}

/// Find HTML elements with id attributes, return (id_value, tag_line 0-based, tag_label)
pub fn find_html_id_elements(lines: &[&str]) -> Vec<(String, usize, String)> {
    let mut elements = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        // Skip script/style tags
        if lower.contains("<script") || lower.contains("<style") {
            continue;
        }
        if let Some(id_val) = extract_id_attr(line) {
            let tag_label = extract_element_tag_label(line, &id_val);
            elements.push((id_val, idx, tag_label));
        }
    }
    elements
}

/// Extract id="..." value from a line
fn extract_id_attr(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let pattern = "id=\"";
    if let Some(pos) = lower.find(pattern) {
        let after = &line[pos + pattern.len()..];
        if let Some(end) = after.find('"') {
            let val = after[..end].to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// Build a tag label like `<div id="app">` from a line
fn extract_element_tag_label(line: &str, id: &str) -> String {
    let trimmed = line.trim();
    let lower = trimmed.to_lowercase();
    // Find the tag name
    if let Some(start) = lower.find('<') {
        let after = &lower[start + 1..];
        let tag_end = after.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(after.len());
        let tag = &after[..tag_end];
        return format!("<{} id=\"{}\">", tag, id);
    }
    format!("<div id=\"{}\">", id)
}

fn extract_title_text(content: &str) -> Option<String> {
    let lower = content.to_lowercase();
    if let Some(start) = lower.find("<title>") {
        let after = &content[start + 7..];
        if let Some(end) = after.to_lowercase().find("</title>") {
            let title = after[..end].trim().to_string();
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}
