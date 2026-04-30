# rustycode-tool-server

Standalone HTTP server for remote tool execution.

## Purpose

Provides a standalone server that exposes RustyCode's tool execution capabilities over HTTP and WebSocket. Allows remote clients (IDEs, CI/CD systems, external services) to execute tools and receive results without embedding RustyCode directly.

## Features

- **HTTP API** — POST `/call` endpoint for tool execution
- **WebSocket** — Real-time bidirectional communication at `/ws`
- **Caching** — Result caching with GET `/cache/:call_id`
- **Web UI** — Browser-based testing interface
- **Multi-threaded** — CPU-aware worker thread pool
- **Async** — Full tokio-based async runtime

## Running the Server

```bash
# Start server (defaults to localhost:3000)
cargo run -p rustycode-tool-server

# Server will be available at:
# HTTP: http://127.0.0.1:3000
# WebSocket: ws://127.0.0.1:3000/ws
# Web UI: http://127.0.0.1:3000/
```

## API Usage

### HTTP POST /call

Execute a tool via HTTP and get the result:

```bash
curl -X POST http://127.0.0.1:3000/call \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "bash",
    "args": {"command": "ls -la"}
  }'
```

Response:
```json
{
  "call_id": "call-123",
  "tool": "bash",
  "status": "success",
  "result": "..."
}
```

### WebSocket /ws

Connect for bidirectional tool communication:

```javascript
const ws = new WebSocket('ws://127.0.0.1:3000/ws');
ws.onopen = () => {
  ws.send(JSON.stringify({
    tool: 'bash',
    args: { command: 'echo hello' }
  }));
};
ws.onmessage = (event) => {
  console.log('Result:', JSON.parse(event.data));
};
```

### GET /cache/:call_id

Retrieve cached result from previous call:

```bash
curl http://127.0.0.1:3000/cache/call-123
```

## Architecture

The server creates a shared `ToolExecutor` from the current working directory, making all tools in that workspace available. Requests are queued and executed with CPU-aware concurrency (one worker per available CPU core).

Results are cached to avoid re-execution of identical requests. WebSocket connections maintain persistent state and can stream progress updates.

## Dependencies

- `tokio` — Async runtime
- `axum` — Web framework
- `tokio-tungstenite` — WebSocket support
- `rustycode-tools` — Tool execution
- `rustycode-protocol` — Message types

## Use Cases

- **Remote IDE Integration** — IDEs connect to server for tool execution
- **CI/CD Integration** — External systems call tools via HTTP
- **Distributed Execution** — Tools run on central server, not on dev machine
- **Headless Automation** — Scripts/bots use WebSocket for tool access

## See Also

- `rustycode-tools` — Tool implementation and execution
- `rustycode-guard` — Security rules applied to all tool calls
- `rustycode-acp` — Agent Client Protocol (similar but different protocol)
