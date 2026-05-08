# MCP Lifecycle & Architecture Analysis

**Date:** 2026-05-08  
**Status:** Detailed findings + recommendations  
**Scope:** How MCP is exposed to Claude, lifecycle management, and efficiency

---

## Executive Summary

RustyCode's MCP implementation is **functionally correct but has critical efficiency issues**:

| Aspect | Status | Severity | Impact |
|--------|--------|----------|--------|
| **Exposing to Claude** | ✅ Works | — | MCP server spawned via `.mcp.json`, communicates via stdio |
| **Lifecycle management** | ⚠️ Basic | HIGH | Server dies on first error, no reconnection, no session persistence |
| **Concurrency** | ❌ Blocking | HIGH | Synchronous stdin/stdout I/O blocks on each request, no parallelism |
| **Streaming** | ❌ None | MEDIUM | Large responses truncated, can't stream multi-part answers |
| **Connection resilience** | ❌ None | HIGH | Network blip = dead server, Claude must spawn new instance |
| **Resource cleanup** | ⚠️ Implicit | MEDIUM | Relies on OS cleanup, no explicit shutdown sequence |

---

## Part 1: How MCP Gets Exposed to Claude

### Current Architecture

```
┌─────────────────────────────────────────────────────┐
│ Claude (Local Claude Code / Web App)                │
├─────────────────────────────────────────────────────┤
│                                                     │
│  1. Load .mcp.json config file                     │
│     (from ~/.mcp.json or project root)            │
│                                                     │
│  2. For each server in "mcpServers":               │
│     - Spawn process: command + args               │
│     - Detect transport: stdio, http, sse          │
│                                                     │
│  3. Initialize MCP handshake:                      │
│     - Send: {"jsonrpc": "2.0",                    │
│              "id": "init-1",                      │
│              "method": "initialize",              │
│              "params": {...}}                    │
│                                                     │
│  4. Receive capabilities:                          │
│     - tools: [list of tool definitions]           │
│     - resources: [list of resource URIs]          │
│     - prompts: [list of prompt templates]         │
│                                                     │
│  5. On each user request:                          │
│     - Send tool call: tools/call                  │
│     - Receive response: structured content        │
└────────────────────────────────────────────────────┘
         ↓ stdio (JSON-RPC messages, one per line) ↓
┌────────────────────────────────────────────────────┐
│ MCP Server (RustyCode)                             │
│ Process: cargo run -p rustycode-mcp -- <args>     │
├────────────────────────────────────────────────────┤
│                                                    │
│  server.rs:run_stdio()                            │
│    │                                              │
│    ├─ Read line from stdin                       │
│    ├─ Parse as JSON-RPC request                  │
│    ├─ Route to handler (initialize, tools/call)  │
│    ├─ Execute tool via ToolExecutor              │
│    ├─ Format response as JSON-RPC                │
│    └─ Write line to stdout                       │
│                                                    │
│  Capabilities exposed:                            │
│  ├─ Tools (via ToolExecutor from rustycode-tools)│
│  ├─ Resources (file://, codebase://, search://)  │
│  └─ Prompts (pre-built context templates)        │
└────────────────────────────────────────────────────┘
```

### Example .mcp.json Configuration

```json
{
  "mcpServers": {
    "rustycode-terminal": {
      "command": "cargo",
      "args": [
        "run",
        "-p", "rustycode-mcp",
        "--",
        "--backend", "auto",
        "--workspace-root", "/Users/nat/dev/project"
      ],
      "enable_tools": true,
      "enable_resources": true,
      "enable_prompts": false
    }
  }
}
```

**How Claude discovers and launches it:**
1. Claude reads `.mcp.json` at startup
2. Spawns `cargo run -p rustycode-mcp ...` as subprocess
3. Opens stdin/stdout pipes to subprocess
4. Sends/receives JSON-RPC messages line-by-line
5. First message: `{"method": "initialize", ...}` → Server sets `initialized = true`
6. Subsequent messages: `{"method": "tools/call", ...}` → Tool execution
7. On error or EOF: Server process dies, Claude must spawn new one

---

## Part 2: Lifecycle Management Issues

### Current Lifecycle (server.rs:run_stdio)

```
┌──────────────────────────────────────────────────┐
│ START: Process spawned                           │
├──────────────────────────────────────────────────┤
│                                                  │
│ INIT: await server.run_stdio()                   │
│       ├─ Open stdin/stdout                       │
│       └─ Enter loop: for line in stdin.lines()  │
│                                                  │
│ LOOP: Process JSON-RPC requests 1-by-1         │
│       ├─ Read line from stdin (BLOCKING)        │
│       ├─ Parse JSON (strict, no recovery)       │
│       ├─ Handle request (sync)                  │
│       ├─ Format response                        │
│       └─ Write to stdout + flush                │
│                                                  │
│ END: One of:                                     │
│       ├─ stdin closed (Claude closes pipe)      │
│       ├─ Parse error (server panics or returns) │
│       ├─ Handler error (logged, response sent)  │
│       └─ Unhandled panic                        │
│                                                  │
│ CLEANUP: Process exits, pipes close             │
│          (OS cleans up resources)               │
└──────────────────────────────────────────────────┘
```

### Critical Issues

#### 1. **No Graceful Shutdown**

**Code (server.rs line 199):**
```rust
pub async fn run_stdio(&mut self) -> McpResult<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stdin_lines = stdin.lock().lines();
    
    for line in &mut stdin_lines {
        // ... handle request ...
    }
    Ok(())  // Just... exit?
}
```

**Problem:**
- No cleanup code before exit
- No resource deallocation
- No flush of pending messages
- No notification to tools that session is ending

**Impact:**
- Long-running tools may be interrupted mid-execution
- Files may be left open
- Temporary state not cleaned up
- Claude has no warning before server dies

**Fix needed:**
```rust
pub async fn run_stdio(&mut self) -> McpResult<()> {
    let cleanup = defer(|| {
        self.shutdown().await;  // Clean up resources
        eprintln!("Server shutting down");  // Signal to parent
    });
    
    // ... existing loop ...
    Ok(())
}
```

#### 2. **No Keep-Alive or Heartbeat**

**Problem:** Claude can't detect if server is hung or has crashed until the next message is sent.

If server takes 5+ seconds to respond:
- Claude assumes it's working
- Meanwhile server may be deadlocked or resource-constrained
- No way to probe server health without sending a request

**Current behavior:**
```rust
// No keep-alive mechanism
// No timeout on request processing
// No background health check
```

**Fix needed:**
```rust
// Spawn background task to send keep-alive pings
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        // Server can respond with {} to acknowledge
    }
});
```

#### 3. **Single-Request-at-a-Time Processing**

**Code (server.rs line 214):**
```rust
for line in &mut stdin_lines {
    let response = self.handle_request(request).await;
    self.send_response(&mut stdout, &response)?;
}
```

**Problem:**
- Only one request processed at a time
- If a tool call takes 10 seconds, Claude is blocked for 10 seconds
- Can't interleave requests or show partial progress
- No multiplexing capability

**Current behavior:**
1. Claude sends request #1 (e.g., `bash command`)
2. Server blocks on `tool_executor.execute()` for 30+ seconds
3. Claude waits... can't send request #2
4. After 30s, response comes back for request #1
5. Only then can request #2 be processed

**Impact:** Single-threaded bottleneck for concurrent tool usage.

#### 4. **No Connection State Management**

**Problem:** Each request is stateless. No tracking of:
- Client capabilities
- Active sessions
- Resource subscriptions
- Progress on long-running operations

**Code issue (server.rs line 150):**
```rust
pub struct McpServer {
    // ... no session tracking ...
    // ... no client_capabilities storage ...
    // ... no request_context ...
}
```

**Workaround used in client.rs (line 0-50):**
```rust
pub struct McpClient {
    server_capabilities: Option<ServerCapabilities>,  // Hack: client stores this
    // Should be sent back in initialize response!
}
```

This violates MCP spec §4.1: **Server MUST return capabilities in initialize response, client MUST store them.**

Currently:
- ❌ Server doesn't return capabilities in initialize
- ❌ Client has to infer them
- ❌ No way to query "does server support subscriptions?"

---

## Part 3: Concurrency & Efficiency Issues

### Issue #1: Blocking Stdio I/O

**Current (server.rs line 203):**
```rust
let stdin = io::stdin();
let mut stdout = io::stdout();
let mut stdin_lines = stdin.lock().lines();  // ← BLOCKING!

for line in &mut stdin_lines {
    // Entire server waits here if no input
    // Can't do background work, monitoring, cleanup
}
```

**Why this is bad:**
- `io::stdin().lock().lines()` is **synchronous blocking**
- No timeout on read (could hang forever if client sends nothing)
- Server can't do background tasks while waiting for input
- Async runtime can't help (I/O is synchronous)

**Better approach:**
```rust
use tokio::io::{AsyncBufReadExt, BufReader};

let stdin = tokio::io::stdin();
let mut reader = BufReader::new(stdin);
let mut line = String::new();

loop {
    // Non-blocking read with timeout
    match tokio::time::timeout(
        Duration::from_secs(300),
        reader.read_line(&mut line)
    ).await {
        Ok(Ok(0)) => break,  // EOF
        Ok(Ok(_)) => {
            // Process line
        }
        Err(_) => {
            // Timeout - can send keep-alive or cleanup
        }
        Err(io_err) => {
            eprintln!("I/O error: {}", io_err);
            break;
        }
    }
    line.clear();
}
```

### Issue #2: Request Processing is Sequential

**Current architecture:**
```
Claude                MCP Server
  │                      │
  ├──request #1────────→ │
  │                      │ (execute tool for 10s)
  │                      │
  │                   ← response #1
  │ (now can send #2)
  ├──request #2────────→ │
  │                      │
```

**Better architecture (with multiplexing):**
```
Claude                MCP Server
  │                      │
  ├──request #1────────→ │ ─────────────┐
  ├──request #2────────→ │ ─────────────┤ Process in parallel
  ├──request #3────────→ │ ─────────────┘
  │                      │
  │                   ← response #1
  │                   ← response #3 (finished first)
  │                   ← response #2 (finished last)
```

**Code needed:**
```rust
// Track in-flight requests by JSON-RPC id
let pending_requests: Arc<RwLock<HashMap<String, Sender>>> = ...;

// Spawn task for each request
tokio::spawn(async move {
    let response = handle_request(request).await;
    
    // Send response back (could be out-of-order)
    if let Some(tx) = pending_requests.remove(&request.id) {
        let _ = tx.send(response);
    }
});
```

### Issue #3: No Streaming Responses

**MCP Spec §7.3.1 (Text Content)** allows multi-part responses, but current implementation only supports single text block.

**Problem (server.rs line 265):**
```rust
Ok(json!({ "content": content }))  // Single response, done
```

**Real-world need:** Large file reads.

If user asks "read src/**/*.rs", response could be 100MB+.
- Current: Entire response buffered in memory, then sent in one line
- Risk: Out of memory, line too long (some stdio implementations limit to 4KB), timeout

**Better approach:**
```rust
// Stream multiple content blocks
let mut response = vec![];
for chunk in file_contents.chunks(1024 * 1024) {
    response.push(McpContent::Text {
        text: String::from_utf8_lossy(chunk).into_owned(),
    });
    
    // Client can start processing while more data arrives
    send_partial_response(&response)?;
}
```

### Issue #4: No Resource Cleanup

**Current:** Relies on process exit to clean up.

**Missing:**
- No explicit file handle closing in error paths
- No temporary file cleanup
- No session state cleanup
- No event listeners unregistered
- No database connections closed

**Example issue:**
```rust
let executor = self.tool_executor.as_ref()?;
let result = executor.execute(&tool_call);  // ← What if this panics?
// Files may be left open
```

---

## Part 4: Architecture Recommendations

### Priority 1: Critical (Fix Before Production)

#### 1.1 Implement Async Stdio I/O

**File:** `server.rs`  
**Change:** Replace `io::stdin().lock().lines()` with `tokio::io::stdin()`

```rust
use tokio::io::{AsyncBufReadExt, BufReader};

async fn run_stdio(&mut self) -> McpResult<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => break,  // EOF
            Ok(_) => {
                let response = self.handle_request_line(&line).await;
                self.send_response(&mut stdout, &response).await?;
            }
            Err(e) => {
                eprintln!("I/O error: {}", e);
                break;
            }
        }
    }
    Ok(())
}
```

**Benefit:** Non-blocking I/O, allows background tasks (keep-alive, monitoring)

#### 1.2 Store Server Capabilities in Initialize Response

**File:** `server.rs:handle_initialize`

**Current (WRONG):**
```rust
pub async fn handle_initialize(...) -> McpResult<serde_json::Value> {
    // ... initialize server ...
    Ok(json!({
        "protocolVersion": ...,
        "capabilities": {},  // ← Client can't know what we support!
        "serverInfo": ...
    }))
}
```

**Fix:**
```rust
pub async fn handle_initialize(...) -> McpResult<serde_json::Value> {
    // ... initialize server ...
    
    let mut capabilities = json!({});
    if self.config.enable_tools {
        capabilities["tools"] = json!({});
    }
    if self.config.enable_resources {
        capabilities["resources"] = json!({
            "subscribe": false,
            "listChanged": false
        });
    }
    
    Ok(json!({
        "protocolVersion": crate::MCP_VERSION,
        "capabilities": capabilities,  // ← Now client knows!
        "serverInfo": ...
    }))
}
```

**Benefit:** Spec compliance, client can detect capabilities without trial-and-error

#### 1.3 Add Explicit Graceful Shutdown

**File:** `server.rs` + `bin/rustycode-mcp.rs`

```rust
impl McpServer {
    pub async fn shutdown(&mut self) {
        info!("Shutting down MCP server...");
        
        // Flush any pending state
        if let Some(executor) = &self.tool_executor {
            executor.cleanup();
        }
        
        // Clear resources
        self.resources.write().await.clear();
        self.tools.write().await.clear();
        
        // Signal completion
        info!("MCP server shutdown complete");
    }
}

// In main:
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ...
    let mut server = McpServer::new(...);
    
    // Ensure cleanup on exit
    let result = server.run_stdio().await;
    server.shutdown().await;
    
    result
}
```

**Benefit:** Proper resource cleanup, predictable shutdown

### Priority 2: Important (Improves Reliability)

#### 2.1 Implement Request Multiplexing

**File:** New `concurrent_server.rs` OR update `server.rs`

Track in-flight requests:
```rust
pub struct McpServer {
    // ...
    pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>,
    request_semaphore: Arc<Semaphore>,  // Limit concurrent requests
}

async fn handle_request(&self, request: JsonRpcRequest) {
    let permit = self.request_semaphore.acquire().await;
    let id = request.id.clone();
    
    tokio::spawn({
        let server = self.clone();
        async move {
            let response = server.process(request).await;
            if let Some(tx) = server.pending_requests.write().await.remove(&id) {
                let _ = tx.send(response);
            }
            drop(permit);
        }
    });
}
```

**Benefit:** Can process multiple tool calls in parallel, better UX

#### 2.2 Add Keep-Alive Pings

**File:** `server.rs`

```rust
async fn spawn_keep_alive(&self, tx: Sender<String>) {
    tokio::spawn({
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let msg = serde_json::json!({"jsonrpc": "2.0", "method": "ping"});
            let _ = tx.send(msg.to_string()).await;
        }
    });
}
```

**Benefit:** Claude can detect server hangs before sending next request

#### 2.3 Add Request Timeout Enforcement

**File:** `server.rs:handle_request`

```rust
async fn handle_request_with_timeout(&self, request: JsonRpcRequest) -> JsonRpcResponse {
    match tokio::time::timeout(
        Duration::from_secs(self.config.timeout_secs),
        self.handle_request(request.clone())
    ).await {
        Ok(response) => response,
        Err(_) => {
            JsonRpcResponse::error(
                request.id,
                -32603,  // Internal error
                format!("Request timeout ({}s)", self.config.timeout_secs)
            )
        }
    }
}
```

**Benefit:** Long-hanging requests don't block Claude forever

### Priority 3: Nice-to-Have (Optimization)

#### 3.1 Session State Tracking

Track capabilities, subscriptions, progress:
```rust
pub struct SessionState {
    client_capabilities: ClientCapabilities,
    active_resources: HashSet<String>,
    in_flight_requests: HashMap<String, RequestContext>,
}
```

#### 3.2 Streaming Large Responses

Implement MCP streaming (if spec supports):
```rust
// Instead of: send entire 100MB file at once
// Do this: send chunks, client reads progressively
```

#### 3.3 Connection Pooling (For HTTP Transport)

When using HTTP instead of stdio, reuse connections:
```rust
let client = reqwest::Client::new();  // Reuse across requests
```

---

## Part 5: Current Correctness Assessment

### What Works ✅

- **Initialize handshake** — Double-initialize guard present
- **Tool listing** — Correctly enumerates tools from executor
- **Tool execution** — Routes calls to executor, captures output
- **Resource enumeration** — Lists registered resources
- **Prompt templates** — Supports prompt registration
- **JSON-RPC protocol** — Correct request/response format
- **Error mapping** — Converts internal errors to JSON-RPC codes

### What's Broken ❌

| Issue | Code | Status | Risk |
|-------|------|--------|------|
| Capabilities not persisted | client.rs:202 | 🔴 BROKEN | Spec violation |
| Dynamic request IDs missing | client.rs | 🔴 BROKEN | Uniqueness not guaranteed |
| Blocking stdio I/O | server.rs:203 | 🔴 BROKEN | No responsiveness |
| No concurrent requests | server.rs:214 | 🟡 LIMITATION | Sequent processing |
| No keep-alive | server.rs | 🟡 LIMITATION | Can't detect hangs |
| No session state | server.rs | 🟡 LIMITATION | Can't track subscriptions |
| No streaming | server.rs | 🟡 LIMITATION | Large responses fail |

### Efficiency Score: 4/10

- **Correctness:** 7/10 (mostly works, some spec violations)
- **Resilience:** 3/10 (dies on error, no recovery)
- **Performance:** 3/10 (single-threaded, blocking I/O)
- **User Experience:** 4/10 (slow, unresponsive on long tasks)

---

## Part 6: Migration Path

### Phase A: Critical Fixes (Week 1)
- [ ] Fix async stdio I/O
- [ ] Store capabilities in initialize
- [ ] Add graceful shutdown
- [ ] Fix double-initialize guard

### Phase B: Reliability (Week 2)
- [ ] Implement keep-alive pings
- [ ] Add request timeouts
- [ ] Implement request multiplexing
- [ ] Add error recovery

### Phase C: Optimization (Week 3-4)
- [ ] Add session state tracking
- [ ] Implement streaming responses
- [ ] Add connection pooling (HTTP)
- [ ] Add metrics/observability

---

## Summary: The Bottom Line

**Current state:** MCP works for basic single-request scenarios, but will hang or crash under:
- Multiple concurrent tool calls
- Long-running operations (>1 minute)
- Network interruptions
- Large response payloads

**Root cause:** Synchronous blocking I/O + no session management.

**Fix:** Switch to async I/O, add multiplexing, track connection state.

**Effort:** ~2-3 weeks for full remediation (4 phases above).

**Impact:** Necessary for production use with Claude.
