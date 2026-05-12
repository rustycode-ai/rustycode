# Phase 4: Crate Decomposition — Reducing Coupling in Hotspot Crates

**Date:** 2026-05-12
**Status:** Design Document
**Related:** Phase 3 (App Server), Architecture Review 2026-04-20

---

## Goal

Reduce coupling and complexity in the three largest crates (`rustycode-tui`, `rustycode-core`, `rustycode-tools`) by extracting focused, single-purpose crates following Codex patterns. Target: modules under 500 LOC, files under 800 LOC, and dependency reduction of 50%+ in hotspot crates.

---

## Current Metrics

| Crate | Workspace Deps | LOC | Coupling Score (deps × LOC) | Status |
|-------|---------------|-----|----------------------------|--------|
| `rustycode-tui` | 26 | 109,277 | 2,841,202 | 🔴 Critical |
| `rustycode-core` | 19 | 26,122 | 496,318 | 🟠 High |
| `rustycode-cli` | 16 | 44,569 | 713,104 | 🟠 High |
| `rustycode-tools` | 12 | 68,094 | 817,128 | 🔴 Critical |
| `rustycode-runtime` | 11 | 26,129 | 287,419 | 🟡 Medium |

**Largest files (need immediate splitting):**
- `rustycode-tui/src/app/event_loop.rs` — 2,236 LOC
- `rustycode-tui/src/app/render/messages.rs` — 1,936 LOC (legacy) / 1,114 LOC (new)
- `rustycode-tui/src/app/tasks.rs` — 1,154 LOC
- `rustycode-tools/src/indexing/semantic_search.rs` — 1,454 LOC
- `rustycode-tools/src/providers/powershell.rs` — 1,387 LOC

### Pre-work Completed (2026-05-12)

Some intra-crate restructuring has been done out of order as part of code quality improvements — this reduces the scope of the crate extraction work:

- **TUI struct refactored**: 60+ flat fields extracted into 11 typed sub-structs (`UIComponents`, `ServiceIntegrationState`, `TaskWorkspaceState`, `InteractionSessionState`, `SystemState`, `OverlayState`, `ToolExecutionPanel`, `ThemeNotificationState`, `TeamModeState`, `MessageSearchState`, `ProviderModelState`) in `app/state_model.rs`
- **`rustycode-core` runtime split**: `runtime.rs` monolith → `runtime/` directory with `mod.rs` + 6 domain files (`session_ops.rs`, `execution_ops.rs`, `event_ops.rs`, `tool_ops.rs`, `plan_ops.rs`, `memory_ops.rs`)
- **`rustycode-core` context consolidated**: `context_management/` + `context_prio/` merged into single `context/` submodule
- **`rustycode-core` recovery consolidated**: flat `checkpoint_*.rs` files merged into `recovery/` submodule
- **TUI input/render sub-loops renamed**: `app/input/event_loop.rs` → `app/input/handler.rs`; `app/render/event_loop.rs` → `app/render/viewport.rs`

The main `app/event_loop.rs` (2,236 LOC) remains and is still the primary splitting target.

---

## Target Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────┐
│  Presentation Layer (UI-only, ~5 deps each)                         │
├─────────────────────────────────────────────────────────────────────┤
│  rustycode-tui → server-client, protocol, ui-core, ui-model, config │
│  rustycode-cli → server-client, protocol, config                     │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Application Layer (thin orchestration, ~8 deps each)               │
├─────────────────────────────────────────────────────────────────────┤
│  rustycode-core → session, thread-manager, tool-dispatch, protocol  │
│  rustycode-runtime → task-scheduler, resource-pool, protocol         │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Domain Services (focused capabilities, 3-6 deps each)              │
├─────────────────────────────────────────────────────────────────────┤
│  tool-dispatch → tools-api, protocol, bus                           │
│  thread-manager → session, protocol, bus                            │
│  tools-fs → tools-api, security, protocol                           │
│  tools-bash → tools-api, security, sandbox                          │
│  tools-lsp → tools-api, lsp, protocol                               │
│  tools-mcp → tools-api, mcp, protocol                               │
└─────────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────────┐
│  Infrastructure (no business logic, 2-4 deps each)                  │
├─────────────────────────────────────────────────────────────────────┤
│  rustycode-storage, rustycode-bus, rustycode-config, rustycode-git  │
│  rustycode-llm, rustycode-lsp, rustycode-memory, rustycode-auth     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## `rustycode-tui` Decomposition

**Current Dependencies (26):**
```
protocol, runtime, bus, core, shared-runtime, tools, tools-api, tools-security,
llm, lsp, memory, ui-core, config, prompt, providers, session, skill, auth,
mcp, agent-runtime, storage, team, observability, classification, orchestration,
guard, vector-memory
```

**Target Dependencies (5):**
```
server-client, protocol, ui-core, ui-model, config
```

### Extraction Plan

#### 1. Extract `rustycode-tui-event-handlers` (NEW)
**Purpose:** Centralized event processing logic for streams, tools, workspace
**Modules to extract:**
- `app/handlers/*` → `rustycode-tui-event-handlers/src/handlers/`
  - `stream_approval.rs` (300 LOC)
  - `stream_core.rs` (400 LOC)
  - `stream_data.rs` (350 LOC)
  - `stream_done.rs` (250 LOC)
  - `stream_error.rs` (200 LOC)
  - `stream_tools.rs` (611 LOC)
  - `tool_result.rs` (400 LOC)
  - `workspace.rs` (500 LOC)
  - `helpers.rs` (300 LOC)
**Dependencies:** `protocol`, `tools-api`, `ui-model`, `bus`
**Removes from TUI:** 3,311 LOC, 9 direct deps

#### 2. Extract `rustycode-tui-input` (NEW)
**Purpose:** Keyboard, mouse, text input handling
**Modules to extract:**
- `app/input/*` → `rustycode-tui-input/src/`
  - `handler.rs` (861 LOC)
  - `text_input.rs` (839 LOC)
  - `keyboard.rs` (816 LOC)
  - `special_handlers.rs` (806 LOC)
  - `mouse.rs` (500 LOC)
**Dependencies:** `ui-core`, `crossterm`, `ratatui`
**Removes from TUI:** 3,822 LOC, 2 direct deps

#### 3. Extract `rustycode-tui-render` (NEW)
**Purpose:** Rendering logic for messages, tools, status
**Modules to extract:**
- `app/render/*` → `rustycode-tui-render/src/`
  - `messages.rs` (1,114 LOC) → split into `messages/` module
  - `tools.rs` (736 LOC)
  - `status.rs` (400 LOC)
  - `input.rs` (350 LOC)
  - `layout.rs` (600 LOC)
  - `brutalist/*` (2,000 LOC) → split into sub-modules
**Dependencies:** `ui-core`, `ratatui`, `protocol`
**Removes from TUI:** 5,200 LOC, 3 direct deps

#### 4. Extract `rustycode-tui-workspace` (NEW)
**Purpose:** Workspace scanning, project operations
**Modules to extract:**
- `workspace/*` → `rustycode-tui-workspace/src/`
  - `scanner.rs` (600 LOC)
  - `context.rs` (500 LOC)
  - `ignore.rs` (400 LOC)
**Dependencies:** `config`, `protocol`, `git`
**Removes from TUI:** 1,500 LOC, 4 direct deps

#### 5. Extract `rustycode-tui-tasks` (NEW)
**Purpose:** Background task management, service polling
**Modules to extract:**
- `app/tasks.rs` (1,154 LOC)
- `app/task_commands.rs` (700 LOC)
- `app/service_polling.rs` (400 LOC)
- `app/service_integration.rs` (1,074 LOC)
**Dependencies:** `runtime`, `protocol`, `bus`
**Removes from TUI:** 3,328 LOC, 3 direct deps

#### 6. Split `event_loop.rs` (2,236 LOC)
**Action:** Split into focused modules
- `event_loop.rs` → 400 LOC (core loop only)
- `event_loop_slash_commands.rs` → 400 LOC (existing)
- `event_loop_messages.rs` → 500 LOC (NEW, message handling)
- `event_loop_tools.rs` → 400 LOC (NEW, tool orchestration)
- `event_loop_state.rs` → 300 LOC (NEW, state transitions)
- `event_loop_tests.rs` → 236 LOC (existing)

#### 7. Extract `rustycode-tui-agent` (NEW)
**Purpose:** Agent factory, manager, step execution
**Modules to extract:**
- `app/agent_factory.rs` (500 LOC)
- `app/agent_manager.rs` (600 LOC)
- `app/agent_step.rs` (400 LOC)
**Dependencies:** `agent-runtime`, `protocol`, `runtime`
**Removes from TUI:** 1,500 LOC, 3 direct deps

#### 8. Extract `rustycode-tui-browser` (NEW)
**Purpose:** Browser automation, extraction
**Modules to extract:**
- `app/browser_*.rs` (4 files, ~1,200 LOC total)
**Dependencies:** `chromiumoxide`, `protocol`
**Removes from TUI:** 1,200 LOC, 2 direct deps

### Remaining TUI Core (after extraction)
**Target LOC:** ~90,000 (reduced from 109,277)
**Target deps:** 5 (down from 26)
**Structure:**
```
rustycode-tui/
├── src/
│   ├── main.rs (200 LOC)
│   ├── lib.rs (100 LOC)
│   ├── app/
│   │   ├── event_loop.rs (400 LOC)
│   │   ├── adapter.rs (300 LOC)
│   │   ├── executor.rs (400 LOC)
│   │   ├── renderer.rs (676 LOC)
│   │   ├── mod.rs (100 LOC)
│   ├── ui/ (UI state models only)
│   └── theme/ (theme definitions only)
```

---

## `rustycode-core` Decomposition

**Current Dependencies (19):**
```
config, git, lsp, memory, protocol, skill, storage, tools-api, tools,
tools-security, bus, llm, orchestration, mcp, shared-runtime, observability,
session, team, agent-runtime
```

**Target Dependencies (8):**
```
session, thread-manager, tool-dispatch, protocol, tools-api, bus, config, storage
```

### Extraction Plan

#### 1. Extract `rustycode-thread-manager` (NEW)
**Purpose:** Thread pool management, async task scheduling
**Modules to extract:**
- `runtime/mod.rs` (600 LOC, thread pool logic)
- `runtime/task_scheduler.rs` (500 LOC, if exists)
- Create new module: `thread_pool.rs` (400 LOC)
**Dependencies:** `tokio`, `protocol`, `bus`
**Removes from core:** 1,500 LOC, 3 direct deps

#### 2. Extract `rustycode-tool-dispatch` (NEW)
**Purpose:** Tool execution orchestration, result routing
**Modules to extract:**
- `execution.rs` (686 LOC) → split: `dispatch.rs` (400), `validation.rs` (286)
- `tool_result_storage.rs` (888 LOC)
- `validation.rs` (775 LOC) → merge into dispatch
**Dependencies:** `tools-api`, `protocol`, `bus`, `storage`
**Removes from core:** 2,349 LOC, 4 direct deps

#### 3. Extract `rustycode-context-manager` (NEW)
**Purpose:** Context building, ignore patterns, compaction
**Modules to extract:**
- `context/*` → `rustycode-context-manager/src/`
  - `mod.rs` (400 LOC)
  - `auto_compact.rs` (689 LOC)
  - `ignore.rs` (711 LOC)
- `cache.rs` (1,060 LOC) → move to context-manager
**Dependencies:** `protocol`, `git`, `storage`
**Removes from core:** 2,860 LOC, 4 direct deps

#### 4. Extract `rustycode-plan-executor` (NEW)
**Purpose:** Plan execution, milestone tracking
**Modules to extract:**
- `plan/*` → `rustycode-plan-executor/src/`
  - `plan_executor.rs` (684 LOC)
  - `plan_manager.rs` (600 LOC, if exists)
**Dependencies:** `protocol`, `tool-dispatch`, `orchestration`
**Removes from core:** 1,284 LOC, 3 direct deps

#### 5. Extract `rustycode-anomaly-detector` (NEW)
**Purpose:** Anomaly detection, tenacity logic
**Modules to extract:**
- `anomaly.rs` (1,115 LOC)
- `tenacity.rs` (582 LOC)
**Dependencies:** `protocol`, `storage`
**Removes from core:** 1,697 LOC, 2 direct deps

#### 6. Extract `rustycode-recovery` (NEW)
**Purpose:** Checkpoint recovery, edit history
**Modules to extract:**
- `recovery/*` → `rustycode-recovery/src/`
  - `checkpoint.rs` (500 LOC)
  - `recovery_manager.rs` (400 LOC)
- `edit_history.rs` (702 LOC)
**Dependencies:** `storage`, `protocol`
**Removes from core:** 1,602 LOC, 2 direct deps

#### 7. Move `session.rs` to `rustycode-session` crate
**Action:** Extract session management logic
- `session.rs` (1,276 LOC) → move to `rustycode-session/src/lib.rs`
**Dependencies:** `protocol`, `storage`, `config`
**Removes from core:** 1,276 LOC, 3 direct deps

### Remaining Core (after extraction)
**Target LOC:** ~12,000 (reduced from 26,122)
**Target deps:** 8 (down from 19)
**Structure:**
```
rustycode-core/
├── src/
│   ├── lib.rs (100 LOC)
│   ├── orchestrator.rs (500 LOC, thin coordination layer)
│   ├── integration.rs (400 LOC, integration glue)
│   └── headless/ (keep existing, ~2,000 LOC)
```

---

## `rustycode-tools` Decomposition

**Current Dependencies (12):**
```
tools-api, tools-security, executable, config, lsp, protocol, bus,
shared-runtime, thread-guard, storage, tool-integration, sandbox
```

**Target Dependencies (6 per crate):**
Split by category into focused crates

### Extraction Plan

#### 1. Extract `rustycode-tools-fs` (NEW)
**Purpose:** File system operations (read, write, edit, glob)
**Modules to extract:**
- `providers/fs/*` → `rustycode-tools-fs/src/providers/fs/`
  - `read_file.rs` (1,014 LOC)
  - `edit.rs` (600 LOC)
  - `write.rs` (400 LOC)
  - `multiedit.rs` (879 LOC)
  - `glob.rs` (500 LOC)
**Dependencies:** `tools-api`, `tools-security`, `protocol`
**Removes from tools:** 3,393 LOC, 4 direct deps

#### 2. Extract `rustycode-tools-bash` (NEW)
**Purpose:** Shell command execution
**Modules to extract:**
- `providers/bash.rs` (800 LOC)
- `providers/powershell.rs` (1,387 LOC) → split into sub-modules
- `providers/shell.rs` (400 LOC)
**Dependencies:** `tools-api`, `sandbox`, `protocol`
**Removes from tools:** 2,587 LOC, 3 direct deps

#### 3. Extract `rustycode-tools-lsp` (NEW)
**Purpose:** LSP tool implementations
**Modules to extract:**
- `providers/lsp.rs` (600 LOC)
- `providers/symbol.rs` (1,081 LOC) → split into sub-modules
- `lsp/*` (if exists, ~500 LOC)
**Dependencies:** `tools-api`, `lsp`, `protocol`
**Removes from tools:** 2,181 LOC, 3 direct deps

#### 4. Extract `rustycode-tools-mcp` (NEW)
**Purpose:** MCP server tool implementations
**Modules to extract:**
- `providers/mcp.rs` (500 LOC)
- All MCP-specific tools (~400 LOC)
**Dependencies:** `tools-api`, `mcp`, `protocol`
**Removes from tools:** 900 LOC, 3 direct deps

#### 5. Extract `rustycode-tools-indexing` (NEW)
**Purpose:** Code indexing, semantic search, repo map
**Modules to extract:**
- `indexing/*` → `rustycode-tools-indexing/src/indexing/`
  - `semantic_search.rs` (1,454 LOC) → split into modules
  - `repo_map/parser.rs` (1,123 LOC) → split into modules
  - `indexer.rs` (600 LOC)
**Dependencies:** `tools-api`, `vector-memory`, `protocol`
**Removes from tools:** 3,177 LOC, 3 direct deps

#### 6. Extract `rustycode-tools-registry` (NEW)
**Purpose:** Tool registry, selector, metadata
**Modules to extract:**
- `registry/*` → `rustycode-tools-registry/src/`
  - `selector.rs` (836 LOC)
  - `registry.rs` (600 LOC)
  - `metadata.rs` (400 LOC)
**Dependencies:** `tools-api`, `protocol`
**Removes from tools:** 1,836 LOC, 2 direct deps

#### 7. Extract `rustycode-tools-executor` (NEW)
**Purpose:** Tool execution engine, hooks, recipes
**Modules to extract:**
- `executor/*` → `rustycode-tools-executor/src/`
  - `executor.rs` (700 LOC)
  - `hooks.rs` (500 LOC)
- `recipes/*` (600 LOC)
- `templates/*` (500 LOC)
**Dependencies:** `tools-api`, `bus`, `protocol`
**Removes from tools:** 2,300 LOC, 3 direct deps

### Remaining Tools (after extraction)
**Target LOC:** ~48,000 (reduced from 68,094)
**Target deps:** 4 (down from 12)
**Structure:**
```
rustycode-tools/
├── src/
│   ├── lib.rs (200 LOC, re-exports only)
│   ├── mod.rs (100 LOC)
│   ├── json_repair.rs (1,077 LOC, utility)
│   ├── truncation.rs (947 LOC, utility)
│   ├── file_formatter.rs (867 LOC, utility)
│   ├── observation_layer.rs (926 LOC, logging)
│   └── hints_loader.rs (834 LOC, hints)
```

---

## New Crates Summary

| Crate | Purpose | LOC | Dependencies |
|-------|---------|-----|--------------|
| `rustycode-tui-event-handlers` | Stream/tool/workspace event handlers | 3,311 | 4 |
| `rustycode-tui-input` | Keyboard/mouse/text input | 3,822 | 3 |
| `rustycode-tui-render` | Message/tool/status rendering | 5,200 | 3 |
| `rustycode-tui-workspace` | Workspace scanning & context | 1,500 | 3 |
| `rustycode-tui-tasks` | Background task management | 3,328 | 3 |
| `rustycode-tui-agent` | Agent factory & manager | 1,500 | 3 |
| `rustycode-tui-browser` | Browser automation | 1,200 | 2 |
| `rustycode-thread-manager` | Thread pool & scheduling | 1,500 | 3 |
| `rustycode-tool-dispatch` | Tool orchestration | 2,349 | 4 |
| `rustycode-context-manager` | Context building & compaction | 2,860 | 3 |
| `rustycode-plan-executor` | Plan execution | 1,284 | 3 |
| `rustycode-anomaly-detector` | Anomaly detection | 1,697 | 2 |
| `rustycode-recovery` | Checkpoint recovery | 1,602 | 2 |
| `rustycode-tools-fs` | File operations | 3,393 | 3 |
| `rustycode-tools-bash` | Shell execution | 2,587 | 3 |
| `rustycode-tools-lsp` | LSP tools | 2,181 | 3 |
| `rustycode-tools-mcp` | MCP tools | 900 | 3 |
| `rustycode-tools-indexing` | Code indexing | 3,177 | 3 |
| `rustycode-tools-registry` | Tool registry | 1,836 | 2 |
| `rustycode-tools-executor` | Execution engine | 2,300 | 3 |

**Total new crates:** 20
**Total extracted LOC:** ~47,000
**Average LOC per crate:** ~2,350 (well under 5,000 target)

---

## Circular Dependency Resolution

### 1. `rustycode-core` ↔ `rustycode-orchestration`
**Current issue:** Core imports orchestration for task execution; orchestration imports core for session management.

**Solution:**
- Move `session.rs` from core to `rustycode-session` crate
- Create `rustycode-plan-executor` to own plan execution logic
- Both `core` and `orchestration` depend on `session` and `plan-executor`
- Remove direct core ↔ orchestration dependency

**Dependency path after fix:**
```
rustycode-core → session, plan-executor, protocol
rustycode-orchestration → session, plan-executor, protocol
```

### 2. `rustycode-tools` ↔ `rustycode-execution`
**Current issue:** Tools crate depends on execution for executor logic; execution depends on tools for tool definitions.

**Solution:**
- Extract `rustycode-tools-executor` to own execution engine
- `rustycode-tools` only defines tool implementations
- `rustycode-execution` (if exists) merges into `rustycode-tools-executor`
- Both depend on `tools-api` for trait definitions

**Dependency path after fix:**
```
rustycode-tools → tools-api, protocol
rustycode-tools-executor → tools-api, protocol, tools
```

### 3. `rustycode-llm` ↔ `rustycode-tools` (via `rustycode-tool-integration`)
**Status:** Already resolved via shim crate providing `ToolExecutorApi`, `ToolInfo`, `TokenCounter`, `CostTracker`.

**No action needed.**

---

## Dependency Rules (Layering Constraints)

### Rule 1: No Upward Dependencies
- **Infrastructure** must not depend on **Application** or **Presentation**
- **Domain Services** must not depend on **Presentation**
- **Application** must not depend on **Presentation**

### Rule 2: Protocol-First Communication
- All cross-crate communication uses types from `rustycode-protocol`
- No direct crate-to-crates type dependencies (except protocol)
- Event bus (`rustycode-bus`) for async pub/sub

### Rule 3: API Crate Separation
- `rustycode-tools-api` defines traits only
- `rustycode-tools-*` crates implement traits
- `rustycode-tools-executor` consumes traits

### Rule 4: UI Isolation
- `rustycode-tui-*` crates can depend on `ui-core`, `ui-model`
- No other crates depend on `ui-core` or `ui-model`
- TUI-specific crates prefixed with `rustycode-tui-`

### Rule 5: Feature Flag Gates
- Optional features use `dep:` syntax in Cargo.toml
- Default feature set minimized
- Vector memory, browser tools are opt-in

---

## Migration Path (Ordered Extraction)

### Phase 4.1: Tools Crate Split (2 weeks)
**Priority:** Highest impact, lowest risk
1. Create `rustycode-tools-fs` (extract file operations)
2. Create `rustycode-tools-bash` (extract shell operations)
3. Create `rustycode-tools-lsp` (extract LSP tools)
4. Create `rustycode-tools-mcp` (extract MCP tools)
5. Create `rustycode-tools-indexing` (extract indexing)
6. Create `rustycode-tools-registry` (extract registry)
7. Create `rustycode-tools-executor` (extract executor)
8. Update `rustycode-tools` to re-export all new crates
9. Verify zero test failures

### Phase 4.2: Core Crate Split (2 weeks)
**Priority:** Break circular dependencies
1. Create `rustycode-thread-manager` (extract runtime)
2. Create `rustycode-tool-dispatch` (extract execution)
3. Create `rustycode-context-manager` (extract context)
4. Move `session.rs` to `rustycode-session`
5. Create `rustycode-plan-executor` (extract plans)
6. Create `rustycode-anomaly-detector` (extract anomaly)
7. Create `rustycode-recovery` (extract recovery)
8. Update `rustycode-core` to thin orchestrator
9. Verify circular dependencies resolved

### Phase 4.3: TUI Crate Split (3 weeks)
**Priority:** Most complex, highest LOC reduction
1. Create `rustycode-tui-event-handlers` (extract handlers)
2. Create `rustycode-tui-input` (extract input)
3. Create `rustycode-tui-render` (extract render)
4. Create `rustycode-tui-workspace` (extract workspace)
5. Create `rustycode-tui-tasks` (extract tasks)
6. Create `rustycode-tui-agent` (extract agent)
7. Create `rustycode-tui-browser` (extract browser)
8. Split `event_loop.rs` into focused modules
9. Update `rustycode-tui` to thin UI shell
10. Verify zero UI regressions

### Phase 4.4: Cleanup & Verification (1 week)
1. Update all imports
2. Remove dead code
3. Update documentation
4. Run full test suite
5. Measure coupling scores
6. Update architecture docs

---

## Metrics Targets

### After Phase 4 Complete

| Crate | Target Deps | Target LOC | Target Score | Reduction |
|-------|-------------|------------|--------------|-----------|
| `rustycode-tui` | 5 | 90,000 | 450,000 | 84% ↓ |
| `rustycode-core` | 8 | 12,000 | 96,000 | 81% ↓ |
| `rustycode-tools` | 4 | 48,000 | 192,000 | 76% ↓ |
| `rustycode-runtime` | 8 | 20,000 | 160,000 | 44% ↓ |
| `rustycode-cli` | 5 | 40,000 | 200,000 | 72% ↓ |

### File Size Targets
- **Maximum file size:** 800 LOC (hard limit)
- **Target file size:** 400 LOC (preferred)
- **Maximum module size:** 2,000 LOC (hard limit)
- **Target module size:** 1,000 LOC (preferred)

### Dependency Targets
- **Maximum crate deps:** 15 (hard limit)
- **Target crate deps:** 8 (preferred)
- **No circular dependencies:** 0 (enforced)

---

## Success Criteria

1. **Coupling Reduction:**
   - [ ] `rustycode-tui` coupling score < 500,000 (82% reduction)
   - [ ] `rustycode-core` coupling score < 100,000 (80% reduction)
   - [ ] `rustycode-tools` coupling score < 200,000 (75% reduction)

2. **File Size Limits:**
   - [ ] Zero files over 800 LOC
   - [ ] 90%+ of files under 500 LOC

3. **Dependency Limits:**
   - [ ] Zero circular dependencies
   - [ ] All crates under 15 workspace deps
   - [ ] 80%+ of crates under 10 workspace deps

4. **Test Coverage:**
   - [ ] All new crates have 80%+ test coverage
   - [ ] Zero test failures after extraction
   - [ ] Zero clippy warnings

5. **Documentation:**
   - [ ] All new crates have README.md
   - [ ] Architecture diagram updated
   - [ ] CRATES.md updated

6. **Performance:**
   - [ ] No regression in build time (<10% variance)
   - [ ] No regression in runtime performance (<5% variance)

---

## Risks & Mitigations

### Risk 1: Breaking Changes During Extraction
**Mitigation:**
- Extract with re-exports first (backward compatible)
- Deprecate old paths gradually
- Run full test suite after each extraction

### Risk 2: Circular Dependency Reintroduction
**Mitigation:**
- Enforce dependency rules via CI check
- Use `cargo machete` to detect unused deps
- Document all dependency paths

### Risk 3: Test Coverage Gaps
**Mitigation:**
- Write tests before extracting (TDD)
- Keep module tests with extracted code
- Add integration tests for cross-crate boundaries

### Risk 4: Build Time Increase
**Mitigation:**
- Use workspace dependencies to avoid duplicate compilation
- Enable incremental compilation
- Profile build time with `cargo timing`

---

## Next Steps

1. **Review & Approval** — Stakeholder review of this design
2. **Create Tracking Issues** — One issue per extraction phase
3. **Start Phase 4.1** — Begin with tools crate split (lowest risk)
4. **Weekly Progress Reviews** — Track coupling score reduction
5. **Update Architecture Docs** — Keep docs in sync with changes

---

## References

- [Architecture Review 2026-04-20](ARCHITECTURE-REVIEW-2026-04-20.md)
- [Codex Architecture Patterns](https://github.com/codex-storage/codex)
- [Rust Crate Design Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Phase 3: App Server Design](PHASE3-APP-SERVER.md) (if exists)
