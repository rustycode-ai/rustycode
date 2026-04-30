# Turn-Based Stream Rendering -- Compact TUI Implementation Plan

**Date**: 2026-04-28  
**Status**: Proposed  
**Goal**: Make concurrent streaming output feel compact, stable, and easy to scan when thinking, text, tool calls, and tool output arrive together.

---

## 1. Problem Statement

The current TUI treats streaming as a mostly linear feed of chunks. That works for simple responses, but it gets noisy when a model emits:

- thinking deltas
- text deltas
- tool call start / argument deltas / stop
- tool output / progress / completion

The result is usually too much motion in the main transcript. The user sees interleaved events instead of a coherent turn.

---

## 2. Target Experience

The UI should present each assistant turn as one compact container with a few stable lanes:

- Main answer
- Thinking, collapsed by default
- Tool cards, keyed by `tool_id`
- Tool output nested under the matching tool card
- Turn status footer, showing whether the turn is live, waiting on tools, or complete

The transcript should feel like a readable summary of work, not a raw event log.

---

## 3. Design Principles

1. Group by semantic identity, not by arrival order.
2. Keep the main answer dominant and readable.
3. Collapse noisy lanes by default.
4. Let tool activity stay visible, but compact.
5. Batch and coalesce frequent deltas before repainting.
6. Preserve the raw event stream for debugging and replay, but do not render it verbatim.

---

## 4. Proposed Data Model

Introduce a turn-oriented model in the TUI layer:

```text
Conversation
  -> Turn
      -> Assistant answer buffer
      -> Thinking buffer
      -> Tool cards keyed by tool_id
      -> Status metadata
```

Suggested structures:

- `TurnState`
  - `turn_id`
  - `assistant_text: String`
  - `thinking: Option<String>`
  - `tools: HashMap<String, ToolCardState>`
  - `status: TurnStatus`

- `ToolCardState`
  - `tool_id`
  - `name`
  - `status`
  - `input_preview`
  - `output_preview`
  - `expanded: bool`

- `TurnStatus`
  - `Thinking`
  - `StreamingText`
  - `WaitingOnTools`
  - `Completing`
  - `Done`
  - `Cancelled`
  - `Error`

This should live in the TUI boundary, not in the orchestration layer.

---

## 5. Rendering Model

The renderer should become a projection from `TurnState` to visible UI elements.

### Default rendering

- Show the assistant answer inline.
- Show a small status row for the current turn.
- Show one compact tool card per active or completed tool.
- Keep thinking collapsed unless the user expands it.

### Expanded rendering

- Expand thinking into its own lane.
- Expand tool output only on demand.
- Preserve scroll position when a tool card updates.

### Compact rules

- Merge consecutive text deltas into one visible body.
- Merge consecutive thinking deltas into one visible body.
- Keep tool progress updates on the tool card instead of emitting a new row.
- Collapse completed tools older than the current turn unless the user explicitly opens them.

---

## 6. Event Reduction Layer

Add a small reducer between stream chunks and render state.

Responsibilities:

- Normalize raw chunks into turn-scoped updates.
- Deduplicate back-to-back identical chunks.
- Coalesce repeated token deltas within a short window.
- Route each event to the correct lane.
- Keep tool events attached to the right `tool_id`.

This is the right place to handle concurrency because it is aware of:

- which assistant turn is active
- which tool is currently running
- whether the model is still thinking or already answering

---

## 7. Phase Breakdown

### Phase 1: Turn State

Create the TUI-side turn state model and wire it to the current stream lifecycle.

Deliverables:

- `TurnState`
- `ToolCardState`
- `TurnStatus`
- reset logic when a new stream starts

### Phase 2: Stream Reducer

Convert `StreamChunk` into turn-scoped state transitions.

Deliverables:

- append text to the active answer buffer
- append thinking to the active thinking buffer
- attach tool updates to the matching card
- ignore duplicate consecutive deltas

### Phase 3: Compact Renderer

Render the active turn as a compact card stack rather than a raw event list.

Deliverables:

- main answer lane
- optional thinking expander
- compact tool cards
- live status strip

### Phase 4: Interaction

Add compact affordances for expanding and collapsing lanes.

Deliverables:

- toggle thinking visibility
- toggle a tool card
- preserve scroll and selection state

### Phase 5: Verification

Prove the new model does not regress the current transcript.

Deliverables:

- unit tests for event reduction
- renderer tests for collapsed vs expanded views
- integration test for mixed thinking/text/tool streaming

---

## 8. Suggested File Map

Likely files to touch:

```text
crates/rustycode-tui/src/app/handlers.rs
crates/rustycode-tui/src/app/message_ops.rs
crates/rustycode-tui/src/app/event_loop.rs
crates/rustycode-tui/src/app/streaming/*
crates/rustycode-tui/src/app/render/*
crates/rustycode-tui/src/ui/message_types.rs
```

Likely new file(s):

```text
crates/rustycode-tui/src/app/turn_state.rs
crates/rustycode-tui/src/app/turn_reducer.rs
```

---

## 9. Success Criteria

The change is successful if:

- A mixed response with thinking, text, and tool activity stays readable.
- The main answer does not get polluted by raw event churn.
- Completed tool output stays attached to the right tool.
- Duplicate or repeated deltas no longer create visible stutter.
- The compact view still preserves enough detail for debugging and review.

---

## 10. Implementation Notes

- Keep the raw stream channel intact for diagnostics and replay.
- Avoid pushing render complexity into the provider layer.
- Prefer a single authoritative reducer over ad hoc dedupe logic in many handlers.
- Keep the main transcript stable while live turn details update in place.

