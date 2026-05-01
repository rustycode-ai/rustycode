# rustycode-web

Browser-based chat interface for RustyCode. React 19 + TypeScript frontend that connects to `rustycode-ws-server` over a WebSocket protocol.

## Architecture

```
Browser (this crate)              Rust backend
┌──────────────────┐              ┌─────────────────────┐
│  React 19 + TS   │   WebSocket  │  rustycode-ws-server │
│  Vite build      │◄────────────►│  (axum)             │
│                  │   v2 proto   │    │                 │
│  ┌────────────┐  │              │    ▼                 │
│  │ Components │  │              │  rustycode-core      │
│  │  MessageList│  │              │    │                 │
│  │  MessageBubble│ │              │    ▼                 │
│  │  InputBar   │  │              │  rustycode-llm       │
│  │  StatusBar  │  │              │  rustycode-tools     │
│  └────────────┘  │              └─────────────────────┘
│  ┌────────────┐  │
│  │ State      │  │
│  │  useReducer│  │
│  │  + event-  │  │
│  │  reducer   │  │
│  └────────────┘  │
│  ┌────────────┐  │
│  │ Protocol   │  │
│  │  WsClient  │  │
│  │  types.ts  │  │
│  └────────────┘  │
└──────────────────┘
```

## Directory Layout

```
src/
├── protocol/         # Transport layer
│   ├── types.ts      # Envelope, StreamEvent, FrontendMessage types
│   └── ws-client.ts  # WsClient: connection, reconnection, heartbeat
├── state/            # State management
│   ├── session-store.ts  # useReducer + SessionContext
│   └── event-reducer.ts  # StreamEvent → FrontendSession mutations
├── hooks/            # React hooks
│   ├── useWebSocket.ts   # WsClient lifecycle + message dispatch
│   └── useSession.ts     # Session provider (sends input, manages state)
├── components/       # UI components
│   ├── App.tsx           # Root: provider + layout
│   ├── MessageList.tsx   # Scrollable message list with auto-scroll
│   ├── MessageBubble.tsx # Per-message rendering (Markdown/pre/plain)
│   ├── InputBar.tsx      # Text input + Send/Stop buttons
│   ├── StatusBar.tsx     # Connection status + tool count
│   └── ErrorBoundary.tsx # React error boundary
├── App.css           # Dark theme styles (oklch colors)
└── main.tsx          # Vite entry point
```

## WebSocket Protocol (v2)

All messages use a JSON envelope:

```ts
{ v: 2, type: string, id: string, payload: unknown }
```

### Client → Server

| Type | Payload | Description |
|------|---------|-------------|
| `hello` | `{ session_token? }` | Handshake. Send token to resume, omit for new session. |
| `input` | `{ content: string }` | User message to process. |
| `abort` | `{}` | Cancel current generation. |
| `heartbeat` | `{ ts: number }` | Keep-alive ping. |

### Server → Client

| Type | Payload | Description |
|------|---------|-------------|
| `session_created` | `{ session_token, capabilities }` | New session established. |
| `session_resumed` | `{ session_token }` | Existing session resumed. |
| `state_snapshot` | `FrontendSession` | Full state replacement. |
| `event` | `{ seq, event: StreamEvent }` | Incremental update (streamed). |
| `heartbeat_ack` | `{ ts, server_ts }` | Heartbeat response. |
| `error` | `{ code, message }` | Server error. |

### Stream Events

The `event` payload carries tagged union `StreamEvent` variants:

- `text_delta` — assistant text chunk
- `thinking_delta` — thinking/reasoning chunk (not rendered in UI currently)
- `tool_call_started` — tool invocation begins
- `tool_input_delta` — tool input streaming
- `tool_exec_started` / `tool_exec_completed` — tool execution lifecycle
- `turn_started` / `turn_completed` — LLM turn boundaries
- `token_usage` — token counts
- `cache_usage` — cache read/creation tokens
- `done` — generation complete

## State Management

Uses `useReducer` with a single `FrontendSession` state object:

```ts
interface FrontendSession {
  input: string;
  messages: FrontendMessage[];
  last_user_prompt: string | null;
  pending_request: boolean;
  tool_iteration_count: number;
  current_response: string;
}
```

Actions flow: `WsClient` → `useWebSocket` dispatch → `sessionReducer` → `applyEvent` → state update → React re-render.

The `event-reducer.ts` mirrors the Rust accumulator in `rustycode-ui-model::accumulator` — the same event-to-state mapping runs on both sides.

## Getting Started

```bash
cd crates/rustycode-web
npm install
npm run dev      # Vite dev server with HMR
npm run build    # Production build → dist/
```

The dev server proxies `/ws` to `ws://127.0.0.1:8080/ws` (configure in `vite.config.ts`). Start the Rust WS server separately:

```bash
cargo run -p rustycode-cli -- start --port 8080
```

## Security Considerations

- **Markdown sanitization**: `react-markdown` configured with `allowedElements` whitelist — no raw HTML rendering.
- **Tool output**: Rendered inside `<pre>` as plain text (React escapes by default).
- **No secrets in frontend**: API keys live in the Rust backend; the browser only holds a session token.

## Known Limitations

- Thinking content (`thinking_delta`) is silently ignored — not rendered in the UI yet.
- No message editing or regeneration.
- No file upload support.
- No multi-session management (single tab per session token).
- Reconnection uses exponential backoff (max 10 attempts) but does not persist session token across page reloads.
