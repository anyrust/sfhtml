use anyhow::{bail, Result};
use std::path::Path;

/// Read a specific line range from a file
pub fn read_lines(path: &Path, start: Option<usize>, end: Option<usize>, head: Option<usize>, tail: Option<usize>) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if total == 0 {
        return Ok(String::new());
    }

    let (from, to) = if let Some(h) = head {
        (1, std::cmp::min(h, total))
    } else if let Some(t) = tail {
        (total.saturating_sub(t) + 1, total)
    } else if let (Some(s), Some(e)) = (start, end) {
        if s == 0 || e == 0 {
            bail!("Line numbers are 1-based. Use 1 for the first line.");
        }
        if s > total {
            bail!("Start line {} exceeds file length ({} lines)", s, total);
        }
        (s, std::cmp::min(e, total))
    } else if let Some(s) = start {
        if s == 0 {
            bail!("Line numbers are 1-based.");
        }
        (s, total)
    } else {
        (1, total)
    };

    let mut output = String::new();
    for i in (from - 1)..to {
        let line_num = i + 1;
        output.push_str(&format!("{:>6}│{}\n", line_num, lines[i]));
    }

    Ok(output)
}
