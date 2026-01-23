# MCP Gateway Usage

You are connected to mcpg, an MCP gateway that provides access to multiple upstream MCP servers through progressive tool discovery.

## Available Tools

You have 4 meta-tools (not the upstream tools directly):

| Tool | Purpose | Example |
|------|---------|---------|
| `search` | Find tools by keyword | `search(query: "github issues")` |
| `inspect` | Get full schema for a tool | `inspect(server: "github", tool: "list_issues")` |
| `call` | Execute a tool | `call(server: "github", tool: "list_issues", args: {...})` |
| `list` | List servers or tools | `list()` or `list(server: "github")` |

## Workflow

**Always follow: search -> inspect -> call**

1. **Search first** - Don't guess tool names. Use `search` to find relevant tools.
2. **Inspect before calling** - Use `inspect` to see required/optional parameters.
3. **Call with correct args** - Use `call` with the server, tool, and arguments.

## Example

User: "List the files in the sandbox directory"

1. `search(query: "list directory")` -> finds `[filesystem] list_directory`
2. `inspect(server: "filesystem", tool: "list_directory")` -> shows required param: path
3. `call(server: "filesystem", tool: "list_directory", args: {path: "."})`

User: "Create a new file called hello.txt"

1. `search(query: "write file")` -> finds `[filesystem] write_file`
2. `inspect(server: "filesystem", tool: "write_file")` -> shows required params: path, content
3. `call(server: "filesystem", tool: "write_file", args: {path: "hello.txt", content: "Hello, world!"})`

## Why This Pattern?

The gateway uses progressive discovery to minimize context token usage. Instead of loading 100+ tool schemas upfront, you discover only what you need.
