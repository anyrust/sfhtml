---
name: sfhtml
description: Read, edit, scan, validate, and interact with single-file HTML applications. Provides structured access to HTML files with AI-SKILL-HEADERs, workspace scanning, diff-based editing with history/rollback, dependency analysis, and browser page interaction via CDP.
tools:
  - run_in_terminal
applyTo: "**/*.html"
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
sfhtml scan <dir> --sort-by modified --top 10 --json    # Recent files
sfhtml scan <dir> --match "game,canvas" --json          # Filter by keywords
sfhtml search "function render" --dir . --top 5         # TF-based code search
```

### Read & Navigate
```bash
sfhtml header <file> --json                  # Full AI-SKILL-HEADER
sfhtml header <file> --section 5 --json      # Just Module Map section
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
sfhtml header-rebuild <file> --dry-run  # Preview header rebuild
sfhtml check-output <file> --context js # Check bracket/quote balance
```

### History & Rollback
```bash
sfhtml history list --json               # All saved diffs
sfhtml history show <id> --json          # View a specific diff
sfhtml history rollback <file> <id>      # Undo a change
sfhtml history clean                     # Clear all history
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

## Important Notes

- **All write operations support `--dry-run`** — always preview before applying
- **`apply` auto-saves history** — use `history rollback` to undo
- **`--json` is required for structured output** — without it, output is human-readable text
- **Browser features are optional** — if no Chrome/Chromium/Edge is found, commands return a warning but all other sfhtml features work normally
- **Header size warning** — files >50KB trigger a warning on `header` command; use `read` or `locate` instead
- **Scan auto-summarizes** — when >300 HTML files found, auto-switches to summary mode; use `--top N` to limit
