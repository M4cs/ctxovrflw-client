#!/usr/bin/env bash
set -euo pipefail

# ctxovrflw setup script for OpenClaw skill installation
# Installs the daemon (if needed) and runs init for the current agent

INSTALL_URL="https://ctxovrflw.dev/install.sh"

echo "=== ctxovrflw setup ==="

# Check if ctxovrflw is already installed
if command -v ctxovrflw &>/dev/null; then
    echo "✓ ctxovrflw already installed: $(ctxovrflw version 2>/dev/null || echo 'unknown version')"
else
    echo "Installing ctxovrflw..."
    curl -fsSL "$INSTALL_URL" | sh
    
    # Verify installation
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

echo ""
echo "Setup complete. The agent can now use ctxovrflw MCP tools for persistent memory."
echo "Run 'ctxovrflw init' to configure additional AI agents."
