# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**mcpg** (MCP Gateway) is an MCP server that acts as a gateway to upstream MCP servers. It provides progressive tool discovery to minimize context token usage for AI agents.

**Key Architecture**: mcpg is itself an MCP server (not a CLI tool) that AI agents connect to. It then forwards tool calls to configured upstream MCP servers on-demand.

**Current Status**: MCP Gateway Server fully implemented with search/inspect/call/list tools, resources and prompts passthrough, cursor-based pagination, and comprehensive test coverage.

## CLI Commands

```bash
# Run gateway server (default command)
mcpg
mcpg serve

# Install mcpg for current project (interactive)
mcpg install

# Install with options
mcpg install --dry-run    # Show what would change without writing
mcpg install --yes        # Skip confirmation prompts
mcpg install -y           # Short form of --yes
mcpg install --verbose    # Show debug output
mcpg -v install           # Verbose flag is global

# Uninstall mcpg from current project
mcpg uninstall

# Uninstall with options
mcpg uninstall --dry-run    # Show what would change without writing
mcpg uninstall --yes        # Skip confirmation prompts
mcpg uninstall -y           # Short form of --yes
mcpg uninstall --verbose    # Show debug output
mcpg -v uninstall           # Verbose flag is global

# Check status
mcpg status

# Status with options
mcpg status --detail       # Show detailed configuration info
mcpg status --verbose      # Show debug output
mcpg -v status             # Verbose flag is global
```

### Install Command

The `install` command bootstraps mcpg for a project by:
1. Detecting AI agent configurations (currently Claude Code)
2. Reading MCP servers from all config sources
3. Creating `.mcpg.json` with upstream server configs
4. Adding mcpg to the agent's config (in the same file where servers are defined)
5. Disabling migrated servers in their source configs (via `disabledMcpServers`)
6. Creating backup files before modifying any configs

### Status Command

The `status` command checks if mcpg is configured for the current project:
1. Detects AI agent configurations
2. Checks for `.mcpg.json` and mcpg in agent config
3. Shows install status, agent name, and server count
4. With `--detail`: shows upstream servers, agent config path, and backups

### Uninstall Command

The `uninstall` command removes mcpg from a project by:
1. Removing `.mcpg.json`
2. Restoring backup files (original configs from before install)

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

# CLI integration tests
cargo test --test cli

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
│  + resources/prompts passthrough                    │
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
├── main.rs           # Entry point, CLI dispatch
├── lib.rs            # Library exports
├── telemetry.rs      # OpenTelemetry setup
├── cli/              # CLI commands
│   ├── mod.rs        # Clap CLI definitions
│   ├── install.rs    # Install command implementation
│   ├── status.rs     # Status command implementation
│   ├── uninstall.rs  # Uninstall command implementation
│   └── style.rs      # Terminal styling utilities
├── agents/           # AI agent configuration management
│   ├── mod.rs        # Agent trait, discovery
│   ├── claude_code.rs # Claude Code agent implementation
│   ├── config_file.rs # Generic JSON config file helper
│   └── registry.rs   # Agent registration system
├── server/           # MCP server implementation
│   ├── mod.rs        # GatewayServer, ToolCache
│   ├── handler.rs    # Tool call handlers (search/inspect/call/list)
│   ├── stdio.rs      # Stdio transport, JSON-RPC routing, MCP request handlers
│   ├── search.rs     # Reverse index search implementation
│   └── tools.rs      # Gateway tool schema definitions
├── config/           # Configuration management
│   ├── types.rs      # Config, ServerConfig, TransportConfig
│   └── loader.rs     # Config discovery and validation
└── mcp/              # MCP client for upstream servers
    ├── mod.rs         # Module exports
    ├── protocol.rs    # JSON-RPC 2.0 types, MCP protocol types
    ├── client.rs      # McpClient trait, create_client() factory
    ├── stdio.rs       # StdioClient implementation
    ├── http.rs        # HttpClient implementation (Streamable HTTP)
    └── error.rs       # MCP error types
```

### Key Types

**Server Side (mcpg as MCP server)**:
- **GatewayServer**: Main server state, manages upstream connections
- **ToolCache**: Reverse-index search with LRU caching
- **Tools**: `search`, `inspect`, `call`, `list`
- **Resources passthrough**: `resources/list`, `resources/read` forwarded to upstream
- **Prompts passthrough**: `prompts/list`, `prompts/get` forwarded to upstream

**Client Side (mcpg connecting to upstream)**:
- **McpClient trait**: `initialize()`, `list_tools()`, `call_tool()`, `list_resources()`, `read_resource()`, `list_prompts()`, `get_prompt()`, `shutdown()`
- **StdioClient**: Spawns subprocess, JSON-RPC over stdin/stdout
- **HttpClient**: MCP Streamable HTTP client with session management
- **TransportConfig**: `Stdio { command, args, env }` or `Http { url, headers }`
- **create_client()**: Factory function that creates the right client from ServerConfig

**Agent System (for install/uninstall commands)**:
- **Agent trait**: Interface for AI agent config management
- **ClaudeCodeAgent**: Reads/writes Claude Code MCP configs
- **ConfigLevel**: Project vs User scoped configs
- **ServerWithSource**: Tracks which config file each server came from

### Claude Code Config Sources (priority order, highest first)

1. `.mcp.json` (project directory)
2. `.claude/settings.local.json` (project directory)
3. `~/.claude/settings.local.json` (user home)
4. `~/.claude/settings.json` (user home)
5. `~/.claude.json` (user home)
6. `~/.claude/mcp_servers.json` (user home)

### Config Discovery Order

1. `$MCPG_CONFIG` environment variable
2. `./.mcpg.json` (project-level)
3. `~/.config/mcpg/config.json` (XDG user-level)

## Testing

Integration tests use the official `@modelcontextprotocol/server-everything` via npx. Tests are run with `--test-threads=1` in CI to avoid conflicts.

If tests hang, kill orphaned servers: `pkill -f "server-everything"`

## Key Implementation Details

- **Async runtime**: Tokio with multi-threaded runtime
- **Protocol version**: "2025-06-18"
- **Message format**: Newline-delimited JSON-RPC over stdio
- **Error handling**: anyhow with context chaining, thiserror for MCP errors
- **Request IDs**: UUID v4 for HTTP clients, sequential i64 for stdio
- **Search**: Reverse index with prefix matching, O(k) lookups
- **Pagination**: Cursor-based pagination with safety limits (max 100 pages) for tools, resources, and prompts
- **Resources/Prompts**: Passthrough to upstream servers with collision detection

## Resources

- [MCP Protocol Specification](https://modelcontextprotocol.io/specification)
