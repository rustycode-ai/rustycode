# TaskBrief: End-to-End Delegated-Agent Contract

> Status: Draft, implementation-ready
> Updated: 2026-05-14
> Scope: `rustycode-agent-runtime`, `rustycode-orchestration`, `rustycode-tui`

## Objective

Make delegated agents behave correctly from the **start of the request to the end of verification**, not just at nudge-generation time.

`ThoughtFrame` should become aware of the delegated task via a lightweight `TaskBrief`, but that is only one part of the fix. The complete lifecycle must also ensure that:

1. the delegated task is shaped correctly,
2. the right tools are exposed,
3. the runtime executes with the correct role context,
4. verification agents get verification-oriented guidance and tool access,
5. completion is judged against the delegated mission, not generic session progress.

This document replaces the earlier mid-pipeline-only design with a full end-to-end plan.

---

## Problem

Today, delegated agents receive:

- a role-specific system prompt from `TaskRole::system_prompt()`, and
- generic `ThoughtFrame` turn nudges derived from the current session.

They do **not** reliably receive a consistent delegated-agent contract across the whole execution path.

### Current failure modes

1. **Role drift**
   - Explorer/reviewer/planner agents can still see broad tool schemas in some paths.
   - The nudge does not remind them what role they are playing.

2. **Mission drift**
   - A delegated agent can lose sight of the original delegated task after several turns.

3. **Scope drift**
   - The agent can read files outside the assigned `path_scope` with no immediate feedback.

4. **Inconsistent execution surfaces**
   - `TaskDispatcher::execute_via_session()` and `tui::DelegationExecutor` are separate entry points.
   - They do not currently shape tools and ThoughtFrame context in the same way.

5. **Verification is under-specified**
   - The lifecycle from delegated request → execution → verification → done is not explicitly modeled in the current draft.

---

## Design principles

1. **One delegated-agent contract, used everywhere**
   - The same task-role, scope, and mission data should shape prompts, tool exposure, runtime context, and nudges.

2. **No new dependency edge from agent-runtime to orchestration**
   - `agent-runtime` must not import orchestration types directly.

3. **Behavior belongs at the session layer, not in `AgentConfig`**
   - `AgentConfig` remains mechanical limits plus existing toggles.
   - Delegation context lives on `AgentSession` / `ThoughtFrame`.

4. **Reuse existing primitives**
   - Use `ToolScope` instead of inventing `allowed_tools` / `denied_tools` vectors.
   - Use `TaskRole::allowed_tools()` as the canonical role-to-tool mapping.

5. **Nudge text is advisory, not the enforcement mechanism**
   - Real safety comes from filtered tool schemas and filtered registries.
   - The nudge keeps the model aligned and focused.

---

## What exists today

### Delegation and role types

| Type | Location | Purpose |
|---|---|---|
| `TaskSpec` | `crates/rustycode-orchestration/src/delegation.rs` | delegated task prompt, role, scope, tier, budget, steps |
| `TaskRole` | `crates/rustycode-orchestration/src/delegation.rs` | semantic delegated role + `allowed_tools()` + `system_prompt()` |
| `ExecutionTier` | `crates/rustycode-orchestration/src/types.rs` | model tier for orchestration |
| `ToolScope` | `crates/rustycode-protocol/src/tool_scope.rs` | allow/deny tool set with serialization |
| `AgentRole` | `crates/rustycode-protocol/src/agent_protocol.rs` | runtime/protocol role |

### Session/runtime types

| Type | Location | Purpose |
|---|---|---|
| `ThoughtFrame` | `crates/rustycode-agent-runtime/src/session.rs` | per-session working memory and turn-reflection generation |
| `AgentSession` | `crates/rustycode-agent-runtime/src/session.rs` | thin LLM↔tool loop |
| `AgentConfig` | `crates/rustycode-agent-runtime/src/session.rs` | mechanical runtime settings |
| `ToolContext` | `crates/rustycode-tools-api/src/lib.rs` | per-tool execution context including `role` and `plan_gate` |

### Real execution surfaces

#### A. Orchestration V2 session path

`TaskDispatcher::execute_via_session()`

```
TaskSpec
  -> task_spec_to_agent_config(spec)
  -> run_agent_session(..., spec.prompt, spec.role.system_prompt(), ...)
  -> AgentSession::run(...)
```

Current issues:

- `task_spec_to_agent_config()` only maps hard limits.
- `run_agent_session()` builds `tools_schema` from **all** tools in `tool_registry`.
- delegated role/scope/mission are not attached to `ThoughtFrame`.

#### B. TUI delegated sub-agent path

`crates/rustycode-tui/src/agents/delegation_executor.rs`

```
delegate_task tool call
  -> parse TaskRole + path_scope
  -> enrich prompt
  -> execute_delegated_task_inner(...)
  -> AgentSession::run(...)
```

Current issues:

- `build_subagent_tool_registry()` registers a broad fixed set.
- the passed `tools_schema` is cloned from the parent executor, not role-filtered.
- no `TaskBrief` / ThoughtFrame mission context is attached.

#### C. Tool execution path inside agent-runtime

`session.rs` -> `execute_tool(...)` -> `ToolContext::new(cwd)`

Current issue:

- `ToolContext.role` is left at the default `AgentRole::Coordinator`.
- no delegated role is carried into tool execution context.

---

## The right abstraction: `TaskBrief`

The working name changed: this plan uses **`TaskBrief`**.

That is the right name.

It describes the delegated mission in a compact, persistent, session-local form:

- who this agent is,
- what task it was spawned for,
- what area of the repo it should focus on,
- what tools it should see.

This is not the full orchestration `AgentContext`. It is the **session-facing snapshot** needed by the delegated runtime.

---

## Proposed data model

Create a new runtime-local type:

```rust
// crates/rustycode-agent-runtime/src/task_brief.rs

use rustycode_protocol::tool_scope::ToolScope;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BriefRole {
    Explorer,
    Researcher,
    Implementer,
    Reviewer,
    Verifier,
    Planner,
    Debugger,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBrief {
    /// Delegated semantic role.
    pub role: BriefRole,
    /// Original delegated task, truncated for nudge use if needed.
    pub brief: String,
    /// Repo paths the agent is expected to focus on.
    pub path_scope: Vec<PathBuf>,
    /// Tool policy for this delegated session.
    pub tool_scope: ToolScope,
}

impl TaskBrief {
    pub fn is_in_scope(&self, file_path: &Path) -> bool {
        if self.path_scope.is_empty() {
            return true;
        }

        self.path_scope.iter().any(|scope| file_path.starts_with(scope))
    }

    pub const fn role_hint(&self) -> &'static str {
        match self.role {
            BriefRole::Explorer => "Explorer: read and map the area. Do not edit.",
            BriefRole::Researcher => "Researcher: gather evidence and narrow uncertainty.",
            BriefRole::Implementer => "Implementer: make targeted changes and verify them.",
            BriefRole::Reviewer => "Reviewer: inspect and critique. Do not edit.",
            BriefRole::Verifier => "Verifier: run checks and prove correctness.",
            BriefRole::Planner => "Planner: analyze and produce a plan. Do not implement.",
            BriefRole::Debugger => "Debugger: find the root cause and apply a minimal fix.",
        }
    }
}
```

### Why this exact shape

| Field | Why it stays |
|---|---|
| `role` | needed for role-aware nudge phrasing |
| `brief` | needed for mission reminder |
| `path_scope` | needed for scope-drift detection |
| `tool_scope` | needed for end-to-end tool shaping and future runtime checks |

### What is intentionally excluded in v1

| Omitted | Reason |
|---|---|
| cost/budget fields | stale quickly; not actionable in the nudge; can be added later if runtime tracks spend |
| execution tier | tier already shapes model selection; not worth nudging on every turn |
| max_steps | already represented by `ThoughtFrame.turn/max_turns` |
| denied tools list | redundant once `ToolScope` exists |

---

## Where `TaskBrief` lives

**Put `TaskBrief` in `rustycode-agent-runtime`, not protocol.**

Reasoning:

- It is specifically for delegated-session runtime behavior.
- It exists to feed `ThoughtFrame::generate_nudge()` and related runtime shaping.
- Protocol should remain foundational and generic.

To avoid `agent-runtime -> orchestration` dependencies, define `BriefRole` locally and convert from `TaskRole` in orchestration/TUI code.

---

## End-to-end lifecycle

This is the actual lifecycle the implementation must satisfy.

### Phase 1: delegation request intake

Entry points:

1. `TaskDispatcher::execute_via_session()` path (orchestration)
2. `DelegationExecutor::execute_delegated_task_inner()` path (TUI)

At this phase we have:

- delegated prompt / task description,
- delegated role,
- optional `path_scope`.

### Phase 2: delegated task shaping

Construct `TaskBrief` from the delegated inputs.

Canonical shaping rules:

- `brief` comes from `TaskSpec.prompt` or the enriched delegation prompt.
- `role` comes from `TaskRole -> BriefRole` conversion.
- `path_scope` comes from `TaskSpec.path_scope` or TUI `path_scope`.
- `tool_scope` comes from `ToolScope::allow_only(TaskRole::allowed_tools())`.

This is the **single delegated-agent contract** used across the rest of the lifecycle.

### Phase 3: session construction

Add runtime surface for carrying the contract:

```rust
impl AgentSession {
    pub fn with_task_brief(mut self, brief: TaskBrief) -> Self;
}
```

`AgentConfig` remains unchanged except for existing `thinking_nudge`.

### Phase 4: tool exposure before the first turn

This is critical. The plan is incomplete unless the tool surface is shaped here.

#### 4A. Filter the LLM-visible tool schema

Today:

- `run_agent_session()` builds schema from `tool_registry.list()` with no role filtering.
- `DelegationExecutor` passes an inherited parent schema.

Required change:

- build a **role-scoped tool schema** using `TaskBrief.tool_scope.is_allowed(name)`.

#### 4B. Filter the executable tool registry

Schema filtering alone is not enough. If the model somehow emits a hidden tool name, execution should still be constrained.

Required change:

- create a filtered registry for delegated sessions using the same `TaskBrief.tool_scope`.
- do this in both execution surfaces:
  - orchestration `run_agent_session()` path,
  - TUI `DelegationExecutor` path.

This keeps **what the model sees** and **what can actually execute** aligned.

### Phase 5: session startup and ThoughtFrame hydration

When `AgentSession::run()` loads or creates `ThoughtFrame`, attach the brief:

```rust
if let Some(brief) = self.task_brief.clone() {
    thought_frame.task_brief = Some(brief);
}
```

This should happen **after** loading from disk so the caller-supplied delegated brief wins.

### Phase 6: turn loop and nudge generation

`ThoughtFrame::generate_nudge()` keeps its current phase/stuck/read-tracking logic.

Then, if a `TaskBrief` exists, append a compact delegated-agent supplement:

1. **Role hint** (turns 1-2 only)
2. **Scope drift warning** (only when violated)
3. **Mission reminder** (every 5 turns, truncated)

Example:

```text
Explorer: read and map the area. Do not edit.
SCOPE DRIFT: src/payments/mod.rs — focus on your assigned scope.
Task: "Investigate auth refresh handling in src/auth and summarize findings"
```

### Phase 7: tool execution context

When tools actually run, `ToolContext` should no longer default silently to `AgentRole::Coordinator` for delegated sessions.

Required change:

- extend `execute_tool(...)` to accept optional delegated runtime role data,
- set `ToolContext::with_role(...)` when a delegated brief exists,
- optionally add a role-aware plan gate later; the immediate strong guarantee remains the filtered registry.

This makes runtime context truthful and prepares future enforcement.

### Phase 8: verification lifecycle

This feature must explicitly support verification, not just implementation.

For `TaskRole::Verify`:

- tool scope must include the verify/debug tool set (`Read`, `Grep`, `ListDir`, `Glob`, `Bash`),
- role hint should bias toward proving correctness,
- completion is not “I made a change”, it is “I produced evidence”.

The runtime already ends when no more tool calls are made. The plan must require that verification agents are shaped to:

1. run the relevant checks,
2. capture the output in tool results,
3. finish with evidence-oriented final text.

### Phase 9: completion and persistence

At session end:

- `ThoughtFrame` persists as today if `thought_frame_path` is configured,
- `task_brief` persists with it,
- resulting delegated transcript contains both mission-aligned tool use and mission-aligned final text.

---

## Concrete implementation plan

### Step 1 — Introduce `TaskBrief`

**Files**

- `crates/rustycode-agent-runtime/src/task_brief.rs` (new)
- `crates/rustycode-agent-runtime/src/lib.rs`

**Changes**

- add `BriefRole`
- add `TaskBrief`
- export both from `lib.rs`

### Step 2 — Attach `TaskBrief` to `ThoughtFrame`

**File**

- `crates/rustycode-agent-runtime/src/session.rs`

**Changes**

- add `task_brief: Option<TaskBrief>` to `ThoughtFrame`
- initialize to `None` in `ThoughtFrame::new`
- add `append_task_brief_nudge(&self, brief, lines)`
- call it from `generate_nudge()`

### Step 3 — Add `AgentSession::with_task_brief()`

**File**

- `crates/rustycode-agent-runtime/src/session.rs`

**Changes**

- add `task_brief: Option<TaskBrief>` field to `AgentSession`
- add builder method
- apply caller-provided brief to loaded/new `ThoughtFrame` before entering `run_loop`

### Step 4 — Shape delegated sessions in orchestration V2 path

**File**

- `crates/rustycode-orchestration/src/task_dispatcher.rs`

**Changes**

- add `From<TaskRole> for BriefRole`
- add helper `task_spec_to_task_brief(spec: &TaskSpec) -> TaskBrief`
- in `execute_via_session()`, build `TaskBrief`
- pass it into `run_agent_session(...)`
- inside `run_agent_session(...)`, create session with `.with_task_brief(task_brief)`
- set `thinking_nudge: true` for delegated sessions

### Step 5 — Filter tool schema in orchestration V2 path

**File**

- `crates/rustycode-orchestration/src/task_dispatcher.rs`

**Changes**

- build schema from filtered tool infos instead of all infos
- use `build_canonical_tool_schemas(&filtered_infos)`
- `filtered_infos` comes from `tool_registry.list().into_iter().filter(|t| brief.tool_scope.is_allowed(&t.name))`

### Step 6 — Filter executable registry in orchestration V2 path

**Files**

- `crates/rustycode-orchestration/src/task_dispatcher.rs`
- possibly a small helper near runtime setup

**Changes**

- create a delegated-session registry from the parent registry containing only allowed tools
- pass that filtered registry to `session.run(...)`

### Step 7 — Shape delegated sessions in TUI executor path

**File**

- `crates/rustycode-tui/src/agents/delegation_executor.rs`

**Changes**

- change `execute_delegated_task_inner(...)` to accept role + path scope or a prebuilt `TaskBrief`
- build `TaskBrief` from `task_role`, `prompt`, and `path_scope`
- call `.with_task_brief(task_brief)` on `AgentSession`
- replace inherited `self.tools_schema` with a role-scoped schema
- replace the broad subagent registry with a role-scoped registry

### Step 8 — Make execution-time `ToolContext` truthful

**Files**

- `crates/rustycode-agent-runtime/src/tool_exec.rs`
- `crates/rustycode-agent-runtime/src/session.rs`

**Changes**

- extend `execute_tool(...)` to accept optional delegated role information
- apply `ToolContext::with_role(...)` when available
- keep this as a correctness improvement even if the main enforcement is registry filtering

### Step 9 — Verification-specific tests

Add tests for the full lifecycle, not only the new struct.

---

## Verification plan

### Unit tests

#### `task_brief.rs`

- `brief_role_conversion_covers_all_task_roles`
- `task_brief_is_in_scope_with_empty_scope`
- `task_brief_is_in_scope_uses_path_prefix_not_string_prefix`
- `task_brief_serializes_round_trip`

#### `session.rs` / ThoughtFrame

- `task_brief_role_hint_only_appears_early`
- `task_brief_scope_drift_warning_appears_when_explored_outside_scope`
- `task_brief_mission_reminder_is_truncated`
- `task_brief_nudge_stays_under_budget`

### Integration tests

#### orchestration V2 path

- delegated `Explore` task only sees read-only tools in schema
- delegated `Code` task sees write tools
- delegated `Verify` task sees bash but not write tools
- delegated session gets `thinking_nudge = true`
- delegated session receives `TaskBrief` and emits role-aware nudge text

#### TUI `DelegationExecutor` path

- `execute_delegated_task_inner` builds role-scoped registry/schema
- delegated prompt + path scope produce the expected `TaskBrief`
- reviewer/planner roles cannot execute write tools through the delegated registry

#### execution context truthfulness

- delegated tool execution sets `ToolContext.role` to the delegated role mapping instead of `Coordinator`

### Manual verification

1. Spawn an **explore** subagent on `src/auth`
   - confirm schema only contains read-only tools
   - confirm early turns include explorer role hint
   - confirm reading `src/payments/...` triggers scope drift

2. Spawn a **code** subagent on `src/auth`
   - confirm write/edit tools are available
   - confirm role hint pushes toward targeted changes and verification

3. Spawn a **verify** subagent after a code change
   - confirm bash is available
   - confirm no write/edit tools are exposed
   - confirm final answer contains evidence from executed checks

4. Confirm persisted ThoughtFrame JSON still loads when `task_brief` is absent.

---

## Rollout plan

### PR 1 — delegated mission context

- add `TaskBrief`
- attach to `ThoughtFrame`
- add `AgentSession::with_task_brief()`
- wire into orchestration V2 path and TUI delegation path
- add role/scope/mission nudge supplement

### PR 2 — delegated tool shaping

- filter schemas by `TaskBrief.tool_scope`
- filter executable registries by `TaskBrief.tool_scope`
- add tests for explore/code/verify role exposure

### PR 3 — execution-context correctness

- pass delegated role into `ToolContext`
- prepare for future plan-gate integration if needed

This split keeps the work reviewable and makes it easy to bisect regressions.

---

## Resolved decisions

1. **Name**: `TaskBrief`
2. **Location**: `rustycode-agent-runtime`
3. **Attach point**: `AgentSession` + `ThoughtFrame`, not `AgentConfig`
4. **Tool policy type**: reuse `ToolScope`
5. **Wire-up mode**: prefer explicit `AgentSession::with_task_brief()` over orchestration writing ThoughtFrame JSON directly
6. **Nudge scope**: role + scope drift + mission reminder only
7. **Verification**: included explicitly in the lifecycle and tests

---

## Non-goals for this work

- moving `TaskRole` into protocol
- redesigning orchestration budgets or cost accounting
- merging all tool-gating systems in the repo
- extracting `ThoughtFrame` into its own module as part of the same change
- event-bus-based dynamic updates to the brief

---

## Future follow-ups

- connect `structured_thinking` output to `ThoughtFrame.hypothesis`
- bridge orchestration stuck signals into runtime nudges
- add richer completion contracts per delegated role
- consider extracting `ThoughtFrame` from `session.rs` once this feature lands cleanly
