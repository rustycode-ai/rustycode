# Plan: TUI God Object Extraction — Phase 1

**Date:** 2026-05-04
**Status:** Proposed
**Priority:** P2 (architectural debt)
**Impact:** 22 dependencies, 5K+ LOC in `rustycode-tui`

## Problem

`rustycode-tui` is the largest god object in the codebase. The main `event_loop.rs` contains a monolithic `App` struct that handles:

1. **Event routing** (keyboard, mouse, resize, paste)
2. **Service initialization** (LSP, MCP, tools, sessions)
3. **LLM provider management** (API key validation, provider switching)
4. **Session lifecycle** (create, resume, switch, delete)
5. **Message rendering** (markdown, tool calls, streaming)
6. **Command palette** (slash commands, autocomplete)
7. **Tool registration** (builtin + MCP tool merging)
8. **Status bar management** (LSP status, MCP status, model info)

### Evidence

- **56 communities** detected by code-review-graph, `ui-tool` community alone has **5,628 nodes**
- `event_loop.rs` has 20+ methods including `run()`, `event_loop()`, `init_services()`, `register_builtin_tools()`, `load_mcp_tools()`, `handle_slash_command()`, `poll_mcp_events()`
- The `App` struct has 75 allowlisted clippy lints in `lib.rs` (lines 1-76), the most of any crate
- CLAUDE.md explicitly flags this: "rustycode-tui (22 dependencies, 5K+ LOC) → Will split into thin UI layer"

### Before Metrics

| Metric | Value |
|--------|-------|
| LOC in event_loop.rs | ~1,500+ |
| App struct fields | ~40+ |
| Dependencies | 22 |
| Clippy allowlist entries | 75 |
| Methods on App | 20+ |
| Communities containing TUI code | 1 (5,628 nodes) |

## Proposed Change: Extract Service Layer

Create a new `rustycode-tui-services` crate (or `rustycode-tui::services` module) that extracts service initialization and management from the monolithic `App`:

### Files to Create

1. **`crates/rustycode-tui/src/services/mod.rs`** — Service registry
2. **`crates/rustycode-tui/src/services/tool_manager.rs`** — Tool registration + MCP tool loading
3. **`crates/rustycode-tui/src/services/session_manager.rs`** — Session create/resume/switch/delete
4. **`crates/rustycode-tui/src/services/provider_manager.rs`** — LLM provider init, API key validation, switching

### Files to Modify

1. **`crates/rustycode-tui/src/app/event_loop.rs`** — Remove extracted methods, delegate to service structs
2. **`crates/rustycode-tui/src/app/mod.rs`** — Add service fields to App, remove direct logic
3. **`crates/rustycode-tui/Cargo.toml`** — No new deps (internal reorganization only)

### After Metrics (Expected)

| Metric | Before | After |
|--------|--------|-------|
| LOC in event_loop.rs | ~1,500 | ~900 |
| App struct fields | ~40 | ~25 |
| Methods on App | 20+ | ~10 (delegating) |
| Max function length | ~200 LOC | ~80 LOC |
| Service modules | 0 | 3 |

## Scope Boundaries

**In scope:**
- Extract service initialization methods from App
- Create thin service wrapper structs
- Maintain all existing behavior (no UX changes)
- All existing tests pass

**Out of scope:**
- UI rendering extraction (separate phase)
- Command palette extraction (separate phase)
- New crate creation (module extraction first, crate split later if needed)

## Risk Assessment

| Risk | Mitigation |
|------|------------|
| Borrow checker fights with split methods | Service structs own their data; App borrows via `&self` |
| Regression in event handling | All existing TUI tests must pass; manual TUI testing via mcpretentious |
| Performance impact from delegation | Negligible — all in-process method calls, no allocation changes |

## Verification

1. `cargo test -p rustycode-tui` — all existing tests pass
2. `cargo clippy -p rustycode-tui -- -D warnings` — zero warnings
3. TUI binary runs and accepts input (manual verification via mcpretentious)
4. No new dependencies added
