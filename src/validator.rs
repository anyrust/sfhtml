use anyhow::Result;
use serde::Serialize;

use crate::header;
use crate::html_structure;
use crate::js_scope;
use crate::syntax_check::{self, CheckContext};

#[derive(Debug, Serialize)]
pub struct ValidateResult {
    pub anchor_consistency: AnchorConsistency,
    pub syntax_validation: SyntaxValidation,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Debug, Serialize)]
pub struct AnchorConsistency {
    pub total_in_header: usize,
    pub found_in_code: usize,
    pub missing_from_code: Vec<String>,
    pub missing_from_header: Vec<MissingAnchor>,
    pub api_issues: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MissingAnchor {
    pub name: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct SyntaxValidation {
    pub brackets_balanced: bool,
    pub id_references_valid: bool,
    pub id_issues: Vec<String>,
    pub bracket_issues: Vec<String>,
}

/// Validate header-to-code consistency
pub fn validate_file(content: &str, check_syntax_flag: bool) -> Result<ValidateResult> {
    let lines: Vec<&str> = content.lines().collect();

    // Get header anchors
    let header_info = header::extract_header(content).ok();
    let header_anchors: Vec<String> = header_info
        .as_ref()
        .and_then(|h| h.sections.iter().find(|s| s.number == 5))
        .map(|s| {
            header::parse_anchor_table(&s.content)
                .into_iter()
                .map(|a| a.name)
                .collect()
        })
        .unwrap_or_default();

    // Get actual code declarations
    let script_regions = header::find_script_regions(&lines);
    let mut code_decls = Vec::new();
    for (start, end) in &script_regions {
        let region = &lines[*start..*end];
        let decls = js_scope::extract_js_declarations(region);
        for (name, _, rel_line) in decls {
            code_decls.push((name, start + rel_line + 1));
        }
    }

    // Anchor consistency
    let mut missing_from_code = Vec::new();
    for header_anchor in &header_anchors {
        let found = code_decls.iter().any(|(name, _)| name == header_anchor);
        if !found {
            missing_from_code.push(header_anchor.clone());
        }
    }

    let mut missing_from_header = Vec::new();
    for (name, line) in &code_decls {
        let found = header_anchors.iter().any(|a| a == name);
        if !found {
            missing_from_header.push(MissingAnchor {
                name: name.clone(),
                line: *line,
            });
        }
    }

    let found_in_code = header_anchors.len() - missing_from_code.len();

    // API consistency check (window.XXX)
    let api_issues = check_api_consistency(content, &header_info);

    let anchor_consistency = AnchorConsistency {
        total_in_header: header_anchors.len(),
        found_in_code,
        missing_from_code,
        missing_from_header,
        api_issues,
    };

    // Syntax validation
    let syntax_validation = if check_syntax_flag {
        validate_syntax(content, &header_info)
    } else {
        SyntaxValidation {
            brackets_balanced: true,
            id_references_valid: true,
            id_issues: Vec::new(),
            bracket_issues: Vec::new(),
        }
    };

    let errors = anchor_consistency.missing_from_code.len()
        + if !syntax_validation.brackets_balanced { 1 } else { 0 }
        + syntax_validation.id_issues.iter().filter(|i| i.contains("not found")).count();

    let warnings = anchor_consistency.missing_from_header.len()
        + anchor_consistency.api_issues.len();

    Ok(ValidateResult {
        anchor_consistency,
        syntax_validation,
        errors,
        warnings,
    })
}

fn check_api_consistency(
    content: &str,
    header_info: &Option<header::HeaderInfo>,
) -> Vec<String> {
    let mut issues = Vec::new();

    // Look for window.XXX references in header Section 2
    if let Some(info) = header_info {
        if let Some(section) = info.sections.iter().find(|s| s.number == 2) {
            // Find window.XXX patterns
            for line in section.content.lines() {
                let mut pos = 0;
                while let Some(idx) = line[pos..].find("window.") {
                    let start = pos + idx + 7;
                    let end = line[start..]
                        .find(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
                        .map(|i| start + i)
                        .unwrap_or(line.len());
                    let api_name = &line[start..end];
                    if !api_name.is_empty() && !content.contains(&format!("window.{}", api_name)) {
                        // Check if it's defined somewhere in the code
                        let full_ref = format!("window.{}", api_name);
                        if !content.contains(&full_ref) {
                            issues.push(format!(
                                "API {} listed in header but definition not found in code",
                                full_ref
                            ));
                        }
                    }
                    pos = end;
                }
            }
        }
    }

    issues
}

fn validate_syntax(
    content: &str,
    header_info: &Option<header::HeaderInfo>,
) -> SyntaxValidation {
    let mut bracket_issues = Vec::new();
    let mut id_issues = Vec::new();

    // Check header content syntax
    if let Some(info) = header_info {
        let check = syntax_check::check_syntax(&info.full_markdown, CheckContext::Header);
        for issue in &check.issues {
            bracket_issues.push(format!(
                "Line {} in header: {}",
                issue.line, issue.message
            ));
        }

        // Check id references in Section 6 Tag-Pair Tree
        if let Some(section6) = info.sections.iter().find(|s| s.number == 6) {
            for line in section6.content.lines() {
                if let Some(id_start) = line.find("id=\"") {
                    let rest = &line[id_start + 4..];
                    if let Some(id_end) = rest.find('"') {
                        let id_val = &rest[..id_end];
                        if !html_structure::id_exists(content, id_val) {
                            id_issues.push(format!(
                                "id=\"{}\" referenced in Tag-Pair Tree but not found in HTML",
                                id_val
                            ));
                        }
                    }
                }
            }
        }
    }

    let brackets_balanced = bracket_issues.is_empty();
    let id_references_valid = id_issues.is_empty();

    SyntaxValidation {
        brackets_balanced,
        id_references_valid,
        id_issues,
        bracket_issues,
    }
}

/// Format validation result as text
pub fn format_text(result: &ValidateResult) -> String {
    let mut output = String::new();

    output.push_str("=== Anchor Consistency ===\n");
    output.push_str(&format!(
        "✓ {}/{} anchors found in code\n",
        result.anchor_consistency.found_in_code,
        result.anchor_consistency.total_in_header
    ));

    for name in &result.anchor_consistency.missing_from_code {
        output.push_str(&format!("✗ Header lists \"{}\" but not found in code\n", name));
    }

    for anchor in &result.anchor_consistency.missing_from_header {
        output.push_str(&format!(
            "✗ Missing from header: {} (line {})\n",
            anchor.name, anchor.line
        ));
    }

    for issue in &result.anchor_consistency.api_issues {
        output.push_str(&format!("⚠ {}\n", issue));
    }

    output.push_str("\n=== Syntax Validation ===\n");

    if result.syntax_validation.brackets_balanced {
        output.push_str("✓ Bracket pairs balanced\n");
    } else {
        for issue in &result.syntax_validation.bracket_issues {
            output.push_str(&format!("✗ {}\n", issue));
        }
    }

    if result.syntax_validation.id_references_valid {
        output.push_str("✓ All id references exist in HTML\n");
    } else {
        for issue in &result.syntax_validation.id_issues {
            output.push_str(&format!("✗ {}\n", issue));
        }
    }

    output.push_str(&format!(
        "\n→ {} error{}, {} warning{}.",
        result.errors,
        if result.errors != 1 { "s" } else { "" },
        result.warnings,
        if result.warnings != 1 { "s" } else { "" },
    ));

    if result.errors > 0 || result.warnings > 0 {
        output.push_str(" Run `sfhtml validate --fix` to auto-repair or pass to AI for review.\n");
    } else {
        output.push('\n');
    }

    output
}
