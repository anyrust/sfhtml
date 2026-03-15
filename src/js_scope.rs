/// JS scope and declaration detection using oxc_parser AST
/// Provides reliable function/class/variable boundary detection

use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::SourceType;

/// Classify a JS declaration type
#[derive(Debug, Clone, PartialEq)]
pub enum JsDeclType {
    Const,
    Let,
    Var,
    Function,
    Class,
    #[allow(dead_code)]
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

/// A JS declaration with precise start/end line info from AST
#[derive(Debug, Clone)]
pub struct JsDeclaration {
    pub name: String,
    pub decl_type: JsDeclType,
    pub start_line: usize, // 0-based relative to provided lines
    pub end_line: usize,   // 0-based relative to provided lines
}

/// Build a byte-offset → line-number lookup table
fn build_line_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

/// Convert byte offset to 0-based line number
fn offset_to_line(offsets: &[usize], byte_offset: u32) -> usize {
    let offset = byte_offset as usize;
    match offsets.binary_search(&offset) {
        Ok(line) => line,
        Err(line) => line.saturating_sub(1),
    }
}

/// Extract top-level JS declarations with precise scope boundaries using oxc_parser.
/// Returns Vec<JsDeclaration> with start_line and end_line (0-based, relative to provided lines).
pub fn extract_js_declarations_full(lines: &[&str]) -> Vec<JsDeclaration> {
    let source = lines.join("\n");
    let allocator = Allocator::default();
    let source_type = SourceType::mjs();
    let ret = Parser::new(&allocator, &source, source_type).parse();

    let line_offsets = build_line_offsets(&source);
    let mut results = Vec::new();

    for stmt in ret.program.body.iter() {
        match stmt {
            Statement::FunctionDeclaration(decl) => {
                if let Some(ref id) = decl.id {
                    let name = format!("function {}", id.name);
                    let start = offset_to_line(&line_offsets, decl.span.start);
                    let end = offset_to_line(&line_offsets, decl.span.end.saturating_sub(1));
                    results.push(JsDeclaration {
                        name,
                        decl_type: JsDeclType::Function,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Statement::ClassDeclaration(decl) => {
                if let Some(ref id) = decl.id {
                    let name = format!("class {}", id.name);
                    let start = offset_to_line(&line_offsets, decl.span.start);
                    let end = offset_to_line(&line_offsets, decl.span.end.saturating_sub(1));
                    results.push(JsDeclaration {
                        name,
                        decl_type: JsDeclType::Class,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Statement::VariableDeclaration(decl) => {
                let decl_type = match decl.kind {
                    VariableDeclarationKind::Const => JsDeclType::Const,
                    VariableDeclarationKind::Let => JsDeclType::Let,
                    VariableDeclarationKind::Var => JsDeclType::Var,
                    _ => JsDeclType::Unknown,
                };
                let keyword = match decl.kind {
                    VariableDeclarationKind::Const => "const",
                    VariableDeclarationKind::Let => "let",
                    VariableDeclarationKind::Var => "var",
                    _ => "var",
                };
                for declarator in decl.declarations.iter() {
                    if let BindingPatternKind::BindingIdentifier(ref id) = declarator.id.kind {
                        let name = format!("{} {}", keyword, id.name);
                        let start = offset_to_line(&line_offsets, decl.span.start);
                        let end = offset_to_line(&line_offsets, decl.span.end.saturating_sub(1));
                        results.push(JsDeclaration {
                            name,
                            decl_type: decl_type.clone(),
                            start_line: start,
                            end_line: end,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    results
}

/// Extract top-level JS declarations from script content lines (compatibility wrapper).
/// Returns: Vec<(name, declaration_type, 0-based line index within the provided lines)>
pub fn extract_js_declarations(lines: &[&str]) -> Vec<(String, JsDeclType, usize)> {
    extract_js_declarations_full(lines)
        .into_iter()
        .map(|d| (d.name, d.decl_type, d.start_line))
        .collect()
}

/// Detect the end of a JS scope starting from a given line using AST.
/// Returns the 0-based line index where the scope ends.
/// `lines` should be the full file lines, `start_line` is the 0-based line of the declaration.
/// `script_start`/`script_end` define the script region (0-based, exclusive end).
pub fn detect_scope_end_in_region(
    lines: &[&str],
    start_line: usize,
    script_start: usize,
    script_end: usize,
) -> Option<usize> {
    let region_lines = &lines[script_start..script_end];
    let decls = extract_js_declarations_full(region_lines);
    let rel_start = start_line.checked_sub(script_start)?;

    // Find the declaration at rel_start line
    decls
        .iter()
        .find(|d| d.start_line == rel_start)
        .map(|d| script_start + d.end_line)
}

/// Legacy detect_scope_end that searches across all script regions.
/// Kept for backward compatibility with locator.rs.
pub fn detect_scope_end(lines: &[&str], start_line: usize) -> Option<usize> {
    // Find which script region contains this line
    let regions = find_script_regions_simple(lines);
    for (region_start, region_end) in &regions {
        if start_line >= *region_start && start_line < *region_end {
            return detect_scope_end_in_region(lines, start_line, *region_start, *region_end);
        }
    }
    None
}

/// Simple script region finder (to avoid circular dependency with header.rs)
fn find_script_regions_simple(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut regions = Vec::new();
    let mut in_script = false;
    let mut script_start = 0;

    for (idx, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if !in_script && lower.contains("<script") && !lower.contains("src=") {
            in_script = true;
            script_start = idx + 1;
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

/// Extract purpose comment from the line above a declaration.
/// Matches pattern: `// name() — description` or `// name — description`
pub fn extract_purpose_comment(lines: &[&str], decl_line_idx: usize) -> Option<String> {
    if decl_line_idx == 0 {
        return None;
    }
    let prev = lines[decl_line_idx - 1].trim();
    if !prev.starts_with("//") {
        return None;
    }
    // Match: // ... — description (em dash)
    if let Some(pos) = prev.find(" — ") {
        return Some(prev[pos + " — ".len()..].trim().to_string());
    }
    // Match: // ... - description (after name())
    if let Some(pos) = prev.find("() - ") {
        return Some(prev[pos + "() - ".len()..].trim().to_string());
    }
    None
}
