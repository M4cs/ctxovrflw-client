#!/usr/bin/env bash
set -euo pipefail

# ctxovrflw setup script for OpenClaw skill installation
# Installs the daemon (if needed), configures mcporter MCP connection

INSTALL_URL="https://ctxovrflw.dev/install.sh"
MCP_URL="http://localhost:7437/mcp/sse"

echo "=== ctxovrflw setup ==="

# Check if ctxovrflw is already installed
if command -v ctxovrflw &>/dev/null; then
    echo "✓ ctxovrflw already installed: $(ctxovrflw version 2>/dev/null || echo 'unknown version')"
else
    echo "Installing ctxovrflw..."
    curl -fsSL "$INSTALL_URL" | sh
    
    if ! command -v ctxovrflw &>/dev/null; then
        echo "✗ Installation failed. Please install manually: curl -fsSL $INSTALL_URL | sh"
        exit 1
    fi
    echo "✓ ctxovrflw installed: $(ctxovrflw version 2>/dev/null || echo 'unknown version')"
fi

# Check if daemon is running
if ctxovrflw status &>/dev/null; then
    echo "✓ Daemon is running"
else
    echo "Starting daemon..."
    ctxovrflw daemon &
    sleep 2
    if ctxovrflw status &>/dev/null; then
        echo "✓ Daemon started"
    else
        echo "⚠ Daemon may not have started. Run 'ctxovrflw daemon' manually if needed."
    fi
fi

# Configure mcporter MCP connection
if command -v mcporter &>/dev/null; then
    echo "Configuring mcporter → ctxovrflw MCP server..."
    mcporter config add ctxovrflw --url "$MCP_URL" 2>/dev/null || true
    echo "✓ mcporter configured (ctxovrflw → $MCP_URL)"
else
    echo "⚠ mcporter not found. Install it: npm install -g mcporter"
    echo "  Then run: mcporter config add ctxovrflw --url $MCP_URL"
fi

echo ""
echo "Setup complete. ctxovrflw MCP tools are now available via mcporter."
echo "Run 'ctxovrflw init' to configure additional AI agents."
