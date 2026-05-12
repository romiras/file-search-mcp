# Commands and Techniques - File Search MCP

This guide provides a reference for common commands and technical patterns used in the development and troubleshooting of the File Search MCP server.

## 🛠️ Build and Development Commands

### Compilation
Always prefer `--release` for production use due to heavy I/O and indexing logic.
```bash
# Standard debug build
cargo build

# Optimized release build (highly recommended)
cargo build --release

# Continuous check (fast feedback)
cargo check
```

### Testing and Quality
```bash
# Run all unit tests
cargo test

# Run linter (Clippy)
cargo clippy

# Format code
cargo fmt
```

## 🔍 Manual MCP Interaction (Terminal)

Since MCP servers communicate via JSON-RPC over `stdio`, you can simulate a client handshake and tool call manually to verify behavior.

### 1. Perform a Search via Terminal
This sequence handles the `initialize` handshake before calling the `search` tool.
```bash
(
  echo '{"jsonrpc": "2.0", "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test-client", "version": "1.0.0"}}, "id": 1}'
  sleep 0.1
  echo '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
  sleep 0.1
  echo '{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "search", "arguments": {"directory": ".", "keyword": "rust"}}, "id": 2}'
  sleep 0.5
) | ./target/release/file-search-mcp
```

### 2. Read a File via Terminal
```bash
(
  echo '{"jsonrpc": "2.0", "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test-client", "version": "1.0.0"}}, "id": 1}'
  sleep 0.1
  echo '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
  sleep 0.1
  echo '{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "read_file_content", "arguments": {"file_path": "README.md"}}, "id": 2}'
) | ./target/release/file-search-mcp
```

## 🐞 Troubleshooting and Logging

The server uses the `tracing` crate. Logs are sent to `stderr` so they don't break the MCP protocol on `stdout`.

### Enable Debug Logs
```bash
# Run with info level logging
RUST_LOG=info ./target/release/file-search-mcp

# Run with detailed debug logging (shows skipped files, etc.)
RUST_LOG=debug ./target/release/file-search-mcp
```

## 🏗️ Technical Techniques

### 1. Persistent & Incremental Indexing (Tantivy)
We use persistent on-disk indexing in `~/.cache/file-search-mcp/`.
- **Unique IDs**: Each search path is hashed using `Sha256` to create a unique index subdirectory.
- **Incremental Logic**: We compare the filesystem `mtime` with the `modified` field stored in the index.
- **Cleanup**: Files that no longer exist in the filesystem are automatically removed from the index during each search scan.

### 2. Binary Detection (Content Sniffing)
To avoid indexing non-text files, we use a two-step check:
1. **Extension Blacklist**: Quickly skip `.exe`, `.png`, `.zip`, etc.
2. **Byte Sniffing**: Read the first 8KB. If it contains `NULL` bytes or a high ratio of non-printable control characters, it is treated as binary and skipped.

### 3. Performance Exclusions
To keep search fast, the server automatically ignores these large/internal directories:
- `.git`
- `target`
- `node_modules`
- `.vscode`
- `build` / `dist`

### 4. Precision Timing
The `search` tool outputs detailed timing metrics to the logs:
- **Indexing Duration**: Time spent scanning and building the RAM index.
- **Commit Duration**: Time spent finalizing the Tantivy index.
- **Search Duration**: Time spent executing the actual query.
- **Total Time**: Full request lifecycle.
