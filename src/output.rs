use serde::Serialize;

/// Format output as either text or JSON based on the --json flag
pub fn format_output<T: Serialize>(value: &T, json: bool) -> anyhow::Result<String> {
    if json {
        Ok(serde_json::to_string_pretty(value)?)
    } else {
        // For text output, we rely on Display implementations
        Ok(serde_json::to_string_pretty(value)?)
    }
}

/// Print a text table with aligned columns
pub fn print_aligned_table(rows: &[(String, String)]) -> String {
    let max_left = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
    rows.iter()
        .map(|(left, right)| format!("{:<width$}  →  {}", left, right, width = max_left))
        .collect::<Vec<_>>()
        .join("\n")
}
