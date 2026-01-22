# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**mcpg** (MCP Gateway) is an MCP server that acts as a gateway to upstream MCP servers. It provides progressive tool discovery to minimize context token usage for AI agents.

**Key Architecture**: mcpg is itself an MCP server (not a CLI tool) that AI agents connect to. It then forwards tool calls to configured upstream MCP servers on-demand.

**Current Status**: MCP Gateway Server fully implemented with search/inspect/call/list tools. Includes reverse-index search, LRU caching, and comprehensive test coverage.

## Build & Test Commands

```bash
# Build
cargo build

# Run all tests (unit + integration)
cargo test

# Unit tests only
cargo test --lib --bins

# Integration tests only (requires Node.js 18+)
cargo test --test integration

# Skip integration tests when Node.js unavailable
SKIP_INTEGRATION_TESTS=1 cargo test

# Integration tests with verbose output
cargo test --test integration -- --nocapture

# Formatting and linting (run before commits, or use pre-commit hooks)
cargo fmt
cargo clippy -- -D warnings
```

## Pre-commit Hooks

The project uses Husky for pre-commit hooks that automatically run on every commit:
- `cargo fmt` - Auto-formats code
- `cargo clippy --fix` - Runs clippy and auto-fixes issues

Setup: `npm install` (installs husky)

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              AI Agent (Claude)                      │
│            Connects to mcpg via stdio               │
└────────────────────┬────────────────────────────────┘
                     │ MCP protocol
                     ▼
┌─────────────────────────────────────────────────────┐
│              mcpg (MCP Server)                      │
│  Tools: search, inspect, call, list                 │
│  - Progressive tool discovery                       │
│  - Forwards calls to upstream servers               │
└────────────────────┬────────────────────────────────┘
                     │ Lazy connections
         ┌───────────┼───────────┐
         ▼           ▼           ▼
    ┌────────┐  ┌────────┐  ┌────────┐
    │ GitHub │  │ Slack  │  │ Sentry │
    │  MCP   │  │  MCP   │  │  MCP   │
    └────────┘  └────────┘  └────────┘
    Upstream MCP Servers (stdio/http)
```

### Code Structure

```
src/
├── main.rs           # MCP server entry point (stdio)
├── lib.rs            # Library exports
├── server/           # MCP server implementation
│   ├── mod.rs        # GatewayServer, ToolCache
│   ├── handler.rs    # Request handlers (search/inspect/call/list)
│   ├── stdio.rs      # Stdio transport and JSON-RPC routing
│   ├── search.rs     # Reverse index search implementation
│   └── tools.rs      # Gateway tool schema definitions
├── config/           # Configuration management
│   ├── types.rs      # Config, ServerConfig, TransportConfig
│   └── loader.rs     # Config discovery and validation
└── mcp/              # MCP client for upstream servers
    ├── protocol.rs   # JSON-RPC 2.0 types, MCP protocol types
    ├── client.rs     # McpClient trait (async interface)
    └── stdio.rs      # StdioClient implementation
```

### Key Types

**Server Side (mcpg as MCP server)**:
- **GatewayServer**: Main server state, manages upstream connections
- **ToolCache**: Reverse-index search with LRU caching
- **Tools**: `search`, `inspect`, `call`, `list`

**Client Side (mcpg connecting to upstream)**:
- **McpClient trait**: `initialize()`, `list_tools()`, `call_tool()`, `shutdown()`
- **StdioClient**: Spawns subprocess, JSON-RPC over stdin/stdout
- **TransportConfig**: `Stdio { command, args, env }` or `Http { url, headers }`

### Config Discovery Order

1. `$MCPG_CONFIG` environment variable
2. `./mcp.json` (project-level)
3. `~/.config/mcp/servers.json` (XDG user-level)

## Testing

Integration tests use the official `@modelcontextprotocol/server-everything` via npx. Tests are run with `--test-threads=1` in CI to avoid conflicts.

If tests hang, kill orphaned servers: `pkill -f "server-everything"`

## Key Implementation Details

- **Async runtime**: Tokio with multi-threaded runtime
- **Protocol version**: "2025-06-18"
- **Message format**: Newline-delimited JSON-RPC over stdio
- **Error handling**: anyhow with context chaining
- **Request IDs**: Atomic i64 counter, sequential
- **Search**: Reverse index with prefix matching, O(k) lookups

## Resources

- [MCP Protocol Specification](https://modelcontextprotocol.io/specification)
