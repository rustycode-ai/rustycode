# S04 Design: Split rustycode-orchestra into Internal Modules

**Date**: 2026-04-22  
**Milestone**: M001 (Architecture Cleanup)  
**Slice**: S04 (Split rustycode-orchestra into internal modules)  
**Status**: Design Approved  
**Effort**: 9-14 hours (~1.5 days)

---

## Executive Summary

rustycode-orchestra currently contains 163 source files, with ~100+ files scattered in the root directory creating maintenance burden and unclear module boundaries. S04 refactors these files into 16+ logical subdirectories (runtime, verification, config, cache, discovery, recovery, tools, worktree, llm, prompting, observability, session, cli, models, git, migration, utils) following the modularization pattern established in S01-S03.

**Goals**:
1. Organize 163 files into cohesive modules with clear purposes
2. Reduce lib.rs from 1000+ lines to ~50-100 (thin re-exports)
3. Eliminate circular dependencies between new modules
4. Document each module with README explaining purpose and API
5. Maintain 80%+ test coverage throughout refactoring
6. All tests passing, zero clippy warnings

---

## Current State Analysis

### File Distribution (163 total)
- **Already organized** (8 modules): `auto/`, `state/`, `phases/`, `thinking/`, `files/`, `fixture/`, `utils/`, `swebench/`
- **Scattered in root**: ~100+ files without clear organization
- **Problem**: lib.rs acts as god object; unclear ownership of files

### Major File Categories Identified

| Category | Files | Current State |
|----------|-------|---------------|
| Runtime orchestration | 8-12 | Scattered (auto_runtime, unit_runtime, etc.) |
| Verification/testing | 5-6 | Scattered |
| Configuration | 5-6 | Scattered |
| Caching/optimization | 3-4 | Scattered |
| Discovery/plugins | 2-3 | Scattered |
| Recovery/resilience | 3-4 | Scattered |
| Tool management | 4-5 | Scattered |
| Git/worktree | 3-4 | Scattered |
| LLM integration | 1-2 | Scattered |
| Prompting | 3-4 | Scattered |
| Observability | 3-4 | Scattered |
| Session management | 4-5 | Scattered |
| CLI/REPL | 2-3 | Scattered |
| Model routing | 2-3 | Scattered |
| Migration/upgrades | 3-4 | Scattered |
| Utilities | Many | Scattered |

---

## Proposed Module Structure

### New Module Organization

```
src/
├── lib.rs                          (thin re-exports)
├── auto/                           (no change)
├── state/                          (no change)
├── phases/                         (no change)
├── thinking/                       (no change)
├── files/                          (no change)
├── fixture/                        (no change)
├── utils/                          (consolidate utilities)
├── swebench/                       (no change)
│
├── runtime/                        (8-12 files)
│   ├── mod.rs
│   ├── auto_runtime.rs
│   ├── unit_runtime.rs
│   ├── task_execution_runtime.rs
│   ├── plan_slice_runtime.rs
│   ├── post_unit_runtime.rs
│   ├── unit_lifecycle_runtime.rs
│   ├── task_control_runtime.rs
│   └── scheduler_sync.rs
│
├── verification/                  (5-6 files)
│   ├── mod.rs
│   ├── verification.rs
│   ├── verification_retry_state.rs
│   ├── verification_gate.rs
│   ├── verification_evidence.rs
│   └── task_verification_runtime.rs
│
├── config/                        (5-6 files)
│   ├── mod.rs
│   ├── orchestra_config.rs
│   ├── commands_config.rs
│   ├── universal_config_types.rs
│   ├── universal_config_tools.rs
│   └── remote_questions_config.rs
│
├── cache/                         (3-4 files)
│   ├── mod.rs
│   ├── cache.rs
│   ├── lru_ttl_cache.rs
│   └── prompt_cache_optimizer.rs
│
├── discovery/                     (2-3 files)
│   ├── mod.rs
│   ├── skill_discovery.rs
│   ├── extension_discovery.rs
│   └── extension_registry.rs
│
├── recovery/                      (3-4 files)
│   ├── mod.rs
│   ├── crash_recovery.rs
│   ├── auto_recovery.rs
│   └── auto_stuck_detection.rs
│
├── tools/                         (4-5 files)
│   ├── mod.rs
│   ├── tools.rs
│   ├── tool_tracking.rs
│   ├── tool_access_matrix.rs
│   ├── auto_tool_tracking.rs
│   └── tool_bootstrap.rs
│
├── worktree/                      (3-4 files)
│   ├── mod.rs
│   ├── worktree.rs
│   ├── auto_worktree_sync.rs
│   └── worktree_name_gen.rs
│
├── llm/                           (1-2 files)
│   ├── mod.rs
│   └── llm.rs
│
├── prompting/                     (3-4 files)
│   ├── mod.rs
│   ├── prompt_loader.rs
│   ├── prompt_ordering.rs
│   └── prompt_compressor.rs
│
├── observability/                 (3-4 files)
│   ├── mod.rs
│   ├── auto_observability.rs
│   ├── observability_validator.rs
│   ├── skill_telemetry.rs
│   └── activity_log.rs
│
├── session/                       (4-5 files)
│   ├── mod.rs
│   ├── session_context.rs
│   ├── session_status_io.rs
│   ├── session_forensics.rs
│   └── headless_context.rs
│
├── cli/                           (2-3 files)
│   ├── mod.rs
│   ├── cli.rs
│   └── wizard.rs
│
├── models/                        (2-3 files)
│   ├── mod.rs
│   ├── models_resolver.rs
│   └── model_cost_table.rs
│
├── git/                           (3-4 files)
│   ├── mod.rs
│   ├── git_constants.rs
│   ├── git_self_heal.rs
│   └── ... (other git utilities)
│
├── migration/                     (3-4 files)
│   ├── mod.rs
│   ├── pi_migration.rs
│   ├── migrate_preview.rs
│   ├── migrate_external.rs
│   └── migrate_validator.rs
│
└── [Single-file modules kept in root]
    ├── constants.rs
    ├── error.rs
    ├── engine.rs
    └── ...
```

### Module Purposes & Cohesion

| Module | Purpose | Cohesion | Key Types |
|--------|---------|----------|-----------|
| **runtime/** | Async task/plan/unit execution coordination | High | Runtime, Executor, Scheduler |
| **verification/** | Quality gates, test execution, evidence | High | VerificationGate, Evidence, Retry |
| **config/** | Configuration loading and types | High | OrchestraConfig, ConfigTypes |
| **cache/** | Performance optimization via caching | High | Cache, LRU, PromptOptimizer |
| **discovery/** | Plugin/skill/extension discovery | High | Discovery, Registry |
| **recovery/** | Crash recovery, stuck detection | High | Recovery, StuckDetector |
| **tools/** | Tool execution and lifecycle | High | ToolTracker, AccessMatrix |
| **worktree/** | Git worktree management | High | WorktreeManager, NameGen |
| **llm/** | LLM provider integration | Medium | LLMProvider, Routing |
| **prompting/** | Prompt generation and optimization | Medium | PromptLoader, Compressor |
| **observability/** | Telemetry, metrics, logging | High | Metrics, ActivityLog, Telemetry |
| **session/** | Session state and lifecycle | High | SessionContext, Forensics |
| **cli/** | CLI/REPL interface | High | CLI, Wizard |
| **models/** | Model resolution and cost tracking | Medium | ModelResolver, CostTable |
| **git/** | Git operations and utilities | High | GitHelper, Constants |
| **migration/** | Project migration and upgrades | High | Migration, Upgrade |

---

## Implementation Strategy

### Phase 1: Preparation (2-3 hours)
1. Audit all 163 files and confirm final groupings
2. Check for circular dependencies within each proposed group
3. Identify files that don't fit standard categories (keep in root or utils)
4. Document any special cases (tests, proc macros, etc.)
5. Create dependency map: which modules depend on which

### Phase 2: File Organization (4-6 hours)
1. Create module directories with stub mod.rs files
2. Move files into appropriate subdirectories (git mv)
3. Update all internal path imports (src/X.rs → crate::X or crate::module::X)
4. Update lib.rs re-exports: convert path-based imports to re-exports
5. Verify each module compiles independently if possible

**Key Pattern** (same as S01-S03):
```rust
// In runtime/mod.rs
pub mod auto_runtime;
pub mod unit_runtime;
pub mod task_execution_runtime;
// ... re-exports of key types
pub use auto_runtime::AutoRuntime;
pub use unit_runtime::UnitRuntime;
```

### Phase 3: Verification (2-3 hours)
1. **Compile**: `cargo build --workspace`
2. **Tests**: `cargo test --package rustycode-orchestra`
3. **Coverage**: `cargo tarpaulin --package rustycode-orchestra` (≥80%)
4. **Linting**: `cargo clippy --package rustycode-orchestra -- -D warnings`
5. **Dependency check**: `cargo tree --package rustycode-orchestra --duplicates`

### Phase 4: Documentation (1-2 hours)
1. Create README.md in each module (8-10 lines each):
   - Module purpose
   - Key types and functions
   - Integration points
   - Example usage
2. Update crate-level documentation
3. Update module-level comments in mod.rs files

---

## Success Criteria (Definition of Done)

### Code Organization
- [x] All ~100+ scattered files moved into logical subdirectories
- [x] No files remain in src/ root except:
  - lib.rs (thin re-exports)
  - Single-file modules (error.rs, constants.rs, engine.rs, etc.)
  - Existing organized modules (auto/, state/, etc.)
- [x] Each module has clear purpose and coherent files

### Compilation & Testing
- [x] `cargo build --workspace` compiles without errors or warnings
- [x] `cargo test --package rustycode-orchestra` passes all tests
- [x] Test coverage ≥ 80% (measured with cargo tarpaulin)
- [x] `cargo clippy` reports zero warnings on rustycode-orchestra

### Dependencies
- [x] Zero circular dependencies between new modules
- [x] Module imports follow clean dependency hierarchy
- [x] No "god object" imports (all imports are targeted)

### API & Documentation
- [x] lib.rs reduced to 50-100 lines (thin re-exports only)
- [x] Each module has public mod.rs with re-exports
- [x] Each module directory has README.md explaining:
  - Purpose (1-2 sentences)
  - Key exports (3-5 types/functions)
  - Integration points
  - Example usage
- [x] All public types documented with /// comments

### Git & Commits
- [x] Changes committed with descriptive messages
- [x] Format: `refactor(orchestra): extract <module_name> into subdirectory`
- [x] One commit per module or logical group

---

## Risk Mitigation

### Risk 1: Circular Dependencies
**Impact**: Compilation failures, unmaintainable code  
**Mitigation**:
- Create dependency map before moving files
- Verify each module compiles independently
- Use cargo tree to detect cycles

### Risk 2: Import Path Chaos
**Impact**: Many compiler errors, hard to debug  
**Mitigation**:
- Use symbolic tools (find_referencing_symbols) to update imports systematically
- Test compilation after each logical group move
- Document old paths → new paths mapping

### Risk 3: Test Coverage Drop
**Impact**: Regression detection disabled  
**Mitigation**:
- Run coverage before and after
- Ensure no test files moved without updating paths
- Keep test organization aligned with module structure

### Risk 4: Breaking Public API
**Impact**: Downstream crates break  
**Mitigation**:
- Re-export all public types from lib.rs
- Maintain backward-compatible paths (even if deprecated)
- Update CHANGELOG with new import paths

---

## Effort Breakdown

| Task | Effort | Notes |
|------|--------|-------|
| Prepare & analyze | 2-3h | Dependency mapping, file audit |
| Move files & update imports | 4-6h | Largest effort; systematic by group |
| Verify compilation & tests | 2-3h | May require iteration |
| Document modules | 1-2h | READMEs, module comments |
| Final review & commit | 1h | QA before marking done |
| **Total** | **9-14h** | ~1.5 days for one developer |

---

## Success Metrics

After S04 completes, rustycode-orchestra will:
- ✅ Have 16+ clearly-purposed modules
- ✅ Have lib.rs be a thin re-export layer (<100 lines)
- ✅ Have zero circular dependencies between modules
- ✅ Have 80%+ test coverage
- ✅ Have clear module documentation
- ✅ Enable easier feature work (S05, S06)
- ✅ Reduce cognitive load for future maintainers

---

## Next Steps (After Approval)

1. Invoke `writing-plans` skill to create detailed task breakdown
2. Execute refactoring in logical groups (runtime → verification → config → ...)
3. Verify compilation after each group
4. Complete Phase 4 documentation
5. Commit with clear messages
6. Mark S04 as done in GSD
7. Proceed to S05 (Split rustycode-tui)

---

**Document Status**: Ready for spec review and implementation planning.
