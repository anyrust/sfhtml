"""CLI entry point — delegates to the sfhtml Rust binary."""

import sys
import os
import shutil
import subprocess
from pathlib import Path


def _find_binary() -> str:
    pkg_dir = Path(__file__).parent
    for candidate in [pkg_dir / "sfhtml", pkg_dir / "sfhtml.exe"]:
        if candidate.exists():
            return str(candidate)
    found = shutil.which("sfhtml")
    if found:
        return found
    cargo_bin = Path.home() / ".cargo" / "bin" / "sfhtml"
    if cargo_bin.exists():
        return str(cargo_bin)
    print(
        "Error: sfhtml binary not found.\n"
        "Install with: cargo install sfhtml\n"
        "Or download from: https://github.com/anyrust/sfhtml/releases",
        file=sys.stderr,
    )
    sys.exit(1)


def main():
    binary = _find_binary()
    result = subprocess.run([binary] + sys.argv[1:])
    sys.exit(result.returncode)


if __name__ == "__main__":
    main()
