# Web Stack Replacement Design

**Status:** DESIGN
**Replaces:** `WEB_FRONTEND_REPLACEMENT.md`, `WEBSOCKET_PROTOCOL.md` (v1)
**Affects:** `crates/rustycode-web`, `crates/rustycode-ui-core`, `crates/rustycode-ui-model`, `crates/rustycode-tool-server`

---

## 1. Goal

Retire the WASM/ratzilla browser frontend (`rustycode-web`) and replace it with a Rust-native web stack. This is a clean break: the existing browser UI is legacy, shared session logic is preserved only where it serves the new architecture, and the new stack carries zero TUI dependencies.

## 2. What Gets Deleted

| Target | Reason |
|--------|--------|
| `crates/rustycode-web/` (entire crate) | Ratzilla TUI-in-browser. Dead end. |
| `crates/rustycode-ui-core/src/renderer.rs` | Ratatui-specific TUI renderer. Not web-relevant. |
| `crates/rustycode-ui-core/src/syntax_highlighter.rs` | Ratatui-specific highlighter. Replaced by web-native (Prism/highlight.js or server-side). |
| `crates/rustycode-tool-server/` (HTTP `/call` endpoint) | Replaced by WebSocket-native backend. Can be removed once WS is stable. |
| `docs/design/WEB_FRONTEND_REPLACEMENT.md` | Superseded by this doc. |
| `docs/design/WEBSOCKET_PROTOCOL.md` (v1 HTTP transport sections) | HTTP fallback sections removed; WS protocol v2 replaces it. |

## 3. What Gets Preserved

| Target | Why |
|--------|-----|
| `crates/rustycode-ui-model/` | Platform-agnostic session model (`FrontendSession`, `FrontendMessage`, `SubmittedInput`, `RunController`). Used by both TUI and web. No TUI deps. |
| `crates/rustycode-ui-core/src/markdown.rs` | Markdown parsing logic. Extract the parser, drop ratatui `Line`/`Span` output. |
| `crates/rustycode-protocol/src/stream_event.rs` | `StreamEvent` enum — the canonical agent event set. Wire-directly over WebSocket. |
| `crates/rustycode-protocol/src/session_event.rs` | `SyncEvent` envelope — sequencing, timestamps, replay. |
| `crates/rustycode-protocol/src/tool.rs` | `ToolCall`, `ToolResult` types. |
| `crates/rustycode-protocol/src/session.rs` | `SessionId`, session metadata. |

## 4. What Gets Reimplemented

### 4.1 Backend: WebSocket Server

New crate: `rustycode-ws-server`

Responsibility: accept WebSocket connections, bridge to `rustycode-orchestration` / `rustycode-core` session machinery, stream `StreamEvent`s to connected clients.

```
Browser ──ws──▶ rustycode-ws-server ──▶ rustycode-core (session)
                                          │
                                          ▼
                                    rustycode-orchestration (agent loop)
                                          │
                                          ▼
                                    rustycode-llm (providers)
```

**Framework:** `axum` + `tokio-tungstenite` (already in the Rust ecosystem, well-maintained, works with `tower` middleware).

**Key components:**

1. **`WsRouter`** — route handler upgrading HTTP to WS at `/ws`
2. **`SessionManager`** — maps `SessionId` to active agent sessions, handles creation/resumption
3. **`EventBridge`** — subscribes to `rustycode-bus::EventBus`, converts `StreamEvent` → WS frames
4. **`AuthMiddleware`** — validates session tokens (same model as TUI: localhost trust boundary for local, token-based for remote)

### 4.2 Wire Protocol (v2)

Versioned JSON envelope over WebSocket:

```json
{
  "v": 2,
  "type": "<message_type>",
  "id": "<correlation_id>",
  "payload": { ... }
}
```

**Client → Server:**

| Type | Payload | Description |
|------|---------|-------------|
| `hello` | `{session_token, client_info}` | Handshake. Resume or create session. |
| `input` | `{content, metadata}` | User message, slash command, or bang command. |
| `abort` | `{}` | Cancel in-flight request. |
| `heartbeat` | `{ts}` | Keepalive. |

**Server → Client:**

| Type | Payload | Description |
|------|---------|-------------|
| `session_created` | `{session_token, capabilities}` | New session established. |
| `session_resumed` | `{session_token}` | Existing session recovered. |
| `event` | `StreamEvent` | Direct passthrough of `rustycode-protocol::StreamEvent`. This is the core streaming primitive. |
| `state_snapshot` | `FrontendSession` | Full authoritative state (sent on connect and periodically). |
| `heartbeat_ack` | `{ts, server_ts}` | RTT measurement. |
| `error` | `{code, message}` | Typed errors. |

**Design principle:** `StreamEvent` is the protocol. The backend streams `StreamEvent` variants directly as `event` frames. No custom delta type needed — the existing `StreamEvent` enum already covers all streaming cases (TextDelta, ThinkingDelta, ToolCallStarted, ToolExecCompleted, TokenUsage, etc.). The client accumulates state from these events.

### 4.3 Frontend: React/TypeScript SPA

New directory: `crates/rustycode-web-v2/` (or reuse `rustycode-web` name after deletion)

**Stack:** React + TypeScript + Vite

The frontend is a standard SPA that communicates exclusively over WebSocket. No Rust code in the browser — the Rust backend owns all agent logic, tool execution, and state management. The frontend is a pure view/state layer.

**Why React/TS over Rust WASM frameworks:**
- Mature browser tooling (debugging, profiling, browser extensions)
- Fast iteration with HMR — UI changes in <1s
- Rich ecosystem for markdown rendering, syntax highlighting, virtualized lists
- No WASM compilation step for frontend builds
- The WS protocol is the contract — Rust backend and TS frontend evolve independently

**Generated types:** `StreamEvent`, `FrontendSession`, `ToolCall`, `ToolResult` Rust types are serialized over WS as JSON. TypeScript types are maintained manually in the frontend (or generated via `ts-rs` from Rust structs in a future automation pass).

**Component model:**

```
App
├── Header (model, git branch, session timer)
├── MessageList
│   ├── UserMessage
│   ├── AssistantMessage
│   │   ├── ThinkingBlock (collapsible)
│   │   ├── ContentBlock (markdown rendered)
│   │   └── ToolChain (inline, expandable)
│   └── SystemMessage
├── InputBar
│   ├── TextInput (multiline)
│   └── StatusBar (context usage, cost, active tools)
└── Sidebar (sessions, optional)
```

**State management:**

The frontend holds a `FrontendSession` (from `rustycode-ui-model`) as its source of truth. Incoming `StreamEvent` frames mutate it:

- `TextDelta` → append to current assistant message
- `ThinkingDelta` → append to thinking block
- `ToolCallStarted` → add tool entry
- `ToolExecCompleted` → update tool output
- `TokenUsage` → update token counts in status bar
- `TurnCompleted` → finalize current assistant message
- `Done` → mark response complete

Periodic `state_snapshot` frames reset the client to authoritative state, handling any drift.

### 4.4 Markdown Rendering

The current `rustycode-ui-core/src/markdown.rs` outputs ratatui `Line`/`Span`. For the web:

- **Option A:** Server renders markdown → HTML, sends as `String` payload in events
- **Option B:** Client renders markdown using a JS/WASM library (pulldown-cmark compiled to WASM, or a JS library like marked.js)

**Recommendation:** Option B (client-side). Raw markdown in `TextDelta` events, rendered by the browser. Simpler server, better interactivity (code copy, link handling).

## 5. Implementation Phases

### Phase 1: WebSocket Server + Protocol (backend only)

1. Create `crates/rustycode-ws-server/`
2. Implement `WsRouter`, `SessionManager`, `EventBridge`
3. Bridge to `rustycode-core` session lifecycle (create, resume, submit input, abort)
4. Subscribe to `EventBus` and forward `StreamEvent`s as WS frames
5. Protocol v2 envelope serialization/deserialization
6. Integration test: connect WS client → send input → receive streaming events → verify state

**Exit criteria:** A WebSocket endpoint that accepts connections, creates agent sessions, and streams `StreamEvent`s verbatim to a connected client.

### Phase 2: UI Model Cleanup

1. Audit `rustycode-ui-model` — ensure zero TUI dependencies
2. Extract markdown parser from `rustycode-ui-core/markdown.rs` into `rustycode-ui-model` (parser only, no ratatui output)
3. Add `serde` derives where missing for JSON serialization
4. Ensure `rustycode-ui-model` compiles for both `x86_64` and `wasm32-unknown-unknown`

**Exit criteria:** `rustycode-ui-model` is fully platform-neutral, no ratatui deps, compiles for WASM.

### Phase 3: Browser Frontend

1. Scaffold new web crate
2. Implement WebSocket client (connect, reconnect with backoff, heartbeat)
3. Build `FrontendSession` accumulator from `StreamEvent` stream
4. Implement components: MessageList, InputBar, Header, StatusBar
5. Markdown rendering in browser
6. Slash command parsing (reuse `SubmittedInput` from `rustycode-ui-model`)

**Exit criteria:** Functional browser UI that connects to WS backend, sends messages, receives streaming responses.

### Phase 4: Cleanup

1. Delete `crates/rustycode-web/` (legacy ratzilla)
2. Delete `crates/rustycode-tool-server/` (if WS-only path is stable)
3. Delete `crates/rustycode-ui-core/src/renderer.rs` and `syntax_highlighter.rs`
4. Update workspace `Cargo.toml` members
5. Update `CLAUDE.md` architecture docs
6. Remove `docs/design/WEB_FRONTEND_REPLACEMENT.md`

**Exit criteria:** Legacy web stack fully removed. No ratzilla dependency anywhere.

## 6. Dependency Graph

```
rustycode-web-v2/          (React/TypeScript SPA — not a Rust crate)
├── src/
│   ├── components/        (React components)
│   ├── hooks/             (useWebSocket, useSession, useStreamEvents)
│   ├── protocol/          (TS types matching rustycode-protocol StreamEvent)
│   └── state/             (FrontendSession accumulator from StreamEvents)
├── package.json
└── vite.config.ts

rustycode-ws-server/       (Rust crate)
├── rustycode-core         (session lifecycle)
├── rustycode-protocol     (StreamEvent, tool types)
├── rustycode-bus          (EventBus subscription)
├── axum + tokio-tungstenite (WS server)
└── rustycode-ui-model     (FrontendSession for state snapshots)
```

No TUI crates (ratatui, ratzilla) in either path. No WASM.

## 7. Testing Strategy

### Unit Tests
- **Protocol v2:** Envelope serialization roundtrip for every message type
- **UI model:** `FrontendSession` mutation from `StreamEvent` sequence (already partially covered by `rustycode-ui-model` tests)
- **Input parsing:** `SubmittedInput` classification (chat, slash, bang, empty)
- **Markdown parser:** Extracted parser produces correct AST from markdown strings

### Integration Tests
- **WS server:** Connect → hello → input → receive streaming events → verify event sequence
- **Session lifecycle:** Create → chat → stream → done → resume → continue
- **Error recovery:** Server disconnect → client reconnects → session resumes
- **Abort:** Send input → send abort → verify streaming stops

### Regression Tests
- `rustycode-web` (legacy) is no longer in the workspace build path
- `cargo build -p rustycode-ws-server` succeeds independently
- `cargo build -p rustycode-tui` still works (TUI unaffected by web changes)
- No ratzilla/ratatui dependency in `rustycode-web-v2` or `rustycode-ws-server`

## 8. Security Model

Inherits the existing model from `WEBSOCKET_PROTOCOL.md` v1:

- **Local mode:** WebSocket on `127.0.0.1` only, no auth required (same trust boundary as TUI)
- **Remote mode:** Session tokens, `wss://`, origin validation, rate limiting
- **Secret sanitization:** `rustycode-tools/src/security.rs` strips API keys before event emission
- **Frame size limit:** 256KB per frame (larger payloads rejected with error)
- **`#[non_exhaustive]` on `StreamEvent`:** Unknown variants ignored, forward-compatible

## 9. Multi-Frontend Architecture

The design intentionally splits the system into layers that enable any number of frontends without duplication:

```
                    ┌──────────────┐
                    │  Frontend A  │  (React/TS web app)
                    └──────┬───────┘
                           │ WebSocket
                    ┌──────┴───────┐
                    │ rustycode-   │
                    │ ws-server    │ ← shared transport
                    └──────┬───────┘
                           │ StreamEvent (via EventBus)
         ┌─────────────────┼─────────────────┐
         │                 │                  │
  ┌──────┴──────┐  ┌───────┴───────┐  ┌──────┴──────┐
  │ Frontend B  │  │ rustycode-    │  │ Frontend C  │
  │ (TUI)       │  │ core         │  │ (mobile/    │
  │             │  │ (session,    │  │  CLI/IDE    │
  │ consumes    │  │  agent loop) │  │  plugin)    │
  │ EventBus    │  │              │  │             │
  │ directly    │  │              │  │ WS client   │
  └─────────────┘  └──────────────┘  └─────────────┘
```

### Deduplication Layers

| Layer | Crate | What's shared | What's frontend-specific |
|-------|-------|---------------|--------------------------|
| **Wire types** | `rustycode-protocol` | `StreamEvent`, `ToolCall`, `ToolResult`, `SessionId` | Nothing — pure data |
| **View model** | `rustycode-ui-model` | `FrontendSession`, `FrontendMessage`, `SubmittedInput`, `RunController` | Nothing — pure state |
| **Transport** | `rustycode-ws-server` | WS handshake, event framing, session management, heartbeat | Nothing — protocol implementation |
| **State machine** | Frontend-local | — | Each frontend accumulates `StreamEvent` → `FrontendSession` using the same logic |

### The State Accumulator Pattern

The critical shared logic is: **given a sequence of `StreamEvent`s, produce a `FrontendSession`**. This is the same transformation whether you're a TUI or a web app.

**Options for sharing the accumulator:**

1. **Rust crate** (`rustycode-ui-model`): The accumulator logic lives in Rust. The TUI calls it directly. The WS server uses it for `state_snapshot` generation. The React frontend re-implements the same logic in TypeScript (matching the `StreamEvent` → `FrontendSession` mapping). Duplication: the TS re-implementation.

2. **WASM module**: Compile the Rust accumulator to WASM. Both TUI (native) and React (WASM) use the same Rust code. Zero logic duplication, but adds WASM build complexity for the web frontend.

3. **Server-side only**: The accumulator runs only in the WS server. Every frontend receives pre-computed `state_snapshot` frames and never processes raw `StreamEvent`s. Simplest frontend code, but higher bandwidth (full state on every change) and no client-side low-latency rendering.

**Recommendation:** Option 1 (fastest to ship, no WASM). The TS re-implementation is a small reducer matching `StreamEvent` variants to state mutations. It stays in sync because the `StreamEvent` enum is the contract. Option 3 is the fallback if client-side state accumulation proves fragile.

**WASM is not in scope.** The web frontend is a plain React/TypeScript SPA. No Rust runs in the browser.

### Frontend Independence Contract

Any frontend that implements this contract works:

1. **Connect** via WebSocket to `rustycode-ws-server`
2. **Send** `hello`, `input`, `abort`, `heartbeat` frames
3. **Receive** `event` (StreamEvent), `state_snapshot`, `error` frames
4. **Accumulate** StreamEvents into local FrontendSession state
5. **Render** the FrontendSession using platform-native UI
6. **Resync** on state_snapshot — discard local state, use authoritative snapshot

Adding a new frontend means: connect to WS, implement the accumulator, render. No backend changes needed.

## 10. Open Questions

1. **TS type generation:** Use `ts-rs` derive on Rust structs to auto-generate TypeScript types, or maintain manually?
2. **SSR:** Does the web app need server-side rendering for initial load, or is SPA acceptable?
3. **File handling:** Browser can't access filesystem directly. Tool execution must remain server-side. How to handle file upload/download for user-provided files?
4. **Multi-session:** Should one WS connection support multiple concurrent sessions, or one session per connection?
5. **TS type generation:** Use `ts-rs` derive on Rust structs to auto-generate TypeScript types, or maintain manually?
