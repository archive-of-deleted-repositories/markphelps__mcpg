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
- ✅ Progressive tool discovery via search/inspect/call/list
- ✅ Upstream MCP client for stdio and HTTP servers
- ✅ Resources and prompts passthrough
- ✅ Cursor-based pagination for list operations
- ✅ Universal configuration layer
- ✅ Automatic process lifecycle management
- ✅ Install/uninstall commands for easy setup
- 🚧 BM25 search ranking (planned)

## Quick Start

The fastest way to get started is with `mcpg install`, which automatically detects your AI agent's MCP servers and configures mcpg as a gateway.

### 1. Run `mcpg install`

```bash
# Interactive setup - detects your agent and migrates servers
mcpg install

# Or skip prompts and select all servers
mcpg install --yes
```

This will:

1. Detect your AI agent configuration (supports Claude Code and OpenCode)
2. Show you all configured MCP servers across your config files
3. Let you select which servers to route through mcpg
4. Create `.mcpg.json` with your upstream server configs
5. Update your agent's config to use mcpg instead of the individual servers

### 2. Restart your AI agent

After install completes, restart your AI agent (e.g., restart Claude Code). The agent will now connect to mcpg and see 4 gateway tools instead of all upstream tools.

### Manual Setup

If you prefer to configure manually:

1. Create `.mcpg.json` in your project directory:

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

2. Point your AI agent's MCP config to mcpg:

```json
{
  "mcpServers": {
    "mcpg": {
      "command": "mcpg"
    }
  }
}
```

## Gateway Tools

mcpg exposes 4 gateway tools for progressive discovery:

| Tool      | Description                                    |
| --------- | ---------------------------------------------- |
| `search`  | Find tools matching a query across all servers |
| `inspect` | Get detailed schema for a specific tool        |
| `call`    | Execute a tool on an upstream server           |
| `list`    | List configured servers or tools on a server   |

### Resources and Prompts

mcpg passes through resources and prompts from upstream servers:

- **resources/list** and **resources/read** are forwarded to the appropriate upstream server
- **prompts/list** and **prompts/get** are forwarded to the appropriate upstream server
- Collision detection warns when multiple servers expose the same resource URI or prompt name

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

## CLI Commands

```bash
# Run the gateway server (default command)
mcpg
mcpg serve

# Install mcpg for a project
mcpg install

# Install options
mcpg install --dry-run    # Preview changes without writing files
mcpg install --yes        # Skip prompts, select all servers
mcpg install -y           # Short form of --yes
mcpg -v install           # Verbose debug output

# Uninstall mcpg from a project
mcpg uninstall

# Uninstall options
mcpg uninstall --dry-run  # Preview changes without writing files
mcpg uninstall --yes      # Skip confirmation prompts
mcpg uninstall -y         # Short form of --yes
mcpg -v uninstall         # Verbose debug output
```

### `mcpg install`

The `install` command bootstraps mcpg for a project by:

1. **Detecting your AI agent** - Finds supported agent configurations in the current project
2. **Discovering MCP servers** - Reads servers from all agent config sources, tracking which file each came from
3. **Server selection** - Interactive multi-select to choose which servers to route through mcpg (or `--yes` to select all)
4. **Creating `.mcpg.json`** - Writes the gateway config with your selected upstream servers
5. **Updating agent config** - Adds mcpg to your agent's MCP config and disables the migrated servers
6. **Creating backups** - Backs up any modified config files (`.backup` extension)

After running `install`, restart your AI agent for changes to take effect.

### `mcpg uninstall`

The `uninstall` command reverses the install process:

1. **Removing `.mcpg.json`** - Deletes the gateway configuration
2. **Restoring backups** - Restores agent config files from `.backup` files created during install

## Configuration

mcpg looks for its gateway configuration in the following order:

1. `$MCPG_CONFIG` environment variable
2. `./.mcpg.json` (project directory)
3. `~/.config/mcpg/config.json` (XDG user config)

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

## Supported Agents

<details>
<summary>Claude Code</summary>

`mcpg install` detects and migrates servers from Claude Code's config files, checked in priority order:

**Project-scoped:**

1. `.mcp.json`
2. `.claude/settings.local.json`

**User-scoped:**

3. `~/.claude.json`
4. `~/.claude/mcp_servers.json`

When migrating, mcpg adds itself to the highest-priority config source that contains servers, and disables the migrated servers via `disabledMcpServers` in each source file.

</details>

<details>
<summary>OpenCode</summary>

`mcpg install` detects and migrates servers from [OpenCode](https://github.com/sst/opencode)'s config files, checked in priority order:

1. `opencode.json` (project directory)
2. `~/.config/opencode/opencode.json` (global)

Both config files use the same flat structure with MCP servers under the `mcp` key. OpenCode supports `"local"` (stdio) and `"remote"` (HTTP) server types.

When migrating, mcpg adds itself to the highest-priority config source that contains servers, and disables the migrated servers by setting `enabled: false` on each server entry.

</details>

## Development

For information on building, testing, and contributing to mcpg, see [DEVELOPMENT.md](DEVELOPMENT.md).

## License

[MIT](License)
