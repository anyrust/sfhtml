---
name: sfhtml
description: Read, edit, scan, validate, and interact with single-file HTML applications. Provides structured access to HTML files with AI-SKILL-HEADERs, workspace scanning, diff-based editing with history/rollback, dependency analysis, and browser page interaction via CDP.
---

# sfhtml — Single-File HTML AI-Skill CLI

## When to Use This Skill

Use `sfhtml` when working with **single-file HTML applications** (HTML + CSS + JS in one `.html` file). It is your primary tool for:

- **Discovering** HTML files in a workspace (`scan`)
- **Understanding** file structure via AI-SKILL-HEADERs (`header`, `anchor-list`, `module`)
- **Reading** specific code sections (`read`, `locate`)
- **Editing** files safely via unified diffs (`apply`, `diff`)
- **Validating** changes didn't break anything (`validate`, `check-output`)
- **Interacting** with the rendered page in a browser (`page click`, `page screenshot`, etc.)

## Core Workflow

```bash
# 1. Discover + Understand (3-step fast workflow with --expand)
sfhtml scan . --recursive --expand --json  # Scan AND inline all headers

# 2. Navigate — find specific code
sfhtml locate app.html "initApp" --context 10

# 3. Edit — apply changes via diff
sfhtml apply app.html --diff patch.diff --backup
```

Or the standard step-by-step workflow:

```bash
# 1. Discover — find all HTML files in the project
sfhtml scan . --recursive --json

# 2. Understand — read the structured header
sfhtml header app.html --json

# 3. Navigate — find specific code
sfhtml locate app.html "initApp" --context 10
sfhtml anchor-list app.html --json

# 4. Read — get specific lines
sfhtml read app.html 45 120

# 5. Edit — apply changes via diff
sfhtml apply app.html --diff patch.diff --backup

# 6. Validate — check consistency after editing
sfhtml validate app.html --json

# 7. View result — interact with the rendered page
sfhtml debug start app.html
sfhtml page screenshot --output result.png
sfhtml page click "#run-btn"
sfhtml page console --json
sfhtml debug stop
```

## Command Reference

### Always use `--json` flag for structured output.

### Scan & Search
```bash
sfhtml scan <dir> --recursive --json                    # Find all HTML files
sfhtml scan <dir> --expand --json                       # Scan + inline full headers (3-step workflow)
sfhtml scan <dir> --sort-by relevance --json             # Sort by usage × recency
sfhtml scan <dir> --sort-by modified --top 10 --json    # Recent files
sfhtml scan <dir> --match "game,canvas" --json          # Filter by keywords
sfhtml search "function render" --dir . --top 5         # TF-based code search
```

### File Metadata
```bash
sfhtml stat <file>                       # Size, lines, header, anchors, deps, calls
sfhtml stat <file> --json                # Machine-readable metadata
```

### Read & Navigate
```bash
sfhtml header <file> --json                  # Full AI-SKILL-HEADER
sfhtml header <file> --section 5 --json      # Just Key Internal Modules section
sfhtml read <file> 100 200                   # Lines 100–200
sfhtml read <file> --head 50                 # First 50 lines
sfhtml locate <file> "functionName" --context 5  # Find anchor + context
sfhtml anchor-list <file> --json             # All navigable anchors
sfhtml module <file> --depth 2 --json        # Dependency tree (2 levels deep)
```

### Edit
```bash
sfhtml apply <file> --diff <patch> --json            # Apply diff
sfhtml apply <file> --diff <patch> --dry-run --json  # Preview changes
sfhtml apply <file> --diff - --json                  # Diff from stdin
sfhtml diff <new-file> <old-file> --context 3        # Generate diff
sfhtml create <path> --with-header --title "My App"  # New file
sfhtml save-as <src> <dest> --inject-header           # Copy + add header
sfhtml init <file>                                    # Add header to existing file
```

### Validate
```bash
sfhtml validate <file> --json           # Header↔code consistency
sfhtml validate <file> --fix            # Auto-fix by rebuilding header
sfhtml header-rebuild <file> --dry-run  # Preview Section 5 rebuild
sfhtml check-output <file> --mode js   # Check bracket/quote balance
```

### History & Rollback
```bash
sfhtml history list --json               # All saved diffs
sfhtml history show <id> --json          # View a specific diff
sfhtml history rollback <file> <id>      # Undo a change
sfhtml history clean                     # Clear all history
```

### Live Serve
```bash
sfhtml serve <file> --port 8080          # Serve with live reload
sfhtml serve <file> --open               # Auto-open in browser
sfhtml serve <file> --live=false         # Serve without live script injection
```

### Page Interaction (Browser)
```bash
# Start a browser session (headless by default)
sfhtml debug start <file> [--port 9222] [--no-headless]

# Observe
sfhtml page screenshot [--selector "canvas"] [--output shot.png]
sfhtml page dom [--selector "#app"]
sfhtml page console
sfhtml page network [--wait 3000]

# Interact
sfhtml page click "<css-selector>"
sfhtml page type "<css-selector>" "input text"
sfhtml page scroll --y 500
sfhtml page touch 100 200
sfhtml page eval "document.title"

# Export
sfhtml page pdf [--output page.pdf]

# End session
sfhtml debug stop [--port 9222]
```

Multiple browser sessions can run on different ports simultaneously:
```bash
sfhtml debug start app1.html --port 9222
sfhtml debug start app2.html --port 9223
sfhtml page screenshot --port 9222 --output app1.png
sfhtml page screenshot --port 9223 --output app2.png
```

## Section 5 Format (Key Internal Modules)

Section 5 lists **block-level anchors** (no line ranges — code is dynamic). Each entry identifies a readable code block that AI can `sfhtml locate` in one shot:

```
- `<script type="module">` — App entry: initApp, bindEvents, state management
- `<script>` — Data processing: parseGsiData, DataFusion, export
- `<div id="app">` — Dashboard layout, chart panels, data table
- `function initApp` — Bootstrap: loads config, binds events, first render
- `class TraverseRenderer` — Canvas traverse diagram with pan/zoom
```

Anchor types (by priority):
1. **`<script>`** / **`<script type="module">`** — script blocks (most important)
2. **`<div id="...">` / `<section>` / `<nav>`** — significant HTML elements with id
3. **`function name`** / **`class name`** — major functions and classes

`const` / `let` / `var` should NOT be standalone anchors — mention them inside the block description instead.

Section 5 is auto-generated by `sfhtml header-rebuild` or `sfhtml init`.

## Expected Output Formats

### scan --json
```json
{
  "html_files": [
    {
      "path": "todo.html",
      "app_name": "TodoApp",
      "summary": "Minimal task manager",
      "has_header": true,
      "file_lines": 169,
      "modified_ts": 1742288244,
      "calls": 3,
      "modified_ago": "6d ago"
    }
  ],
  "dirs": [{ "path": "components", "children": 1 }],
  "other_files": [{ "path": "button.js" }],
  "html_total": 3
}
```

### scan --expand --json (includes inline header)
Same as above, but each html_file entry with `has_header: true` gains a `"header"` field containing the full markdown header text.

### header --json
```json
{
  "full_markdown": "# AppName — Summary\n\n## 1. Overview\n...",
  "title_line": "# AppName — Summary",
  "app_name": "AppName",
  "summary": "Summary",
  "sections": [
    { "number": 1, "title": "Overview", "content": "..." },
    { "number": 5, "title": "Key Internal Modules", "content": "..." }
  ],
  "start_line": 2,
  "end_line": 28
}
```

### apply --json
```json
{
  "hunks_applied": 1,
  "lines_removed": 2,
  "lines_added": 3,
  "new_size_bytes": 7066,
  "dry_run": false,
  "history_id": "1773588106_308020016_todo.html",
  "hunk_details": [
    { "hunk_index": 0, "stated_line": 83, "matched_line": 83, "fuzz_offset": 0, "context_search": false }
  ],
  "validation": { "status": "success", "warnings": [] }
}
```

### locate (text output)
```
Anchor "render" found at line 108-120:
        function render() {
            var list = document.getElementById("task-list");
            ...
```

### validate --json
```json
{
  "anchor_consistency": { "total": 10, "found": 10, "missing_from_code": [] },
  "syntax": { "balanced": true, "errors": [], "warnings": [] },
  "total_errors": 0,
  "total_warnings": 0
}
```

## Important Notes

- **All write operations support `--dry-run`** — always preview before applying
- **`apply` auto-saves history** — use `history rollback` to undo
- **`--json` is required for structured output** — without it, output is human-readable text
- **Browser features are optional** — if no Chrome/Chromium/Edge is found, commands return a warning but all other sfhtml features work normally
- **Header size warning** — files >50KB trigger a warning on `header` command; use `read` or `locate` instead
- **`--timeout` enforces real deadlines** — scan stops collecting/scanning files when time runs out, returning partial results with a timeout summary
- **Output controls work on any command** — use `--head N`, `--tail N`, `--grep "pattern"`, `--count`, or `--truncate N` to shape output without external pipes
- **Diff patches >10 lines: use `create_file`, not heredoc** — when a patch or diff content exceeds 10 lines, write it to a temp file (e.g. `/tmp/patch.diff`) via `create_file` first, then reference it with `sfhtml apply <file> --diff /tmp/patch.diff`. Do NOT attempt to pass large content via shell heredoc (`cat << 'EOF'`), as heredoc is unreliable in automated terminal sessions and frequently causes escaping or truncation errors.
- **Live serve auto-reloads** — `sfhtml serve` watches the file and pushes changes to all connected browsers via WebSocket. Use `sfhtml apply` to edit the file and browsers update instantly.
