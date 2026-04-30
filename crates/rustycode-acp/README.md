# rustycode-acp

Agent Client Protocol (ACP) server implementation for RustyCode.

## Purpose

Implements the Agent Client Protocol specification (https://agentclientprotocol.com/), making RustyCode compatible with ACP clients like Zed, VS Code, and other IDEs. Enables IDE-based agent interaction using a standardized JSON-RPC protocol.

## Key Types

- `ACPServer` — Main ACP server handling protocol negotiation and session management
- `PromptHandler` — Processes user prompts and coordinates agent execution
- `LLMIntegration` — Bridges RustyCode LLM providers to ACP protocol
- `ToolExecutor` — Executes tools within ACP sessions
- `ACPMessage` — Base message type for protocol communication
- `SessionManager` — Session lifecycle (new, load, resume, close)

## Public API

```rust
use rustycode_acp::ACPServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create and start ACP server
    let mut server = ACPServer::new();
    
    // Initialize with client capabilities
    let client_info = ClientInfo {
        name: "zed".to_string(),
        version: "0.1.0".to_string(),
    };
    
    server.initialize(client_info).await?;
    
    // Server listens on stdin/stdout for JSON-RPC messages
    server.run().await?;
    
    Ok(())
}
```

## Protocol Support

- ✅ **initialize** — Protocol negotiation and capability exchange
- ✅ **session/new** — Create new agent sessions
- ✅ **session/load** — Resume existing sessions
- ✅ **session/prompt** — Process user messages (basic support)
- 🔄 **Streaming responses** — Planned
- 🔄 **Tool progress reporting** — Planned

## Communication

Messages are JSON-RPC 2.0 format over stdin/stdout:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": { ... }
}
```

## Running the Server

```bash
# Start ACP server (defaults to current directory)
rustycode-acp

# Start in specific directory
rustycode-acp --cwd /path/to/project
```

## Dependencies

- `serde_json` — JSON serialization for ACP protocol
- `tokio` — Async runtime
- `rustycode-core` — Core session management
- `rustycode-llm` — LLM provider integration
- `rustycode-tools` — Tool execution
- `anyhow` — Error handling

## Architecture Notes

The ACP server translates between two protocol worlds:
1. **ACP Protocol** — Standardized IDE-facing protocol (JSON-RPC)
2. **RustyCode Internal** — Session management, agents, tools, LLM providers

Messages flow: IDE → ACP Server → SessionManager → Agents → Tools/LLM

Sessions are isolated and can run multiple agents concurrently. Tool results are streamed back to IDE via JSON-RPC notifications.

## Testing

Unit tests verify protocol message handling. Integration tests run the server with mock IDE clients to verify end-to-end flows.

## See Also

- `rustycode-core` — Core session and execution
- `rustycode-llm` — LLM provider implementations
- `rustycode-tools` — Tool execution framework
- `rustycode-agents` — Agent implementations
- Agent Client Protocol Spec: https://agentclientprotocol.com/
