use serde::Serialize;

/// An HTML element with an id attribute
#[derive(Debug, Serialize, Clone)]
pub struct HtmlElement {
    pub tag: String,
    pub id: Option<String>,
    pub line: usize, // 1-based line number
    pub depth: usize,
}

/// Extract all elements with id attributes from HTML content
pub fn extract_ids(content: &str) -> Vec<HtmlElement> {
    let mut results = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let mut pos = 0;
        let bytes = line.as_bytes();

        while pos < bytes.len() {
            if bytes[pos] == b'<' && pos + 1 < bytes.len() && bytes[pos + 1] != b'/' && bytes[pos + 1] != b'!' {
                // Extract tag name
                let tag_start = pos + 1;
                let mut end = tag_start;
                while end < bytes.len() && bytes[end] != b' ' && bytes[end] != b'>' && bytes[end] != b'/' {
                    end += 1;
                }
                let tag_name = line[tag_start..end].to_lowercase();

                if tag_name.is_empty() {
                    pos += 1;
                    continue;
                }

                // Look for id attribute in this tag
                let tag_end = line[pos..].find('>').map(|i| pos + i).unwrap_or(bytes.len());
                let tag_content = &line[pos..tag_end];

                if let Some(id) = extract_id_attr(tag_content) {
                    results.push(HtmlElement {
                        tag: tag_name,
                        id: Some(id),
                        line: line_idx + 1,
                        depth: 0, // Will be computed later if needed
                    });
                }

                pos = tag_end;
            }
            pos += 1;
        }
    }

    results
}

fn extract_id_attr(tag_content: &str) -> Option<String> {
    // Look for id="..." or id='...'
    let patterns = ["id=\"", "id='", "id = \"", "id = '"];
    for pattern in &patterns {
        if let Some(start) = tag_content.find(pattern) {
            let val_start = start + pattern.len();
            let quote = if pattern.ends_with('"') { '"' } else { '\'' };
            if let Some(val_end) = tag_content[val_start..].find(quote) {
                return Some(tag_content[val_start..val_start + val_end].to_string());
            }
        }
    }
    None
}

/// Check if a given id exists in the HTML content
pub fn id_exists(content: &str, id: &str) -> bool {
    let patterns = [
        format!("id=\"{}\"", id),
        format!("id='{}'", id),
    ];
    patterns.iter().any(|p| content.contains(p.as_str()))
}

/// Build a simple tag-pair tree structure for Section 6
pub fn build_tag_pair_tree(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut stack: Vec<(String, Option<String>, usize)> = Vec::new(); // (tag, id, depth)
    let lines: Vec<&str> = content.lines().collect();

    let void_elements = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];

    for line in &lines {
        let mut pos = 0;
        let bytes = line.as_bytes();

        while pos < bytes.len() {
            if bytes[pos] != b'<' {
                pos += 1;
                continue;
            }

            // Skip comments
            if line[pos..].starts_with("<!--") {
                if let Some(end) = line[pos..].find("-->") {
                    pos += end + 3;
                    continue;
                }
                break;
            }

            let is_closing = pos + 1 < bytes.len() && bytes[pos + 1] == b'/';
            let tag_start = if is_closing { pos + 2 } else { pos + 1 };
            let mut end = tag_start;
            while end < bytes.len() && bytes[end] != b' ' && bytes[end] != b'>' && bytes[end] != b'/' {
                end += 1;
            }
            let tag_name = line[tag_start..end].to_lowercase();

            if tag_name.is_empty() || tag_name.starts_with('!') {
                pos = end;
                continue;
            }

            let tag_end = line[pos..].find('>').map(|i| pos + i).unwrap_or(bytes.len());
            let tag_content = &line[pos..tag_end];
            let is_self_closing = tag_end > 0 && pos < tag_end && line.as_bytes()[tag_end.saturating_sub(1)] == b'/';

            if void_elements.contains(&tag_name.as_str()) || is_self_closing {
                pos = tag_end + 1;
                continue;
            }

            if is_closing {
                // Pop from stack
                if let Some(idx) = stack.iter().rposition(|(name, _, _)| *name == tag_name) {
                    let depth = stack[idx].2;
                    let indent = "  ".repeat(depth);
                    let id_str = stack[idx]
                        .1
                        .as_ref()
                        .map(|id| format!(" id=\"{}\"", id))
                        .unwrap_or_default();
                    result.push(format!("{}end: </{}{}>", indent, tag_name, id_str));
                    stack.truncate(idx);
                }
            } else {
                let id = extract_id_attr(tag_content);
                let depth = stack.len();
                let indent = "  ".repeat(depth);
                let id_str = id
                    .as_ref()
                    .map(|id| format!(" id=\"{}\"", id))
                    .unwrap_or_default();
                result.push(format!("{}start: <{}{}>", indent, tag_name, id_str));
                stack.push((tag_name, id, depth));
            }

            pos = tag_end + 1;
        }
    }

    result
}
