mod applier;
mod browser;
mod creator;
mod differ;
mod header;
mod history;
mod js_scope;
mod live;
mod locator;
mod module_deps;
mod page;
mod reader;
mod scanner;
mod search;
mod stats;
mod syntax_check;
mod validator;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;
use std::process;

#[derive(Parser)]
#[command(name = "sfhtml", version, about = "Single-File HTML AI-Skill CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Maximum execution time in milliseconds (0 = no timeout)
    #[arg(long, global = true)]
    timeout: Option<u64>,

    /// Output structured JSON instead of human-readable text
    #[arg(long, global = true, default_value_t = false)]
    json: bool,

    /// Append machine-readable diagnostic block to stderr
    #[arg(long, global = true, default_value_t = false)]
    diagnostic: bool,

    /// Step-by-step execution log to stderr
    #[arg(long, global = true, default_value_t = false)]
    trace: bool,

    /// Show only the first N lines of output
    #[arg(long, global = true)]
    head: Option<usize>,

    /// Show only the last N lines of output
    #[arg(long, global = true)]
    tail: Option<usize>,

    /// Filter output lines matching a pattern
    #[arg(long, global = true)]
    grep: Option<String>,

    /// Print the number of output lines instead of content
    #[arg(long, global = true, default_value_t = false)]
    count: bool,

    /// Truncate output to at most N bytes
    #[arg(long, global = true)]
    truncate: Option<usize>,
}

#[derive(Subcommand)]
enum Commands {
    /// Fast-scan a directory for HTML files with AI-SKILL-HEADERs
    Scan {
        /// Directory to scan
        dir: PathBuf,
        /// Scan recursively
        #[arg(long, default_value_t = false)]
        recursive: bool,
        /// Return only top N HTML results (0 = all)
        #[arg(long, default_value_t = 0)]
        top: usize,
        /// Sort by: modified (default), created, name, size, relevance
        #[arg(long, default_value = "modified")]
        sort_by: String,
        /// Sort order: desc (default), asc
        #[arg(long, default_value = "desc")]
        order: String,
        /// Filter: only show entries whose path contains ALL given keywords
        #[arg(long, value_delimiter = ',')]
        r#match: Vec<String>,
        /// Expand results: inline full header for each matched file (enables 3-step workflow)
        #[arg(long, default_value_t = false)]
        expand: bool,
    },

    /// Search HTML files by query with TF-based scoring
    Search {
        /// Search query
        query: String,
        /// Directory to search (default: current dir)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Return only top N results
        #[arg(long, default_value_t = 5)]
        top: usize,
        /// Lines of context around matches
        #[arg(long, default_value_t = 0)]
        context: usize,
    },

    /// Extract the AI-SKILL-HEADER from a file
    Header {
        /// HTML file path
        file: PathBuf,
        /// Extract only a specific section number
        #[arg(long)]
        section: Option<usize>,
    },

    /// Locate a code anchor in the file
    Locate {
        /// HTML file path
        file: PathBuf,
        /// Anchor text to locate
        anchor: String,
        /// Context lines around the match
        #[arg(long, default_value_t = 0)]
        context: usize,
    },

    /// Read a line range from a file
    Read {
        /// HTML file path
        file: PathBuf,
        /// Start line (1-based)
        start_line: Option<usize>,
        /// End line (1-based)
        end_line: Option<usize>,
        /// Read first N lines
        #[arg(long)]
        head: Option<usize>,
        /// Read last N lines
        #[arg(long)]
        tail: Option<usize>,
    },

    /// Apply a unified diff to a file
    Apply {
        /// HTML file path
        file: PathBuf,
        /// Diff file path (or - for stdin)
        #[arg(long)]
        diff: String,
        /// Show what would change without writing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Create backup before writing
        #[arg(long, default_value_t = false)]
        backup: bool,
        /// Allow context to match within ±N lines
        #[arg(long, default_value_t = 2)]
        fuzz: usize,
        /// Skip post-apply validation
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Generate a unified diff between two files
    Diff {
        /// First (old) file
        file: PathBuf,
        /// Second (new) file
        old_file: PathBuf,
        /// Context lines around changes
        #[arg(long, default_value_t = 3)]
        context: usize,
    },

    /// List all locatable anchors in the file
    AnchorList {
        /// HTML file path
        file: PathBuf,
        /// Return only top N results (0 = all)
        #[arg(long, default_value_t = 0)]
        top: usize,
    },

    /// Validate header-to-code consistency
    Validate {
        /// HTML file path
        file: PathBuf,
        /// Also check bracket/quote pair syntax
        #[arg(long, default_value_t = true)]
        syntax: bool,
        /// Auto-fix by running header-rebuild
        #[arg(long, default_value_t = false)]
        fix: bool,
    },

    /// Rebuild header Section 5 from code
    HeaderRebuild {
        /// HTML file path
        file: PathBuf,
        /// Show what would be generated without writing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Preserve AI-written semantic descriptions
        #[arg(long, default_value_t = false)]
        preserve_descriptions: bool,
    },

    /// Inject an initial AI-SKILL-HEADER into an HTML file
    Init {
        /// HTML file path
        file: PathBuf,
    },

    /// Create a new HTML file
    Create {
        /// Output file path
        path: PathBuf,
        /// Document title
        #[arg(long, default_value = "New App")]
        title: String,
        /// Include an AI-SKILL-HEADER template
        #[arg(long, default_value_t = false)]
        with_header: bool,
        /// Overwrite if file already exists
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Save a file to a new location (optionally inject header)
    SaveAs {
        /// Source HTML file
        source: PathBuf,
        /// Destination file path
        dest: PathBuf,
        /// Inject AI-SKILL-HEADER if not present
        #[arg(long, default_value_t = false)]
        inject_header: bool,
        /// Overwrite if destination exists
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Manage diff history cache (list, show, rollback, delete, clean)
    History {
        #[command(subcommand)]
        action: HistoryAction,
    },

    /// Scan local ES module / resource dependencies
    Module {
        /// HTML file path
        file: PathBuf,
        /// Recursively scan dependencies up to N levels deep (0 = direct only)
        #[arg(long, default_value_t = 0)]
        depth: usize,
        /// Return only top N results (0 = all)
        #[arg(long, default_value_t = 0)]
        top: usize,
    },

    /// Check symbol balance of text input
    CheckOutput {
        /// File to check (omit for stdin)
        file: Option<PathBuf>,
        /// Check mode: cli, header, js, html
        #[arg(long, default_value = "cli")]
        mode: String,
    },

    /// Show file metadata, usage stats, and structure summary
    Stat {
        /// HTML file path
        file: PathBuf,
    },

    /// Serve an HTML file with live reload (file watch + WebSocket push)
    Serve {
        /// HTML file to serve
        file: PathBuf,
        /// HTTP port
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Open browser automatically
        #[arg(long, default_value_t = false)]
        open: bool,
        /// Inject live-reload client script (enabled by default)
        #[arg(long, default_value_t = true)]
        live: bool,
    },

    /// Launch/manage a browser with CDP debugging
    Debug {
        #[command(subcommand)]
        action: DebugAction,
    },

    /// Interact with a browser page via CDP
    Page {
        #[command(subcommand)]
        action: PageAction,
    },
}

#[derive(Subcommand)]
enum DebugAction {
    /// Start a browser with CDP debugging enabled
    Start {
        /// HTML file to open
        file: PathBuf,
        /// CDP debugging port
        #[arg(long, default_value_t = 9222)]
        port: u16,
        /// Show the browser window (default: headless)
        #[arg(long, default_value_t = false)]
        no_headless: bool,
    },
    /// Stop a running browser session
    Stop {
        /// CDP port of the session to stop
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// List active browser sessions
    List,
}

#[derive(Subcommand)]
enum PageAction {
    /// Connect to an existing CDP browser (verify connection)
    Open {
        /// CDP port to connect to
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Capture a screenshot (PNG)
    Screenshot {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
        /// CSS selector to capture (default: full page)
        #[arg(long)]
        selector: Option<String>,
        /// Save to file instead of returning base64
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Get page DOM (HTML)
    Dom {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
        /// CSS selector for subtree (default: full document)
        #[arg(long)]
        selector: Option<String>,
    },
    /// Get console log messages
    Console {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Get network request events
    Network {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
        /// How long to listen for events (ms)
        #[arg(long, default_value_t = 2000)]
        wait: u64,
    },
    /// Click an element
    Click {
        /// CSS selector to click
        selector: String,
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Type text into an element
    Type {
        /// CSS selector of input element
        selector: String,
        /// Text to type
        text: String,
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Scroll the page
    Scroll {
        /// Horizontal scroll amount (pixels)
        #[arg(long, default_value_t = 0.0)]
        x: f64,
        /// Vertical scroll amount (pixels)
        #[arg(long, default_value_t = 0.0)]
        y: f64,
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Simulate a touch event
    Touch {
        /// X coordinate
        x: f64,
        /// Y coordinate
        y: f64,
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Evaluate JavaScript expression
    Eval {
        /// JavaScript expression to evaluate
        expression: String,
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
    /// Export page as PDF
    Pdf {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
        /// Save to file instead of returning base64
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Disconnect from the browser (doesn't stop it)
    Close {
        /// CDP port
        #[arg(long, default_value_t = 9222)]
        port: u16,
    },
}

#[derive(Subcommand)]
enum HistoryAction {
    /// List saved diff history entries
    List {
        /// Filter by file path substring
        #[arg(long)]
        file: Option<String>,
        /// Return only top N entries (0 = all)
        #[arg(long, default_value_t = 0)]
        top: usize,
    },
    /// Show a specific history entry (diff content)
    Show {
        /// History entry ID
        id: String,
    },
    /// Rollback a file using a saved diff
    Rollback {
        /// File to rollback
        file: PathBuf,
        /// History entry ID
        id: String,
        /// Show what would change without writing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Context match fuzz tolerance
        #[arg(long, default_value_t = 3)]
        fuzz: usize,
    },
    /// Delete a specific history entry
    Delete {
        /// History entry ID
        id: String,
    },
    /// Show cache size info
    Status,
    /// Remove all cached history entries
    Clean,
}

fn main() {
    let cli = Cli::parse();

    let result = run(cli);

    match result {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

fn run(cli: Cli) -> Result<i32> {
    let json = cli.json;
    let _trace = cli.trace;

    // Compute deadline from --timeout
    let deadline = cli.timeout.and_then(|ms| {
        if ms == 0 { None } else { Some(std::time::Instant::now() + std::time::Duration::from_millis(ms)) }
    });

    // Capture output controls
    let oc_head = cli.head;
    let oc_tail = cli.tail;
    let oc_grep = cli.grep.clone();
    let oc_count = cli.count;
    let oc_truncate = cli.truncate;

    let (output_text, exit_code) = match cli.command {
        Commands::Scan { dir, recursive, top, sort_by, order, r#match, expand } => {
            let sort_key = scanner::SortKey::from_str(&sort_by);
            let sort_order = scanner::SortOrder::from_str(&order);
            let mut result = scanner::scan_directory(&dir, recursive, sort_key, sort_order, &r#match, deadline)?;
            // Inject usage stats into scan results
            let stats_data = stats::load();
            scanner::inject_stats(&mut result.html_files, &stats_data);
            // Re-sort if relevance was requested (needs calls data)
            if matches!(sort_key, scanner::SortKey::Relevance) {
                result.html_files.sort_by(|a, b| {
                    let ra = stats::relevance(a.calls, a.modified_ts);
                    let rb = stats::relevance(b.calls, b.modified_ts);
                    rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            let text = if json {
                if expand {
                    // In JSON mode with expand, inject header_text into each result
                    let mut json_val = serde_json::to_value(&result)?;
                    if let Some(files) = json_val.get_mut("html_files").and_then(|v| v.as_array_mut()) {
                        for (i, r) in result.html_files.iter().enumerate() {
                            if r.has_header {
                                let full_path = dir.join(&r.path);
                                if let Ok(content) = std::fs::read_to_string(&full_path) {
                                    if let Ok(h) = header::extract_header(&content) {
                                        files[i]["header"] = serde_json::Value::String(h.full_markdown);
                                    }
                                }
                            }
                        }
                    }
                    serde_json::to_string_pretty(&json_val)?
                } else {
                    serde_json::to_string_pretty(&result)?
                }
            } else {
                let mut t = scanner::format_text(&result, top);
                if expand {
                    let display_count = if top > 0 && top < result.html_files.len() { top } else { result.html_files.len() };
                    for r in result.html_files.iter().take(display_count) {
                        if r.has_header {
                            let full_path = dir.join(&r.path);
                            if let Ok(content) = std::fs::read_to_string(&full_path) {
                                if let Ok(h) = header::extract_header(&content) {
                                    t.push_str(&format!("\n━━ {} ━━\n{}\n", r.path, h.full_markdown));
                                }
                            }
                        }
                    }
                }
                if result.timed_out {
                    t.push_str(&scanner::format_timeout_summary(&result));
                }
                t
            };
            (text, 0)
        }

        Commands::Search { query, dir, top, context } => {
            let results = search::search_files(&dir, &query, top, context)?;
            (serde_json::to_string_pretty(&results)?, 0)
        }

        Commands::Header { file, section } => {
            stats::increment(&file);
            let file_size = std::fs::metadata(&file)?.len();
            const HEADER_SIZE_LIMIT: u64 = 50 * 1024; // 50KB
            if file_size > HEADER_SIZE_LIMIT {
                let content = std::fs::read_to_string(&file)?;
                let h = header::extract_header(&content)?;
                let header_text = if let Some(section_num) = section {
                    let s = header::extract_section(&content, section_num)?;
                    if json {
                        serde_json::to_string_pretty(&s)?
                    } else {
                        format!("## {}. {}\n{}", s.number, s.title, s.content)
                    }
                } else if json {
                    serde_json::to_string_pretty(&h)?
                } else {
                    h.full_markdown.clone()
                };
                eprintln!("\n⚠ File size ({:.1} KB) exceeds 50 KB limit. Use `sfhtml read {} --head N` or `sfhtml locate {} <anchor>` to inspect code sections.",
                    file_size as f64 / 1024.0, file.display(), file.display());
                (header_text, 0)
            } else {
                let content = std::fs::read_to_string(&file)?;
                if let Some(section_num) = section {
                    let s = header::extract_section(&content, section_num)?;
                    let text = if json {
                        serde_json::to_string_pretty(&s)?
                    } else {
                        format!("## {}. {}\n{}", s.number, s.title, s.content)
                    };
                    (text, 0)
                } else {
                    let h = header::extract_header(&content)?;
                    let text = if json {
                        serde_json::to_string_pretty(&h)?
                    } else {
                        h.full_markdown.clone()
                    };
                    (text, 0)
                }
            }
        }

        Commands::Locate { file, anchor, context } => {
            stats::increment(&file);
            let content = std::fs::read_to_string(&file)?;
            let result = locator::locate_anchor(&content, &anchor, context)?;
            let text = if json {
                serde_json::to_string_pretty(&result)?
            } else {
                let mut out = String::new();
                for m in &result.matches {
                    let end_str = m.end_line
                        .map(|e| format!("-{}", e))
                        .unwrap_or_default();
                    out.push_str(&format!("Anchor \"{}\" found at line {}{}:\n", result.anchor, m.line, end_str));
                    out.push_str(&m.context_preview);
                    out.push('\n');
                }
                out
            };
            (text, 0)
        }

        Commands::Read { file, start_line, end_line, head, tail } => {
            stats::increment(&file);
            let output = reader::read_lines(&file, start_line, end_line, head, tail)?;
            (output, 0)
        }

        Commands::Apply { file, diff, dry_run, backup, fuzz, force } => {
            stats::increment(&file);
            let diff_text = if diff == "-" {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                std::fs::read_to_string(&diff)?
            };

            let result = applier::apply_diff(&file, &diff_text, fuzz, dry_run, backup, force)?;
            let file_name = file.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file");

            let text = if json {
                let validation_json = result.validation.as_ref().map(|v| {
                    let status_str = match v.status {
                        applier::ApplyStatus::Success => "success",
                        applier::ApplyStatus::SuccessWithWarnings => "success_with_warnings",
                    };
                    let warnings: Vec<serde_json::Value> = v.warnings.iter().map(|w| serde_json::json!({
                        "severity": w.severity,
                        "line": w.line,
                        "message": w.message,
                        "locate_hint": w.locate_hint,
                    })).collect();
                    serde_json::json!({
                        "status": status_str,
                        "warnings": warnings,
                    })
                });
                let hunk_details_json: Vec<serde_json::Value> = result.hunk_details.iter().map(|d| serde_json::json!({
                    "hunk_index": d.hunk_index,
                    "stated_line": d.stated_line,
                    "matched_line": d.matched_line,
                    "fuzz_offset": d.fuzz_offset,
                    "context_search": d.context_search,
                })).collect();
                let json_result = serde_json::json!({
                    "hunks_applied": result.hunks_applied,
                    "lines_removed": result.lines_removed,
                    "lines_added": result.lines_added,
                    "new_size_bytes": result.new_size,
                    "dry_run": dry_run,
                    "history_id": result.history_id,
                    "hunk_details": hunk_details_json,
                    "validation": validation_json,
                });
                serde_json::to_string_pretty(&json_result)?
            } else {
                applier::format_apply_result(&result, file_name)
            };
            (text, 0)
        }

        Commands::Diff { file, old_file, context } => {
            stats::increment(&file);
            stats::increment(&old_file);
            let new_text = std::fs::read_to_string(&file)?;
            let old_text = std::fs::read_to_string(&old_file)?;
            let new_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("new");
            let old_name = old_file.file_name().and_then(|n| n.to_str()).unwrap_or("old");
            let diff_output = differ::generate_diff(&old_text, &new_text, old_name, new_name, context);
            (diff_output, 0)
        }

        Commands::AnchorList { file, top } => {
            stats::increment(&file);
            let content = std::fs::read_to_string(&file)?;
            let anchors = locator::list_anchors(&content);
            let display: &[locator::AnchorListEntry] = if top > 0 { &anchors[..std::cmp::min(top, anchors.len())] } else { &anchors };
            let text = if json {
                serde_json::to_string_pretty(&display)?
            } else {
                let mut out = String::new();
                for a in display {
                    let header_mark = if a.in_header { "" } else { " [not in header]" };
                    out.push_str(&format!("{:<40} line {:>6}  {}{}\n", a.name, a.line, a.anchor_type, header_mark));
                }
                if top > 0 && anchors.len() > top {
                    out.push_str(&format!("\n... and {} more (use --top 0 to show all)\n", anchors.len() - top));
                }
                out
            };
            (text, 0)
        }

        Commands::Validate { file, syntax, fix } => {
            stats::increment(&file);
            let content = std::fs::read_to_string(&file)?;

            let text = if fix {
                let new_content = header::rebuild_header(&content, true)?;
                std::fs::write(&file, &new_content)?;
                let result = validator::validate_file(&new_content, syntax)?;
                let mut out = String::from("Header rebuilt. Re-validating...\n\n");
                if json {
                    out.push_str(&serde_json::to_string_pretty(&result)?);
                } else {
                    out.push_str(&validator::format_text(&result));
                }
                out
            } else {
                let result = validator::validate_file(&content, syntax)?;
                if json {
                    serde_json::to_string_pretty(&result)?
                } else {
                    validator::format_text(&result)
                }
            };
            (text, 0)
        }

        Commands::HeaderRebuild { file, dry_run, preserve_descriptions } => {
            stats::increment(&file);
            let content = std::fs::read_to_string(&file)?;
            let new_content = header::rebuild_header(&content, preserve_descriptions)?;

            if dry_run {
                (new_content, 0)
            } else {
                std::fs::write(&file, &new_content)?;
                (String::from("Header rebuilt successfully."), 0)
            }
        }

        Commands::Init { file } => {
            stats::increment(&file);
            let content = std::fs::read_to_string(&file)?;

            if content.contains("<!-- AI-SKILL-HEADER START") {
                eprintln!("Error: File already has an AI-SKILL-HEADER.");
                return Ok(1);
            }

            let new_content = header::generate_init_header(&content)?;
            std::fs::write(&file, &new_content)?;
            (format!("AI-SKILL-HEADER injected into {}", file.display()), 0)
        }

        Commands::Create { path, title, with_header, force } => {
            creator::create_html(&path, &title, with_header, force)?;
            let text = if json {
                serde_json::json!({
                    "created": path.display().to_string(),
                    "with_header": with_header,
                }).to_string()
            } else {
                format!("Created {}{}", path.display(),
                    if with_header { " (with AI-SKILL-HEADER)" } else { "" })
            };
            (text, 0)
        }

        Commands::SaveAs { source, dest, inject_header, force } => {
            creator::save_as(&source, &dest, inject_header, force)?;
            let text = if json {
                serde_json::json!({
                    "source": source.display().to_string(),
                    "dest": dest.display().to_string(),
                    "header_injected": inject_header && !std::fs::read_to_string(&dest)
                        .map(|c| c.contains("<!-- AI-SKILL-HEADER START"))
                        .unwrap_or(false),
                }).to_string()
            } else {
                format!("Saved {} → {}{}", source.display(), dest.display(),
                    if inject_header { " (header injected)" } else { "" })
            };
            (text, 0)
        }

        Commands::History { action } => {
            match action {
                HistoryAction::List { file, top } => {
                    let entries = history::list_entries(file.as_deref())?;
                    let display = if top > 0 { &entries[..std::cmp::min(top, entries.len())] } else { &entries[..] };
                    let text = if json {
                        serde_json::to_string_pretty(&display)?
                    } else if entries.is_empty() {
                        String::from("No history entries found.")
                    } else {
                        let cache_size = history::cache_size()?;
                        let mut out = format!("Diff history ({} entries, {:.1} KB / 10240 KB):\n\n",
                            entries.len(), cache_size as f64 / 1024.0);
                        for e in display {
                            out.push_str(&format!("  {} | {} | {} | +{} -{} | {}\n",
                                e.id, e.timestamp_human, e.file_path,
                                e.lines_added, e.lines_removed, e.description));
                        }
                        if top > 0 && entries.len() > top {
                            out.push_str(&format!("\n... and {} more (use --top 0 to show all)\n", entries.len() - top));
                        }
                        out
                    };
                    (text, 0)
                }

                HistoryAction::Show { id } => {
                    let entry = history::show_entry(&id)?;
                    let text = if json {
                        serde_json::to_string_pretty(&entry)?
                    } else {
                        format!("ID:        {}\nFile:      {}\nTime:      {}\nChanges:   +{} -{} ({} hunks)\n\n--- Forward Diff ---\n{}\n\n--- Reverse Diff (for rollback) ---\n{}",
                            entry.id, entry.file_path, entry.timestamp_human,
                            entry.lines_added, entry.lines_removed, entry.hunks_applied,
                            entry.diff_text, entry.reverse_diff)
                    };
                    (text, 0)
                }

                HistoryAction::Rollback { file, id, dry_run, fuzz } => {
                    let result = history::rollback(&file, &id, fuzz, dry_run)?;
                    let text = if json {
                        serde_json::json!({
                            "rollback": !dry_run,
                            "id": id,
                            "message": result,
                        }).to_string()
                    } else if dry_run {
                        format!("--- Dry run: reverse diff to apply ---\n{}", result)
                    } else {
                        result
                    };
                    (text, 0)
                }

                HistoryAction::Delete { id } => {
                    let freed = history::delete_entry(&id)?;
                    let text = if json {
                        serde_json::json!({
                            "deleted": id,
                            "freed_bytes": freed,
                        }).to_string()
                    } else {
                        format!("Deleted history entry: {} (freed {} bytes)", id, freed)
                    };
                    (text, 0)
                }

                HistoryAction::Status => {
                    let size = history::cache_size()?;
                    let entries = history::list_entries(None)?;
                    let dir = history::cache_dir()?;
                    let text = if json {
                        serde_json::json!({
                            "cache_dir": dir.display().to_string(),
                            "entries": entries.len(),
                            "size_bytes": size,
                            "limit_bytes": 10 * 1024 * 1024,
                            "usage_percent": (size as f64 / (10.0 * 1024.0 * 1024.0) * 100.0),
                        }).to_string()
                    } else {
                        format!("Cache dir:  {}\nEntries:    {}\nSize:       {:.1} KB / 10240 KB ({:.1}%)",
                            dir.display(), entries.len(),
                            size as f64 / 1024.0,
                            size as f64 / (10.0 * 1024.0 * 1024.0) * 100.0)
                    };
                    (text, 0)
                }

                HistoryAction::Clean => {
                    let (removed, freed) = history::clean_cache()?;
                    let text = if json {
                        serde_json::json!({
                            "removed": removed,
                            "freed_bytes": freed,
                        }).to_string()
                    } else {
                        format!("Cleaned {} entries, freed {:.1} KB", removed, freed as f64 / 1024.0)
                    };
                    (text, 0)
                }
            }
        }

        Commands::Module { file, depth, top } => {
            stats::increment(&file);
            let result = if depth > 0 {
                module_deps::scan_deps_recursive(&file, depth)?
            } else {
                module_deps::scan_deps(&file)?
            };
            let text = if json {
                serde_json::to_string_pretty(&result)?
            } else {
                module_deps::format_text(&result, top)
            };
            let code = if result.missing > 0 { 1 } else { 0 };
            (text, code)
        }

        Commands::CheckOutput { file, mode } => {
            let text = if let Some(path) = file {
                stats::increment(&path);
                std::fs::read_to_string(&path)?
            } else {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };

            let ctx = syntax_check::CheckContext::from_str(&mode);
            let result = syntax_check::check_syntax(&text, ctx);
            let code = if result.balanced { 0 } else { 1 };
            (serde_json::to_string_pretty(&result)?, code)
        }

        Commands::Stat { file } => {
            stats::increment(&file);
            let meta = std::fs::metadata(&file)?;
            let size = meta.len();
            let modified_ts = meta.modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            let created_ts = meta.created()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);

            let content = std::fs::read_to_string(&file)?;
            let line_count = content.lines().count();
            let has_header = content.contains("<!-- AI-SKILL-HEADER START");

            let anchor_count = locator::list_anchors(&content).len();
            let deps = module_deps::scan_deps(&file).ok();
            let dep_count = deps.as_ref().map(|d| d.total).unwrap_or(0);
            let missing_deps = deps.as_ref().map(|d| d.missing).unwrap_or(0);

            let file_stats = stats::get(&file);
            let modified_ago = stats::format_ago(modified_ts);
            let last_call_ago = if file_stats.last_call > 0 {
                stats::format_ago(file_stats.last_call)
            } else {
                String::from("never")
            };

            let text = if json {
                serde_json::to_string_pretty(&serde_json::json!({
                    "path": file.display().to_string(),
                    "size_bytes": size,
                    "lines": line_count,
                    "has_header": has_header,
                    "anchors": anchor_count,
                    "dependencies": dep_count,
                    "missing_dependencies": missing_deps,
                    "modified_ts": modified_ts,
                    "created_ts": created_ts,
                    "calls": file_stats.calls,
                    "last_call_ts": file_stats.last_call,
                }))?
            } else {
                let mut out = String::new();
                out.push_str(&format!("File:     {}\n", file.display()));
                out.push_str(&format!("Size:     {} bytes ({:.1} KB)\n", size, size as f64 / 1024.0));
                out.push_str(&format!("Lines:    {}\n", line_count));
                out.push_str(&format!("Header:   {}\n", if has_header { "yes" } else { "no" }));
                out.push_str(&format!("Anchors:  {}\n", anchor_count));
                if dep_count > 0 || missing_deps > 0 {
                    out.push_str(&format!("Deps:     {} (missing: {})\n", dep_count, missing_deps));
                }
                out.push_str(&format!("Modified: {}\n", modified_ago));
                out.push_str(&format!("Calls:    {}\n", file_stats.calls));
                if file_stats.last_call > 0 {
                    out.push_str(&format!("Last use: {}\n", last_call_ago));
                }
                out
            };
            (text, 0)
        }

        Commands::Serve { file, port, open, live: live_inject } => {
            stats::increment(&file);
            live::serve(&file, port, open, live_inject)?;
            (String::new(), 0)
        }

        Commands::Debug { action } => {
            match action {
                DebugAction::Start { file, port, no_headless } => {
                    stats::increment(&file);
                    let result = page::debug_start(&file, port, !no_headless);
                    match result {
                        Ok(v) => {
                            let text = if json {
                                serde_json::to_string_pretty(&v)?
                            } else {
                                format!("Browser started on port {} (pid {})\nWebSocket: {}\n\nUse `sfhtml page screenshot --port {}` to interact.",
                                    v["port"], v["pid"], v["ws_url"].as_str().unwrap_or(""), port)
                            };
                            (text, 0)
                        }
                        Err(e) => {
                            eprintln!("⚠ debug start failed: {}", e);
                            eprintln!("All other sfhtml commands remain available.");
                            (String::new(), 1)
                        }
                    }
                }
                DebugAction::Stop { port } => {
                    let result = page::debug_stop(port)?;
                    let text = if json {
                        serde_json::to_string_pretty(&result)?
                    } else {
                        format!("Stopped session on port {}", port)
                    };
                    (text, 0)
                }
                DebugAction::List => {
                    let result = page::debug_list()?;
                    let text = if json {
                        serde_json::to_string_pretty(&result)?
                    } else {
                        let sessions = result["sessions"].as_array();
                        if let Some(arr) = sessions {
                            if arr.is_empty() {
                                String::from("No active browser sessions.")
                            } else {
                                arr.iter().map(|s| {
                                    format!("  port {} | pid {} | {}",
                                        s["port"], s["pid"], s["ws_url"].as_str().unwrap_or(""))
                                }).collect::<Vec<_>>().join("\n")
                            }
                        } else {
                            String::new()
                        }
                    };
                    (text, 0)
                }
            }
        }

        Commands::Page { action } => {
            let page_result: Result<serde_json::Value> = match action {
                PageAction::Open { port } => page::page_open(port),
                PageAction::Screenshot { port, selector, output } =>
                    page::page_screenshot(port, selector.as_deref(), output.as_deref()),
                PageAction::Dom { port, selector } =>
                    page::page_dom(port, selector.as_deref()),
                PageAction::Console { port } => page::page_console(port),
                PageAction::Network { port, wait } => page::page_network(port, wait),
                PageAction::Click { selector, port } => page::page_click(port, &selector),
                PageAction::Type { selector, text, port } => page::page_type(port, &selector, &text),
                PageAction::Scroll { x, y, port } => page::page_scroll(port, x, y),
                PageAction::Touch { x, y, port } => page::page_touch(port, x, y),
                PageAction::Eval { expression, port } => page::page_eval(port, &expression),
                PageAction::Pdf { port, output } => page::page_pdf(port, output.as_deref()),
                PageAction::Close { port } => page::page_close(port),
            };

            match page_result {
                Ok(v) => {
                    (serde_json::to_string_pretty(&v)?, 0)
                }
                Err(e) => {
                    eprintln!("⚠ page command failed: {}", e);
                    eprintln!("Ensure a browser session is running: `sfhtml debug start <file>`");
                    (String::new(), 1)
                }
            }
        }
    };

    // Apply output controls pipeline
    let final_output = apply_output_controls(&output_text, oc_head, oc_tail, oc_grep.as_deref(), oc_count, oc_truncate);
    if !final_output.is_empty() {
        print!("{}", final_output);
        // Ensure trailing newline
        if !final_output.ends_with('\n') {
            println!();
        }
    }

    Ok(exit_code)
}

/// Apply universal output controls: --head, --tail, --grep, --count, --truncate
fn apply_output_controls(
    text: &str,
    head: Option<usize>,
    tail: Option<usize>,
    grep: Option<&str>,
    count: bool,
    truncate: Option<usize>,
) -> String {
    if head.is_none() && tail.is_none() && grep.is_none() && !count && truncate.is_none() {
        return text.to_string();
    }

    let mut lines: Vec<&str> = text.lines().collect();

    // --grep: filter lines matching pattern
    if let Some(pattern) = grep {
        let lower_pattern = pattern.to_lowercase();
        lines.retain(|line| line.to_lowercase().contains(&lower_pattern));
    }

    // --head: keep only first N lines
    if let Some(n) = head {
        lines.truncate(n);
    }

    // --tail: keep only last N lines
    if let Some(n) = tail {
        if lines.len() > n {
            lines = lines[lines.len() - n..].to_vec();
        }
    }

    // --count: return line count instead of content
    if count {
        return format!("{}", lines.len());
    }

    let mut result = lines.join("\n");
    if !lines.is_empty() {
        result.push('\n');
    }

    // --truncate: cap output at N bytes
    if let Some(max_bytes) = truncate {
        if result.len() > max_bytes {
            // Truncate at char boundary
            let mut end = max_bytes;
            while end > 0 && !result.is_char_boundary(end) {
                end -= 1;
            }
            result.truncate(end);
            result.push_str("\n... [truncated]\n");
        }
    }

    result
}
