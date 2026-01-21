# mcpg

![CI](https://github.com/markphelps/mcpg/workflows/CI/badge.svg)

MCP (Model Context Protocol) client library in Rust.

## Features

- ✅ Stdio transport for MCP servers
- ✅ JSON-RPC 2.0 protocol implementation
- ✅ Tool listing and calling
- ✅ Automatic process lifecycle management
- 🚧 Resources (planned)
- 🚧 Prompts (planned)

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
