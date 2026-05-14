# 08 — Cross-Cutting Concerns ✅

Concerns that span all levels of the agent hierarchy.

---

## Provider / Model Per Agent ✅

Providers and models are **not stored on the agent** — they are injected at call time via
`AgentSession::run(provider, model, ...)`. This enables polyglot orchestration: the same
`AgentSession` instance can be driven with different models on successive turns.

Provider configuration is carried in `ProviderContext` (`crates/rustycode-agent-runtime/src/provider_context.rs`):

```rust
pub struct ProviderContext {
    pub provider_name: String,       // "anthropic", "openai", "bedrock", etc.
    pub model: String,               // model identifier
    pub auth_key: String,            // redacted in logs
    pub rate_limit_settings: RateLimitSettings,  // { rpm: u64, tpm: u64 }
}
```

The `LifecyclePlugin` captures `ProviderContext` during onboarding and exposes it via
`OffboardingResult` for handoff to subsequent agents.

---

## Doom Loop Prevention ✅

Two independent mechanisms at different hierarchy levels:

| Mechanism | Owner | Trigger | Location |
|-----------|-------|---------|----------|
| `DoomLoopDetector` | Tool execution layer | Repeated identical tool calls (same name + args hash) | `crates/rustycode-tools/src/doom_loop.rs` |
| `ApproachFingerprint` counter in `Coordinator` | Team layer | Same high-level approach attempted multiple times | `crates/rustycode-team/src/coordinator.rs`, type defined in `crates/rustycode-protocol/src/team.rs` |

Both mechanisms independently detect and stop unproductive loops. Neither depends on the other.

### DoomLoopDetector

Tracks a sliding window of recent tool calls (default: 50 entries, 2-minute window).

- **Warning threshold**: 3 identical tool+args calls → `DoomLoopStatus::Warning`
- **Abort threshold**: 5 identical tool+args calls → `DoomLoopStatus::Abort`
- Key API: `record(tool_name, args)` → `DoomLoopStatus`, `would_warn(tool_name, args)`, `reset()`
- Uses `FxHasher` for argument hashing; entries older than the window are pruned automatically.

---

## Event Messaging Architecture ✅

### EventMsg (Outbound)

`AgentSession` emits `EventMsg` on a broadcast channel (capacity 256). Subscribers that lag
receive a `Lagged` notification rather than blocking the agent.

### Op (Inbound)

An `mpsc::UnboundedReceiver<Op>` accepts commands to control the running agent. This enables
external steering: stop streams, change behavior mid-turn, inject messages.

### Dual Emission (Phase 1B)

The agent runtime supports dual emission: events go to both the broadcast channel AND the
`AgentEvents` trait implementation. This ensures both bus subscribers and direct consumers
receive events.

### Team Events

33+ `TeamEvent` types for team-level coordination. Emitted via `broadcast::Sender<TeamEvent>`.

---

## Error Hardening ✅

28+ hardening fixes applied across all architecture crates:

| Pattern | Fix |
|---------|-----|
| `.expect()` panics | Replaced with `?` or `unwrap_or_else(\|e\| e.into_inner())` |
| `.unwrap()` on mutex | `into_inner()` recovery for poisoned mutexes |
| UTF-8 panics in truncation | `floor_char_boundary()` for safe char-boundary alignment |
| Integer overflow | `saturating_add()` for counters, `.max(0)` before `as u64` |
| Division by zero | Empty collection guards before division |
| Signal handler panics | Graceful error handling in signal handlers |
| Clock skew | `.max(0)` clamp on chrono durations |

### Mutex Recovery Pattern

```rust
// Instead of .lock().unwrap():
let guard = mutex.lock().unwrap_or_else(|e| e.into_inner());
```

This recovers from poisoned mutexes (where a previous holder panicked) by accessing the
underlying data despite the poison.

---

## Plugin System ✅

`AgentPlugin` trait allows observers/mutators at every agent lifecycle hook point. Plugins
are stored as `Vec<Box<dyn AgentPlugin>>` — empty vec means zero overhead.

Built-in plugins:
- **LifecyclePlugin** — captures provider context and reasoning summaries for handoff/compaction
  (`crates/rustycode-agent-runtime/src/plugins/lifecycle.rs`)
- **EarlyStopPolicy** — configurable stop conditions
- **RepetitionDetector** — detects repeated tool calls with identical inputs

---

## Budget Enforcement ✅

`CostBudget` (`crates/rustycode-protocol/src/cost_budget.rs`) tracks token and cost usage per task.
Budget cascades from team to agent to sub-agent via `subdivide(fraction)`:

```
Team budget: $2.00 / 500K tokens
  → Agent 1: $0.80 / 200K tokens (subdivide(0.4))
  → Agent 2: $0.80 / 200K tokens (subdivide(0.4))
  → Reserve: $0.40 / 100K tokens
```

When budget is exhausted (`is_exhausted()` returns true), the current tier escalates or the
task fails. `record(tokens, cost_usd)` returns false if the call would exceed limits (atomic
check-and-update).

### Budget in Handoff

`BudgetSummary` (`crates/rustycode-orchestration/src/handoff.rs`) carries budget state across
tiers: `{ tier, tokens_used, tokens_limit, cost_usd_used, cost_usd_limit }`.

---

## Tool System ✅

### Tool Activation

`ToolActivationManager` gates which tools are visible to an agent. Tools are organized by
tier — higher tiers get access to more powerful tools.

### Tool Scope ✅

`ToolScope` (`crates/rustycode-protocol/src/tool_scope.rs`) controls which tools an agent
has access to. Sub-agents receive scoped tool sets via `restrict()`, which intersects the
requested tool set with the parent's allowed set while propagating denied tools.

```rust
let parent = ToolScope::deny_only(["bash"]);
let child = parent.restrict(["read", "bash", "edit"]);
// child: read=allowed, bash=denied (inherited), edit=allowed
```

### Tool Deferral

Tools can be deferred (made available later) via the tool deferral system, enabling
progressive capability unlocking as agents prove reliability.

---

## State Boundaries ✅

### CompactionSnapshot

`CompactionSnapshot` (`crates/rustycode-session/src/compaction.rs`) preserves WIP state
across session compaction. It is atomically saved to disk (temp file + rename) before
compaction and restored after.

Key fields that survive compaction:
- `current_task` — what the agent was working on
- `active_files` — file paths and their state
- `pending_changes_summary` — unsaved work description
- `pending_tool_call` — in-flight tool call
- `handoff_summaries` — agent handoff data (`HandoffSummary: { from_agent, summary, success }`)

The `handoff_summaries` field uses `#[serde(default)]` for backward compatibility with
pre-multi-agent snapshots.

---

## Lock Acquisition Order

In `StepOrchestrator`, locks must be acquired in this order to prevent deadlocks:

1. `isolation` (RwLock)
2. `activation` (RwLock)
3. `budget_enforcer` (RwLock)

Violating this order anywhere in the codebase is a deadlock risk.
