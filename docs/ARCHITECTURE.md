# Architecture Decisions - File Search MCP

This document outlines the key architectural decisions made during the development of the File Search MCP server.

## 1. Language and Runtime
- **Decision**: Use **Rust** (2024 Edition) with the **Tokio** asynchronous runtime.
- **Rationale**: 
    - **Performance**: File scanning and full-text indexing are CPU and I/O intensive tasks. Rust provides the necessary performance without the overhead of a garbage collector.
    - **Safety**: Rust's memory safety guarantees are critical for a tool that parses arbitrary file content.
    - **Async I/O**: Tokio allows the server to handle I/O operations efficiently, which is important for recursive directory walking and standard input/output communication.

## 2. Model Context Protocol (MCP) Integration
- **Decision**: Use the **`rmcp`** crate (Rust SDK for MCP).
- **Rationale**: 
    - Provides a high-level, idiomatic Rust interface for implementing MCP servers.
    - Simplifies tool definition using procedural macros (`#[tool]`).
    - Handles JSON-RPC serialization and deserialization according to the MCP specification.
    - Supports **stdio** transport, which is the standard for most MCP hosts (Claude Desktop, Cursor, etc.).

## 3. Search Engine Selection
- **Decision**: Use **Tantivy**.
- **Rationale**: 
    - Tantivy is a high-performance, full-text search engine library written in Rust (inspired by Apache Lucene).
    - It is significantly faster than other alternatives and provides advanced features like BM25 scoring.
    - Its ability to create and query indexes entirely in RAM fits the "on-demand" nature of this tool.

## 4. Indexing Strategy: Persistent & Incremental
- **Decision**: Use **persistent, on-disk indexing** with **incremental updates**.
- **Rationale**: 
    - **Performance**: Significant speedup for subsequent searches by only re-indexing changed files.
    - **Scalability**: Allows searching much larger directories without the overhead of re-scanning everything on every request.
    - **Persistence**: Index data is stored in the system's cache directory (`~/.cache/file-search-mcp/`).
    - **Isolation**: Each search directory is hashed to create a unique, isolated index subdirectory.
    - **Incremental Logic**: Uses file modification timestamps (`mtime`) stored in the index to determine which files need updating.
- **Trade-off**: Requires disk space in the cache directory, but ensures the best balance of speed and freshness.

## 5. File Discovery and Safety
- **Decision**: Implement a two-tier **text/binary detection** system.
- **Rationale**: 
    - **Tier 1 (Extension-based)**: Quickly skips known binary formats (images, executables, archives) to save time.
    - **Tier 2 (Content-based)**: Performs "sniffing" on the first 8KB of unknown files to detect NULL bytes, high control-character ratios, and UTF-8 validity.
    - **Goal**: Prevents the search engine from attempting to index binary data, which could lead to garbled results or performance degradation.

## 6. Observability and Diagnostics
- **Decision**: Use the **`tracing`** crate with logs directed to **`stderr`**.
- **Rationale**: 
    - MCP communication happens over `stdout` (JSON-RPC). Any unexpected text on `stdout` would break the protocol.
    - `stderr` is the correct channel for logs, which MCP hosts typically capture and display in their own debug consoles.
    - `tracing` provides structured logging that can be easily filtered via environment variables (e.g., `RUST_LOG=debug`).

## 7. Tool Design
- **Decision**: Consolidate functionality into a single `SearchTool` struct.
- **Rationale**: 
    - Groups related operations (searching and reading content) into a logical unit.
    - Simplifies the server initialization in `main.rs`.
    - Leverages the `tool_box` pattern in `rmcp` to expose multiple methods as individual MCP tools.
