# sfhtml (Python)

Python wrapper for [sfhtml](https://github.com/anyrust/sfhtml) — Single-File HTML AI-Skill CLI.

## Install

```bash
pip install sfhtml
```

> **Requires** the `sfhtml` binary. Install via `cargo install sfhtml` or download from [GitHub Releases](https://github.com/anyrust/sfhtml/releases).

## Usage

### As a Python library

```python
import sfhtml

# Scan for HTML files
files = sfhtml.scan("./my-project")

# Read file header
header = sfhtml.header("app.html")

# Apply a diff
result = sfhtml.apply("app.html", "patch.diff", backup=True)

# Validate
report = sfhtml.validate("app.html")

# Browser interaction
sfhtml.debug_start("app.html")
sfhtml.page_click("#submit-btn")
logs = sfhtml.page_console()
sfhtml.debug_stop()

# Run any sfhtml command
result = sfhtml.run("anchor-list", "app.html")
```

### As a CLI (same as the Rust binary)

```bash
sfhtml scan . --recursive --json
sfhtml header app.html
sfhtml page screenshot --output shot.png
```

## License

MIT
