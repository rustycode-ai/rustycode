# Tool Interface Redesign — Detailed Design

> Companion to ADR 004. Read the ADR first for decision context.
>
> **Status**: Design accepted, implementation is partial. The circular dependency (Phase 0b) has been resolved via `rustycode-tool-integration`. Phases 1–6 are mostly pending. See ADR 004 for current implementation status and resolved decisions.

## Current State

### Surface Inventory

**`rustycode-tools-api`** (4,500 LOC across 15 files):
- `Tool` trait (16 methods)
- `RustyCodeTool` trait (parallel trait for TUI browser adapters)
- `define_tool!` macro (90+ invocations)
- `ToolContext` (16 fields, god struct)
- `ToolRegistry` (registration + execution)
- `ToolPermission`, `ToolTag`, `ToolOutput`
- 14 utility submodules (`edit_format`, `search_strategy`, `tool_selector`, etc.)

**`rustycode-tools`** (104 files, ~66K LOC):
- All tool implementations
- `token_counter.rs` (~400 LOC, duplicated in 3 other crates)
- `todo.rs` / `todo_read.rs` (~580 LOC, `GLOBAL_TODO_STATES` singleton)
- `providers/mod.rs` (42 glob re-exports)
- Security module, registry defaults, execution middleware

### Problem Summary

| Problem | Impact | Severity |
|---------|--------|----------|
| `ToolContext` god struct (16 fields) | Every tool pays for every dependency | High |
| `rustycode-tools-api` god crate (4,500 LOC) | Unclear what's "API" vs "runtime" | High |
| 90+ `define_tool!` invocations | Interface changes are high-risk | High |
| Four execution paths (not two) | Inspector pipeline inconsistently applied | Medium |
| `TokenCounter` duplicated ×4 | Divergent implementations, wasted maintenance | Medium |
| Naming collision: two `ToolExecutor` concepts | Concrete struct vs async trait in orchestration | Medium |
| Duplicate `ToolInfo` types | Rich (tools-api) vs stripped (shim) with manual mapping | Medium |
| 42 glob re-exports | Flat namespace, import collisions | Low-Medium |
| `RustyCodeTool` legacy trait | Parallel to `Tool`, unclear deprecation path | Low |
| Dead streaming macro | References nonexistent `$crate::streaming::ToolStreaming` | Low |

### What Has Changed Since This Design Was Written

**Phase 0b (Circular Dependency Break) — Completed Early:**
- `rustycode-tool-integration` shim crate **built and in production**.
- Provides `ToolExecutorApi` trait and lightweight `ToolInfo` (stripped) + `ToolExecutor` implementations.
- `rustycode-llm` now depends only on the shim, not on `rustycode-tools` — circular dependency **resolved**.
- `CostTracker` and `TokenCounter` **centralized** in the shim.

**Additional Infrastructure Built (Not In Original Design):**
- **`rustycode-tools-registry`** — Wraps core `ToolRegistry` with plugin discovery and tier-based filtering. `get()` method currently unimplemented.
- **`rustycode-tool-server`** — HTTP REST + WebSocket server exposing tool execution via web APIs.
- **`rustycode-acp`** — IDE/ACP integration with its own `ToolExecutor` struct; currently bypasses `ToolExecutorApi` shim.

**Remaining Design Issues (Not Yet Addressed):**
- `ToolCtx` / `ExtToolCtx` split **not implemented** — still only `ToolContext` (16 fields).
- `define_tool!` macro **not updated** with new context arms.
- Naming collision (`ToolExecutor` as both struct and async trait) **not resolved**.
- Duplicate `ToolInfo` types **not unified** — still mapping manually in `executor.rs`.
- Phases 1–6 **mostly pending** (except for Phase 0b, which was accelerated).

## Execution Flow

### Current Dispatch Path (5 Layers)

Tool calls traverse five layers from LLM response to tool execution:

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

### Existing Wrapper Patterns

Three wrapper/middleware patterns already exist:

**A. ExecutionMiddleware** (`executor/middleware.rs`)
- Wraps any `Tool` with: PreToolUse hooks, input validation, plan mode checks, cost tracking, PostToolUse hooks
- Usage: `middleware.execute(&tool, params, ctx)` — wraps the raw `tool.execute()`

**B. ToolInspectionManager** (`executor/manager.rs`)
- Pipeline of `ToolInspector` trait objects that run *before* execution
- Inspectors: `SecurityInspector`, `EgressInspector`, `OsvInspector`, `RepetitionInspector`, `PermissionInspector`
- Each returns `InspectionAction::{Allow, Deny, RequireApproval(msg)}`
- Most restrictive action wins

**C. ConvoyDispatcher** (`executor/convoy.rs`)
- Wraps tool execution with a `ToolGate` check
- Guards execution with `gate.check_access(role, tool_name)`

### Orchestration Interface Contract (Two-Layer Dispatch)

The orchestration layer never calls `Tool::execute` directly. It uses a two-layer dispatch:

```
Orchestration (Musician::play_step_with_context)
    │  calls TaskToolExecutor::execute(task_id, tool_name, input, allowed_tools, model)
    ▼
AgentSessionExecutor (production)   ← Creates AgentSession, builds schemas from ToolRegistry
ShellTaskToolExecutor (simple)      ← Direct shell, sandbox + security
ExecutableToolExecutor (external)   ← Bridges to rustycode-executable
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

### AST Pipeline Tool Abstractions

The AST pipeline has its own interface layer:

- **`StepRunner`** (`ast/executor.rs`) — `run(step: &ExecutionStep, step_index: usize) -> StepEvidence`
- **`ToolAdapter`** (`ast/tool_adapter.rs`) — normalizes tool names/args between harnesses (ClaudeCode, RustyCode, Gemini, Codex)
- **`ToolExecution`** (`executor/parallel_executor.rs`) — parallel batch execution with semaphore-bounded concurrency

### Security Model (4 Layers)

```
Layer 1: Path Validation (rustycode-tools-security)
  - Traversal prevention, symlink detection, blocked extensions (.env, .key, .pem)
  - Blocked filenames (credentials.json, id_rsa, .netrc)
  - Blocked path components (.ssh, .gnupg, .aws, .git)
  - Size limits (10MB), cross-platform path normalization

Layer 2: Command Threat Scanning (rustycode-tools-security)
  - ThreatScanner: 40+ regex patterns across 8 categories
  - Categories: FileSystemDestruction, RemoteCodeExecution, DataExfiltration,
    SystemModification, NetworkAccess, ProcessManipulation, PrivilegeEscalation, CommandInjection
  - Risk levels: Critical/High → Deny, Medium → RequireApproval, Low → Allow

Layer 3: Inspector Pipeline (executor/inspector/)
  - SecurityInspector integrates ThreatScanner into the tool call pipeline
  - EgressInspector detects network destinations in commands
  - OsvInspector checks for known vulnerable packages

Layer 4: Permission & Sandbox (executor/permission.rs, ToolContext.sandbox)
  - Permission hierarchy: None < Read < Write < Execute < Network
  - SandboxConfig: allowed_paths, denied_paths, timeout, docker/os sandbox
  - Session mode gating: Planning mode allows only read-only tools
```

### Redesign Constraints

The following invariants must be preserved across all phases:

1. **`TaskToolExecutor` trait shape** — all 3 implementations and `Musician` call site depend on `(task_id, tool_name, input, allowed_tools, model) -> StepResult`
2. **`Tool` trait signature** — 30+ tool implementations depend on it; `ToolRegistry` dispatches via it
3. **`ToolRegistry` registration pattern** — `register(impl Tool)`, `execute(ToolCall, ToolContext)`, `list()` — used by `AgentSessionExecutor` to build schemas
4. **Tiered activation** — `ToolActivationManager.is_active()` checked before execution
5. **Hook lifecycle** — PreToolUse/PostToolUse/ToolError hooks fire around `TaskToolExecutor::execute()`
6. **Bus event forwarding** — `EventForwarder` maps `EventMsg::ToolCallStarted/ToolExecCompleted` to `OrchestrationEvent`
7. **Inspector pipeline** — `ToolInspector` implementations run pre-execution and return `InspectionAction`

## Design C: Two-Tier Context

### Key Insight: The Trait Doesn't Change

The `Tool` trait signature stays `fn execute(&self, params: Value, ctx: &ToolContext)`. The `define_tool!` macro generates a **thin wrapper** that extracts `ToolCtx` from `ToolContext` for tools that opt into minimal context. This means:

- Zero trait changes
- Zero registry changes
- Zero breaking changes
- 90+ existing invocations continue to compile unchanged

### New Types

```rust
// --- Core context (4 fields) — most tools only need this ---

pub struct ToolCtx {
    pub cwd: PathBuf,
    pub role: Role,
    pub plan_gate: PlanGate,
    pub cancellation_token: CancellationToken,
}

impl ToolCtx {
    /// Extract the 4 core fields from a full ToolContext.
    /// Used by the macro-generated wrapper.
    pub fn from(ctx: &ToolContext) -> Self {
        Self {
            cwd: ctx.cwd.clone(),
            role: ctx.role.clone(),
            plan_gate: ctx.plan_gate.clone(),
            cancellation_token: ctx.cancellation_token.clone(),
        }
    }
}

// ToolContext stays as-is (16 fields). No ExtToolCtx alias needed.
// "Extended context" = just passing &ToolContext directly.
```

### `define_tool!` Changes — Wrapper Pattern

The macro generates different code depending on whether `context: extended` is specified.

```rust
// DEFAULT — minimal context (ToolCtx):
// Tool author writes:
define_tool! {
    name: "bash",
    description: "Run shell commands",
    ...
    fn execute(params: BashParams, ctx: &ToolCtx) -> Result<ToolOutput> {
        // ctx has only 4 fields — exactly what bash needs
        // Cannot access http_client, config, etc. — type-enforced
    }
}

// Macro generates (impl Tool):
fn execute(&self, params: Value, ctx_full: &ToolContext) -> BoxFuture<'_, Result<ToolOutput>> {
    let ctx = ToolCtx::from(ctx_full);  // extract 4 fields
    let params: BashParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return Box::pin(async move { Err(ToolError::Validation(e.to_string())) }),
    };
    Box::pin(async move {
        // delegates to tool author's fn with &ToolCtx
        self.execute_inner(params, &ctx).await
    })
}

// ──────────────────────────────────────────────────

// OPT-IN — extended context (&ToolContext passed through):
define_tool! {
    name: "web_fetch",
    description: "Fetch URL content",
    context: extended,  // <- opt-in keyword
    ...
    fn execute(params: WebFetchParams, ctx: &ToolContext) -> Result<ToolOutput> {
        // ctx has all 16 fields — web_fetch needs http_client, config, etc.
        // This is identical to the current behavior
    }
}

// Macro generates (impl Tool) — same as today, no wrapper:
fn execute(&self, params: Value, ctx: &ToolContext) -> BoxFuture<'_, Result<ToolOutput>> {
    let params: WebFetchParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return Box::pin(async move { Err(ToolError::Validation(e.to_string())) }),
    };
    Box::pin(async move {
        self.execute_inner(params, ctx).await
    })
}

// ──────────────────────────────────────────────────

// EXISTING — no changes needed, continues working:
define_tool! {
    name: "grep",
    ...
    fn execute(params: GrepParams, ctx: &ToolContext) -> Result<ToolOutput> {
        // Still works exactly as before. No migration required.
    }
}
```

### Migration Strategy

**Step 1**: Add `ToolCtx` struct (4 fields) + `ToolCtx::from(&ToolContext)` to `tools-api`.

**Step 2**: Update `define_tool!` macro — add new arm for `&ToolCtx` that generates the wrapper. The `context: extended` arm and the default (no keyword) arm both pass `&ToolContext` directly (backward compat).

**Step 3**: Migrate tools one at a time by changing their `execute` signature from `ctx: &ToolContext` to `ctx: &ToolCtx`. Each migration is a single-line change in the tool file.

**Step 4**: Once all tools that CAN use `ToolCtx` are migrated, make `ToolCtx` the macro default. Tools that need `&ToolContext` add `context: extended`.

**Step 5**: (Design D horizon) Tools using `&ToolCtx` are already "halfway" to the port pattern — they depend on minimal context. Extract adapters from `ToolContext` fields.

### Which Tools Need Extended Context

Based on field usage analysis:

| Context Level | Tools | Count |
|---------------|-------|-------|
| `ToolCtx` (4 fields) | bash, glob, grep, ls, read, write, edit, mkdir, mv, cp, rm, ... | ~70 |
| `ExtToolCtx` (16 fields) | web_fetch (http_client), todo_read (token_counter), notebook_edit (config), ... | ~20 |

Roughly 75% of tools can use the minimal context immediately.

## Crate Decomposition

### Target Structure

```
rustycode-tools-api/       # Contract crate (~600 LOC target)
├── Tool trait (unchanged signature)
├── define_tool! macro (with wrapper pattern)
├── ToolCtx (4 fields) + ToolContext (16 fields, unchanged)
├── ToolOutput, ToolError
├── ToolMeta, ToolPermission, ToolTag
├── ToolTestFixture (test helper, added in Phase 4)
└── NO utility modules, NO registry, NO runtime code

rustycode-tools/           # Tool implementations + runtime (existing home)
├── providers/             # All tool implementations
├── ToolRegistry
├── ExecutionMiddleware
├── Inspector pipeline
├── security.rs
└── (no token_counter, no todo types — moved out)

rustycode-tool-integration/ # Shim (built, in production)
├── ToolExecutorApi trait
├── ToolCallInfo (renamed from ToolInfo, DEC-6)
├── CostTracker
├── TokenCounter
└── ToolExecutor implementations

rustycode-protocol/        # Shared domain types
├── todo_types.rs          # TodoItem, TodoStatus, TodoState (Phase 1)
└── (existing protocol types)
```

**Note:** `rustycode-tools-runtime` crate deferred to Phase 6 (DEC-3). The execution infra stays in `rustycode-tools` for now. `tools-registry` and `tool-server` are separate crates already handling their own concerns.

### What Moves Where

| Current Location | Target | Rationale | Phase |
|------------------|--------|-----------|-------|
| `tools-api::ToolRegistry` | stays in `tools` (via re-export) | Registry is runtime infra, not contract | 6+ |
| `tools-api::edit_format` | `tools` (or `tools-runtime` later) | Utility, not contract | 3 |
| `tools-api::search_strategy` | `tools` (or `tools-runtime` later) | Utility, not contract | 3 |
| `tools-api::tool_selector` | `tools` (or `tools-runtime` later) | Runtime decision logic | 3 |
| `tools::token_counter` | `tool-integration` (DONE) | Shared across crates | 0b ✅ |
| `tools::todo` / `todo_read` types | `protocol` | Shared with TUI, core | 1 |
| `tools::todo_read::TodoReadTool` | stays in `tools` | Tool implementation stays | — |
| 42 glob re-exports in `providers/mod.rs` | Explicit `pub use` per item | Namespace hygiene | 3 |
| shim `ToolInfo` → `ToolCallInfo` | `tool-integration` (rename) | Clear naming (DEC-6) | 2c |

## Phase Execution Order

Each phase = one PR. Every PR must leave `cargo test --workspace` green.

### Phase 0b: Circular Dependency Resolution (COMPLETED ✅)
- Built `rustycode-tool-integration` shim crate with `ToolExecutorApi` trait.
- Moved `CostTracker` and `TokenCounter` to shim (partial Phase 1 work, early).
- Wired LLM providers to use shim instead of depending on `rustycode-tools`.
- **Result:** Circular dependency `rustycode-llm` ↔ `rustycode-tools` is **resolved**. ✅

### Phase 1: Module Extractions (IN PROGRESS ⏳)
- ✅ Move `TokenCounter` to `rustycode-tool-integration` — **DONE** (as part of Phase 0b).
- ❌ Move `TodoItem`, `TodoStatus`, `TodoState` to `rustycode-protocol` — **PENDING**.
- ❌ `GLOBAL_TODO_STATES` stays in `tools` (it's runtime state).
- ✅ No interface changes, no macro changes yet — **READY TO PROCEED**.

**Next**: Complete Phase 1 by moving todo types. No behavior change, all tests should pass.

### Phase 2: Two-Tier Context via Wrapper Pattern (READY ✅ — Unblocked by DEC-1)
- [ ] Add `ToolCtx` struct (4 fields) to `rustycode-tools-api`
- [ ] Add `ToolCtx::from(ctx: &ToolContext)` extraction
- [ ] Update `define_tool!` macro: new arm generates wrapper for `&ToolCtx` tools
- [ ] `context: extended` keyword passes `&ToolContext` directly (current behavior)
- [ ] No keyword (default for now) also passes `&ToolContext` directly (backward compat)
- [ ] Migrate 5-10 simple tools (bash, glob, grep, ls, read, write, edit) to `&ToolCtx`
- [ ] Run `cargo test --workspace` — all 90+ existing invocations must compile

**Key insight**: The `Tool` trait signature does not change. The macro generates a thin extraction wrapper. Zero breaking changes.

### Phase 2b: Resolve Naming Collision (READY ✅ — DEC-5)
- Rename orchestration's async `ToolExecutor` trait to `TaskToolExecutor`.
- Update `rustycode-orchestration/src/musician.rs` and all call sites.
- Can run in parallel with Phase 2.

### Phase 2c: Rename Duplicate `ToolInfo` (READY ✅ — DEC-6)
- Rename shim's `ToolInfo` to `ToolCallInfo` in `rustycode-tool-integration`.
- Rich `ToolInfo` stays in `tools-api`. Two types, two purposes, clear names.
- Update all consumers of the shim type.
- Can run in parallel with Phase 2.

### Phase 3: Kill Glob Re-exports (LOW PRIORITY ⏳)
- Replace 42 `pub use module.*` with explicit `pub use` per item in `providers/mod.rs`.
- Mechanical change, no behavior difference.
- Can be done in parallel with other phases.

### Phase 4: ToolTestFixture (DEPENDS ON PHASE 2 ⏳)
- Add `ToolTestFixture` to `tools-api` (not a new crate — DEC-3).
- Minimal `ToolCtx::test_fixture()` constructor for tests.
- `ToolContext::test_fixture()` that builds a full context from minimal inputs.
- Migrate existing tests to use fixture instead of hand-building `ToolContext`.

### Phase 4b: Formalize ACP Execution Path (TECH DEBT 📋 — DEC-7)
- Currently: `rustycode-acp/src/tool_executor.rs` bypasses `ToolExecutorApi` shim.
- Documented as known tech debt. Does not block current phases.
- Will be addressed when ACP integration is formalized.

### Phase 4c: Implement `rustycode-tools-registry::get()` (PENDING ⏳)
- Currently returns `None` with a warning.
- Scope: single-tool lookup by `ToolName` with tier filtering.

### Phase 5: IO Trait Abstraction (DEPENDS ON PHASE 2 ⏳)
- Introduce `FileSystem` and `CommandRunner` traits behind `ToolCtx`.
- Tools depend on traits, not `tokio::fs` / `std::process::Command` directly.
- Enables in-memory testing without filesystem mocking.

### Phase 6+: Evolve Toward Design D (LONG-TERM 🔄)
- Identify which `ToolContext` fields are truly universal vs adapter-internal.
- Tools already using `&ToolCtx` are "halfway" to the port pattern.
- Extract adapters (sandbox, permissions, MCP) behind explicit constructor injection.
- Contract the port toward 4 methods: `name`, `description`, `input_schema`, `invoke`.
- Remove `ToolContext` entirely — deps flow through adapter constructors.
- `rustycode-tools-runtime` crate extraction may happen here (DEC-3).

## Design D: Ports-and-Adapters (Target)

The eventual target architecture. Not implemented yet — this is the north star.

```rust
/// The port — what every tool implements.
/// No ToolContext. Dependencies flow through the adapter constructor.
pub trait Tool: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    fn invoke(&self, params: serde_json::Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>>;
}

/// Flat output — no generics, no context leaking.
pub struct ToolOutput {
    pub text: String,
    pub is_error: bool,
}

/// Adapters absorb their own dependencies.
pub struct BashTool {
    cwd: PathBuf,
    sandbox: SandboxConfig,
    permissions: Arc<ToolPermissions>,
}

impl Tool for BashTool {
    fn name(&self) -> &str { "bash" }
    fn description(&self) -> &str { "Run shell commands" }
    fn input_schema(&self) -> Value { /* ... */ }
    fn invoke(&self, params: Value) -> Pin<Box<dyn Future<Output = ToolOutput> + Send + '_>> {
        // Has cwd, sandbox, permissions — exactly what it needs
        Box::pin(async move { /* ... */ })
    }
}

/// MCP adapter — same port, different constructor.
pub struct McpToolAdapter {
    client: Arc<dyn McpClient>,
    tool_name: String,
}

impl Tool for McpToolAdapter { /* same 4 methods */ }
```

### Why Not Start Here

- 90+ `define_tool!` invocations would all need rewriting.
- Every tool would need a constructor change simultaneously.
- Risk of breaking the entire tool surface in one shot.
- Design C lets us migrate incrementally: tools using `ToolCtx` are already "halfway" to the port pattern.

## Alternatives Considered

### Design A: Minimal (2-method trait + extension map)
- **Rejected**: Extension map (`get::<T>()`) loses type safety and makes dependency discovery impossible at compile time. No streaming support. No typed errors.

### Design B: Tower Service/Layer
- **Rejected**: Prototyped in `docs/designs/tool-plugin-system.rs`. Composable middleware is elegant but:
  - Erased `dyn ToolService` stacks are hard to debug in production.
  - Boxing overhead on every invocation (not just at registration).
  - Complex generic bounds propagate through the entire middleware chain.
  - Overkill for our current needs — we have ~5 cross-cutting concerns, not 20.

### Design D: Ports-and-adapters (immediate)
- **Rejected as first step**: Too disruptive to 90+ call sites. Adopted as target architecture instead.

## Resolved Questions

All questions from the initial design have been resolved. See ADR 004 "Resolved Decisions" section for full rationale.

| # | Question | Decision | ADR Ref |
|---|----------|----------|---------|
| 1 | Should `ExtToolCtx` use `Deref<Target = ToolCtx>`? | No `Deref` — `ToolCtx` is standalone, no `ExtToolCtx` needed. Wrapper pattern extracts 4 fields. | DEC-1, DEC-2 |
| 2 | When to create `rustycode-tools-runtime` crate? | Deferred to Phase 6. `ToolTestFixture` goes in `tools-api`. | DEC-3 |
| 3 | `context: extended` vs `#[extended_context]` attribute? | `context: extended` keyword in macro body. Simpler, no proc macro infra. | DEC-1 |
| 4 | Streaming support in Design D port? | Defer. Dead code. Add `StreamingTool` port when needed. | DEC-4 |
| 5 | `ToolExecutor` naming collision? | Rename orchestration async trait to `TaskToolExecutor`. | DEC-5 |
| 6 | Duplicate `ToolInfo` types? | Rename shim type to `ToolCallInfo`. Two types, two purposes, clear names. | DEC-6 |
| 7 | ACP bypasses `ToolExecutorApi`? | Documented as tech debt. Does not block current phases. | DEC-7 |
| 8 | Does redesign affect `TaskToolExecutor` orchestration gateway? | No. `TaskToolExecutor` trait shape is preserved. Redesign only affects the inner `Tool::execute` + `ToolContext` layer. | — |
| 9 | How does `ToolActivationManager` interact with `allowed_tools` whitelist? | Two mechanisms coexist: static whitelist (`allowed_tools: &[&str]` in `TaskToolExecutor`) and dynamic tier filtering (`ToolActivationManager`). Tier only promotes upward; cannot demote. Reconciling these is a future optimization, not a blocker. | — |
| 10 | Should the 3 existing wrapper patterns (middleware, inspector, convoy) be unified? | Not in this redesign. They serve different lifecycle points (pre-execution inspection vs hook execution vs role gating). Unification would be a Phase 6+ concern. | — |
| 11 | Does `ExecutionMiddleware::execute()` bypass `ToolRegistry`? | No. `ExecutionMiddleware` wraps individual `Tool::execute()` calls. `ToolRegistry::execute()` is the dispatcher that calls middleware. The layering is: Registry → Middleware → Inspector → Convoy → Tool. | — |
