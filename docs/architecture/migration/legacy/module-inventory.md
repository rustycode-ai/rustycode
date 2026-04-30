# Module Inventory Before Consolidation

**Date:** 2026-04-24
**Author:** Sisyphus (automated audit)

## Summary

| Crate | LOC | Modules | Dependencies (unique) |
|-------|-----|---------|----------------------|
| `rustycode-deep-thinker` | 6,047 | 9 top-level + 3 subdirs | 9 (rustycode-llm) |
| `rustycode-orchestra` | 42,521 | 36+ directories | 14+ (rustycode-llm, config, protocol, runtime, tools) |
| `rustycode-orchestration` | 1,986 | 16 files + 1 subdir | 9 (rustycode-llm, tools, deep-thinker, protocol, config) |

## rustycode-deep-thinker (6,047 LOC)

**Purpose:** Graph-of-Thoughts reasoning engine with 5 adaptive strategies.

### Modules
| Module | LOC | Description |
|--------|-----|-------------|
| `core/graph.rs` | 475 | ReasoningGraph data structure, thought nodes |
| `core/scoring.rs` | 376 | 7-factor confidence scoring |
| `core/parsing.rs` | 299 | LLM response parsing |
| `core/pruning.rs` | 250 | Intelligent graph pruning |
| `core/types.rs` | 246 | Operation, Thought, ThoughtId types |
| `core/metacog.rs` | 202 | Metacognitive monitoring |
| `core/knowledge.rs` | 132 | Knowledge source integration |
| `core/error.rs` | 60 | Error types |
| `core/mod.rs` | 17 | Re-exports |
| `executor.rs` | 567 | Main ThinkingExecutor |
| `operations.rs` | 413 | Strategy-specific graph operations |
| `activator.rs` | 409 | Activation signal detection + policies |
| `convergence.rs` | 397 | Convergence detection + termination |
| `persistence.rs` | 387 | Session save/load (JSON + binary) |
| `executor_with_persistence.rs` | 315 | Persistent executor wrapper |
| `prompting/context.rs` | 304 | Prompt context builder |
| `prompting/templates.rs` | 376 | Strategy-specific Handlebars templates |
| `prompting/mod.rs` | 68 | Re-exports |
| `persistence_helpers.rs` | 224 | Serialization helpers |
| `selection.rs` | 129 | Strategy selection logic |
| `strategies/mod.rs` | 76 | Strategy trait + registry |
| `strategies/dialectic.rs` | 39 | Dialectic strategy |
| `strategies/abductive.rs` | 39 | Abductive strategy |
| `strategies/analogical.rs` | 44 | Analogical strategy |
| `strategies/parallel.rs` | 44 | Parallel strategy |
| `strategies/sequential.rs` | 30 | Sequential strategy |
| `lib.rs` | 129 | Crate root |

### Key Public API
- `ThinkingExecutor` — main entry point
- `ReasoningGraph` — thought graph with scoring
- `Thought`, `ThoughtId`, `Operation` — core types
- `ActivationSignals`, `ThinkingActivationPolicy` — activation control
- `SessionManager`, `SerializedGraph` — persistence
- `PromptContext` — template rendering

### External Dependencies
- `rustycode-llm` — LLM provider trait
- `handlebars` — prompt templates
- `bincode` — binary serialization
- `tokio`, `async-trait` — async runtime

## rustycode-orchestration (1,986 LOC)

**Purpose:** Tiered execution pipeline (Musician→Editor→Composer) with budget management.

### Modules
| Module | LOC | Description |
|--------|-----|-------------|
| `verification_gates.rs` | 242 | Quality gate registry + strategies |
| `task_context.rs` | 208 | Task lifecycle, budget, phase tracking |
| `config.rs` | 208 | OrchestrationConfig + sub-configs |
| `pipeline.rs` | 110 | OrchestrationPipeline entry point |
| `conductor.rs` | 133 | Budget enforcement, hallucination detection |
| `failure_store/sqlite.rs` | 211 | SQLite failure pattern backend |
| `error.rs` | 79 | OrchestrationError enum |
| `error_signal.rs` | 106 | ErrorSignal, ErrorCategory, ErrorClassifier |
| `model_registry.rs` | 72 | Tier-to-model mapping |
| `execution_trace.rs` | 81 | Step execution recording |
| `musician.rs` | 66 | Step executor |
| `state_machine.rs` | 47 | Phase state machine |
| `task_decomposer.rs` | 46 | Task decomposition |
| `types.rs` | 31 | Step, Difficulty, OutputType, TaskOutcome |
| `failure_store/memory.rs` | 108 | In-memory failure store |
| `failure_store/metrics_db.rs` | 86 | Failure metrics |
| `failure_store/mod.rs` | 65 | Store trait + re-exports |
| `composer.rs` | 25 | Composer stub |
| `editor.rs` | 33 | Editor stub |
| `lib.rs` | 29 | Crate root |

### Key Public API
- `OrchestrationPipeline` — main entry point
- `Conductor` — budget + lifecycle management
- `Musician`, `Editor`, `Composer` — tier executors
- `TaskContext` — task state + budget
- `OrchestrationConfig` — configuration
- `OrchestrationError` — error type
- `ErrorSignal`, `ErrorCategory` — error classification
- `VerificationGateRegistry` — quality gates

### External Dependencies
- `rustycode-llm` — LLM provider trait
- `rustycode-tools` — tool execution
- `rustycode-deep-thinker` — **already depends on deep-thinker**
- `rustycode-protocol` — shared types
- `rustycode-config` — configuration
- `rusqlite` — SQLite for failure patterns

## rustycode-orchestra (42,521 LOC)

**Purpose:** Full autonomous development framework — service lifecycle, planning, recovery, git, worktree.

### Top-Level Modules (36+)
| Directory | Description |
|-----------|-------------|
| `service/` | Bootstrap, lifecycle, start/stop |
| `execution/` | Step execution engine |
| `planning/` | Plan creation and management |
| `detection/` | Issue/bug detection |
| `verification/` | Quality verification |
| `recovery/` | Failure recovery strategies |
| `git/` | Git operations |
| `worktree/` | Worktree management |
| `session/` | Session lifecycle (forensics, headless) |
| `discovery/` | Codebase discovery |
| `models/` | Model definitions |
| `llm/` | LLM integration |
| `providers/` | Provider management |
| `tools/` | Tool execution |
| `observability/` | Monitoring and metrics |
| `cache/` | LRU + TTL cache, request dedup |
| `context/` | Context budget, prompt compression |
| `thinking/` | **DUPLICATE: Thin wrapper around deep-thinker** |
| `config/` | Commands config, remote questions |
| `harnesses/` | Test harness definitions |
| `cli/` | CLI interface |
| `coordinator/` | Task coordination |
| `convoy/` | Batch processing |
| `migration/` | Migration utilities |
| `phases/` | Phase management |
| `swebench/` | SWE-bench integration |
| `files/` | File operations |
| `fixture/` | Test fixtures |
| `state/` | State management |

### Key Public API
- `Orchestra` / `OrchestraService` — main orchestrator
- `OrchestraExecutor` — execution engine
- `PlanMode` — planning mode handler
- `SessionManager` — session lifecycle

### External Dependencies
- `rustycode-llm`, `rustycode-config`, `rustycode-protocol`, `rustycode-runtime`, `rustycode-tools`
- Heavy dependency footprint (chrono, cron, rand, regex, sha2, walkdir, etc.)

## Overlap Analysis

### Critical Duplications

1. **Thinking/Reasoning**
   - `orchestra/src/thinking/` → re-export layer
   - `deep-thinker/src/executor.rs` → actual engine
   - `orchestration/src/conductor.rs` → tier orchestration
   - **Action:** orchestration owns canonical `thinking/`

2. **Error Handling**
   - `deep-thinker/src/core/error.rs` → `Error` enum
   - `orchestration/src/error.rs` → `OrchestrationError` enum
   - `orchestration/src/error_signal.rs` → `ErrorSignal` + `ErrorCategory`
   - `orchestra/src/error.rs` → `OrchestraError` enum
   - **Action:** Single unified error type

3. **Session Management**
   - `deep-thinker/src/persistence.rs` → `SessionManager` (graph persistence)
   - `orchestration/src/task_context.rs` → `TaskContext` (task state)
   - `orchestra/src/session/` → Full session lifecycle (5 files, 2,580 LOC)
   - **Action:** Consolidate session into orchestration

4. **Model Registry**
   - `orchestration/src/model_registry.rs` → tier-to-model mapping
   - `orchestra/src/models/` → model definitions
   - **Action:** Unified registry

5. **Tool Execution**
   - `orchestration/` → depends on `rustycode-tools`
   - `orchestra/src/tools/` → tool wrappers
   - **Action:** Single tool interface

### Shared Dependencies
All three depend on: `rustycode-llm`, `tokio`, `serde`, `anyhow`, `async-trait`, `thiserror`

### Dependency Chain
```
orchestra → (runtime, tools, llm, config, protocol)
orchestration → (deep-thinker, tools, llm, config, protocol)
deep-thinker → (llm)
```

Note: `orchestration` already depends on `deep-thinker` — consolidation here is natural.
