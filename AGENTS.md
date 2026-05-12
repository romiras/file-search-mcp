# Project Context & Instructions

## Project Overview
**File Search MCP** is a specialized Model Context Protocol (MCP) server built with Rust that provides high-performance full-text search capabilities within a filesystem using the **Tantivy** search engine.

### Key Technologies
- **Rust (2024 Edition)**: High-performance core logic.
- **Tantivy**: Full-text search engine (Rust-native).
- **RMCP (0.1.5)**: Rust SDK for Model Context Protocol.
- **Tokio**: Asynchronous runtime for I/O efficiency.

## 📁 Important Project Files
- **`docs/ARCHITECTURE.md`**: Detailed rationale for key architectural decisions (Tantivy vs. others, in-memory indexing, etc.).
- **`docs/COMMANDS_AND_TECHNIQUES.md`**: Comprehensive reference for build commands, manual MCP interaction via terminal, and technical implementation details.
- **`src/tools/search_tool.rs`**: Core implementation of the search and file reading logic.

## 🛠️  Project Tools
- **`cargo`**: Standard Rust build tool. Always prefer `cargo build --release` for production use to ensure optimal search performance.

## 🔍 Tool Capabilities

### 1. `search`
- **Function**: Recursively indexes text files in a specified directory and performs keyword searches.
- **Performance**:
    - **In-Memory**: Creates a fresh Tantivy index in RAM for every request.
    - **Exclusions**: Automatically skips `.git`, `target`, `node_modules`, `.vscode`, `build`, and `dist` to maintain speed.
    - **Timing**: Logs detailed metrics (Indexing, Commit, Search, Total) to `stderr`.
- **Safety**: Uses a two-tier detection (extension blacklist + content sniffing) to skip binary files.

### 2. `read_file_content`
- **Function**: Reads and returns the raw text content of a specified file path.
- **Validation**: Includes checks for file existence and text validity to prevent reading binary data.

## 🚀 Building and Running

- **Handshake Handshake**: The server uses **stdio** transport.
- **Production Build**: `cargo build --release`
- **Execution**: `./target/release/file-search-mcp`
- **Logging**: Controlled via `RUST_LOG` (e.g., `RUST_LOG=info`). Logs are sent to `stderr`.

## 🛠️  Development Conventions

### Implementation Philosophy
- **Type-Driven**: Define domain models and parameters first.
- **Surgical Updates**: Prefer precise changes to existing logic over large refactors.
- **TDD**: Use `cargo test` to verify logic. A unit test for directory exclusion is available in `search_tool.rs`.

### Tool Extension
- Add new tools as `async fn` in `src/tools/` with the `#[tool]` attribute.
- Ensure all parameters derive `serde::Deserialize` and `schemars::JsonSchema`.

## 📝 Important Notes
- **Index Lifecycle**: The index is ephemeral and recreated on every search. This ensures 100% data freshness and zero persistent disk footprint.
- **MCP Compatibility**: Always use `rmcp` version `0.1.5` or later to avoid JSON schema validation issues in clients like Claude Desktop.
