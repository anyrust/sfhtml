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
        /// Number of concurrent worker threads (0 = auto)
        #[arg(long, default_value_t = 0)]
        jobs: usize,
        /// Return only top N HTML results (0 = all)
        #[arg(long, default_value_t = 0)]
        top: usize,
        /// Show summary statistics only (auto-enabled when HTML > 300)
        #[arg(long, default_value_t = false)]
        summary: bool,
        /// Sort by: modified (default), created, name, size
        #[arg(long, default_value = "modified")]
        sort_by: String,
        /// Sort order: desc (default), asc
        #[arg(long, default_value = "desc")]
        order: String,
        /// Filter: only show entries whose path contains ALL given keywords
        #[arg(long, value_delimiter = ',')]
        r#match: Vec<String>,
        /// Max non-HTML items (rough + dirs + other) to collect (default: 3000)
        #[arg(long, default_value_t = 3000)]
        misc_limit: usize,
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
        /// Context type: cli, header, js, html
        #[arg(long, default_value = "cli")]
        context: String,
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

    match cli.command {
        Commands::Scan { dir, recursive, jobs, top, summary, sort_by, order, r#match, misc_limit } => {
            let sort_key = scanner::SortKey::from_str(&sort_by);
            let sort_order = scanner::SortOrder::from_str(&order);
            let result = scanner::scan_directory(&dir, recursive, jobs, sort_key, sort_order, &r#match, misc_limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if summary || (!summary && top == 0 && result.html_total > 300) {
                println!("{}", scanner::format_summary(&result));
            } else {
                println!("{}", scanner::format_text(&result, top));
            }
            Ok(0)
        }

        Commands::Search { query, dir, top, context } => {
            let results = search::search_files(&dir, &query, top, context)?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            Ok(0)
        }

        Commands::Header { file, section } => {
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
                println!("{}", header_text);
                eprintln!("\n⚠ File size ({:.1} KB) exceeds 50 KB limit. Use `sfhtml read {} --head N` or `sfhtml locate {} <anchor>` to inspect code sections.",
                    file_size as f64 / 1024.0, file.display(), file.display());
            } else {
                let content = std::fs::read_to_string(&file)?;
                if let Some(section_num) = section {
                    let s = header::extract_section(&content, section_num)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&s)?);
                    } else {
                        println!("## {}. {}\n{}", s.number, s.title, s.content);
                    }
                } else {
                    let h = header::extract_header(&content)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&h)?);
                    } else {
                        println!("{}", h.full_markdown);
                    }
                }
            }
            Ok(0)
        }

        Commands::Locate { file, anchor, context } => {
            let content = std::fs::read_to_string(&file)?;
            let result = locator::locate_anchor(&content, &anchor, context)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                for m in &result.matches {
                    let end_str = m.end_line
                        .map(|e| format!("-{}", e))
                        .unwrap_or_default();
                    println!("Anchor \"{}\" found at line {}{}:", result.anchor, m.line, end_str);
                    println!("{}", m.context_preview);
                    println!();
                }
            }
            Ok(0)
        }

        Commands::Read { file, start_line, end_line, head, tail } => {
            let output = reader::read_lines(&file, start_line, end_line, head, tail)?;
            print!("{}", output);
            Ok(0)
        }

        Commands::Apply { file, diff, dry_run, backup, fuzz, force } => {
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

            if json {
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
                println!("{}", serde_json::to_string_pretty(&json_result)?);
            } else {
                print!("{}", applier::format_apply_result(&result, file_name));
            }
            Ok(0)
        }

        Commands::Diff { file, old_file, context } => {
            let new_text = std::fs::read_to_string(&file)?;
            let old_text = std::fs::read_to_string(&old_file)?;
            let new_name = file.file_name().and_then(|n| n.to_str()).unwrap_or("new");
            let old_name = old_file.file_name().and_then(|n| n.to_str()).unwrap_or("old");
            let diff_output = differ::generate_diff(&old_text, &new_text, old_name, new_name, context);
            print!("{}", diff_output);
            Ok(0)
        }

        Commands::AnchorList { file, top } => {
            let content = std::fs::read_to_string(&file)?;
            let anchors = locator::list_anchors(&content);
            let display: &[locator::AnchorListEntry] = if top > 0 { &anchors[..std::cmp::min(top, anchors.len())] } else { &anchors };
            if json {
                println!("{}", serde_json::to_string_pretty(&display)?);
            } else {
                for a in display {
                    let header_mark = if a.in_header { "" } else { " [not in header]" };
                    println!("{:<40} line {:>6}  {}{}", a.name, a.line, a.anchor_type, header_mark);
                }
                if top > 0 && anchors.len() > top {
                    println!("\n... and {} more (use --top 0 to show all)", anchors.len() - top);
                }
            }
            Ok(0)
        }

        Commands::Validate { file, syntax, fix } => {
            let content = std::fs::read_to_string(&file)?;

            if fix {
                // Auto-fix by running header-rebuild
                let new_content = header::rebuild_header(&content, true)?;
                std::fs::write(&file, &new_content)?;
                println!("Header rebuilt. Re-validating...\n");
                let result = validator::validate_file(&new_content, syntax)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    print!("{}", validator::format_text(&result));
                }
            } else {
                let result = validator::validate_file(&content, syntax)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    print!("{}", validator::format_text(&result));
                }
            }
            Ok(0)
        }

        Commands::HeaderRebuild { file, dry_run, preserve_descriptions } => {
            let content = std::fs::read_to_string(&file)?;
            let new_content = header::rebuild_header(&content, preserve_descriptions)?;

            if dry_run {
                println!("{}", new_content);
            } else {
                std::fs::write(&file, &new_content)?;
                println!("Header rebuilt successfully.");
            }
            Ok(0)
        }

        Commands::Init { file } => {
            let content = std::fs::read_to_string(&file)?;

            // Check if header already exists
            if content.contains("<!-- AI-SKILL-HEADER START") {
                eprintln!("Error: File already has an AI-SKILL-HEADER.");
                return Ok(1);
            }

            let new_content = header::generate_init_header(&content)?;
            std::fs::write(&file, &new_content)?;
            println!("AI-SKILL-HEADER injected into {}", file.display());
            Ok(0)
        }

        Commands::Create { path, title, with_header, force } => {
            creator::create_html(&path, &title, with_header, force)?;
            if json {
                println!("{}", serde_json::json!({
                    "created": path.display().to_string(),
                    "with_header": with_header,
                }));
            } else {
                println!("Created {}{}", path.display(),
                    if with_header { " (with AI-SKILL-HEADER)" } else { "" });
            }
            Ok(0)
        }

        Commands::SaveAs { source, dest, inject_header, force } => {
            creator::save_as(&source, &dest, inject_header, force)?;
            if json {
                println!("{}", serde_json::json!({
                    "source": source.display().to_string(),
                    "dest": dest.display().to_string(),
                    "header_injected": inject_header && !std::fs::read_to_string(&dest)
                        .map(|c| c.contains("<!-- AI-SKILL-HEADER START"))
                        .unwrap_or(false),
                }));
            } else {
                println!("Saved {} → {}{}", source.display(), dest.display(),
                    if inject_header { " (header injected)" } else { "" });
            }
            Ok(0)
        }

        Commands::History { action } => {
            match action {
                HistoryAction::List { file, top } => {
                    let entries = history::list_entries(file.as_deref())?;
                    let display = if top > 0 { &entries[..std::cmp::min(top, entries.len())] } else { &entries[..] };
                    if json {
                        println!("{}", serde_json::to_string_pretty(&display)?);
                    } else if entries.is_empty() {
                        println!("No history entries found.");
                    } else {
                        let cache_size = history::cache_size()?;
                        println!("Diff history ({} entries, {:.1} KB / 10240 KB):\n",
                            entries.len(), cache_size as f64 / 1024.0);
                        for e in display {
                            println!("  {} | {} | {} | +{} -{} | {}",
                                e.id, e.timestamp_human, e.file_path,
                                e.lines_added, e.lines_removed, e.description);
                        }
                        if top > 0 && entries.len() > top {
                            println!("\n... and {} more (use --top 0 to show all)", entries.len() - top);
                        }
                    }
                    Ok(0)
                }

                HistoryAction::Show { id } => {
                    let entry = history::show_entry(&id)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&entry)?);
                    } else {
                        println!("ID:        {}", entry.id);
                        println!("File:      {}", entry.file_path);
                        println!("Time:      {}", entry.timestamp_human);
                        println!("Changes:   +{} -{} ({} hunks)",
                            entry.lines_added, entry.lines_removed, entry.hunks_applied);
                        println!("\n--- Forward Diff ---\n{}", entry.diff_text);
                        println!("\n--- Reverse Diff (for rollback) ---\n{}", entry.reverse_diff);
                    }
                    Ok(0)
                }

                HistoryAction::Rollback { file, id, dry_run, fuzz } => {
                    let result = history::rollback(&file, &id, fuzz, dry_run)?;
                    if json {
                        println!("{}", serde_json::json!({
                            "rollback": !dry_run,
                            "id": id,
                            "message": result,
                        }));
                    } else if dry_run {
                        println!("--- Dry run: reverse diff to apply ---\n{}", result);
                    } else {
                        println!("{}", result);
                    }
                    Ok(0)
                }

                HistoryAction::Delete { id } => {
                    let freed = history::delete_entry(&id)?;
                    if json {
                        println!("{}", serde_json::json!({
                            "deleted": id,
                            "freed_bytes": freed,
                        }));
                    } else {
                        println!("Deleted history entry: {} (freed {} bytes)", id, freed);
                    }
                    Ok(0)
                }

                HistoryAction::Status => {
                    let size = history::cache_size()?;
                    let entries = history::list_entries(None)?;
                    let dir = history::cache_dir()?;
                    if json {
                        println!("{}", serde_json::json!({
                            "cache_dir": dir.display().to_string(),
                            "entries": entries.len(),
                            "size_bytes": size,
                            "limit_bytes": 10 * 1024 * 1024,
                            "usage_percent": (size as f64 / (10.0 * 1024.0 * 1024.0) * 100.0),
                        }));
                    } else {
                        println!("Cache dir:  {}", dir.display());
                        println!("Entries:    {}", entries.len());
                        println!("Size:       {:.1} KB / 10240 KB ({:.1}%)",
                            size as f64 / 1024.0,
                            size as f64 / (10.0 * 1024.0 * 1024.0) * 100.0);
                    }
                    Ok(0)
                }

                HistoryAction::Clean => {
                    let (removed, freed) = history::clean_cache()?;
                    if json {
                        println!("{}", serde_json::json!({
                            "removed": removed,
                            "freed_bytes": freed,
                        }));
                    } else {
                        println!("Cleaned {} entries, freed {:.1} KB", removed, freed as f64 / 1024.0);
                    }
                    Ok(0)
                }
            }
        }

        Commands::Module { file, depth, top } => {
            let result = if depth > 0 {
                module_deps::scan_deps_recursive(&file, depth)?
            } else {
                module_deps::scan_deps(&file)?
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                print!("{}", module_deps::format_text(&result, top));
            }
            if result.missing > 0 { Ok(1) } else { Ok(0) }
        }

        Commands::CheckOutput { file, context } => {
            let text = if let Some(path) = file {
                std::fs::read_to_string(&path)?
            } else {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };

            let ctx = syntax_check::CheckContext::from_str(&context);
            let result = syntax_check::check_syntax(&text, ctx);

            println!("{}", serde_json::to_string_pretty(&result)?);

            if result.balanced {
                Ok(0)
            } else {
                Ok(1)
            }
        }

        Commands::Serve { file, port, open, live: live_inject } => {
            live::serve(&file, port, open, live_inject)?;
            Ok(0)
        }

        Commands::Debug { action } => {
            match action {
                DebugAction::Start { file, port, no_headless } => {
                    let result = page::debug_start(&file, port, !no_headless);
                    match result {
                        Ok(v) => {
                            if json { println!("{}", serde_json::to_string_pretty(&v)?); }
                            else {
                                println!("Browser started on port {} (pid {})",
                                    v["port"], v["pid"]);
                                println!("WebSocket: {}", v["ws_url"].as_str().unwrap_or(""));
                                println!("\nUse `sfhtml page screenshot --port {}` to interact.", port);
                            }
                            Ok(0)
                        }
                        Err(e) => {
                            eprintln!("⚠ debug start failed: {}", e);
                            eprintln!("All other sfhtml commands remain available.");
                            Ok(1)
                        }
                    }
                }
                DebugAction::Stop { port } => {
                    let result = page::debug_stop(port)?;
                    if json { println!("{}", serde_json::to_string_pretty(&result)?); }
                    else { println!("Stopped session on port {}", port); }
                    Ok(0)
                }
                DebugAction::List => {
                    let result = page::debug_list()?;
                    if json { println!("{}", serde_json::to_string_pretty(&result)?); }
                    else {
                        let sessions = result["sessions"].as_array();
                        if let Some(arr) = sessions {
                            if arr.is_empty() {
                                println!("No active browser sessions.");
                            } else {
                                for s in arr {
                                    println!("  port {} | pid {} | {}",
                                        s["port"], s["pid"], s["ws_url"].as_str().unwrap_or(""));
                                }
                            }
                        }
                    }
                    Ok(0)
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
                    println!("{}", serde_json::to_string_pretty(&v)?);
                    Ok(0)
                }
                Err(e) => {
                    eprintln!("⚠ page command failed: {}", e);
                    eprintln!("Ensure a browser session is running: `sfhtml debug start <file>`");
                    Ok(1)
                }
            }
        }
    }
}
