# Development Testing Setup

This directory provides a ready-to-use configuration for testing mcpg with real MCP servers.

## Quick Start

### 1. Run mcpg with the example config

```bash
./scripts/dev.sh
```

Or manually:

```bash
MCPG_CONFIG=examples/mcp.json cargo run
```

### 2. Test the gateway

Once running, mcpg listens on stdio for MCP protocol messages. You can test it by connecting with an MCP client.

## Configuration

### Example Config (`mcp.json`)

The example config uses the `@modelcontextprotocol/server-filesystem` server pointed at the `sandbox/` directory. This provides real file operations for testing:

- `read_file` - Read file contents
- `write_file` - Create/overwrite files
- `list_directory` - List directory contents
- `search_files` - Search for files by pattern
- And more...

### Personal Config Override

To test with your own servers (GitHub, Slack, etc.) without committing secrets:

1. Create `mcp.local.json` in the repo root
2. Add your server configurations
3. Run `./scripts/dev.sh` - it will automatically use your personal config

Example `mcp.local.json`:

```json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "./examples/sandbox"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "ghp_your_token_here"
      }
    }
  }
}
```

The `mcp.local.json` file is gitignored to prevent accidental secret commits.

## Claude Desktop Integration

To test mcpg with Claude Desktop, add it to your Claude configuration:

### Option 1: Using the dev script

```json
{
  "mcpServers": {
    "mcpg": {
      "command": "/path/to/mcpg/scripts/dev.sh"
    }
  }
}
```

### Option 2: Direct cargo run

```json
{
  "mcpServers": {
    "mcpg": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/path/to/mcpg/Cargo.toml"],
      "env": {
        "MCPG_CONFIG": "/path/to/mcpg/examples/mcp.json",
        "RUST_LOG": "mcpg=info"
      }
    }
  }
}
```

## Sandbox Directory

The `sandbox/` directory is where the filesystem server has access. Files created here during testing are gitignored (except `.gitkeep`).

## Troubleshooting

### "Node.js not found" or npx errors

The filesystem server requires Node.js 18+. Install it via:
- macOS: `brew install node`
- Linux: Use your package manager or [nvm](https://github.com/nvm-sh/nvm)

### Permission errors

Ensure the `sandbox/` directory exists and is writable:

```bash
mkdir -p examples/sandbox
```

### Debug logging

Enable verbose output:

```bash
RUST_LOG=mcpg=debug ./scripts/dev.sh
```

### Viewing logs

The `dev.sh` script automatically logs all stderr output to timestamped files in `/tmp/`.

**Find the current log file:**

```bash
ls -lt /tmp/mcpg-*.log | head -1
```

**Tail logs in real-time:**

```bash
tail -f $(ls -t /tmp/mcpg-*.log | head -1)
```

**Log levels:**

- `RUST_LOG=mcpg=info` - Production monitoring (tool call forwarding only)
- `RUST_LOG=mcpg=debug` - Development/troubleshooting (includes cache operations, searches)
- `RUST_LOG=mcpg=trace` - Very verbose (includes all internal details)

**What you'll see:**

- Upstream server connections and initialization
- Tool cache population and searches
- Request/response lifecycle with unique IDs
- Tool call forwarding to upstream servers
- Timing and performance information

### Test that mcpg loads correctly

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}' | MCPG_CONFIG=examples/mcp.json cargo run 2>/dev/null
```

You should see a JSON response with mcpg's capabilities.
