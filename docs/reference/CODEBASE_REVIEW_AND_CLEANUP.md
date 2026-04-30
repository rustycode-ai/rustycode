# Codebase Review & Cleanup

**Date**: 2026-04-13
**Status**: ✅ Phases 1–3 executed. Phase 4 (code quality) remains.
**Scope**: Full repository audit — structure, dead code, clutter, stale artifacts, and refactoring opportunities.

---

## 1. Repository Overview

### Before Cleanup

| Metric | Before | After |
|--------|--------|-------|
| Root-level entries | 82 | **40** |
| Root shell scripts | 29 | **0** |
| Crates on disk | 38 dirs | **37 dirs** |
| Workspace members | 34 | **35** (added `rustycode-bench`) |
| Stale artifacts on disk | ~40MB | **0** (archived to `.archive-2026-04-13/`) |

### After Cleanup — Root Directory Layout

```
rustycode/
├── .archive-2026-04-13/    ← Archived stale dirs (1.5MB tarball)
├── .audit.toml
├── .claude/
├── .clippy.toml
├── .code-review-graph/
├── .git/
├── .github/
├── .gitignore
├── .gitleaks.toml
├── .orchestra/
├── .planning/
├── .pre-commit-config.yaml
├── .rustycode/
├── .rustycodeignore
├── .serena/
├── apps/
├── benches/
├── build-all.bat
├── build-all.sh
├── Cargo.lock
├── Cargo.toml              ← Fixed: one crate per line
├── CONTRIBUTING.md
├── crates/                 ← 37 dirs (removed rustycode-orchestra, rustycode-models)
├── deny.toml
├── docs/
├── examples/
├── harbor-agent/           ← Kept (active)
├── jobs/
├── mcp-test-server/        ← Kept (active)
├── mcp.json.example
├── README.md
├── release/
├── REVIEW_UX.md
├── rust-analyzer.toml
├── scripts/                ← 29 root scripts moved to scripts/archive/
├── src/
├── target/
└── TEAM_LEARNINGS.md
```

The codebase is a **mature Rust workspace** with strong lint discipline (pedantic + nursery clippy, `unwrap_used`/`expect_used` as warnings, `unsafe_code` forbidden).

---

## 2. Root Directory Clutter

### 2.1 Disposable Test/Debug/Demo Scripts (29 files) — ✅ DONE

All 29 scripts moved to `scripts/archive/`. Utility scripts (`query_users.py`, `show_themes.py`) moved to `scripts/`.

| File | Purpose |
|------|---------|
| `automated_tui_test.sh` | Automated TUI testing |
| `automated_tui_test_clean.sh` | TUI test cleanup |
| `debug_tool_execution.sh` | Tool execution debugging |
| `demonstrate_orchestra2.sh` | Autonomous Mode demo |
| `execute-tests.sh` | Test execution |
| `hands_on_test.sh` | Hands-on testing |
| `hands-on-test.sh` | **Duplicate** of above |
| `practical_comparison_test.sh` | Comparison testing |
| `quick_comparison.sh` | Quick comparison testing |
| `real_user_simulation.sh` | User simulation |
| `real_world_comparison_test.sh` | Real-world comparison |
| `run-comparison.sh` | Comparison running |
| `run_lsp_performance_tests.sh` | LSP performance tests |
| `run_ensemble_demo.sh` | Ensemble demo |
| `test_in_tmux.sh` | Tmux testing |
| `test_in_tmux_fixed.sh` | **Fixed version** of above |
| `test_semantic_search.sh` | Semantic search testing |
| `test_ensemble_agent_tmux.sh` | Ensemble agent tmux testing |
| `test_tool_execution.sh` | Tool execution testing |
| `test_tool_schema_debug.sh` | Tool schema debugging |
| `test_tui_comparison.sh` | TUI comparison testing |
| `test_tui_comprehensive.sh` | Comprehensive TUI testing |
| `test_tui_debug.sh` | TUI debugging |
| `test_tui_project_building.sh` | TUI project building |
| `test_tui_tmux.sh` | TUI tmux testing |
| `tmux-test.sh` | Tmux testing |
| `tmux_comparison_test.sh` | Tmux comparison testing |
| `verify_integration.sh` | Integration verification |

**Recommendation**: Move useful scripts to `scripts/archive/`, delete obvious duplicates.

### 2.2 Temp/Artifact Files (4 files) — ✅ DONE

| File | Action Taken |
|------|-------------|
| `benchmark_leaderboard.json` | Moved into `benchmark_results/` before archival (→ tarball) |
| `cs` | Deleted |
| `test_profile` | Deleted |
| `test.txt` | Deleted |

### 2.3 Non-Rust Utility Scripts (3 files) — ✅ DONE

| File | Action Taken |
|------|-------------|
| `httparty_example.rb` | Deleted (not part of Rust project) |
| `query_users.py` | Moved to `scripts/` |
| `show_themes.py` | Moved to `scripts/` |

### 2.4 Essential Root Files (Keep)

`.gitignore`, `.gitleaks.toml`, `.pre-commit-config.yaml`, `.rustycodeignore`, `build-all.sh`, `build-all.bat`, `Cargo.lock`, `Cargo.toml`, `CONTRIBUTING.md`, `README.md`, `REVIEW_UX.md`, `TEAM_LEARNINGS.md`, `mcp.json.example`, `deny.toml`, `.audit.toml`, `.clippy.toml`, `rust-analyzer.toml`

---

## 3. Stale & Orphan Crates

### 3.1 `crates/rustycode-orchestra/` — ✅ REMOVED

- Was NOT in workspace `Cargo.toml` members list
- Was NOT depended on by any workspace crate
- Contained 20 `.rs` files (~300KB of code) with heavy `TODO: Implement` stubs
- `agents.rs` alone was 71.7KB, `specialized_agents.rs` was 31.4KB
- The `rustycode-orchestra` crate is the active successor

### 3.2 `crates/rustycode-models/` — ✅ REMOVED

- Had a `Cargo.toml` but **no `src/` directory**
- Was not in workspace members list
- No other crate depended on it

### 3.3 `crates/ratzilla-wasm/` and `crates/rustycode-web/` — **Explicitly excluded**

- Both in `exclude` list in workspace Cargo.toml
- `rustycode-web/` has its own `Cargo.lock` and `Trunk.toml` (separate build target)
- These are intentional exclusions — **keep as-is**.

### 3.4 `crates/rustycode-connector/` — **In workspace but needs review**

- Recently added (in workspace members)
- Recent commits added 185 lines of tests
- Needs verification that it's actively used

---

## 4. Large Files (Potential Refactoring Targets)

Files over 1,500 lines that may benefit from splitting:

| Lines | File | Suggestion |
|-------|------|------------|
| 4,575 | `rustycode-core/src/headless.rs` | Split into submodules (streaming, tool dispatch, session) |
| 3,790 | `rustycode-git/src/lib.rs` | Extract worktree ops, status, commit logic |
| 3,413 | `rustycode-tui/src/app/brutalist_renderer.rs` | Extract component renderers |
| 2,474 | `rustycode-llm/src/openai.rs` | Extract streaming from request building |
| 2,425 | `rustycode-storage/src/lib.rs` | Extract session vs cache vs migration modules |
| 2,365 | `rustycode-tui/src/app/event_loop.rs` | Extract handler groups |
| 2,273 | `rustycode-tui/src/ui/wizard.rs` | Extract wizard steps |
| 2,105 | `rustycode-orchestra/src/agents.rs` | **Dead crate — remove** |
| 2,023 | `rustycode-llm/src/anthropic.rs` | Extract streaming from request building |
| 1,962 | `rustycode-tools/src/repo_map.rs` | Extract tree-sitter parsing from map generation |
| 1,960 | `rustycode-protocol/src/ensemble.rs` | Extract message types from ensemble logic |
| 1,917 | `rustycode-tools/src/lib.rs` | Extract tool trait definitions |
| 1,905 | `rustycode-tools/src/tool_inspector.rs` | Consider splitting inspection logic |
| 1,824 | `rustycode-bus/src/lib.rs` | Extract pub/sub from dispatch |
| 1,766 | `rustycode-tools/src/bash.rs` | Extract validation from execution |
| 1,725 | `rustycode-runtime/src/monitoring.rs` | Extract metrics from health checks |
| 1,687 | `rustycode-runtime/src/service_discovery.rs` | Review complexity |
| 1,678 | `rustycode-bus/src/events.rs` | Extract event definitions |
| 1,648 | `rustycode-runtime/src/event_system.rs` | Extract handler registration |
| 1,648 | `rustycode-runtime/src/agent_lifecycle.rs` | Extract state machine |
| 1,632 | `rustycode-vector-memory/src/lib.rs` | Extract index management |
| 1,623 | `rustycode-tools/src/lsp.rs` | Extract LSP protocol from tool integration |
| 1,591 | `rustycode-ui-core/src/markdown.rs` | Extract parsing from rendering |
| 1,571 | `rustycode-runtime/src/resource_manager.rs` | Review complexity |

---

## 5. Dead Code & Suppressions

### 5.1 `#[allow(dead_code)]` Annotations (31 instances)

| Crate | Count | Notes |
|-------|-------|-------|
| `rustycode-providers` | 9 | `registry.rs` — many `dead_code` fields on provider structs |
| `rustycode-runtime` | 7 | `benchmark/`, `multi_agent`, `worker_pool`, `negotiation`, `enhanced_orchestrator`, `advanced_orchestrator` |
| `rustycode-auth` | 4 | `github_copilot.rs` — unused struct fields |
| `rustycode-tools` | 6 | Examples and benchmarks |
| `rustycode-plugins` | 1 | `tests.rs` |

**Recommendation**: Audit each instance. If the field/function isn't needed, remove it. If it's planned for future use, add a clear comment.

### 5.2 Actionable TODOs (source code, not tests/examples)

| File | TODO | Priority |
|------|------|----------|
| `rustycode-runtime/src/workflow/meta_tool.rs:298` | Implement actual tool execution | High |
| `rustycode-orchestra/src/llm.rs:160,172` | Implement autonomous/guided task execution | High |
| `rustycode-orchestra/src/milestone_actions.rs:173` | Prune from QUEUE-ORDER.json | Medium |
| `rustycode-orchestra/src/auto_direct_dispatch.rs:373` | Load from preferences.md | Medium |
| `rustycode-orchestra/src/swebench/predictor.rs:124,166` | Wire Autonomous Mode headless + git diff collection | High |

The `rustycode-orchestra/src/agents.rs` file has **20+ TODO/FIXME** entries but is in the legacy crate (see §3.1).

---

## 6. Stale Directories & Artifacts — ✅ ARCHIVED

| Directory | Original Size | Action Taken |
|-----------|---------------|-------------|
| `archive/` | 1.3MB | Archived to `.archive-2026-04-13/` tarball, removed from tree |
| `benchmark_results/` | 72KB | Archived to tarball, removed from tree |
| `reports/` | 328KB | Archived to tarball, removed from tree |
| `test-orchestra/` | 28MB | Archived to tarball, removed from tree |
| `.worktrees/` | 0 | Deleted (empty) |
| `.ruff_cache/` | — | Deleted (cache, regenerated on demand) |
| `.bg-shell/` | — | Deleted (ephemeral) |
| `harbor-agent/` | 9.1MB | **Kept** — active component |
| `mcp-test-server/` | 12KB | **Kept** — active test server |
| `.code-review-graph/` | — | **Kept** — active tool |
| `.orchestra/` | — | **Kept** — runtime state |
| `.planning/` | — | **Kept** — active planning |
| `.serena/` | — | **Kept** — active tooling |
| `jobs/` | 7MB | **Kept** — may have active entries |

**Recovery**: All archived content is in `.archive-2026-04-13/archive-benchmarks-reports-testorchestra.tar.gz` (1.5MB).

---

## 7. docs/ Directory Bloat

The `docs/` directory has **78 entries** with significant redundancy:

- Multiple "complete" summaries: `PHASE3_INTEGRATION_COMPLETE.md`, `PHASE4_EVENT_DRIVEN_COMPLETE.md`, `ENHANCED_AGENT_IMPLEMENTATION_COMPLETE.md`, `TEAM_IMPLEMENTATION_COMPLETE.md`, `AUTOAGENT_IMPLEMENTATION_SUMMARY.md`, etc.
- Multiple architecture docs that may overlap: `architecture.md`, `design/`, `architecture-upgrade/`
- Multiple Orchestra docs: `orchestra-architecture.md`, `orchestra-commands.md`, `orchestra-file-structure.md`, `orchestra-implementation.md`, `orchestra-lifecycle-evaluation.md`, `orchestra-prompts.md`, `orchestra-workflow.md`
- Multiple tool specs: `TOOL_INTERFACE_SPEC.md`, `TOOL_GENERATION_IMPLEMENTATION.md`, `TOOL_PERMISSIONS.md`, `INTERFACE_SPEC.md`

**Recommendation**: Consolidate into a structured hierarchy:
```
docs/
  architecture/
  orchestra/
  guides/
  reference/
  archive/  (completed phase docs)
```

---

## 8. Workspace Configuration Issues

### 8.1 Formatting Inconsistency in Cargo.toml — ✅ FIXED

Lines with multiple crates per line have been reformatted to one crate per line for consistency.

---

## 9. Cleanup Execution Log

### Phase 1: Low Risk, High Impact — ✅ COMPLETED

| # | Action | Status | Detail |
|---|--------|--------|--------|
| 1 | Delete `crates/rustycode-orchestra/` (legacy crate) | ✅ Done | Removed ~300KB dead code |
| 2 | Delete `crates/rustycode-models/` (empty shell) | ✅ Done | Removed empty crate dir |
| 3 | Delete root temp files: `cs`, `test_profile`, `test.txt` | ✅ Done | Root cleaned |
| 4 | Delete `httparty_example.rb` (not project-related) | ✅ Done | Root cleaned |
| 5 | Move `benchmark_leaderboard.json` → `benchmark_results/` | ✅ Done | Then archived with benchmark_results |
| 6 | Clean caches: `.ruff_cache/`, `.bg-shell/`, `.worktrees/` | ✅ Done | Caches regenerated on demand |
| 7 | Fix Cargo.toml formatting (one crate per line) | ✅ Done | Consistent formatting |

### Phase 2: Script Cleanup — ✅ COMPLETED

| # | Action | Status | Detail |
|---|--------|--------|--------|
| 8 | Move root scripts → `scripts/archive/` | ✅ Done | 29 scripts moved |
| 9 | Duplicate scripts archived together | ✅ Done | `hands_on_test.sh` + `hands-on-test.sh` etc. |
| 10 | Move `query_users.py`, `show_themes.py` → `scripts/` | ✅ Done | Utility scripts organized |

### Phase 3: Artifact Cleanup — ✅ COMPLETED

| # | Action | Status | Detail |
|---|--------|--------|--------|
| 11 | Archive `archive/`, `benchmark_results/`, `reports/`, `test-orchestra/` | ✅ Done | Tarball at `.archive-2026-04-13/archive-benchmarks-reports-testorchestra.tar.gz` (1.5MB) |
| 12 | Remove originals from tree | ✅ Done | ~37MB reclaimed from working tree |

### Phase 4: Code Quality — 🔲 REMAINING (higher effort, requires careful review)

| # | Action | Status | Notes |
|---|--------|--------|-------|
| 13 | Audit and resolve `#[allow(dead_code)]` instances | 🔲 TODO | 31 instances across 5 crates |
| 14 | Resolve actionable TODOs in §5.2 | 🔲 TODO | 5 high/medium priority TODOs |
| 15 | Split files over 2,000 lines (§4) | 🔲 TODO | 8 files over 2K lines |
| 16 | Consolidate docs/ directory (§7) | 🔲 TODO | 78 entries with redundancy |

---

## 10. Summary Statistics

| Category | Before | After |
|----------|--------|-------|
| Root-level entries | 82 | **40** |
| Root shell scripts | 29 | **0** |
| Temp/artifact root files | 4 | **0** |
| Non-project utility scripts at root | 3 | **0** |
| Stale/orphan crates | 2 | **0** |
| Stale artifacts on disk | ~37MB | **0** (archived) |
| `#[allow(dead_code)]` instances | 31 | 31 (Phase 4) |
| Actionable TODOs (source) | 5 | 5 (Phase 4) |
| Files over 1,500 lines | 24 | 23 (Phase 4) |
| docs/ entries | 78 | 79 (+ this doc) (Phase 4) |

**Completed cleanup impact**: Removed ~300KB of dead Rust code, eliminated 42 root-level clutter entries, archived ~37MB of stale artifacts to a 1.5MB tarball, fixed workspace configuration formatting.

---

*Generated 2026-04-13. Phases 1–3 executed. Phase 4 (code quality) requires dedicated review sessions.*
