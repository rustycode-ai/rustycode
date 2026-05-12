# Phase 3: App-Server Daemon

> JSON-RPC protocol separating UI from core.

## Goal

Introduce a JSON-RPC 2.0 app-server layer that decouples the TUI (and future clients) from the core runtime. The TUI talks to a thin client crate (`rustycode-server-client`) instead of depending directly on 26 workspace crates. Start with an in-process client (mpsc channels, zero serialization), then add remote transport (WebSocket) when needed.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  rustycode-tui                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  TUI App (ratatui)                                   │   │
│  │  depends on: server-client, protocol, config (~5)    │   │
│  └──────────────┬───────────────────────────────────────┘   │
│                 │ ClientHandle                                │
└─────────────────┼───────────────────────────────────────────┘
                  │ tokio::sync::mpsc (capacity 64)
                  ▼
┌─────────────────────────────────────────────────────────────┐
│  rustycode-server (message processor)                        │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  RequestRouter — dispatches JSON-RPC methods         │   │
│  │  NotificationBroadcaster — fans out EventMsg         │   │
│  │  ApprovalHandler — bidirectional request/response    │   │
│  └──────────────┬───────────────────────────────────────┘   │
│                 │ submit(Op) / broadcast(EventMsg)           │
└─────────────────┼───────────────────────────────────────────┘
                  │
┌─────────────────▼───────────────────────────────────────────┐
│  rustycode-core / rustycode-agent-runtime                    │
│  (session, headless runtime, tool execution)                 │
└─────────────────────────────────────────────────────────────┘
```

**In-process mode** (MVP): ClientHandle ↔ Server communicate via `tokio::sync::mpsc` channels. No serialization overhead.

**Remote mode** (future): ClientHandle ↔ WebSocket ↔ Server. JSON-RPC 2.0 over TCP/Unix socket.

## New Crates

### `rustycode-server-protocol`

Typed request/response/notification definitions. No async runtime dependency.

```rust
// src/lib.rs
pub mod requests;
pub mod responses;
pub mod notifications;
pub mod envelope;

// src/envelope.rs — JSON-RPC 2.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,  // always "2.0"
    pub id: RequestId,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
```

### `rustycode-server`

Message processor. Routes requests to core, broadcasts events to clients.

```rust
// src/lib.rs
pub mod router;
pub mod handler;
pub mod approval;
pub mod server;

// src/server.rs
pub struct AppServer {
    /// Core runtime handle
    runtime: Arc<RuntimeHandle>,
    /// Connected clients
    clients: Arc<DashMap<ClientId, ClientHandle>>,
    /// Inbound from clients
    inbound_rx: mpsc::Receiver<(ClientId, ClientMessage)>,
    /// Pending approval requests
    pending_approvals: Arc<Mutex<HashMap<ApprovalId, ApprovalState>>>,
}

impl AppServer {
    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                msg = self.inbound_rx.recv() => {
                    let (client_id, msg) = msg.context("channel closed")?;
                    self.handle_client_message(client_id, msg).await?;
                }
                event = self.runtime.next_event() => {
                    self.broadcast_event(event).await;
                }
            }
        }
    }
}
```

### `rustycode-server-client`

Thin client crate. TUI only depends on this + protocol + config.

```rust
// src/lib.rs
pub mod in_process;
pub mod remote;  // future: WebSocket

// src/in_process.rs
pub struct InProcessClient {
    outbound_tx: mpsc::Sender<ClientMessage>,
    inbound_rx: mpsc::Receiver<ServerMessage>,
    notification_tx: broadcast::Sender<Notification>,
}

impl InProcessClient {
    /// Send a request and wait for the response.
    pub async fn request(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id();
        self.outbound_tx.send(ClientMessage::Request {
            id: id.clone(),
            method: method.to_string(),
            params,
        }).await?;
        // wait for response with matching id
        self.wait_for_response(&id).await
    }

    /// Send a notification (no response expected).
    pub async fn notify(&self, method: &str, params: Value) -> anyhow::Result<()> {
        self.outbound_tx.send(ClientMessage::Notification {
            method: method.to_string(),
            params,
        }).await?;
        Ok(())
    }

    /// Subscribe to server-pushed notifications.
    pub fn subscribe(&self) -> broadcast::Receiver<Notification> {
        self.notification_tx.subscribe()
    }
}
```

## MVP Method Set

~25 methods covering core TUI needs. Organized by resource prefix following Codex convention.

### Session Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `session/create` | `{ model, system_prompt?, tools? }` | `{ session_id }` | Start new session |
| `session/stop` | `{ session_id }` | `{}` | Stop session |
| `session/list` | `{ cursor?, limit? }` | `{ sessions: [...], next_cursor? }` | List sessions |
| `session/get` | `{ session_id }` | `Session` | Get session details |
| `session/delete` | `{ session_id }` | `{}` | Delete session |

### Turn Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `turn/submit` | `{ session_id, message }` | `{ turn_id }` | Submit user message (replaces submit(Op::SendMessage)) |
| `turn/cancel` | `{ session_id }` | `{}` | Cancel current turn |
| `turn/list` | `{ session_id, cursor?, limit? }` | `{ turns: [...], next_cursor? }` | List turns |
| `turn/get` | `{ turn_id }` | `Turn` | Get turn details |

### Tool Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `tool/approve` | `{ request_id, decision }` | `{}` | Respond to approval request |
| `tool/list` | `{ session_id? }` | `{ tools: [...] }` | List available tools |
| `tool/toggle` | `{ session_id, tool_name, enabled }` | `{}` | Enable/disable tool |

### Config Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `config/get` | `{ key? }` | `Config` | Get current config |
| `config/set` | `{ key, value }` | `{}` | Update config value |
| `config/model/list` | `{}` | `{ models: [...] }` | List available models |

### Filesystem Methods (read-only for TUI)

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `fs/read` | `{ path }` | `{ content }` | Read file content |
| `fs/list` | `{ path }` | `{ entries: [...] }` | List directory |

### History Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `history/search` | `{ query, limit? }` | `{ results: [...] }` | Search conversation history |

### Plan Methods

| Method | Params | Response | Notes |
|--------|--------|----------|-------|
| `plan/create` | `{ session_id, plan }` | `{ plan_id }` | Create execution plan |
| `plan/approve` | `{ plan_id }` | `{}` | Approve plan for execution |
| `plan/status` | `{ plan_id }` | `PlanStatus` | Get plan status |

## Notification Types

Server-pushed notifications streamed to all connected clients.

### Lifecycle Notifications

| Notification | Params | Notes |
|-------------|--------|-------|
| `session/started` | `{ session_id }` | Session created |
| `session/stopped` | `{ session_id, reason }` | Session ended |
| `turn/started` | `{ session_id, turn_id }` | Turn processing begins |
| `turn/completed` | `{ session_id, turn_id }` | Turn finished |

### Streaming Notifications

| Notification | Params | Notes |
|-------------|--------|-------|
| `turn/textDelta` | `{ session_id, delta }` | LLM text output (lossless) |
| `turn/thinkingDelta` | `{ session_id, delta }` | Thinking output (lossless) |
| `turn/toolCallStarted` | `{ session_id, tool_call }` | Tool execution begins |
| `turn/toolCallDelta` | `{ session_id, tool_call_id, delta }` | Tool stdout (best-effort) |
| `turn/toolCallCompleted` | `{ session_id, tool_call, result }` | Tool finished |
| `turn/commandOutputDelta` | `{ session_id, delta }` | Bash stdout (best-effort) |

### Approval Notifications

| Notification | Params | Notes |
|-------------|--------|-------|
| `approval/requested` | `{ request_id, tool, args, risk }` | Server asks client for approval |
| `approval/resolved` | `{ request_id, decision }` | Approval decision made |

## Bidirectional Requests (Approval Flow)

The server can send requests *to* the client. This is critical for the approval flow.

```
TUI (Client)                              Server
    │                                        │
    │  turn/submit("run deploy.sh")          │
    │───────────────────────────────────────→│
    │                                        │  tool needs approval
    │  approval/requested                    │
    │←───────────────────────────────────────│
    │                                        │
    │  (user sees dialog, clicks Approve)    │
    │                                        │
    │  tool/approve(request_id, Accept)      │
    │───────────────────────────────────────→│
    │                                        │  tool executes
    │  turn/toolCallDelta (stdout)           │
    │←───────────────────────────────────────│
    │  turn/toolCallCompleted                │
    │←───────────────────────────────────────│
```

### Approval Decision Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    /// Approve this one invocation
    Accept,
    /// Approve all invocations for this tool in this session
    AcceptForSession,
    /// Reject this invocation
    Decline { reason: String },
    /// Cancel the entire turn
    Cancel,
}
```

### Approval Request

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub risk_level: RiskLevel,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,      // read-only operations
    Medium,   // file edits
    High,     // command execution, network access
}
```

## InProcessClient Architecture

### Channel Design

```
┌────────────────┐                    ┌────────────────┐
│  InProcessClient│                   │  AppServer      │
│                │                    │                │
│  outbound_tx ──┼─── mpsc(64) ─────→│ inbound_rx     │
│  inbound_rx ←──┼─── mpsc(64) ──────│ outbound_tx    │
│  notify_sub  ←─┼─── broadcast(256)─←│ notify_tx      │
└────────────────┘                    └────────────────┘
```

- **outbound** (client → server): `mpsc::Sender<ClientMessage>` capacity 64
- **inbound** (server → client): `mpsc::Sender<ServerMessage>` capacity 64
- **notifications**: `broadcast::Sender<Notification>` capacity 256

### Backpressure

- `mpsc::Sender::send()` is async — blocks when channel is full (natural backpressure)
- `broadcast::Receiver` uses `try_recv()` — lagged messages emit `Lagged { skipped: N }` notification
- Streaming deltas use best-effort delivery; critical lifecycle events use lossless delivery

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "delivery")]
pub enum DeliveryGuarantee {
    /// Every message must arrive. Client blocks if saturated.
    Lossless,
    /// Best-effort. Late messages are dropped; Lagged marker emitted.
    #[serde(rename = "best-effort")]
    BestEffort,
}
```

### Lag Detection

Following Codex's pattern, when the broadcast receiver falls behind:

```rust
impl InProcessClient {
    async fn receive_loop(&mut self) {
        loop {
            match self.notify_sub.recv().await {
                Ok(notification) => self.handle_notification(notification).await,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    self.handle_notification(Notification::Lagged { skipped }).await;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    }
}
```

## RemoteClient (Future)

### WebSocket Transport

```
┌────────────────┐    WebSocket     ┌────────────────┐
│  RemoteClient  │◄───────────────→│  AppServer     │
│  (TUI process) │  JSON-RPC 2.0   │  (daemon)      │
└────────────────┘                  └────────────────┘
```

- **Transport**: WebSocket with binary frames (MessagePack) or text frames (JSON)
- **Auth**: Bearer token in first message, or Unix socket (no auth needed)
- **Max message size**: 128 MiB (matches Codex)
- **Heartbeat**: ping/pong every 30s, connection timeout 60s

### Wire Protocol

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WireMessage {
    #[serde(rename = "request")]
    Request(JsonRpcRequest),
    #[serde(rename = "response")]
    Response(JsonRpcResponse),
    #[serde(rename = "notification")]
    Notification(JsonRpcNotification),
}
```

### Connection Lifecycle

1. Client connects via WebSocket to `ws://127.0.0.1:{port}/` or Unix socket
2. Client sends `auth/initialize` with `{ token, client_info }`
3. Server validates token, responds with `{ server_info, capabilities }`
4. Normal JSON-RPC message exchange begins
5. Either side can close cleanly; server sends `session/stopped` first

## TUI Migration

### Before (current)

```rust
// rustycode-tui/Cargo.toml — 26 workspace dependencies
[dependencies]
rustycode-core = { workspace = true }
rustycode-tools = { workspace = true }
rustycode-llm = { workspace = true }
rustycode-orchestration = { workspace = true }
rustycode-protocol = { workspace = true }
rustycode-bus = { workspace = true }
rustycode-agent-runtime = { workspace = true }
# ... 19 more
```

### After

```rust
// rustycode-tui/Cargo.toml — ~8 workspace dependencies
[dependencies]
rustycode-server-client = { workspace = true }
rustycode-server-protocol = { workspace = true }
rustycode-protocol = { workspace = true }
rustycode-config = { workspace = true }
# ratatui, tokio, etc. (non-workspace)
```

### Migration Steps

1. **Phase 3A**: Add server crate alongside TUI. TUI creates both server and client in-process.
2. **Phase 3B**: Move TUI event handling from direct `EventMsg` consumption to `ClientHandle.subscribe()`.
3. **Phase 3C**: Move TUI command submission from direct `runtime.submit(Op)` to `ClientHandle.request()`.
4. **Phase 3D**: Remove direct dependencies on core, tools, llm, orchestration from TUI's Cargo.toml.
5. **Phase 3E**: (Future) Add remote mode. TUI can connect to an external daemon.

### App Event Loop Migration

```rust
// BEFORE: TUI directly consumes runtime events
loop {
    tokio::select! {
        event = runtime.next_event() => { /* handle */ }
        key = terminal.poll_event() => { /* handle */ }
    }
}

// AFTER: TUI consumes via client
loop {
    tokio::select! {
        msg = client.recv() => match msg {
            ServerMessage::Notification(n) => { /* handle streaming, lifecycle */ }
            ServerMessage::Response(r) => { /* handle request response */ }
        }
        key = terminal.poll_event() => {
            // Submit via client
            client.request("turn/submit", params).await?;
        }
    }
}
```

## Dependency Reduction

### Current TUI Dependencies (26)

Direct workspace crate dependencies:
rustycode-core, rustycode-tools, rustycode-tools-api, rustycode-llm, rustycode-orchestration,
rustycode-protocol, rustycode-bus, rustycode-agent-runtime, rustycode-config, rustycode-session,
rustycode-storage, rustycode-skill, rustycode-sandbox, rustycode-git, rustycode-auth,
rustycode-team, rustycode-connector, rustycode-learning, rustycode-observability, rustycode-memory,
rustycode-lsp, rustycode-acp, rustycode-bench, rustycode-cli-standalone, ...

### Target TUI Dependencies (~5)

| Dependency | Purpose |
|-----------|---------|
| `rustycode-server-client` | Client handle (in-process or remote) |
| `rustycode-server-protocol` | Typed requests/responses/notifications |
| `rustycode-protocol` | Shared types (Message, ToolCall, EventMsg) |
| `rustycode-config` | Config loading (theme, keybindings) |
| `rustycode-session` | Session type definitions |

Reduction: 26 → 5 (81% fewer workspace deps)

## Integration Points

### Phase 1 (Unified EventMsg)

The server broadcasts `EventMsg` variants as notifications:

```rust
impl AppServer {
    async fn broadcast_event(&self, event: EventMsg) {
        let notification = match &event {
            EventMsg::TextDelta { session_id, delta } => {
                Notification::TurnTextDelta {
                    session_id: session_id.clone(),
                    delta: delta.clone(),
                }
            }
            EventMsg::ToolExecCompleted { session_id, tool_call, result } => {
                Notification::TurnToolCallCompleted {
                    session_id: session_id.clone(),
                    tool_call: tool_call.clone(),
                    result: result.clone(),
                }
            }
            // ... map all EventMsg variants to notifications
        };
        let _ = self.notify_tx.send(notification);
    }
}
```

### Phase 2 (Event Sourcing)

The server uses the rollout recorder to persist events:

```rust
impl AppServer {
    fn new(runtime: RuntimeHandle, recorder: RolloutRecorder) -> Self {
        // Server wraps Phase 1 runtime + Phase 2 persistence
        // RolloutRecorder appends every EventMsg to JSONL
    }
}
```

Cursor pagination for list methods uses Phase 2's SQLite indexes:

```rust
// session/list with cursor
fn handle_session_list(&self, params: Value) -> anyhow::Result<Value> {
    let cursor: Option<String> = params.get("cursor").and_then(|c| c.as_str()).map(String::from);
    let limit: usize = params.get("limit").and_then(|l| l.as_u64()).unwrap_or(20) as usize;

    let sessions = self.state_runtime.list_sessions(cursor, limit)?;
    let next_cursor = sessions.last().map(|s| s.cursor_token());

    Ok(json!({
        "sessions": sessions,
        "next_cursor": next_cursor,
    }))
}
```

## Migration Strategy

### Phase 3A: Scaffolding (Week 1-2)

1. Create `rustycode-server-protocol` with envelope types
2. Create `rustycode-server` with router skeleton
3. Create `rustycode-server-client` with `InProcessClient`
4. Wire: TUI creates server + client at startup

### Phase 3B: Event Tunneling (Week 3-4)

1. Server subscribes to Phase 1 EventMsg broadcast
2. Server maps EventMsg → Notification and broadcasts to clients
3. TUI subscribes to client notifications (parallel path, not replacing direct consumption yet)
4. Verify: TUI receives identical events via both paths

### Phase 3C: Command Tunneling (Week 5-6)

1. Map TUI's Op submissions to client.request() calls
2. Server routes requests to runtime.submit(Op)
3. Verify: all TUI actions work via client
4. Remove direct runtime access from TUI

### Phase 3D: Dependency Cleanup (Week 7-8)

1. Remove unused workspace deps from TUI's Cargo.toml
2. TUI only imports from server-client, server-protocol, protocol, config
3. Verify: `cargo tree -p rustycode-tui --depth 1` shows ~5 workspace deps
4. All tests pass, TUI functionality preserved

### Phase 3E: Remote Mode (Future)

1. Add `RemoteClient` with WebSocket transport
2. Add server TCP/Unix listener
3. Support multiple connected clients
4. Authentication via Bearer token

## Success Criteria

### Functional

- [ ] All current TUI features work via InProcessClient (no feature regressions)
- [ ] Approval flow works bidirectionally over protocol
- [ ] Streaming text deltas delivered in <50ms latency (in-process)
- [ ] Tool execution output streamed in real-time
- [ ] Session create/stop/list/get all functional
- [ ] Config get/set functional

### Performance

- [ ] InProcessClient adds <100μs latency per message (vs direct function call)
- [ ] No measurable throughput degradation for streaming tokens
- [ ] Memory overhead <5MB for server + client infrastructure

### Structural

- [ ] TUI workspace dependencies reduced from 26 to ~5
- [ ] TUI has zero direct imports from core, tools, llm, orchestration
- [ ] Server crate is testable independently (mock runtime)
- [ ] Protocol crate has no async dependency

### Testing

- [ ] Unit tests for all 25+ method handlers
- [ ] Integration test: full TUI session via InProcessClient
- [ ] Integration test: approval flow round-trip
- [ ] Benchmark: streaming throughput vs direct (must be within 95%)
