# Workspace Cleanup Plan — 2026-04-27

## Status: IN PROGRESS (Phase 3)

## Summary

Full workspace cleanup: fix broken build, enforce workspace lints across 51 crates, resolve clippy warnings, split oversized files, remove dead code.

## Phase 1: Fix Build — COMPLETE

- `rustycode-storage/src/lib.rs:2304` — Added `execution_trace: None` to `session_from_row()`
- `rustycode-orchestration/src/execution_trace.rs:6` — Added `PartialEq` derive to `TraceEntry`
- `rustycode-tui/src/app/service_integration.rs:225-226` — Fixed 3-tuple return from `load_provider_config_from_env()`
- `rustycode-tui/src/app/service_integration.rs:410-419` — Replaced `StreamChunk::SystemMessage` with proper variants

## Phase 2: Workspace Lints — COMPLETE

- Added `[lints] workspace = true` to all 51 crate Cargo.toml files
- `rustycode-bench` uses manual lints (workspace=true can't mix with `rust.unsafe_code = "allow"`)
- Added `uninlined_format_args = "allow"` to workspace lints (500+ instances, zero functional impact)
- Removed duplicate `[lints]` sections created by initial script

## Phase 3: Clippy Errors — IN PROGRESS

### Current error breakdown (after uninlined_format_args fix):

| Error | Count | Strategy |
|-------|-------|----------|
| `unwrap()` on Result (test code) | ~280 | Allow at test module level |
| `unwrap()` on Result (non-test) | ~30 | Fix with `?` or `.context()` |
| `unwrap()` on Option (non-test) | ~21 | Fix with `?` or `.unwrap_or_default()` |
| `cast_precision_loss` (usize→f64) | 28 | Allow at workspace level (pervasive) |
| `float_cmp` | 22 | Allow at workspace level (test assertions) |
| `format_push_string` | 9 | Fix with `push_str` or `write!` |
| `unused_self` | 8 | Fix to associated functions |
| `used_underscore_binding` | 7 | Fix or rename |
| `option_if_let_else` | 9 | Allow (less readable when "fixed") |
| `unused_async` | 6 | Remove async from sync functions |
| Other (misc) | ~40 | Fix individually |

### Recommended workspace-level allows:

```toml
# Lossy casts are common in metrics/cost calculations where precision
# loss is acceptable (e.g., token counts to USD).
cast_precision_loss = "allow"
cast_lossless = "allow"  # too noisy for usize→u64 etc.

# Float comparisons are used in test assertions and cost calculations.
float_cmp = "allow"

# option_if_let_else suggestions often reduce readability for complex branches.
option_if_let_else = "allow"

# Significant drop tightening: common pattern with Mutex guards in async code.
significant_drop_tightening = "allow"
```

### Files modified so far:
- `crates/rustycode-agent/src/context.rs` — map_or_else fix, too_many_lines allow
- `crates/rustycode-agent/src/intelligence.rs` — unwrap→map_or_else, if_let→and_then
- `crates/rustycode-agent/src/session.rs` — items_after_statements, match arms, dead_code
- `crates/rustycode-agent/tests/agent_integration.rs` — test-level allows
- `crates/rustycode-tools/src/executor/tool_shim.rs` — JSON_TOOL_CALL_RE rename (auto-fixed)

## Phase 4: Split Large Files — NOT STARTED

| File | Lines | Split Into |
|------|-------|-----------|
| `bench/src/agent/code_agent.rs` | 7,590 | Deprecate (replaced by real_agent.rs) |
| `core/src/headless/mod.rs` | 4,778 | runner.rs, context.rs, hints.rs |
| `git/src/lib.rs` | 3,865 | operations.rs, blame.rs, diff.rs, worktree.rs |
| `storage/src/lib.rs` | 3,843 | session_store.rs, plan_store.rs, conversation.rs, schema.rs |
| `tools/src/providers/lsp.rs` | 3,235 | client.rs, operations.rs, types.rs |
| `llm/src/openai.rs` | 3,002 | streaming.rs, types.rs |
| `tools/src/providers/bash.rs` | 2,474 | security.rs, execution.rs |
| `bus/src/events.rs` | 2,322 | types.rs, bus_impl.rs |

## Phase 5: Dead Code Cleanup — NOT STARTED

Dead code locations:
- `rustycode-runtime/src/multi_agent.rs:298`
- `rustycode-runtime/src/negotiation.rs:299`
- `rustycode-runtime/src/enhanced_orchestrator.rs:27`
- `rustycode-runtime/src/advanced_orchestrator.rs:28`
- `rustycode-runtime/src/benchmark/task_evaluator.rs:295`
- `rustycode-runtime/src/orchestration/routing.rs:262`
- `rustycode-tools-api/src/search_strategy.rs:8,24`

## Phase 6: Unused Deps — NOT STARTED

- `rustycode-bench`: `console` crate not found in source
- Verify `globset` and `ignore` usage (they ARE used in environment modules)

## Verification Command

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo fmt --check
```
