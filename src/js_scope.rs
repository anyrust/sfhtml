/// Lightweight JS scope detector — brace counting with string/comment awareness
/// Used by the locate command to determine the end line of JS declarations

/// Detect the end of a JS scope starting from a given line.
/// Returns the 0-based line index where the scope ends (closing brace).
pub fn detect_scope_end(lines: &[&str], start_line: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut found_first_open = false;
    let mut in_single_comment = false;
    let mut in_multi_comment = false;
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_template = false;

    for (idx, line) in lines.iter().enumerate().skip(start_line) {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        in_single_comment = false;

        // Reset string state at start of each line — JS strings don't span lines
        // This prevents false positives from regex literals containing quotes
        if in_string {
            in_string = false;
        }

        while i < chars.len() {
            let ch = chars[i];
            let next = chars.get(i + 1).copied();

            if in_multi_comment {
                if ch == '*' && next == Some('/') {
                    in_multi_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }

            if in_single_comment {
                break;
            }

            if in_template {
                if ch == '\\' {
                    i += 2;
                    continue;
                }
                if ch == '`' {
                    in_template = false;
                }
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

            // Comment start
            if ch == '/' && next == Some('/') {
                in_single_comment = true;
                break;
            }
            if ch == '/' && next == Some('*') {
                in_multi_comment = true;
                i += 2;
                continue;
            }

            // String start
            if ch == '\'' || ch == '"' {
                in_string = true;
                string_char = ch;
                i += 1;
                continue;
            }
            if ch == '`' {
                in_template = true;
                i += 1;
                continue;
            }

            if ch == '{' {
                depth += 1;
                found_first_open = true;
            } else if ch == '}' {
                depth -= 1;
                if found_first_open && depth == 0 {
                    return Some(idx);
                }
            }

            i += 1;
        }
    }

    None
}

/// Classify a JS declaration type from a line
#[derive(Debug, Clone, PartialEq)]
pub enum JsDeclType {
    Const,
    Let,
    Var,
    Function,
    Class,
    Unknown,
}

impl std::fmt::Display for JsDeclType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsDeclType::Const => write!(f, "js-const"),
            JsDeclType::Let => write!(f, "js-let"),
            JsDeclType::Var => write!(f, "js-var"),
            JsDeclType::Function => write!(f, "js-function"),
            JsDeclType::Class => write!(f, "js-class"),
            JsDeclType::Unknown => write!(f, "js-unknown"),
        }
    }
}

/// Extract top-level JS declarations from script content lines
/// Returns: Vec<(name, declaration_type, 0-based line index within the provided lines)>
pub fn extract_js_declarations(lines: &[&str]) -> Vec<(String, JsDeclType, usize)> {
    let mut results = Vec::new();
    let mut depth: i32 = 0;
    let mut in_single_comment = false;
    let mut in_multi_comment = false;
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_template = false;

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        in_single_comment = false;

        // Reset string state at start of each line — JS strings don't span lines
        if in_string {
            in_string = false;
        }

        // At top level (depth 0), look for declarations
        if depth == 0 && !in_multi_comment && !in_string && !in_template {
            if let Some(decl) = parse_declaration(trimmed) {
                results.push((decl.0, decl.1, line_idx));
            }
        }

        // Track brace depth
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];
            let next = chars.get(i + 1).copied();

            if in_multi_comment {
                if ch == '*' && next == Some('/') {
                    in_multi_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }

            if in_single_comment {
                break;
            }

            if in_template {
                if ch == '\\' {
                    i += 2;
                    continue;
                }
                if ch == '`' {
                    in_template = false;
                }
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

            if ch == '/' && next == Some('/') {
                in_single_comment = true;
                break;
            }
            if ch == '/' && next == Some('*') {
                in_multi_comment = true;
                i += 2;
                continue;
            }
            if ch == '\'' || ch == '"' {
                in_string = true;
                string_char = ch;
                i += 1;
                continue;
            }
            if ch == '`' {
                in_template = true;
                i += 1;
                continue;
            }

            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }

            i += 1;
        }
    }

    results
}

fn parse_declaration(trimmed: &str) -> Option<(String, JsDeclType)> {
    let prefixes: &[(&str, JsDeclType)] = &[
        ("const ", JsDeclType::Const),
        ("let ", JsDeclType::Let),
        ("var ", JsDeclType::Var),
        ("function ", JsDeclType::Function),
        ("class ", JsDeclType::Class),
        ("async function ", JsDeclType::Function),
    ];

    for (prefix, decl_type) in prefixes {
        if trimmed.starts_with(prefix) {
            let rest = &trimmed[prefix.len()..];
            // Extract the name (up to space, (, =, {, <)
            let name_end = rest
                .find(|c: char| c == ' ' || c == '(' || c == '=' || c == '{' || c == '<')
                .unwrap_or(rest.len());
            let name = rest[..name_end].trim().to_string();
            if !name.is_empty() {
                let full_name = if *prefix == "async function " {
                    format!("function {}", name)
                } else {
                    format!("{}{}", prefix, name)
                };
                return Some((full_name, decl_type.clone()));
            }
        }
    }
    None
}
