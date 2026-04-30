# Tiered Model Orchestration for Terminal-Bench

**Date**: 2026-04-24
**Status**: Design Review
**Owner**: RustyCode Deep-Thinker Team
**Version**: 1.0

---

## Executive Summary

This document specifies a **model-agnostic tiered orchestration system** that enables RustyCode to solve complex multi-step problems (such as those in [Terminal-Bench 2.0](https://www.tbench.ai/benchmarks/terminal-bench-2)) reliably and cost-effectively, regardless of the underlying LLM model quality.

**Core Insight**: Instead of relying on a single powerful model to reason through complex tasks, the system decomposes tasks into small steps that even weak models can orchestrate, and escalates to progressively more capable models only when necessary.

**Goals**:
- **Higher solve rate**: Solve 95%+ of terminal-bench tasks across all difficulty levels
- **Lower cost**: 80-85% of tasks run on the cheapest available model (Haiku/GPT-3.5 class)
- **Better reliability**: Graceful degradation with any combination of available models
- **Faster convergence**: Tight feedback loops catch errors early, reduce wasted turns
- **Model portability**: Works with Claude, OpenAI, or local models with zero code changes

**Non-Goals**:
- Training new models
- Building a new LLM provider abstraction (reuse existing `rustycode-llm`)
- Replacing the existing deep-thinker engine (it becomes Tier 4)

---

## 1. Motivation

### 1.1 Current Problems

Analysis of `~/.rustycode/debug.log` revealed three critical issues limiting deep-thinker effectiveness:

1. **Thinking timeouts at 60s with only 40-100 char thinking outputs** — Complex multi-step reasoning is cut off before completion
2. **Activation threshold too low (3)** — Fires on simple tasks, wastes budget on exploration
3. **Sparse thinking outputs** — Model isn't using its thinking budget effectively; prompt injection may interfere

### 1.2 The "Weak Model" Challenge

Terminal-Bench 2.0 contains 89 diverse tasks across:
- Scientific computing (statistical sampling, Bayesian networks, DNA assembly)
- Software engineering (compilation, optimization, polyglot programming)
- Security (cryptanalysis, vulnerability identification)
- Data processing (large-scale ETL, log analysis)
- Machine learning (model inference, training, tensor parallelism)
- System administration (server configuration, VM setup, database recovery)

Solving these reliably with *any* model requires treating reasoning as an *orchestration problem*, not a *model capability problem*.

---

## 2. Architecture Overview

### 2.1 High-Level Pipeline

```
┌──────────────────────────────────────────────────────────────┐
│  Task Input (terminal-bench task description)               │
└─────────────────────┬────────────────────────────────────────┘
                      ▼
┌──────────────────────────────────────────────────────────────┐
│  Tier 1: Task Decomposer                                    │
│  - Parse task description                                   │
│  - Break into 5-10 micro-steps                              │
│  - Classify difficulty & expected output per step           │
└─────────────────────┬────────────────────────────────────────┘
                      ▼
┌──────────────────────────────────────────────────────────────┐
│  Plan Refiner (consults FailurePatternStore)                │
│  - Review plan against historical failure patterns          │
│  - Suggest adjustments or approve                           │
└─────────────────────┬────────────────────────────────────────┘
                      ▼
┌──────────────────────────────────────────────────────────────┐
│  Tier 2: Simple-Reasoning Model (Haiku-class)               │
│  - Execute steps, emit tool calls                           │
│  - Build ExecutionTrace                                     │
│  - Max 2 attempts per step                                  │
└─────────────────────┬────────────────────────────────────────┘
                      ▼
┌──────────────────────────────────────────────────────────────┐
│  Verification Gate (lightweight, heuristic + optional LLM)  │
│  - Validate output is logically valid                       │
│  - Catch garbage writes, invalid syntax, logic errors       │
└─────────────────────┬────────────────────────────────────────┘
                      │
          ┌───────────┴────────────┐
          │ Valid?                 │ Invalid/Failed?
          ▼                        ▼
      Continue            ┌─────────────────────┐
                          │  Plan Refiner       │
                          │  (escalation path)  │
                          └─────────┬───────────┘
                                    ▼
                          ┌──────────────────────┐
                          │ Tier 3: Medium-      │
                          │ Reasoning (Sonnet)   │
                          │ - Receives Trace     │
                          │ - Replans            │
                          │ - Max 2 attempts     │
                          └─────────┬────────────┘
                                    │
                          ┌─────────┴──────────┐
                          │ Success?           │ Fail?
                          ▼                    ▼
                      Continue          ┌──────────────────────┐
                                        │ Tier 4: Advanced-    │
                                        │ Reasoning (Opus +    │
                                        │ extended thinking)   │
                                        │ - Full reasoning     │
                                        │ - 1 attempt          │
                                        └─────────┬────────────┘
                                                  │
                                          ┌───────┴────────┐
                                          │ Success?       │ Fail?
                                          ▼                ▼
                                      Continue         [ABANDON]
```

### 2.2 Key Design Principles

1. **Fail fast, escalate smart** — Each tier gets a bounded number of attempts before escalating
2. **Model as orchestrator, not reasoner** — Weak models execute pre-planned steps; strong models plan
3. **Immutable Execution Trace** — Higher tiers receive clean trace data, not raw LLM conversation history
4. **Learning from escalations** — FailurePatternStore captures what fails where, improves decomposition over time
5. **Cost-optimized by default** — Always pick cheapest available model that meets reasoning requirement
6. **Graceful degradation** — Works with 1, 2, or 3+ models; scales with what's available

---

## 3. Components

### 3.1 Task Decomposer (Tier 1)

**Purpose**: Break task description into executable micro-steps.

**Interface**:
```rust
pub trait TaskDecomposer: Send + Sync {
    async fn decompose(&self, task: &str, context: &DecompositionContext) -> Result<DecomposedTask>;
}

pub struct DecomposedTask {
    pub original_task: String,
    pub task_category: String,       // e.g., "rust_refactoring", "data_etl"
    pub steps: Vec<Step>,
    pub estimated_difficulty: Difficulty,
}

pub struct Step {
    pub id: String,                  // UUID
    pub index: u8,                   // 0-based
    pub description: String,
    pub expected_output_type: OutputType,  // File, Command, Query, etc.
    pub suggested_tool: Option<String>,
    pub retry_on_failure: bool,
}

pub enum Difficulty { Easy, Medium, Hard }
pub enum OutputType { File, Command, Query, Code, Data, Verification }
```

**Implementation Notes**:
- Uses medium-reasoning model (Tier 3) initially
- As FailurePatternStore accumulates data, decomposition quality improves
- Decomposer itself does NOT modify plans; that's the Plan Refiner's job

### 3.2 Plan Refiner

**Purpose**: Review proposed plans against historical failure patterns; suggest or approve.

**Runs at two integration points**:
1. **Before Tier 2 execution** — Initial plan review
2. **On tier escalation (Tier 2 → 3, Tier 3 → 4)** — Refines plan using ExecutionTrace

**Interface**:
```rust
pub trait PlanRefiner: Send + Sync {
    fn refine(
        &self,
        plan: &DecomposedTask,
        failure_patterns: &[FailurePattern],
        trace: Option<&ExecutionTrace>,
    ) -> Result<RefinementResult>;
}

pub enum RefinementResult {
    Approve,
    Modify { updated_steps: Vec<Step>, reasoning: String },
    Reject { reason: String, suggested_alternative: Option<DecomposedTask> },
}
```

**Implementation Notes**:
- Lightweight Sonnet call (~500 tokens)
- Queries `FailurePatternStore` before running
- Logs its reasoning to ExecutionTrace for auditability

### 3.3 Orchestrator (Tier 2)

**Purpose**: Execute decomposed steps via simple-reasoning model.

**Interface**:
```rust
pub trait Orchestrator: Send + Sync {
    async fn execute_step(
        &self,
        step: &Step,
        context: &OrchestrationContext,
    ) -> Result<StepResult>;
}

pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub tool_output: Option<String>,
    pub exit_code: Option<i32>,
    pub error_signal: Option<ErrorSignal>,
    pub duration_ms: u64,
    pub cost_usd: f64,
}
```

**Prompt Style** (Simple Reasoning):
```
You are a task executor. Execute this step exactly:
Step 3: Compile the Rust project

Expected output type: Command execution
Output ONLY: [bash command to run]
No explanation needed.
```

### 3.4 Verification Gate

**Purpose**: Validate step outputs are logically valid, not just technically successful.

**Interface**:
```rust
pub trait VerificationGate: Send + Sync {
    fn verify(&self, step: &Step, result: &StepResult) -> VerificationOutcome;
}

pub enum VerificationOutcome {
    Valid,
    Invalid { reason: String, category: ErrorCategory },
    Uncertain { reason: String },  // Request LLM spot-check
}
```

**Implementation Notes**:
- Uses configurable rules per task category (YAML files in `rules/` directory)
- Cheap heuristics first (regex, JSON validation, syntax checks)
- Escalates to lightweight Sonnet check only on `Uncertain` outcomes
- Examples:
  - `rules/rust_refactoring.yaml`: "Generated code must parse as valid Rust syntax"
  - `rules/data_etl.yaml`: "Output file must contain N rows ± 5%"

### 3.5 Reasoner (Tier 3)

**Purpose**: Replan failed steps using the ExecutionTrace.

**Interface**:
```rust
pub trait Reasoner: Send + Sync {
    async fn replan(
        &self,
        trace: &ExecutionTrace,
        failed_step: &Step,
        error_signal: &ErrorSignal,
        patterns: &[FailurePattern],
    ) -> Result<ReplanResult>;
}

pub struct ReplanResult {
    pub updated_step: Step,
    pub reasoning: String,
    pub confidence: f64,              // 0.0 to 1.0
    pub reasoning_quality_score: u8,  // 1-5 self-rated
}
```

**Prompt Style** (Medium Reasoning):
```
You are a task planner. This step failed:
Step 3: Compile failed with: 'error[E0599]: no method named add'

Previous execution trace:
[Step 1] ran `cargo init` → exit 0 ✓
[Step 2] wrote file src/main.rs → exit 0 ✓
[Step 3] ran `cargo build` → exit 101 ✗ (CompileError)

Historical patterns (for rust_refactoring):
- Step 3 CompileError: 60% of time, fix is adding `use` statement
- Similar error resolved by: importing from std::ops::Add

Based on the error, how would you fix this? Show your reasoning,
then provide the next command(s) to try.
Rate your reasoning quality (1-5) at the end.
```

### 3.6 Deep Thinker (Tier 4)

**Purpose**: Last-resort advanced reasoning with extended thinking enabled.

**Reuses**: `crates/rustycode-orchestration::thinking` (migrated from deleted `rustycode-deep-thinker`)

**Interface**:
```rust
pub trait DeepThinker: Send + Sync {
    async fn solve(
        &self,
        trace: &ExecutionTrace,
        context: &DeepThinkingContext,
    ) -> Result<DeepThinkingResult>;
}

pub struct DeepThinkingContext {
    pub original_task: String,
    pub decomposed_plan: DecomposedTask,
    pub all_failures: Vec<FailurePattern>,
    pub tier3_attempts: Vec<ReplanResult>,
}
```

**Prompt Style** (Advanced Reasoning):
```
You are an expert problem-solver. Use your extended thinking to
deeply analyze why this task is failing.

Consider:
- What are multiple root causes?
- Which strategy is most likely to work?
- What edge cases are we missing?

Full context: [ExecutionTrace + previous attempts + patterns]

Then provide your solution.
[extended_thinking: true, max_thinking_tokens: 30000]
```

### 3.7 Escalation Router

**Purpose**: Central decision-maker for tier transitions.

**Interface**:
```rust
pub trait EscalationRouter: Send + Sync {
    fn should_escalate(
        &self,
        context: &TaskContext,
        error_signal: &ErrorSignal,
    ) -> EscalationDecision;
}

pub enum EscalationDecision {
    Retry,
    Escalate { next_tier: u8, reason: String },
    Abandon { reason: String },
    WarnBudget { remaining_usd: f64 },
}
```

### 3.8 FailurePatternStore

**Purpose**: Persistent store of failure patterns; enables learning over time.

**Schema** (SQLite):
```sql
CREATE TABLE failure_patterns (
    id INTEGER PRIMARY KEY,
    task_type TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    error_signal TEXT NOT NULL,
    occurrence_count INTEGER DEFAULT 1,
    first_seen TIMESTAMP,
    last_seen TIMESTAMP,
    suggested_fix TEXT,
    alternative_approach TEXT,
    tier_failed TEXT,
    escalation_success_rate REAL,
    UNIQUE(task_type, step_index, error_signal)
);

CREATE TABLE escalation_logs (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    error_signal TEXT,
    cost_used REAL,
    timestamp TIMESTAMP,
    success BOOLEAN
);

CREATE TABLE custom_categories (
    category_name TEXT PRIMARY KEY,
    occurrence_count INTEGER DEFAULT 1,
    first_seen TIMESTAMP,
    last_seen TIMESTAMP,
    example_messages TEXT  -- JSON array of examples
);

CREATE INDEX idx_patterns_task ON failure_patterns(task_type);
CREATE INDEX idx_patterns_error ON failure_patterns(error_signal);
CREATE INDEX idx_custom_count ON custom_categories(occurrence_count DESC);
```

**Interface**:
```rust
pub trait FailurePatternStore: Send + Sync {
    fn record_failure(&self, pattern: FailurePattern) -> Result<()>;
    fn record_escalation(&self, log: EscalationLog) -> Result<()>;
    fn record_custom_category(&self, name: &str, example: &str) -> Result<()>;

    fn query_patterns(&self, task_type: &str) -> Result<Vec<FailurePattern>>;
    fn get_escalation_success_rate(&self, error: &ErrorCategory) -> Option<f64>;
    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>>;
}
```

---

## 4. State Machine

### 4.1 TaskContext & Phase

```rust
pub struct TaskContext {
    pub task_id: String,
    pub phase: TaskPhase,
    pub current_tier: u8,           // 2, 3, or 4
    pub attempt_count: u8,
    pub cost_used: f64,
    pub execution_trace: Vec<Step>,
}

pub enum TaskPhase {
    Decomposed,   // Plan ready, not yet executing
    Executing,    // Running at current_tier
    Refining,     // Plan Refiner evaluating failure
    Success,
    Abandoned,
}
```

### 4.2 Valid Transitions

```
Decomposed → Executing (initial Tier 2 start)
Executing → Success | Refining | Abandoned
Refining → Executing (tier may increment)
          | Abandoned (Plan Refiner Reject without alternative)
```

**Plan Refiner Rejection Handling**:
- `RefinementResult::Reject { suggested_alternative: Some(...) }` → Transition `Refining → Executing` with new plan
- `RefinementResult::Reject { suggested_alternative: None }` → Transition `Refining → Abandoned` (unrecoverable)

### 4.3 State Machine Invariants

- `current_tier` can only increase (2 → 3 → 4), never decrease
- `attempt_count` resets when `current_tier` changes
- `cost_used` is monotonically increasing
- `Abandoned` and `Success` are terminal states

---

## 5. Error Handling

### 5.1 Error Signal & Categories

```rust
pub struct ErrorSignal {
    pub category: ErrorCategory,
    pub exit_code: Option<i32>,
    pub message: String,         // Raw error message (truncated to 2KB)
    pub step_id: String,         // Which step produced this error
    pub tool_name: String,       // Which tool produced this error
    pub captured_at: DateTime<Utc>,
}

pub enum ErrorCategory {
    // Well-known errors (first-class)
    SyntaxError,
    CompileError,
    TypeError,
    LogicError,              // VerificationGate failure
    PermissionDenied,
    DiskFull,
    ToolTimeout,
    ContextLengthExceeded,

    // Extensible
    Custom(String),          // Logged to FailurePatternStore for promotion
}
```

### 5.2 Escalation Triggers Matrix

| Error Category | Tier 2 → 3 | Tier 3 → 4 | Notes |
|----------------|------------|------------|-------|
| SyntaxError | Immediate | Immediate | Hard failure |
| CompileError | Immediate | Immediate | Hard failure |
| TypeError | Immediate | Immediate | Hard failure |
| LogicError (VerGate) | Immediate | Immediate | Quality failure |
| PermissionDenied | After retry | Immediate | May self-recover |
| DiskFull | After cleanup | Immediate | Infrastructure |
| ToolTimeout | After 2 attempts | Immediate | May be transient |
| ContextLengthExceeded | Immediate | Immediate | Model switch needed |
| Custom(*) | After 2 attempts | After 1 attempt | Unknown severity |

### 5.3 Hard Stops (Any Tier)

1. **Hallucination Loop**: Same tool call with identical args 3+ consecutive times → `Abandon`
2. **Budget Exceeded**: `cost_used >= max_budget` → `Abandon`
3. **Budget Warning**: `cost_used >= 0.8 * max_budget` → emit warning, continue

### 5.4 Escalation Decision Algorithm

```rust
fn should_escalate(ctx: &TaskContext, error: &ErrorSignal) -> EscalationDecision {
    // Hard stops (any state)
    if hallucination_detected(&ctx.execution_trace) {
        return Abandon { reason: "hallucination_loop".into() };
    }
    if ctx.cost_used >= ctx.max_budget {
        return Abandon { reason: "budget_exceeded".into() };
    }
    if ctx.cost_used >= 0.8 * ctx.max_budget {
        // Warn but continue
        return WarnBudget { remaining_usd: ctx.max_budget - ctx.cost_used };
    }

    // Per-tier logic
    match ctx.current_tier {
        2 => {
            if is_critical_error(&error.category) {
                Escalate { next_tier: 3, reason: format!("critical:{:?}", error.category) }
            } else if ctx.attempt_count >= 2 {
                Escalate { next_tier: 3, reason: "max_attempts_tier2".into() }
            } else {
                Retry
            }
        }
        3 => {
            if is_critical_error(&error.category) || ctx.attempt_count >= 2 {
                Escalate { next_tier: 4, reason: "tier3_exhausted".into() }
            } else {
                Retry
            }
        }
        4 => Abandon { reason: "tier4_exhausted".into() },
        _ => Retry,
    }
}
```

---

## 6. Data Flow

### 6.1 Execution Trace

The `ExecutionTrace` is the single source of truth passed between tiers:

```rust
pub struct ExecutionTrace {
    pub task_id: String,
    pub steps: Vec<TraceEntry>,
}

pub struct TraceEntry {
    pub step_id: String,
    pub step_index: u8,
    pub tier: u8,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub output: String,
    pub exit_code: Option<i32>,
    pub error_signal: Option<ErrorSignal>,
    pub timestamp: DateTime<Utc>,
    pub cost_usd: f64,
}
```

**Key property**: Immutable append-only log. Higher tiers see the trace, never raw LLM conversations.

### 6.2 Tier Context Injection

What each tier receives:

**Tier 2 (Orchestrator)**:
- Original task description
- Decomposed plan (from Tier 1)
- Plan Refiner's notes ("avoid approach X")
- Current step (ID, description, expected output type)
- Prior step outputs from the ExecutionTrace (e.g., file paths created, data extracted, values computed) so Step N can reference Step N-1's artifacts

**Tier 3 (Reasoner)**:
- Original task description
- Original decomposed plan
- Plan Refiner's notes
- Full ExecutionTrace
- Failed step ID + error signal
- FailurePatternStore matches (historical data)
- NOT INCLUDED: Tier 2's raw LLM conversation history

**Tier 4 (Deep Thinker)**:
- Everything Tier 3 received
- Tier 3's replanning attempts and why they failed
- Full FailurePatternStore summary
- Extended thinking enabled

---

## 7. Configuration

### 7.1 Unified YAML Config

Location: `~/.rustycode/orchestration.yaml` (overridable via `RUSTYCODE_ORCHESTRATION_CONFIG` env var)

```yaml
models:
  # Ordered by preference within each tier
  tier_2:  # Simple-reasoning models
    - name: claude-haiku-4-5
      provider: anthropic
      cost_per_1m_tokens_input: 0.80
      cost_per_1m_tokens_output: 4.00
      context_window: 200000
    - name: gpt-3.5-turbo
      provider: openai
      cost_per_1m_tokens_input: 0.50
      cost_per_1m_tokens_output: 1.50
      context_window: 16000

  tier_3:  # Medium-reasoning models
    - name: claude-sonnet-4-6
      provider: anthropic
      cost_per_1m_tokens_input: 3.00
      cost_per_1m_tokens_output: 15.00
      context_window: 200000
    - name: gpt-4
      provider: openai
      cost_per_1m_tokens_input: 10.00
      cost_per_1m_tokens_output: 30.00
      context_window: 128000

  tier_4:  # Advanced-reasoning models (extended thinking preferred)
    - name: claude-opus-4-7
      provider: anthropic
      cost_per_1m_tokens_input: 15.00
      cost_per_1m_tokens_output: 75.00
      supports_extended_thinking: true
      max_thinking_tokens: 31999
      context_window: 200000

escalation:
  tier_2:
    max_attempts: 2
    critical_errors: [SyntaxError, TypeError, CompileError, LogicError]
    recoverable_errors: [PermissionDenied, DiskFull]
  tier_3:
    max_attempts: 2
    critical_errors: [SyntaxError, CompileError, ContextLengthExceeded]
  tier_4:
    max_attempts: 1

budget:
  # Per-tier caps are SOFT caps (advisory, for tier-local escalation decisions).
  # total_max_usd is the HARD cap (task abandons when exceeded).
  # Soft caps can overlap up to total_max_usd.
  total_max_usd: 0.50
  tier_2_max_usd: 0.05   # Soft: escalates to Tier 3 if exceeded on this task
  tier_3_max_usd: 0.30   # Soft: escalates to Tier 4 if exceeded on this task
  tier_4_max_usd: 0.20   # Soft: triggers budget warning
  warn_threshold_pct: 80  # % of total_max_usd
  burst_enabled_for: [CriticalInfrastructure, Security]
  burst_multiplier: 2.0   # Multiplies total_max_usd for matching task categories

hallucination:
  detection_window: 3
  action: abandon

failure_store:
  backend: sqlite
  path: ~/.rustycode/failure_patterns.db
  retention_days: 90
  promotion_threshold: 10  # Custom categories seen 10+ times are promotion candidates

verification_gate:
  rules_dir: ~/.rustycode/verification_rules
  default_action_on_uncertain: llm_spot_check  # or "pass" or "fail"
```

### 7.2 Rules Directory Layout

```
~/.rustycode/verification_rules/
  rust_refactoring.yaml
  data_etl.yaml
  web_scraping.yaml
  scientific_computing.yaml
  default.yaml
```

Example rule file (`rust_refactoring.yaml`):
```yaml
task_type: rust_refactoring
rules:
  - description: Generated code must parse as valid Rust
    check: syntax
    syntax_validator: rustc
    on_failure: LogicError
  - description: Must not contain TODO comments
    check: regex
    pattern: '(?i)TODO|FIXME'
    on_match_as: Invalid
  - description: Imports must be used
    check: command
    command: cargo clippy -- -D warnings
    on_failure: LogicError
```

---

## 8. Observability

### 8.1 Metrics Collected Per Task

```rust
pub struct OrchestrationMetrics {
    pub task_id: String,
    pub task_category: String,
    pub total_duration_ms: u64,
    pub cost_breakdown: HashMap<u8, f64>,       // tier → cost
    pub attempts_per_tier: HashMap<u8, u8>,
    pub final_outcome: TaskOutcome,
    pub escalation_reasons: Vec<String>,
    pub reasoning_quality_score: Option<u8>,    // 1-5
    pub hallucination_detected: bool,
    pub budget_warnings_emitted: u8,
    pub steps_succeeded: u8,
    pub steps_failed: u8,
}

pub enum TaskOutcome {
    SuccessAtTier(u8),
    Abandoned { reason: String },
    BudgetExceeded,
    HallucinationLoop,
}
```

### 8.2 Tracing Integration

Uses `tracing` crate with structured spans:

```rust
tracing::info_span!(
    "task_orchestration",
    task_id = %ctx.task_id,
    task_category = %category,
    initial_tier = 2,
);

tracing::info!(
    tier = ctx.current_tier,
    cost_usd = ctx.cost_used,
    duration_ms = duration.as_millis(),
    outcome = ?outcome,
    "Task completed"
);
```

### 8.3 Aggregate Reports

Nightly job generates:
- Solve rate by task category
- Cost distribution (P50, P95, P99)
- Escalation reasons frequency
- Tier-wise success rates
- Promotion candidates (unknown error categories appearing frequently)

---

## 9. Testing Strategy

### 9.1 Layer 1: Unit Tests

Location: Inline `#[cfg(test)]` modules in each crate.

Coverage:
- State machine transitions (valid/invalid)
- Error signal classification
- Budget calculation logic
- FailurePatternStore CRUD operations
- Escalation decision algorithm (exhaustive per error category)

### 9.2 Layer 2: Integration Tests

Location: `crates/rustycode-orchestration/tests/`.

Coverage:
- Full orchestration with mocked models
- Escalation scenarios (each tier → next)
- Budget exhaustion scenarios
- Hallucination loop detection
- Verification Gate with real rule files

### 9.3 Layer 3: End-to-End Tests

Location: `tests/terminal_bench_e2e.rs`.

Coverage:
- Real models against 10-20 representative terminal-bench tasks
- Compare: old system vs new system (A/B)
- Measure `OrchestrationMetrics` for each run

Example:
```rust
#[tokio::test]
#[ignore]  // Requires real API keys
async fn test_rust_refactoring_task() {
    let task = load_terminal_bench_task("tb-001-rust-refactor");
    let result = orchestrate(&task).await.unwrap();

    assert_eq!(result.final_outcome, TaskOutcome::SuccessAtTier(2));
    assert!(result.cost_breakdown[&2] < 0.05);
}
```

### 9.4 Layer 4: Continuous Evaluation

GitHub Actions workflow:
- Nightly run against full terminal-bench (89 tasks)
- Track solve rate, cost, latency trends
- Alert on regressions (> 5% drop in solve rate)
- Generate weekly report

---

## 10. Integration with Existing RustyCode

### 10.1 Crate Structure

**Note on naming**: The former `rustycode-orchestra` crate has been deleted. All autonomous-development orchestration and tiered model execution now live in `rustycode-orchestration`, which handles both multi-agent coordination and single-task tiered model execution.

New crate: `crates/rustycode-orchestration/`

```
crates/rustycode-orchestration/
  src/
    lib.rs                    # Public API
    decomposer.rs             # TaskDecomposer
    plan_refiner.rs           # PlanRefiner
    orchestrator.rs           # Orchestrator (Tier 2)
    verification_gate.rs      # VerificationGate
    reasoner.rs               # Reasoner (Tier 3)
    deep_thinker_adapter.rs   # Tier 4 (wraps orchestration::thinking)
    escalation_router.rs      # EscalationRouter
    failure_store.rs          # FailurePatternStore (SQLite)
    state_machine.rs          # TaskContext & TaskPhase
    execution_trace.rs        # ExecutionTrace
    metrics.rs                # OrchestrationMetrics
    config.rs                 # YAML config loader
    error_classifier.rs       # ErrorClassifier
    model_registry.rs         # Model capability registry
  tests/
    integration_tests.rs
  Cargo.toml
  README.md
```

### 10.2 Integration Points

- **rustycode-llm**: Use for all model calls (each tier picks a different model)
- **rustycode-tools**: Use for tool execution (bash, file, grep, etc.)
- **rustycode-orchestration::thinking**: Used as Tier 4 implementation (was `rustycode-deep-thinker`)
- **rustycode-storage**: Used for SQLite backend of FailurePatternStore
- **rustycode-observability**: Emit tracing spans and metrics
- **rustycode-config**: Load unified YAML config

### 10.3 Backwards Compatibility

- Existing single-model execution path is preserved
- Orchestration is opt-in via `--orchestrate` CLI flag or config setting
- Current deep-thinker service in `rustycode-tui/src/services/deep_thinking.rs` becomes a Tier 4 entry point

---

## 11. Rollout Plan

### Phase A: Foundation (Weeks 1-2)
- New crate `rustycode-orchestration`
- Core types: `TaskContext`, `ExecutionTrace`, `ErrorCategory`, `FailurePattern`
- State machine with tests
- FailurePatternStore with SQLite backend
- Unit tests for all core logic

### Phase B: Pipeline (Weeks 3-4)
- Task Decomposer with medium-reasoning model
- Plan Refiner with FailurePatternStore integration
- Orchestrator (Tier 2) with tool execution
- Verification Gate with rule files
- Reasoner (Tier 3)
- Integration tests with mocked models

### Phase C: Deep Thinking Integration (Week 5)
- Deep Thinker adapter using `rustycode-orchestration::thinking` module
- Escalation Router connecting all tiers
- End-to-end test on small terminal-bench subset (5-10 tasks)

### Phase D: Production Readiness (Weeks 6-7)
- Unified YAML configuration
- OrchestrationMetrics + tracing integration
- Model Registry with capability detection
- Full terminal-bench evaluation run
- Documentation + examples

### Phase E: Continuous Improvement (Weeks 8+)
- Nightly CI evaluation
- FailurePatternStore analysis + promotion workflow
- Tuning based on real data
- Extend verification rules for new task categories

---

## 12. Success Criteria

The design is successful if:

1. **Solve rate**: 95%+ of terminal-bench tasks completed successfully
2. **Cost efficiency**: 80-85% of tasks resolved at Tier 2 (cheapest model)
3. **Reliability**: Zero thinking-timeout abandonments (all tier 4 runs complete)
4. **Portability**: System works with Claude-only, OpenAI-only, or mixed models
5. **Observability**: All escalations logged with clear reasons
6. **Learning**: FailurePatternStore contains patterns after 1 week of use

---

## 13. Open Questions

1. Should the Verification Gate run *per step* or *per tier-complete*? (Currently per-step for faster failure detection)
2. How should we seed the initial FailurePatternStore? (Probably empty; grows from real usage)
3. Should Plan Refiner be optional for small tasks? (Initially always on; make configurable later)
4. What's the fallback if SQLite is unavailable? (In-memory mode; no persistence)

---

## 14. Appendix: Error Signal Classifier

Example classification rules (embedded in `error_classifier.rs`):

```rust
fn default_patterns() -> Vec<(Regex, ErrorCategory)> {
    vec![
        (regex!(r"(?i)syntax error|unexpected token"), ErrorCategory::SyntaxError),
        (regex!(r"(?i)error\[E\d+\]|compilation failed"), ErrorCategory::CompileError),
        (regex!(r"(?i)TypeError|type mismatch|undefined"), ErrorCategory::TypeError),
        (regex!(r"(?i)permission denied|EACCES"), ErrorCategory::PermissionDenied),
        (regex!(r"(?i)no space left|disk full|ENOSPC"), ErrorCategory::DiskFull),
        (regex!(r"(?i)context length exceeded|too many tokens"), ErrorCategory::ContextLengthExceeded),
    ]
}

fn classify_by_exit_code(exit_code: i32) -> ErrorCategory {
    match exit_code {
        13 => ErrorCategory::PermissionDenied,
        28 => ErrorCategory::DiskFull,
        124 => ErrorCategory::ToolTimeout,
        _ => ErrorCategory::Custom(format!("ExitCode{}", exit_code)),
    }
}
```

---

**End of Specification**
