# Phase 1: Unified Event System Architecture

**Status:** In Progress (40%)  
**Author:** Architect Agent  
**Last Updated:** 2026-05-12  
**Phase:** 1 of 4 — Event Consolidation

## Goal

Consolidate RustyCode's four parallel event systems into a single, unified `EventMsg` channel that carries all outbound data from Core to TUI. This eliminates callback-based communication, reduces type duplication, and establishes a clear data flow boundary where agents emit events and the UI consumes them.

## Current Progress (2026-05-12)

- **EventMsg (Implemented)**: `rustycode-protocol/src/event_msg.rs` contains the unified event type with 40+ variants.
- **Op (Implemented)**: `rustycode-protocol/src/op.rs` contains the submission queue type.
- **AgentSession (Pending)**: Still using `AgentEvents` trait; needs migration to `broadcast` channel.
- **Runtime (Pending)**: Still using manual event conversion; needs migration to `submit(Op)` API.

### 1. EventBus (rustycode-bus)
**Location:** `crates/rustycode-bus/src/lib.rs`

Type-safe pub/sub system with ~15 domain events:
- `SessionStartedEvent`, `SessionStoppedEvent`
- `ToolExecutedEvent`, `ToolApprovedEvent`
- `PlanCreatedEvent`, `StepCompletedEvent`
- `MessageAddedEvent`, `ErrorEvent`

**Current usage:** Infrastructure notifications, supplemental to primary data flow

### 2. StreamEvent (rustycode-protocol)
**Location:** `crates/rustycode-protocol/src/stream_event.rs`

Raw agent events emitted via `AgentEvents` callback trait:
- `TextDelta(String)`, `ToolCallStarted(ToolCall)`, `ToolCallFinished(ToolCallResult)`
- `ThinkingStarted`, `ThinkingDelta(String)`, `ThinkingStopped`
- `ApprovalNeeded(ToolCall)`, `Question(Question)`

**Current usage:** Passed through `AgentEvents::on_event()` callback to Runtime

### 3. OrchestrationEvent (rustycode-orchestration)
**Location:** `crates/rustycode-orchestration/src/events.rs`

Orchestration layer events:
- `StreamDelta(String)`, `ToolExecutionStarted`, `ToolExecutionCompleted`
- `PhaseTransition(Phase)`, `MilestoneProgress(MilestoneId)`

**Current usage:** Emitted within orchestration layer, NOT consistently surfaced to TUI

### 4. EventMsg (rustycode-protocol)
**Location:** `crates/rustycode-protocol/src/event_msg.rs`

40+ typed variants for Core→TUI communication:
- `TextDelta`, `TurnStarted`, `TurnCompleted`
- `ToolExecCompleted`, `ToolApprovalRequired`
- `MemoryCreated`, `MemoryDeleted`, `WorkspaceChanged`
- `CheckpointCreated`, `CheckpointRestored`

**Current usage:** Defined but NOT consistently emitted — many code paths still use callbacks

## Current Execution Flow

```
AgentSession::run()
  ↓ (callback trait)
AgentEvents::on_event(StreamEvent)
  ↓ (manual conversion)
Runtime::run()
  ↓ (direct method calls)
TUI handler methods
  ↓ (side effects)
UI updates

Parallel flow:
AgentSession::run()
  ↓
EventBus::publish(DomainEvent)
  ↓
EventBus subscribers (infrastructure only)
```

**Problems:**
- Callback-based flow is hard to trace and test
- StreamEvent → EventMsg conversion happens in multiple places
- EventBus domain events duplicate EventMsg variants
- Orchestration events never reach TUI
- No single source of truth for "what happened"

## Target Architecture (Codex Pattern)

### Core Principle

**Single outbound event type.** All data from Core to TUI flows as `EventMsg` through a broadcast channel. All commands from TUI to Core flow as `Op` through `submit()`.

### Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│  TUI Layer                                                   │
│  ┌──────────────┐         ┌──────────────────────────────┐  │
│  │ UI Component │  reads  │ EventMsg broadcast channel   │  │
│  │ (render)     │ ←───────┤ (tokio::sync::broadcast)      │  │
│  └──────────────┘         └──────────────────────────────┘  │
│         ↑                                                │  │
│         │ subscribes                                     │  │
└─────────┼────────────────────────────────────────────────┘  │
          │                                                │
          │ emits EventMsg                                 │
┌─────────┼────────────────────────────────────────────────┐  │
│  Core Layer                                               │  │
│  ┌──────────────┐         ┌──────────────────────────────┐  │
│  │AgentSession  │  emits  │ EventMsg broadcast channel   │  │
│  │(agent loop)  │ ───────→│ (tokio::sync::broadcast)     │  │
│  └──────────────┘         └──────────────────────────────┘  │
│         ↑                                                │  │
│         │                                                │  │
│  ┌──────┴──────┐         ┌──────────────────────────────┐  │
│  │ submit(Op)  │  commands │ Op enum (12 variants)       │  │
│  │ (inbound)   │ ───────→│ SendMessage, StopStream,     │  │
│  └─────────────┘         │ ApproveTool, RejectTool, etc.│  │
│                          └──────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

Side channel (infrastructure only):
┌─────────────────────────────────────────────────────────────┐
│  EventBus (rustycode-bus)                                   │
│  ┌──────────────────────────────────────────────────────┐  │
│  │ Domain events NOT part of primary data flow:         │  │
│  │ - MetricsUpdated (analytics, not UI)                 │  │
│  │ - SystemHealthChanged (monitoring, not UI)           │  │
│  │ - WorkerPoolChanged (internal orchestration)         │  │
│  └──────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Type Flow

```
Inbound:  TUI → submit(Op) → AgentSession
Outbound: AgentSession → EventMsg → broadcast channel → TUI

No callbacks. No direct method calls. No parallel event types.
```

## EventMsg Changes

### Variants to Add

**From OrchestrationEvent (currently not surfaced):**

```rust
// crates/rustycode-protocol/src/event_msg.rs

/// Orchestration phase transition
PhaseTransition {
    from: Phase,
    to: Phase,
},

/// Milestone progress update
MilestoneProgress {
    milestone_id: String,
    completed_steps: usize,
    total_steps: usize,
},

/// Strategy switch (deep-thinker)
StrategySwitch {
    from: String,
    to: String,
    reason: String,
},

/// Quality gate result
QualityGateResult {
    gate_name: String,
    passed: bool,
    score: Option<f64>,
    details: String,
},
```

**From EventBus domain events (consolidate into EventMsg):**

```rust
/// Plan created (replaces PlanCreatedEvent)
PlanCreated {
    plan_id: String,
    name: String,
    steps: Vec<StepSummary>,
},

/// Step completed (replaces StepCompletedEvent)
StepCompleted {
    step_id: String,
    status: StepStatus,
    output: String,
},

/// Memory operation result (replaces multiple EventBus events)
MemoryOperation {
    op_type: MemoryOpType, // Created, Updated, Deleted, Listed
    memory_id: String,
    content: Option<String>,
    error: Option<String>,
},
```

### Variants to Remove

**Duplicate StreamEvent variants (already in EventMsg):**

- `TextDelta` — keep, consolidate all text emission here
- `ThinkingStarted` — keep, add thinking state to TurnStarted
- `ThinkingDelta` — keep, merge into TextDelta with type annotation
- `ThinkingStopped` — keep, add to TurnCompleted

**Replaced by Op commands:**

- `ToolApproved` — use `submit(Op::ApproveTool)` instead
- `ToolRejected` — use `submit(Op::RejectTool)` instead

### Variants to Modify

```rust
// Before:
ToolExecCompleted {
    tool_name: String,
    result: String,
}

// After (add duration, exit code):
ToolExecCompleted {
    tool_name: String,
    result: String,
    duration_ms: u64,
    exit_code: Option<i32>,
}

// Before:
TurnStarted {
    turn_id: String,
}

// After (include thinking state):
TurnStarted {
    turn_id: String,
    thinking_enabled: bool,
    phase: Option<Phase>, // if in orchestration
}
```

## Op Changes

**Location:** `crates/rustycode-protocol/src/op.rs`

### New Op Variants

```rust
/// Resume from checkpoint
ResumeFromCheckpoint {
    checkpoint_id: String,
},

/// Retry failed step
RetryStep {
    step_id: String,
},

/// Skip current step
SkipStep {
    step_id: String,
},

/// Set orchestration strategy
SetStrategy {
    strategy: String,
    config: serde_json::Value,
},

/// Query milestone progress
QueryMilestoneProgress {
    milestone_id: String,
},
```

### Modified Op Variants

```rust
// Before:
ApproveTool {
    tool_call_id: String,
},

// After (add optional overrides):
ApproveTool {
    tool_call_id: String,
    modified_input: Option<String>, // for edit tool content changes
    timeout_override: Option<u64>,  // for long-running tools
},
```

## AgentSession Migration

**Location:** `crates/rustycode-agent-runtime/src/session.rs`

### Current State

```rust
pub trait AgentEvents: Send + Sync {
    fn on_event(&self, event: StreamEvent);
    fn on_approval_needed(&self, tool_call: ToolCall) -> ApprovalResponse;
    fn on_question(&self, question: Question) -> QuestionResponse;
    fn on_done(&self, report: RunReport);
}

pub struct AgentSession {
    events: Arc<dyn AgentEvents>,
    // ...
}

impl AgentSession {
    pub async fn run(&mut self) -> RunReport {
        // Agent loop emits events via callback
        self.events.on_event(StreamEvent::TextDelta(delta));
        
        // Blocking approval via callback
        let response = self.events.on_approval_needed(tool_call);
        
        // Final report via callback
        self.events.on_done(report);
    }
}
```

### Target State

```rust
pub struct AgentSession {
    // Event emission channel (outbound)
    event_tx: tokio::sync::broadcast::Sender<EventMsg>,
    
    // Command receiver (inbound)
    op_rx: tokio::sync::mpsc::UnboundedReceiver<Op>,
    
    // Pending approvals (in-memory state)
    pending_approvals: HashMap<String, ToolCall>,
}

impl AgentSession {
    pub async fn run(&mut self) -> RunReport {
        loop {
            tokio::select! {
                // Agent work generates events
                result = self.execute_next_step() => {
                    self.emit_event(EventMsg::StepCompleted { ... });
                }
                
                // Inbound commands from TUI
                Some(op) = self.op_rx.recv() => {
                    match op {
                        Op::ApproveTool { tool_call_id, .. } => {
                            self.handle_approval(tool_call_id);
                        }
                        Op::StopStream => {
                            self.emit_event(EventMsg::TurnStopped);
                            break;
                        }
                        // ... other Op handlers
                    }
                }
            }
        }
    }
    
    fn emit_event(&self, event: EventMsg) {
        let _ = self.event_tx.send(event);
    }
}

// Remove AgentEvents trait entirely
```

## Runtime Migration

**Location:** `crates/rustycode-core/src/runtime/` (split across `mod.rs`, `session_ops.rs`, `execution_ops.rs`, `event_ops.rs`, `tool_ops.rs`, `plan_ops.rs`, `memory_ops.rs`)

### Current State

```rust
impl Runtime {
    pub async fn run(&mut self) -> RunReport {
        // Direct method calls to TUI
        self.update_status(Status::Running);
        
        // Manual event conversion
        let report = self.session.run().await?;
        
        // More direct calls
        self.finalize_report(report);
        
        Ok(report)
    }
}
```

### Target State

```rust
impl Runtime {
    pub async fn run(&mut self) -> RunReport {
        // Subscribe to session's EventMsg broadcast
        let mut event_rx = self.session.subscribe_events();
        
        // Spawn session in background
        let session_handle = tokio::spawn(async move {
            self.session.run().await
        });
        
        // Event loop
        loop {
            tokio::select! {
                // Consume events from session
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            // Forward to TUI via same broadcast channel
                            // (TUI subscribes directly to session's channel)
                        }
                        Err(_) => break, // channel closed
                    }
                }
                
                // Session completion
                result = &mut session_handle => {
                    return result.unwrap_or_else(|_| RunReport::error("session crashed"));
                }
            }
        }
    }
    
    // Inbound command API (called by TUI)
    pub fn submit(&self, op: Op) {
        self.session.submit_op(op);
    }
}
```

## EventBus Role

**Location:** `crates/rustycode-bus/src/lib.rs`

### What Stays in EventBus

**Infrastructure notifications** (NOT part of primary Core→TUI data flow):

```rust
// crates/rustycode-bus/src/lib.rs

/// Worker pool state changed (orchestration internal)
struct WorkerPoolChangedEvent {
    active_workers: usize,
    queued_tasks: usize,
}

/// Metrics updated (analytics, not UI)
struct MetricsUpdatedEvent {
    token_usage: TokenStats,
    cost: CostStats,
}

/// System health changed (monitoring, not UI)
struct SystemHealthChangedEvent {
    status: HealthStatus,
    message: String,
}

/// Log entry (for logging subsystem, not TUI)
struct LogEvent {
    level: LogLevel,
    message: String,
}
```

### What Moves to EventMsg

**All domain events that affect UI state:**

| Current EventBus Event | Target EventMsg Variant |
|------------------------|-------------------------|
| `SessionStartedEvent` | `SessionStarted` (already exists) |
| `SessionStoppedEvent` | `SessionStopped` (already exists) |
| `ToolExecutedEvent` | `ToolExecCompleted` (already exists) |
| `ToolApprovedEvent` | `ToolApprovalRequired` → Op::ApproveTool |
| `PlanCreatedEvent` | `PlanCreated` (new variant) |
| `StepCompletedEvent` | `StepCompleted` (new variant) |
| `MemoryCreatedEvent` | `MemoryOperation` (new variant) |
| `MessageAddedEvent` | `MessageAdded` (already exists) |
| `ErrorEvent` | `ErrorOccurred` (already exists) |

### EventBus Usage Pattern

```rust
// BEFORE (current mixed usage)
event_bus.publish(ToolExecutedEvent { ... }); // data flow
event_bus.publish(WorkerPoolChangedEvent { ... }); // infra

// AFTER (separated)
event_tx.send(EventMsg::ToolExecCompleted { ... }); // data flow
event_bus.publish(WorkerPoolChangedEvent { ... }); // infra only
```

## Crate Changes

### rustycode-protocol

**Files to modify:**

1. **`crates/rustycode-protocol/src/event_msg.rs`**
   - Add orchestration variants (PhaseTransition, MilestoneProgress, StrategySwitch, QualityGateResult)
   - Add plan variants (PlanCreated, StepCompleted)
   - Add memory variant (MemoryOperation)
   - Modify ToolExecCompleted to include duration_ms, exit_code
   - Modify TurnStarted to include thinking_enabled, phase

2. **`crates/rustycode-protocol/src/op.rs`**
   - Add ResumeFromCheckpoint, RetryStep, SkipStep
   - Add SetStrategy, QueryMilestoneProgress
   - Modify ApproveTool to include modified_input, timeout_override

3. **`crates/rustycode-protocol/src/lib.rs`**
   - Re-export new EventMsg variants
   - Re-export new Op variants

**Files to deprecate:**

4. **`crates/rustycode-protocol/src/stream_event.rs`**
   - Mark StreamEvent enum as deprecated
   - Add migration notice: "Use EventMsg instead"

5. **`crates/rustycode-protocol/src/session_event.rs`**
   - Deprecate SyncEvent wrapper (no longer needed with EventMsg)

### rustycode-agent-runtime

**Files to modify:**

1. **`crates/rustycode-agent-runtime/src/session.rs`**
   - Remove AgentEvents trait
   - Add event_tx: broadcast::Sender<EventMsg>
   - Add op_rx: mpsc::UnboundedReceiver<Op>
   - Implement emit_event() helper
   - Implement submit_op() method
   - Modify run() to use tokio::select! for event/command loop

2. **`crates/rustycode-agent-runtime/src/lib.rs`**
   - Update AgentSession constructor to accept channels
   - Remove AgentEvents from public API

**Files to add:**

3. **`crates/rustycode-agent-runtime/src/approval.rs`** (new)
   - Pending approval state management
   - Approval timeout handling
   - Approval response mapping

### rustycode-core

**Files to modify:**

1. **`crates/rustycode-core/src/runtime/mod.rs`** (and domain files in `runtime/`)
   - Remove direct TUI method calls
   - Add submit(op: Op) public method
   - Remove on_approval_needed callback
   - Modify run() to subscribe to EventMsg broadcast

2. **`crates/rustycode-core/src/session.rs`**
   - Update SessionManager to use submit(Op) instead of callbacks
   - Wire up broadcast channel from AgentSession to TUI

3. **`crates/rustycode-core/src/lib.rs`**
   - Re-export Runtime::submit method

### rustycode-tui

**Files to modify:**

1. **`crates/rustycode-tui/src/app/event_loop.rs`** (main TUI loop)
   - Subscribe to EventMsg broadcast channel
   - Remove callback-based event handlers
   - Route EventMsg variants to UI update methods

2. **`crates/rustycode-tui/src/app/handlers/*.rs`**
   - Convert handler methods to consume EventMsg
   - Remove StreamEvent pattern matching

3. **`crates/rustycode-tui/src/app/input/handler.rs`** (was `event_loop.rs`)
   - Route user input to submit(Op) calls
   - Remove direct Runtime method calls

### rustycode-orchestration

**Files to modify:**

1. **`crates/rustycode-orchestration/src/events.rs`**
   - Deprecate OrchestrationEvent enum
   - Add conversion function: orchestration_event_to_event_msg()

2. **`crates/rustycode-orchestration/src/executor.rs`**
   - Emit EventMsg instead of OrchestrationEvent
   - Use broadcast channel for phase transitions

### rustycode-bus

**Files to modify:**

1. **`crates/rustycode-bus/src/lib.rs`**
   - Remove domain event types that moved to EventMsg
   - Keep only infrastructure event types
   - Update documentation to clarify EventBus scope

2. **`crates/rustycode-bus/src/events.rs`**
   - Remove: SessionStartedEvent, SessionStoppedEvent, ToolExecutedEvent, etc.
   - Keep: WorkerPoolChangedEvent, MetricsUpdatedEvent, SystemHealthChangedEvent

## Migration Strategy

### Phase 1A: Foundation (1 week)

**Goal:** Establish EventMsg broadcast channel without breaking existing code.

1. **Add broadcast channel to AgentSession**
   - Add event_tx field alongside existing AgentEvents callback
   - Implement emit_event() helper that sends to BOTH channel and callback
   - Emit EventMsg for all StreamEvent callbacks (dual emission)

2. **Add Op receiver to AgentSession**
   - Add op_rx field
   - Implement submit_op() method
   - Handle Op::ApproveTool in pending_approvals map

3. **Wire up Runtime**
   - Add Runtime::submit(op: Op) method
   - Subscribe to AgentSession's EventMsg broadcast
   - Forward EventMsg to TUI (existing callback path)

4. **Verify:** All existing tests pass, no behavior change

### Phase 1B: TUI Migration (1 week)

**Goal:** TUI consumes EventMsg directly, removes callback handlers.

1. **Subscribe to EventMsg in TUI event loop**
   - Replace callback registration with broadcast subscription
   - Add EventMsg routing to existing handler methods

2. **Convert TUI handlers to EventMsg**
   - Change handler signatures from StreamEvent → EventMsg
   - Update all call sites

3. **Remove AgentEvents trait**
   - Remove callback implementation from TUI
   - Remove AgentEvents from rustycode-agent-runtime public API

4. **Verify:** TUI renders correctly, no regression

### Phase 1C: EventBus Cleanup (3 days)

**Goal:** Move domain events from EventBus to EventMsg.

1. **Audit EventBus usage**
   - Search for all event_bus.publish() calls
   - Categorize as "data flow" or "infrastructure"

2. **Migrate data flow events to EventMsg**
   - Replace event_bus.publish(ToolExecutedEvent) with emit_event(EventMsg::ToolExecCompleted)
   - Repeat for PlanCreatedEvent, StepCompletedEvent, etc.

3. **Remove migrated variants from EventBus**
   - Delete domain event types from rustycode-bus
   - Update EventBus documentation

4. **Verify:** EventBus only contains infrastructure events

### Phase 1D: Orchestration Integration (1 week)

**Goal:** Orchestration layer emits EventMsg for phase/milestone progress.

1. **Add orchestration variants to EventMsg**
   - PhaseTransition, MilestoneProgress, StrategySwitch, QualityGateResult

2. **Emit EventMsg in orchestration**
   - Replace OrchestrationEvent::emit() with EventMsg emission
   - Wire up broadcast channel in orchestration executor

3. **Surface orchestration events in TUI**
   - Add TUI rendering for PhaseTransition, MilestoneProgress
   - Update status bar to show current phase

4. **Verify:** Orchestration progress visible in TUI

### Phase 1E: Cleanup (3 days)

**Goal:** Remove deprecated types and consolidate.

1. **Remove deprecated StreamEvent enum**
   - Delete crates/rustycode-protocol/src/stream_event.rs
   - Update all references to EventMsg

2. **Remove deprecated SyncEvent wrapper**
   - Delete crates/rustycode-protocol/src/session_event.rs

3. **Remove dual emission code**
   - Remove AgentEvents callback paths
   - Keep only EventMsg emission

4. **Final verification**
   - All tests pass
   - Zero clippy warnings
   - TUI functional with EventMsg only

## Backward Compatibility

### During Migration

- **Dual emission:** Emit to both EventMsg channel and AgentEvents callback
- **Feature flag:** Add `unified_events` feature flag, default false
- **Gradual rollout:** Enable flag per-crate to test incrementally

### Post-Migration

- **Deprecation period:** Keep deprecated types for 1 release cycle with `#[deprecated]` attrs
- **Migration guide:** Document StreamEvent → EventMsg mapping in CLAUDE.md
- **Tests:** Add tests that verify EventMsg emission for all agent operations

## Success Criteria

### Functional Requirements

1. **Single event type:** All Core→TUI data flows as EventMsg
2. **Command API:** All TUI→Core commands use `submit(Op)`
3. **Zero callbacks:** AgentEvents trait removed from codebase
4. **Orchestration visible:** Phase/milestone events surface in TUI
5. **EventBus scoped:** Only infrastructure events remain

### Verification Tests

```rust
// 1. EventMsg emission test
#[tokio::test]
async fn agent_session_emits_event_msg_for_all_operations() {
    let (tx, mut rx) = broadcast::channel(100);
    let session = AgentSession::new_with_event_channel(tx);
    
    session.run().await.unwrap();
    
    // Verify we received all expected EventMsg variants
    assert!(matches!(rx.recv().await.unwrap(), EventMsg::TurnStarted { .. }));
    assert!(matches!(rx.recv().await.unwrap(), EventMsg::TextDelta { .. }));
    assert!(matches!(rx.recv().await.unwrap(), EventMsg::TurnCompleted { .. }));
}

// 2. Op submission test
#[tokio::test]
async fn agent_session_handles_approve_tool_op() {
    let (event_tx, _) = broadcast::channel(100);
    let (op_tx, op_rx) = mpsc::unbounded_channel();
    let mut session = AgentSession::new_with_channels(event_tx, op_rx);
    
    // Simulate pending approval
    session.add_pending_approval("tool-1".to_string(), tool_call);
    
    // Submit approval
    op_tx.send(Op::ApproveTool { tool_call_id: "tool-1".to_string(), modified_input: None, timeout_override: None }).unwrap();
    
    // Verify tool execution proceeded
    assert!(session.pending_approvals.is_empty());
}

// 3. EventBus scope test
#[test]
fn event_bus_contains_only_infrastructure_events() {
    // Verify no domain events in EventBus
    assert!(!std::any::TypeId::of::<SessionStartedEvent>().is::<DomainEvent>());
    assert!(std::any::TypeId::of::<WorkerPoolChangedEvent>().is::<DomainEvent>());
}

// 4. TUI integration test
#[tokio::test]
async fn tui_renders_orchestration_progress() {
    let (tx, mut rx) = broadcast::channel(100);
    let mut tui = TestTui::new_with_subscription(&tx);
    
    // Emit orchestration event
    tx.send(EventMsg::PhaseTransition {
        from: Phase::Planning,
        to: Phase::Execution,
    }).unwrap();
    
    // Verify TUI updates
    tui.handle_events().await;
    assert_eq!(tui.current_phase(), Phase::Execution);
}
```

### Performance Metrics

- **Event latency:** < 1ms from AgentSession emit to TUI receive
- **Channel capacity:** < 1000 messages buffered (no memory bloat)
- **Zero callback overhead:** Remove Arc<dyn AgentEvents> allocation

## Next Steps

After Phase 1 completion:

1. **Phase 2:** Consolidate persistence layer (Session state → EventStore)
2. **Phase 3:** Unify tool execution (ToolExecutor → ToolService)
3. **Phase 4:** Extract TUI state management (AppState → StateStore)

See `docs/architecture/PHASE2-EVENT-SOURCING.md` for Phase 2 design.
