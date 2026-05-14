# 07 — Session Persistence

Session persistence is separate from agent execution. Sessions store message history and
metadata; agents are stateless execution engines that receive messages at call time.

---

## Session Architecture

| Component | Crate | Responsibility |
|-----------|-------|----------------|
| `Session` | rustycode-session | Immutable log of messages and project metadata |
| `AgentSession` | rustycode-agent-runtime | Live execution engine (no persistent state) |
| `SessionManager` | rustycode-session | Serialization, atomic writes |
| `CompactionEngine` | rustycode-session | Prunes history, preserves WIP |

---

## Session (Data)

*`crates/rustycode-session/src/session.rs`*

The immutable append-only log. No running-agent state lives here.

```rust
pub struct Session {
    pub id: SessionId,                  // "sess_" prefixed, path-traversal protected
    pub name: String,
    pub messages: Vec<Message>,
    pub metadata: SessionMetadata,
    pub context: SessionContext,
    pub status: SessionStatus,          // Active | Archived | Deleted
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

pub struct SessionMetadata {
    pub project_path: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub model_used: Option<String>,
    pub provider_used: Option<String>,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub tags: Vec<String>,
    pub custom: HashMap<String, String>,
}

pub struct SessionContext {
    pub task: Option<String>,
    pub files_touched: Vec<String>,
    pub decisions: Vec<String>,
    pub errors_resolved: Vec<String>,
    pub current_phase: Option<String>,
}
```

---

## Session Type Duality

Two session types coexist, serving different scopes:

| Type | Crate | Purpose |
|------|-------|---------|
| `rustycode_session::Session` | rustycode-session | Full persistent session with message history, metadata, context |
| `rustycode_protocol::Session` | rustycode-protocol | Lean cross-crate type with builder pattern and `is_terminal()` status |

The protocol type is what crates exchange. The session type is what gets persisted to disk.
`SessionManager` stores `rustycode_protocol::Session` internally.

---

## SessionManager

*`crates/rustycode-session/src/session_manager.rs`*

Handles serialization with atomic writes (temp file + rename) to prevent partial writes.

```rust
pub struct SessionManager { storage_dir: PathBuf }
```

Methods: `save_session`, `load_session`, `list_sessions` (newest-first), `fork_session`,
`delete_session`, `cleanup_old_sessions`, `stats() -> SessionStats`.

---

## Compaction

*`crates/rustycode-session/src/compaction.rs`*

Long-running sessions accumulate messages that exceed LLM context windows. Compaction prunes
history while preserving work-in-progress state.

### CompactionSnapshot (Current)

```rust
pub struct CompactionSnapshot {
    pub session_id: String,
    pub snapshot_at: DateTime<Utc>,
    pub current_task: Option<String>,
    pub active_files: HashMap<String, String>,  // path → content hash
    pub pending_changes_summary: Option<String>,
    pub pending_tool_call: Option<String>,
    pub token_count: usize,
    pub custom_state: HashMap<String, String>,
}
```

Saved to `sessions/{session_id}/compaction-snapshot.json` via atomic temp file + rename.

### Compaction Strategies

```rust
pub enum CompactionStrategy {
    TokenThreshold { target_ratio, min_messages },
    MessageAge { max_age, keep_recent },
    SemanticImportance { importance_threshold, min_messages },
    Custom(Arc<CompactionFn>),
}
```

`CompactionEngine::compact()` returns a `CompactionReport` with before/after counts and
reduction percentages.

---

## Agent Lifecycle Hooks

### Current State

Lifecycle hooks exist in `rustycode-agents` (agent trait):

```rust
// crates/rustycode-agents/src/agent.rs
async fn on_boarding(&self, _context: &str) -> Result<()>;
async fn on_offboarding(&self) -> Result<String>;
```

These are defined on the `Agent` trait, separate from the `AgentPlugin` system in
`rustycode-agent-runtime`.

### Planned: Unified via AgentPlugin

The goal is to integrate onboarding/offboarding into the `AgentPlugin` system so that:
- **Onboarding** loads the relevant session history slice into the agent
- **Offboarding** serializes a handoff summary back to the session

This is tracked in the implementation plan (Phase 2, Wave 5, tasks 2.16-2.18).

---

## Session Key Methods

| Method | Purpose |
|--------|---------|
| `add_message(msg)` | Append to history |
| `touch_file(path)` | Track modified files |
| `record_decision(d)` | Persist architectural decisions |
| `pre_compact() -> CompactionSnapshot` | Snapshot WIP before pruning |
| `post_compact(&snapshot)` | Restore WIP after pruning |
| `fork() -> Session` | Branch a new session from current history |

---

## State Boundary Summary

| What | Type | Persistent? | Crosses tiers? | Crosses agents? |
|------|------|-------------|----------------|-----------------|
| Message history | `Session.messages` | Yes (disk) | No (per-session) | No |
| WIP during compaction | `CompactionSnapshot` | Yes (disk, atomic) | No | No |
| Task execution state | `TaskContext` | Partial (serde skip) | Yes (via escalate) | No |
| Cross-tier transfer | `HandoffPackage` | No | Yes (explicit) | No |
| Cross-agent transfer | `AgentOutcome` | No | No | Yes (parent-child) |
| Team aggregation | `TeamContext` | No | No | Yes (team-ensemble) |
| Shared files | `SharedWorkspace` | Yes (filesystem) | Yes | Yes |
| Agent catalog | `AgentRegistry` | No (OnceLock) | Yes | Yes |
| Reasoning per agent | `ReasoningGraph` | Yes (serialized) | No | No (local) |
| Reasoning aggregated | `ConvergenceView` | No | Yes | Yes |
