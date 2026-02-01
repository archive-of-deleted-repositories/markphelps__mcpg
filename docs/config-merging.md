# Config Merging

mcpg reads MCP server configurations from multiple Claude Code config files and merges them into a single `.mcpg.json` gateway config. This document describes how that merging works.

## Config Sources

Config sources are checked in priority order (highest first):

**Project-scoped:**

1. `.mcp.json` — top-level `mcpServers` object
2. `.claude/settings.local.json` — top-level `mcpServers` object

**User-scoped:**

3. `~/.claude.json` — servers nested under `projects.<project_path>.mcpServers`
4. `~/.claude/mcp_servers.json` — servers nested under `projects.<project_path>.mcpServers`

### Config Formats

**Project-level** files have `mcpServers` at the top level:

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["@modelcontextprotocol/server-github"]
    }
  },
  "disabledMcpServers": []
}
```

**User-level** files nest servers under a project path key:

```json
{
  "projects": {
    "/path/to/project": {
      "mcpServers": {
        "slack": {
          "command": "npx",
          "args": ["@anthropic/mcp-server-slack"]
        }
      },
      "disabledMcpServers": []
    }
  }
}
```

## Merging Algorithm

1. Sources are processed in **reverse priority order** (lowest priority first)
2. Each server is added to a map keyed by name
3. Higher-priority sources **overwrite** same-named servers from lower-priority sources
4. `disabledMcpServers` arrays are collected from **all** sources
5. After all sources are processed, disabled servers are removed from the final result

### Name Collision Resolution

When the same server name appears in multiple config files, the **higher-priority source wins**. The lower-priority version is silently dropped.

Priority within levels:
- Project-level always wins over user-level
- `.mcp.json` wins over `.claude/settings.local.json`
- `~/.claude.json` wins over `~/.claude/mcp_servers.json`

### Disabled Servers

`disabledMcpServers` from **any** source removes that server from the final result, regardless of where the server was originally defined. For example, a user-level config can disable a project-level server.

## Install Behavior

When `mcpg install` runs:

1. **Collect** — All servers from all config sources are merged (as described above)
2. **Select** — User chooses which servers to migrate (or all with `--yes`)
3. **Write** — Selected servers are written to `.mcpg.json`
4. **Disable** — Each migrated server is disabled via `disabledMcpServers` in its original source file
5. **Add mcpg** — The `mcpg` server entry is added to the **highest-priority source** that contained servers. If servers only exist in user-level configs, mcpg is added there.

### Example: Servers Across Multiple Sources

Given:
- `.mcp.json` has `github`
- `~/.claude.json` has `slack` for this project

After `mcpg install --yes`:
- `.mcpg.json` contains both `github` and `slack`
- `.mcp.json` has `github` in `disabledMcpServers` and `mcpg` added to `mcpServers`
- `~/.claude.json` has `slack` in `disabledMcpServers` for this project (mcpg is not added here since it's already in the higher-priority project config)
