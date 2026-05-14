# ADR 004: Tool Interface Redesign — Two-Tier Context to Ports-and-Adapters

- Status: Accepted
- Date: 2026-05-14
- Last Updated: 2026-05-14
- Implementation Started: Mid-2026
- Open Questions Resolved: 2026-05-14

## Context

The tool layer has grown organically into a maintenance burden:

1. **`rustycode-tools-api` is a god crate** (4,500 LOC) mixing trait definitions, runtime infrastructure (`ToolContext`, `ToolRegistry`), utility modules, and macros.
2. **`ToolContext` is a god struct** (16 fields) passed to every tool, but most tools only use 3-5 fields (`cwd`, `role`, `plan_gate`, `cancellation_token`).
3. **`define_tool!` has 90+ invocations** — any interface change must be backward-compatible or incremental.
4. **Four distinct execution paths** exist across the codebase (CLI/TUI, LLM bridge via shim, ACP/IDE integration, Orchestration task dispatch), with inconsistent tool resolution and context passing patterns.
5. **Duplicate implementations** of `TokenCounter` across crates (now consolidated in `rustycode-tool-integration`).
6. **Glob re-exports** in `providers/mod.rs` (42 `pub use module::*`) flatten namespace.
7. **Naming collision:** Two unrelated types are named `ToolExecutor` — a concrete struct in `rustycode-tools/src/executor/executor.rs` and an async trait in `rustycode-orchestration/src/musician.rs`.
8. **Duplicate `ToolInfo` types:** Rich metadata type in `rustycode-tools-api` vs. stripped protocol type in `rustycode-tool-integration`. Mapping is manual in `executor.rs`.

### Execution Flow (5 Layers)

Tool calls traverse five layers from LLM response to execution:

```
LLM ToolCall
    │
    ▼
ToolRegistry::execute()         ← Name resolution, dispatch, audit logging
    │
    ▼
ExecutionMiddleware::execute()  ← PreToolUse hooks → validate_input → plan_mode → cost check
    │                              → EXECUTE → PostToolUse hooks → cost recording
    ▼
ToolInspectionManager::check()  ← Inspector pipeline: Security → Egress → OSV → Repetition → Permission
    │                              (each returns Allow/Deny/RequireApproval)
    ▼
ConvoyDispatcher::execute_guarded()  ← Role-based gating (ToolGate trait)
    │
    ▼
Tool::execute(params, ctx)     ← Actual tool implementation
```

Three wrapper/middleware patterns already exist in the tool stack:
1. **ExecutionMiddleware** — wraps any `Tool` with hooks, validation, plan-mode checks, cost tracking
2. **ToolInspectionManager** — pre-execution pipeline of `ToolInspector` trait objects (security, egress, OSV, repetition)
3. **ConvoyDispatcher** — wraps execution with `ToolGate` role-based access

### Orchestration Interface Contract

The orchestration layer (`rustycode-orchestration`) never calls `Tool::execute` directly. It uses a two-layer dispatch:

```
Orchestration (Musician)
    │  calls TaskToolExecutor::execute(task_id, tool_name, input, allowed_tools, model)
    ▼
AgentSessionExecutor (production) or ShellTaskToolExecutor (simple) or ExecutableToolExecutor (external)
    │  internally calls ToolRegistry::execute(ToolCall, ToolContext)
    ▼
Tool::execute(params, ctx)
```

The primary gateway is `TaskToolExecutor` in `musician.rs`:

```rust
#[async_trait]
pub trait TaskToolExecutor: Send + Sync {
    async fn execute(
        &self,
        task_id: &str,
        tool_name: &str,
        input: &str,
        allowed_tools: &[&'static str],
        model: &str,
    ) -> Result<StepResult>;
}
```

Three implementations exist: `ShellTaskToolExecutor` (direct shell), `ExecutableToolExecutor` (bridges to external), and `AgentSessionExecutor` (production — creates real LLM tool-use loop). The redesign must preserve this trait's shape and all three implementations.

Additionally, the AST pipeline has its own abstractions: `StepRunner` (step execution), `ToolAdapter` (cross-harness normalization for ClaudeCode/Gemini/Codex formats), and `ToolExecution` (parallel batch with semaphore).

## Implementation Status

**What has been resolved since this ADR was written:**

- ✅ **Circular dependency `rustycode-llm` ↔ `rustycode-tools` is resolved** via the `rustycode-tool-integration` shim crate, which provides `ToolExecutorApi` trait and lightweight protocol types (`ToolInfo`, `ToolExecutorApi`) without depending on `rustycode-tools` itself.
- ✅ **Phase 0 complete:** ADR and design documents written.
- ✅ **Shim crate built** (early, ahead of full migration timeline) and integrated into LLM provider layer.
- ✅ **`CostTracker` and `TokenCounter` centralized** in `rustycode-tool-integration`.

**What is still pending:**

- ❌ **Phase 1–6 designs not yet executed:** `ToolCtx`/`ExtToolCtx` split, `define_tool!` macro updates, IO trait abstraction, `ToolTestFixture`.
- ❌ **`ToolExecutor` naming collision** not resolved — the orchestration async trait should be renamed (e.g., `TaskToolExecutor` or `OrchestratedToolExecutor`).
- ❌ **Duplicate `ToolInfo` types** not unified — still mapping manually between versions.
- ❌ **`rustycode-tools-registry` integration incomplete** — `get()` method unimplemented, tier filtering implemented but underutilized.
- ❌ **ACP execution path not formalized** — still bypasses `ToolExecutorApi` shim, should route through it.

### What We Evaluated

Four designs were generated and compared (see `docs/designs/tool-interface-redesign.md` for full analysis):

| Design | Approach | Strengths | Weaknesses |
|--------|----------|-----------|------------|
| A — Minimal | 2-method trait, extension map via `get::<T>()` | Simple, small surface | No streaming, typed errors, or context scoping |
| B — Tower-style | `ToolService` + `Layer` trait, composable middleware | Gold standard composability | Complex generics, hard to debug erased stacks |
| C — Two-tier context | `ToolCtx` (4 fields) default, `ExtToolCtx` (16) opt-in | 90% of tools unchanged, incremental | Two new types, `Deref` chain risk |
| D — Ports-and-adapters | 4-method port, no `ToolContext`, deps through adapters | Cleanest boundary, best testability | Big disruption to 90+ call sites |

## Decision

**Adopt Design C as the migration path, with Design D as the architectural target.**

### Rationale

- Design C is a **refactor** (split one struct), not a rewrite. The `define_tool!` macro shape stays identical.
- 90+ existing `define_tool!` invocations need zero changes for `ctx: ToolCtx`.
- Tools that need more context opt in via `ctx: ExtToolCtx` — explicit, not forced.
- Over time, as we identify universal vs adapter-internal concerns, we contract the port toward Design D.
- Design B (Tower) was prototyped in `docs/designs/tool-plugin-system.rs` but rejected for production use due to debugging complexity with erased middleware stacks and boxing overhead on every invocation.

### Concrete First Step

Split `ToolContext` (16 fields) into:
- `ToolCtx` — 4 fields: `cwd`, `role`, `plan_gate`, `cancellation_token`
- `ExtToolCtx` — all 16 fields (inherits or wraps `ToolCtx`)

Update `define_tool!` so `ctx: ToolCtx` is the default, `ctx: ExtToolCtx` is opt-in.

## Consequences

### Positive

- Most tools (90%) need zero changes immediately.
- New tools default to minimal context — cleaner, easier to test.
- `ToolCtx` is cheaply constructible in tests (4 fields vs 16).
- Migration is incremental — no big-bang rewrite.
- Clear path toward Design D: each extraction makes the port thinner.

### Negative

- Two context types to maintain during migration.
- `Deref`/`DerefMut` from `ExtToolCtx` → `ToolCtx` could cause confusion if misused.
- ~30 helper functions that take `&ToolContext` need signature updates over time.
- Not the final architecture — additional refactoring rounds needed to reach Design D.

### Neutral

- `define_tool!` macro gains a new arm for `ExtToolCtx` opt-in.
- Existing `ToolContext` type alias to `ExtToolCtx` during transition (backward compat).

## Resolved Decisions

### DEC-1: Macro Approach — Wrapper Pattern (Unblocks Phase 2)

The `Tool` trait signature does **not** change. The `define_tool!` macro generates a thin wrapper that extracts `ToolCtx` from `ToolContext`:

```rust
// Tool author writes:
fn execute(params: BashParams, ctx: &ToolCtx) -> Result<ToolOutput> { ... }

// Macro generates (impl Tool):
fn execute(&self, params: Value, ctx_full: &ToolContext) -> BoxFuture<'_, Result<ToolOutput>> {
    let ctx = ToolCtx::from(ctx_full);  // extracts 4 fields
    let params = serde_json::from_value(params)?;
    // delegates to tool author's fn — they physically cannot access other 12 fields
}
```

Default = `ToolCtx` (4 fields). Opt-in `context: extended` passes `&ToolContext` directly.

**Why this approach:** Zero trait changes. Zero registry changes. Zero breaking changes. Tools get type-enforced minimal context without touching the trait signature.

### DEC-2: No `Deref` — Inline Fields

`ExtToolCtx` inlines the 4 `ToolCtx` fields rather than using `Deref<Target = ToolCtx>`. Avoids the Deref anti-pattern and keeps field access explicit.

### DEC-3: `tools-runtime` Crate Deferred to Phase 6

New crates (`tools-registry`, `tool-server`) already absorb some planned runtime infra. `ToolTestFixture` goes into `tools-api` for now. Full `tools-runtime` extraction happens when we start building adapters toward Design D.

### DEC-4: Streaming — Defer

Current streaming macro is dead code (references nonexistent `$crate::streaming::ToolStreaming`). If/when streaming is needed, add a `StreamingTool` port alongside `Tool` — don't complicate the base trait.

### DEC-5: `ToolExecutor` Naming Collision → Rename to `TaskToolExecutor`

The orchestration async trait in `musician.rs` gets renamed to `TaskToolExecutor`. Frees up `ToolExecutor` to refer unambiguously to the concrete struct in `rustycode-tools`. Low risk, mechanical rename.

### DEC-6: Duplicate `ToolInfo` → Rename Shim Type to `ToolCallInfo`

Rename the stripped type in `rustycode-tool-integration` to `ToolCallInfo`. Rich `ToolInfo` stays in `tools-api`. Two types serve two purposes (definition vs invocation), and the names make that clear. No breaking change to `tools-api`.

### DEC-7: ACP Bypass — Documented Tech Debt

`rustycode-acp` bypassing the `ToolExecutorApi` shim is known tech debt. Does not block current phases. Will be addressed when ACP integration is formalized.

### Redesign Constraints

The following invariants must be preserved across all phases:

1. **`TaskToolExecutor` trait shape** — all 3 implementations and the `Musician` call site depend on `(task_id, tool_name, input, allowed_tools, model) -> StepResult`
2. **`Tool` trait signature** — 30+ tool implementations depend on it; `ToolRegistry` dispatches via it
3. **`ToolRegistry` registration pattern** — `register(impl Tool)`, `execute(ToolCall, ToolContext)`, `list()` — used by `AgentSessionExecutor` to build schemas
4. **Tiered activation** — `ToolActivationManager.is_active()` is checked before execution in `Musician::play_step_with_context`
5. **Hook lifecycle** — PreToolUse/PostToolUse/ToolError hooks fire around `TaskToolExecutor::execute()` via `ExecutionMiddleware`
6. **Bus event forwarding** — `EventForwarder` maps `EventMsg::ToolCallStarted/ToolExecCompleted` to `OrchestrationEvent`
7. **Inspector pipeline** — `ToolInspector` implementations (security, egress, OSV, repetition, permission) run pre-execution and return `InspectionAction`

## Migration Plan

| Phase | Task | Status | Notes |
|-------|------|--------|-------|
| 0 | Write design doc (this ADR + detailed design) | ✅ Done | ADR written 2026-05-14 |
| 0b | Build `rustycode-tool-integration` shim (early, unplanned) | ✅ Done | Resolves circular dep; now in production use |
| 1 | Extract misplaced modules (`token_counter` → integration, `todo` types → protocol) | ⏳ Partial | `TokenCounter` centralized in shim; `todo` types still in api |
| 2 | Implement `ToolCtx` type + `define_tool!` wrapper pattern (DEC-1) | ⏳ Ready | Unblocked: trait doesn't change, macro generates wrapper |
| 2b | Resolve `ToolExecutor` naming collision → `TaskToolExecutor` (DEC-5) | ⏳ Ready | Mechanical rename in orchestration |
| 2c | Rename shim `ToolInfo` → `ToolCallInfo` (DEC-6) | ⏳ Ready | Clear naming, no breaking change |
| 3 | Kill glob re-exports in `providers/mod.rs` | ⏳ Pending | Mechanical, can run in parallel |
| 4 | Build `ToolTestFixture` in `tools-api` (DEC-3) | ⏳ Pending | Depends on Phase 2 |
| 4b | Formalize ACP execution path (DEC-7) | 📋 Tech debt | Documented, not blocking |
| 4c | Implement `rustycode-tools-registry::get()` method | ⏳ Pending | Currently returns `None` with warning |
| 5 | IO trait abstraction for filesystem/command operations | ⏳ Pending | Design not yet finalized |
| 6+ | Evolve toward Design D (ports-and-adapters) | 🔄 Ongoing | Extract adapters, contract the port |

## Next Immediate Steps

**Phase 2 implementation (unblocked by DEC-1):**
- [ ] Add `ToolCtx` struct (4 fields) to `rustycode-tools-api`
- [ ] Add `ToolCtx::from(ctx: &ToolContext)` extraction
- [ ] Update `define_tool!` macro: default generates `ToolCtx` wrapper, `context: extended` passes `&ToolContext` directly
- [ ] Migrate 5-10 simple tools (bash, glob, grep, ls, read, write, edit, mkdir, mv, cp) to `&ToolCtx` as proof of concept
- [ ] Run `cargo test --workspace` — all 90+ existing invocations must still compile

**Phase 2b (parallel):**
- [ ] Rename orchestration `ToolExecutor` trait → `TaskToolExecutor` (DEC-5)
- [ ] Update all call sites in `rustycode-orchestration`

**Phase 2c (parallel):**
- [ ] Rename shim `ToolInfo` → `ToolCallInfo` (DEC-6)
- [ ] Update all consumers of the shim type

**Phase 1 completion (parallel):**
- [ ] Move `TodoItem`, `TodoStatus`, `TodoState` to `rustycode-protocol`
- [ ] `GLOBAL_TODO_STATES` stays in `tools` (runtime state)

## References

### Design Documents
- Detailed design: `docs/designs/tool-interface-redesign.md`
- Tower prototype (Design B): `docs/designs/tool-plugin-system.rs`

### Primary Crates
- **Tool API:** `crates/rustycode-tools-api/src/lib.rs` — defines `Tool` trait, `ToolContext` (16 fields), `ToolRegistry`, `ToolInfo` (rich), macro `define_tool!`
- **Tool Implementations:** `crates/rustycode-tools/src/` — built-in tools (Bash, Read, Write, Edit, etc.), concrete `ToolExecutor` struct
- **Tool Integration Shim:** `crates/rustycode-tool-integration/src/` — `ToolExecutorApi` trait, `ToolInfo` (stripped), `CostTracker`, `TokenCounter`. **This crate breaks the circular dependency.**
- **Tool Registry (Plugin-aware):** `crates/rustycode-tools-registry/` — wraps core registry with plugin discovery and tier filtering; `get()` method unimplemented
- **Tool Server:** `crates/rustycode-tool-server/` — HTTP REST + WebSocket server for tool execution
- **Orchestration Layer:** `crates/rustycode-orchestration/src/musician.rs` — async `ToolExecutor` trait (distinct from sync struct in `rustycode-tools`); `ShellToolExecutor`, `ExecutableToolExecutor`
- **IDE Integration:** `crates/rustycode-acp/src/tool_executor.rs` — ACP's own `ToolExecutor` struct, currently bypasses `ToolExecutorApi` shim

### Key Findings
- `ToolContext` analysis: 16 fields, most tools use 3-5 (`cwd`, `role`, `plan_gate`, `cancellation_token`)
- Macro scale: `define_tool!` has 90+ invocations across workspace
- Naming collision: Two unrelated `ToolExecutor` concepts (concrete vs. async trait)
- Type duplication: Two `ToolInfo` types with different scopes and manual mapping
- Execution paths: Four distinct paths with varying degrees of integration (CLI/TUI, LLM bridge, ACP, Orchestration)
- Orchestration gateway: `TaskToolExecutor` trait is the single choke point; never bypassed
- Existing wrappers: Three middleware patterns (ExecutionMiddleware, ToolInspectionManager, ConvoyDispatcher) already in production
- Security: 4-layer model (path validation → threat scanning → inspector pipeline → permission/sandbox)
- Two-layer dispatch: Orchestration → TaskToolExecutor → ToolRegistry → Tool::execute
