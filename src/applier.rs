use anyhow::{bail, Result};
use std::path::Path;
use tempfile::NamedTempFile;
use std::io::Write;

use crate::differ::{self, DiffLine};
use crate::history;
use crate::syntax_check::{self, CheckContext};
use crate::validator;

pub struct ApplyResult {
    pub hunks_applied: usize,
    pub lines_removed: usize,
    pub lines_added: usize,
    pub new_size: usize,
    pub hunk_details: Vec<HunkDetail>,
    pub validation: Option<ValidationResult>,
    pub history_id: Option<String>,
}

pub struct HunkDetail {
    pub hunk_index: usize,
    pub stated_line: usize,
    pub matched_line: usize,
    pub fuzz_offset: i32,
}

pub struct ValidationResult {
    pub status: ApplyStatus,
    pub syntax_ok: bool,
    pub anchor_ok: bool,
    pub tag_pair_ok: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApplyStatus {
    /// Edit succeeded with no issues
    Success,
    /// Edit succeeded but there are warnings/errors in the HTML
    SuccessWithIssues,
    /// Edit rolled back due to critical errors
    RolledBack,
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub severity: String,
    pub line: usize,
    pub message: String,
    pub locate_hint: Option<String>,
}

/// Apply a unified diff to a file
pub fn apply_diff(
    file_path: &Path,
    diff_text: &str,
    fuzz: usize,
    dry_run: bool,
    backup: bool,
    force: bool,
) -> Result<ApplyResult> {
    let content = std::fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    let hunks = differ::parse_unified_diff(diff_text)?;

    let mut total_removed = 0usize;
    let mut total_added = 0usize;
    let mut offset: i64 = 0;
    let mut hunk_details = Vec::new();

    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        // Calculate the expected position with accumulated offset
        let stated_line = hunk.old_start;
        let search_start = ((stated_line as i64 - 1 + offset) as isize).max(0) as usize;

        // Extract context lines from the hunk for matching
        let context_lines: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                DiffLine::Context(s) => Some(s.as_str()),
                DiffLine::Remove(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();

        // Try to find the context match with fuzz
        let matched_pos = find_context_match(&lines, &context_lines, search_start, fuzz)?;

        match matched_pos {
            Some(pos) => {
                let fuzz_offset = pos as i64 - search_start as i64;

                hunk_details.push(HunkDetail {
                    hunk_index: hunk_idx + 1,
                    stated_line,
                    matched_line: pos + 1,
                    fuzz_offset: fuzz_offset as i32,
                });

                // Apply the hunk
                let mut new_lines: Vec<String> = Vec::new();
                let mut removed = 0;
                let mut added = 0;

                // Collect lines before the hunk
                // (already handled by pos)

                let mut old_idx = pos;
                for diff_line in &hunk.lines {
                    match diff_line {
                        DiffLine::Context(_) => {
                            old_idx += 1;
                        }
                        DiffLine::Remove(_) => {
                            removed += 1;
                            old_idx += 1;
                        }
                        DiffLine::Add(s) => {
                            new_lines.push(s.clone());
                            added += 1;
                        }
                    }
                }

                // Apply: remove old lines and insert new ones
                let remove_start = pos;
                let remove_end = old_idx;

                // Build the kept lines + new lines
                let mut result: Vec<String> = Vec::new();
                result.extend_from_slice(&lines[..remove_start]);

                // Interleave: go through hunk lines in order
                let mut src_idx = remove_start;
                for diff_line in &hunk.lines {
                    match diff_line {
                        DiffLine::Context(_) => {
                            result.push(lines[src_idx].clone());
                            src_idx += 1;
                        }
                        DiffLine::Remove(_) => {
                            src_idx += 1;
                        }
                        DiffLine::Add(s) => {
                            result.push(s.clone());
                        }
                    }
                }

                result.extend_from_slice(&lines[remove_end..]);
                lines = result;

                offset += added as i64 - removed as i64;
                total_removed += removed;
                total_added += added;
            }
            None => {
                // Context mismatch error
                let actual_start = search_start.min(lines.len().saturating_sub(1));
                let actual_end = std::cmp::min(actual_start + context_lines.len(), lines.len());
                let actual: Vec<&str> = lines[actual_start..actual_end]
                    .iter()
                    .map(|s| s.as_str())
                    .collect();

                let mut error_msg = format!(
                    "Error: Hunk {} context mismatch at line {}.\n\n",
                    hunk_idx + 1,
                    stated_line
                );
                error_msg.push_str("  Expected (from diff):     Actual (in file):\n");
                error_msg.push_str("  ─────────────────────     ─────────────────\n");

                let max_lines = std::cmp::max(context_lines.len(), actual.len());
                for i in 0..std::cmp::min(max_lines, 5) {
                    let expected = context_lines.get(i).unwrap_or(&"");
                    let got = actual.get(i).map(|s| *s).unwrap_or("");
                    let exp_display = if expected.len() > 25 {
                        format!("{}...", &expected[..22])
                    } else {
                        expected.to_string()
                    };
                    let got_display = if got.len() > 25 {
                        format!("{}...", &got[..22])
                    } else {
                        got.to_string()
                    };
                    error_msg.push_str(&format!(
                        "  {:<28} {}\n",
                        exp_display, got_display
                    ));
                }

                error_msg.push_str(&format!(
                    "\n  The file may have been modified since the diff was generated.\n  Re-read the target region with `sfhtml read {} {} {}` and regenerate the diff.",
                    file_path.display(),
                    stated_line.saturating_sub(5),
                    stated_line + 10
                ));

                bail!("{}", error_msg);
            }
        }
    }

    let new_content = lines.join("\n");
    let new_size = new_content.len();

    // Post-apply validation — symbol balance check is default ON (unless --force)
    let validation = if !force {
        Some(run_post_apply_validation(&new_content))
    } else {
        None
    };

    // Check if validation has critical syntax errors → rollback
    if let Some(ref v) = validation {
        if !v.syntax_ok && v.status == ApplyStatus::RolledBack {
            if !dry_run {
                let error_summary: Vec<String> = v.errors.iter()
                    .map(|e| format!("  line {}: {}{}", e.line, e.message,
                        e.locate_hint.as_ref().map(|h| format!(" (locate: {})", h)).unwrap_or_default()))
                    .collect();
                bail!(
                    "⚠ ROLLBACK: The applied diff introduces syntax errors.\n{}\n  Fix the diff content and retry.\n  Use `--force` to skip validation and write anyway.",
                    error_summary.join("\n")
                );
            }
        }
    }

    let mut history_id = None;

    if !dry_run {
        // Backup if requested
        if backup {
            let backup_path = format!(
                "{}.bak.{}",
                file_path.display(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            );
            std::fs::copy(file_path, &backup_path)?;
        }

        // Atomic write: write to temp file, then rename
        let dir = file_path.parent().unwrap_or(Path::new("."));
        let mut tmp = NamedTempFile::new_in(dir)?;
        tmp.write_all(new_content.as_bytes())?;
        tmp.persist(file_path)?;

        // Save to diff history for rollback
        let entry = history::create_entry(
            file_path,
            &content,
            &new_content,
            diff_text,
            hunk_details.len(),
            total_added,
            total_removed,
        );
        history_id = Some(entry.id.clone());
        if let Err(e) = history::save_entry(&entry) {
            eprintln!("Warning: failed to save diff history: {}", e);
        }
    }

    Ok(ApplyResult {
        hunks_applied: hunk_details.len(),
        lines_removed: total_removed,
        lines_added: total_added,
        new_size,
        hunk_details,
        validation,
        history_id,
    })
}

fn find_context_match(
    lines: &[String],
    context: &[&str],
    expected_pos: usize,
    fuzz: usize,
) -> Result<Option<usize>> {
    if context.is_empty() {
        return Ok(Some(expected_pos.min(lines.len())));
    }

    // Try exact position first
    if matches_at(lines, context, expected_pos) {
        return Ok(Some(expected_pos));
    }

    // Try with fuzz (search ±fuzz lines)
    for delta in 1..=fuzz {
        if expected_pos + delta < lines.len() && matches_at(lines, context, expected_pos + delta) {
            return Ok(Some(expected_pos + delta));
        }
        if expected_pos >= delta && matches_at(lines, context, expected_pos - delta) {
            return Ok(Some(expected_pos - delta));
        }
    }

    Ok(None)
}

fn matches_at(lines: &[String], context: &[&str], pos: usize) -> bool {
    if pos + context.len() > lines.len() {
        return false;
    }
    for (i, ctx) in context.iter().enumerate() {
        if lines[pos + i] != *ctx {
            return false;
        }
    }
    true
}

fn run_post_apply_validation(content: &str) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Symbol balance check (default ON)
    let syntax_check_result = syntax_check::check_syntax(content, CheckContext::Html);
    let syntax_ok = syntax_check_result.balanced;

    for issue in &syntax_check_result.issues {
        let vi = ValidationIssue {
            severity: issue.severity.clone(),
            line: issue.line,
            message: issue.message.clone(),
            locate_hint: issue.context_snippet.clone(),
        };
        if issue.severity == "error" {
            errors.push(vi);
        } else {
            warnings.push(vi);
        }
    }

    // 2. Full validate (anchor consistency, tag-pair, id references)
    let mut anchor_ok = true;
    let mut tag_pair_ok = true;

    if let Ok(validate_result) = validator::validate_file(content, false) {
        // Anchor consistency issues
        for name in &validate_result.anchor_consistency.missing_from_code {
            anchor_ok = false;
            // Try to find section 5 line referencing this anchor
            let line = find_line_containing(content, name).unwrap_or(0);
            errors.push(ValidationIssue {
                severity: "error".to_string(),
                line,
                message: format!("Header anchor \"{}\" not found in code", name),
                locate_hint: Some(name.clone()),
            });
        }

        for anchor in &validate_result.anchor_consistency.missing_from_header {
            warnings.push(ValidationIssue {
                severity: "warning".to_string(),
                line: anchor.line,
                message: format!("Code declaration \"{}\" not listed in header", anchor.name),
                locate_hint: Some(anchor.name.clone()),
            });
        }

        for issue in &validate_result.anchor_consistency.api_issues {
            warnings.push(ValidationIssue {
                severity: "warning".to_string(),
                line: 0,
                message: issue.clone(),
                locate_hint: None,
            });
        }

        // Id reference issues
        for issue in &validate_result.syntax_validation.id_issues {
            tag_pair_ok = false;
            warnings.push(ValidationIssue {
                severity: "warning".to_string(),
                line: 0,
                message: issue.clone(),
                locate_hint: None,
            });
        }
    }

    let status = if !syntax_ok {
        ApplyStatus::RolledBack
    } else if !errors.is_empty() || !warnings.is_empty() {
        ApplyStatus::SuccessWithIssues
    } else {
        ApplyStatus::Success
    };

    ValidationResult {
        status,
        syntax_ok,
        anchor_ok,
        tag_pair_ok,
        errors,
        warnings,
    }
}

/// Find the first line number (1-based) containing the given text
fn find_line_containing(content: &str, needle: &str) -> Option<usize> {
    for (idx, line) in content.lines().enumerate() {
        if line.contains(needle) {
            return Some(idx + 1);
        }
    }
    None
}

/// Format the apply result for text output
pub fn format_apply_result(result: &ApplyResult, file_name: &str) -> String {
    let mut output = String::new();

    output.push_str(&format!(
        "Applied {} hunk{} to {}\n",
        result.hunks_applied,
        if result.hunks_applied != 1 { "s" } else { "" },
        file_name
    ));

    for detail in &result.hunk_details {
        let fuzz_str = if detail.fuzz_offset == 0 {
            "(exact)".to_string()
        } else {
            format!("(fuzz {:+})", detail.fuzz_offset)
        };
        output.push_str(&format!(
            "  Hunk {}: line {} → matched at {} {}\n",
            detail.hunk_index, detail.stated_line, detail.matched_line, fuzz_str
        ));
    }

    output.push_str(&format!(
        "  Lines removed: {}, lines added: {}, new size: {} bytes\n",
        result.lines_removed, result.lines_added, result.new_size
    ));

    if let Some(ref id) = result.history_id {
        output.push_str(&format!("  History saved: {} (use `sfhtml history rollback` to undo)\n", id));
    }

    if let Some(ref v) = result.validation {
        output.push_str("\n=== Post-Apply Validation ===\n");

        match v.status {
            ApplyStatus::Success => {
                output.push_str("✓ Edit success — no issues detected.\n");
            }
            ApplyStatus::SuccessWithIssues => {
                output.push_str("✓ Edit success — but issues detected:\n");
            }
            ApplyStatus::RolledBack => {
                output.push_str("✗ Edit rolled back — critical syntax errors:\n");
            }
        }

        output.push_str(&format!(
            "  [1/3] Symbol balance...                 {}\n",
            if v.syntax_ok { "✓" } else { "✗ FAIL" }
        ));
        output.push_str(&format!(
            "  [2/3] Anchor consistency...              {}\n",
            if v.anchor_ok { "✓" } else { "⚠" }
        ));
        output.push_str(&format!(
            "  [3/3] Tag-pair / id references...        {}\n",
            if v.tag_pair_ok { "✓" } else { "⚠" }
        ));

        for e in &v.errors {
            let hint = e.locate_hint.as_ref()
                .map(|h| format!(" → locate \"{}\"" , h))
                .unwrap_or_default();
            output.push_str(&format!(
                "  ✗ [error] line {}: {}{}\n",
                e.line, e.message, hint
            ));
        }

        for w in &v.warnings {
            let hint = w.locate_hint.as_ref()
                .map(|h| format!(" → locate \"{}\"" , h))
                .unwrap_or_default();
            output.push_str(&format!(
                "  ⚠ [warning] line {}: {}{}\n",
                w.line, w.message, hint
            ));
        }
    }

    output
}
