#!/bin/sh
# sfhtml installer — downloads the latest release binary for Linux/macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/anyrust/sfhtml/main/install.sh | sh
# For Windows, use: irm https://raw.githubusercontent.com/anyrust/sfhtml/main/install.ps1 | iex

set -e

REPO="anyrust/sfhtml"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux)  OS_NAME="linux" ;;
    Darwin) OS_NAME="macos" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH_NAME="x86_64" ;;
    aarch64|arm64) ARCH_NAME="aarch64" ;;
    *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

ARCHIVE="sfhtml-${OS_NAME}-${ARCH_NAME}.tar.gz"

# Get latest release tag
echo "Fetching latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST" ]; then
    echo "Error: Could not determine latest release."
    echo "Install manually: cargo install sfhtml"
    exit 1
fi

URL="https://github.com/${REPO}/releases/download/${LATEST}/${ARCHIVE}"

echo "Downloading sfhtml ${LATEST} for ${OS_NAME}-${ARCH_NAME}..."
echo "  ${URL}"

# Download and extract
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"
tar xzf "${TMPDIR}/${ARCHIVE}" -C "$TMPDIR"

# Install
if [ -w "$INSTALL_DIR" ]; then
    mv "${TMPDIR}/sfhtml" "${INSTALL_DIR}/sfhtml"
else
    echo "Need sudo to install to ${INSTALL_DIR}"
    sudo mv "${TMPDIR}/sfhtml" "${INSTALL_DIR}/sfhtml"
fi

chmod +x "${INSTALL_DIR}/sfhtml"

echo ""
echo "sfhtml ${LATEST} installed to ${INSTALL_DIR}/sfhtml"
echo ""
echo "Verify: sfhtml --version"
