# Development Guide

This guide covers development setup, building, testing, and contributing to mcpg.

## Prerequisites

- **Rust** - 1.85+ (required for edition 2024)
- **Node.js** - 18+ (required for integration tests)
- **npm** - For installing development dependencies

The easiest way to install the correct versions is with [mise](https://mise.jdx.dev/):

```bash
mise install
```

This reads `mise.toml` and installs the pinned versions of Rust and Node.js automatically.

## Setup

1. Clone the repository:

```bash
git clone https://github.com/markphelps/mcpg.git
cd mcpg
```

2. Install tool versions (if using mise):

```bash
mise install
```

3. Install npm dependencies (for pre-commit hooks):

```bash
npm install
```

This sets up [Husky](https://typicode.github.io/husky/) pre-commit hooks that automatically run:

- `cargo fmt` - Auto-formats code
- `cargo clippy --fix` - Runs linter and auto-fixes issues

## Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run directly without installing
cargo run

# Run with arguments
cargo run -- install --dry-run
```

## Testing

### Running Tests

```bash
# Run all tests (unit + integration)
cargo test

# Unit tests only (no Node.js required)
cargo test --lib --bins

# Integration tests only (requires Node.js 18+)
cargo test --test integration

# CLI integration tests
cargo test --test cli

# Skip integration tests when Node.js is unavailable
SKIP_INTEGRATION_TESTS=1 cargo test

# Run tests with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

### Integration Tests

Integration tests use the official `@modelcontextprotocol/server-everything` MCP server via npx. These tests:

- Require Node.js 18+ to be installed
- Run with `--test-threads=1` in CI to avoid conflicts
- May be skipped locally using `SKIP_INTEGRATION_TESTS=1`

**Troubleshooting**: If integration tests hang, kill orphaned server processes:

```bash
pkill -f "server-everything"
```

See [tests/README.md](tests/README.md) for detailed testing documentation.

## Code Quality

### Formatting

```bash
# Check if code is formatted correctly
cargo fmt --check

# Auto-format all code
cargo fmt
```

### Linting

```bash
# Run clippy linter
cargo clippy

# Run clippy with warnings as errors (CI mode)
cargo clippy -- -D warnings

# Auto-fix clippy issues
cargo clippy --fix
```

### Pre-commit Hooks

The project uses Husky to run checks before each commit. After running `npm install`, every commit will automatically:

1. Format code with `cargo fmt`
2. Run clippy and auto-fix issues with `cargo clippy --fix`

This ensures code quality and consistency without manual intervention.

## Development Workflow

1. Create a feature branch from `main`
2. Make your changes
3. Run tests: `cargo test`
4. Pre-commit hooks will format and lint automatically
5. Commit your changes (include `-s` flag for sign-off)
6. Push and create a pull request

## Debugging

### Enable Debug Logging

```bash
# Run with debug output
RUST_LOG=debug cargo run

# Run specific command with verbose flag
cargo run -- -v install
```

### Testing JSON-RPC Manually

```bash
# Initialize connection
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"test","version":"1.0"},"capabilities":{}}}' | cargo run

# List gateway tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | cargo run

# Search for tools
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"query":"echo"}}}' | cargo run
```

See `docs/examples/gateway-demo.sh` for a complete working demo.

### Common Issues

**Integration tests hang**: Kill orphaned processes with `pkill -f "server-everything"`

**Node.js not found**: Skip integration tests with `SKIP_INTEGRATION_TESTS=1 cargo test`

**Pre-commit hooks fail**: Run `cargo fmt` and `cargo clippy --fix` manually

## Contributing

We welcome contributions! Please:

1. Follow the existing code style
2. Add tests for new functionality
3. Update documentation as needed
4. Ensure all tests pass before submitting PR
5. Sign your commits with `-s` flag

## Architecture Notes

### Key Implementation Details

- **Async runtime**: Tokio with multi-threaded runtime
- **Protocol version**: MCP 2025-06-18
- **Message format**: Newline-delimited JSON-RPC over stdio
- **Error handling**: `anyhow` with context chaining, `thiserror` for MCP errors
- **Request IDs**: UUID v4 for HTTP clients, sequential i64 for stdio
- **Search**: Reverse index with prefix matching, O(k) lookups
- **Pagination**: Cursor-based with safety limits (max 100 pages)

### Resources

- [MCP Protocol Specification](https://modelcontextprotocol.io/specification)
- [Rust Async Book](https://rust-lang.github.io/async-book/)
- [Tokio Documentation](https://tokio.rs/)
