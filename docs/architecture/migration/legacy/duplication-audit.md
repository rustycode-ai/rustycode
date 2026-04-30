# Duplication Audit: Three-Crate Orchestration Stack

**Date:** 2026-04-24
**Status:** Audit complete, ready for consolidation

## Overview

Three crates with significant overlap:

| Aspect | deep-thinker | orchestration | orchestra |
|--------|-------------|---------------|-----------|
| **LOC** | 6,047 | 1,986 | 42,521 |
| **Purpose** | Graph-of-Thoughts reasoning | Tiered execution pipeline | Full autonomous framework |
| **Thinking** | ✅ Full engine | ✅ Canonical Graph-of-Thoughts engine | ✅ Re-exported from orchestration |
| **Error type** | `Error` (core/error.rs) | `OrchestrationError` | `OrchestraError` wraps orchestration errors |
| **Session** | `SessionManager` (persistence) | `TaskContext` (budget/phase) | Full lifecycle (5 files) |

## Duplication #1: Thinking/Reasoning (CRITICAL)

**orchestra/src/thinking/** — Deleted.

**orchestration/src/thinking/** — Canonical implementation:
- `core/graph.rs` → real `ReasoningGraph` with edges and roots
- `core/types.rs` → full `Thought`, `ThoughtId`, `Edge`, `ThoughtKind`, `ThoughtMetadata`
- `core/scoring.rs` → multi-factor confidence scoring
- `core/metacog.rs` → metacognitive monitoring

**Action:** Keep orchestra as a re-export layer only.

## Duplication #2: Error Types (HIGH)

Three separate error enums:

| Error Type | Crate | Variants |
|-----------|-------|----------|
| `deep_thinker::core::error::Error` | deep-thinker | GraphError, ThoughtNotFound, ParseError, ExecutionError, ConvergenceError, StrategyError, Io |
| `OrchestrationError` | orchestration | Config, LLM, Tool, Execution, Budget, Conductor, Verification, etc. |
| `OrchestraError` | orchestra | Io, ConfigurationError, NotInitialized, Harness, and `Orchestration(OrchestrationError)` |

**Overlapping semantics:**
- Algorithmic failures now flow through `OrchestraError::Orchestration(...)`
- `OrchestraError` retains only shell/runtime concerns

**Action:** Keep `OrchestrationError` as base. Add thinking-specific variants. Keep `OrchestraError` for backward compat but have it wrap `OrchestrationError`.

## Duplication #3: Session/Context Management (MEDIUM)

| Feature | deep-thinker | orchestration | orchestra |
|---------|-------------|---------------|-----------|
| Persistence | `SessionManager` (JSON + binary) | `failure_store/` (SQLite + memory) | `session/` (5 files, 2,580 LOC) |
| Task context | None (stateless executor) | `TaskContext` (budget, phase, constraints) | `execution::TaskContext` (different struct) |
| State tracking | `ReasoningGraph` | `ExecutionTrace` | `state/` module |

**Action:** Consolidate into unified session module in orchestration.

## Duplication #4: Model Registry (LOW)

- `orchestration/src/model_registry.rs` (72 LOC) → tier-to-model mapping with cost tracking
- `orchestra/src/models/` → model definitions + resolution
- Both map model names to capabilities

**Action:** Single registry in orchestration, orchestra re-exports.

## Already Integrated (No Duplication)

- `orchestration` already depends on `deep-thinker` in Cargo.toml
- `orchestra` has `orchestration_adapter/` that bridges `ComplexityTier` → `TaskComplexity`
- `orchestra_executor.rs` dispatches to harnesses (independent of orchestration)

## Dependency Chain

```
orchestra ──→ orchestration ──→ deep-thinker ──→ rustycode-llm
    │              │                                   ↑
    │              ├→ rustycode-tools                   │
    │              ├→ rustycode-protocol                │
    │              └→ rustycode-config                  │
    ├→ rustycode-runtime
    ├→ rustycode-tools
    ├→ rustycode-config
    └→ rustycode-protocol
```

**Circular dependency risk:** None currently. orchestra → orchestration is one-way.

## Consolidation Strategy

1. **Phase 1:** Completed: orchestration owns canonical `thinking/`
2. **Phase 2:** Completed: orchestra re-exports `orchestration::thinking`
3. **Phase 3:** In progress: keep tightening error boundaries
4. **Phase 4:** Consolidate session management
5. **Phase 5:** Document the split everywhere it matters
