# rustycode-mcp

Model Context Protocol (MCP) server and client for RustyCode.

## Purpose

Implements the Model Context Protocol, enabling Claude and other clients to access RustyCode's capabilities (codebase context, tools, memory) through a standardized protocol.

## Key Types

- `MCPServer` — MCP server exposing RustyCode as a context provider
- `ResourceProvider` — Serves codebase files and resources
- `ToolProvider` — Exposes RustyCode tools via MCP
- `ContextProvider` — Serves ranked context (code, memory, history)

## MCP Resources

### Code Resources
- `file://path/to/file` — File content
- `codebase://` — Codebase summary
- `search://query` — Search results

### Context Resources
- `memory://` — Session memories
- `history://` — Conversation history
- `plan://` — Current plan

## MCP Tools

Exposes RustyCode tools as MCP tools:
- `bash` — Execute commands
- `git` — Git operations
- `file_read` / `file_write` — File operations
- `lsp_diagnostics`, `lsp_hover`, `lsp_definition`, `lsp_completion` — Language intelligence
- All tools from `rustycode-tools`

## Public API

```rust
use rustycode_mcp::{MCPServer, ResourceProvider};

// Create MCP server
let server = MCPServer::new()?;

// Add resource provider (codebase context)
let provider = ResourceProvider::new(".")?;
server.register_provider(provider)?;

// Start serving
server.listen("localhost:3000").await?;

// Clients can now:
// 1. Request file contents: resource.read("file://src/main.rs")
// 2. Search codebase: tool.call("search_code", { "query": "async fn" })
// 3. Execute tools: tool.call("bash", { "command": "cargo test" })
```

## Terminal MCP launcher

The crate also ships a stdio launcher binary named `rustycode-mcp` for
agent-driven terminal control.

Example `.mcp.json` entry:

```json
{
  "mcpServers": {
    "rustycode-terminal": {
      "command": "cargo",
      "args": [
        "run",
        "-p",
        "rustycode-mcp",
        "--bin",
        "rustycode-mcp",
        "--",
        "--backend",
        "auto",
        "--workspace-root",
        "/Users/nat/dev/rustycode"
      ]
    }
  }
}
```

Supported `--backend` values:

- `auto` chooses the first available backend
- `tmux` forces tmux
- `it2` forces the it2 CLI backend
- `iterm2_native` forces the native iTerm2 backend

## Use Cases

1. **Claude Integration** — Use Claude with full codebase context
2. **IDE Integration** — Build IDE extensions with RustyCode backend
3. **CI/CD Integration** — Provide context to automated pipelines
4. **Multi-tool Workflows** — Coordinate multiple AI tools

## Protocol Support

- ✅ Resources (read files, codebase)
- ✅ Tools (execute commands, git operations)
- ✅ Prompts (pre-built context prompts)
- 🔄 Sampling (Claude can request more analysis)

## Dependencies

- `tokio` — Async runtime
- `serde_json` — Protocol messages
- `rustycode-core` — Session context
- `rustycode-lsp` — Code intelligence
- `rustycode-tools` — Tool execution
- `anyhow` — Error handling

## Architecture Notes

Server exposes RustyCode's capabilities as MCP resources and tools. Resource providers fetch from filesystem, git, LSP. Tool providers wrap tool executor.

Authentication optional (can use shared secret or API key).

Streaming responses for large resources (files, history).

## Testing

Tests verify protocol compliance, resource serving, tool execution, and error handling. Mock clients test protocol conformance.

## See Also

- Model Context Protocol: https://modelcontextprotocol.io/
- `rustycode-lsp` — Code intelligence
- `rustycode-tools` — Tool definitions
- `rustycode-core` — Session and context
