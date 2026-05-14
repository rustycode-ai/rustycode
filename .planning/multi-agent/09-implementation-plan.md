# 09 — Implementation Plan

Three phases organized into parallel waves. Each wave's tasks can run simultaneously.
Each task has goals, validation, and tests.

---

## Wave Dependency Map

```
Phase 1 (all parallel):
  Wave 0: 1.1  1.2  1.3  1.4  1.5

Phase 2 (5 waves):
  Wave 1: 2.1  2.2  2.3  2.4  2.5  2.14        ← all independent
  Wave 2: 2.6  2.7  2.8  2.9  2.10              ← depend on Wave 1 types
  Wave 3: 2.11 2.12 2.13                         ← wiring to existing code
  Wave 4: 2.15                                   ← doc update
  Wave 5: 2.16 2.17 2.18                         ← lifecycle hooks (from doc 07 updates)

Phase 3 (3 waves):
  Wave 6: 3.1                                    ← config
  Wave 7: 3.2 3.3 3.4                            ← ensemble core (parallel)
  Wave 8: 3.5 3.6                                ← CLI wiring + doc update
```

---

## Phase 1: Document Current State

**Goal:** Bring all architecture docs to 100% accuracy with current code.

### Wave 0 — All tasks parallel

| # | Task | Agent type | Crate(s) | File(s) to verify |
|---|------|-----------|----------|-------------------|
| 1.1 | ~~Delete old doc~~ DONE | — | `.planning/` | — |
| 1.2 | Verify structs in docs 02-08 match source | code-explorer | All | agent-runtime, orchestration, team, session, protocol |
| 1.3 | Add missing methods to doc tables | code | orchestration | task_context.rs, orchestrator.rs |
| 1.4 | Document SessionSnapshot vs CompactionSnapshot | code | session, protocol | session.rs (both crates) |
| 1.5 | Verify model catalog entries match registry.rs | code-explorer | providers | registry.rs |

### Validation

- [ ] Every struct in docs has matching `pub struct` in source
- [ ] Every enum variant in docs has matching variant in source
- [ ] Every file path in key file map resolves to an existing file
- [ ] No TODO/placeholder text remains

### Tests

- `cargo test --workspace` passes (baseline, no code changes)

---

## Phase 2: Implement Context Objects + Lifecycle Hooks

**Goal:** Introduce `AgentContext`, `AgentOutcome`, `ReasoningSummary`, `ConvergenceView`,
`TeamContext`, onboarding/offboarding hooks, and `ProviderContext`.

### Wave 1 — Protocol types (all independent, run in parallel)

| # | Task | Agent type | Crate | Scope |
|---|------|-----------|-------|-------|
| 2.1 | `ReasoningSummary` + `Insight` types | code | protocol | New file `src/reasoning_summary.rs` |
| 2.2 | `FileChange` + `FileSnippet` types | code | protocol | New file `src/file_context.rs` |
| 2.3 | `UsageStats` (unify token counting) | code | protocol | New file `src/usage_stats.rs` |
| 2.4 | `BudgetAllocation` type | code | protocol | New file `src/budget.rs` |
| 2.5 | `ToolScope` type | code | protocol | New file `src/tool_scope.rs` |
| 2.14 | Document `SharedWorkspace` API | code | orchestration | Doc comments on existing code |

**Validation per task:**
- [ ] Type compiles with `cargo check -p rustycode-protocol`
- [ ] `Serialize`/`Deserialize` derived
- [ ] Unit test for construction and serialization round-trip

### Wave 2 — Composite types (depend on Wave 1, run in parallel)

| # | Task | Agent type | Crate | Depends on | Scope |
|---|------|-----------|-------|------------|-------|
| 2.6 | `AgentContext` struct | code | orchestration | 2.1-2.5 | New file `src/agent_context.rs` |
| 2.7 | `AgentOutcome` struct | code | orchestration | 2.1-2.3 | New file `src/agent_outcome.rs` |
| 2.8 | Add `reasoning_summary` to `HandoffPackage` | code | orchestration | 2.1 | Edit `src/handoff.rs` |
| 2.9 | `ConvergenceView` + `DissentingOpinion` | code | team | 2.1 | New file `src/convergence.rs` |
| 2.10 | `TeamContext` struct | code | team | 2.9 | New file `src/team_context.rs` |

**Validation per task:**
- [ ] Type compiles with `cargo check -p <crate>`
- [ ] Builder/constructor works
- [ ] Serialization round-trip test

### Wave 3 — Wiring (depend on Wave 2, run in parallel)

| # | Task | Agent type | Crate | Depends on | Scope |
|---|------|-----------|-------|------------|-------|
| 2.11 | Wire `AgentContext` in `AgentSessionExecutor` | code | orchestration | 2.6 | Edit `src/agent_executor.rs` |
| 2.12 | Wire `AgentOutcome` in `AgentSession` | code | agent-runtime | 2.7 | Edit `src/session.rs` |
| 2.13 | Wire `ConvergenceView` in `Coordinator` | code | team | 2.9 | Edit `src/coordinator.rs` |

**Validation per task:**
- [ ] Existing tests still pass: `cargo test -p <crate>`
- [ ] New integration point produces correct output
- [ ] No breaking changes to public API

### Wave 4 — Doc update (single task)

| # | Task | Agent type | Depends on |
|---|------|-----------|------------|
| 2.15 | Update docs 04, 08 to reflect implementation | code | 2.1-2.13 |

### Wave 5 — Lifecycle Hooks (from doc 07 onboarding/offboarding model)

| # | Task | Agent type | Crate | Scope |
|---|------|-----------|-------|-------|
| 2.16 | Implement `ProviderContext` on `AgentSession` | code | agent-runtime | Edit `src/session.rs` |
| 2.17 | Implement onboarding/offboarding as `AgentPlugin` | code | agent-runtime | New file `src/plugins/lifecycle.rs` |
| 2.18 | Wire handoff summaries into `CompactionSnapshot` | code | session | Edit `src/compaction.rs` |

**Validation for Wave 5:**
- [ ] `ProviderContext` carries model, auth, rate-limit settings
- [ ] Onboarding loads relevant session history slice
- [ ] Offboarding serializes handoff summary
- [ ] `CompactionSnapshot` includes `handoff_summaries` map
- [ ] Round-trip: onboard → execute → offboard → compact → resume

### Tests for Phase 2

| Test | Wave | What it validates |
|------|------|-------------------|
| `reasoning_summary_from_graph` | 1 | Top-N insight extraction |
| `usage_stats_arithmetic` | 1 | Addition/saturation |
| `budget_allocation_split` | 1 | Subdivision math |
| `agent_context_construction` | 2 | Builds from executor state |
| `agent_outcome_from_result` | 2 | Maps all fields from AgentResult |
| `convergence_view_aggregation` | 2 | Mean/max confidence from summaries |
| `handoff_with_reasoning` | 2 | Serialization round-trip |
| `team_context_from_outcomes` | 2 | Aggregates outcomes correctly |
| `agent_context_wired` | 3 | Executor produces valid AgentContext |
| `agent_outcome_wired` | 3 | Session produces valid AgentOutcome |
| `convergence_wired` | 3 | Coordinator produces valid ConvergenceView |
| `provider_context_creation` | 5 | ProviderContext carries all fields |
| `lifecycle_onboard_offboard` | 5 | Full lifecycle round-trip |
| `compaction_with_handoff` | 5 | CompactionSnapshot includes summaries |

### Estimated Scope

- ~200 LOC new types in `rustycode-protocol` (Wave 1)
- ~250 LOC composite types in orchestration + team (Wave 2)
- ~200 LOC wiring in 3 crates (Wave 3)
- ~150 LOC lifecycle hooks (Wave 5)
- ~400 LOC tests across 4 crates

---

## Phase 3: Implement Ensemble Layer

**Goal:** Multiple teams with shared `ConvergenceView` and consensus.

### Wave 6 — Config (single task)

| # | Task | Agent type | Crate | Scope |
|---|------|-----------|-------|-------|
| 3.1 | `EnsembleConfig` + `EnsembleOrchestrator` skeleton | code | team | New file `src/ensemble.rs` |

### Wave 7 — Core (3 parallel tasks)

| # | Task | Agent type | Crate | Depends on | Scope |
|---|------|-----------|-------|------------|-------|
| 3.2 | `EnsembleOrchestrator` execution loop | code | team | 3.1 | Edit `src/ensemble.rs` |
| 3.3 | Consensus mechanisms | code | team | 3.1 | New file `src/consensus.rs` |
| 3.4 | Shared `ConvergenceView` for ensemble | code | team | 3.1 | Edit `src/convergence.rs` |

### Wave 8 — Integration (2 parallel tasks)

| # | Task | Agent type | Crate | Depends on | Scope |
|---|------|-----------|-------|------------|-------|
| 3.5 | Wire ensemble into CLI/TUI | code | cli, tui | 3.2 | Edit mode selection |
| 3.6 | Update doc 06 | code | .planning/ | 3.1-3.5 | Edit 06-teams-and-ensembles.md |

### Validation

- [ ] Ensemble dispatches sub-tasks to multiple teams
- [ ] Teams produce `TeamContext` correctly
- [ ] Shared `ConvergenceView` aggregates from all teams
- [ ] Consensus mechanisms produce correct decisions
- [ ] Dissenting opinions surfaced when no consensus
- [ ] CLI can launch an ensemble run

### Tests for Phase 3

| Test | Wave | What it validates |
|------|------|-------------------|
| `ensemble_simple_majority` | 7 | 3 teams, 2 agree → majority |
| `ensemble_weighted_confidence` | 7 | High-confidence outweighs low |
| `ensemble_unanimous_veto` | 7 | Single dissent blocks unanimous |
| `ensemble_convergence_view` | 7 | Shared view aggregates all teams |
| `ensemble_budget_enforcement` | 7 | Budget distributed across teams |
| `ensemble_dissent_surface` | 7 | Dissenting opinions preserved |
| `ensemble_cli_launch` | 8 | CLI can start ensemble mode |

### Estimated Scope

- ~400 LOC ensemble + consensus in `rustycode-team`
- ~100 LOC CLI/TUI wiring
- ~250 LOC tests

---

## Parallel Execution Summary

| Wave | Parallel tasks | Total agents | Estimated time |
|------|---------------|-------------|----------------|
| 0 | 4 | 4 | ~5 min |
| 1 | 6 | 6 | ~10 min |
| 2 | 5 | 5 | ~10 min |
| 3 | 3 | 3 | ~10 min |
| 4 | 1 | 1 | ~5 min |
| 5 | 3 | 3 | ~10 min |
| 6 | 1 | 1 | ~5 min |
| 7 | 3 | 3 | ~15 min |
| 8 | 2 | 2 | ~10 min |

**Total: 9 waves, ~80 min estimated wall-clock with parallel agents**

---

## Success Criteria (All Phases)

- [ ] All workspace tests pass (`cargo test --workspace`)
- [ ] Zero clippy warnings (`cargo clippy --workspace --all-targets -- -D warnings`)
- [ ] Architecture docs match code with no discrepancies
- [ ] New types have >80% test coverage
- [ ] No breaking changes to existing public APIs
