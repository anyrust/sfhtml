use similar::TextDiff;

/// Generate a unified diff between two texts
pub fn generate_diff(old_text: &str, new_text: &str, old_name: &str, new_name: &str, context_lines: usize) -> String {
    let diff = TextDiff::from_lines(old_text, new_text);

    diff.unified_diff()
        .context_radius(context_lines)
        .header(&format!("a/{}", old_name), &format!("b/{}", new_name))
        .to_string()
}

/// Parse a unified diff string into hunks
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Remove(String),
    Add(String),
}

pub fn parse_unified_diff(diff_text: &str) -> anyhow::Result<Vec<DiffHunk>> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;
    let mut parsing_started = false;

    for line in diff_text.lines() {
        // Skip --- and +++ lines
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            parsing_started = true;
            continue;
        }

        // Parse hunk header
        if line.starts_with("@@ ") {
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }

            let hunk = parse_hunk_header(line)?;
            current_hunk = Some(hunk);
            continue;
        }

        if let Some(ref mut hunk) = current_hunk {
            if line.starts_with('-') {
                hunk.lines.push(DiffLine::Remove(line[1..].to_string()));
            } else if line.starts_with('+') {
                hunk.lines.push(DiffLine::Add(line[1..].to_string()));
            } else if line.starts_with(' ') {
                hunk.lines.push(DiffLine::Context(line[1..].to_string()));
            } else if line.is_empty() && parsing_started {
                hunk.lines.push(DiffLine::Context(String::new()));
            }
        }
    }

    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    if hunks.is_empty() {
        anyhow::bail!("Error: Invalid unified diff format — no hunks found");
    }

    Ok(hunks)
}

fn parse_hunk_header(line: &str) -> anyhow::Result<DiffHunk> {
    // Parse "@@ -old_start,old_count +new_start,new_count @@"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 4 || parts[0] != "@@" {
        anyhow::bail!("Error: Invalid hunk header: {}", line);
    }

    let old_part = parts[1].trim_start_matches('-');
    let new_part = parts[2].trim_start_matches('+');

    let (old_start, old_count) = parse_range(old_part)?;
    let (new_start, new_count) = parse_range(new_part)?;

    Ok(DiffHunk {
        old_start,
        old_count,
        new_start,
        new_count,
        lines: Vec::new(),
    })
}

fn parse_range(s: &str) -> anyhow::Result<(usize, usize)> {
    if let Some(comma) = s.find(',') {
        let start: usize = s[..comma].parse()?;
        let count: usize = s[comma + 1..].parse()?;
        Ok((start, count))
    } else {
        let start: usize = s.parse()?;
        Ok((start, 1))
    }
}
