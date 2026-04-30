# rustycode-orchestration

Algorithm core for autonomous development. Owns all reasoning, task execution, quality
evaluation, and model orchestration. Has no knowledge of CLI flags, terminal I/O, or disk
layout — those belong in `rustycode-cli` and `rustycode-tui`.

## Responsibility

> **Orchestration owns the brain. CLI/TUI own the shell.**

| Adding or changing... | Goes in... |
|---|---|
| A new execution strategy or reasoning loop | this crate |
| A quality gate, AST phase, or tier escalation rule | this crate |
| A prompt budgeting or chunking algorithm | this crate |
| A CLI command or TUI event | `rustycode-cli` / `rustycode-tui` |
| Session state visible to the user | `rustycode-cli` / `rustycode-tui` |
| Project directory layout (milestones/slices/tasks) | `rustycode-cli` / `rustycode-tui` |

## Dependency Direction

```
rustycode-orchestration   ← this crate
        ↑
rustycode-cli / rustycode-tui  ← runtime shells; import this crate
```

## Architecture

### Tiered Model Execution (Composer/Conductor/Musician)

Tasks escalate through tiers as complexity demands:

```
Step → Musician (Tier 2, fast model) → success? → done
                ↓ failed
         Editor (Tier 3, capable model) → patch → retry
                ↓ failed
        Composer (Tier 4, most capable) → recompose → retry
                ↓ failed
              abandon + failure pattern stored
```

- **Musician** executes individual steps.
- **Editor** reviews results and patches plans.
- **Composer** rewrites the approach from scratch.
- **Conductor** enforces budgets, detects loops, decides escalation.

### 6-Phase AST Pipeline

For complex tasks, the Adaptive Structured Thinking pipeline runs first:

```
CLASSIFY → RESEARCH → SKELETON → EXPAND → EXECUTE → VERIFY
```

Each phase is a discrete module under `ast/` with its own state, schema, and tests.

## Module Map

| Module | Key Types | Description |
|---|---|---|
| `ast/` | `AstPipeline`, `AstConfig` | 6-phase structured thinking pipeline |
| `conductor/` | `Conductor` | Budget enforcement, escalation decisions |
| `composer/` | `Composer` | Tier 4: full plan recomposition |
| `editor/` | `Editor` | Tier 3: patch and refine |
| `musician/` | `Musician` | Tier 2: step execution |
| `orchestrator/` | `StepOrchestrator` | Wires the four tiers together |
| `pipeline/` | `OrchestrationPipeline` | Top-level task lifecycle coordinator |
| `context/` | `PromptBudget`, `SemanticChunker`, `CacheOptimizer`, `SummaryDistiller` | Prompt budgeting, chunking, cache ordering, distillation |
| `thinking/` | Graph-of-Thoughts types | Canonical reasoning graph. |
| `isolation/` | `WorktreeManager`, `IsolationConfig` | Git worktree isolation for parallel tasks |
| `recovery/` | `CrashLock`, `SessionForensics` | Snapshot-before-execute, crash rollback |
| `fork_join/` | `ForkJoinExecutor` | Parallel branch execution with join |
| `judge/` | `JudgeConfig`, `JudgeVerdict` | Quality evaluation with rubrics |
| `skeptic/` | — | Adversarial quality reviewer |
| `supervisor/` | `RuleBasedSupervisor` | Policy enforcement over execution |
| `harness/` | `TieredHarness`, `TieredExecutionResult` | Canonical multi-tier executor |
| `verification_gates/` | `VerificationGateRegistry` | Step output verification with pluggable strategies |
| `failure_store/` | `FailurePatternStore` | SQLite + in-memory failure pattern storage |
| `task_context/` | `TaskContext`, `TaskComplexity` | Task lifecycle, budget tracking, complexity classification |
| `task_decomposer/` | `TaskDecomposer` | Breaks tasks into executable `Step`s |
| `model_registry/` | `ModelRegistry`, `CostTracker` | Tier-to-model mapping with cost tracking |
| `reasoning_store/` | `ReasoningStore` | Persistent reasoning chain storage |
| `shared_workspace/` | `SharedWorkspace` | Cross-agent artifact sharing |
| `session/` | `OrchestrationSession` | Provider-neutral conversation state |
| `error/` | `OrchestrationError`, `ErrorCategory` | Canonical algorithmic error hierarchy |
| `bus/` | `BusHandle`, `MessageBus` | Internal event bus for inter-module communication |
| `autonomy/` | `AutonomyDecider`, `AutonomyLevel` | Policy: when to proceed vs. pause for human input |

## Usage

```rust
use rustycode_orchestration::pipeline::OrchestrationPipeline;
use rustycode_orchestration::config::OrchestrationConfig;

let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());

// Conduct a task through the unified pipeline
let result = pipeline.conduct("session-id".into(), "Implement user authentication".into()).await?;
```

## Testing

```bash
cargo test -p rustycode-orchestration
```

~1,614 tests across all modules (unit + integration). AST pipeline alone has 456 tests.
