#!/bin/sh
set -e

REPO="navicore/jdwp-mcp"
INSTALL_DIR="${JDWP_MCP_INSTALL_DIR:-/usr/local/bin}"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux)  OS_NAME="linux" ;;
    Darwin) OS_NAME="macos" ;;
    MINGW*|MSYS*|CYGWIN*) OS_NAME="windows" ;;
    *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)  ARCH_NAME="x86_64" ;;
    aarch64|arm64) ARCH_NAME="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${OS_NAME}-${ARCH_NAME}"

# Get latest release tag
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

if [ -z "$LATEST" ]; then
    echo "Failed to fetch latest release. Install manually:"
    echo "  cargo install --git https://github.com/${REPO}"
    exit 1
fi

echo "Installing jdwp-mcp ${LATEST} (${TARGET})..."

if [ "$OS_NAME" = "windows" ]; then
    URL="https://github.com/${REPO}/releases/download/${LATEST}/jdwp-mcp-${TARGET}.zip"
    TMPFILE=$(mktemp /tmp/jdwp-mcp.XXXXXX.zip)
    curl -fsSL "$URL" -o "$TMPFILE"
    unzip -o "$TMPFILE" -d "$INSTALL_DIR"
    rm "$TMPFILE"
else
    URL="https://github.com/${REPO}/releases/download/${LATEST}/jdwp-mcp-${TARGET}.tar.gz"
    curl -fsSL "$URL" | tar xz -C "$INSTALL_DIR"
fi

echo "Installed jdwp-mcp to ${INSTALL_DIR}/jdwp-mcp"
echo ""
echo "Next: configure your agent:"
echo "  claude mcp add jdwp jdwp-mcp"
