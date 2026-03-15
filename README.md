# sfhtml

Single-File HTML AI-Skill CLI — a fast, zero-dependency command-line tool for AI agents to read, edit, scan, validate, and interact with single-file HTML applications.

## Why sfhtml?

Single-file HTML apps (one `.html` file containing HTML + CSS + JS) are the simplest deployable web format. **sfhtml** gives AI agents structured access to these files: scan workspaces, extract metadata headers, apply diffs, validate consistency, trace module dependencies, and even interact with the rendered page via a browser.

## Install

### From crates.io (Rust)
```bash
cargo install sfhtml
```

### From source
```bash
git clone https://github.com/anthropic-ai/sfhtml.git
cd sfhtml
cargo build --release
# Binary at target/release/sfhtml (~2 MB)
```

### Python (pip)
```bash
pip install sfhtml
```

## Quick Start

```bash
# Scan a directory for HTML files with AI-SKILL-HEADERs
sfhtml scan ./my-project --recursive --json

# Extract the structured header from an HTML file
sfhtml header app.html

# Apply a code change via unified diff
sfhtml apply app.html --diff changes.patch

# Open the page in a browser and take a screenshot
sfhtml debug start app.html
sfhtml page screenshot --output shot.png

# Click a button and check console output
sfhtml page click "#submit-btn"
sfhtml page console
```

## Commands

### Workspace Discovery
| Command | Description |
|---------|-------------|
| `scan <dir>` | Fast-scan directory for HTML files (supports `--recursive`, `--sort-by`, `--match`, `--top`, `--summary`) |
| `search <query>` | TF-based search across HTML files |

### File Reading
| Command | Description |
|---------|-------------|
| `header <file>` | Extract the AI-SKILL-HEADER (structured metadata) |
| `read <file> [start] [end]` | Read specific line ranges (`--head`, `--tail`) |
| `locate <file> <anchor>` | Find a code anchor with context |
| `anchor-list <file>` | List all locatable anchors |
| `module <file>` | Scan ES module / resource dependencies (`--depth N` for recursive) |

### File Editing
| Command | Description |
|---------|-------------|
| `apply <file> --diff <patch>` | Apply unified diff (with fuzz, backup, validation) |
| `diff <new> <old>` | Generate unified diff between two files |
| `create <path>` | Create a new HTML file (`--with-header`) |
| `save-as <src> <dest>` | Copy file, optionally inject header |
| `init <file>` | Inject an AI-SKILL-HEADER template |

### Validation & Maintenance
| Command | Description |
|---------|-------------|
| `validate <file>` | Check header↔code consistency + syntax |
| `header-rebuild <file>` | Auto-rebuild header sections from code |
| `check-output [file]` | Check symbol balance (brackets, quotes) |
| `history list\|show\|rollback\|delete\|clean` | Diff history management with rollback |

### Page Interaction (Browser via CDP)
| Command | Description |
|---------|-------------|
| `debug start <file>` | Launch browser with CDP (`--port`, `--no-headless`) |
| `debug stop` | Stop browser session |
| `debug list` | List active sessions |
| `page screenshot` | Capture PNG (`--selector`, `--output`) |
| `page dom` | Get rendered DOM HTML (`--selector`) |
| `page console` | Get console log messages |
| `page network` | Get network request events |
| `page click <sel>` | Click an element |
| `page type <sel> <text>` | Type into an input |
| `page scroll` | Scroll page (`--x`, `--y`) |
| `page touch <x> <y>` | Simulate touch event |
| `page eval <expr>` | Execute JavaScript |
| `page pdf` | Export as PDF |

### Global Flags
```
--json          Output structured JSON (recommended for AI agents)
--timeout <ms>  Maximum execution time
--diagnostic    Machine-readable diagnostic on stderr
--trace         Step-by-step execution log on stderr
```

## AI-SKILL-HEADER

sfhtml recognizes a structured comment block in HTML files:

```html
<!-- AI-SKILL-HEADER START
## 1. App Name
My Application

## 2. Purpose
A brief description...

## 3. Tech Stack
Vanilla JS, CSS Grid

## 4. Constraints
- Single file, no build step
- Zero external dependencies

## 5. Module Map
- initApp() : line 45 — Application bootstrap
- render()  : line 120 — Main render function

## 6. Style Map
- .container : line 200 — Main layout
AI-SKILL-HEADER END -->
```

This header lets AI agents understand the file structure without reading the entire codebase.

## Output Size Control

All list commands support `--top N` to limit results. When scan finds >300 HTML files, it auto-switches to summary mode. Use `--summary` to force summary, or `--top 0` for all results.

## Design Principles

- **Single binary, zero runtime deps** — just copy and run
- **AI-first** — all commands support `--json` for structured output
- **Non-destructive** — `--dry-run` and `--backup` on writes, history with rollback
- **Gracefully optional** — browser features fail with a warning, core editing always works
- **Fast** — parallel scanning with rayon, memory-mapped file reading

## License

MIT
