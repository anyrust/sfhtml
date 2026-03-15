"""sfhtml — Single-File HTML AI-Skill CLI (Python wrapper)

Provides a Python interface to the sfhtml Rust binary.
All commands support --json for structured output.

Usage:
    import sfhtml
    result = sfhtml.run("scan", ".", "--recursive")
    result = sfhtml.scan(".")
    result = sfhtml.header("app.html")
"""

import json
import subprocess
import shutil
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Union


def _find_binary() -> str:
    """Find the sfhtml binary. Checks: same directory, PATH, cargo install location."""
    # 1. Bundled binary next to this package
    pkg_dir = Path(__file__).parent
    for candidate in [pkg_dir / "sfhtml", pkg_dir / "sfhtml.exe"]:
        if candidate.exists():
            return str(candidate)

    # 2. On PATH
    found = shutil.which("sfhtml")
    if found:
        return found

    # 3. Cargo install location
    cargo_bin = Path.home() / ".cargo" / "bin" / "sfhtml"
    if cargo_bin.exists():
        return str(cargo_bin)

    raise FileNotFoundError(
        "sfhtml binary not found. Install it with:\n"
        "  cargo install sfhtml\n"
        "Or download from: https://github.com/anyrust/sfhtml/releases"
    )


def run(*args: str, json_output: bool = True, timeout: Optional[int] = None) -> Union[Dict, str]:
    """Run an sfhtml command and return the result.

    Args:
        *args: Command arguments (e.g. "scan", ".", "--recursive")
        json_output: If True, append --json and parse output as JSON
        timeout: Timeout in seconds (None = no timeout)

    Returns:
        Parsed JSON dict if json_output=True, raw stdout string otherwise.

    Raises:
        FileNotFoundError: If sfhtml binary is not found
        subprocess.CalledProcessError: If command fails
        json.JSONDecodeError: If JSON parsing fails
    """
    binary = _find_binary()
    cmd = [binary] + list(args)
    if json_output and "--json" not in args:
        cmd.append("--json")

    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
    )

    if result.returncode != 0:
        raise subprocess.CalledProcessError(
            result.returncode, cmd, result.stdout, result.stderr
        )

    if json_output:
        return json.loads(result.stdout)
    return result.stdout


# ---------------------------------------------------------------------------
# Convenience functions
# ---------------------------------------------------------------------------

def scan(directory: str = ".", recursive: bool = True, **kwargs) -> Dict:
    """Scan a directory for HTML files with AI-SKILL-HEADERs."""
    args = ["scan", directory]
    if recursive:
        args.append("--recursive")
    for k, v in kwargs.items():
        args.extend([f"--{k.replace('_', '-')}", str(v)])
    return run(*args)


def header(file: str, section: Optional[int] = None) -> Dict:
    """Extract the AI-SKILL-HEADER from an HTML file."""
    args = ["header", file]
    if section is not None:
        args.extend(["--section", str(section)])
    return run(*args)


def read(file: str, start: Optional[int] = None, end: Optional[int] = None,
         head: Optional[int] = None, tail: Optional[int] = None) -> str:
    """Read lines from a file."""
    args = ["read", file]
    if start is not None:
        args.append(str(start))
    if end is not None:
        args.append(str(end))
    if head is not None:
        args.extend(["--head", str(head)])
    if tail is not None:
        args.extend(["--tail", str(tail)])
    return run(*args, json_output=False)


def locate(file: str, anchor: str, context: int = 0) -> Dict:
    """Locate a code anchor in the file."""
    args = ["locate", file, anchor, "--context", str(context)]
    return run(*args)


def apply(file: str, diff: str, dry_run: bool = False, backup: bool = False) -> Dict:
    """Apply a unified diff to a file."""
    args = ["apply", file, "--diff", diff]
    if dry_run:
        args.append("--dry-run")
    if backup:
        args.append("--backup")
    return run(*args)


def validate(file: str, fix: bool = False) -> Dict:
    """Validate header-to-code consistency."""
    args = ["validate", file]
    if fix:
        args.append("--fix")
    return run(*args)


def module(file: str, depth: int = 0) -> Dict:
    """Scan local ES module / resource dependencies."""
    args = ["module", file, "--depth", str(depth)]
    return run(*args)


def search(query: str, directory: str = ".", top: int = 5) -> Dict:
    """Search HTML files by query."""
    return run("search", query, "--dir", directory, "--top", str(top))


def page_screenshot(port: int = 9222, selector: Optional[str] = None,
                    output: Optional[str] = None) -> Dict:
    """Capture a page screenshot."""
    args = ["page", "screenshot", "--port", str(port)]
    if selector:
        args.extend(["--selector", selector])
    if output:
        args.extend(["--output", output])
    return run(*args)


def page_click(selector: str, port: int = 9222) -> Dict:
    """Click an element on the page."""
    return run("page", "click", selector, "--port", str(port))


def page_eval(expression: str, port: int = 9222) -> Dict:
    """Evaluate JavaScript in the page."""
    return run("page", "eval", expression, "--port", str(port))


def page_dom(port: int = 9222, selector: Optional[str] = None) -> Dict:
    """Get page DOM HTML."""
    args = ["page", "dom", "--port", str(port)]
    if selector:
        args.extend(["--selector", selector])
    return run(*args)


def page_console(port: int = 9222) -> Dict:
    """Get console log messages."""
    return run("page", "console", "--port", str(port))


def debug_start(file: str, port: int = 9222, headless: bool = True) -> Dict:
    """Start a browser with CDP debugging."""
    args = ["debug", "start", file, "--port", str(port)]
    if not headless:
        args.append("--no-headless")
    return run(*args)


def debug_stop(port: int = 9222) -> Dict:
    """Stop a browser session."""
    return run("debug", "stop", "--port", str(port))
