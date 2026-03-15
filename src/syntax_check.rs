use serde::Serialize;
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

/// Result of a syntax/symbol balance check
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub input_bytes: usize,
    pub input_lines: usize,
    pub balanced: bool,
    pub issues: Vec<Issue>,
    pub summary: String,
}

#[derive(Debug, Serialize)]
pub struct Issue {
    pub severity: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snippet: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheckContext {
    Cli,
    Header,
    Js,
    Html,
}

impl CheckContext {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cli" => Self::Cli,
            "header" => Self::Header,
            "js" => Self::Js,
            "html" => Self::Html,
            _ => Self::Cli,
        }
    }
}

/// Perform syntax checking using oxc_parser (JS) and html5ever (HTML)
pub fn check_syntax(text: &str, context: CheckContext) -> CheckResult {
    let lines: Vec<&str> = text.lines().collect();
    let input_bytes = text.len();
    let input_lines = lines.len();
    let mut issues = Vec::new();

    match context {
        CheckContext::Js => check_js_with_oxc(text, &mut issues),
        CheckContext::Html => {
            // For full HTML: extract <script> contents and validate with oxc,
            // then validate HTML structure with html5ever
            check_html_with_parsers(text, &lines, &mut issues);
        }
        CheckContext::Header => {
            check_markdown_table_integrity(&lines, &mut issues);
        }
        CheckContext::Cli => {
            check_js_with_oxc(text, &mut issues);
        }
    }

    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.iter().filter(|i| i.severity == "warning").count();
    let balanced = errors == 0;

    let summary = if balanced && warnings == 0 {
        "No issues found — output is safe to use".to_string()
    } else {
        format!(
            "{} error{}, {} warning{} — review before applying",
            errors,
            if errors != 1 { "s" } else { "" },
            warnings,
            if warnings != 1 { "s" } else { "" },
        )
    };

    CheckResult {
        input_bytes,
        input_lines,
        balanced,
        issues,
        summary,
    }
}

/// Validate JavaScript using oxc_parser AST
fn check_js_with_oxc(js_text: &str, issues: &mut Vec<Issue>) {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path("check.js")
        .unwrap_or_default()
        .with_module(true);
    let ret = Parser::new(&allocator, js_text, source_type).parse();

    for error in ret.errors {
        let msg = format!("{}", error);
        // Extract line info from the error message or default to line 0
        let (line, col) = extract_line_col_from_error(&msg, js_text);
        issues.push(Issue {
            severity: "warning".to_string(),
            line,
            column: col,
            message: msg,
            context_snippet: js_text.lines().nth(line.saturating_sub(1)).map(|s| s.to_string()),
        });
    }
}

/// Validate full HTML document: html5ever for structure, oxc_parser for embedded JS
fn check_html_with_parsers(html: &str, lines: &[&str], issues: &mut Vec<Issue>) {
    // 1. Parse HTML with html5ever to check structure
    check_html_structure(html, lines, issues);

    // 2. Extract <script> blocks and validate JS inside them
    let mut in_script = false;
    let mut script_start: usize = 0;
    let mut script_content = String::new();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim().to_lowercase();
        if !in_script && trimmed.contains("<script") && trimmed.contains('>') {
            in_script = true;
            script_start = idx + 1;
            // Capture text after <script...> on the same line
            if let Some(pos) = line.to_lowercase().find('>') {
                let after = &line[pos + 1..];
                if !after.trim().is_empty() {
                    script_content.push_str(after);
                    script_content.push('\n');
                }
            }
            continue;
        }
        if in_script && trimmed.contains("</script") {
            // Validate the accumulated JS content
            if !script_content.trim().is_empty() {
                let allocator = Allocator::default();
                let source_type = SourceType::from_path("inline.js")
                    .unwrap_or_default()
                    .with_module(true);
                let ret = Parser::new(&allocator, &script_content, source_type).parse();
                for error in ret.errors {
                    let msg = format!("{}", error);
                    let (err_line, col) = extract_line_col_from_error(&msg, &script_content);
                    let absolute_line = script_start + err_line.saturating_sub(1);
                    issues.push(Issue {
                        severity: "warning".to_string(),
                        line: absolute_line,
                        column: col,
                        message: format!("JS syntax: {}", msg),
                        context_snippet: lines.get(absolute_line.saturating_sub(1)).map(|s| s.to_string()),
                    });
                }
            }
            in_script = false;
            script_content.clear();
            continue;
        }
        if in_script {
            script_content.push_str(line);
            script_content.push('\n');
        }
    }
}

/// Check HTML structure using html5ever DOM parsing
fn check_html_structure(html: &str, lines: &[&str], issues: &mut Vec<Issue>) {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap_or_else(|_| RcDom::default());

    // html5ever is error-recovering — collect its parse errors
    for err in &dom.errors {
        let msg = format!("{}", err);
        // Try to find the relevant line
        let line = find_error_line(lines, &msg);
        issues.push(Issue {
            severity: "warning".to_string(),
            line,
            column: 0,
            message: format!("HTML parse: {}", msg),
            context_snippet: lines.get(line.saturating_sub(1)).map(|s| s.to_string()),
        });
    }

    // Check for unclosed significant elements by walking the DOM
    check_dom_completeness(&dom.document, lines, issues);
}

/// Walk DOM to detect issues like unclosed elements
fn check_dom_completeness(node: &Handle, lines: &[&str], issues: &mut Vec<Issue>) {
    let children = node.children.borrow();
    for child in children.iter() {
        if let NodeData::Element { ref name, .. } = child.data {
            let tag = name.local.to_string();
            // Check if a significant element has both opening and closing tags
            // html5ever auto-closes, so we verify by looking at source lines
            let void_elements = [
                "area", "base", "br", "col", "embed", "hr", "img", "input",
                "link", "meta", "param", "source", "track", "wbr",
            ];
            if !void_elements.contains(&tag.as_str()) {
                let has_open = lines.iter().any(|l| {
                    let lower = l.to_lowercase();
                    lower.contains(&format!("<{}", tag)) && !lower.contains(&format!("</{}", tag))
                });
                let has_close = lines.iter().any(|l| l.to_lowercase().contains(&format!("</{}>", tag)));
                if has_open && !has_close && !["html", "head", "body"].contains(&tag.as_str()) {
                    let line = lines.iter().position(|l| l.to_lowercase().contains(&format!("<{}", tag)))
                        .map(|i| i + 1).unwrap_or(0);
                    issues.push(Issue {
                        severity: "warning".to_string(),
                        line,
                        column: 0,
                        message: format!("HTML: `<{}>` has no matching `</{}>` in source", tag, tag),
                        context_snippet: lines.get(line.saturating_sub(1)).map(|s| s.to_string()),
                    });
                }
            }
        }
        check_dom_completeness(child, lines, issues);
    }
}

/// Extract line/column from an oxc error message (format: "filename:line:col ...")
fn extract_line_col_from_error(msg: &str, source: &str) -> (usize, usize) {
    // oxc errors typically contain offset info; fall back to line 1
    // Try to find a pattern like ": something at offset N"
    let _ = source;
    // Simple heuristic: look for line:col pattern
    let parts: Vec<&str> = msg.splitn(4, ':').collect();
    if parts.len() >= 3 {
        if let (Ok(line), Ok(col)) = (parts[1].trim().parse::<usize>(), parts[2].trim().parse::<usize>()) {
            if line > 0 {
                return (line, col);
            }
        }
    }
    (1, 0)
}

/// Try to match an error message to a source line
fn find_error_line(lines: &[&str], error_msg: &str) -> usize {
    // html5ever errors don't always have line info, try basic matching
    let lower = error_msg.to_lowercase();
    if let Some(tag_start) = lower.find('<') {
        if let Some(tag_end) = lower[tag_start..].find('>') {
            let tag_ref = &error_msg[tag_start..tag_start + tag_end + 1];
            for (i, line) in lines.iter().enumerate() {
                if line.to_lowercase().contains(&tag_ref.to_lowercase()) {
                    return i + 1;
                }
            }
        }
    }
    0
}

fn check_markdown_table_integrity(lines: &[&str], issues: &mut Vec<Issue>) {
    let mut in_table = false;
    let mut expected_pipes = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let pipe_count = trimmed.chars().filter(|c| *c == '|').count();
            if !in_table {
                in_table = true;
                expected_pipes = pipe_count;
            } else if pipe_count != expected_pipes {
                issues.push(Issue {
                    severity: "warning".to_string(),
                    line: line_idx + 1,
                    column: 1,
                    message: format!(
                        "Markdown table row has {} `|` but expected {} (based on header row)",
                        pipe_count, expected_pipes
                    ),
                    context_snippet: Some(line.to_string()),
                });
            }
        } else {
            in_table = false;
        }
    }
}
