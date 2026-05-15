# TUI Architecture Decomposition: God Struct → Host + Feature Modules

## TL;DR

> **Quick Summary**: Decompose the RustyCode TUI god struct (`TUI` in event_loop.rs, ~2338 lines, 13 sub-structs containing ~70 fields) into a thin AppShell + feature modules registering routes/commands/panels. Use `StreamChunk` (LLM stream output) and `EventMsg` (general service events) as the two existing typed channels — both remain intact and serve different purposes. Features own their state+update+render, never get `&mut AppShell`.
>
> **Deliverables**:
> - `TuiFeature` trait with lifecycle methods (register/update/render)
> - `AppShell` struct owning terminal lifecycle, focus, frame budget, feature registry
> - `UpdateCtx` / `RenderCtx` narrow context types (no god-struct access)
> - Feature registry for routes, commands, keybindings, surfaces
> - One fully-migrated feature module (Plugin Manager) as proof
> - Handler→field mapping document enabling remaining migrations
>
> **Estimated Effort**: Large (multi-wave, structural refactoring of core crate)
> **Parallel Execution**: YES - 5 waves, max 7 concurrent tasks
> **Critical Path**: Phase 0 (traits + mapping) → Phase 1 (first feature) → Phase 2 (render extraction) → Phase 3 (handler decomposition) → Phase 4 (remaining features)

---

## Context

### Original Request
"let's rethink of tui. I find that it's very entangled with very sloppy wiring. Can we think of a better way? In ~/dev/opencode, it seems to have plugin/component to reduce complexity"

### Interview Summary
**Key Discussions**:
- Mapped current god struct: 50+ fields spanning widgets, services, workspace, session, overlays, panels, theme, team, search, model state
- Studied opencode: Component trait (init/handle_events/update/render), action-based message passing
- Evaluated 3 designs: Better God Object, Elm/Action Reducer, Host + Feature Modules
- Recommended Design 3 + Design 2 event model (hybrid)

**Research Findings**:
- `StreamChunk` (in `async_.rs`, cap 100) carries LLM stream output (text deltas, tool calls, errors, Done sentinel) — NOT being replaced
- `EventMsg` (in `rustycode-protocol`) carries general service events (team events, workspace events, etc.) — a separate channel serving a different purpose
- **These two channels serve distinct event domains and must both remain; they are NOT a migration in progress**
- `poll_services()` is extracted to `service_polling.rs` (impl TUI block), not inline in event_loop.rs
- `poll_services()` drain limits: stream=8/frame, tool=8/frame — carefully commented, do not change without benchmarking
- `poll_services()` handlers mutate 6+ god struct field groups: session.streaming.*, session.messages, session.active_tools, panels.tool_panel, panels.tool_approval, model.token_budget, workspace.*, sys.dirty
- `StreamEventAdapter` in `streaming/adapter.rs` converts source events → StreamChunk (central coupling point)
- `approval_tx/approval_rx` is in `ServiceManager` (service_integration.rs) — synchronous back-channel; streaming thread blocks on it
- session_mode.rs and mcp_mode.rs bypass channel architecture entirely (direct state read/write)
- `tick_pipeline()` uses `block_on()` synchronously in the event loop — required, do not remove
- **New since plan creation**: `streaming/` module (7 files), `pipeline/` module (11+ files), `agents/` module (4 files), `state/` module have been partially extracted
- **State model progress**: 13 named sub-structs now in `state_model.rs` (was 50+ flat fields)

### Metis Review
**Critical Discoveries**:
- EventMsg IS the typed event bus — plan should complete existing migration, not build new
- poll_services() must remain single drain point — features must NOT own drain timing
- Tool approval back-channel must be accessible through UpdateCtx/CommandEffect
- session_mode/mcp_mode channel-ification is explicitly deferred to follow-up

**Identified Gaps (addressed)**:
- Test coverage: `event_loop_tests.rs` (298 lines, ~25 tests) now exists; 1,962 total `#[test]` in crate — Task 1 PARTIALLY DONE
- 12 async pathways with drain ordering: AppShell owns drain, features get UpdateCtx
- Handler→field mapping needed before migration: Added mapping task (must now include streaming/, pipeline/ modules)
- Tool approval threading: Modeled as CommandEffect
- Frame budget concerns: Added benchmarking guardrail
- StreamChunk/EventMsg clarification: They are separate channels for separate domains — GUARDRAIL-ASYNC-1 updated accordingly
- New modules: streaming/, pipeline/, agents/ must be mapped in Task 3 and covered in Wave 3

### Metis Validation (Post-Generation Review)
**Validation Status**: PASSED with fixes applied

**Fixes Applied**:
- ✅ File paths corrected: `poll_services()` → `service_polling.rs`, `session_mode.rs` → `services/`, `mcp_mode.rs` → `services/`
- ✅ Task 1 updated: Audit existing `event_loop_tests.rs` and `handlers/tests.rs` before writing new tests
- ✅ Existing render decomposition acknowledged: `render/` directory already exists with `brutalist.rs`, `layout.rs`, `messages.rs`, etc.
- ✅ Existing state extraction acknowledged: `state/` directory with `state_manager.rs`, `scrolling_ops.rs`
- ✅ Wave 3 merge conflict risk noted: executors should coordinate or run sequentially if conflicts arise

**Remaining Recommendations (for executor awareness)**:
- Task 12 may exceed 500 lines — executor should split into sub-commits per handler group
- Feature flag mechanism: executor should choose compile-time `cfg(feature = "app-shell")`
- Wave 3 parallel extractions all modify god struct — if merge conflicts arise, switch to sequential
- FocusRing should acknowledge existing focus tracking in state_model.rs

---

## Work Objectives

### Core Objective
Decompose the TUI god struct into a host+feature architecture where AppShell owns infrastructure and feature modules own their state+update+render lifecycle, connected via the existing EventMsg channel.

### Concrete Deliverables
- Core trait definitions: `TuiFeature`, `FeatureRegistry`, `UpdateCtx`, `RenderCtx`
- AppShell shell running alongside existing TUI (dual-path)
- Handler→field mapping document
- One fully-migrated feature module (Plugin Manager)
- Unified command dispatch via existing CommandContext/CommandEffect
- event_loop.rs reduced to <500 lines (orchestration wiring only)

### Definition of Done
- [ ] `cargo build -p rustycode-tui` succeeds
- [ ] `cargo test -p rustycode-tui` passes
- [ ] `cargo clippy -p rustycode-tui -- -D warnings` clean
- [ ] `cargo run -p rustycode-cli -- tui` launches and functions identically
- [ ] event_loop.rs is <500 lines (pure orchestration wiring)
- [ ] No feature module receives `&mut AppShell` or `&mut TUI`
- [ ] All async channel drain ordering preserved

### Must Have
- `TuiFeature` trait with id(), register(), update(), render()
- AppShell owning terminal lifecycle, focus routing, feature registry
- UpdateCtx/RenderCtx narrow contexts (no god struct access)
- One complete feature module extraction as proof
- Handler→field mapping for all remaining extractions
- Zero visual regression after each step

### Must NOT Have (Guardrails)
- **GUARDRAIL-ASYNC-1**: MUST NOT merge or unify `StreamChunk` and `EventMsg` into a single channel or enum. They serve different event domains (LLM stream output vs. general service events) and must remain as two separate channels. Do not treat their coexistence as an incomplete migration — it is intentional.
- **GUARDRAIL-ASYNC-2**: MUST NOT change poll_services() drain limits without benchmarking.
- **GUARDRAIL-ASYNC-3**: MUST NOT remove approval_tx/approval_rx back-channel without replacement.
- **GUARDRAIL-ASYNC-4**: MUST NOT change Arc<Mutex<>>/Arc<RwLock<>> patterns in shared state during Phase 1.
- **GUARDRAIL-ASYNC-5**: MUST preserve BoundedChannel backpressure semantics.
- **GUARDRAIL-ASYNC-6**: MUST NOT channel-ify session_mode.rs or mcp_mode.rs in this refactoring.
- **GUARDRAIL-ASYNC-7**: MUST NOT remove or replace `block_on(tick_pipeline())` — it is intentional and load-bearing.
- **GUARDRAIL-BP-1**: Zero visual regression after each step.
- **GUARDRAIL-MS-1**: Every step compiles and passes tests independently.
- **GUARDRAIL-MS-3**: No step exceeds ~500 lines of changed code.
- **GUARDRAIL-MS-4**: `cargo run -p rustycode-cli -- tui` works after every commit.
- **GUARDRAIL-AB-1**: Feature modules NEVER receive &mut AppShell.
- **GUARDRAIL-AB-4**: rustycode-orchestration boundary is NOT violated.
- **GUARDRAIL-SCOPE-1**: MUST NOT consolidate 12 channels into fewer.
- **GUARDRAIL-SCOPE-2**: MUST NOT remove block_on(tick_pipeline()).
- **GUARDRAIL-SCOPE-3**: MUST NOT add performance optimization unless regression detected.
- **GUARDRAIL-SCOPE-4**: MUST NOT add new user-facing features.
- **GUARDRAIL-SCOPE-5**: MUST NOT refactor `pipeline/` internals — wire it as an AppShell service as-is.
- **GUARDRAIL-SCOPE-6**: MUST NOT refactor `streaming/` internals — wire into SessionStreaming feature as-is.
- **GUARDRAIL-AB-3**: `StreamEventAdapter` in `streaming/adapter.rs` MUST NOT be modified.
- **GUARDRAIL-RENDER-1**: Feature module render() MUST NOT call block_on().
- **GUARDRAIL-ADAPTER-1**: MUST NOT modify StreamEventAdapter conversion layer.

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** - ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: YES (cargo test)
- **Automated tests**: Tests-after (structural tests alongside implementation)
- **Framework**: Rust built-in #[test] + cargo test

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **CLI/TUI**: Use interactive_bash (tmux) — Run TUI, send keystrokes, validate output
- **Build**: Use Bash — cargo build/test/clippy/fmt
- **Module**: Use Bash (cargo test) — Run specific test modules

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 0 (Foundation — sequential, everything depends on this):
├── Task 1: Assess test coverage + write characterization tests [deep]
├── Task 2: Define TuiFeature trait + contexts + feature registry [deep]
├── Task 3: Map handlers → (feature, fields_mutated) extraction contract [deep]
└── Task 4: Create AppShell shell alongside existing TUI [deep]

Wave 1 (First Feature Extraction — sequential, proves the pattern):
├── Task 5: Extract Plugin Manager state into FeatureState [unspecified-high]
├── Task 6: Implement TuiFeature for Plugin Manager [unspecified-high]
├── Task 7: Wire Plugin Manager into AppShell + verify [unspecified-high]
└── Task 8: Remove extracted Plugin Manager code from god struct [quick]

Wave 2 (Render + Command Unification — parallel where possible):
├── Task 9: Extract rendering into feature-aware dispatch [deep]
├── Task 10: Unify command dispatch via CommandContext/CommandEffect [deep]
├── Task 11: Wire tool approval back-channel through CommandEffect [unspecified-high]
└── Task 12: Route poll_services() output through feature UpdateCtx [deep]

Wave 3 (Remaining Features — parallel per feature):
├── Task 13: Extract Help/CommandPalette feature [unspecified-high]
├── Task 14: Extract FileSelector/FileFinder feature [unspecified-high]
├── Task 15: Extract Search/MessageSearch feature [unspecified-high]
├── Task 16: Extract MCP panel feature [unspecified-high]
├── Task 17: Extract SessionStreaming feature (wraps streaming/ module) [deep]
├── Task 18: Extract Theme/Toast/ErrorManager feature [unspecified-high]
├── Task 19: Extract TeamMode/WorkerPanel/AgentManager feature [deep]
└── Task 20: Wire pipeline/ as AppShell service (not TuiFeature) [deep]

Wave FINAL (After ALL tasks — 4 parallel reviews):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA of TUI (unspecified-high)
└── Task F4: Scope fidelity check (deep)
→ Present results → Get explicit user okay

Critical Path: T1 → T2 → T3 → T4 → T5 → T6 → T7 → T8 → T9 → T12 → T17 → F1-F4
Max Concurrent: 8 (Wave 3)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1 | - | 2, 3, 4 | 0 |
| 2 | 1 | 5, 6, 7 | 0 |
| 3 | 1 | 5, 12 | 0 |
| 4 | 2 | 5, 6, 7, 8 | 0 |
| 5 | 2, 3, 4 | 6 | 1 |
| 6 | 5 | 7 | 1 |
| 7 | 6 | 8 | 1 |
| 8 | 7 | 9, 13-18 | 1 |
| 9 | 8 | F1-F4 | 2 |
| 10 | 8 | 11, F1-F4 | 2 |
| 11 | 10 | F1-F4 | 2 |
| 12 | 8, 3 | F1-F4 | 2 |
| 13 | 8, 12 | F1-F4 | 3 |
| 14 | 8, 12 | F1-F4 | 3 |
| 15 | 8, 12 | F1-F4 | 3 |
| 16 | 8, 12 | F1-F4 | 3 |
| 17 | 8, 12 | F1-F4 | 3 |
| 18 | 8, 12 | F1-F4 | 3 |
| 19 | 8, 12 | F1-F4 | 3 |
| 20 | 8 | F1-F4 | 3 |

### Agent Dispatch Summary

- **Wave 0**: 4 tasks — T1→`deep`, T2→`deep`, T3→`deep`, T4→`deep` (sequential)
- **Wave 1**: 4 tasks — T5→`unspecified-high`, T6→`unspecified-high`, T7→`unspecified-high`, T8→`quick` (sequential)
- **Wave 2**: 4 tasks — T9→`deep`, T10→`deep`, T11→`unspecified-high`, T12→`deep` (T9+T10 parallel, T11 after T10, T12 after T8+T3)
- **Wave 3**: 8 tasks — T13-T16,T18→`unspecified-high`, T17,T19,T20→`deep` (all parallel; T20 only needs T8)
- **FINAL**: 4 tasks — F1→`oracle`, F2→`unspecified-high`, F3→`unspecified-high`, F4→`deep` (all parallel)

---

## TODOs

- [x] 1. Complete Characterization Tests — **COMPLETE** (commit `81eef3d6a`)

  **Final Status**: 16 new tests written + pre-existing renderer/commands tests confirmed adequate. New baseline: **2,022 tests passed**, 1 pre-existing failure (`memory::compaction::tests::test_context_monitor_update_from_api` — out of scope).

  **What was done**:
  - `service_polling_tests.rs`: 10 tests (drain limits, disconnected cleanup, unconditional processing, capacities, reset_streaming_state, system message scroll)
  - `handlers/tests.rs`: 6 tests (Thinking, Stopped, ToolComplete, SystemMessage, TokenUsage, FileSnapshot)
  - Pre-existing `renderer.rs` characterization tests: confirmed adequate for RendererState
  - Pre-existing `commands::characterization_tests`: confirmed adequate for CommandEffect dispatch

  **Must NOT do**:
  - Do NOT refactor anything while writing tests
  - Do NOT change existing behavior
  - Do NOT duplicate tests already in `event_loop_tests.rs`
  - Do NOT add tests for private internals unless they're on a critical hot path

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-testing`, `rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 0 (sequential, first task)
  - **Blocks**: Tasks 2, 3, 4
  - **Blocked By**: None

  **References**:
  - `crates/rustycode-tui/src/app/event_loop_tests.rs` — Existing tests; check these first for gaps before adding
  - `crates/rustycode-tui/src/app/service_polling.rs` — `poll_services()` drain logic with `MAX_STREAM_CHUNKS_PER_FRAME=8` and disconnected-channel cleanup
  - `crates/rustycode-tui/src/app/handlers/stream_core.rs` — StreamChunk handler to cover
  - `crates/rustycode-tui/src/app/renderer.rs` — `RendererState` snapshot pattern
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandEffect` — Dispatch variants to cover

  **WHY each reference matters**:
  - `service_polling.rs` contains the drain limits that MUST be preserved — characterization tests here catch regressions from any refactoring that touches drain ordering
  - `stream_core.rs` handles the most critical streaming path — tests here prevent silent behavioral changes during feature extraction
  - `renderer.rs` is the cleanest extraction pattern — tests validate the snapshot approach we'll replicate in `RenderCtx`

  **Acceptance Criteria**:
  - [ ] `cargo test -p rustycode-tui` test count ≥ 1,962 (baseline)
  - [ ] Tests exist covering `service_polling.rs` drain limits and disconnected-channel cleanup
  - [ ] Tests exist covering at least 3 `StreamChunk` variants in stream handlers
  - [ ] `cargo test -p rustycode-tui` → all pass (0 failures)
  - [ ] `cargo clippy -p rustycode-tui -- -D warnings` clean

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Characterization tests extend existing coverage
    Tool: Bash
    Steps:
      1. Run: grep -r "#\[test\]" crates/rustycode-tui/src/ | wc -l
      2. Assert: count >= 1962
      3. Run: cargo test -p rustycode-tui 2>&1 | tail -5
      4. Assert: output contains "test result: ok" with 0 failures
    Evidence: .sisyphus/evidence/task-1-test-coverage-baseline.txt

  Scenario: Existing behavior unchanged by test additions
    Tool: Bash
    Steps:
      1. Run: cargo build -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings
      2. Assert: both exit code 0
    Evidence: .sisyphus/evidence/task-1-build-unchanged.txt
  ```

  **Commit**: YES
  - Message: `test(tui): extend characterization tests for service_polling, stream_core, renderer — step 0.1`
  - Files: inline `#[cfg(test)]` modules in `service_polling.rs`, `handlers/stream_core.rs`, `renderer.rs`
  - Pre-commit: `cargo test -p rustycode-tui`

- [x] 2. Define TuiFeature Trait + Contexts + Feature Registry — **COMPLETE**

  **Final Status**: All types defined in `features/mod.rs` with 27 unit tests passing. Implementation deviated from plan in acceptable ways (see notes).

  **What was implemented**:
  - `SurfaceId`, `RouteId`, `ModalId` newtypes
  - `TuiEvent` with separate `Stream`/`Service` variants (GUARDRAIL-ASYNC-1 ✅)
  - `TuiAction` (Navigate, RequestFocus, OpenModal, CloseModal, StatusMessage, MarkDirty)
  - `UpdateCtx` (10 fields, FnMut callbacks for navigate/dispatch/approve)
  - `RenderCtx` (3 fields: frame_area, focused_surface, theme_colors)
  - `TuiFeature` trait (id, register, update, render)
  - `FeatureRegistry` (routes, commands, keymaps, surfaces with lookup/iteration)
  - 27 unit tests — ALL PASSING

  **Deviations from plan (all acceptable per Metis review)**:
  - `TuiAction::Command(CommandEffect)` → replaced with `StatusMessage` + `MarkDirty` (cleaner decoupling)
  - `TuiEvent::Input` → renamed to `TuiEvent::Key`; added `Resize`; no `Mouse` yet
  - `UpdateCtx` uses `FnMut` callbacks instead of direct field access
  - `FocusGained`/`FocusLost` don't carry `SurfaceId` (tracked by FocusRing instead)

  **Original plan details** (superseded by implementation):
  - Create new module: `crates/rustycode-tui/src/app/features/mod.rs`
  - Define core trait:
    ```rust
    /// A self-contained TUI feature module.
    /// Features own their state, handle events, and render to allocated surfaces.
    pub trait TuiFeature: Send + Sync + 'static {
        fn id(&self) -> &'static str;
        fn register(&self, reg: &mut FeatureRegistry);
        fn update(&mut self, event: &TuiEvent, ctx: &mut UpdateCtx) -> Vec<TuiAction>;
        fn render(&self, surface: SurfaceId, frame: &mut Frame, ctx: &RenderCtx);
    }
    ```
  - Define `UpdateCtx` — narrow mutable context providing:
    - Access to shared services (LLM provider, event bus sender)
    - Focus query (am I focused?)
    - Command dispatch (emit TuiAction)
    - Tool approval channel access (for approval feature)
    - Route navigation
  - Define `RenderCtx` — narrow immutable context providing:
    - Frame area allocation
    - Theme/styles access
    - Focus state (read-only)
  - Define `TuiEvent` enum wrapping the two existing channel types as separate variants:
    ```rust
    pub enum TuiEvent {
        Input(crossterm::event::KeyEvent),
        Mouse(crossterm::event::MouseEvent),
        StreamChunk(crate::app::async_::StreamChunk),    // LLM stream output — NOT merged with ServiceEvent
        ServiceEvent(rustycode_protocol::EventMsg),       // General service events — NOT merged with StreamChunk
        Tick,
        FocusGained(SurfaceId),
        FocusLost(SurfaceId),
    }
    ```
    **IMPORTANT**: Keep `StreamChunk` and `ServiceEvent` as separate variants. They drain from different channels and go to different feature handlers. Merging them would violate GUARDRAIL-ASYNC-1.
  - Define `TuiAction` enum for feature→shell communication:
    ```rust
    pub enum TuiAction {
        Navigate(RouteId),
        RequestFocus(SurfaceId),
        OpenModal(ModalId),
        CloseModal,
        Command(CommandEffect),  // Reuses existing type
        StatusMessage(String),
    }
    ```
  - Define `FeatureRegistry` containing:
    - `RouteRegistry` (route_id → feature_id)
    - `CommandRegistry` (command_name → feature_id + handler)
    - `KeymapRegistry` (key_binding → action)
    - `SurfaceRegistry` (surface_id → feature_id + area spec)
  - Define `SurfaceId` and `RouteId` as newtype wrappers around &'static str
  - Write unit tests for each struct/enum

  **Must NOT do**:
  - Do NOT wire into existing TUI yet (dual-path, AppShell comes in Task 4)
  - Do NOT merge `StreamChunk` and `ServiceEvent` into one variant — GUARDRAIL-ASYNC-1
  - Do NOT define a new event channel — `TuiEvent` wraps existing channel types, not a new channel
  - Do NOT make UpdateCtx/RenderCtx own any data — they borrow from AppShell
  - Do NOT add async methods to the trait

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`, `design-an-interface`]
    - `rust-patterns`: Idiomatic Rust trait design, newtype patterns, enum design
    - `design-an-interface`: Ensures we explore the interface shape properly

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 1 results for test patterns)
  - **Parallel Group**: Wave 0 (sequential after Task 1)
  - **Blocks**: Tasks 4, 5, 6, 7
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandContext` — **NEGATIVE example**: has ~20 borrowed fields, making it a god context. `UpdateCtx` must be narrower (≤10 fields), borrowing sub-struct references not raw TUI fields.
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandEffect` — Existing action enum to reuse. `TuiAction::Command(CommandEffect)` wraps it; do not replace it.
  - `crates/rustycode-tui/src/app/renderer.rs:RendererState` — **POSITIVE example**: immutable snapshot per frame. `RenderCtx` follows this exactly.
  - `~/dev/opencode/packages/plugin/src/tui.ts` — Opencode Component lifecycle (init/handle_events/update/render). Our TuiFeature mirrors this.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/state_model.rs` — 13 sub-struct definitions inform what `UpdateCtx` borrows (by sub-struct, not raw field).
  - `crates/rustycode-tui/src/app/async_.rs:StreamChunk` — LLM stream enum; becomes `TuiEvent::StreamChunk`.
  - `rustycode_protocol::EventMsg` — Service event enum; becomes `TuiEvent::ServiceEvent`. Keep separate from `StreamChunk`.

  **External References**:
  - Ratatui `Component` trait pattern: `handle_event() -> Option<Action>`, `render()`

  **WHY Each Reference Matters**:
  - `CommandContext` is the *anti-pattern* — study it to understand why `UpdateCtx` must be narrower
  - `CommandEffect` is load-bearing — features emit it via `TuiAction::Command`, AppShell handles dispatch
  - `RendererState` is the positive pattern — snapshot semantics are exactly right for `RenderCtx`
  - `StreamChunk` and `EventMsg` are the existing channel types wrapped as distinct `TuiEvent` variants

  **Acceptance Criteria**:
  - [ ] `crates/rustycode-tui/src/app/features/mod.rs` exists with TuiFeature trait
  - [ ] UpdateCtx, RenderCtx, TuiEvent, TuiAction, FeatureRegistry defined
  - [ ] SurfaceId, RouteId, ModalId defined as newtypes
  - [ ] Unit tests for each type compile and pass
  - [ ] `cargo clippy -p rustycode-tui -- -D warnings` clean

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Feature trait and contexts compile with correct signatures
    Tool: Bash
    Preconditions: features/mod.rs created with all types
    Steps:
      1. Run: cargo build -p rustycode-tui
      2. Assert: exit code 0
      3. Run: grep "pub trait TuiFeature" crates/rustycode-tui/src/app/features/mod.rs
      4. Assert: output shows trait definition with id, register, update, render methods
      5. Run: grep "pub struct UpdateCtx" crates/rustycode-tui/src/app/features/mod.rs
      6. Assert: output shows struct definition
      7. Run: grep "pub struct RenderCtx" crates/rustycode-tui/src/app/features/mod.rs
      8. Assert: output shows struct definition
    Expected Result: All types defined, compiles, clippy clean
    Failure Indicators: Missing types, compilation errors, clippy warnings
    Evidence: .sisyphus/evidence/task-2-trait-definitions.txt

  Scenario: TuiEvent has separate StreamChunk and ServiceEvent variants
    Tool: Bash
    Preconditions: TuiEvent enum defined
    Steps:
      1. Run: grep "StreamChunk\|ServiceEvent" crates/rustycode-tui/src/app/features/mod.rs
      2. Assert: both variants present as distinct enum arms (not merged)
      3. Run: grep -c "Sender<" crates/rustycode-tui/src/app/features/mod.rs
      4. Assert: count is 0 (no new channel definitions in features/mod.rs)
    Expected Result: TuiEvent has distinct variants for StreamChunk and ServiceEvent; no new channels defined
    Failure Indicators: Variants merged, new Sender/Receiver in features/mod.rs
    Evidence: .sisyphus/evidence/task-2-no-new-bus.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): define TuiFeature trait, contexts, and feature registry — step 0.2`
  - Files: `crates/rustycode-tui/src/app/features/mod.rs`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [x] 3. Map Handlers → (Feature, Fields Mutated) Extraction Contract — **COMPLETE**

  **Final Status**: `EXTRACTION_MAP.md` created mapping 29 handler functions to 13 feature domains. Committed in `81eef3d6a`.

  **What was done**:
  - Mapped all handler functions in `handlers/` directory to feature modules
  - Identified fields mutated and read per handler
  - Documented shared state and coupling hotspots
  - Written to `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md`

  **Original plan details** (superseded by implementation):
  - For each handler function in `crates/rustycode-tui/src/app/handlers/`, create a mapping document:
    ```
    Handler Function → Feature Module → Fields Mutated → Fields Read (immutable)
    ```
  - Use `ast_grep_search` to find all `self.session.*`, `self.panels.*`, `self.model.*`, `self.workspace.*`, `self.sys.*` patterns in each handler
  - Identify which handlers touch multiple feature domains (coupling hotspots)
  - Identify shared state: fields read/written by handlers belonging to different features
  - Document the extraction contract: for each feature, which handlers move, which fields become FeatureState
  - Write results to `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md`
  - This document is the migration blueprint — every subsequent task references it

  **Must NOT do**:
  - Do NOT change any code — this is documentation only
  - Do NOT start any extraction yet
  - Do NOT make assumptions about feature boundaries — map first, decide after

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`]
    - `rust-patterns`: Understanding ownership patterns in Rust

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 2)
  - **Parallel Group**: Wave 0
  - **Blocks**: Tasks 5, 12
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/handlers/` — All 10+ handler files. Every file here needs mapping.
  - `crates/rustycode-tui/src/app/service_polling.rs` — `poll_services()` drain+dispatch (NOT in event_loop.rs). Maps drain order and which handlers are called per frame.
  - `crates/rustycode-tui/src/app/event_loop.rs:handle_event()` — Input event routing. Maps mode→handler dispatch.
  - `crates/rustycode-tui/src/app/streaming/` — 7-file streaming module (adapter, events, mod, response, system_prompt, tool_detection, tool_execution). Map integration points in EXTRACTION_MAP.
  - `crates/rustycode-tui/src/app/pipeline/tui_integration.rs` — Pipeline↔TUI boundary. Map as AppShell service (not TuiFeature).
  - `crates/rustycode-tui/src/agents/` — 4-file agent module. Map which TUI fields it accesses.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/state_model.rs` — 13 sub-struct definitions that define the initial feature boundary candidates.

  **WHY Each Reference Matters**:
  - `handlers/` is the mutation surface — every handler that mutates god struct fields must be mapped
  - `service_polling.rs` determines drain ordering — extraction must preserve this exactly
  - `streaming/` and `pipeline/` are new since the original plan — they must be mapped to avoid incomplete extraction
  - `state_model.rs` sub-structs are the feature boundary candidates; each sub-struct likely maps to one feature

  **Acceptance Criteria**:
  - [ ] `EXTRACTION_MAP.md` exists with complete handler→feature mapping
  - [ ] Every handler function in handlers/ directory is mapped
  - [ ] Shared state (fields accessed by multiple features) is identified
  - [ ] Coupling hotspots (handlers touching 3+ feature domains) are flagged
  - [ ] `cargo build -p rustycode-tui` still passes (no code changed)

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Extraction map covers all handlers
    Tool: Bash
    Preconditions: EXTRACTION_MAP.md written
    Steps:
      1. Count handler files: find crates/rustycode-tui/src/app/handlers/ -name "*.rs" | wc -l
      2. Count mapped handlers in EXTRACTION_MAP.md: grep -c "→" EXTRACTION_MAP.md
      3. Assert: mapped count >= handler file count (each file at least referenced)
    Expected Result: Every handler file is accounted for in the extraction map
    Failure Indicators: Handler count > mapped count
    Evidence: .sisyphus/evidence/task-3-extraction-map-coverage.txt

  Scenario: Shared state identified
    Tool: Bash
    Preconditions: EXTRACTION_MAP.md written
    Steps:
      1. grep "SHARED STATE" EXTRACTION_MAP.md or grep "cross-feature" EXTRACTION_MAP.md
      2. Assert: at least some shared state documented (this IS the god struct problem)
    Expected Result: Shared state section exists with concrete field names
    Failure Indicators: No shared state documentation
    Evidence: .sisyphus/evidence/task-3-shared-state-identified.txt
  ```

  **Commit**: YES
  - Message: `docs(tui): add handler→feature extraction mapping — step 0.3`
  - Files: `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md`
  - Pre-commit: `cargo build -p rustycode-tui` (verify no code changed)

- [ ] 4. Create AppShell Shell Alongside Existing TUI

  **What to do**:
  - Create `crates/rustycode-tui/src/app/shell/mod.rs` with AppShell struct:
    ```rust
    pub struct AppShell {
        features: FeatureRegistry,  // From Task 2
        focus: FocusRing,           // New: tracks which feature is focused
        theme: Arc<Theme>,          // Shared theme reference
        terminal: Terminal,         // Terminal lifecycle (from existing TUI)
        event_rx: mpsc::Receiver<TuiEvent>,  // Drains from poll_services
    }
    ```
  - Create `crates/rustycode-tui/src/app/shell/focus.rs` with FocusRing:
    - Maintains ordered list of focusable surface IDs
    - `focus_next()` / `focus_prev()` / `focus_set(SurfaceId)`
    - `focused() -> Option<SurfaceId>`
  - AppShell implements a run loop that:
    1. Drains events from event_rx (single drain point — matches current poll_services pattern)
    2. Routes input events to focused feature
    3. Dispatches service events to all features
    4. Collects TuiAction responses
    5. Handles navigation/focus/modal actions
    6. Calls render on all registered features
  - Wire AppShell as a NEW entry point in `lib.rs` — existing TUI entry point UNTOUCHED
  - Add a feature flag or config option to switch between old TUI and new AppShell (for testing)
  - Write integration tests for FocusRing and event routing

  **Must NOT do**:
  - Do NOT modify existing TUI struct or event_loop.rs
  - Do NOT remove any existing functionality
  - Do NOT change poll_services() drain logic
  - Do NOT wire AppShell into production path yet — feature flag only

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`, `ratatui-patterns` (if available)]
    - `rust-patterns`: Arc sharing, trait objects, module organization

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 2 trait definitions)
  - **Parallel Group**: Wave 0 (after Task 2)
  - **Blocks**: Tasks 5, 6, 7, 8
  - **Blocked By**: Task 2

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/event_loop.rs:run()` — Current run loop. AppShell mirrors this structure but with feature dispatch instead of god-struct mutation.
  - `crates/rustycode-tui/src/app/service_polling.rs:poll_services()` — Current drain logic. AppShell's drain phase must match this exactly.
  - `crates/rustycode-tui/src/app/renderer.rs:render_frame()` — Current rendering. AppShell's render phase follows this pattern.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/features/mod.rs` — TuiFeature trait, UpdateCtx, RenderCtx (from Task 2)
  - `crates/rustycode-tui/src/lib.rs:132` — Current TUI construction. AppShell needs similar construction but with feature registry.

  **WHY Each Reference Matters**:
  - event_loop.rs run() is the exact loop AppShell replaces — mirror its structure
  - poll_services() drain ordering MUST be preserved — AppShell copies this ordering
  - lib.rs construction is the entry point where feature flag switches between old/new

  **Acceptance Criteria**:
  - [ ] `shell/mod.rs` exists with AppShell struct and run loop
  - [ ] `shell/focus.rs` exists with FocusRing
  - [ ] Feature flag in lib.rs switches between old TUI and AppShell
  - [ ] AppShell compiles and basic integration tests pass
  - [ ] `cargo clippy -p rustycode-tui -- -D warnings` clean
  - [ ] Existing TUI still works: `cargo run -p rustycode-cli -- tui` (using old path)

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: AppShell compiles alongside existing TUI
    Tool: Bash
    Preconditions: shell/mod.rs and shell/focus.rs created
    Steps:
      1. Run: cargo build -p rustycode-tui
      2. Assert: exit code 0
      3. Run: grep "pub struct AppShell" crates/rustycode-tui/src/app/shell/mod.rs
      4. Assert: struct found
      5. Run: grep "pub struct FocusRing" crates/rustycode-tui/src/app/shell/focus.rs
      6. Assert: struct found
    Expected Result: AppShell and FocusRing defined, crate compiles
    Failure Indicators: Compilation errors, missing types
    Evidence: .sisyphus/evidence/task-4-appshell-compiles.txt

  Scenario: Existing TUI still works (no regression)
    Tool: Bash
    Preconditions: AppShell added alongside TUI
    Steps:
      1. Run: cargo build -p rustycode-cli
      2. Assert: exit code 0
      3. Run: cargo test -p rustycode-tui
      4. Assert: all tests pass (including characterization tests from Task 1)
    Expected Result: No regressions from adding AppShell
    Failure Indicators: Build failures, test failures
    Evidence: .sisyphus/evidence/task-4-no-regression.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): add AppShell shell alongside existing TUI — step 0.4`
  - Files: `crates/rustycode-tui/src/app/shell/mod.rs`, `shell/focus.rs`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 5. Extract Plugin Manager State into FeatureState

  **What to do**:
  - Read `EXTRACTION_MAP.md` (from Task 3) to identify Plugin Manager's fields and handlers
  - Identify all Plugin Manager state in the god struct: look for fields like `plugin_manager`, `plugin_list`, `plugin_search`, marketplace-related state
  - Create `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs`
  - Define `PluginManagerState` struct containing all plugin-related fields extracted from TUI
  - Define `PluginManagerFeature` struct implementing `TuiFeature`
  - Move plugin-related handler functions from `handlers/` into the feature module
  - Plugin Manager is chosen as first extraction because it's self-contained: list/install/uninstall with no deep coupling to session streaming or workspace state
  - Update EXTRACTION_MAP.md to mark Plugin Manager fields as "extracted"

  **Must NOT do**:
  - Do NOT remove fields from TUI yet (Task 8 does this)
  - Do NOT modify the existing Plugin Manager behavior
  - Do NOT touch session, workspace, or streaming fields
  - Do NOT change channel architecture

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]
    - `rust-patterns`: Struct extraction, ownership transfer

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Tasks 2, 3, 4)
  - **Parallel Group**: Wave 1 (sequential first task)
  - **Blocks**: Task 6
  - **Blocked By**: Tasks 2, 3, 4

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/plugin/ui.rs` — Current Plugin Manager UI implementation. This is the primary file being extracted.
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract mapping Plugin Manager fields.
  - `crates/rustycode-tui/src/app/features/mod.rs:TuiFeature` — Trait definition being implemented.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/state_model.rs` — Plugin-related field groupings.
  - `crates/rustycode-tui/src/plugin/` — Plugin management service layer (NOT being modified, just referenced).

  **WHY Each Reference Matters**:
  - plugin/ui.rs IS the feature being extracted — it's the blueprint
  - EXTRACTION_MAP.md tells us exactly which fields move and which handlers
  - state_model.rs shows the current field grouping for Plugin Manager state

  **Acceptance Criteria**:
  - [ ] `features/plugin_manager/mod.rs` exists with PluginManagerState and PluginManagerFeature
  - [ ] PluginManagerFeature implements TuiFeature (id, register, update, render)
  - [ ] All plugin-related fields identified in EXTRACTION_MAP are in PluginManagerState
  - [ ] `cargo build -p rustycode-tui` succeeds
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Plugin Manager feature implements TuiFeature trait
    Tool: Bash
    Preconditions: plugin_manager/mod.rs created
    Steps:
      1. Run: grep "impl TuiFeature for PluginManagerFeature" crates/rustycode-tui/src/app/features/plugin_manager/mod.rs
      2. Assert: match found
      3. Run: grep "fn id\|fn register\|fn update\|fn render" crates/rustycode-tui/src/app/features/plugin_manager/mod.rs
      4. Assert: all 4 methods found
    Expected Result: Full TuiFeature implementation for Plugin Manager
    Failure Indicators: Missing trait methods, compilation errors
    Evidence: .sisyphus/evidence/task-5-plugin-feature-impl.txt

  Scenario: Build succeeds with new feature module
    Tool: Bash
    Preconditions: Plugin Manager feature module added
    Steps:
      1. Run: cargo build -p rustycode-tui
      2. Assert: exit code 0
      3. Run: cargo test -p rustycode-tui
      4. Assert: all tests pass
    Expected Result: No build regressions
    Failure Indicators: Compilation errors, test failures
    Evidence: .sisyphus/evidence/task-5-build-passes.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract Plugin Manager state into feature module — step 1.1`
  - Files: `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs`
  - Pre-commit: `cargo test -p rustycode-tui`

- [ ] 6. Implement TuiFeature for Plugin Manager

  **What to do**:
  - Implement `update()` method: handle plugin-related TuiEvents (search, install, uninstall, list navigation)
  - Implement `render()` method: render plugin list, search bar, install buttons to allocated surface
  - Implement `register()` method: register plugin route, plugin commands (e.g., `/plugin install`, `/plugin list`), plugin keybindings
  - Ensure update() returns TuiAction for commands that affect other features (e.g., status messages, navigation)
  - Write unit tests for each lifecycle method
  - Verify behavior matches existing Plugin Manager by comparing with characterization tests from Task 1

  **Must NOT do**:
  - Do NOT change the visual appearance of Plugin Manager
  - Do NOT add new Plugin Manager features
  - Do NOT modify plugin service layer (src/plugin/)
  - Do NOT access any TUI god struct fields

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`, `ratatui-patterns` (if available)]
    - `rust-patterns`: Trait implementation patterns
    - ratatui rendering patterns

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 5)
  - **Parallel Group**: Wave 1 (sequential after Task 5)
  - **Blocks**: Task 7
  - **Blocked By**: Task 5

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/plugin/ui.rs` — Current rendering and event handling. Mirror this behavior exactly.
  - `crates/rustycode-tui/src/app/features/mod.rs:UpdateCtx, RenderCtx` — Context types to use in update/render.
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandContext` — Existing context pattern for command registration.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs:PluginManagerFeature` — Struct being implemented.

  **WHY Each Reference Matters**:
  - plugin/ui.rs is the behavioral reference — feature must produce identical visual output and handle identical events
  - UpdateCtx/RenderCtx define the narrow API surface — must not leak god struct

  **Acceptance Criteria**:
  - [ ] All 4 TuiFeature methods implemented for PluginManagerFeature
  - [ ] Unit tests for update() with search, install, uninstall events
  - [ ] Unit tests for render() verifying widget tree structure
  - [ ] register() registers at least 1 route, 2 commands, and keybindings
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Plugin Manager feature handles all expected events
    Tool: Bash
    Preconditions: TuiFeature implemented for PluginManagerFeature
    Steps:
      1. Run: cargo test -p rustycode-tui -- plugin_manager
      2. Assert: tests pass, covering search/install/uninstall/navigation
      3. Run: cargo test -p rustycode-tui -- plugin_manager::render
      4. Assert: render test passes
    Expected Result: All plugin manager tests pass
    Failure Indicators: Test failures for plugin manager
    Evidence: .sisyphus/evidence/task-6-plugin-tests-pass.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): implement TuiFeature lifecycle for Plugin Manager — step 1.2`
  - Files: `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs`
  - Pre-commit: `cargo test -p rustycode-tui`

- [ ] 7. Wire Plugin Manager into AppShell + Verify

  **What to do**:
  - Register PluginManagerFeature with AppShell's FeatureRegistry
  - Wire AppShell to route Plugin Manager events to the feature module
  - Verify Plugin Manager works through AppShell path:
    - Use feature flag to switch to AppShell
    - Launch TUI
    - Open Plugin Manager (same trigger as before)
    - Verify: list loads, search works, install/uninstall works
  - Compare behavior with old TUI path (characterization tests from Task 1)
  - If behavioral differences found, fix in feature module (NOT in old code)

  **Must NOT do**:
  - Do NOT remove old Plugin Manager code yet (Task 8)
  - Do NOT modify AppShell infrastructure — only wire the feature
  - Do NOT change event routing for other features

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 6)
  - **Parallel Group**: Wave 1 (sequential after Task 6)
  - **Blocks**: Task 8
  - **Blocked By**: Task 6

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/shell/mod.rs:AppShell` — Host to wire feature into.
  - `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs` — Feature being wired.

  **Acceptance Criteria**:
  - [ ] PluginManagerFeature registered with AppShell
  - [ ] Plugin Manager route registered and accessible
  - [ ] Plugin Manager commands registered
  - [ ] AppShell routes Plugin Manager events correctly
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] Plugin Manager works through AppShell path (feature flag enabled)

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Plugin Manager works through AppShell
    Tool: interactive_bash (tmux)
    Preconditions: AppShell feature flag enabled, TUI compiled
    Steps:
      1. Start TUI: cargo run -p rustycode-cli -- tui
      2. Navigate to Plugin Manager (same keybinding as before)
      3. Verify plugin list renders
      4. Type search query
      5. Verify list filters
      6. Exit Plugin Manager
      7. Verify normal TUI operation continues
    Expected Result: Plugin Manager fully functional through AppShell path
    Failure Indicators: Plugin list empty, search not filtering, navigation broken
    Evidence: .sisyphus/evidence/task-7-plugin-appshell-works.txt

  Scenario: Old TUI path still works (dual-path verification)
    Tool: Bash
    Preconditions: Both paths available
    Steps:
      1. Disable AppShell feature flag
      2. Run: cargo run -p rustycode-cli -- tui
      3. Navigate to Plugin Manager
      4. Verify identical behavior
    Expected Result: Old path still works perfectly
    Failure Indicators: Regression in old TUI path
    Evidence: .sisyphus/evidence/task-7-old-path-still-works.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): wire Plugin Manager feature into AppShell — step 1.3`
  - Files: `crates/rustycode-tui/src/app/shell/mod.rs`, feature registration code
  - Pre-commit: `cargo test -p rustycode-tui`

- [ ] 8. Remove Extracted Plugin Manager Code from God Struct

  **What to do**:
  - Remove Plugin Manager fields from TUI struct in event_loop.rs
  - Remove Plugin Manager handling from handle_event() match arms
  - Remove Plugin Manager rendering from render path
  - Remove Plugin Manager handlers from handlers/ directory (now in feature module)
  - Update state_model.rs to remove Plugin Manager field groupings
  - Update EXTRACTION_MAP.md: mark Plugin Manager as "COMPLETED"
  - Make AppShell the default path for Plugin Manager (feature flag on by default)
  - Verify: cargo build/test/clippy/fmt all pass
  - Verify: `cargo run -p rustycode-cli -- tui` works with Plugin Manager through AppShell

  **Must NOT do**:
  - Do NOT remove the feature flag yet (keep for rollback)
  - Do NOT extract any other features (that's Wave 3)
  - Do NOT change any non-Plugin-Manager code in event_loop.rs

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 7)
  - **Parallel Group**: Wave 1 (final task)
  - **Blocks**: Tasks 9, 10, 11, 12, 13-18
  - **Blocked By**: Task 7

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Shows exactly which fields/handlers to remove.
  - `crates/rustycode-tui/src/app/event_loop.rs` — God struct to clean up.
  - `crates/rustycode-tui/src/app/state_model.rs` — Field groupings to update.

  **Acceptance Criteria**:
  - [ ] No Plugin Manager fields remain in TUI struct
  - [ ] No Plugin Manager handling in handle_event() match arms
  - [ ] No Plugin Manager rendering in render path
  - [ ] Plugin Manager handlers removed from handlers/ (now in features/plugin_manager/)
  - [ ] EXTRACTION_MAP.md updated: Plugin Manager marked COMPLETED
  - [ ] `cargo build -p rustycode-tui` succeeds
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] `cargo clippy -p rustycode-tui -- -D warnings` clean
  - [ ] `cargo run -p rustycode-cli -- tui` works

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Plugin Manager fields removed from god struct
    Tool: Bash
    Preconditions: Extraction complete
    Steps:
      1. Run: grep -i "plugin" crates/rustycode-tui/src/app/event_loop.rs | grep -v "// " | grep -v "plugin_manager_feature" | head -20
      2. Assert: no plugin state fields in TUI struct (only AppShell wiring may reference it)
      3. Run: wc -l crates/rustycode-tui/src/app/event_loop.rs
      4. Note: line count should have decreased from baseline
    Expected Result: Plugin Manager state removed from god struct
    Failure Indicators: Plugin fields still in TUI struct
    Evidence: .sisyphus/evidence/task-8-plugin-fields-removed.txt

  Scenario: Full TUI still works after extraction
    Tool: Bash
    Preconditions: Plugin Manager fully extracted
    Steps:
      1. Run: cargo build -p rustycode-tui && cargo test -p rustycode-tui
      2. Assert: build succeeds, all tests pass
      3. Run: cargo clippy -p rustycode-tui -- -D warnings
      4. Assert: no warnings
    Expected Result: Clean build, all tests pass, no warnings
    Failure Indicators: Build failures, test failures, clippy warnings
    Evidence: .sisyphus/evidence/task-8-full-verification.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): remove Plugin Manager from god struct, AppShell is default — step 1.4`
  - Files: `event_loop.rs`, `state_model.rs`, `handlers/` cleanup
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 9. Extract Rendering into Feature-Aware Dispatch

  **What to do**:
  - Refactor AppShell's render phase to dispatch to registered features instead of the monolithic renderer
  - Create a `RenderDispatch` struct that:
    - Queries FeatureRegistry for all registered surfaces
    - Allocates frame areas to surfaces based on current layout
    - Calls `feature.render(surface, frame, &render_ctx)` for each visible feature
  - Port the layout computation from current `renderer.rs` into RenderDispatch:
    - Main area splits (conversation, input, sidebar)
    - Overlay/modal layer rendering
    - Status bar rendering
  - Keep the existing `RendererState` snapshot pattern — RenderCtx is built from a snapshot, not from live god struct
  - Features render into their allocated surface; AppShell handles the frame commit
  - Write tests for RenderDispatch: verify surface allocation, verify feature render ordering

  **Must NOT do**:
  - Do NOT change visual appearance of any rendered element
  - Do NOT modify individual feature rendering code (only the dispatch mechanism)
  - Do NOT change the RendererState snapshot pattern
  - Do NOT make render() async or add block_on() in render paths

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 10)
  - **Parallel Group**: Wave 2
  - **Blocks**: F1-F4
  - **Blocked By**: Task 8

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/renderer.rs` — Current monolithic renderer. RenderDispatch mirrors the layout computation but dispatches to features instead of rendering everything.
  - `crates/rustycode-tui/src/app/renderer.rs:RendererState` — Snapshot pattern to preserve.
  - `crates/rustycode-tui/src/app/shell/mod.rs:AppShell` — Host that gets the RenderDispatch.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/features/mod.rs:RenderCtx, SurfaceId` — Types for feature rendering.
  - `crates/rustycode-tui/src/app/shell/focus.rs:FocusRing` — Focus state for render context.

  **WHY Each Reference Matters**:
  - renderer.rs IS the rendering logic being decomposed — it defines the layout and surface allocation
  - RendererState is the proven snapshot pattern — must be preserved
  - RenderCtx is the narrow context features receive — must contain everything needed for rendering

  **Acceptance Criteria**:
  - [ ] `RenderDispatch` struct created in shell/render_dispatch.rs
  - [ ] Surface allocation logic ported from renderer.rs
  - [ ] Feature render dispatching works (Plugin Manager renders correctly)
  - [ ] RendererState snapshot pattern preserved
  - [ ] No block_on() in any render path
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] `cargo run -p rustycode-cli -- tui` renders identically

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Feature-aware render dispatch produces identical output
    Tool: interactive_bash (tmux)
    Preconditions: RenderDispatch wired into AppShell
    Steps:
      1. Start TUI: cargo run -p rustycode-cli -- tui
      2. Navigate through: conversation view, Plugin Manager, help overlay, command palette
      3. Verify each view renders correctly (no missing elements, no layout shifts)
      4. Take screenshot at each view
    Expected Result: Visually identical to pre-refactoring TUI
    Failure Indicators: Missing UI elements, layout shifts, blank areas
    Evidence: .sisyphus/evidence/task-9-render-dispatch-identical.txt

  Scenario: No block_on in render paths
    Tool: Bash
    Preconditions: RenderDispatch implemented
    Steps:
      1. Run: grep -n "block_on" crates/rustycode-tui/src/app/shell/render_dispatch.rs
      2. Assert: no matches found
      3. Run: grep -rn "block_on" crates/rustycode-tui/src/app/features/*/mod.rs | grep "fn render"
      4. Assert: no matches in render methods
    Expected Result: Zero block_on calls in any render path
    Failure Indicators: block_on found in render_dispatch or feature render methods
    Evidence: .sisyphus/evidence/task-9-no-block-on-render.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract rendering into feature-aware dispatch — step 2.1`
  - Files: `crates/rustycode-tui/src/app/shell/render_dispatch.rs`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 10. Unify Command Dispatch via CommandContext/CommandEffect

  **What to do**:
  - Extend the existing `CommandContext`/`CommandEffect` pattern in `commands/mod.rs` to become the single command dispatch mechanism for AppShell
  - Create `CommandDispatch` struct that:
    - Accepts command invocations from: slash commands, keyboard shortcuts, overlay toggles, service-triggered UI changes
    - Routes to the appropriate feature module via FeatureRegistry
    - Returns CommandEffect results to the caller
  - Converge three current dispatch paths:
    1. Slash commands (from input handler) → currently handled inline in event_loop
    2. Keyboard shortcuts (from handle_event) → currently mode-based match arms
    3. Service-triggered UI changes (from poll_services) → currently direct field mutation
  - Into one: all go through CommandDispatch → FeatureRegistry → feature.update()
  - Map existing slash commands to CommandRegistry entries
  - Map existing keyboard shortcuts to KeymapRegistry entries
  - Write tests for CommandDispatch routing

  **Must NOT do**:
  - Do NOT remove existing commands or change their behavior
  - Do NOT change keyboard shortcut bindings
  - Do NOT modify rustycode-orchestration boundary

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 9)
  - **Parallel Group**: Wave 2
  - **Blocks**: Task 11
  - **Blocked By**: Task 8

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/commands/mod.rs` — Existing CommandContext/CommandEffect pattern. EXTEND this, don't replace it.
  - `crates/rustycode-tui/src/app/event_loop.rs:handle_event()` — Current mode-based dispatch (lines ~1357+). This is what gets unified.
  - `crates/rustycode-tui/src/app/features/mod.rs:FeatureRegistry, CommandRegistry, KeymapRegistry` — Registry types from Task 2.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/features/mod.rs:TuiAction::Command(CommandEffect)` — Action type that bridges feature→shell command dispatch.

  **WHY Each Reference Matters**:
  - commands/mod.rs is the best seed crystal — it already has the right shape, just needs to become the canonical dispatch instead of one-of-many
  - handle_event() match arms show what needs to be converted to registry lookups
  - FeatureRegistry/CommandRegistry are the data structures that replace match-arm dispatch

  **Acceptance Criteria**:
  - [ ] `CommandDispatch` struct created in shell/command_dispatch.rs
  - [ ] Slash commands route through CommandDispatch
  - [ ] Keyboard shortcuts route through KeymapRegistry → CommandDispatch
  - [ ] No inline command handling in AppShell run loop (all through registry)
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] All existing commands still work (slash commands, shortcuts)

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: All slash commands still work through unified dispatch
    Tool: interactive_bash (tmux)
    Preconditions: CommandDispatch wired in
    Steps:
      1. Start TUI
      2. Type /help — verify help overlay appears
      3. Type /plugin — verify plugin manager appears
      4. Type /model — verify model selector appears
      5. Test 3-5 more slash commands
    Expected Result: All slash commands produce identical behavior to pre-refactoring
    Failure Indicators: Commands not recognized, wrong behavior, errors
    Evidence: .sisyphus/evidence/task-10-slash-commands-work.txt

  Scenario: Keyboard shortcuts route through registry
    Tool: Bash
    Preconditions: KeymapRegistry populated
    Steps:
      1. Run: grep -c "KeymapEntry\|Keybinding" crates/rustycode-tui/src/app/shell/command_dispatch.rs
      2. Assert: >0 (keybindings registered through registry)
      3. Run: cargo test -p rustycode-tui -- command_dispatch
      4. Assert: routing tests pass
    Expected Result: Keybindings registered as data, not match arms
    Failure Indicators: No keybinding registrations, test failures
    Evidence: .sisyphus/evidence/task-10-keymap-registry.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): unify command dispatch through CommandContext/CommandEffect — step 2.2`
  - Files: `crates/rustycode-tui/src/app/shell/command_dispatch.rs`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 11. Wire Tool Approval Back-Channel Through CommandEffect

  **What to do**:
  - Model tool approval as a `CommandEffect` variant (e.g., `CommandEffect::ToolApproved` / `CommandEffect::ToolRejected`)
  - Extend `UpdateCtx` to provide access to `approval_tx` sender when tool approval is pending
  - When the tool approval feature module calls `update()` with a tool approval event:
    - Feature renders approval UI (already exists)
    - User approves/rejects → feature returns `TuiAction::Command(CommandEffect::ToolApproved/Rejected)`
    - AppShell routes the CommandEffect to the streaming thread via approval_tx
  - This replaces the current direct `self.approval_tx.send()` calls in the god struct
  - Write tests verifying the approval flow through the new path

  **Must NOT do**:
  - Do NOT remove approval_tx/approval_rx channels
  - Do NOT change the streaming thread's blocking behavior on approval_rx
  - Do NOT make approval async — it must remain synchronous from streaming thread's perspective

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Task 10)
  - **Parallel Group**: Wave 2 (after Task 10)
  - **Blocks**: F1-F4
  - **Blocked By**: Task 10

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandEffect` — Existing enum to extend with tool approval variants.
  - `crates/rustycode-tui/src/app/features/mod.rs:UpdateCtx` — Context that needs approval_tx access.
  - `crates/rustycode-tui/src/app/event_loop.rs` — Search for `approval_tx` to find current usage patterns.

  **API/Type References**:
  - `std::sync::mpsc::Sender` — The approval_tx type. UpdateCtx borrows it.

  **WHY Each Reference Matters**:
  - approval_tx is a synchronous back-channel that the streaming thread blocks on — breaking it breaks all tool execution
  - CommandEffect is the canonical action type — tool approval is just another command effect
  - UpdateCtx is the narrow API surface — features must access approval through this, not god struct

  **Acceptance Criteria**:
  - [ ] `CommandEffect::ToolApproved` / `CommandEffect::ToolRejected` variants added
  - [ ] `UpdateCtx` provides access to `approval_tx` (borrowed, not owned)
  - [ ] Tool approval flow tested: feature → TuiAction::Command → AppShell → approval_tx → streaming thread
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] Tool approval still works end-to-end in live TUI

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Tool approval flows through CommandEffect
    Tool: Bash
    Preconditions: Tool approval wired through CommandEffect
    Steps:
      1. Run: grep "ToolApproved\|ToolRejected" crates/rustycode-tui/src/app/commands/mod.rs
      2. Assert: variants found in CommandEffect enum
      3. Run: grep "approval_tx" crates/rustycode-tui/src/app/features/mod.rs
      4. Assert: UpdateCtx provides access to approval sender
      5. Run: cargo test -p rustycode-tui -- tool_approval
      6. Assert: approval flow tests pass
    Expected Result: Tool approval modeled as CommandEffect, accessible through UpdateCtx
    Failure Indicators: Missing variants, no UpdateCtx access, test failures
    Evidence: .sisyphus/evidence/task-11-tool-approval-wired.txt

  Scenario: Tool approval back-channel not broken
    Tool: interactive_bash (tmux)
    Preconditions: Full TUI running with tool approval wired
    Steps:
      1. Start TUI, trigger a tool that requires approval
      2. Verify approval prompt appears
      3. Approve the tool
      4. Verify tool executes
    Expected Result: Tool approval works end-to-end
    Failure Indicators: Approval prompt doesn't appear, tool hangs, approval not sent
    Evidence: .sisyphus/evidence/task-11-tool-approval-e2e.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): wire tool approval through CommandEffect — step 2.3`
  - Files: `crates/rustycode-tui/src/app/shell/command_dispatch.rs`, `features/mod.rs`
  - Pre-commit: `cargo test -p rustycode-tui`

- [ ] 12. Route poll_services() Output Through Feature UpdateCtx

  **What to do**:
  - This is the hardest refactoring task. The goal: poll_services() drains async channels and feeds results to feature modules via UpdateCtx instead of mutating god struct fields directly.
  - Step 1: Identify every handler called from poll_services():
    - `handle_stream_chunk()` → mutates session.streaming.*, session.messages, session.active_tools, panels.tool_panel, panels.tool_approval, model.token_budget, workspace.*, sys.dirty
    - `handle_tool_result()` → mutates panels.tool_panel
    - Other handlers in the drain loop
  - Step 2: For each handler, convert from `fn(&mut self, event)` to `fn(&mut FeatureState, ctx: &mut UpdateCtx)`:
    - Create per-feature event types (e.g., `SessionEvent`, `ToolPanelEvent`)
    - Convert handler body: `self.session.streaming.*` → `self.streaming.*` (operating on FeatureState)
    - Route cross-feature mutations through UpdateCtx/CommandEffect
  - Step 3: AppShell's drain phase:
    - Drain channels (preserving existing order)
    - Convert raw events to feature-specific events
    - Dispatch to feature.update() with UpdateCtx
    - Collect TuiAction responses
  - Keep poll_services() drain ordering EXACTLY the same
  - This task likely exceeds 500 lines — split into sub-commits per handler conversion

  **Must NOT do**:
  - Do NOT change drain ordering
  - Do NOT change BoundedChannel capacities
  - Do NOT consolidate channels
  - Do NOT make features drain their own channels (AppShell owns drain)
  - Do NOT channel-ify session_mode.rs or mcp_mode.rs

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Tasks 8 and 3)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 13-18
  - **Blocked By**: Tasks 8, 3

  **References**:

  **Pattern References**:
  - `crates/rustycode-tui/src/app/service_polling.rs` — `poll_services()` lives here (NOT in event_loop.rs). AppShell's drain phase MUST match this exact drain ordering and limits.
  - `crates/rustycode-tui/src/app/handlers/stream_core.rs` — StreamChunk handler. Must convert to feature-specific event handling.
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Maps handlers to features and fields mutated.

  **API/Type References**:
  - `crates/rustycode-tui/src/app/features/mod.rs:UpdateCtx` — Context features receive during update.
  - `crates/rustycode-tui/src/app/features/mod.rs:TuiEvent` — Both `StreamChunk` and `ServiceEvent` variants (kept separate).

  **WHY Each Reference Matters**:
  - `service_polling.rs` is the authoritative drain loop — AppShell's drain phase must mirror it exactly
  - `handlers/` contain the mutation logic — every `self.session.*` access must become `FeatureState` access
  - EXTRACTION_MAP.md is the blueprint for which handlers move to which features

  **Acceptance Criteria**:
  - [ ] poll_services() drain ordering preserved (verified by test)
  - [ ] At least 2 handler functions converted to feature-module style
  - [ ] AppShell drain phase dispatches events to features via UpdateCtx
  - [ ] No direct god struct mutation in converted handlers
  - [ ] BoundedChannel capacities unchanged
  - [ ] `cargo test -p rustycode-tui` passes
  - [ ] `cargo run -p rustycode-cli -- tui` works with streaming conversation

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Drain ordering preserved
    Tool: Bash
    Preconditions: AppShell drain phase implemented
    Steps:
      1. Run: grep "try_recv\|recv_timeout" crates/rustycode-tui/src/app/shell/mod.rs | head -20
      2. Assert: drain calls appear in same order as original poll_services()
      3. Run: cargo test -p rustycode-tui -- drain_ordering
      4. Assert: drain ordering test passes
    Expected Result: Drain order matches original poll_services() exactly
    Failure Indicators: Different ordering, missing drains, test failure
    Evidence: .sisyphus/evidence/task-12-drain-ordering-preserved.txt

  Scenario: Streaming conversation works through feature dispatch
    Tool: interactive_bash (tmux)
    Preconditions: AppShell handling service events
    Steps:
      1. Start TUI: cargo run -p rustycode-cli -- tui
      2. Send a message to start a conversation
      3. Verify streaming tokens appear
      4. Verify tool execution triggers and renders in tool panel
      5. Verify conversation completes normally
    Expected Result: Full conversation flow works identically
    Failure Indicators: Tokens not streaming, tool panel not updating, conversation hanging
    Evidence: .sisyphus/evidence/task-12-streaming-works.txt
  ```

  **Commit**: YES (split into sub-commits if >500 lines)
  - Message: `refactor(tui): route poll_services through feature UpdateCtx — step 2.4`
  - Files: `crates/rustycode-tui/src/app/shell/mod.rs`, handler files, feature modules
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 13. Extract Help/CommandPalette Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for Help and CommandPalette fields/handlers
  - Create `features/help/mod.rs` with HelpFeature implementing TuiFeature
  - Create `features/command_palette/mod.rs` with CommandPaletteFeature implementing TuiFeature
  - May combine into one feature if they share significant state; separate if not
  - Move help overlay rendering, command palette search/filter logic into feature modules
  - Register help route, help keybinding (e.g., `?` or `F1`), command palette keybinding
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct
  - Verify: help overlay renders identically, command palette search/filter works

  **Must NOT do**:
  - Do NOT change help content or command palette search behavior
  - Do NOT modify other features' code

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 14, 15, 16, 17, 18)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/app/state_model.rs:HelpState` — Current help state grouping
  - `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs` — Completed feature extraction to use as pattern

  **Acceptance Criteria**:
  - [ ] HelpFeature and/or CommandPaletteFeature implement TuiFeature
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] God struct fields removed
  - [ ] Help overlay renders identically
  - [ ] Command palette search/filter works
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Help overlay renders correctly through feature
    Tool: interactive_bash (tmux)
    Preconditions: Help feature wired into AppShell
    Steps:
      1. Start TUI
      2. Press help keybinding (?)
      3. Verify help overlay appears with all keybinding entries
      4. Press help keybinding again
      5. Verify help overlay closes
    Expected Result: Help overlay identical to pre-refactoring
    Failure Indicators: Missing entries, layout issues, doesn't open/close
    Evidence: .sisyphus/evidence/task-13-help-overlay.txt

  Scenario: Command palette search works through feature
    Tool: interactive_bash (tmux)
    Preconditions: Command palette feature wired
    Steps:
      1. Start TUI
      2. Open command palette (Ctrl+K or equivalent)
      3. Type search query
      4. Verify list filters
      5. Select an entry
      6. Verify command executes
    Expected Result: Command palette search and execution work identically
    Failure Indicators: Search not filtering, command not executing
    Evidence: .sisyphus/evidence/task-13-command-palette.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract Help and CommandPalette features — step 3.1`
  - Files: `features/help/mod.rs`, `features/command_palette/mod.rs`, god struct cleanup
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 14. Extract FileSelector Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for FileSelector fields/handlers
  - Create `features/file_selector/mod.rs` with FileSelectorFeature implementing TuiFeature
  - Move file browsing, selection, filtering logic into feature module
  - Register file selector route and keybinding
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct
  - Verify: file selector opens, browses directories, selects files

  **Must NOT do**:
  - Do NOT change file browsing behavior or filtering logic

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 13, 15, 16, 17, 18)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/app/state_model.rs` — FileSelector-related state groupings
  - `crates/rustycode-tui/src/app/features/plugin_manager/mod.rs` — Pattern reference

  **Acceptance Criteria**:
  - [ ] FileSelectorFeature implements TuiFeature
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] God struct fields removed
  - [ ] File selector opens, browses, selects files
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: File selector works through feature
    Tool: interactive_bash (tmux)
    Preconditions: FileSelector feature wired
    Steps:
      1. Start TUI
      2. Open file selector (keybinding)
      3. Browse directory tree
      4. Filter by name
      5. Select a file
      6. Verify file content loads in editor
    Expected Result: File selection works identically
    Failure Indicators: Can't browse, filter broken, file not loading
    Evidence: .sisyphus/evidence/task-14-file-selector.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract FileSelector feature — step 3.2`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 15. Extract Search Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for Search fields/handlers
  - Create `features/search/mod.rs` with SearchFeature implementing TuiFeature
  - Move search input, result rendering, navigation logic into feature module
  - Register search route and keybinding (e.g., `/`)
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct
  - Verify: search input works, results render, navigation jumps to results

  **Must NOT do**:
  - Do NOT change search behavior or result ordering

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 13, 14, 16, 17, 18)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/app/state_model.rs` — Search-related state

  **Acceptance Criteria**:
  - [ ] SearchFeature implements TuiFeature
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] God struct fields removed
  - [ ] Search input, results, navigation work
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Search works through feature
    Tool: interactive_bash (tmux)
    Preconditions: Search feature wired
    Steps:
      1. Start TUI
      2. Open search (keybinding)
      3. Type search query
      4. Verify results appear
      5. Navigate through results
      6. Verify jumping to result location works
    Expected Result: Search works identically to pre-refactoring
    Failure Indicators: No results, navigation broken
    Evidence: .sisyphus/evidence/task-15-search.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract Search feature — step 3.3`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 16. Extract MCP Panel Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for MCP panel fields/handlers
  - Create `features/mcp_panel/mod.rs` with McpPanelFeature implementing TuiFeature
  - Move MCP server status, server management, tool rendering into feature module
  - Register MCP panel route and keybinding
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct
  - NOTE: mcp_mode.rs bypasses channel architecture — feature module wraps existing logic, channel-ification is explicitly deferred (GUARDRAIL-ASYNC-6)
  - Verify: MCP panel shows server status, tool calls render

  **Must NOT do**:
  - Do NOT channel-ify mcp_mode.rs (GUARDRAIL-ASYNC-6)
  - Do NOT change MCP server management behavior

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 13, 14, 15, 17, 18)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/services/mcp_mode.rs` — Current MCP mode logic (bypasses channels)
  - `crates/rustycode-tui/src/app/state_model.rs` — MCP-related state

  **Acceptance Criteria**:
  - [ ] McpPanelFeature implements TuiFeature
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] God struct fields removed
  - [ ] MCP panel shows server status, tool calls render
  - [ ] mcp_mode.rs NOT channel-ified (still wraps existing logic)
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: MCP panel renders through feature
    Tool: interactive_bash (tmux)
    Preconditions: MCP panel feature wired
    Steps:
      1. Start TUI with MCP servers configured
      2. Open MCP panel
      3. Verify server status shows
      4. Trigger a tool call through MCP
      5. Verify tool call renders in panel
    Expected Result: MCP panel works identically
    Failure Indicators: Missing servers, tool calls not rendering
    Evidence: .sisyphus/evidence/task-16-mcp-panel.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract MCP panel feature — step 3.4`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 17. Extract Session Streaming Feature

  **What to do**:
  - This is the most complex feature extraction. Reference EXTRACTION_MAP.md for session streaming fields/handlers.
  - Create `features/session/mod.rs` with SessionFeature implementing TuiFeature
  - Move conversation rendering, message streaming, token counting, active tools display into feature module
  - This feature handles `StreamChunk`/`EventMsg` events from poll_services()
  - Register session route (default/main route), session-related commands
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct
  - NOTE: session_mode.rs bypasses channel architecture — feature module wraps existing logic, channel-ification is explicitly deferred (GUARDRAIL-ASYNC-6)
  - Verify: conversation streaming works, messages render, token counting works

  **Must NOT do**:
  - Do NOT channel-ify session_mode.rs (GUARDRAIL-ASYNC-6)
  - Do NOT change streaming behavior, message rendering, or token counting
  - Do NOT merge `StreamChunk` and `ServiceEvent` events — GUARDRAIL-ASYNC-1
  - Do NOT modify `streaming/adapter.rs` — GUARDRAIL-AB-3
  - Do NOT refactor `streaming/` internals — GUARDRAIL-SCOPE-6

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 13, 14, 15, 16, 18)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/services/session_mode.rs` — Current session mode logic (bypasses channels)
  - `crates/rustycode-tui/src/app/handlers/stream_core.rs` — StreamChunk handling
  - `crates/rustycode-tui/src/app/state_model.rs` — Session-related state (session.streaming.*, session.messages, session.active_tools)

  **Acceptance Criteria**:
  - [ ] SessionFeature implements TuiFeature
  - [ ] Registered with AppShell FeatureRegistry as default route
  - [ ] God struct session fields removed
  - [ ] Conversation streaming works end-to-end
  - [ ] Message rendering identical
  - [ ] Token counting works
  - [ ] session_mode.rs NOT channel-ified
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Full conversation streaming through feature
    Tool: interactive_bash (tmux)
    Preconditions: Session feature wired, LLM provider configured
    Steps:
      1. Start TUI
      2. Send a message
      3. Verify streaming tokens appear character by character
      4. Verify message completes with full response
      5. Send another message to verify multi-turn
      6. Trigger a tool call and verify tool execution renders
    Expected Result: Full conversation flow identical to pre-refactoring
    Failure Indicators: Tokens not streaming, messages not completing, tool calls failing
    Evidence: .sisyphus/evidence/task-17-session-streaming.txt

  Scenario: Token counting preserved
    Tool: Bash
    Preconditions: Session feature extracted
    Steps:
      1. Run: cargo test -p rustycode-tui -- token_budget
      2. Assert: token counting tests pass
    Expected Result: Token budget tracking works correctly
    Failure Indicators: Token count wrong, budget exceeded incorrectly
    Evidence: .sisyphus/evidence/task-17-token-counting.txt
  ```

  **Commit**: YES (split into sub-commits if >500 lines)
  - Message: `refactor(tui): extract Session streaming feature — step 3.5`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 18. Extract Theme/Settings Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for Theme and Settings fields/handlers
  - Create `features/theme/mod.rs` with ThemeFeature implementing TuiFeature
  - Move theme data, style computation, settings management into feature module
  - Register theme-related commands (e.g., `/theme`, `/set`)
  - Wire into AppShell FeatureRegistry
  - Make theme accessible to other features via RenderCtx (read-only access to Arc<Theme>)
  - Remove extracted fields/handlers from god struct
  - Verify: theme switching works, styles apply correctly to all features

  **Must NOT do**:
  - Do NOT change theme values or style computation
  - Do NOT make features depend on ThemeFeature directly — use RenderCtx

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
  - **Skills**: [`rust-patterns`]

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 13, 14, 15, 16, 17)
  - **Parallel Group**: Wave 3
  - **Blocks**: F1-F4
  - **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/features/EXTRACTION_MAP.md` — Extraction contract
  - `crates/rustycode-tui/src/app/state_model.rs` — Theme/settings-related state

  **Acceptance Criteria**:
  - [ ] ThemeFeature implements TuiFeature
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] Theme accessible via RenderCtx for all features
  - [ ] God struct theme fields removed
  - [ ] Theme switching works
  - [ ] Styles apply correctly to all features
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Theme switching works through feature
    Tool: interactive_bash (tmux)
    Preconditions: Theme feature wired
    Steps:
      1. Start TUI
      2. Switch theme (e.g., /theme dark or /theme light)
      3. Verify all UI elements update to new theme
      4. Switch back
      5. Verify original theme restored
    Expected Result: Theme switching works identically
    Failure Indicators: Partial theme application, style glitches
    Evidence: .sisyphus/evidence/task-18-theme-switching.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract Theme/Settings feature — step 3.6`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 19. Extract TeamMode/WorkerPanel/AgentManager Feature

  **What to do**:
  - Reference EXTRACTION_MAP.md for TeamMode fields/handlers
  - Create `features/team_mode/mod.rs` with `TeamModeFeature` implementing `TuiFeature`
  - State: `team.team_panel`, `team.team_handler`, `team.worker_panel`, `team.agent_manager`
  - Update: handle `TuiEvent::ServiceEvent(EventMsg::Team*)` variants; team command dispatch (`/team`, CancelTeam CommandEffect)
  - Render: team panel overlay, worker panel
  - The `agents/` module (delegation_executor, definitions, agent_tool) is used internally by `team_handler` — do NOT restructure it, just keep the reference
  - Wire into AppShell FeatureRegistry
  - Remove extracted fields/handlers from god struct

  **Must NOT do**:
  - Do NOT refactor `agents/` internals — wire team_handler as-is
  - Do NOT change TeamEvent semantics

  **Recommended Agent Profile**: `deep` (TeamModeHandler has significant complexity)
  **Blocked By**: Tasks 8, 12

  **References**:
  - `crates/rustycode-tui/src/app/state_model.rs:TeamModeState` — Fields to extract
  - `crates/rustycode-tui/src/app/team_mode_handler.rs` — TeamModeHandler implementation
  - `crates/rustycode-tui/src/ui/team_panel.rs`, `ui/worker_panel.rs` — Render logic
  - `crates/rustycode-tui/src/agents/` — Agent module used by team handler (read-only reference)
  - `crates/rustycode-tui/src/app/commands/mod.rs:CommandEffect::StartTeam, CancelTeam` — Existing CommandEffect variants to route through

  **Acceptance Criteria**:
  - [ ] `TeamModeFeature` implements `TuiFeature`
  - [ ] Registered with AppShell FeatureRegistry
  - [ ] `team.*` fields removed from god struct
  - [ ] `/team` command and team panel work in `TUI_NEW_SHELL=1` path
  - [ ] `agents/` internals not modified
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Team mode starts and renders through feature
    Tool: interactive_bash (tmux)
    Preconditions: TeamMode feature wired
    Steps:
      1. Start TUI
      2. Type /team <task description>
      3. Verify team panel appears showing agents
      4. Verify worker panel shows activity
      5. Wait for completion or cancel with /cancelteam
    Expected Result: Team orchestration works identically
    Failure Indicators: Panel doesn't appear, agents not shown, cancel broken
    Evidence: .sisyphus/evidence/task-19-team-mode.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): extract TeamMode/WorkerPanel/AgentManager feature — step 3.7`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

- [ ] 20. Wire pipeline/ as an AppShell Service (Not a TuiFeature)

  **What to do**:
  - `pipeline/` is a background execution system with no UI surface of its own — it must NOT implement `TuiFeature`
  - Instead, `AppShell` holds a `PipelineRegistry` + `PipelineContext` directly as service references
  - `AppShell` drains `integration.scheduler_rx` in its run loop (after stream/event drains) and dispatches `ScheduledPhaseEvent`s to `pipeline/executor.rs`
  - Update `pipeline/tui_integration.rs` to take `&mut PipelineRegistry` and `&mut PipelineContext` instead of `&mut TUI`
  - Remove `integration.pipeline`, `integration.pipeline_ctx`, `integration.scheduler_rx` from `ServiceIntegrationState`
  - Do NOT refactor any pipeline/ internal logic — only the TUI integration point

  **Must NOT do**:
  - Do NOT refactor `pipeline/` internals — GUARDRAIL-SCOPE-5
  - Do NOT make pipeline a `TuiFeature` — it has no user-facing surface
  - Do NOT change pipeline execution semantics

  **Recommended Agent Profile**: `deep`
  **Blocked By**: Task 8

  **References**:
  - `crates/rustycode-tui/src/app/pipeline/tui_integration.rs` — Current TUI↔pipeline boundary; this is what changes
  - `crates/rustycode-tui/src/app/pipeline/registry.rs` — `PipelineRegistry`, `PipelineContext` types
  - `crates/rustycode-tui/src/app/pipeline/scheduler.rs` — `ScheduledPhaseEvent` type
  - `crates/rustycode-tui/src/app/state_model.rs:ServiceIntegrationState` — Fields to remove

  **Acceptance Criteria**:
  - [ ] `AppShell` holds `PipelineRegistry` and drains `scheduler_rx`
  - [ ] `pipeline/tui_integration.rs` no longer references `TUI` struct
  - [ ] `integration.pipeline`, `integration.pipeline_ctx`, `integration.scheduler_rx` removed from `ServiceIntegrationState`
  - [ ] Pipeline execution still works end-to-end (agent runs complete)
  - [ ] `pipeline/` internal files (executor, registry, steps/, etc.) NOT modified
  - [ ] `cargo test -p rustycode-tui` passes

  **QA Scenarios (MANDATORY)**:

  ```
  Scenario: Pipeline execution still works after wiring
    Tool: interactive_bash (tmux)
    Preconditions: Pipeline wired as AppShell service
    Steps:
      1. Start TUI (TUI_NEW_SHELL=1)
      2. Trigger a pipeline-using command (e.g., /agent or orchestration task)
      3. Verify pipeline phases execute and progress renders
      4. Verify completion state is correct
    Expected Result: Pipeline execution identical to pre-refactoring
    Failure Indicators: Pipeline phases not executing, progress not rendering
    Evidence: .sisyphus/evidence/task-20-pipeline-service.txt

  Scenario: TUI struct no longer holds pipeline fields
    Tool: Bash
    Steps:
      1. grep "pipeline\|scheduler_rx" crates/rustycode-tui/src/app/state_model.rs
      2. Assert: no pipeline fields in ServiceIntegrationState
      3. cargo build -p rustycode-tui && exit code 0
    Evidence: .sisyphus/evidence/task-20-pipeline-fields-removed.txt
  ```

  **Commit**: YES
  - Message: `refactor(tui): wire pipeline/ as AppShell service — step 3.8`
  - Pre-commit: `cargo test -p rustycode-tui && cargo clippy -p rustycode-tui -- -D warnings`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo clippy -p rustycode-tui -- -D warnings` + `cargo fmt -- --check` + `cargo test -p rustycode-tui`. Review all changed files for: `as any` equivalents in Rust (unnecessary unsafe, unwrap in prod), empty catches, println! in prod, commented-out code, unused imports. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Lint [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high`
  Start from clean state. Launch TUI with `cargo run -p rustycode-cli -- tui`. Verify: conversation flow, plugin management, help overlay, command palette, file selector, search, MCP panel, session streaming, theme switching. Test edge cases: empty state, long input, rapid actions. Save screenshots to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff (git log/diff). Verify 1:1 — everything in spec was built (no missing), nothing beyond spec was built (no creep). Check "Must NOT do" compliance per task. Detect cross-task contamination. Flag unaccounted changes. Verify `event_loop.rs` < 500 lines. Verify `StreamChunk` and `EventMsg`/`ServiceEvent` remain as two separate types (`git diff crates/rustycode-tui/src/app/async_.rs` — no merging). Verify `streaming/adapter.rs` unmodified (`git diff`). Verify `session_mode.rs` and `mcp_mode.rs` unmodified.
  Output: `Tasks [N/N compliant] | Guardrail violations [CLEAN/N] | Unaccounted [CLEAN/N files] | event_loop.rs [NNN lines] | VERDICT`

---

## Commit Strategy

Every task = one atomic commit. Format:
```
refactor(tui): [what changed] — step X.Y

- Specific change 1
- Specific change 2

Verification: cargo build -p rustycode-tui && cargo test -p rustycode-tui
```

No commit should touch more than ~500 lines. If a step exceeds this, split it.

---

## Success Criteria

### Verification Commands
```bash
cargo build -p rustycode-tui                          # Expected: success
cargo test -p rustycode-tui                           # Expected: all pass (count ≥ 1,962)
cargo clippy -p rustycode-tui -- -D warnings          # Expected: no warnings
cargo fmt -- --check                                  # Expected: no diffs
cargo run -p rustycode-cli -- tui                     # Expected: launches, functional (old path)
TUI_NEW_SHELL=1 cargo run -p rustycode-cli -- tui     # Expected: launches, functional (new path)
wc -l crates/rustycode-tui/src/app/event_loop.rs     # Expected: < 500
grep -rn "StreamChunk\|ServiceEvent" crates/rustycode-tui/src/app/features/ | grep "enum TuiEvent" # Expected: both variants present
```

### Final Checklist
- [ ] All "Must Have" present: TuiFeature trait, AppShell, UpdateCtx/RenderCtx, FeatureRegistry, one migrated feature (PluginManager), EXTRACTION_MAP.md, pipeline wired as AppShell service
- [ ] All guardrails satisfied: `StreamChunk` and `EventMsg` remain distinct types; `streaming/adapter.rs` unmodified; no `block_on()` in render; no `&mut AppShell` in features; no channel consolidation; `session_mode.rs` and `mcp_mode.rs` unmodified
- [ ] All tests pass (count ≥ 1,962 baseline)
- [ ] `event_loop.rs` < 500 lines
- [ ] No feature module has `&mut AppShell` or `&mut TUI` in signatures
- [ ] Async channel drain ordering preserved (stream=8/frame, tool=8/frame limits intact)
- [ ] `TUI_NEW_SHELL=1` path fully functional for all features
