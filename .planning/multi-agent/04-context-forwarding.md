# 04 — Context Forwarding & Shared State ✅

Context forwarding defines how information flows between levels of the agent hierarchy.
This is the critical integration surface — without it, agents are isolated silos.

---

## Design Principles

1. **Inbound/Outbound symmetry** — what an agent receives (`AgentContext`) and what it produces
   (`AgentOutcome`) form a matched pair at every hierarchy level.

2. **Reasoning carries forward** — each `AgentOutcome` includes a `ReasoningSummary` (condensed
   graph) so parent agents and teams can incorporate child reasoning without full graph merging.

3. **Workspace is the shared surface** — files, plans, and notes live in `SharedWorkspace`.
   Agents read/write through workspace API; the orchestrator handles conflict resolution.

4. **Budget cascades** — budget allocations are subdivided at each level via `CostBudget::subdivide()`.
   A team's budget is split among its agents; an agent's budget is split among sub-agents.

---

## AgentContext (Inbound) ✅

What an agent receives to start work. Constructed by the parent (orchestrator or team lead).

*`crates/rustycode-orchestration/src/agent_context.rs`*

```rust
pub struct AgentContext {
    pub task_id: String,                               // unique task identifier for tracing
    pub session_id: String,                            // session this agent belongs to
    pub agent_role: AgentRole,                         // Architect, Builder, Skeptic, etc.
    pub tool_scope: ToolScope,                         // scoped tool set
    pub budget: CostBudget,                            // token/cost ceiling
    pub conversation_history: Vec<Message>,            // prior conversation to carry forward
    pub files_in_scope: Vec<FileSnippet>,              // relevant code snippets
    pub reasoning_from_parent: Option<ReasoningSummary>, // condensed parent graph (if sub-agent)
}
```

### Supporting types

- **`AgentRole`** — `crates/rustycode-protocol/src/agent_protocol.rs` — enum with variants:
  `Architect`, `Builder`, `Skeptic`, `Judge`, `Scalpel`, `Coordinator`, `Planner`, `Worker`,
  `Reviewer`, `Researcher` (`#[non_exhaustive]`).

- **`ToolScope`** — `crates/rustycode-protocol/src/tool_scope.rs` — allow/deny `HashSet<String>`
  with `full()`, `allow_only()`, `deny_only()`, `restrict()` (creates sub-scope), and `is_allowed()`.
  Denied always takes precedence over allowed.

- **`CostBudget`** — `crates/rustycode-protocol/src/cost_budget.rs` — tracks `max_tokens`,
  `max_cost_usd`, `tokens_used`, `cost_used_usd`. Key methods: `record()`, `subdivide(fraction)`,
  `is_exhausted()`, `remaining_tokens()`.

- **`FileSnippet`** — `crates/rustycode-protocol/src/agent_protocol.rs` — `{ path: String,
  content: String, line_range: Option<(usize, usize)> }`.

### Construction

| Hierarchy Level | Who constructs | Key differences |
|----------------|---------------|-----------------|
| Top-level | CLI/TUI or `AgentSessionExecutor` | Full budget, all tools, no parent reasoning |
| Sub-agent | Parent agent | Scoped tools, subset of budget, parent reasoning |
| Team agent | `TeamOrchestrator` | Role-specific tools, plan step, team workspace |
| Escalated tier | `StepOrchestrator` | Previous tier's assessment + code snippets |

---

## AgentOutcome (Outbound) ✅

What an agent produces. Defined in protocol and re-exported by orchestration.

*`crates/rustycode-protocol/src/agent_outcome.rs`*

```rust
pub struct AgentOutcome {
    pub agent_id: String,                              // which agent produced this
    pub task_id: String,                               // task this outcome belongs to
    pub success: bool,                                 // agent considers work successful
    pub output_text: String,                           // final text output
    pub files_changed: Vec<FileChange>,                // paths + diffs
    pub usage: TokenUsage,                             // cumulative token usage
    pub reasoning_summary: ReasoningSummary,           // condensed reasoning for parent
}
```

### Construction from AgentResult

*`crates/rustycode-orchestration/src/agent_outcome.rs`*

The free function `agent_outcome_from_result()` converts an `AgentResult` (from agent-runtime)
into an `AgentOutcome`. It maps `StoppedReason::MaxTurnsReached` and `StoppedReason::TimeoutExceeded`
to `success: false`, and copies token counts from the result.

### TokenUsage

*`crates/rustycode-protocol/src/token_usage.rs`*

```rust
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}
```

Key methods: `total()` (input+output, saturating), `effective_input()` (input minus cache read),
`saturating_add()` (never overflows), `Sum` trait for `Iterator::sum()`. Derives `Copy`.

### FileChange

Two definitions exist for different contexts:

- **`agent_protocol::FileChange`** — `crates/rustycode-protocol/src/agent_protocol.rs` — used in
  `AgentOutcome` and team contexts. Fields: `path`, `summary`, `diff_hunk`, `lines_added`,
  `lines_removed`.

- **`file_context::FileChange`** — `crates/rustycode-protocol/src/file_context.rs` — lightweight
  variant. Fields: `file_path`, `change_type` (Added/Modified/Deleted enum), `snippet: Option<FileSnippet>`.

---

## ReasoningSummary ✅

A lightweight representation of an agent's `ReasoningGraph` for upward communication.

*`crates/rustycode-protocol/src/reasoning_summary.rs`*

```rust
pub struct ReasoningSummary {
    pub thought_count: usize,
    pub max_confidence: f64,
    pub mean_confidence: f64,
    pub top_insights: Vec<Insight>,                // top-10 by confidence
    pub strategy_used: String,
    pub convergence_achieved: bool,
}

pub struct Insight {
    pub content: String,
    pub confidence: f64,                           // clamped [0.0, 1.0]
    pub strategy: String,
    pub depth: usize,
}
```

Key methods: `empty()`, `from_parts()`, `merge()` (combines two summaries with weighted mean,
truncates insights to top-10, ANDs convergence).

---

## HandoffPackage (Escalation) ✅

*`crates/rustycode-orchestration/src/handoff.rs`*

Carries context between escalation tiers. Each tier starts fresh with only what it needs
— no full conversation history is included.

```rust
pub struct HandoffPackage {
    pub task_description: String,
    pub target_tier: ExecutionTier,
    pub source_tier: ExecutionTier,
    pub code_snippets: Vec<CodeSnippet>,
    pub constraints: Vec<String>,
    pub previous_assessment: Option<String>,
    pub budget_summary: Option<BudgetSummary>,
    pub task_id: String,
    pub reasoning_summary: Option<ReasoningSummary>,  // cross-tier reasoning carry-forward
}
```

### Supporting types

- **`CodeSnippet`** — `{ file_path: String, content: String, relevance: String }`
- **`BudgetSummary`** — `{ tier: u8, tokens_used: u64, tokens_limit: u64, cost_usd_used: f64,
  cost_usd_limit: f64 }`. Methods: `tokens_remaining()`, `budget_remaining()`.
- **`HandoffBuilder`** — fluent builder API for constructing `HandoffPackage`.

---

## TeamContext (Aggregation) ✅

What a team produces for ensemble consumption.

*`crates/rustycode-team/src/team_context.rs`*

```rust
pub struct TeamContext {
    pub team_id: String,
    pub task_id: String,
    pub agent_outcomes: Vec<AgentOutcome>,           // outcomes from each agent
    pub convergence: ConvergenceView,                 // aggregated confidence
    pub combined_changes: Vec<FileChange>,            // deduplicated file changes
    pub total_usage: TokenUsage,                      // total tokens/cost
}
```

### ConvergenceView

*`crates/rustycode-team/src/convergence.rs`*

Lightweight team-level aggregation of agent reasoning:

```rust
pub struct ConvergenceView {
    pub team_count: usize,                            // number of contributing teams
    pub max_confidence: f64,
    pub mean_confidence: f64,
    pub top_insights: Vec<Insight>,                   // merged from all agents
    pub dissenting_opinions: Vec<DissentingOpinion>,
    pub convergence_achieved: bool,
}

pub struct DissentingOpinion {
    pub agent_id: String,
    pub team_id: String,
    pub opinion: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
}
```

---

## SharedWorkspace ✅

*`crates/rustycode-orchestration/src/shared_workspace.rs`*

In-memory coordination surface backed by `Arc<Mutex<HashMap<String, WorkspaceEntry>>>`.

### WorkspaceEntry

```rust
pub struct WorkspaceEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub written_by: String,
    pub step_id: Option<String>,
    pub timestamp: chrono::DateTime<Utc>,
}
```

### API

| Method | Description |
|--------|-------------|
| `write(key, value, written_by, step_id)` | Insert or overwrite entry |
| `read(key)` | Get full entry |
| `read_value(key)` | Get value only |
| `contains(key)` | Check existence |
| `remove(key)` | Remove and return entry |
| `keys()` | List all keys |
| `len()` / `is_empty()` | Size queries |
| `clear()` | Remove all entries |
| `snapshot()` | Clone entire HashMap |
| `with_lock(f)` | Atomic compound operation under single lock |

### Properties

- **In-memory** — backed by `Arc<Mutex<HashMap>>`, not filesystem
- **Provenance tracking** — records which agent wrote which entry (`written_by`)
- **Step association** — entries can be associated with a step via `step_id`
- **Thread-safe** — `Arc<Mutex>` allows sharing across async tasks

---

## LifecyclePlugin (Onboarding/Offboarding) ✅

*`crates/rustycode-agent-runtime/src/plugins/lifecycle.rs`*

Captures provider context and reasoning summaries so they can be forwarded to subsequent
agents or persisted across compaction.

```rust
pub struct LifecyclePlugin {
    pub provider_context: Option<ProviderContext>,
    pub handoff_summary: Option<ReasoningSummary>,
    // turns_completed: usize (private, incremented on on_done)
}
```

On agent completion, `into_offboarding_result()` produces an `OffboardingResult` with
`provider_context`, `reasoning_summary`, and `turns_completed`.

### ProviderContext

*`crates/rustycode-agent-runtime/src/provider_context.rs`*

```rust
pub struct ProviderContext {
    pub provider_name: String,                        // "anthropic", "openai", etc.
    pub model: String,
    pub auth_key: String,
    pub rate_limit_settings: RateLimitSettings,       // { rpm: u64, tpm: u64 }
}
```

---

## Handoff Across Compaction ✅

*`crates/rustycode-session/src/compaction.rs`*

`CompactionSnapshot` preserves agent handoff data across session compaction:

```rust
pub struct CompactionSnapshot {
    pub session_id: String,
    pub snapshot_at: chrono::DateTime<Utc>,
    pub current_task: Option<String>,
    pub active_files: HashMap<String, String>,
    pub pending_changes_summary: Option<String>,
    pub pending_tool_call: Option<String>,
    pub token_count: usize,
    pub custom_state: HashMap<String, String>,
    pub handoff_summaries: Vec<HandoffSummary>,       // survives compaction
}
```

### HandoffSummary

```rust
pub struct HandoffSummary {
    pub from_agent: String,
    pub summary: String,
    pub success: bool,
}
```

`handoff_summaries` uses `#[serde(default)]` for backward compatibility — old snapshots
without this field deserialize with an empty vec.

---

## Flow Diagram

```
Top-level dispatch:
  CLI/TUI creates AgentContext (full budget, all tools)
    → AgentSession.run()
    → AgentOutcome (output_text, files_changed, reasoning_summary)

Sub-agent dispatch:
  Parent creates AgentContext (scoped tools, subset budget, reasoning_from_parent)
    → AgentSession.run()
    → AgentOutcome reported back to parent
    → Parent merges ReasoningSummary into its ConvergenceView

Team dispatch:
  TeamOrchestrator creates AgentContext per role
    → Builder runs → Skeptic reviews → Judge approves
    → Each produces AgentOutcome
    → Coordinator aggregates into TeamContext

Escalation:
  StepOrchestrator creates HandoffPackage (assessment, code, reasoning)
    → Next tier receives AgentContext derived from HandoffPackage
    → Produces AgentOutcome at higher capability

Ensemble:
  Multiple teams produce TeamContext
    → Shared ConvergenceView across teams
    → Consensus via simple majority, weighted confidence, or unanimous

Compaction:
  CompactionSnapshot preserves HandoffSummary entries
    → Survives token-reduction compaction
    → Restored after compaction completes
```
