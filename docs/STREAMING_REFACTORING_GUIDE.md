# Streaming Module Refactoring Guide

## Overview

This document captures the streaming infrastructure refactoring work and provides a roadmap for completing the TUI integration.

## Current Status

### ✅ Completed Work

1. **Unified Streaming Infrastructure** (`crates/rustycode-core/src/streaming/`)
   - `tool_state.rs`: `ToolAccumulator` struct
   - `processor.rs`: `SseEventProcessor` + `StreamingCallbacks` trait
   - `mod.rs`: Public API exports
   - Comprehensive unit tests for all code paths

2. **Headless Full Migration** (`crates/rustycode-core/src/headless/mod.rs`)
   - Replaced SSE match arm with `processor.process_event()` calls
   - Created `HeadlessStreamCallbacks<'a>` implementing `StreamingCallbacks`
   - All headless functionality preserved (repetition detection, token counting, etc.)
   - ~100 lines of duplication eliminated

3. **TUI Infrastructure Preparation** (`crates/rustycode-tui/src/app/streaming/`)
   - Re-export: `pub use rustycode_core::streaming::ToolAccumulator as ActiveToolUse`
   - Backward-compatible API layer

### 🟡 Deferred: Full TUI Integration

Deferred to preserve stability while headless is in production use.

## Architecture

### StreamingCallbacks Trait

```rust
pub trait StreamingCallbacks {
    fn on_text(&mut self, text: &str);
    fn on_thinking(&mut self, _thinking: &str) {}
    fn on_tool_start(&mut self, _id: &str, _name: &str) {}
    fn on_tool_complete(&mut self, tool: ToolAccumulator);
    fn on_content_block_stop(&mut self) {}
    fn on_message_delta(&mut self, stop_reason: Option<&str>, usage: Option<&Usage>);
    fn on_message_stop(&mut self);
    fn on_error(&mut self, error_type: &str, message: &str);
}
```

### SseEventProcessor

- Maintains state: `in_tool_use` flag, `active_tool` accumulator
- Calls callbacks for each semantic event
- Returns `Ok(bool)` where `false` means stop processing

## TUI Integration Roadmap

### Step 1: Prepare Callbacks Structure

**Status**: DONE in `response.rs` (lines 213-289)

The `TuiStreamCallbacks<'a>` struct is already prepared with all necessary callbacks:
- `on_text()`: Accumulates text, filters tool-like JSON, sends `StreamChunk::Text`
- `on_thinking()`: Accumulates thinking, sends `StreamChunk::Thinking`
- `on_tool_start()`: Sets `in_tool_use`, creates `ToolAccumulator`
- `on_tool_complete()`: Stores completed tool for main loop execution
- `on_content_block_stop()`: Resets `in_tool_use` for text blocks
- `on_message_delta()`: Updates `stop_action`
- `on_error()`: Sends `StreamChunk::Error`

### Step 2: Integrate Processor into Main Loop

**Location**: `response.rs`, around line 630-1040

**Current Structure**:
```rust
loop {
    let chunk_result = stream.next(); // Get SSE event
    
    match chunk_result {
        Ok(event) => {
            match &event {
                SSEEvent::Text { text } => { /* handle text */ }
                SSEEvent::ThinkingDelta { thinking } => { /* handle thinking */ }
                SSEEvent::SignatureDelta { signature } => { /* handle signature */ }
                SSEEvent::ContentBlockStart { .. } => { /* handle start */ }
                SSEEvent::ContentBlockDelta { .. } => { /* handle delta */ }
                SSEEvent::ContentBlockStop { .. } => {
                    if let Some(tool) = active_tool.take() {
                        // COMPLEX: Tool approval/execution flow here
                        // Approval request → wait for approval_rx → execute tool
                        // Question handling with question_rx
                    }
                }
                SSEEvent::MessageDelta { .. } => { /* handle delta */ }
                SSEEvent::MessageStop => { /* handle stop */ }
                _ => { /* fallback */ }
            }
        }
    }
}
```

**Refactored Structure**:
```rust
let mut processor = SseEventProcessor::new();

loop {
    let chunk_result = stream.next(); // Get SSE event
    
    match chunk_result {
        Ok(event) => {
            // Handle non-standard events (e.g., SignatureDelta)
            if let SSEEvent::SignatureDelta { signature } = &event {
                thinking_signature.push_str(signature);
            }

            // Process event through unified processor
            {
                let mut callbacks = create_tui_callbacks(
                    &mut assistant_response,
                    &mut thinking_content,
                    &mut in_tool_use,
                    &mut active_tool,
                    &mut stop_action,
                    &stream_tx,
                );
                let keep_going = processor.process_event(event, &mut callbacks)?;
                if !keep_going {
                    break; // Error or MessageStop
                }
            } // callbacks dropped, borrows released

            // Handle tool execution after processor marks tool complete
            if let Some(tool) = active_tool.take() {
                // Approval request → wait for approval_rx → execute tool
                // Question handling with question_rx
                // (existing code, unchanged)
            }
        }
    }
}
```

### Step 3: Remove Old Event Handlers

Delete or deprecate:
- `crate::app::streaming::events::handle_sse_event()`
- `crate::app::streaming::tool_detection::handle_content_block_start()`
- `crate::app::streaming::tool_detection::handle_partial_json()`
- `crate::app::streaming::tool_detection::looks_like_tool_call()` (migrate to callback)

Keep:
- `crate::app::streaming::tool_detection::handle_message_delta()` (used in callback)
- All tool execution code (approve, execute, snapshot, question handling)

### Step 4: Test Thoroughly

- Unit tests for `TuiStreamCallbacks`
- Integration tests with mock approval/question flows
- Manual testing:
  - Text streaming
  - Tool execution with approval
  - Question tool with answers
  - Error handling and timeouts
  - Thinking block rendering

## Implementation Checklist

- [ ] Add `use rustycode_core::streaming::{SseEventProcessor, StreamingCallbacks};` to imports
- [ ] Add `let mut processor = SseEventProcessor::new();` before main loop
- [ ] Replace SSE match arm with `processor.process_event()` call
- [ ] Wrap callbacks in a scope to manage borrows
- [ ] Extract tool execution to happen after processor.process_event()
- [ ] Remove old event handler functions
- [ ] Run full test suite: `cargo test --workspace`
- [ ] Manual testing: Stream response with tools, approval, questions
- [ ] Code review for approval/question flow preservation

## Risk Mitigation

1. **Test Coverage**: Processor has comprehensive unit tests
2. **Headless Validation**: Headless is fully migrated and working
3. **Backward Compatibility**: TUI imports already point to shared ToolAccumulator
4. **Minimal Changes**: Main change is event dispatch mechanism, not logic
5. **Gradual Rollout**: Test in feature branch first, separate PR from headless work

## Timeline Estimate

- Implementation: 2-3 hours
- Testing: 1-2 hours
- Code review: 1 hour
- **Total**: 1 workday

## Key Files

- `crates/rustycode-core/src/streaming/processor.rs` - Event processor
- `crates/rustycode-tui/src/app/streaming/response.rs` - Main integration point
- `crates/rustycode-tui/src/app/streaming/tool_execution.rs` - Tool execution (unchanged)
- `crates/rustycode-core/src/headless/mod.rs` - Reference implementation

## Notes

- The TUI integration is more complex than headless due to approval/question flows
- Approval/question handling must run in the main loop after processor.process_event()
- Tool execution logic (snapshot, execute, track) remains unchanged
- This refactoring eliminates code duplication without changing functionality
