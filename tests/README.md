# Integration Tests

This directory contains integration tests for the MCP stdio client implementation.

## Prerequisites

Integration tests require:

- **Node.js** (version 18 or higher)
- **npm** or **npx** (comes with Node.js)

The tests use the official MCP everything server: `@modelcontextprotocol/server-everything`

## Running Tests

### All Tests (Unit + Integration)

```bash
cargo test
```

### Unit Tests Only

```bash
cargo test --lib --bins
```

### Integration Tests Only

```bash
cargo test --test integration
```

### Run with Output

```bash
cargo test --test integration -- --nocapture
```

### Skip Integration Tests

If Node.js is not available, you can skip integration tests:

```bash
SKIP_INTEGRATION_TESTS=1 cargo test
```

## Test Structure

```
tests/
├── integration/
│   ├── mod.rs           # Integration test module
│   ├── helpers.rs       # Test helper functions
│   └── stdio_tests.rs   # StdioClient integration tests
└── README.md            # This file
```

## Integration Test Coverage

The integration tests verify:

1. **Initialization**: Protocol handshake and capability negotiation
2. **Tool Listing**: Retrieving available tools from the server
3. **Tool Calling**: Executing tools with arguments
4. **Caching**: Initialization result caching
5. **Lifecycle**: Shutdown and cleanup
6. **Error Handling**: Invalid commands and error scenarios
7. **Process Management**: Drop trait cleanup

## Process Cleanup

Integration tests spawn MCP server processes. Cleanup is handled automatically through:

- **Explicit cleanup**: `cleanup_client()` helper in each test
- **Drop trait**: Automatic cleanup when client goes out of scope
- **Graceful shutdown**: stdin close → kill signal → wait for exit

## CI/CD

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs:

- **Unit tests**: Fast feedback with linting and formatting
- **Integration tests**: Full MCP protocol testing

## Troubleshooting

### Tests Hang or Timeout

If tests hang, there may be orphaned processes:

```bash
pkill -f "server-everything"
cargo test --test integration
```

### Node.js Not Found

Install Node.js:

```bash
# macOS
brew install node

# Ubuntu/Debian
sudo apt-get install nodejs npm

# Or skip integration tests
SKIP_INTEGRATION_TESTS=1 cargo test
```

### Everything Server Not Found

The tests use npx to download the server automatically. If you want to pre-install:

```bash
npm install -g @modelcontextprotocol/server-everything@latest
```

## Writing New Tests

When adding new integration tests:

1. Use the `skip_if_no_nodejs!()` macro at the start
2. Use helper functions from `helpers.rs`:
   - `create_everything_client()` - Create client with everything server
   - `init_and_verify()` - Initialize and verify connection
   - `cleanup_client()` - Clean up after test
3. Always call `cleanup_client()` at the end of tests
4. Run tests sequentially if they might interfere: `cargo test --test integration -- --test-threads=1`

Example test:

```rust
#[tokio::test]
async fn test_my_feature() {
    skip_if_no_nodejs!();

    let mut client = create_everything_client();
    init_and_verify(&mut client).await.unwrap();

    // Your test code here

    cleanup_client(&mut client).await;
}
```
