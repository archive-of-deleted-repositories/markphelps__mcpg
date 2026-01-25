# mcpg

![CI](https://github.com/markphelps/mcpg/workflows/CI/badge.svg)

**MCP Gateway** - A lightweight MCP server that acts as a gateway to upstream MCP servers with progressive tool discovery.

## What is mcpg?

`mcpg` is an MCP (Model Context Protocol) server that sits between AI agents (like Claude) and your collection of MCP servers. Instead of loading all tool definitions upfront (consuming thousands of context tokens), it provides progressive discovery through a search-first workflow.

### The Problem

Traditional MCP integration loads all tool schemas at startup:
- 5 MCP servers × ~3,500 tokens each = **~18,000 tokens**
- Severely limits context available for actual work
- Wasteful when you only need 1-2 tools per task

### The Solution

`mcpg` acts as an MCP gateway server that exposes discovery tools:
- **search** - Find relevant tools across all servers (~50 tokens)
- **inspect** - Get schema for specific tool (~150 tokens)
- **call** - Execute a tool on the upstream server
- **list** - List available servers/tools

**Result**: 99% reduction in context token usage

## Features

- ✅ MCP server implementation (stdio transport)
- ✅ Progressive tool discovery via search/inspect/call
- ✅ Upstream MCP client for stdio and HTTP servers
- ✅ Universal configuration layer
- ✅ Automatic process lifecycle management
- ✅ HTTP transport for upstream servers
- 🚧 BM25 search ranking (planned)
- 🚧 Resources and prompts (planned)

## Quick Start

### 1. Configure upstream servers

Create `mcp.json` in your project directory:

```json
{
  "servers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"]
    },
    "slack": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-slack"]
    }
  }
}
```

### 2. Run mcpg

```bash
# mcpg reads from stdin and writes to stdout (MCP stdio transport)
cargo run
```

### 3. Connect your AI agent

Configure your AI agent to use mcpg as its MCP server. The agent will see 4 gateway tools instead of all upstream tools.

## Gateway Tools

mcpg exposes 4 gateway tools for progressive discovery:

| Tool | Description |
|------|-------------|
| `search` | Find tools matching a query across all servers |
| `inspect` | Get detailed schema for a specific tool |
| `call` | Execute a tool on an upstream server |
| `list` | List configured servers or tools on a server |

### Example Workflow

```
AI Agent                      mcpg                     Upstream Servers
    │                          │                              │
    │ tools/list               │                              │
    │─────────────────────────>│                              │
    │ [search, inspect,        │                              │
    │  call, list]             │                              │
    │<─────────────────────────│                              │
    │                          │                              │
    │ call search(query="file")│                              │
    │─────────────────────────>│  initialize()                │
    │                          │─────────────────────────────>│
    │                          │  tools/list                  │
    │                          │─────────────────────────────>│
    │                          │<─────────────────────────────│
    │ [github] create_file     │                              │
    │ [github] read_file       │                              │
    │<─────────────────────────│                              │
    │                          │                              │
    │ call inspect(github,     │                              │
    │              create_file)│                              │
    │─────────────────────────>│                              │
    │ {name, description,      │                              │
    │  inputSchema}            │                              │
    │<─────────────────────────│                              │
    │                          │                              │
    │ call call(github,        │  tools/call create_file      │
    │           create_file,   │─────────────────────────────>│
    │           {path, content})│                              │
    │                          │<─────────────────────────────│
    │ {result}                 │                              │
    │<─────────────────────────│                              │
```

### JSON-RPC Examples

```bash
# Initialize connection
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"test","version":"1.0"},"capabilities":{}}}' | cargo run

# List gateway tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | cargo run

# Search for tools
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"query":"echo"}}}' | cargo run

# Call an upstream tool
echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"call","arguments":{"server":"everything","tool":"echo","args":{"message":"hello"}}}}' | cargo run
```

See `docs/examples/gateway-demo.sh` for a complete working demo.

## Configuration

mcpg looks for configuration in the following order:

1. `$MCPG_CONFIG` environment variable
2. `./mcp.json` (project directory)
3. `~/.config/mcp/servers.json` (user config)

### Config Format

```json
{
  "servers": {
    "server-name": {
      "command": "executable",
      "args": ["arg1", "arg2"],
      "env": {
        "API_KEY": "your-key"
      }
    }
  }
}
```

## Transport Types

mcpg supports two transport types for connecting to upstream MCP servers:

### Stdio Transport

Runs MCP servers as local subprocesses communicating over stdin/stdout.

```json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "./data"],
      "env": {
        "API_KEY": "optional-env-var"
      },
      "timeout_secs": 30
    }
  }
}
```

### HTTP Transport

Connects to remote MCP servers over HTTP using the Streamable HTTP protocol (MCP 2025-06-18).

```json
{
  "servers": {
    "remote-api": {
      "url": "https://api.example.com/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY"
      },
      "timeout_secs": 60
    }
  }
}
```

**Configuration Options:**
- `timeout_secs` (optional): Request timeout in seconds (default: 30)
- `headers` (optional): Custom HTTP headers to send with each request
- `url`: The MCP endpoint URL (must support Streamable HTTP)

**Protocol Details:**
- Uses MCP Streamable HTTP specification (2025-06-18)
- Single endpoint for all operations (POST to URL)
- Session management via `Mcp-Session-Id` header
- No server-initiated messages in v1 (GET SSE stream not implemented)

## Testing

### Prerequisites

Integration tests require Node.js (version 18+) to run the official MCP everything server.

### Running Tests

```bash
# All tests (unit + integration)
cargo test

# Unit tests only
cargo test --lib --bins

# Integration tests only
cargo test --test integration

# Skip integration tests (if Node.js not available)
SKIP_INTEGRATION_TESTS=1 cargo test
```

See [tests/README.md](tests/README.md) for more details.

## Development

```bash
# Check formatting
cargo fmt --check

# Run linter
cargo clippy

# Build
cargo build

# Run
cargo run
```

## License

[Add your license here]
