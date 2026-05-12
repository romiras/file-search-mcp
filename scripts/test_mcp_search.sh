#!/bin/bash
# Reusable test script for file-search-mcp JSON-RPC communication

BINARY_PATH="./target/debug/file-search-mcp"
KEYWORD="${1:-api}"
DIRECTORY="${2:-.}"

if [ ! -f "$BINARY_PATH" ]; then
    echo "Error: Binary not found at $BINARY_PATH"
    exit 1
fi

echo "--- Testing MCP Search ---"
echo "Keyword: $KEYWORD"
echo "Directory: $DIRECTORY"
echo "--------------------------"

(
  # 1. Initialize Handshake
  echo '{"jsonrpc": "2.0", "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test-client", "version": "1.0.0"}}, "id": 1}'
  sleep 0.1

  # 2. Notification: Initialized
  echo '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
  sleep 0.1

  # 3. Tool Call: search
  # Using printf to safely handle JSON strings with variables
  printf '{"jsonrpc": "2.0", "method": "tools/call", "params": {"name": "search", "arguments": {"directory": "%s", "keyword": "%s"}}, "id": 2}\n' "$DIRECTORY" "$KEYWORD"

  # Allow time for processing/indexing
  sleep 2.0
) | "$BINARY_PATH" | jq .
