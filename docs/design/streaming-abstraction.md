# Phase B: Streaming Abstraction Design

> Status: DRAFT | Date: 2026-04-27 | Follows: Phase A (OrchestrationPipeline wraps AgentSession)

## Problem

Three issues with the current `AgentEvents` trait:

1. **Sync-in-async**: `AgentEvents` is a sync trait (`&mut self`) called from async `run_loop`. Interactive methods (`on_approval_needed`) need user input, forcing `block_in_place` hacks in orchestration's `BridgeEvents`.

2. **Buffered, not streamed**: `run_loop` drains the **entire** SSE stream into `StreamState`, then fires `on_text_delta` with the complete text. The UI sees nothing until the full LLM response arrives.

3. **No unified event type**: 6+ separate callback methods instead of one event stream. Adding new event types requires trait changes that ripple to all 8 implementations.

## What We're Not Building

- **Event sourcing / replay system** — YAGNI. OpenCode has this for multi-device sync; we don't need it.
- **SyncEvent / BusEvent dual system** — YAGNI. We have one bus, one event flow.
- **Interleaved tool execution** — The LLM protocol sends all tool calls in one response; we execute after drain. True interleaving would require a different protocol.
- **Cross-instance sync** — Single-machine only for now.

## Solution: StreamEvent enum + async AgentEvents

Replace the sync callback trait with an async trait that receives typed `StreamEvent` enum values. Restructure `run_loop` to fire events **during** SSE drain instead of after.

### New Types

#### StreamEvent (in `rustycode-protocol`)

```rust
// crates/rustycode-protocol/src/stream_event.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum StreamEvent {
    // --- Streaming content (fired during SSE drain) ---
    /// LLM text content arriving incrementally.
    TextDelta { content: String },
    /// LLM thinking/reasoning arriving incrementally.
    ThinkingDelta { content: String },
    /// A tool call block has started in the LLM output.
    ToolCallStarted { id: String, name: String },
    /// Tool input JSON arriving incrementally.
    ToolInputDelta { id: String, chunk: String },

    // --- Tool execution (fired during tool execution phase) ---
    /// Tool execution has begun.
    ToolExecStarted { id: String, name: String },
    /// Tool execution finished (success or error).
    ToolExecCompleted { id: String, name: String, output: String, is_error: bool },

    // --- Turn lifecycle ---
    /// A new agent turn has started.
    TurnStarted { turn: usize },

    // --- Token usage ---
    /// Token usage report from the LLM.
    TokenUsage { input: u64, output: u64 },

    // --- Terminal ---
    /// Session completed normally.
    Done,
}
```

**Why these variants and not more:**
- No `TurnEnded` — the next `TurnStarted` or `Done` implies it. No consumer needs explicit turn-end.
- No `Error` variant — errors are handled by `Result` in `run()` return. Consumers that need error notification can wrap in their own error type.
- No `FileSnapshot` — file tracking is a storage concern, not a streaming concern.
- No `Started`/`Ended` lifecycle triples — we don't need them yet. The LLM protocol already provides `ContentBlockStart`/`ContentBlockDelta`/`MessageDelta` boundaries internally.

#### ApprovalDecision (move from agent to protocol)

```rust
// Currently in rustycode-agent/src/session.rs:86-93
// Move to rustycode-protocol/src/stream_event.rs

pub enum ApprovalDecision {
    Approve,
    Reject(String),
    AutoApproved,
}
```

**Why move**: `ApprovalDecision` is used by orchestration, core, bench, and TUI. It belongs in the shared types crate, not locked inside agent.

#### Async AgentEvents (in `rustycode-agent`)

```rust
// crates/rustycode-agent/src/session.rs (replaces current trait)

#[async_trait::async_trait]
pub trait AgentEvents: Send {
    /// Receive a streaming event. Called for every event during the session.
    async fn on_event(&mut self, event: StreamEvent);

    /// Interactive: request approval for a tool call.
    /// Default: auto-approve.
    async fn on_approval_needed(
        &mut self,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> ApprovalDecision {
        ApprovalDecision::AutoApproved
    }

    /// Interactive: ask the user a question.
    /// Default: no answer (returns None).
    async fn on_question(
        &mut self,
        _question: &str,
        _options: &[String],
    ) -> Option<String> {
        let _ = _options;
        None
    }

    /// Session completed. Fires after all streaming is done.
    /// Default: no-op.
    async fn on_done(&mut self, _result: &AgentResult) {}
}
```

**Why `on_event` instead of individual methods:**
- Adding new event types = adding enum variants, no trait change.
- Consumers pattern-match what they care about, ignore the rest.
- Single method = single await point = simpler async flow.
- Serializable — can be persisted, sent over channels, logged.

**Why keep `on_approval_needed` and `on_question` separate:**
- They're request-response, not fire-and-forget.
- They block the agent loop until the consumer responds.
- Separate methods make the interactive contract explicit.

**Why keep `on_done`:**
- Provides the final `AgentResult` with token counts, stopped reason, etc.
- `StreamEvent::Done` doesn't carry this data (it's a simple signal).

---

## Phase Breakdown

### B1: Add StreamEvent + ApprovalDecision to protocol

**Scope**: 1 new file, 1 edit to lib.rs. No breakage.

| File | Action |
|------|--------|
| `crates/rustycode-protocol/src/stream_event.rs` | **CREATE** — `StreamEvent` enum + `ApprovalDecision` enum |
| `crates/rustycode-protocol/src/lib.rs` | **EDIT** — add `pub mod stream_event;` + re-exports |

**Verification**: `cargo check -p rustycode-protocol` passes.

---

### B2: Replace AgentEvents with async trait + true streaming

**Scope**: Major refactor of `rustycode-agent/src/session.rs`. Breaks all consumers (fixed in B3).

| File | Action |
|------|--------|
| `crates/rustycode-agent/src/session.rs` | **REFACTOR** — see below |
| `crates/rustycode-agent/src/lib.rs` | **EDIT** — update re-exports, remove `ApprovalDecision` |

**Detailed changes to `session.rs`:**

#### 2a. Remove old `ApprovalDecision` (now imported from protocol)

```rust
// DELETE lines 86-93 (ApprovalDecision enum)
// ADD: use rustycode_protocol::stream_event::{StreamEvent, ApprovalDecision};
```

#### 2b. Replace `AgentEvents` trait (lines 99-123)

Replace the sync trait with the async version shown above.

#### 2c. Restructure `run_loop` SSE drain (lines 286-301)

**Current behavior**: Buffer entire stream, then fire events.
**New behavior**: Fire events as each SSE chunk arrives.

```rust
// CURRENT (lines 286-301):
loop {
    let sse = match tokio::time::timeout(chunk_timeout, stream.next()).await {
        Ok(Some(Ok(ev))) => ev,
        Ok(Some(Err(e))) => { break; }
        Ok(None) => break,
        Err(_) => break,
    };
    process_sse_event(sse, &mut state);
}
// THEN: events.on_text_delta(&state.assistant_text);
```

```rust
// NEW:
loop {
    let sse = match tokio::time::timeout(chunk_timeout, stream.next()).await {
        Ok(Some(Ok(ev))) => ev,
        Ok(Some(Err(e))) => {
            tracing::warn!("Mid-stream error: {e}. Ending turn early.");
            break;
        }
        Ok(None) => break,
        Err(_) => {
            tracing::warn!("Stream chunk timeout. Ending turn early.");
            break;
        }
    };

    // Fire streaming events IMMEDIATELY as SSE chunks arrive
    match &sse {
        SSEEvent::ContentBlockDelta { delta, .. } => match delta {
            ContentDelta::Text { text } => {
                state.assistant_text.push_str(text);
                events.on_event(StreamEvent::TextDelta { content: text.clone() }).await;
            }
            ContentDelta::PartialJson { partial_json } => {
                if let Some(last) = state.tools.last() {
                    events.on_event(StreamEvent::ToolInputDelta {
                        id: last.id.clone(),
                        chunk: partial_json.clone(),
                    }).await;
                }
            }
            ContentDelta::Thinking { thinking } => {
                state.thinking_text.push_str(thinking);
                events.on_event(StreamEvent::ThinkingDelta { content: thinking.clone() }).await;
            }
            _ => {}
        },
        SSEEvent::ContentBlockStart { content_block, .. } => {
            if let ContentBlockType::ToolUse { id, name, .. } = content_block {
                state.tools.push(PendingTool {
                    id: id.clone(),
                    name: name.clone(),
                    input_json: String::new(),
                });
                events.on_event(StreamEvent::ToolCallStarted {
                    id: id.clone(),
                    name: name.clone(),
                }).await;
            }
        },
        SSEEvent::MessageDelta { stop_reason, usage } => {
            state.stop_reason = stop_reason.clone();
            if let Some(u) = usage {
                state.total_output_tokens += u64::from(u.output_tokens);
                state.total_input_tokens += u64::from(u.input_tokens);
                state.total_cache_read_tokens += u64::from(u.cache_read_input_tokens);
                state.total_cache_creation_tokens += u64::from(u.cache_creation_input_tokens);
                events.on_event(StreamEvent::TokenUsage {
                    input: u.input_tokens as u64,
                    output: u.output_tokens as u64,
                }).await;
            }
        },
        SSEEvent::Text { text } => {
            state.assistant_text.push_str(text);
            events.on_event(StreamEvent::TextDelta { content: text.clone() }).await;
        },
        SSEEvent::ThinkingDelta { thinking } => {
            state.thinking_text.push_str(thinking);
            events.on_event(StreamEvent::ThinkingDelta { content: thinking.clone() }).await;
        },
        _ => {}
    }
}
```

#### 2d. Remove post-drain text/thinking events (lines 303-309)

```rust
// DELETE these lines — events are now fired during drain:
if !state.assistant_text.is_empty() {
    events.on_text_delta(&state.assistant_text);
}
if !state.thinking_text.is_empty() {
    events.on_thinking_delta(&state.thinking_text);
}
```

#### 2e. Update tool execution to fire events (lines 354-388)

```rust
// Replace direct callback calls with StreamEvent emissions:
// Before:
events.on_tool_call(&tool.id, &tool.name, &input);
events.on_tool_result(&tool.id, &tool.name, &truncated, error_flag);

// After:
events.on_event(StreamEvent::ToolExecStarted {
    id: tool.id.clone(),
    name: tool.name.clone(),
}).await;
events.on_event(StreamEvent::ToolExecCompleted {
    id: tool.id.clone(),
    name: tool.name.clone(),
    output: truncated.clone(),
    is_error: error_flag,
}).await;
```

#### 2f. Update turn start (line 246)

```rust
// Before:
events.on_turn_start(turn);

// After:
events.on_event(StreamEvent::TurnStarted { turn }).await;
```

#### 2g. Update completion (line 422)

```rust
// Before:
events.on_done(&result);

// After:
events.on_event(StreamEvent::Done).await;
events.on_done(&result).await;
```

#### 2h. Delete `process_sse_event` function (lines 572-618)

This function accumulated state without firing events. Its logic is now inlined into the SSE drain loop (2c above). Delete the entire function.

#### 2i. Update Cargo.toml dependencies

```toml
# Add to crates/rustycode-agent/Cargo.toml:
rustycode-protocol = { path = "../rustycode-protocol" }  # likely already present
```

Check that `rustycode-protocol` is already a dependency (it is — confirmed in Cargo.toml).

#### 2j. Update internal tests (lines 624-716)

Update `MockEvents`, `DefaultEvents`, and test assertions to use async trait + `StreamEvent`.

---

### B3: Update all 8 AgentEvents implementations

**Scope**: 5 files across 5 crates. Each impl changes from sync callbacks to async `on_event`.

| # | File | Struct | Key Changes |
|---|------|--------|-------------|
| 1 | `crates/rustycode-orchestration/src/agent_executor.rs` | `BusAgentEvents` | `on_event` maps to `OrchestrationEvent::StreamDelta` |
| 2 | `crates/rustycode-orchestration/src/agent_executor.rs` | `BridgeEvents` | `on_event` maps to bus events; `on_approval_needed` becomes async (remove `block_in_place`) |
| 3 | `crates/rustycode-core/src/headless/events.rs` | `HeadlessAgentBridge` | `on_event` replaces individual callbacks |
| 4 | `crates/rustycode-bench/src/agent/real_agent.rs` | `BenchObserver` | `on_event` replaces individual callbacks |
| 5 | `crates/rustycode-tui/src/app/pipeline/agent_manager.rs` | `TuiAgentBridge` | `on_event` replaces individual callbacks |
| 6 | `crates/rustycode-agent/src/session.rs` | `MockEvents` | Test helper — async `on_event` |
| 7 | `crates/rustycode-agent/src/session.rs` | `DefaultEvents` | Test helper — async `on_event` |
| 8 | `crates/rustycode-agent/tests/agent_integration.rs` | `TestEvents` | Integration test — async `on_event` |

**Pattern for each migration:**

```rust
// BEFORE (sync):
impl AgentEvents for FooEvents {
    fn on_text_delta(&mut self, delta: &str) { self.text.push_str(delta); }
    fn on_thinking_delta(&mut self, delta: &str) { /* ... */ }
    fn on_tool_call(&mut self, id: &str, name: &str, input: &Value) { /* ... */ }
    fn on_tool_result(&mut self, id: &str, name: &str, output: &str, is_error: bool) { /* ... */ }
    fn on_approval_needed(&mut self, tool: &str, input: &Value) -> ApprovalDecision {
        ApprovalDecision::AutoApproved
    }
    fn on_question(&mut self, q: &str, opts: &[String]) -> Option<String> { None }
    fn on_turn_start(&mut self, turn: usize) { /* ... */ }
    fn on_done(&mut self, result: &AgentResult) { /* ... */ }
}

// AFTER (async):
#[async_trait::async_trait]
impl AgentEvents for FooEvents {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta { content } => { self.text.push_str(&content); }
            StreamEvent::ThinkingDelta { .. } => {}
            StreamEvent::ToolCallStarted { id, name } => { /* ... */ }
            StreamEvent::ToolExecCompleted { id, name, output, is_error } => { /* ... */ }
            StreamEvent::TurnStarted { turn } => { /* ... */ }
            StreamEvent::Done => {}
            _ => {}
        }
    }
    // on_approval_needed, on_question, on_done use defaults or override as async
}
```

**Critical: BridgeEvents `block_in_place` removal**

```rust
// BEFORE (line 192 of agent_executor.rs):
fn on_approval_needed(&mut self, tool_name: &str, input: &Value) -> ApprovalDecision {
    let tool = tool_name.to_string();
    let input = input.clone();
    let interaction = self.interaction.clone();
    tokio::task::block_in_place(|| interaction.request_approval(&tool, &input))
}

// AFTER:
async fn on_approval_needed(&mut self, tool_name: &str, input: &Value) -> ApprovalDecision {
    self.interaction.request_approval(tool_name, input).await
}
```

**Note**: `PipelineInteraction::request_approval` must also become async. Check `crates/rustycode-orchestration/src/pipeline.rs` for its current signature.

---

### B4: Verify

```bash
cargo check --workspace
cargo test -p rustycode-agent
cargo test -p rustycode-orchestration
```

---

## SSE Event → StreamEvent Mapping

Reference for the `run_loop` restructuring:

| SSE Event | StreamEvent(s) | Accumulates into |
|-----------|---------------|------------------|
| `SSEEvent::Text { text }` | `TextDelta { content }` | `state.assistant_text` |
| `SSEEvent::ThinkingDelta { thinking }` | `ThinkingDelta { content }` | `state.thinking_text` |
| `SSEEvent::ContentBlockStart` (ToolUse) | `ToolCallStarted { id, name }` | `state.tools.push(...)` |
| `SSEEvent::ContentBlockDelta` (Text) | `TextDelta { content }` | `state.assistant_text` |
| `SSEEvent::ContentBlockDelta` (Thinking) | `ThinkingDelta { content }` | `state.thinking_text` |
| `SSEEvent::ContentBlockDelta` (PartialJson) | `ToolInputDelta { id, chunk }` | `state.tools.last_mut().input_json` |
| `SSEEvent::MessageDelta { usage, .. }` | `TokenUsage { input, output }` | `state.total_*_tokens` |
| Tool execution start | `ToolExecStarted { id, name }` | (side effect) |
| Tool execution end | `ToolExecCompleted { id, name, output, is_error }` | (side effect) |
| Turn loop start | `TurnStarted { turn }` | (counter) |
| Session end | `Done` | (signal) |

---

## Dependency Graph

```
B1 (protocol types)
 │
 ├──→ B2 (agent crate refactor)
 │      │
 │      └──→ B3 (update consumers)
 │             │
 │             └──→ B4 (verify)
 │
 └── (B2 and B3 must land together — trait change breaks all consumers)
```

B1 can land independently (additive only). B2+B3 must be one atomic change.

---

## What Changes for Each Consumer

### BusAgentEvents (orchestration)
- `on_text_delta` → `on_event(TextDelta)` → publish `OrchestrationEvent::StreamDelta`
- `on_tool_call` → `on_event(ToolCallStarted)` → publish `OrchestrationEvent::ToolExecutionStarted`
- `on_tool_result` → `on_event(ToolExecCompleted)` → publish `OrchestrationEvent::ToolExecutionFinished`
- Everything else: discard or log

### BridgeEvents (orchestration, with interaction)
- Same as BusAgentEvents but also forwards to interaction
- `on_approval_needed`: remove `block_in_place`, make async
- This is the key fix — proper async approval flow

### HeadlessAgentBridge (core/headless)
- `on_text_delta` → `on_event(TextDelta)` → collect final text
- `on_tool_call` / `on_tool_result` → `on_event(ToolExecCompleted)` → track writes/calls
- `on_done` → record result

### BenchObserver (bench)
- `on_text_delta` → `on_event(TextDelta)` → collect text
- `on_tool_call` / `on_tool_result` → `on_event(ToolExecCompleted)` → track metrics
- `on_done` → record metrics

### TuiAgentBridge (TUI)
- `on_text_delta` → `on_event(TextDelta)` → forward to TUI channel
- `on_tool_call` / `on_tool_result` → `on_event(ToolExecCompleted)` → forward to TUI channel
- `on_approval_needed` → async → send approval request through TUI channel, await response
- Note: This bridge is separate from the TUI's independent streaming loop (response.rs), which will be replaced in Phase B5.

---

## Resolved Questions

1. **`PipelineInteraction::request_approval`** — Currently **sync** (`&self`). Only one impl: `SilentInteraction` (auto-approve). Must become async. Change: add `#[async_trait]`, make `request_approval` and `is_cancelled` async. Update `SilentInteraction`. This eliminates `block_in_place` in `BridgeEvents`.

2. **`TuiAgentBridge` approval flow** — Currently uses channels. Making it async should be straightforward (await on channel recv instead of blocking).

3. **Backward compatibility for `StreamChunk`** — The TUI uses `StreamChunk` (20+ variants in `async_.rs`). Phase B5 will create an adapter `StreamEvent → StreamChunk`. For now, `TuiAgentBridge` can emit `StreamChunk` internally while implementing `AgentEvents` with `StreamEvent`.

### Additional change: PipelineInteraction → async

File: `crates/rustycode-orchestration/src/pipeline.rs`

```rust
// BEFORE (sync):
pub trait PipelineInteraction: Send + Sync {
    fn request_approval(&self, tool_name: &str, input: &serde_json::Value) -> ApprovalDecision;
    fn is_cancelled(&self) -> bool;
}

// AFTER (async):
#[async_trait::async_trait]
pub trait PipelineInteraction: Send + Sync {
    async fn request_approval(&self, tool_name: &str, input: &serde_json::Value) -> ApprovalDecision;
    async fn is_cancelled(&self) -> bool;
}
```

Update `SilentInteraction` impl to match. Update any callers of `is_cancelled()` and `request_approval()` to `.await`.

---

## Out of Scope (Phase B5+)

- TUI PipelineAdapter (streaming events → StreamChunk mapping)
- Replace TUI's independent streaming loop (`response.rs`)
- Event persistence / replay / rewind (needs storage design)
- Multi-agent concurrent session management
- Delete dead crates (rustycode-agents, tools-registry, tui-core)
