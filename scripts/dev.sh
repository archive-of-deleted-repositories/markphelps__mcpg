#!/usr/bin/env bash
# Run mcpg with example config (or personal override)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Use personal config if exists, otherwise example
if [[ -f "$REPO_ROOT/mcp.local.json" ]]; then
    export MCPG_CONFIG="$REPO_ROOT/mcp.local.json"
    echo "Using personal config: mcp.local.json" >&2
else
    export MCPG_CONFIG="$REPO_ROOT/examples/mcp.json"
    echo "Using example config: examples/mcp.json" >&2
fi

# Optional: enable debug logging
export RUST_LOG="${RUST_LOG:-mcpg=debug}"

# Log stderr to timestamped file
LOG_FILE="/tmp/mcpg-$(date +%s).log"
echo "mcpg logging to: $LOG_FILE" >&2

# Run mcpg and tee stderr to both the log file and stderr
cargo run --manifest-path "$REPO_ROOT/Cargo.toml" 2> >(tee -a "$LOG_FILE" >&2)
