# Revised Unified Agent Architecture — Detailed Task Breakdown

**Date:** 2026-05-06
**Source:** `.planning/UNIFIED_AGENT_ARCHITECTURE_REVISED.md` (authoritative)
**Status:** Codebase-audited implementation plan
**Estimated new code:** ~500 LOC across 5 crates

---

## Audit Summary

All target files were audited across 6 parallel slices. Key findings:

| What we found | Implication |
|---|---|
| `AgentMessage<T>` already has `to`, `directed()`, `with_reply()` | Slice 1 is additive-only, not a redesign |
| `AgentCommunicationHub` in `multi_agent.rs` already routes messages | Slice 2 can extend rather than build from scratch |
| `AgentRegistry` has specialist generation + task history | Slice 3 adds capability descriptors, not a new registry |
| `SendMessageTool` is a pure stub (no delivery) | Slice 4 is pure wiring, no refactoring needed |
| `TaskDispatcher` routes through `ForkJoinExecutor` (V1 adapter) | Slice 5 replaces adapter, keeps ForkJoin for parallelism |
| 6 overlapping role enums, zero conversions between them | Slice 7 is documentation + thin conversion, not consolidation |

---

## Workstream Dependency Graph

```
S1 Protocol ──► S2 MailboxRouter ──► S4 send_message wiring
     │                                       │
     │         S3 Registry ◄─────────────────┘
     │              │
     └──────────────┼──► S5 TaskDispatcher V2
                    │         │
                    └─────────┼──► S6 Harness enforcement
                              │
                    S7 Role reconciliation (parallel with S3–S6)
                    S8 Tests (after S1–S7)
                    S9 Docs (last)
```

---

## Slice 1 — Protocol Narrowing

### Objective
Add `AgentPayload` enum and agent instance IDs to the existing protocol layer.

### Audited Files

**`crates/rustycode-protocol/src/agent_protocol.rs`**
- `AgentMessage<T>` (line 57): already has `id`, `from: AgentRole`, `to: Option<AgentRole>`, `payload: T`, `in_reply_to: Option<String>`
- `AgentRole` (line 98): 10 variants, `#[non_exhaustive]`, `Copy + Hash`
- Helper methods `new()`, `directed()`, `with_reply()` (lines 71-89)
- Existing typed messages: `ArchitectMessage`, `BuilderMessage`, `SkepticMessage`, `JudgeMessage`, `ScalpelMessage`

### What to add

1. **Agent instance ID field** — add `sender_id: String` and `recipient_id: Option<String>` to `AgentMessage<T>` for routing to specific agent instances (not just roles)

2. **`AgentPayload` enum** (~50 LOC, after line 543):
```rust
/// Payloads for routed local agent messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum AgentPayload {
    /// Delegate a task to another agent.
    TaskDelegation { task_id: String, prompt: String, role: AgentRole },
    /// Return a task result.
    TaskResult { task_id: String, success: bool, output: String },
    /// Advertise capabilities (runtime).
    CapabilityAdvertise { agent_id: String, capabilities: Vec<String> },
    /// Query for agents with a capability.
    CapabilityQuery { capability: String },
    /// Response to a capability query.
    CapabilityResponse { agents: Vec<String> },
    /// Object to a decision or proposed action.
    Objection { reason: String, evidence: String },
}
```

3. **Helper constructors** on `AgentMessage<AgentPayload>`:
   - `delegation(from, to, task_id, prompt, role)`
   - `task_result(from, to, task_id, success, output)`
   - `capability_advertise(agent_id, caps)`
   - `capability_query(from, capability)`

### Files touched
- `crates/rustycode-protocol/src/agent_protocol.rs` — add `AgentPayload`, extend `AgentMessage<T>` fields
- `crates/rustycode-protocol/src/lib.rs` — re-export `AgentPayload`

### Acceptance Criteria
- `AgentMessage<AgentPayload>` serializes/deserializes cleanly
- Existing `AgentMessage<T>` usage compiles unchanged
- Existing 30+ protocol tests pass

### Estimated size
~100 LOC

---

## Slice 2 — MailboxRouter

### Objective
Add thin directed routing that complements the existing broadcast bus.

### Audited Files

**`crates/rustycode-orchestration/src/bus.rs`**
- `BusHandle` (line 257): `Arc<broadcast::Sender<OrchestrationEvent>>`, fire-and-forget broadcast only
- `OrchestrationEvent`: 35 variants, all broadcast to all subscribers
- No directed delivery, no agent-specific addressing

**`crates/rustycode-runtime/src/multi_agent.rs`**
- `AgentCommunicationHub` (line 287): already has `send_request()`, `send_response()`, `broadcast()`, `pending_for_agent()`
- `AgentMessage::{Request, Response, Broadcast}` (line 234) — three variants with confidence tracking
- In-memory only, uses `tokio::mpsc` channels internally

### Decision: extend `AgentCommunicationHub` or build new?

The revised architecture doc specifies a new `MailboxRouter` in orchestration (~150 LOC). But `AgentCommunicationHub` already exists in runtime and does most of the work.

**Recommendation:** Build `MailboxRouter` as a thin wrapper in orchestration that:
1. Uses the same `tokio::mpsc` channel pattern as `AgentCommunicationHub`
2. Works with `AgentMessage<AgentPayload>` from protocol (Slice 1)
3. Emits `OrchestrationEvent` observability via `BusHandle`
4. Does NOT replace `AgentCommunicationHub` — both coexist until runtime is consolidated

### What to build

**New file: `crates/rustycode-orchestration/src/mailbox_router.rs`** (~150 LOC)

```rust
pub struct MailboxRouter {
    inboxes: Arc<Mutex<HashMap<String, mpsc::Sender<AgentMessage<AgentPayload>>>>>,
    bus: BusHandle,  // for observability
}

impl MailboxRouter {
    pub fn new(bus: BusHandle) -> Self;
    pub fn register(&self, agent_id: String) -> mpsc::Receiver<AgentMessage<AgentPayload>>;
    pub fn unregister(&self, agent_id: &str);
    pub async fn send(&self, message: AgentMessage<AgentPayload>) -> Result<(), SendError>;
    pub async fn broadcast(&self, from: &str, payload: AgentPayload) -> Vec<Result<(), SendError>>;
}
```

Key behaviors:
- `register()` creates an mpsc channel and stores the Sender
- `send()` looks up recipient by `recipient_id`, delivers to their channel
- `broadcast()` iterates all registered agents except sender
- Each send/broadcast emits a bus event for observability
- Unknown recipient returns error (no silent drop)

### Files touched
- `crates/rustycode-orchestration/src/mailbox_router.rs` — NEW
- `crates/rustycode-orchestration/src/lib.rs` — add `pub mod mailbox_router`

### Acceptance Criteria
- Two agents can exchange directed messages
- Broadcast reaches all except sender
- Bus events are emitted for each routed message
- Unregister cleans up without panicking

### Estimated size
~150 LOC

---

## Slice 3 — Extend AgentRegistry

### Objective
Add structured capability advertisement and availability tracking.

### Audited Files

**`crates/rustycode-orchestration/src/agent_registry.rs`**
- `AgentRegistry` (line 203): `built_in: HashMap<String, AgentRole>`, `generated: HashMap<String, SpecialistAgent>`, `task_history: Vec<TaskAgentMatch>`
- `SpecialistAgent` (line 155): `id`, `name`, `specialist_type`, `role`, `instructions`, `tools: Vec<String>`, `source_task`
- `SpecialistType` (line 28): 5 variants (DatabaseMigration, SecurityAudit, TestDebugging, PerformanceOptimization, ApiIntegration)
- `TaskAgentMatch` (line 214): `task_type`, `agent_id`, `success`, `timestamp`
- `AgentSelection` (line 382): `StandardTeam`, `Reuse`, `NewSpecialist`
- `AgentInfo` (line 397): `id`, `name`, `kind: AgentKind`

### What to add

1. **`CapabilityDescriptor` struct** (~30 LOC):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDescriptor {
    pub name: String,
    pub description: String,
    pub available: bool,
    pub success_rate: f64,  // computed from task_history
    pub tool_scope: Vec<String>,
}
```

2. **Extend `SpecialistAgent`** — add `capabilities: Vec<CapabilityDescriptor>`

3. **Lookup methods on `AgentRegistry`**:
   - `find_by_capability(capability: &str) -> Vec<&SpecialistAgent>` — filter by capability name
   - `find_available(capability: &str) -> Vec<&SpecialistAgent>` — filter by capability + availability
   - `rank_by_success(capability: &str) -> Vec<&SpecialistAgent>` — sort by success_rate from task_history

4. **Availability tracking** — add `available: bool` field to `SpecialistAgent`, default `true`, set `false` when executing

### Files touched
- `crates/rustycode-orchestration/src/agent_registry.rs` — add `CapabilityDescriptor`, extend `SpecialistAgent`, add lookup methods

### Acceptance Criteria
- `registry.find_by_capability("security_audit")` returns matching specialists
- `registry.find_available("security_audit")` excludes busy agents
- Specialist generation still works
- Existing registry tests pass

### Estimated size
~120 LOC

---

## Slice 4 — Wire `send_message` to Real Delivery

### Objective
Replace stub execution with routed message delivery.

### Audited Files

**`crates/rustycode-tools/src/providers/send_message.rs`**
- `SendMessageTool` (line 9): pure stub
- `execute()` (line 64): returns `ToolOutput::with_structured("Message sent to {to}", ...)` — no actual delivery
- Schema: `to` (string, required), `message` (string, required), `summary` (string, optional)
- `ToolPermission::None` (line 36) — no permission gate
- `ToolContext` — passed as `&_ctx` (unused)

### What to change

The challenge: `ToolContext` doesn't currently carry a router reference. Two approaches:

**Option A (trait injection):** Add a `MessageRouter` trait to the tools crate, implement in orchestration:
```rust
pub trait MessageRouter: Send + Sync {
    fn send(&self, to: &str, message: &str) -> Result<(), String>;
    fn broadcast(&self, from: &str, message: &str) -> Result<(), String>;
}
```
Then `ToolContext` carries `Option<Arc<dyn MessageRouter>>`.

**Option B (callback):** Add `message_sender: Option<Box<dyn Fn(&str, &str) -> Result<()> + Send + Sync>>` to `ToolContext`.

**Recommendation:** Option A — cleaner, testable, avoids closure lifetime issues.

### Implementation steps

1. Add `MessageRouter` trait to `crates/rustycode-tools-api/` (or `crates/rustycode-tools/src/`)
2. Add `message_router: Option<Arc<dyn MessageRouter>>` to `ToolContext`
3. Update `SendMessageTool::execute()`:
   - If `ctx.message_router` is Some, route through it
   - If None, fall back to current stub behavior (backward compat)
   - Return real delivery status (sent/failed/unknown recipient)
4. Wire `MailboxRouter` → `MessageRouter` impl in orchestration

### Files touched
- `crates/rustycode-tools/src/providers/send_message.rs` — wire real delivery
- `crates/rustycode-tools/src/context.rs` (or wherever ToolContext is defined) — add router field
- `crates/rustycode-orchestration/src/mailbox_router.rs` — impl `MessageRouter` for `MailboxRouter`

### Acceptance Criteria
- `send_message` routes through `MailboxRouter` when available
- Unknown recipient returns a real error
- Broadcast delivers to all registered agents
- Without router (legacy path), tool still returns stub response

### Estimated size
~80 LOC

---

## Slice 5 — TaskDispatcher V2

### Objective
Replace ForkJoinExecutor adapter path with real `AgentSession` execution.

### Audited Files

**`crates/rustycode-orchestration/src/task_dispatcher.rs`**
- `TaskDispatcher` (line 63): `new()`, `with_runner()`, `dispatch()`
- V1 execution: `execute_single()`, `execute_parallel()`, `execute_ensemble()` — all route through ForkJoinExecutor
- Adapter: `task_spec_to_fork_spec()` converts TaskSpec → ForkSpec

**`crates/rustycode-orchestration/src/delegation.rs`**
- `TaskSpec` (line 94): `task_id`, `prompt`, `role: TaskRole`, `resume_from`, `path_scope`, `tier_override`, `budget_limit`, `max_steps`
- `TaskRole` (line 21): Explore, Research, Code, Review, Verify, Plan, Debug
- `SpawnDecision` (line 183): Inline, Spawn(TaskSpec), SpawnParallel(Vec<TaskSpec>), Ensemble(EnsemblePlan)
- `DelegationPlanner` (line 286): three-gate model (context pressure > 0.75, complexity > 3.0, ensemble fallback)

**`crates/rustycode-agent-runtime/src/session.rs`**
- `AgentSession` (line 125): `new()`, `with_intelligence()`, `with_hooks()`, `with_tier()`
- `AgentConfig` (line 28): `max_turns: 25`, `timeout_secs: 900`, `max_tool_result_bytes: 8000`
- `AgentResult` (line 83): `final_text`, `messages`, `stopped_reason`, token usage

**`crates/rustycode-orchestration/src/fork_join.rs`**
- `ForkJoinExecutor` (line 245): uses `tokio::JoinSet`, semaphore-bound (max 4)
- `ForkSpec` (line 98): path isolation, optional resume, semantic roles
- `ContextSnapshot` (line 21): budget, tokens, workspace state

**`crates/rustycode-orchestration/src/conductor.rs`**
- `EscalationDecision` (line 70): Retry, Escalate{next_tier, reason}, Abandon{reason}, WarnBudget
- Budget enforcement (line 94), hallucination detection (line 228)

### What to change

1. **Add `AgentSession` execution path to `TaskDispatcher`**:
   - New method `execute_via_session(task_spec, provider, tools) -> TaskResult`
   - Maps `TaskSpec` → `AgentConfig` (role → system prompt, budget_limit → AgentConfig, max_steps → max_turns)
   - Creates `AgentSession`, runs it, collects `AgentResult` → `TaskResult`

2. **Keep ForkJoinExecutor for parallel execution**:
   - `SpawnParallel` still uses `ForkJoinExecutor` for bounded concurrency
   - But each fork runs through `execute_via_session()` instead of the V1 adapter

3. **Use extended registry for target selection**:
   - `Spawn(TaskSpec)` with `role` hint → look up agents via `registry.find_available()`

4. **Preserve existing delegation lifecycle events**:
   - `TaskSpawned`, `TaskDelegationCompleted`, `TaskDelegationFailed` still emitted

### Files touched
- `crates/rustycode-orchestration/src/task_dispatcher.rs` — add `execute_via_session()`, update `dispatch()`
- `crates/rustycode-orchestration/src/delegation.rs` — possibly add `TaskSpec → AgentConfig` mapping helper

### Acceptance Criteria
- `Spawn(TaskSpec)` runs through real `AgentSession`, not ForkJoinExecutor adapter
- `SpawnParallel` uses bounded parallelism with real sessions
- Delegation events are still emitted
- Failures produce structured `TaskResult` with error details

### Estimated size
~120 LOC

---

## Slice 6 — Delegation Boundaries and Harness Enforcement

### Objective
Hard boundaries on delegation depth, tool scope, and ancestry.

### Audited Files

- `DelegationPlanner` at `delegation.rs:286` — three-gate model
- `Conductor` at `conductor.rs:70` — escalation decisions, budget enforcement
- `ToolSet` at `team.rs:320` — All, ReadOnly, VerificationOnly, TargetedFix
- `TierIsolation` — path scope enforcement
- `ToolPermission` in tools crate

### What to add

1. **`DelegationToken` on `TaskSpec`** (~30 LOC):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationToken {
    pub parent_agent_id: String,
    pub depth: u32,
    pub max_depth: u32,  // default: 3
    pub allowed_roles: Vec<AgentRole>,
    pub allowed_tools: ToolSet,
}
```

2. **Depth enforcement** in `TaskDispatcher::dispatch()`:
   - Check `token.depth < token.max_depth` before spawning
   - Child task gets `DelegationToken { depth: parent.depth + 1, .. }`

3. **Tool scoping** — when creating child `AgentSession`:
   - Filter available tools by `allowed_tools` from parent's `DelegationToken`
   - Use existing `ToolSet` → tool name filtering from `team.rs`

4. **Ancestry inspection** — add `delegation_chain()` method that walks up the token tree for debugging

### Files touched
- `crates/rustycode-orchestration/src/delegation.rs` — add `DelegationToken` to `TaskSpec`
- `crates/rustycode-orchestration/src/task_dispatcher.rs` — enforce depth and tool scope
- `crates/rustycode-orchestration/src/task_context.rs` — propagate token through context

### Acceptance Criteria
- Delegation depth cannot exceed `max_depth` (default 3)
- Child agents get a subset of parent's tools
- Delegation chain is inspectable for debugging
- Shared workspace is used for artifacts only, not live coordination

### Estimated size
~80 LOC

---

## Slice 7 — Role-System Reconciliation

### Objective
Document relationships and add thin conversion helpers, not a grand rewrite.

### Audited Role Enums

| Role Enum | Location | Variants | Purpose |
|---|---|---|---|
| `AgentRole` | `agent_protocol.rs:98` | 10: Architect, Builder, Skeptic, Judge, Scalpel, Coordinator, Planner, Worker, Reviewer, Researcher | Agent routing, message addressing |
| `TeamRole` | `team.rs:274` AND `ensemble.rs:265` | 6: Builder, Skeptic, Judge, Coordinator, Architect, Scalpel | Team composition (subset of AgentRole) |
| `TaskRole` | `delegation.rs:21` | 7: Explore, Research, Code, Review, Verify, Plan, Debug | Task delegation, tier mapping |
| `CrewRole` | `ast/crew.rs:34` | 6: Foreman, Scout, Architect, Builder, Inspector, Consultant | AST pipeline phases |
| `PermissionRole` | `permission_role.rs:13` | 7: Worker, Planner, Reviewer, Researcher, Architect, Skeptic, Judge | Access control |

### Findings

1. **TeamRole is a strict subset of AgentRole** — all 6 TeamRole variants exist in AgentRole
2. **TeamRole is duplicated** — identical definitions in `ensemble.rs:265` and `team.rs:274`
3. **Zero conversion traits** — no `From<TeamRole>` for `AgentRole`, no `TryFrom<TaskRole>` for anything
4. **Different semantics** — TaskRole (task phase), CrewRole (AST phase), PermissionRole (access), AgentRole/TeamRole (identity)

### What to do

1. **Document canonical role anchor: `AgentRole`** — the routing/messaging layer should use this
2. **Add conversion impls** (~40 LOC):
```rust
impl From<TeamRole> for AgentRole { /* 6-line match */ }
impl TryFrom<TaskRole> for AgentRole { /* 7-line match, some map to same AgentRole */ }
```
3. **Remove duplicate** — `ensemble.rs:265` TeamRole should re-export from `team.rs` or vice versa
4. **Do NOT merge** — each role enum serves a distinct layer; unification would over-couple them

### Files touched
- `crates/rustycode-protocol/src/agent_protocol.rs` — add `From<TeamRole>` and `TryFrom<TaskRole>`
- `crates/rustycode-protocol/src/ensemble.rs` — remove duplicate TeamRole, re-export from team.rs

### Acceptance Criteria
- `AgentRole::from(TeamRole::Builder)` works
- Registry and routing use `AgentRole` as canonical type
- Existing role-specific code compiles unchanged

### Estimated size
~50 LOC

---

## Slice 8 — Tests

### Objective
Prove the new behavior works without destabilizing existing orchestration.

### Test matrix

| Area | Test | Target file |
|---|---|---|
| Protocol | `AgentPayload` serde round-trip | `agent_protocol.rs` tests |
| Protocol | `AgentMessage<AgentPayload>` directed + reply linkage | `agent_protocol.rs` tests |
| Protocol | Backward compat: existing `AgentMessage<BuilderMessage>` still works | `agent_protocol.rs` tests |
| Router | Register → send → receive (two agents) | `mailbox_router.rs` tests |
| Router | Broadcast reaches all except sender | `mailbox_router.rs` tests |
| Router | Unknown recipient returns error | `mailbox_router.rs` tests |
| Router | Unregister prevents further delivery | `mailbox_router.rs` tests |
| Router | Bus events emitted on send/broadcast | `mailbox_router.rs` tests |
| Registry | Find by capability returns matching specialists | `agent_registry.rs` tests |
| Registry | Find available excludes busy agents | `agent_registry.rs` tests |
| Registry | Rank by success uses task_history | `agent_registry.rs` tests |
| Registry | Specialist generation still works | `agent_registry.rs` tests |
| Tool | `send_message` with router: real delivery | `send_message.rs` tests |
| Tool | `send_message` without router: stub fallback | `send_message.rs` tests |
| Tool | Invalid recipient returns error | `send_message.rs` tests |
| Dispatcher | `Spawn` → real session path | `task_dispatcher.rs` tests |
| Dispatcher | `SpawnParallel` → bounded execution | `task_dispatcher.rs` tests |
| Dispatcher | Failure propagation from session | `task_dispatcher.rs` tests |
| Harness | Delegation depth enforcement | `delegation.rs` tests |
| Harness | Child tool scoping | `delegation.rs` tests |
| Harness | Ancestry propagation | `delegation.rs` tests |
| Roles | `From<TeamRole>` for AgentRole | `agent_protocol.rs` tests |
| Roles | `TryFrom<TaskRole>` for AgentRole | `agent_protocol.rs` tests |
| Integration | Full flow: register → delegate → execute → result | `tests/` integration |

### Estimated size
~300 LOC test code

---

## Slice 9 — Documentation

### Tasks
1. Add "implementation status" section to `UNIFIED_AGENT_ARCHITECTURE_REVISED.md`
2. Mark explicitly deferred items:
   - JSONL mailbox persistence
   - A2A / HTTP boundary
   - Dynamic swarms
   - Distributed state sync
3. Add deprecation note to `UNIFIED_AGENT_ARCHITECTURE.md` pointing to revised doc

---

## Recommended Execution Order

### Phase A — Enable communication substrate (S1 → S2 → S4)
1. **Slice 1** — Protocol narrowing (no dependencies)
2. **Slice 2** — MailboxRouter (depends on S1)
3. **Slice 4** — `send_message` wiring (depends on S2)

### Phase B — Enable target selection and execution (S3 → S5)
4. **Slice 3** — Registry extension (no dependencies, parallel with S2)
5. **Slice 5** — TaskDispatcher V2 (depends on S1, S2, S3)

### Phase C — Lock down safety (S6 → S7)
6. **Slice 6** — Harness enforcement (depends on S3, S5)
7. **Slice 7** — Role reconciliation (depends on S1, S3)

### Phase D — Finish strong
8. **Slice 8** — Tests
9. **Slice 9** — Documentation

---

## Suggested PR Shape

| PR | Slices | Risk | LOC |
|---|---|---|---|
| PR 1 | S1 + S2 (protocol + router) | Low | ~250 |
| PR 2 | S3 + S4 (registry + send_message) | Medium | ~200 |
| PR 3 | S5 (TaskDispatcher V2) | High | ~120 |
| PR 4 | S6 + S7 (harness + roles) | Medium | ~130 |
| PR 5 | S8 + S9 (tests + docs) | Low | ~300 |

---

## Definition of Done

1. Local agents can exchange directed messages through `MailboxRouter`
2. `send_message` performs real delivery via `MessageRouter` trait
3. Registry can answer "who can do X right now?" via capability lookup
4. `TaskDispatcher` executes through real `AgentSession` sessions
5. Delegation is bounded: max depth 3, scoped tools, inspectable ancestry
6. `AgentRole` is the canonical role anchor with conversions from `TeamRole`/`TaskRole`
7. Tests prove message-driven delegation flow works end-to-end
8. Existing orchestration tests pass without modification
