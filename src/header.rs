use anyhow::{bail, Result};
use serde::Serialize;

use crate::html_structure;
use crate::js_scope;

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
    pub section_6_depth: usize,
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
pub fn parse_anchor_table(section_content: &str) -> Vec<AnchorEntry> {
    let mut anchors = Vec::new();

    for line in section_content.lines() {
        let trimmed = line.trim();
        // Look for markdown table rows: | name | purpose |
        if trimmed.starts_with('|') && !trimmed.starts_with("|---") && !trimmed.starts_with("| Name") && !trimmed.starts_with("| ---") {
            let parts: Vec<&str> = trimmed.split('|').collect();
            if parts.len() >= 3 {
                let name = parts[1].trim().trim_matches('`').to_string();
                let purpose = parts[2].trim().to_string();
                if !name.is_empty() && name != "Name" && name != "---" {
                    anchors.push(AnchorEntry { name, purpose });
                }
            }
        }
    }

    anchors
}

/// Rebuild Sections 5 and 6 of the header based on actual code
pub fn rebuild_header(
    content: &str,
    preserve_descriptions: bool,
) -> Result<String> {
    let header = extract_header(content)?;

    // Get existing descriptions if preserving
    let old_section5 = header.sections.iter().find(|s| s.number == 5);
    let old_anchors = old_section5
        .map(|s| parse_anchor_table(&s.content))
        .unwrap_or_default();

    // Find script sections and extract JS declarations
    let lines: Vec<&str> = content.lines().collect();
    let script_regions = find_script_regions(&lines);

    let mut all_decls = Vec::new();
    for (start, end) in &script_regions {
        let region_lines = &lines[*start..*end];
        let decls = js_scope::extract_js_declarations(region_lines);
        for (name, decl_type, rel_line) in decls {
            all_decls.push((name, decl_type, start + rel_line + 1)); // 1-based
        }
    }

    // Build new Section 5
    let mut section5 = String::new();
    section5.push_str("| Name | Type | Line | Purpose |\n");
    section5.push_str("|------|------|------|--------|\n");
    for (name, decl_type, line) in &all_decls {
        let purpose = if preserve_descriptions {
            old_anchors
                .iter()
                .find(|a| a.name == *name)
                .map(|a| a.purpose.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        section5.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            name, decl_type, line, purpose
        ));
    }

    // Build new Section 6 (tag-pair tree)
    let tree = html_structure::build_tag_pair_tree(content);
    let section6 = tree.join("\n");

    // Reconstruct header: preserve sections 1-4, replace 5 and 6
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
    new_markdown.push('\n');

    // New Section 6
    new_markdown.push_str("## 6. File Navigation Index\n\n");
    new_markdown.push_str("```\n");
    new_markdown.push_str(&section6);
    new_markdown.push_str("\n```\n");

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

    // Find script declarations
    let script_regions = find_script_regions(&lines);
    let mut all_decls = Vec::new();
    for (start, end) in &script_regions {
        let region_lines = &lines[*start..*end];
        let decls = js_scope::extract_js_declarations(region_lines);
        for (name, decl_type, rel_line) in decls {
            all_decls.push((name, decl_type, start + rel_line + 1));
        }
    }

    // Build Section 5
    let mut section5 = String::new();
    section5.push_str("| Name | Type | Line | Purpose |\n");
    section5.push_str("|------|------|------|--------|\n");
    for (name, decl_type, line) in &all_decls {
        section5.push_str(&format!("| `{}` | {} | {} | |\n", name, decl_type, line));
    }

    // Build Section 6
    let tree = html_structure::build_tag_pair_tree(content);
    let section6 = tree.join("\n");

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
## 6. File Navigation Index

```
{}
```

    AI-SKILL-HEADER END -->"#,
        app_name, section5, section6
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

/// Find <script> ... </script> regions, returning (start_line, end_line) pairs (0-based, exclusive end)
pub fn find_script_regions(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut in_script = false;
    let mut script_start = 0;

    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if !in_script && lower.contains("<script") {
            // Check if it's not a src= external script
            if !lower.contains("src=") {
                in_script = true;
                script_start = idx + 1; // Start from the line after <script>
            }
        }
        if in_script && lower.contains("</script>") {
            regions.push((script_start, idx));
            in_script = false;
        }
    }

    if in_script {
        regions.push((script_start, lines.len()));
    }

    regions
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
