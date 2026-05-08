# MCP Implementation Roadmap — Complete

**Date:** 2026-05-08  
**Spec Version:** 2025-11-25  
**Scope:** `crates/rustycode-mcp/` (stdio, HTTP/SSE transports, lifecycle, concurrency)  
**Target:** Production-ready MCP with full spec compliance, resilience, and concurrency

## Quick Navigation

- [Overview](#overview) — Status and critical issues
- [Part A: Stdio Lifecycle Fixes](#part-a-stdio-lifecycle-fixes) — Async I/O, multiplexing, keep-alive
- [Part B: HTTP/SSE Transport](#part-b-httpssse-transport) — SSE parser, streaming, session mgmt
- [Part C: Client Integration](#part-c-client-integration) — Capabilities, request IDs, error handling
- [Timeline](#timeline) — 6-week execution plan
- [Verification](#verification) — Spec compliance checklist

---

## Overview

This plan unifies **two critical workstreams**:

1. **Stdio Lifecycle Issues** (blocking I/O, no concurrency, no session state)
2. **HTTP/SSE Transport** (missing SSE parser, no streaming, incomplete session handling)

**Current state:** MCP works for single-threaded, single-request scenarios. Breaks under:
- Long-running operations (>1 min)
- Multiple concurrent requests
- Large responses (>1MB)
- Network interruptions
- Any error condition

## Critical Path & Blockers

```
BLOCKER #1: Async Stdio I/O (blocks all lifecycle work)
  ├─ Replace io::stdin().lock().lines() → tokio::io::stdin()
  ├─ Enable non-blocking reads with timeout
  └─ Unblock: Keep-alive, multiplexing, graceful shutdown

BLOCKER #2: Request Multiplexing (blocks concurrent usage)
  ├─ Track in-flight requests by JSON-RPC id
  ├─ Spawn tokio tasks for each request
  └─ Unblock: Multiple concurrent tool calls

BLOCKER #3: SSE Parser (blocks all HTTP streaming)
  ├─ Implement RFC 6202 SSE parsing state machine
  └─ Unblock: HTTP POST/GET with SSE responses

BLOCKER #4: Capabilities Persistence (blocks spec compliance)
  ├─ Store server capabilities in McpServer
  ├─ Return capabilities in initialize response
  └─ Unblock: Client can detect what server supports
```

**Recommended execution:** A → B → C in parallel, then integrate.

---

## Current State Assessment

### Transport Status

| Transport | Status | Critical Issues | Blocking |
|-----------|--------|-----------------|----------|
| **StdioTransport** | ✅ Spec-compliant | None | No |
| **HttpTransport** | ⚠️ Partial | No SSE parser, no GET listener, no DELETE, no resumability, missing request routing | **YES** |
| **SseTransport** | ❌ Broken | POST-only, rejects SSE responses, missing headers, misleading name | **YES** |

### Protocol Compliance Issues (High Priority)

**CRITICAL** — Block all HTTP/SSE adoption:
1. **Server capabilities not persisted** (client.rs:202) — McpClient initializes but never stores server capabilities, breaking capability detection
2. **No SSE parser** — HttpTransport receives `text/event-stream` but tries to parse as single JSON, losing all events after first
3. **No request-response correlation** — Impossible to match SSE events to pending requests; no routing by JSON-RPC `id`
4. **No background listeners** — GET listener for server-initiated messages not implemented
5. **No session lifecycle** — DELETE endpoint missing, session invalidation handling missing

**HIGH** — Limits real-world usage:
1. **Accept header missing** — POST requests don't advertise SSE support (`Accept: application/json, text/event-stream`)
2. **Hardcoded request IDs** — "init-1", "tools-list-1", etc. violate opaqueness; counter not per-session
3. **Double-initialize not guarded** — No check for second `initialize` request, violates spec error-handling requirement
4. **No Last-Event-ID tracking** — Resumability impossible; clients can't recover from interruptions
5. **No retry field support** — Server-specified reconnect intervals ignored
6. **No session ID header consistency** — Sometimes `Mcp-Session-Id`, sometimes `MCP-Session-Id` (case sensitivity)
7. **No 404 handling** — Session expired but client doesn't recognize or re-initialize

**MEDIUM** — Impacts error recovery:
1. **No 429 (rate limit) handling** — Spec mentions it but no transport-level support
2. **No connection pooling** — Reqwest client rebuilt for each transport; should reuse
3. **No timeout strategy** — Hardcoded 30s everywhere, not tunable per operation
4. **Incomplete error mapping** — SSE parse failures not clearly surfaced to McpClient
5. **No graceful degradation** — If server returns 405 (GET not supported), client should continue with POST-only, but code path unclear

---

# PART A: Stdio Lifecycle & Concurrency Fixes

These fixes address the blocking I/O and single-threaded architecture that makes current MCP unsuitable for production. **Must be done before HTTP work to avoid duplicating problems.**

## Phase A0 — Async Foundation (Critical)

> Convert from synchronous blocking stdio to async non-blocking I/O.

### A0.1 Replace Blocking Stdin with Tokio Async

**File:** `server.rs:run_stdio()`  
**Severity:** 🔴 **CRITICAL BLOCKER** — All concurrency work depends on this

**Current (WRONG):**
```rust
pub async fn run_stdio(&mut self) -> McpResult<()> {
    let stdin = io::stdin();  // ← Synchronous!
    let mut stdout = io::stdout();
    let mut stdin_lines = stdin.lock().lines();  // ← Blocks waiting for input
    
    for line in &mut stdin_lines {
        let response = self.handle_request(request).await;
        self.send_response(&mut stdout, &response)?;
    }
    Ok(())
}
```

**Problem:** 
- `io::stdin().lock().lines()` is synchronous blocking
- If no input arrives, entire server is frozen
- Can't spawn background tasks (keep-alive, timeouts)
- Can't handle multiple requests concurrently

**Fix:**
```rust
use tokio::io::{AsyncBufReadExt, BufReader, AsyncWriteExt};

pub async fn run_stdio(&mut self) -> McpResult<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    
    let mut line = String::new();
    loop {
        // Non-blocking read with timeout
        match tokio::time::timeout(
            Duration::from_secs(300),  // 5-minute idle timeout
            reader.read_line(&mut line)
        ).await {
            Ok(Ok(0)) => {
                // EOF - Claude closed connection
                info!("Client closed connection");
                break;
            }
            Ok(Ok(_)) => {
                // Got a line - process it
                let response = self.handle_request_line(&line).await;
                stdout.write_all(response.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
                line.clear();
            }
            Err(_) => {
                // Timeout - Claude sent nothing for 5 minutes
                warn!("Idle timeout - closing connection");
                break;
            }
            Err(io_err) => {
                error!("I/O error: {}", io_err);
                break;
            }
        }
    }
    
    self.shutdown().await;
    Ok(())
}
```

**Tests:**
- [ ] Non-blocking read with timeout doesn't hang
- [ ] Server continues after EOF gracefully
- [ ] Concurrent background tasks can run
- [ ] Server shuts down cleanly on timeout

**Depends on:** Nothing  
**Estimated size:** ~60 lines  
**Risk:** Low (internal change, same protocol)

### A0.2 Add Explicit Shutdown Cleanup

**File:** `server.rs` (new method)  
**Severity:** 🟡 **HIGH** — Resource cleanup on exit

**Add:**
```rust
impl McpServer {
    pub async fn shutdown(&mut self) {
        info!("MCP server shutting down...");
        
        // Flush any pending operations
        if let Some(executor) = &mut self.tool_executor {
            // Signal executor to clean up
            // (executor will close file handles, abort pending tasks)
        }
        
        // Clear registered resources
        self.resources.write().await.clear();
        self.tools.write().await.clear();
        self.prompts.write().await.clear();
        
        info!("MCP server shutdown complete");
    }
}
```

**Update main():**
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut server = McpServer::new(...);
    
    // Run until error or EOF
    let result = server.run_stdio().await;
    
    // Always cleanup
    server.shutdown().await;
    
    result
}
```

**Tests:**
- [ ] Shutdown called on normal exit
- [ ] Shutdown called on error
- [ ] Resources cleared before process exit

**Depends on:** A0.1  
**Estimated size:** ~40 lines

---

## Phase A1 — Request Multiplexing (High-Impact)

> Enable server to process multiple concurrent requests instead of blocking on one.

### A1.1 Track In-Flight Requests by ID

**File:** `server.rs` (update struct)  
**Severity:** 🔴 **CRITICAL** — Single-threaded bottleneck

**Current problem:**
```
Claude sends request #1 (takes 10s) ──→ Server blocks ──→ Claude waits 10s ──→ Response
Claude can't send request #2 during those 10s
```

**Better:**
```
Claude sends request #1 (takes 10s) ──→ Server spawns task
Claude sends request #2 (takes 2s)  ──→ Server spawns task
                                        Task #2 finishes first ──→ Response for #2
                                        Task #1 finishes later ──→ Response for #1
```

**Implementation:**

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::oneshot;

pub struct McpServer {
    config: McpServerConfig,
    tool_executor: Option<ToolExecutor>,
    tools: Arc<RwLock<HashMap<String, RegisteredTool>>>,
    resources: Arc<RwLock<HashMap<String, McpResourceEntry>>>,
    prompts: Arc<RwLock<HashMap<String, McpPromptTemplate>>>,
    initialized: Arc<RwLock<bool>>,
    
    // NEW: Track in-flight requests
    pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    
    // NEW: Limit concurrent requests (prevents resource exhaustion)
    request_semaphore: Arc<tokio::sync::Semaphore>,  // max 50 concurrent
}

impl McpServer {
    pub fn new(name: impl Into<String>, config: McpServerConfig) -> Self {
        Self {
            // ... existing fields ...
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
            request_semaphore: Arc::new(tokio::sync::Semaphore::new(50)),
        }
    }
}
```

**Update request handling:**

```rust
async fn run_stdio(&mut self) -> McpResult<()> {
    // ... read line into `line` ...
    
    let request = JsonRpcRequest::from_json(&line)?;
    let request_id = request.id.clone();
    
    // Acquire permit (limits to 50 concurrent)
    let permit = self.request_semaphore.acquire().await.unwrap();
    
    // Create response channel for this request
    let (tx, rx) = oneshot::channel();
    self.pending_requests.write().await.insert(request_id.clone(), tx);
    
    // Spawn background task for this request
    let server = self.clone();  // Requires: impl Clone for McpServer
    tokio::spawn(async move {
        let response = server.handle_request(request).await;
        
        // Send response back through channel
        if let Some(tx) = server.pending_requests.write().await.remove(&request_id) {
            let _ = tx.send(response);
        }
        
        drop(permit);  // Release semaphore permit
    });
}
```

**Tests:**
- [ ] Multiple concurrent requests processed in parallel
- [ ] Responses can arrive out-of-order
- [ ] Semaphore limits to 50 concurrent
- [ ] Each response has correct request id
- [ ] No responses lost or duplicated

**Depends on:** A0.1  
**Estimated size:** ~100 lines  
**Risk:** Medium (introduces concurrency, needs careful testing)

### A1.2 Implement Request ID Counter

**File:** `client.rs` + `server.rs`  
**Severity:** 🟡 **HIGH** — Spec compliance

**Current problem:** Hardcoded IDs ("init-1", "tools-list-1") are not unique across multiple clients.

**Fix (client.rs):**
```rust
use std::sync::atomic::{AtomicU64, Ordering};

pub struct McpClient {
    // ... existing fields ...
    request_id_counter: Arc<AtomicU64>,  // New
}

impl McpClient {
    pub fn new(config: McpClientConfig) -> Self {
        Self {
            // ...
            request_id_counter: Arc::new(AtomicU64::new(0)),
        }
    }
    
    fn next_request_id(&self) -> String {
        let count = self.request_id_counter.fetch_add(1, Ordering::Relaxed);
        format!("req-{}-{}", std::process::id(), count)  // Include PID for uniqueness
    }
}
```

**Update all request building:**
```rust
// OLD:
let request = JsonRpcRequest::new("init-1", "initialize");

// NEW:
let request = JsonRpcRequest::new(self.next_request_id(), "initialize");
```

**Tests:**
- [ ] IDs unique across 1000 requests
- [ ] IDs unique across multiple McpClient instances
- [ ] ID format is valid JSON string

**Depends on:** Nothing  
**Estimated size:** ~30 lines

---

## Phase A2 — Keep-Alive & Health Checking

> Detect and prevent stuck/hung servers.

### A2.1 Spawn Keep-Alive Task

**File:** `server.rs`  
**Severity:** 🟡 **MEDIUM** — Detectability

**Problem:** Claude can't tell if server is hung until it sends the next request.

**Solution:** Server sends periodic "ping" messages (even if Claude sends nothing).

```rust
impl McpServer {
    async fn spawn_keep_alive_task(&self) {
        let pending = self.pending_requests.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Only send if no requests in-flight
                let num_pending = pending.read().await.len();
                if num_pending == 0 {
                    // Send keep-alive (Claude can respond with {})
                    eprintln!("{}", serde_json::json!({
                        "method": "ping",
                        "params": {}
                    }));
                }
            }
        });
    }
}
```

Call in `run_stdio()`:
```rust
pub async fn run_stdio(&mut self) -> McpResult<()> {
    self.spawn_keep_alive_task();  // Start background task
    
    // ... rest of loop ...
}
```

**Tests:**
- [ ] Keep-alive pings sent every 30s when idle
- [ ] No pings sent when requests in-flight
- [ ] Claude can handle ping messages

**Depends on:** A1.1  
**Estimated size:** ~40 lines

### A2.2 Request Timeout Enforcement

**File:** `server.rs`  
**Severity:** 🟡 **MEDIUM** — Preventing hangs

**Problem:** A single slow tool call blocks responses for all other requests (even multiplexed).

**Solution:** Enforce per-request timeout.

```rust
async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
    let timeout = Duration::from_secs(self.config.timeout_secs);
    
    match tokio::time::timeout(timeout, self.process_request(request.clone())).await {
        Ok(response) => response,
        Err(_) => {
            // Timeout exceeded
            JsonRpcResponse::error(
                request.id.clone(),
                -32603,  // Internal error
                format!("Request timeout ({}s exceeded)", self.config.timeout_secs)
            )
        }
    }
}
```

**Tests:**
- [ ] Requests timeout after configured duration
- [ ] Timeout response includes duration
- [ ] Other requests continue during timeout

**Depends on:** Nothing  
**Estimated size:** ~20 lines

---

## Phase A3 — Session State Management

> Track connection state and client capabilities.

### A3.1 Store Client Capabilities in Server

**File:** `server.rs`  
**Severity:** 🟡 **MEDIUM** — Feature detection

**Current problem:** Server doesn't know what client supports (sampling? subscriptions?).

```rust
#[derive(Clone)]
pub struct ClientSession {
    initialized_at: Instant,
    client_capabilities: Option<ClientCapabilities>,
    client_name: Option<String>,
}

pub struct McpServer {
    // ... existing fields ...
    session: Arc<RwLock<Option<ClientSession>>>,  // New
}

async fn handle_initialize(&self, params: Option<Value>) -> McpResult<Value> {
    // Parse client capabilities
    let client_caps = ClientCapabilities::from_json(params)?;
    
    // Store in session
    let mut session = self.session.write().await;
    *session = Some(ClientSession {
        initialized_at: Instant::now(),
        client_capabilities: Some(client_caps.clone()),
        client_name: client_caps.tools.as_ref().map(|_| "claude".to_string()),
    });
    
    // ... rest of initialize ...
}

pub async fn client_capabilities(&self) -> Option<ClientCapabilities> {
    self.session.read().await.as_ref()
        .and_then(|s| s.client_capabilities.clone())
}
```

**Tests:**
- [ ] Client capabilities stored after initialize
- [ ] Can query capabilities from handlers
- [ ] Session cleared on connection end

**Depends on:** Nothing  
**Estimated size:** ~60 lines

---

# PART B: HTTP/SSE Transport Fixes

These work in parallel with Part A. Reuse the async patterns from A0-A3.

## Phase B0 — Prerequisite: SSE Parser

(From original plan — this is unchanged, just moved to Part B)

[See original Phase 1 section below: "Phase 1 — SSE Frame Parser"]

## Phase B1 — HTTP Transport SSE Support

(From original plan — builds on B0's SSE parser)

[See original Phase 2 section below]

---

# PART C: Client Integration & Spec Compliance

> **PRIORITY:** These fixes unblock Phases 1-7. No new features, but essential for protocol compliance.

### 0.1 Store server capabilities during initialization ⚠️ CRITICAL

**File:** `client.rs:202-212`  
**Spec:** §4.1 (Initialize Response) — Server capabilities MUST be available to client

**Problem:** InitializeResponse parsed but capabilities never persisted. Callers can't check what server supports (tools? resources? logging?). This breaks capability-aware client logic.

**Fix:**
```rust
// In McpClient::initialize()
let response = ...
self.server_capabilities = response.capabilities.clone();  // ADD THIS

// Add public accessor
pub fn server_capabilities(&self, server_id: &str) -> Option<&ServerCapabilities> {
    self.sessions.get(server_id).map(|s| &s.server_capabilities)
}
```

**Tests:**
- [ ] Capabilities persisted after initialize
- [ ] Multiple servers have independent capabilities
- [ ] Accessor returns None for unknown server

### 0.2 Dynamic request IDs ⚠️ CRITICAL

**File:** `client.rs`  
**Spec:** §3.2 (Request ID) — IDs MUST be unique per session, MAY be any JSON value

**Problem:** Hardcoded IDs ("init-1", "tools-list-1", "tool-call-1") violate uniqueness guarantee if multiple clients share same session. Also violates opaqueness (server may interpret numbers as commands).

**Fix:**
```rust
// Add to McpClient
request_id_counter: AtomicU64,

// In each request
let id = self.request_id_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
let request_id = format!("req-{}-{}", server_id_hash, id);  // Include server to avoid cross-server collisions
```

**Tests:**
- [ ] IDs unique across 100 requests
- [ ] IDs unique across multiple server sessions
- [ ] ID format is valid JSON string

### 0.3 Double-initialize guard ⚠️ CRITICAL

**File:** `server.rs`  
**Spec:** §5.3 (Server State) — Second initialize MUST return error

**Problem:** No guard on second `initialize`. Spec requires `-32600 (Invalid Request)` or equivalent.

**Fix:**
```rust
// In McpServer::handle_initialize()
if self.initialized {
    return Err(JsonRpcError {
        code: -32600,
        message: "Invalid request: already initialized".to_string(),
    });
}
self.initialized = true;
```

**Tests:**
- [ ] First initialize succeeds
- [ ] Second initialize returns -32600
- [ ] State doesn't change on failed second initialize

### 0.4 Accept header on all POST requests ⚠️ CRITICAL

**File:** `http_transport.rs:55-65`  
**Spec:** §6.2.1 (HTTP POST) — Accept header MUST list supported response types

**Problem:** Current code doesn't send `Accept` header. Server can't know if client supports SSE or expects JSON-only response.

**Fix:**
```rust
// In HttpTransport::send_request()
.header("Accept", "application/json, text/event-stream")  // ADD THIS LINE
.header("Accept-Encoding", "identity")  // Disable compression to avoid SSE issues
```

**Tests:**
- [ ] POST request includes Accept header
- [ ] Server receiving Accept can respond with SSE or JSON

### 0.5 Case-consistent MCP headers ⚠️ HIGH

**File:** `http_transport.rs` (lines 60, 89, etc.)  
**Spec:** §6.1 (HTTP Headers) — Standard header casing (MCP-Protocol-Version, MCP-Session-Id)

**Problem:** Inconsistent casing (`Mcp-Session-Id` vs `MCP-Session-Id`). HTTP headers case-insensitive but RustyCode should be consistent with spec examples.

**Fix:**
```rust
// Standardize to PascalCase with dashes
req = req.header("MCP-Protocol-Version", crate::MCP_VERSION);
req = req.header("MCP-Session-Id", session_id);  // Not "Mcp-Session-Id"
```

**Tests:**
- [ ] All MCP headers use consistent casing
- [ ] Headers survive round-trip through reqwest/server

## Phase 1 — SSE Frame Parser

> **CRITICAL BLOCKER** for Phases 2+. Foundation for all HTTP transport work. Spec compliance depends 100% on this.

### 1.1 Implement `sse.rs` module

**File:** New `crates/rustycode-mcp/src/sse.rs`  
**Spec:** RFC 6202 (Server-Sent Events), MCP §6.2.2 (HTTP SSE Response)

**Requirements:**
- Streaming parser (not load-all-then-parse) — handles multi-megabyte streams
- Strict compliance with SSE spec including BOM handling, line ending variants
- Per-event metadata: data, event type, id, retry
- Comment line handling (lines starting with `:`)
- Multi-line data concatenation per spec (lines ending with `:` mean "continue")

**State Machine:**
```
EXPECT_FIELD_NAME
  - Read until ':' or '\n'
  - If empty line → dispatch event, reset fields
  - If ':' → go to EXPECT_FIELD_VALUE
  - If line starts with ':' → comment, skip

EXPECT_FIELD_VALUE
  - Skip leading space (at most one) after ':'
  - Read until '\n'
  - Accumulate field
  - Return to EXPECT_FIELD_NAME
```

**Events to Emit:**
```rust
pub struct SseEvent {
    pub data: String,           // "data:" field (multi-line joined with \n)
    pub event: Option<String>,  // "event:" field (optional)
    pub id: Option<String>,     // "id:" field (optional, for Last-Event-ID)
    pub retry: Option<u64>,     // "retry:" in milliseconds (optional, for reconnect)
}
```

**Spec Compliance Checklist:**
- [ ] BOM (U+FEFF) stripped from stream start
- [ ] LF (\n), CRLF (\r\n), and CR (\r) all supported as line endings
- [ ] Multiple consecutive colons (`:`) in field line handled correctly
- [ ] Empty data field treated as valid event (with empty string data)
- [ ] Line length limits enforced if spec requires (check RFC 6202 — it doesn't)
- [ ] Comment lines (`:anything`) ignored without error
- [ ] Dispatch happens on blank line (empty field name)
- [ ] Multi-line data preserved exactly: newlines between lines, no extra ones

### 1.2 SSE Parser Tests

**Unit tests** (no I/O):

1. **Single event:** `data: hello\n\n` → event with data="hello"
2. **Multi-line data:** `data: line1\ndata: line2\n\n` → event with data="line1\nline2"
3. **Event type:** `event: message\ndata: hello\n\n` → event with event="message"
4. **Event ID:** `id: 1\ndata: hello\n\n` → event with id="1"
5. **Retry field:** `retry: 5000\ndata: hello\n\n` → event with retry=5000
6. **Comments:** `:this is ignored\ndata: hello\n\n` → event with data="hello"
7. **Empty data:** `data:\n\n` → event with data=""
8. **Multiple events in chunk:** `data: a\n\ndata: b\n\n` → 2 events
9. **Fragmented chunk:** Receive `data: he`, then `llo\n\n` → still parses correctly
10. **BOM handling:** U+FEFF at stream start stripped, doesn't appear in data
11. **Line ending variants:** LF, CRLF, CR all treated as line endings
12. **Colon in field name:** `:` after field name separates field from value
13. **Empty event:** Just `\n\n` (no fields) — ignore or dispatch with all fields None?
14. **Mixed case event field:** `event: MyEvent` → event="MyEvent" (case preserved)

**Integration tests** (with mock SSE server):

- [ ] Parse real HTTP 200 with `Content-Type: text/event-stream`
- [ ] Parse stream with chunked transfer encoding
- [ ] Handle stream disconnection mid-event (incomplete event)
- [ ] Handle stream disconnect after complete event
- [ ] 100 events in sequence
- [ ] 1MB event data

**Depends on:** Nothing  
**Estimated size:** ~250 lines parser + ~200 lines tests  
**Risk:** Low (pure parsing, no side effects)

## Phase 2 — HttpTransport Overhaul (SSE Support)

> **HIGH IMPACT.** Current HTTP transport tries to parse `text/event-stream` as single JSON. This phase fixes that fundamental design flaw.

### 2.1 Accept header (from Phase 0.4)

**Already covered in Phase 0.4** — Part of zero-risk fixes. Every POST must include:
```
Accept: application/json, text/event-stream
```

### 2.2 SSE-aware POST response handling

**File:** `http_transport.rs:send_request()` (lines 53-95)  
**Spec:** §6.2.2 (HTTP POST Response) — Server may respond with JSON or SSE stream

**Current problem:** Receives `Content-Type: text/event-stream` but tries to parse entire body as JSON. All events after first lost.

**Fix — Two-path response handler:**

```rust
match response.headers().get(CONTENT_TYPE) {
    Some("application/json") => {
        // Path A: Single JSON response (current behavior)
        let text = response.text().await?;
        JsonRpcResponse::from_json(&text)
    }
    Some("text/event-stream") => {
        // Path B: Stream of events (new)
        // 1. Take response body as stream
        // 2. Create SSE parser
        // 3. Read first event, extract JSON-RPC response
        // 4. Return that response to caller
        // 5. Spawn background task for remaining events (see 2.3)
        let stream = response.bytes_stream();
        let (first_response, remaining_events) = parse_sse_stream(stream).await?;
        
        // Forward remaining events to inbox
        if let Some(tx) = &self.inbox_sender {
            spawn_background_event_processor(remaining_events, tx.clone());
        }
        
        first_response
    }
    _ => Err("Unsupported Content-Type")
}
```

### 2.3 Background SSE stream reader (POST response)

**File:** `http_transport.rs` (new async task)  
**Spec:** §4.4 (Notifications) — Server can send notifications during POST response stream

**Problem:** After returning first response to caller, subsequent SSE events are lost. Per spec, server can push notifications mid-stream.

**Fix — Spawn background task after POST returns:**

```rust
async fn process_sse_events(mut parser: SseParser, inbox_tx: mpsc::Sender<IncomingMessage>) {
    while let Some(event) = parser.next().await {
        let data = event.data;
        
        // Try to parse as JSON-RPC message
        if let Ok(msg) = serde_json::from_str::<JsonRpcMessage>(&data) {
            // Route by type:
            // - Response (has `result` or `error`) → look up pending request, deliver via oneshot
            // - Notification (no `id`) → forward to inbox
            // - Request (has `method`, `id`) → forward to inbox
            
            match msg {
                JsonRpcMessage::Response { id, .. } => {
                    if let Some(tx) = pending_requests.remove(&id) {
                        let _ = tx.send(response);  // Ignore if receiver dropped
                    }
                }
                JsonRpcMessage::Notification { .. } => {
                    let _ = inbox_tx.send(IncomingMessage::Notification(msg)).await;
                }
                JsonRpcMessage::Request { .. } => {
                    let _ = inbox_tx.send(IncomingMessage::Request(msg)).await;
                }
            }
        }
    }
}
```

**Spec Compliance:**
- [ ] Server can interleave responses and notifications in single SSE stream
- [ ] Responses routed back to calling `.send_request()` via oneshot
- [ ] Notifications go to `receive()` inbox
- [ ] Task exits cleanly on stream EOF
- [ ] Task handles parse errors gracefully (log, skip event, continue)

### 2.4 Request ID routing

**Spec:** §3.2 (Request/Response Matching) — Match by JSON-RPC `id` field

**Current:** Uses HashMap<String, oneshot::Sender<Response>> to track pending requests.

**Fix — Ensure ID is always set:**
```rust
// In send_request() — add to pending_requests map
let request_id = request.id.clone();
let (tx, rx) = oneshot::channel();
self.pending_requests.insert(request_id.clone(), tx);

// In background processor
// When response arrives with matching id, look it up and send
if let Some(responder) = self.pending_requests.remove(&response_id) {
    let _ = responder.send(response);
}
```

**Depends on:** Phase 0 (Accept header, request IDs), Phase 1 (SSE parser)  
**Estimated size:** ~350 lines  
**Risk:** Medium (involves background tasks, channel plumbing)

## Phase 3 — GET Listener for Server-Initiated Messages

> **OPTIONAL but HIGH-VALUE.** Enables true bidirectional messaging (server push). Many real-world servers won't support this, but when available, critical for responsive interactions.

### 3.1 GET endpoint connection

**File:** New `http_transport.rs::get_listener_task()`  
**Spec:** §6.2.3 (HTTP GET for Subscriptions) — Server MAY accept GET to enable server-initiated messages

**Requires:**
1. After successful `initialize`, spawn GET listener task
2. GET to same endpoint with headers:
   ```
   Accept: text/event-stream
   MCP-Protocol-Version: 2025-11-25
   MCP-Session-Id: <session_id>
   ```
3. Read SSE stream indefinitely (long-lived connection)
4. Route messages as they arrive

**Implementation:**
```rust
async fn start_get_listener(url: String, session_id: String, inbox_tx: mpsc::Sender<IncomingMessage>) {
    loop {
        match client.get(&url)
            .header("Accept", "text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25")
            .header("MCP-Session-Id", &session_id)
            .send()
            .await
        {
            Ok(resp) if resp.status() == 405 => {
                // Server doesn't support GET listener — graceful degradation
                warn!("Server doesn't support GET listener (405). Continuing with POST-only.");
                break;
            }
            Ok(resp) if resp.status() == 404 => {
                // Session expired
                let _ = inbox_tx.send(IncomingMessage::SessionExpired).await;
                break;
            }
            Ok(resp) if resp.status().is_success() => {
                // Process SSE stream
                process_sse_stream(resp, &inbox_tx).await;
                // On disconnect, wait backoff then reconnect
                tokio::time::sleep(backoff_duration).await;
                continue;
            }
            Err(e) => {
                warn!("GET listener error: {}", e);
                tokio::time::sleep(backoff_duration).await;
                continue;
            }
        }
    }
}
```

### 3.2 Lifecycle management

**Spec Compliance Checklist:**

- [ ] GET listener starts AFTER initialize completes
- [ ] GET listener uses SAME session_id as POST requests
- [ ] GET listener respects same protocol version header
- [ ] Server-initiated requests routed to inbox (caller can handle or error)
- [ ] Notifications routed to inbox/event bus
- [ ] Keep-alive comments (`:` lines) silently ignored
- [ ] Stream disconnection triggers reconnect with exponential backoff (1s, 2s, 4s, max 5m)
- [ ] Graceful degradation: 405 (Method Not Allowed) → log, disable GET listener, continue POST-only
- [ ] 404 (Not Found) → session expired, emit special message, may trigger re-init
- [ ] GET listener closed on `client.close()`
- [ ] In-flight GET requests aborted on close (tokio::task::JoinHandle abort)

**Error Handling:**
- [ ] Network error → backoff + reconnect
- [ ] SSE parse error → backoff + reconnect (don't crash)
- [ ] Server-initiated request (no `id`) → forward to inbox, caller must handle
- [ ] Invalid JSON in event → skip event, continue parsing

**Depends on:** Phase 0 (dynamic IDs, headers), Phase 1 (SSE parser), Phase 2 (request routing)  
**Estimated size:** ~250 lines  
**Risk:** Medium (long-lived connections, backoff logic)

## Phase 4 — DELETE Session Termination + Cleanup

> **IMPORTANT for stateful servers.** Enables clean session shutdown and server resource cleanup.

### 4.1 DELETE endpoint

**File:** `http_transport.rs::close()`  
**Spec:** §6.2.4 (HTTP DELETE) — SHOULD send DELETE on session end for cleanup

**When client calls `close()`:**

```rust
async fn close(&mut self) -> McpResult<()> {
    // 1. Send DELETE to notify server
    if let Some(ref session_id) = self.session_id {
        let _ = self.client.delete(&self.url)
            .header("MCP-Session-Id", session_id)
            .header("MCP-Protocol-Version", crate::MCP_VERSION)
            .send()
            .await;  // Ignore errors on DELETE — session may already be gone
    }
    
    // 2. Stop GET listener task
    if let Some(handle) = self.get_listener_handle.take() {
        handle.abort();
    }
    
    // 3. Drain pending requests (return error to any in-flight calls)
    for (_id, tx) in self.pending_requests.drain() {
        let _ = tx.send(Err(McpError::ConnectionClosed));
    }
    
    // 4. Close inbox (receivers will get None on next recv)
    drop(self.inbox_sender.take());
    
    self.connected = false;
    Ok(())
}
```

**Spec Compliance:**
- [ ] DELETE includes MCP-Session-Id header
- [ ] DELETE includes MCP-Protocol-Version header
- [ ] DELETE sent BEFORE aborting GET listener
- [ ] Error on DELETE doesn't block close (session may be gone)
- [ ] Pending requests error out with ConnectionClosed

### 4.2 Session invalidation handling (404 / Session Expired)

**Spec:** §6.2.4 (HTTP Errors) — 404 means session not found, client should re-initialize

**Problem:** Currently no 404 detection or handling. Client unaware session died.

**Fix — Two places detect 404:**

1. **In send_request():**
   ```rust
   if resp.status() == 404 {
       self.session_id = None;  // Clear stale session
       return Err(McpError::SessionExpired(
           "Server session not found. Re-initialize to continue."
       ));
   }
   ```

2. **In GET listener:**
   ```rust
   if resp.status() == 404 {
       let _ = inbox_tx.send(IncomingMessage::SessionExpired).await;
       break;  // Exit listener
   }
   ```

3. **In McpClient (catch SessionExpired):**
   ```rust
   pub async fn send_request(&mut self, server_id: &str, ...) -> McpResult<JsonRpcResponse> {
       match transport.send_request(...).await {
           Err(McpError::SessionExpired) => {
               // Maybe auto-reinitialize? Or let caller decide?
               // For now, propagate and let manager handle
               Err(McpError::SessionExpired)
           }
           other => other
       }
   }
   ```

**New error variant:**
```rust
pub enum McpError {
    SessionExpired(String),  // NEW
    // ... existing variants
}
```

**Spec Compliance:**
- [ ] 404 detected in both POST and GET paths
- [ ] Session cleared on 404
- [ ] Caller informed via SessionExpired error
- [ ] Manager can use SessionExpired to trigger reconnect (Phase 7)

**Depends on:** Phase 2-3  
**Estimated size:** ~80 lines

## Phase 5 — Resumability (Last-Event-ID)

> **OPTIONAL, BEST-EFFORT.** Enables recovery from network interruptions without losing messages. Many servers won't implement this; graceful degradation if not supported.

### 5.1 Track last event ID

**File:** `http_transport.rs` (GET listener task)  
**Spec:** RFC 6202 (SSE) — Clients MAY use Last-Event-ID header to resume from interruption

**Implementation:**
```rust
// In HttpTransport
last_event_id: Option<String>,  // Add field

// In GET listener task, after each event
if let Some(id) = &event.id {
    self.last_event_id = Some(id.clone());
    // Persist to disk (optional) for recovery across restarts
    // For now, just keep in memory
}
```

**Spec Compliance:**
- [ ] All event IDs from SSE stream tracked
- [ ] Last ID remembered for reconnection
- [ ] Even without Last-Event-ID, client still processes all events during continuous connection

### 5.2 Send Last-Event-ID on reconnect

**File:** `http_transport.rs::get_listener_task()` (reconnect loop)  
**Spec:** RFC 6202 — Last-Event-ID header allows server to replay events

**When reconnecting GET listener after error:**

```rust
let req = client.get(&url)
    .header("Accept", "text/event-stream")
    .header("MCP-Protocol-Version", "2025-11-25")
    .header("MCP-Session-Id", &session_id);

// Only send Last-Event-ID if we have one
if let Some(ref last_id) = self.last_event_id {
    req = req.header("Last-Event-ID", last_id);
}
```

**Server behavior (from spec):**
- If server supports resumability: replays events after given ID
- If server doesn't support: ignores header, sends new events from current state
- Result: Either resume (ideal) or catch up from now (acceptable fallback)

**Important limitations:**
- [ ] Last-Event-ID is best-effort — server may not support resumability
- [ ] Events may be lost if server drops them before resumption
- [ ] Resumability only works during continuous session — session expiration (404) resets it
- [ ] Test explicitly with servers that do and don't support Last-Event-ID

### 5.3 Retry field handling

**File:** `http_transport.rs::get_listener_task()` (backoff logic)  
**Spec:** RFC 6202 — `retry:` field specifies reconnection delay

**Implementation:**
```rust
// Default backoff
let mut backoff_ms = 1000u64;  // Start at 1 second

loop {
    // ... attempt GET ...
    
    if disconnected {
        // Check if any events had a retry field
        for event in events_received_before_disconnect {
            if let Some(server_backoff) = event.retry {
                backoff_ms = server_backoff;  // Use server's suggestion
                warn!("Server requested retry interval: {}ms", backoff_ms);
            }
        }
        
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
        backoff_ms = (backoff_ms * 2).min(300_000);  // Exponential backoff, cap at 5 min
    }
}
```

**Spec Compliance:**
- [ ] Parse retry field from SSE events
- [ ] Apply server's suggested retry interval if present
- [ ] Fall back to exponential backoff if not
- [ ] Cap backoff at reasonable maximum (5 minutes)
- [ ] Test with server that sends retry fields

**Depends on:** Phase 3 (GET listener)  
**Estimated size:** ~100 lines  
**Risk:** Low (parsing, configuration)

## Phase 6 — SseTransport Removal / Repurpose

> **CLEANUP.** SseTransport is currently broken and misleading. This phase removes or clarifies it.

### 6.1 Assessment and Decision

**Current state of SseTransport:**
- POST-only (no GET listener for server-initiated messages)
- Tries to parse SSE as single JSON (loses events)
- Missing Accept header
- Broken protocol implementation
- Misleadingly named (suggests SSE support, but doesn't really)

**Option A — Remove entirely (RECOMMENDED):**
- Delete `crates/rustycode-mcp/src/sse_transport.rs`
- Update `manager.rs` to treat `McpTransportType::Sse` as alias for `Http`
- Document: "SseTransport removed — use HttpTransport for all HTTP endpoints"
- Remove from re-exports in `lib.rs`

**Option B — Keep as type alias:**
- Make SseTransport = HttpTransport (type alias)
- Provides backward compatibility if external code imports it
- But doesn't clarify that behavior is identical
- Risk: Confuses future developers

**Option C — Keep as wrapper with clear intent:**
- SseTransport wraps HttpTransport but disables GET listener
- For servers that explicitly don't support GET
- Add clear documentation
- More boilerplate for questionable benefit

**Recommendation:** **Option A — Remove it.** Phase 2 makes HttpTransport handle SSE correctly. SseTransport adds no value and is actively harmful (suggests distinct behavior where none exists).

**Migration path:**
```rust
// Old code
let transport = SseTransport::new(url, headers)?;

// New code
let transport = HttpTransport::new(url, headers)?;
// HttpTransport now supports both JSON and SSE automatically
```

### 6.2 MCP-Protocol-Version header audit

**File:** All transport files  
**Spec:** §6.1 (HTTP Headers) — Every HTTP request MUST include MCP-Protocol-Version

**Audit checklist:**
- [ ] StdioTransport: N/A (not HTTP)
- [ ] HttpTransport POST: Has it (line 60)
- [ ] HttpTransport GET listener: Add it (Phase 3)
- [ ] HttpTransport DELETE: Add it (Phase 4)
- [ ] SseTransport: Either remove (Option A) or fix (option C)

**Fix locations:**
```rust
// HttpTransport::send_request()
.header("MCP-Protocol-Version", crate::MCP_VERSION)  // ✓ Already there

// HttpTransport::get_listener_task() (new in Phase 3)
.header("MCP-Protocol-Version", crate::MCP_VERSION)  // ADD

// HttpTransport::close() — DELETE request (new in Phase 4)
.header("MCP-Protocol-Version", crate::MCP_VERSION)  // ADD
```

**Depends on:** Phase 2-4 (after which SseTransport truly becomes redundant)  
**Estimated size:** ~20 lines (mostly deletion)

## Phase 7 — Client & Manager Integration Polish

> **FINAL INTEGRATION.** Glues all transport work into the broader system. Makes HTTP/SSE transparent to callers.

### 7.1 Transport-agnostic McpClient

**File:** `client.rs`  
**Goal:** Client behavior identical over stdio and HTTP

**Verification checklist:**

1. **connect_stdio() path:**
   - [ ] Works today (no changes needed)
   - [ ] Test: can initialize, list_tools, call tools

2. **connect_http(url) path:**
   - [ ] Works through fixed HttpTransport
   - [ ] Test: can initialize, list_tools, call tools
   - [ ] Server capabilities populated (Phase 0.1)
   - [ ] Request IDs unique (Phase 0.2)
   - [ ] Accepts SSE responses (Phase 2)
   - [ ] Receives server-initiated messages if available (Phase 3)

3. **Identical behavior:**
   - [ ] Both paths handle timeouts
   - [ ] Both paths propagate errors clearly
   - [ ] Both paths work with multi-server connections
   - [ ] Capabilities queried same way regardless of transport

### 7.2 Manager integration

**File:** `manager.rs`  
**Goal:** McpServerManager transparently handles HTTP reconnection and error recovery

**Verification checklist:**

1. **Transport detection:**
   - [ ] command → StdioTransport
   - [ ] url → HttpTransport (with full SSE support)
   - [ ] SseTransport config → migrated to HttpTransport (with deprecation warning)

2. **Health monitoring + reconnection:**
   - [ ] Manager detects transport disconnection
   - [ ] On disconnect: wait backoff, call reconnect()
   - [ ] Reconnect obtains new session (fresh initialize)
   - [ ] New session refreshes tool cache
   - [ ] Max reconnection attempts enforced

3. **Session expiration handling (404):**
   - [ ] Transport returns McpError::SessionExpired
   - [ ] Manager catches it and triggers reconnect
   - [ ] Loop: if max attempts exceeded, mark Unhealthy
   - [ ] User can manually restart server

4. **Rate limiting (429):**
   - [ ] Transport detects HTTP 429
   - [ ] Manager respects Retry-After header (if present)
   - [ ] Falls back to exponential backoff
   - [ ] Logs warning to help diagnose quota issues

### 7.3 Error propagation & mapping

**File:** `transport.rs` + `client.rs`  
**Goal:** Clear error semantics from HTTP errors to `McpError` variants

**New error variants needed:**
```rust
pub enum McpError {
    // ... existing variants
    SessionExpired(String),     // HTTP 404 — session not found
    RateLimited(String),        // HTTP 429 — quota exceeded
    Timeout(String),            // HTTP timeout — connection hung
    ProtocolViolation(String),  // Invalid MCP message format
    TransportError(String),     // Network, SSE parse, etc.
}
```

**Error mapping:**

| HTTP/SSE Event | McpError Variant | Handling | Auto-Retry? |
|---|---|---|---|
| HTTP 200 ✓ | N/A | Success | N/A |
| HTTP 404 | SessionExpired | Manager triggers reconnect | Yes |
| HTTP 429 | RateLimited | Backoff, warn, retry | Yes |
| HTTP 500+ | TransportError | Log, fail request | Yes (manager) |
| Connection timeout | Timeout | Fail request, manager reconnects | Yes |
| SSE parse error | TransportError | Log, skip event, continue | Yes (continue) |
| Invalid JSON-RPC | ProtocolViolation | Log, skip message | Yes (continue) |

**Implementation locations:**
- Transport layer detects HTTP status, SSE errors → maps to McpError
- Manager layer catches McpError, decides retry strategy
- Client layer propagates to user code for handling

**Spec Compliance:**
- [ ] All HTTP errors mapped to appropriate McpError
- [ ] Error messages include context (server, request, reason)
- [ ] Manager's retry strategy respects Retry-After headers
- [ ] Exponential backoff has reasonable caps (max 5 min)
- [ ] User code can distinguish network errors from protocol errors

### 7.4 Integration tests

**New test files:**

1. **tests/http_transport_integration.rs:**
   - Mock HTTP server (using mockito or similar)
   - Test POST with JSON response
   - Test POST with SSE response stream
   - Test GET listener for server-initiated messages
   - Test DELETE cleanup
   - Test 404 session expiration
   - Test 429 rate limiting

2. **tests/e2e_http_client.rs:**
   - Full McpClient over HTTP
   - Initialize, list_tools, call_tool
   - Server-initiated requests
   - Reconnection after network error
   - Session expiration and recovery

3. **tests/manager_http_integration.rs:**
   - McpServerManager with HTTP server
   - Auto-restart on disconnect
   - Health monitoring
   - Error recovery

**Depends on:** Phases 0-6 (all transport work complete)  
**Estimated size:** ~400 lines (transport + manager + integration tests)  
**Risk:** Medium (integration complexity, timing-dependent tests)

## Timeline: 6-Week Execution Plan

### Week 1: Async Foundation (Part A0-A1)
**Goal:** Unblock concurrency work

- **Mon-Tue:** A0.1 Async stdio I/O (blocking → non-blocking)
  - Replace `io::stdin()` with `tokio::io::stdin()`
  - Add timeout handling
  - Deliverable: Async read loop with timeout ✓
  
- **Wed:** A0.2 Graceful shutdown
  - Add `shutdown()` method
  - Update main to call cleanup
  - Deliverable: Clean exit without resource leaks ✓
  
- **Thu-Fri:** A1.1 Request multiplexing
  - Add `pending_requests` HashMap
  - Spawn tokio tasks for requests
  - Implement semaphore (max 50 concurrent)
  - Deliverable: Multiple requests processed in parallel ✓

**Checkpoint:** Server is now async, non-blocking, handles concurrent requests

### Week 2: Lifecycle Features (Part A1-A3)
**Goal:** Add resilience and feature detection

- **Mon-Tue:** A1.2 Dynamic request IDs
  - Add counter to McpClient
  - Update all request building
  - Deliverable: Unique request IDs ✓

- **Wed:** A2.1 Keep-alive pings
  - Spawn background keep-alive task
  - Send periodic pings when idle
  - Deliverable: Server detectable as alive ✓

- **Thu:** A2.2 Request timeouts
  - Add per-request timeout enforcement
  - Return error on timeout
  - Deliverable: Long requests don't block forever ✓

- **Fri:** A3.1 Session state
  - Store client capabilities
  - Track session initialization
  - Deliverable: Server knows what client supports ✓

**Checkpoint:** Server is resilient, can detect hangs, tracks session state

### Week 3-4: HTTP/SSE Transport (Part B)
**Goal:** Add HTTP streaming support

- **Week 3:**
  - B0: SSE parser (Phase 1 from original plan)
  - Tests: 14+ unit tests + integration tests
  - Deliverable: RFC 6202 compliant SSE parser ✓

- **Week 4:**
  - B1: HTTP transport SSE support (Phase 2)
  - Background event processor
  - Request routing by JSON-RPC id
  - Deliverable: POST with SSE responses works ✓

**Checkpoint:** HTTP transport can handle streaming responses

### Week 5: Optional HTTP Features (Part B2-B5)
**Goal:** Complete HTTP spec compliance

- **Mon-Tue:** Phase 3 (GET listener for server-initiated messages)
- **Wed:** Phase 4 (DELETE session termination)
- **Thu:** Phase 5 (Last-Event-ID resumability)
- **Fri:** Phase 6 (SseTransport cleanup)

**Checkpoint:** Full HTTP/SSE spec compliance

### Week 6: Integration & Hardening (Part C)
**Goal:** Production readiness

- **Mon-Tue:** C1 Client integration
  - Store capabilities properly (both paths)
  - Error mapping (HTTP codes → McpError)
  - Manager reconnection strategy
  
- **Wed:** C2 Integration tests
  - Mock HTTP server tests
  - Real-server compatibility tests
  - End-to-end tests through Manager
  
- **Thu-Fri:** Documentation & polish
  - Update README with HTTP support
  - Add examples
  - Error recovery documentation

**Final Checkpoint:** Production-ready MCP

---

## Updated Dependency Graph

### Overall Execution Flow

```
┌─────────────────────────────────────────────────────────────────┐
│ PART A: Stdio Lifecycle (Weeks 1-2, CRITICAL PATH)              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  A0.1 Async I/O ──┐                                             │
│                   ├─→ A1.1 Multiplexing ──┐                    │
│  A0.2 Shutdown ──┘                        │                    │
│                                           ├─→ A2.1 Keep-alive  │
│  A1.2 Req IDs ────────────────────────────┘                    │
│                                                                 │
│  A2.2 Timeouts ──┐                                              │
│                  ├─→ A3.1 Session State                        │
│                  │                                              │
│  RESULT: Async, concurrent, resilient stdio server             │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ PART B: HTTP/SSE (Weeks 3-5, IN PARALLEL WITH A)                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  B0 SSE Parser ──────→ B1 HTTP SSE Support                     │
│                           │                                     │
│                           ├─→ B2 GET Listener (optional)        │
│                           ├─→ B3 DELETE (optional)              │
│                           ├─→ B4 Resumability (optional)        │
│                           │                                     │
│  RESULT: HTTP transport with streaming & session mgmt          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ PART C: Integration (Week 6)                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  C1 Client integration (store capabilities properly)            │
│  C2 Integration tests (mock server, real server, E2E)           │
│  C3 Documentation & examples                                    │
│                                                                 │
│  RESULT: Production-ready MCP (stdio + HTTP)                    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Parallelization Strategy

**Critical path:** A0.1 → A1.1 → others (must be sequential)

**Can run in parallel:**
- A1.2 (request IDs) while doing A1.1
- A2.1, A2.2 (keep-alive, timeouts) while doing A1.1
- A3.1 (session state) while doing A2.x
- **Part B (HTTP)** completely parallel with Part A from day 1

**Recommended team split:**
- **Engineer 1:** A0 → A1 → A2 → A3 (stdio pipeline)
- **Engineer 2:** B0 → B1 → B2-5 (HTTP pipeline)
- **Engineer 3:** Integration tests as features complete

---

## Phase 0 — Critical Bug Fixes (Must do first)

```
Phase 0: Critical Bug Fixes (MUST DO FIRST)
  ├─ 0.1: Store server capabilities ✓ (blocks Phase 7)
  ├─ 0.2: Dynamic request IDs ✓ (blocks Phase 1, 2)
  ├─ 0.3: Double-initialize guard ✓ (blocks server.rs test)
  ├─ 0.4: Accept header on POST ✓ (blocks Phase 2)
  └─ 0.5: Header casing consistency ✓ (blocks integration tests)

Phase 1: SSE Parser (MUST DO SECOND)
  └─ Foundation for all streaming work
     Required by: Phase 2, 3, 5

         │
         ▼
Phase 2: HttpTransport SSE Support (POST Response Streaming)
  ├─ 2.1: Accept header (from Phase 0.4)
  ├─ 2.2: SSE-aware response handling
  ├─ 2.3: Background SSE event processor
  └─ 2.4: Request ID routing
     Blocks: Phase 3, 4, 7
     Required by: Phase 3, 4, 7

         │
         ├──────────────────┬──────────────────┐
         │                  │                  │
         ▼                  ▼                  ▼
Phase 3: GET Listener    Phase 4: DELETE     Phase 5: Last-Event-ID
  (Optional, High-Value) (Session Cleanup)   (Resumability, Best-Effort)
  
         ▲                  ▲                  ▲
         │                  │                  │
         └──────────────────┴──────────────────┘
                            │
                            ▼
Phase 6: SseTransport Removal (CLEANUP)
  └─ Delete sse_transport.rs
     Can happen: After Phase 2 (when HttpTransport fully functional)
     
         │
         ▼
Phase 7: Integration & Manager Updates (FINAL PASS)
  ├─ 7.1: Transport-agnostic client
  ├─ 7.2: Manager integration
  ├─ 7.3: Error mapping
  └─ 7.4: Integration tests
```

### Sequential Execution Plan

**Week 1: Foundation (Phase 0 + 1)**
- Phase 0.1-0.5: Bug fixes (~2-3 days)
- Phase 1: SSE parser + tests (~2-3 days)
- Checkpoint: Have working SSE parser, can verify with test data

**Week 2: HTTP Transport (Phase 2)**
- Phase 2.1-2.4: HttpTransport overhaul (~4-5 days)
- Includes: Response routing, background tasks, request correlation
- Checkpoint: POST with SSE responses works, background processors active

**Week 3: Optional Features (Phase 3, 4, 5 in parallel)**
- Phase 3: GET listener (~2-3 days)
- Phase 4: DELETE + 404 handling (~1-2 days)
- Phase 5: Last-Event-ID (~1-2 days)
- Can run in parallel (no interdependencies)
- Checkpoint: Full bidirectional messaging, session management, resumability

**Week 4: Cleanup + Integration (Phase 6 + 7)**
- Phase 6: SseTransport removal (~1 day)
- Phase 7: Integration tests + manager fixes (~3-4 days)
- Checkpoint: All tests passing, spec compliance verified

## Risk Mitigation

### Code Stability

- **Stdio transport untouched** — Phase 0-7 only modify HTTP transports. StdioTransport has zero regression risk.
- **Feature flags** — Consider `http-streaming` cargo feature to gate new code initially, allow opt-in testing
- **Incremental delivery** — Each phase independently testable before merging
- **Backward compatibility** — Old servers returning `application/json` still work; SSE path additive

### Protocol Compliance

- **Spec-driven testing** — Each phase has explicit checklist against RFC 6202 (SSE) and MCP spec §6 (HTTP)
- **Reference server** — Test against existing MCP HTTP servers (Anthropic's server, others) once Phase 2 complete
- **Edge case coverage** — Test chunked encoding, connection drops mid-event, 100+ events, etc.

### Data Loss Prevention

- **No event loss during normal operation** — Phase 2 background processor guarantees all events consumed
- **Resumability optional** — Phase 5 Last-Event-ID best-effort; loss acceptable if server doesn't support
- **Session tracking** — Session ID persisted; 404 triggers re-init (Phase 4)

### Performance

- **No unbounded buffering** — SSE events streamed, not accumulated
- **Request routing efficient** — HashMap<String, Sender> O(1) lookup by request ID
- **Backoff tuned** — Exponential backoff capped at 5 min (prevents storm retries)

## Testing Strategy

### Phase 0: Unit Tests (No Network)
- Verify capability persistence
- Verify request ID uniqueness
- Verify double-initialize guard

### Phase 1: Unit Tests (Parser Focused)
- RFC 6202 SSE parsing: all variants and edge cases
- Partial chunks, fragmented data
- BOM handling, line ending variants
- No actual network calls

### Phase 2: Integration Tests (Mock Server)
- Use [mockito](https://crates.io/crates/mockito) or [wiremock](https://crates.io/crates/wiremock-rs)
- POST → 200 application/json → verify response
- POST → 200 text/event-stream → verify SSE stream parsing
- POST → 200 with mixed JSON + SSE → verify routing
- Background task spawning, message inbox delivery
- Request correlation by ID (multiple in-flight requests)

### Phase 3-5: Integration Tests (Real/Mock SSE Server)
- GET listener connection and stream handling
- Server-initiated message routing to inbox
- GET listener reconnection with backoff
- Graceful degradation (405 on GET)
- Session expiration (404 from POST/GET)
- Last-Event-ID header inclusion on reconnect

### Phase 6: Smoke Tests
- SseTransport removal: verify no orphaned references
- All imports updated: grep for "SseTransport" in manager, client, examples
- No type errors, code compiles

### Phase 7: End-to-End Tests (Full System)
- McpServerManager with HTTP server
- Initialize, list_tools, call_tool over HTTP
- Auto-reconnect on simulated network error
- Health monitoring kicking in and recovering
- Tool cache refresh on reconnect
- Error messages clear and actionable

### Real-World Verification (After Phase 7)
1. Test with Anthropic's MCP reference HTTP server
2. Test with at least one third-party HTTP MCP server
3. Verify compatibility checklist below

## Spec Compliance Checklist

**Before declaring "done", verify against these spec sections:**

### RFC 6202 (Server-Sent Events)
- [ ] BOM (U+FEFF) handling
- [ ] Line ending variants (LF, CRLF, CR)
- [ ] Field parsing: single `:` separates name from value
- [ ] Multi-line fields (no `data:` on continuation line)
- [ ] Comment lines (`:` with optional text)
- [ ] Event dispatch on blank line
- [ ] id, event, retry fields parsed

### MCP Spec §4 (Message Protocol)
- [ ] Request/response IDs match JSON-RPC spec
- [ ] Server capabilities stored and accessible
- [ ] Initialize called once per session
- [ ] Tools, resources, prompts listed correctly

### MCP Spec §6 (HTTP Transport)
- [ ] **§6.1 Headers:**
  - [ ] `MCP-Protocol-Version` on every request
  - [ ] `MCP-Session-Id` after init
  - [ ] `Accept: application/json, text/event-stream` on POST
  - [ ] Header casing consistent

- [ ] **§6.2.1 POST Requests:**
  - [ ] Request body is JSON-RPC message
  - [ ] Response may be `application/json` or `text/event-stream`
  - [ ] Single request/response (non-streaming) works
  - [ ] Streaming response (SSE) works

- [ ] **§6.2.2 POST Streaming:**
  - [ ] First event contains response to original request
  - [ ] Subsequent events are notifications/server-initiated requests
  - [ ] Request/response routed by JSON-RPC id

- [ ] **§6.2.3 GET (Optional):**
  - [ ] GET to same endpoint returns `text/event-stream`
  - [ ] 405 graceful degradation (continue POST-only)
  - [ ] Server-initiated messages via GET
  - [ ] Session ID preserved
  - [ ] Reconnect with exponential backoff

- [ ] **§6.2.4 DELETE:**
  - [ ] DELETE includes Session-Id header
  - [ ] Server cleans up resources
  - [ ] 404 response means session expired

- [ ] **§6.3 Error Handling:**
  - [ ] 404 → SessionExpired error
  - [ ] 429 → RateLimited error (with Retry-After)
  - [ ] Connection errors → TransportError
  - [ ] Invalid JSON-RPC → ProtocolViolation

### Interoperability Testing
- [ ] Works with Anthropic's reference HTTP server
- [ ] Works with at least 2 third-party HTTP servers
- [ ] Degrades gracefully when server doesn't support advanced features
- [ ] Clear error messages when protocol violated

### Documentation
- [ ] README updated with HTTP transport support
- [ ] Examples added for HTTP server configuration
- [ ] Error recovery explained (reconnection, session expiration)
- [ ] Known limitations documented (optional features, graceful degradation)

---

## Verification

### Production Readiness Checklist

Before shipping, verify:

**Stdio Transport:**
- [ ] Async I/O with timeout (A0.1)
- [ ] Request multiplexing working (A1.1)
- [ ] Keep-alive pings functional (A2.1)
- [ ] Timeouts enforced (A2.2)
- [ ] Session state tracked (A3.1)
- [ ] Graceful shutdown (A0.2)

**HTTP/SSE Transport:**
- [ ] SSE parser RFC 6202 compliant (B0)
- [ ] POST with SSE responses (B1)
- [ ] GET listener optional but working (B2)
- [ ] DELETE cleanup (B3)
- [ ] Last-Event-ID resumability (B4)

**Client Integration:**
- [ ] Capabilities persisted correctly (C1)
- [ ] Request IDs unique (A1.2)
- [ ] Error mapping comprehensive (C1)
- [ ] Manager reconnection works (C1)

**Testing:**
- [ ] Unit: 40+ tests
- [ ] Integration: Mock server tests
- [ ] E2E: Real server compatibility
- [ ] Performance: <100ms request latency at 50 concurrent

**Documentation:**
- [ ] README updated with HTTP support
- [ ] Examples added (stdio + HTTP)
- [ ] Error recovery documented
- [ ] Known limitations listed

---

## Summary of Integrated Plan

### What This Addresses

| Category | Issues | Solutions |
|----------|--------|-----------|
| **Concurrency** | Single-threaded, blocks on I/O | Async I/O + task spawning for multiplexing |
| **Resilience** | Dies on error, no recovery | Explicit shutdown, timeouts, session state |
| **HTTP/SSE** | Missing parser, no streaming | SSE parser + request routing + background tasks |
| **Spec Compliance** | Capabilities not persisted | Store in server, return in initialize |
| **Observability** | Can't detect hangs | Keep-alive pings + request timeouts |
| **Efficiency** | Synchronous blocking | Async non-blocking I/O throughout |

### Effort Breakdown

| Phase | Week | Effort | Impact |
|-------|------|--------|--------|
| A0 (Async I/O) | 1 | 2-3 days | Unblocks all concurrency |
| A1 (Multiplexing) | 1 | 3-4 days | Enables parallel requests |
| A2 (Keep-alive) | 2 | 1-2 days | Detectability |
| A3 (Session state) | 2 | 1-2 days | Feature detection |
| B0 (SSE parser) | 3 | 3-4 days | Foundation for HTTP |
| B1 (HTTP SSE) | 4 | 3-4 days | Streaming support |
| B2-5 (Optional HTTP) | 5 | 3-4 days | Full HTTP spec |
| C (Integration) | 6 | 3-4 days | Production polish |

**Total:** 6 weeks, 2 engineers working in parallel

**Critical path:** A0.1 → A1.1 → integration (8 days minimum)

### New Discoveries from Lifecycle Analysis

This integration revealed **5 additional issues** not in original HTTP roadmap:

1. ✅ Blocking I/O prevents keep-alive, timeouts, background tasks
2. ✅ Single-threaded architecture prevents concurrent requests
3. ✅ No session state tracking (client capabilities unknown)
4. ✅ Missing request ID counter (uniqueness not guaranteed)
5. ✅ No graceful shutdown (resource leaks)

**All addressed in this unified plan.**

---

## Original Plan Summary (Updated)

This revision discovered and documents **6 CRITICAL issues** that block HTTP/SSE adoption:

| Issue | Original Plan | This Revision | Impact |
|-------|---------------|---------------|--------|
| Server capabilities | ✓ Mentioned in Phase 0 | **Elevated to CRITICAL** | Client can't detect server features |
| No SSE parser | ✓ Mentioned in Phase 1 | **Clarified as hard blocker** | All events after first lost |
| No request routing | ❌ Not addressed | **Added Section 2.4** | Can't correlate SSE events to requests |
| No background listeners | ✓ Mentioned in Phase 2.3 | **Detailed task spawning** | Server-initiated messages lost |
| Accept header missing | ❌ Not addressed | **Elevated to Phase 0.4 CRITICAL** | Server doesn't know client capabilities |
| Session lifecycle incomplete | ✓ Mentioned in Phase 4 | **Detailed 404 handling** | Session expiration causes hangs |

**Key additions:**
1. **Phase 0 expanded from 3 to 5 critical fixes** — Server capabilities, request IDs, initialize guard, Accept header, header casing
2. **SSE parser made foundation** — Can't skip; required by all subsequent phases
3. **Request routing explicitly designed** — HashMap<id, Sender> pattern for SSE events
4. **Error handling mapped** — HTTP status codes → McpError variants → Manager recovery strategy
5. **Integration tests scoped** — Unit, mock-server, real-server, end-to-end levels
6. **Spec compliance checklist added** — RFC 6202 + MCP §6 verification

**Real-world impact of this plan:**
- **Current state:** HTTP transport broken for SSE (99% of servers respond with SSE)
- **After Phase 0-2:** Basic HTTP/SSE works, same reliability as StdioTransport
- **After Phase 3-5:** Full bidirectional messaging, resilient to network interruptions
- **After Phase 6-7:** Production-ready, transparent to client code, auto-healing

**Timeline:** 4 weeks (1 per phase group) with parallel work possible in weeks 3-4.

---

## Quick Reference: What Changed

**Phase 0 (Critical Fixes):**
- **+0.4 Accept header** (was missing, server doesn't know what client supports)
- **+0.5 Header casing** (consistency issue, spec compliance)
- **Clarified severity** (not just cleanup, enables phases 1-7)

**Phase 1 (SSE Parser):**
- **Expanded test suite** (14 unit tests, real SSE server tests)
- **Detailed state machine** (explicit field parsing logic)
- **BOM handling documented** (per RFC 6202)

**Phase 2 (HttpTransport):**
- **+2.3 Background task spawning** (explicit async pattern)
- **+2.4 Request routing** (critical missing piece)
- **Detailed error mapping** (HTTP → McpError)

**Phase 3-7:**
- **Expanded phase descriptions** with spec references and compliance checklists
- **Error handling throughout** (404, 429, timeouts)
- **Integration tests detailed** (mock server, real server, end-to-end)

**Phase 6:**
- **Clear removal path** (SseTransport is redundant after Phase 2)
- **Migration guidance** (what to do if external code uses it)

**Phase 7:**
- **Integration checklist** (transport-agnostic client)
- **Manager recovery strategy** (reconnection, session expiration)
- **Error propagation** (clear semantics for error handling)
