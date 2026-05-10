# RustyCode WebSocket Protocol (v1)

**Status:** PLANNED - This protocol is designed for future WebSocket-based streaming communication between the RustyCode web frontend and backend. 

**Current Implementation:** The current web frontend (`crates/rustycode-web`) uses HTTP POST requests to `rustycode-tool-server` (see `/call` endpoint) for all tool execution and LLM communication, due to WASM sandbox constraints. This WebSocket protocol is planned to enable real-time streaming, session recovery, and bidirectional communication in future versions.

This protocol defines the intended communication between the RustyCode web frontend and backend using WebSockets for low-latency, bidirectional messaging with support for real-time streaming responses.

## 1. Connection Lifecycle

1. Client opens a WebSocket connection to `/ws`.
2. The client may include a session token in the connection URL or in an initial `hello` frame for session recovery.
3. The server responds with either:
   - `session_resumed` if the token matches an existing session
   - `session_created` if no token was provided or the token is invalid
4. After the handshake, the server sends an initial `state_update` frame containing the full `FrontendSession`.

## 2. Wire Format

All websocket frames use a canonical JSON envelope:

```json
{
  "v": 1,
  "type": "input",
  "payload": {}
}
```

Rules (when WebSocket transport is used):
- `v` is the protocol version. Must be 1 for this version.
- `type` identifies the message kind. Unknown types must be ignored by clients or rejected by servers with a typed `error` frame.
- `payload` contains the message-specific body, schema-validated per type.
- Unknown `v` values must be rejected with an `error` frame (code: `UNSUPPORTED_VERSION`).

### Current Alternative: HTTP Transport

Until WebSocket support is implemented, the RustyCode web frontend uses HTTP POST requests:

**Endpoint:** `POST http://127.0.0.1:3000/call`

**Request Format:**
```json
{
  "call_id": "web-1234567890",
  "name": "Bash",
  "arguments": { "command": "ls -la" }
}
```

**Response Format:**
```json
{
  "success": true,
  "output": "result text",
  "error": null
}
```

**HTTP Headers:**
- `Content-Type: application/json`
- No authentication required (assumes localhost trust boundary)

**Limitations:**
- No server push or streaming (polling required for progress)
- Higher latency per interaction
- No bidirectional real-time updates
- Session state must be managed client-side only

## 3. Message Types

### 3.1 Client-to-Server

| Type | Payload | Description |
| :--- | :--- | :--- |
| `hello` | `{"session_token": string \| null, "client_info": {"type": "web" \| "wasm", "version": string}}` | Initial handshake for session recovery and capability negotiation. |
| `input` | `{"content": string, "metadata": {"source": "user" \| "programmatic"}}` | User-submitted message, slash command (e.g., `/model gpt-4`), or bang command (e.g., `!ls`). |
| `heartbeat` | `{"ts": number, "session_state": FrontendSession}` | Application-level keepalive with optional state sync. |
| `subscribe` | `{"channels": ["session", "progress", "tools"]}` | Subscribe to real-time update channels for streaming. |
| `unsubscribe` | `{"channels": ["progress"]}` | Unsubscribe from specific channels. |
| `ack` | `{"message_id": string, "type": "delta" \| "state"}` | Acknowledgment of received message for reliable delivery. |

### 3.2 Server-to-Client

| Type | Payload | Description |
| :--- | :--- | :--- |
| `session_created` | `{"session_token": string, "expires_at": number, "capabilities": ["streaming", "mcp"]}` | New session identifier assigned by the server with metadata. |
| `session_resumed` | `{"session_token": string, "expires_at": number}` | Existing session successfully recovered. |
| `state_update` | `FrontendSession` | Full serialization of the current session state (authoritative snapshot). |
| `delta` | `EventDelta` | Incremental update for streaming or fine-grained changes (e.g., LLM tokens). |
| `heartbeat_ack` | `{"ts": number, "server_ts": number}` | Response to a client heartbeat with RTT measurement. |
| `error` | `{"code": string, "message": string, "details": object}` | Typed server-side error with machine-readable code. |
| `progress` | `{"id": string, "type": "llm_stream" \| "tool_exec", "status": "started" \| "in_progress" \| "completed" \| "failed", "progress": number}` | Progress notification for long-running operations. |

## 4. Data Structures

### 4.1 `FrontendSession`

Matches `rustycode_ui_model::FrontendSession` JSON serialization (see `crates/rustycode-ui-model/src/lib.rs`):

```json
{
  "input": "",
  "messages": [
    {
      "content": "Hello World",
      "kind": "User" | "Assistant" | "System" | "Tool" | "Error"
    }
  ],
  "last_user_prompt": "string | null",
  "pending_request": false,
  "tool_iteration_count": 0,
  "current_response": ""
}
```

**Field Descriptions:**
- `input`: Current unsubmitted text in the input buffer
- `messages`: Chronological list of conversation messages
- `last_user_prompt`: Most recent user chat message (for context/reference)
- `pending_request`: Indicates active LLM/tool request in progress
- `tool_iteration_count`: Tracks nested tool call iterations
- `current_response`: Accumulates streaming assistant response chunks

### 4.2 `EventDelta`

Incremental updates must be typed and structurally mergeable. Delta frames enable efficient streaming without resending full state.

```json
{
  "type": "append_chunk" | "set_pending" | "add_message" | "replace_session",
  "payload": {}
}
```

**Delta Types:**

- **`append_chunk`** (streaming text):
  ```json
  { "type": "append_chunk", "payload": { "chunk": "string", "is_final": false } }
  ```
  
- **`set_pending`** (request state):
  ```json
  { "type": "set_pending", "payload": { "pending_request": boolean, "status_text": "string" } }
  ```
  
- **`add_message`** (new message):
  ```json
  { "type": "add_message", "payload": { "message": FrontendMessage } }
  ```
  
- **`replace_session`** (full replacement):
  ```json
  { "type": "replace_session", "payload": { "session": FrontendSession } }
  ```

### 4.3 `FrontendMessage`

Individual message in the conversation history:

```json
{
  "content": "string",
  "kind": "User" | "Assistant" | "System" | "Tool" | "Error"
}
```

### 4.4 `SubmittedInput`

Parsed client input (derived from `rustycode-ui-model::SubmittedInput`):

```json
{
  "type": "Empty" | "SlashCommand" | "BangCommand" | "ChatMessage",
  "value": "string (if applicable)"
}
```

**Parsing Rules:**
- `Empty`: Empty string or whitespace-only
- `SlashCommand`: Starts with `/` (e.g., `/help`, `/model gpt-4`)
- `BangCommand`: Starts with `!` (e.g., `!ls`, `!cargo build`)
- `ChatMessage`: All other non-empty input treated as chat

## 5. State Merge Rules

1. **`state_update`**: Always replaces the client's local session snapshot entirely. Treat as authoritative source of truth.

2. **`delta` frames**: Applied sequentially in order of receipt to the current local snapshot. Must be idempotent.

3. **`replace_session` delta**: Overrides any prior partial deltas and acts as a full state replacement.

4. **Conflict resolution**: If the client receives a `state_update` after streamed `delta` frames, the `state_update` takes precedence and deltas should be discarded/rebased.

5. **Out-of-order handling**: Clients must buffer deltas received out of order (using message IDs or timestamps) and apply them sequentially.

6. **Lost sync recovery**: If the client detects state inconsistency (e.g., missing messages), it should request a fresh `state_update` or wait for the next automatic broadcast.

7. **Acknowledgment**: For reliable delivery in poor network conditions, clients should send `ack` frames for critical messages (state updates, session changes).

## 6. Error Codes

| Code | Description | Action |
| :--- | :--- | :--- |
| `UNSUPPORTED_VERSION` | Protocol version not supported | Reconnect with supported version |
| `INVALID_FRAME` | Malformed JSON or missing required fields | Log error, ignore frame |
| `SESSION_EXPIRED` | Session token no longer valid | Request `hello` with null token |
| `RATE_LIMITED` | Too many requests | Implement exponential backoff |
| `UNAUTHORIZED` | Invalid or missing session token | Clear local session, reconnect |
| `INTERNAL_ERROR` | Server-side failure | Retry or report error to user |
| `TOOL_EXECUTION_FAILED` | Tool call failed on server | Display error, allow retry |

## 7. Heartbeat Rules

1. The client sends `heartbeat` frames periodically (recommended: every 30 seconds) while connected.

2. The server must reply with `heartbeat_ack` using the same timestamp. Clients may calculate RTT (round-trip time).

3. If no ack is received within the configured timeout (recommended: 10 seconds), the client should treat the socket as stale and reconnect.

4. After 3 consecutive missed heartbeats, the client should initiate reconnection with exponential backoff.

5. The server may close idle sockets after its own timeout and should emit a typed `error` (code: `SESSION_EXPIRED`) before disconnecting when possible.

## 8. Example Flows

### 8.1 Basic Session Lifecycle

1. **User opens the app.**
   - Client -> Server:
   ```json
   {
     "v": 1,
     "type": "hello",
     "payload": {
       "session_token": null,
       "client_info": {
         "type": "web",
         "version": "0.1.0"
       }
     }
   }
   ```

2. **Server creates a new session.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "session_created",
     "payload": {
       "session_token": "sess_abc123xyz",
       "expires_at": 1704153600,
       "capabilities": ["streaming", "mcp"]
     }
   }
   ```

3. **Server sends initial state.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "state_update",
     "payload": {
       "input": "",
       "messages": [
         {
           "content": "Welcome to RustyCode!",
           "kind": "System"
         }
       ],
       "last_user_prompt": null,
       "pending_request": false,
       "tool_iteration_count": 0,
       "current_response": ""
     }
   }
   ```

4. **Client subscribes to streaming updates.**
   - Client -> Server:
   ```json
   {
     "v": 1,
     "type": "subscribe",
     "payload": {
       "channels": ["session", "progress", "tools"]
     }
   }
   ```

### 8.2 Chat with Streaming Response

1. **User submits a message.**
   - Client -> Server:
   ```json
   {
     "v": 1,
     "type": "input",
     "payload": {
       "content": "Explain quantum computing in simple terms.",
       "metadata": {
         "source": "user"
       }
     }
   }
   ```

2. **Server acknowledges and sets pending state.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "delta",
     "payload": {
       "type": "set_pending",
       "data": {
         "pending_request": true,
         "status_text": "Thinking..."
       }
     }
   }
   ```

3. **Server streams response chunks.**
   - Server -> Client (multiple frames):
   ```json
   {"v":1,"type":"delta","payload":{"type":"append_chunk","data":{"chunk":"Quantum"}}}
   ```
   ```json
   {"v":1,"type":"delta","payload":{"type":"append_chunk","data":{"chunk":" computing"}}}
   ```
   ```json
   {"v":1,"type":"delta","payload":{"type":"append_chunk","data":{"chunk":" is a way"}}}
   ```

4. **Server completes response with final message.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "state_update",
     "payload": {
       "input": "",
       "messages": [
         {
           "content": "Explain quantum computing in simple terms.",
           "kind": "User"
         },
         {
           "content": "Quantum computing is a way to process information using quantum bits...",
           "kind": "Assistant"
         }
       ],
       "pending_request": false,
       ...
     }
   }
   ```

### 8.3 Slash Command Execution

1. **User enters slash command.**
   - Client -> Server:
   ```json
   {
     "v": 1,
     "type": "input",
     "payload": {
       "content": "/skill run code-review",
       "metadata": {
         "source": "user"
       }
     }
   }
   ```

2. **Server parses as slash command, executes tool.**
   - Server -> Client (progress):
   ```json
   {
     "v": 1,
     "type": "progress",
     "payload": {
       "id": "task_123",
       "type": "tool_exec",
       "status": "started",
       "progress": 0
     }
   }
   ```

3. **Tool produces output (streamed).**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "delta",
     "payload": {
       "type": "add_message",
       "data": {
         "message": {
           "content": "Running code review...",
           "kind": "Tool"
         }
       }
     }
   }
   ```

4. **Final state update with results.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "state_update",
     "payload": {
       "messages": [..., {
         "content": "Review complete: 3 issues found",
         "kind": "Assistant"
       }]
     }
   }
   ```

### 8.4 Heartbeat and Keepalive

1. **Client sends heartbeat.**
   - Client -> Server:
   ```json
   {
     "v": 1,
     "type": "heartbeat",
     "payload": {
       "ts": 1234567890123,
       "session_state": { "pending_request": false }
     }
   }
   ```

2. **Server responds with ack.**
   - Server -> Client:
   ```json
   {
     "v": 1,
     "type": "heartbeat_ack",
     "payload": {
       "ts": 1234567890123,
       "server_ts": 1234567890125
     }
   }
   ```
    (RTT = 2ms)

## 9. Security Considerations

### 9.1 Authentication & Authorization

- **Session Token Security**: Tokens are generated with cryptographic randomness (32+ bytes, base64url encoded) and stored securely in HttpOnly cookies or secure storage (IndexedDB for web)
- **Token Expiration**: Sessions expire after inactivity (default: 24 hours) or absolute timeout (default: 7 days)
- **Origin Validation**: WebSocket connections must validate the `Origin` header matches allowed domains (same-origin policy for web deployments)
- **Token Revocation**: Server maintains token revocation list for logged-out or compromised sessions

### 9.2 Message Security

- **Input Validation**: All incoming message payloads are validated against JSON schemas before processing
- **Size Limits**: Message size limited to 64KB per frame; larger payloads rejected with `error` frame
- **Rate Limiting**: Per-session rate limits applied (default: 60 messages/minute) to prevent abuse
- **Content Filtering**: User input sanitized to prevent XSS and injection attacks (especially important for tool execution results)

### 9.3 Transport Security

- **WSS Required**: In production, WebSocket connections must use `wss://` (TLS encrypted)
- **No Sensitive Data in Logs**: Server-side logging redacts session tokens and user messages
- **Secret Sanitization**: API keys and tool outputs containing secrets are sanitized before transmission (see `rustycode-tools/src/security.rs`)

### 9.4 Cross-Origin Resource Sharing (CORS)

For web deployments:
```
Access-Control-Allow-Origin: https://rustycode.example.com
Access-Control-Allow-Credentials: true
Access-Control-Allow-Methods: GET, POST, OPTIONS
Access-Control-Allow-Headers: Content-Type, Authorization
```

## 10. Migration from HTTP to WebSocket

For applications currently using the HTTP transport (`rustycode-tool-server`), migrating to WebSocket provides:

### Benefits:
- **Lower Latency**: Eliminates HTTP connection overhead per message
- **True Streaming**: Real-time token-by-token streaming of LLM responses
- **Bidirectional Communication**: Server can push updates without client polling
- **Better State Sync**: Real-time session state synchronization

### Migration Path:

1. **Phase 1**: Proxy WebSocket through existing tool-server
   ```
   WASM Client → WebSocket → Tool Server → Backend
   ```

2. **Phase 2**: Native WebSocket support in backend
   ```
   WASM Client → WebSocket → Backend (direct)
                  (fallback to HTTP for tooling)
   ```

3. **Phase 3**: Full feature parity
   - Session recovery via WebSocket
   - Real-time progress notifications
   - Bi-directional streaming

### Backward Compatibility:

The HTTP endpoint (`POST /call`) remains available during transition:
```rust
// Tool server continues to support HTTP
POST /call
Content-Type: application/json

{
  "call_id": "web-123456",
  "name": "Bash",
  "arguments": { "command": "ls -la" }
}
```

### Browser Compatibility:

WebSocket API is supported in all modern browsers:
- Chrome 16+
- Firefox 11+
- Safari 6+
- Edge 12+
- iOS Safari 7.1+
- Android Browser 4.4+

## 11. Related Implementation References

- **UI Model**: `crates/rustycode-ui-model/src/lib.rs` - `FrontendSession`, `FrontendMessage`, `SubmittedInput`
- **Web Frontend**: `crates/rustycode-web/src/main.rs` - Current HTTP-based implementation
- **Tool Server**: `crates/rustycode-tool-server/src/main.rs` - Current `rustycode-tool-server` HTTP endpoint
- **Protocol Constants**: Defined in orchestration layer for version negotiation
- **Error Handling**: See `rustycode-tools/src/security.rs` for sanitization logic

## 12. Future Extensions

Potential additions to the protocol:

1. **Compression**: Support for compressed message payloads for large context windows
2. **File Transfer**: Binary file upload/download for images, documents
3. **Multi-User**: Shared session collaboration with presence indicators
4. **Plugins**: Dynamic capability negotiation via `hello`/`capabilities` exchange
5. **Metrics**: Performance telemetry and diagnostic frames
6. **Tracing**: Distributed tracing correlation IDs for debugging
7. **Reconnection**: Automatic session resumption with exponential backoff
