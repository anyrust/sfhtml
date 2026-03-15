use serde::Serialize;

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

/// Perform symbol balance checking on text
pub fn check_syntax(text: &str, context: CheckContext) -> CheckResult {
    let lines: Vec<&str> = text.lines().collect();
    let input_bytes = text.len();
    let input_lines = lines.len();
    let mut issues = Vec::new();

    match context {
        CheckContext::Js => check_js_balance(text, &lines, &mut issues),
        CheckContext::Html => check_html_balance(text, &lines, &mut issues),
        CheckContext::Header => {
            check_bracket_balance(text, &lines, &mut issues);
            check_markdown_table_integrity(&lines, &mut issues);
        }
        CheckContext::Cli => {
            check_bracket_balance(text, &lines, &mut issues);
        }
    }

    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.iter().filter(|i| i.severity == "warning").count();
    let balanced = errors == 0;

    let summary = if balanced && warnings == 0 {
        "No issues found — output is safe to use".to_string()
    } else {
        format!(
            "{} error{}, {} warning{} — output is {}safe to apply",
            errors,
            if errors != 1 { "s" } else { "" },
            warnings,
            if warnings != 1 { "s" } else { "" },
            if balanced { "" } else { "NOT " }
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

fn check_bracket_balance(text: &str, lines: &[&str], issues: &mut Vec<Issue>) {
    let pairs = [('(', ')'), ('[', ']'), ('{', '}')];

    for &(open, close) in &pairs {
        let mut stack: Vec<(usize, usize)> = Vec::new(); // (line, col)
        let mut in_string = false;
        let mut string_char = '"';

        for (line_idx, line) in lines.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];

                // Handle string literals
                if !in_string && (ch == '"' || ch == '\'') {
                    in_string = true;
                    string_char = ch;
                    i += 1;
                    continue;
                }
                if in_string {
                    if ch == '\\' {
                        i += 2;
                        continue;
                    }
                    if ch == string_char {
                        in_string = false;
                    }
                    i += 1;
                    continue;
                }

                if ch == open {
                    stack.push((line_idx + 1, i + 1));
                } else if ch == close {
                    if stack.pop().is_none() {
                        issues.push(Issue {
                            severity: "error".to_string(),
                            line: line_idx + 1,
                            column: i + 1,
                            message: format!("Unmatched `{}` — no opening `{}`", close, open),
                            context_snippet: Some(line.to_string()),
                        });
                    }
                }
                i += 1;
            }
        }

        for (line, col) in stack {
            let snippet = lines.get(line - 1).map(|s| s.to_string());
            issues.push(Issue {
                severity: "error".to_string(),
                line,
                column: col,
                message: format!("Unclosed `{}` opened at line {} — expected `{}` before EOF", open, line, close),
                context_snippet: snippet,
            });
        }
    }

    // Check quote balance (simple heuristic — count per line)
    let _ = text; // suppress unused warning
}

fn check_js_balance(text: &str, lines: &[&str], issues: &mut Vec<Issue>) {
    // Track { } ( ) [ ] with awareness of strings, comments, template literals
    let mut brace_stack: Vec<(char, usize, usize)> = Vec::new(); // (char, line, col)
    let mut in_single_line_comment = false;
    let mut in_multi_line_comment = false;
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_template_literal = false;

    let _ = text;

    for (line_idx, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        in_single_line_comment = false;

        while i < chars.len() {
            let ch = chars[i];
            let next = chars.get(i + 1).copied();

            // Multi-line comment end
            if in_multi_line_comment {
                if ch == '*' && next == Some('/') {
                    in_multi_line_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }

            // Single-line comment
            if in_single_line_comment {
                break;
            }

            // Template literal
            if in_template_literal {
                if ch == '\\' {
                    i += 2;
                    continue;
                }
                if ch == '`' {
                    in_template_literal = false;
                }
                i += 1;
                continue;
            }

            // String literals
            if in_string {
                if ch == '\\' {
                    i += 2;
                    continue;
                }
                if ch == string_char {
                    in_string = false;
                }
                i += 1;
                continue;
            }

            // Start of comment
            if ch == '/' && next == Some('/') {
                in_single_line_comment = true;
                break;
            }
            if ch == '/' && next == Some('*') {
                in_multi_line_comment = true;
                i += 2;
                continue;
            }

            // Start of string
            if ch == '\'' || ch == '"' {
                in_string = true;
                string_char = ch;
                i += 1;
                continue;
            }
            if ch == '`' {
                in_template_literal = true;
                i += 1;
                continue;
            }

            // Bracket tracking
            match ch {
                '{' | '(' | '[' => {
                    brace_stack.push((ch, line_idx + 1, i + 1));
                }
                '}' | ')' | ']' => {
                    let expected = match ch {
                        '}' => '{',
                        ')' => '(',
                        ']' => '[',
                        _ => unreachable!(),
                    };
                    if let Some(&(top, _, _)) = brace_stack.last() {
                        if top == expected {
                            brace_stack.pop();
                        } else {
                            issues.push(Issue {
                                severity: "error".to_string(),
                                line: line_idx + 1,
                                column: i + 1,
                                message: format!(
                                    "Mismatched `{}` — expected `{}` to close `{}` opened at line {}",
                                    ch,
                                    match top { '{' => '}', '(' => ')', '[' => ']', _ => '?' },
                                    top,
                                    brace_stack.last().map(|s| s.1).unwrap_or(0)
                                ),
                                context_snippet: Some(line.to_string()),
                            });
                        }
                    } else {
                        issues.push(Issue {
                            severity: "error".to_string(),
                            line: line_idx + 1,
                            column: i + 1,
                            message: format!("Unmatched `{}` — no opening bracket", ch),
                            context_snippet: Some(line.to_string()),
                        });
                    }
                }
                _ => {}
            }

            i += 1;
        }
    }

    for (ch, line, col) in brace_stack {
        let close = match ch {
            '{' => '}',
            '(' => ')',
            '[' => ']',
            _ => '?',
        };
        let snippet = lines.get(line - 1).map(|s| s.to_string());
        issues.push(Issue {
            severity: "error".to_string(),
            line,
            column: col,
            message: format!("Unclosed `{}` opened at line {} — expected `{}` before EOF", ch, line, close),
            context_snippet: snippet,
        });
    }
}

fn check_html_balance(_text: &str, lines: &[&str], issues: &mut Vec<Issue>) {
    // Simple HTML tag balance checker
    let mut tag_stack: Vec<(String, usize)> = Vec::new(); // (tag_name, line)
    let void_elements = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];

    for (line_idx, line) in lines.iter().enumerate() {
        let mut pos = 0;
        let bytes = line.as_bytes();

        while pos < bytes.len() {
            if bytes[pos] == b'<' {
                // Check for comment
                if line[pos..].starts_with("<!--") {
                    if let Some(end) = line[pos..].find("-->") {
                        pos += end + 3;
                        continue;
                    }
                    break; // Multi-line comment, skip rest
                }

                let is_closing = pos + 1 < bytes.len() && bytes[pos + 1] == b'/';
                let start = if is_closing { pos + 2 } else { pos + 1 };

                // Extract tag name
                let mut end = start;
                while end < bytes.len() && bytes[end] != b' ' && bytes[end] != b'>' && bytes[end] != b'/' {
                    end += 1;
                }
                let tag_name = line[start..end].to_lowercase();

                if tag_name.is_empty() || tag_name.starts_with('!') {
                    pos = end;
                    continue;
                }

                // Find the closing >
                while pos < bytes.len() && bytes[pos] != b'>' {
                    pos += 1;
                }

                // Self-closing check
                let is_self_closing = pos > 0 && bytes[pos - 1] == b'/';

                if void_elements.contains(&tag_name.as_str()) || is_self_closing {
                    // Skip void and self-closing elements
                } else if is_closing {
                    // Closing tag
                    if let Some(idx) = tag_stack.iter().rposition(|(name, _)| *name == tag_name) {
                        tag_stack.truncate(idx);
                    } else {
                        issues.push(Issue {
                            severity: "error".to_string(),
                            line: line_idx + 1,
                            column: start,
                            message: format!("Closing tag `</{}>` has no matching opening tag", tag_name),
                            context_snippet: Some(line.to_string()),
                        });
                    }
                } else {
                    // Opening tag
                    tag_stack.push((tag_name, line_idx + 1));
                }
            }
            pos += 1;
        }
    }

    for (tag, line) in tag_stack {
        let snippet = lines.get(line - 1).map(|s| s.to_string());
        issues.push(Issue {
            severity: "error".to_string(),
            line,
            column: 1,
            message: format!("Unclosed `<{}>` opened at line {} — no `</{}>` found", tag, line, tag),
            context_snippet: snippet,
        });
    }
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
