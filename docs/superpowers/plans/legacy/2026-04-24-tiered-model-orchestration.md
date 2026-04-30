# Tiered Model Orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a model-agnostic tiered orchestration system that solves terminal-bench tasks reliably regardless of model quality, by decomposing tasks into small steps, escalating to progressively more capable models only when needed, and learning from past failures via a persistent pattern store.

**Architecture:** New `rustycode-orchestration` crate with 4-tier pipeline (Decomposer → Plan Refiner → Orchestrator (Tier 2) → Verification Gate → Reasoner (Tier 3) → Deep Thinker (Tier 4)). Immutable `ExecutionTrace` flows between tiers. SQLite-backed `FailurePatternStore` captures patterns for the Plan Refiner. Unified YAML config drives model selection and budgets.

**Tech Stack:** Rust 2021, tokio async, rusqlite (SQLite), serde_yaml, thiserror, anyhow, tracing, existing `rustycode-llm`, `rustycode-tools`, `rustycode-deep-thinker` crates.

**Reference Spec:** `docs/superpowers/specs/2026-04-24-tiered-model-orchestration-design.md`

---

## Design Philosophy: The Orchestration Symphony

A **Composer/Conductor/Musician** model decouples reasoning complexity from task execution:

- **The Musicians (Tier 2 Orchestrator)**: Execute raw instructions (Bash/File/Tool calls). They play what is on the page.
- **The Editor (Plan Refiner / Reasoner Tier 3)**: Reviews the performance against the score (FailurePatternStore + ExecutionTrace). Performs small edits and patches.
- **The Composer (Deep-Thinker Tier 4)**: Re-composes the core logic when patches fail. Handles total rewrites of the score.
- **The Conductor (Orchestration Crate)**: Manages the symphony lifecycle. Keeps tempo, signals escalations, validates performance, and coordinates the ensemble.

---

## File Structure

### New Crate Layout

```
crates/rustycode-orchestration/
├── Cargo.toml
├── README.md
├── src/
│   ├── lib.rs                    # Public API & re-exports
│   ├── error.rs                  # OrchestrationError (thiserror)
│   ├── types.rs                  # Shared types: Step, Difficulty, OutputType, TaskOutcome
│   ├── error_signal.rs           # ErrorSignal, ErrorCategory, ErrorClassifier
│   ├── execution_trace.rs        # ExecutionTrace, TraceEntry (immutable append-only)
│   ├── state_machine.rs          # TaskContext, TaskPhase, transition validation
│   ├── failure_store/
│   │   ├── mod.rs                # FailurePatternStore trait
│   │   ├── sqlite.rs             # SQLite implementation
│   │   └── memory.rs             # In-memory implementation (fallback)
│   ├── config.rs                 # YAML config loader (OrchestrationConfig)
│   ├── model_registry.rs         # ModelCapabilities, tier-based model selection
│   ├── decomposer.rs             # TaskDecomposer trait + LlmDecomposer impl
│   ├── plan_refiner.rs           # PlanRefiner trait + LlmPlanRefiner impl
│   ├── orchestrator.rs           # Orchestrator (Tier 2) - simple reasoning execution
│   ├── verification_gate.rs      # VerificationGate + rule loading
│   ├── reasoner.rs               # Reasoner (Tier 3) - medium reasoning replanning
│   ├── deep_thinker_adapter.rs   # Tier 4 - wraps rustycode-deep-thinker
│   ├── escalation_router.rs      # EscalationRouter trait + DefaultEscalationRouter
│   ├── metrics.rs                # OrchestrationMetrics, reasoning quality scoring
│   └── pipeline.rs               # Main pipeline: OrchestrationPipeline orchestrates all tiers
├── tests/
│   ├── state_machine_test.rs     # State transition tests
│   ├── error_classifier_test.rs  # Error pattern classification tests
│   ├── failure_store_test.rs     # SQLite store integration tests
│   ├── escalation_router_test.rs # Escalation decision tests
│   ├── verification_gate_test.rs # Rule file loading & validation tests
│   └── pipeline_integration_test.rs # Full pipeline with mocked models
└── rules/                        # Default verification rule files
    ├── default.yaml
    ├── rust_refactoring.yaml
    └── data_etl.yaml
```

### Modified Files

- `Cargo.toml` (workspace root) — Add `rustycode-orchestration` to members
- `crates/rustycode-tui/src/services/deep_thinking.rs` — Wire orchestration pipeline as Tier 4 invocation point (later task)

---

## Phase A: Foundation (Tasks 1-8)

### Task 1: Create Crate Skeleton

**Files:**
- Create: `crates/rustycode-orchestration/Cargo.toml`
- Create: `crates/rustycode-orchestration/src/lib.rs`
- Create: `crates/rustycode-orchestration/README.md`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1: Create Cargo.toml**

Create `crates/rustycode-orchestration/Cargo.toml`:

```toml
[package]
name = "rustycode-orchestration"
version.workspace = true
edition.workspace = true
license = "MIT"
description = "Tiered model orchestration for terminal-bench and complex task solving"

[lib]
doctest = false

[dependencies]
anyhow.workspace = true
async-trait.workspace = true
chrono = { workspace = true, features = ["serde"] }
serde.workspace = true
serde_json.workspace = true
serde_yaml.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid = { workspace = true, features = ["v4", "serde"] }
regex.workspace = true
rusqlite = { version = "0.31", features = ["bundled", "chrono"] }
rustycode-llm = { path = "../rustycode-llm" }
rustycode-tools = { path = "../rustycode-tools", default-features = false }
rustycode-deep-thinker = { path = "../rustycode-deep-thinker" }
rustycode-protocol = { path = "../rustycode-protocol" }
rustycode-config = { path = "../rustycode-config" }

[dev-dependencies]
tempfile.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }

[lints]
workspace = true
```

- [ ] **Step 2: Create initial lib.rs**

Create `crates/rustycode-orchestration/src/lib.rs`:

```rust
//! Tiered model orchestration for terminal-bench and complex task solving.
//!
//! This crate provides a model-agnostic, tiered orchestration pipeline that
//! decomposes complex tasks into small steps, executes them with progressively
//! more capable models only when needed, and learns from past failures via a
//! persistent pattern store.
//!
//! See `docs/superpowers/specs/2026-04-24-tiered-model-orchestration-design.md`
//! for the full design specification.

pub mod error;
pub mod types;

pub use error::{OrchestrationError, Result};
pub use types::{Step, Difficulty, OutputType, TaskOutcome};
```

- [ ] **Step 3: Create minimal README**

Create `crates/rustycode-orchestration/README.md`:

```markdown
# rustycode-orchestration

Tiered model orchestration for solving complex multi-step tasks
(terminal-bench and similar) reliably regardless of model quality.

## Design

Four-tier escalation pipeline:
1. **Decomposer** — breaks task into micro-steps
2. **Plan Refiner** — consults failure patterns, adjusts plan
3. **Tier 2 Orchestrator** — weak model executes steps (80-85% of tasks)
4. **Verification Gate** — validates step outputs
5. **Tier 3 Reasoner** — medium model replans on failure
6. **Tier 4 Deep Thinker** — advanced model + extended thinking, last resort

See `docs/superpowers/specs/2026-04-24-tiered-model-orchestration-design.md`.

## Dependencies

- Data: `rusqlite` (SQLite for `FailurePatternStore`)
- LLM: `rustycode-llm` (all tier model calls)
- Tools: `rustycode-tools` (bash/file/grep execution)
- Deep thinking: `rustycode-deep-thinker` (Tier 4 implementation)

## Integration

Invoked by `rustycode-tui` / `rustycode-cli` / `rustycode-orchestra`
for single-task execution.
```

- [ ] **Step 4: Add crate to workspace**

Modify `Cargo.toml` in the workspace root. Find the `[workspace]` `members` section and add:

```toml
    "crates/rustycode-orchestration",
```

(Insert alphabetically between `rustycode-orchestra` and related entries.)

- [ ] **Step 5: Create placeholder error.rs and types.rs**

Create `crates/rustycode-orchestration/src/error.rs`:

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, OrchestrationError>;

#[derive(Debug, Error)]
pub enum OrchestrationError {
    #[error("invalid state transition: {from} -> {to}")]
    InvalidTransition { from: String, to: String },

    #[error("budget exceeded: used {used_usd}, max {max_usd}")]
    BudgetExceeded { used_usd: f64, max_usd: f64 },

    #[error("task abandoned: {0}")]
    Abandoned(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("storage error: {0}")]
    Storage(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

Create `crates/rustycode-orchestration/src/types.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputType {
    File,
    Command,
    Query,
    Code,
    Data,
    Verification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub index: u8,
    pub description: String,
    pub expected_output_type: OutputType,
    pub suggested_tool: Option<String>,
    pub retry_on_failure: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskOutcome {
    SuccessAtTier(u8),
    Abandoned { reason: String },
    BudgetExceeded,
    HallucinationLoop,
}
```

- [ ] **Step 6: Verify build**

Run: `cargo build -p rustycode-orchestration`

Expected: Clean build, no warnings.

- [ ] **Step 7: Commit**

```bash
cd ~/dev/rustycode
git add Cargo.toml crates/rustycode-orchestration/
git commit -m "feat(orchestration): scaffold rustycode-orchestration crate"
```

---

### Task 2: ErrorSignal & ErrorCategory Types

**Files:**
- Create: `crates/rustycode-orchestration/src/error_signal.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/error_classifier_test.rs`:

```rust
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorClassifier};

#[test]
fn test_classify_syntax_error() {
    let classifier = ErrorClassifier::default();
    let cat = classifier.classify("bash: syntax error near unexpected token", 2);
    assert_eq!(cat, ErrorCategory::SyntaxError);
}

#[test]
fn test_classify_compile_error() {
    let classifier = ErrorClassifier::default();
    let cat = classifier.classify("error[E0599]: no method named `add`", 101);
    assert_eq!(cat, ErrorCategory::CompileError);
}

#[test]
fn test_classify_by_exit_code_permission_denied() {
    let classifier = ErrorClassifier::default();
    let cat = classifier.classify("some output", 13);
    assert_eq!(cat, ErrorCategory::PermissionDenied);
}

#[test]
fn test_classify_unknown_becomes_custom() {
    let classifier = ErrorClassifier::default();
    let cat = classifier.classify("weird error we haven't seen", 99);
    assert!(matches!(cat, ErrorCategory::Custom(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test error_classifier_test`

Expected: FAIL (error_signal module doesn't exist yet).

- [ ] **Step 3: Implement error_signal.rs**

Create `crates/rustycode-orchestration/src/error_signal.rs`:

```rust
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorCategory {
    SyntaxError,
    CompileError,
    TypeError,
    LogicError,
    PermissionDenied,
    DiskFull,
    ToolTimeout,
    ContextLengthExceeded,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorSignal {
    pub category: ErrorCategory,
    pub exit_code: Option<i32>,
    pub message: String,
    pub step_id: String,
    pub tool_name: String,
    pub captured_at: DateTime<Utc>,
}

impl ErrorSignal {
    pub fn new(
        category: ErrorCategory,
        exit_code: Option<i32>,
        message: String,
        step_id: String,
        tool_name: String,
    ) -> Self {
        let truncated = if message.len() > 2048 {
            format!("{}... [truncated]", &message[..2048])
        } else {
            message
        };
        Self {
            category,
            exit_code,
            message: truncated,
            step_id,
            tool_name,
            captured_at: Utc::now(),
        }
    }
}

pub struct ErrorClassifier {
    patterns: Vec<(Regex, ErrorCategory)>,
}

impl Default for ErrorClassifier {
    fn default() -> Self {
        let patterns = vec![
            (Regex::new(r"(?i)syntax error|unexpected token|parse error").unwrap(), ErrorCategory::SyntaxError),
            (Regex::new(r"(?i)error\[E\d+\]|compilation failed|compile error").unwrap(), ErrorCategory::CompileError),
            (Regex::new(r"(?i)TypeError|type mismatch|undefined (symbol|reference)").unwrap(), ErrorCategory::TypeError),
            (Regex::new(r"(?i)permission denied|EACCES").unwrap(), ErrorCategory::PermissionDenied),
            (Regex::new(r"(?i)no space left|disk full|ENOSPC").unwrap(), ErrorCategory::DiskFull),
            (Regex::new(r"(?i)context length exceeded|too many tokens|max tokens").unwrap(), ErrorCategory::ContextLengthExceeded),
            (Regex::new(r"(?i)(tool |command )?timed? ?out|timeout").unwrap(), ErrorCategory::ToolTimeout),
        ];
        Self { patterns }
    }
}

impl ErrorClassifier {
    pub fn classify(&self, output: &str, exit_code: i32) -> ErrorCategory {
        // Pattern match first (more specific)
        for (pattern, category) in &self.patterns {
            if pattern.is_match(output) {
                return category.clone();
            }
        }
        // Fall back to exit-code classification
        match exit_code {
            13 => ErrorCategory::PermissionDenied,
            28 => ErrorCategory::DiskFull,
            124 => ErrorCategory::ToolTimeout,
            _ => ErrorCategory::Custom(format!("ExitCode{}", exit_code)),
        }
    }

    pub fn with_custom_pattern(mut self, pattern: Regex, category: ErrorCategory) -> Self {
        self.patterns.push((pattern, category));
        self
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Modify `crates/rustycode-orchestration/src/lib.rs` — add:

```rust
pub mod error_signal;
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rustycode-orchestration --test error_classifier_test`

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add ErrorSignal and ErrorClassifier with pattern matching"
```

---

### Task 3: ExecutionTrace (Immutable Append-Only Log)

**Files:**
- Create: `crates/rustycode-orchestration/src/execution_trace.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/execution_trace_test.rs`:

```rust
use rustycode_orchestration::execution_trace::{ExecutionTrace, TraceEntry};
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorSignal};

#[test]
fn test_trace_append_only() {
    let mut trace = ExecutionTrace::new("task-1".to_string());
    let entry = TraceEntry::new_success(
        "step-1".to_string(),
        0,
        2,
        "bash".to_string(),
        serde_json::json!({"command": "ls"}),
        "file1\nfile2".to_string(),
        Some(0),
        0.001,
    );
    trace.append(entry);
    assert_eq!(trace.steps.len(), 1);
    assert_eq!(trace.steps[0].tier, 2);
}

#[test]
fn test_trace_total_cost() {
    let mut trace = ExecutionTrace::new("task-2".to_string());
    trace.append(TraceEntry::new_success(
        "step-1".into(), 0, 2, "bash".into(),
        serde_json::json!({}), "ok".into(), Some(0), 0.01,
    ));
    trace.append(TraceEntry::new_success(
        "step-2".into(), 1, 3, "bash".into(),
        serde_json::json!({}), "ok".into(), Some(0), 0.05,
    ));
    assert!((trace.total_cost() - 0.06).abs() < 1e-9);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test execution_trace_test`

Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement execution_trace.rs**

Create `crates/rustycode-orchestration/src/execution_trace.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error_signal::ErrorSignal;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl TraceEntry {
    pub fn new_success(
        step_id: String,
        step_index: u8,
        tier: u8,
        tool_name: String,
        tool_args: serde_json::Value,
        output: String,
        exit_code: Option<i32>,
        cost_usd: f64,
    ) -> Self {
        Self {
            step_id,
            step_index,
            tier,
            tool_name,
            tool_args,
            output,
            exit_code,
            error_signal: None,
            timestamp: Utc::now(),
            cost_usd,
        }
    }

    pub fn new_failure(
        step_id: String,
        step_index: u8,
        tier: u8,
        tool_name: String,
        tool_args: serde_json::Value,
        output: String,
        exit_code: Option<i32>,
        error_signal: ErrorSignal,
        cost_usd: f64,
    ) -> Self {
        Self {
            step_id,
            step_index,
            tier,
            tool_name,
            tool_args,
            output,
            exit_code,
            error_signal: Some(error_signal),
            timestamp: Utc::now(),
            cost_usd,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub task_id: String,
    pub steps: Vec<TraceEntry>,
}

impl ExecutionTrace {
    pub fn new(task_id: String) -> Self {
        Self { task_id, steps: Vec::new() }
    }

    /// Append entry — only way to add to the trace (immutable append-only).
    pub fn append(&mut self, entry: TraceEntry) {
        self.steps.push(entry);
    }

    pub fn total_cost(&self) -> f64 {
        self.steps.iter().map(|s| s.cost_usd).sum()
    }

    pub fn last_n_tool_calls(&self, n: usize) -> Vec<&TraceEntry> {
        self.steps.iter().rev().take(n).collect()
    }

    pub fn failures(&self) -> Vec<&TraceEntry> {
        self.steps.iter().filter(|e| e.error_signal.is_some()).collect()
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod execution_trace;` to `lib.rs`.

- [ ] **Step 5: Run test**

Run: `cargo test -p rustycode-orchestration --test execution_trace_test`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add ExecutionTrace immutable append-only log"
```

---

### Task 4: TaskContext & State Machine

**Files:**
- Create: `crates/rustycode-orchestration/src/state_machine.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/state_machine_test.rs`:

```rust
use rustycode_orchestration::state_machine::{TaskContext, TaskPhase};
use rustycode_orchestration::error::OrchestrationError;

#[test]
fn test_initial_state() {
    let ctx = TaskContext::new("task-1".to_string(), 0.50);
    assert_eq!(ctx.phase, TaskPhase::Decomposed);
    assert_eq!(ctx.current_tier, 2);
    assert_eq!(ctx.attempt_count, 0);
    assert_eq!(ctx.cost_used, 0.0);
}

#[test]
fn test_valid_transition_decomposed_to_executing() {
    let mut ctx = TaskContext::new("t".into(), 0.50);
    assert!(ctx.transition(TaskPhase::Executing, 0.0).is_ok());
    assert_eq!(ctx.phase, TaskPhase::Executing);
}

#[test]
fn test_invalid_transition_success_to_refining() {
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.0).unwrap();
    ctx.transition(TaskPhase::Success, 0.01).unwrap();
    // Success is terminal
    let result = ctx.transition(TaskPhase::Refining, 0.0);
    assert!(matches!(result, Err(OrchestrationError::InvalidTransition { .. })));
}

#[test]
fn test_tier_increments_on_refining_to_executing() {
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.0).unwrap();
    ctx.transition(TaskPhase::Refining, 0.01).unwrap();
    ctx.increment_tier();
    ctx.transition(TaskPhase::Executing, 0.0).unwrap();
    assert_eq!(ctx.current_tier, 3);
    assert_eq!(ctx.attempt_count, 0);
}

#[test]
fn test_tier_never_decreases() {
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.increment_tier();
    ctx.increment_tier();
    assert_eq!(ctx.current_tier, 4);
    ctx.increment_tier();
    // Saturates at 4
    assert_eq!(ctx.current_tier, 4);
}

#[test]
fn test_abandon_from_any_state() {
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.01).unwrap();
    assert!(ctx.transition(TaskPhase::Abandoned, 0.0).is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test state_machine_test`

Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement state_machine.rs**

Create `crates/rustycode-orchestration/src/state_machine.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::error::{OrchestrationError, Result};
use crate::execution_trace::ExecutionTrace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskPhase {
    Decomposed,
    Executing,
    Refining,
    Success,
    Abandoned,
}

impl std::fmt::Display for TaskPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: String,
    pub phase: TaskPhase,
    pub current_tier: u8,
    pub attempt_count: u8,
    pub cost_used: f64,
    pub max_budget: f64,
    pub execution_trace: ExecutionTrace,
}

impl TaskContext {
    pub fn new(task_id: String, max_budget: f64) -> Self {
        Self {
            task_id: task_id.clone(),
            phase: TaskPhase::Decomposed,
            current_tier: 2,
            attempt_count: 0,
            cost_used: 0.0,
            max_budget,
            execution_trace: ExecutionTrace::new(task_id),
        }
    }

    /// Attempt a phase transition. Returns Err on invalid transitions.
    pub fn transition(&mut self, next: TaskPhase, cost_delta: f64) -> Result<()> {
        if !is_valid_transition(self.phase, next) {
            return Err(OrchestrationError::InvalidTransition {
                from: self.phase.to_string(),
                to: next.to_string(),
            });
        }
        self.phase = next;
        self.cost_used += cost_delta;
        Ok(())
    }

    /// Increment the current tier (2 → 3 → 4, saturates at 4).
    /// Resets attempt_count as a new tier starts fresh.
    pub fn increment_tier(&mut self) {
        if self.current_tier < 4 {
            self.current_tier += 1;
        }
        self.attempt_count = 0;
    }

    pub fn increment_attempt(&mut self) {
        self.attempt_count = self.attempt_count.saturating_add(1);
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.phase, TaskPhase::Success | TaskPhase::Abandoned)
    }

    pub fn budget_remaining(&self) -> f64 {
        (self.max_budget - self.cost_used).max(0.0)
    }

    pub fn budget_exceeded(&self) -> bool {
        self.cost_used >= self.max_budget
    }
}

fn is_valid_transition(from: TaskPhase, to: TaskPhase) -> bool {
    use TaskPhase::*;
    match (from, to) {
        // From Decomposed
        (Decomposed, Executing) => true,
        // From Executing
        (Executing, Success | Refining | Abandoned) => true,
        // From Refining
        (Refining, Executing | Abandoned) => true,
        // Abandoned allowed from any non-terminal state
        (Decomposed | Executing | Refining, Abandoned) => true,
        // Self-loops not allowed
        _ => false,
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod state_machine;` to `lib.rs`.

- [ ] **Step 5: Run test**

Run: `cargo test -p rustycode-orchestration --test state_machine_test`

Expected: 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add TaskContext state machine with transition validation"
```

---

### Task 5: FailurePatternStore (SQLite)

**Files:**
- Create: `crates/rustycode-orchestration/src/failure_store/mod.rs`
- Create: `crates/rustycode-orchestration/src/failure_store/sqlite.rs`
- Create: `crates/rustycode-orchestration/src/failure_store/memory.rs`
- Create: `crates/rustycode-orchestration/src/failure_store/seed_loader.rs`
- Create: `crates/rustycode-orchestration/rules/seed_patterns.yaml`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/failure_store_test.rs`:

```rust
use rustycode_orchestration::failure_store::{FailurePattern, FailurePatternStore, SqliteFailureStore};
use rustycode_orchestration::error_signal::ErrorCategory;
use tempfile::tempdir;

#[test]
fn test_record_and_query_failure() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    let pattern = FailurePattern {
        task_type: "rust_refactoring".into(),
        step_index: 3,
        error_category: ErrorCategory::CompileError,
        suggested_fix: Some("add use statement".into()),
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&pattern).unwrap();

    let results = store.query_patterns("rust_refactoring").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].step_index, 3);
}

#[test]
fn test_occurrence_count_increments() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    let pattern = FailurePattern {
        task_type: "t".into(),
        step_index: 1,
        error_category: ErrorCategory::SyntaxError,
        suggested_fix: None,
        alternative_approach: None,
        tier_failed: "Tier2".into(),
    };
    store.record_failure(&pattern).unwrap();
    store.record_failure(&pattern).unwrap();
    store.record_failure(&pattern).unwrap();

    let results = store.query_patterns("t").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].occurrence_count, 3);
}

#[test]
fn test_custom_category_recording() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("patterns.db");
    let store = SqliteFailureStore::open(&path).unwrap();

    store.record_custom_category("NetworkTimeout", "connection timed out").unwrap();
    store.record_custom_category("NetworkTimeout", "another timeout").unwrap();
    store.record_custom_category("RateLimited", "429 too many requests").unwrap();

    let candidates = store.promotion_candidates(2).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category_name, "NetworkTimeout");
    assert_eq!(candidates[0].occurrence_count, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test failure_store_test`

Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement failure_store/mod.rs**

Create `crates/rustycode-orchestration/src/failure_store/mod.rs`:

```rust
mod sqlite;
mod memory;

pub use sqlite::SqliteFailureStore;
pub use memory::MemoryFailureStore;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::error_signal::ErrorCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub task_type: String,
    pub step_index: u8,
    pub error_category: ErrorCategory,
    pub suggested_fix: Option<String>,
    pub alternative_approach: Option<String>,
    pub tier_failed: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPattern {
    pub task_type: String,
    pub step_index: u8,
    pub error_category: ErrorCategory,
    pub occurrence_count: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub suggested_fix: Option<String>,
    pub alternative_approach: Option<String>,
    pub tier_failed: String,
    pub escalation_success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationLog {
    pub task_id: String,
    pub from_state: String,
    pub to_state: String,
    pub error_category: Option<ErrorCategory>,
    pub cost_used: f64,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCategoryStats {
    pub category_name: String,
    pub occurrence_count: u32,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

pub trait FailurePatternStore: Send + Sync {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()>;
    fn record_escalation(&self, log: &EscalationLog) -> Result<()>;
    fn record_custom_category(&self, name: &str, example: &str) -> Result<()>;

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>>;
    fn get_escalation_success_rate(&self, error: &ErrorCategory) -> Result<Option<f64>>;
    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>>;
}
```

- [ ] **Step 4: Implement sqlite.rs**

Create `crates/rustycode-orchestration/src/failure_store/sqlite.rs`:

```rust
use std::path::Path;
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};

use super::{
    CustomCategoryStats, EscalationLog, FailurePattern, FailurePatternStore, StoredPattern,
};
use crate::error::Result;
use crate::error_signal::ErrorCategory;

pub struct SqliteFailureStore {
    conn: Mutex<Connection>,
}

impl SqliteFailureStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        init_schema(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS failure_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_type TEXT NOT NULL,
            step_index INTEGER NOT NULL,
            error_category TEXT NOT NULL,
            occurrence_count INTEGER DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            suggested_fix TEXT,
            alternative_approach TEXT,
            tier_failed TEXT,
            escalation_success_rate REAL DEFAULT 0.5,
            UNIQUE(task_type, step_index, error_category)
        );

        CREATE INDEX IF NOT EXISTS idx_patterns_task ON failure_patterns(task_type);
        CREATE INDEX IF NOT EXISTS idx_patterns_error ON failure_patterns(error_category);

        CREATE TABLE IF NOT EXISTS escalation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            from_state TEXT NOT NULL,
            to_state TEXT NOT NULL,
            error_category TEXT,
            cost_used REAL,
            timestamp TEXT NOT NULL,
            success INTEGER
        );

        CREATE TABLE IF NOT EXISTS custom_categories (
            category_name TEXT PRIMARY KEY,
            occurrence_count INTEGER DEFAULT 1,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            example_messages TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_custom_count ON custom_categories(occurrence_count DESC);
    "#)?;
    Ok(())
}

fn encode_category(cat: &ErrorCategory) -> String {
    serde_json::to_string(cat).unwrap_or_else(|_| "\"Unknown\"".to_string())
}

fn decode_category(s: &str) -> ErrorCategory {
    serde_json::from_str(s).unwrap_or(ErrorCategory::Custom("Unknown".into()))
}

impl FailurePatternStore for SqliteFailureStore {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let now = Utc::now().to_rfc3339();
        let category_json = encode_category(&pattern.error_category);

        conn.execute(
            r#"
            INSERT INTO failure_patterns
                (task_type, step_index, error_category, occurrence_count, first_seen, last_seen, suggested_fix, alternative_approach, tier_failed)
            VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?)
            ON CONFLICT(task_type, step_index, error_category) DO UPDATE SET
                occurrence_count = occurrence_count + 1,
                last_seen = excluded.last_seen,
                suggested_fix = COALESCE(excluded.suggested_fix, failure_patterns.suggested_fix),
                alternative_approach = COALESCE(excluded.alternative_approach, failure_patterns.alternative_approach)
            "#,
            params![
                pattern.task_type,
                pattern.step_index,
                category_json,
                now,
                now,
                pattern.suggested_fix,
                pattern.alternative_approach,
                pattern.tier_failed,
            ],
        )?;
        Ok(())
    }

    fn record_escalation(&self, log: &EscalationLog) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let now = Utc::now().to_rfc3339();
        let category_json = log.error_category.as_ref().map(encode_category);
        conn.execute(
            r#"
            INSERT INTO escalation_logs
                (task_id, from_state, to_state, error_category, cost_used, timestamp, success)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                log.task_id,
                log.from_state,
                log.to_state,
                category_json,
                log.cost_used,
                now,
                log.success as i32,
            ],
        )?;
        Ok(())
    }

    fn record_custom_category(&self, name: &str, _example: &str) -> Result<()> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let now = Utc::now().to_rfc3339();
        conn.execute(
            r#"
            INSERT INTO custom_categories (category_name, occurrence_count, first_seen, last_seen)
            VALUES (?, 1, ?, ?)
            ON CONFLICT(category_name) DO UPDATE SET
                occurrence_count = occurrence_count + 1,
                last_seen = excluded.last_seen
            "#,
            params![name, now, now],
        )?;
        Ok(())
    }

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let mut stmt = conn.prepare(
            r#"
            SELECT task_type, step_index, error_category, occurrence_count,
                   first_seen, last_seen, suggested_fix, alternative_approach,
                   tier_failed, escalation_success_rate
            FROM failure_patterns
            WHERE task_type = ?
            ORDER BY occurrence_count DESC
            "#,
        )?;
        let rows = stmt.query_map(params![task_type], |r| {
            Ok(StoredPattern {
                task_type: r.get(0)?,
                step_index: r.get(1)?,
                error_category: decode_category(&r.get::<_, String>(2)?),
                occurrence_count: r.get(3)?,
                first_seen: parse_ts(&r.get::<_, String>(4)?),
                last_seen: parse_ts(&r.get::<_, String>(5)?),
                suggested_fix: r.get(6)?,
                alternative_approach: r.get(7)?,
                tier_failed: r.get(8)?,
                escalation_success_rate: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn get_escalation_success_rate(&self, error: &ErrorCategory) -> Result<Option<f64>> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let category_json = encode_category(error);
        let rate: Option<f64> = conn
            .query_row(
                "SELECT AVG(escalation_success_rate) FROM failure_patterns WHERE error_category = ?",
                params![category_json],
                |r| r.get(0),
            )
            .ok();
        Ok(rate)
    }

    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>> {
        let conn = self.conn.lock().expect("poisoned mutex");
        let mut stmt = conn.prepare(
            r#"
            SELECT category_name, occurrence_count, first_seen, last_seen
            FROM custom_categories
            WHERE occurrence_count >= ?
            ORDER BY occurrence_count DESC
            "#,
        )?;
        let rows = stmt.query_map(params![min_occurrences], |r| {
            Ok(CustomCategoryStats {
                category_name: r.get(0)?,
                occurrence_count: r.get(1)?,
                first_seen: parse_ts(&r.get::<_, String>(2)?),
                last_seen: parse_ts(&r.get::<_, String>(3)?),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

fn parse_ts(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}
```

- [ ] **Step 5: Implement minimal in-memory fallback (memory.rs)**

Create `crates/rustycode-orchestration/src/failure_store/memory.rs`:

```rust
use std::sync::Mutex;

use super::{
    CustomCategoryStats, EscalationLog, FailurePattern, FailurePatternStore, StoredPattern,
};
use crate::error::Result;
use crate::error_signal::ErrorCategory;

#[derive(Default)]
pub struct MemoryFailureStore {
    patterns: Mutex<Vec<StoredPattern>>,
    escalations: Mutex<Vec<EscalationLog>>,
    custom_categories: Mutex<Vec<CustomCategoryStats>>,
}

impl MemoryFailureStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FailurePatternStore for MemoryFailureStore {
    fn record_failure(&self, pattern: &FailurePattern) -> Result<()> {
        let mut patterns = self.patterns.lock().expect("poisoned");
        let now = chrono::Utc::now();
        if let Some(existing) = patterns.iter_mut().find(|p| {
            p.task_type == pattern.task_type
                && p.step_index == pattern.step_index
                && p.error_category == pattern.error_category
        }) {
            existing.occurrence_count += 1;
            existing.last_seen = now;
        } else {
            patterns.push(StoredPattern {
                task_type: pattern.task_type.clone(),
                step_index: pattern.step_index,
                error_category: pattern.error_category.clone(),
                occurrence_count: 1,
                first_seen: now,
                last_seen: now,
                suggested_fix: pattern.suggested_fix.clone(),
                alternative_approach: pattern.alternative_approach.clone(),
                tier_failed: pattern.tier_failed.clone(),
                escalation_success_rate: 0.5,
            });
        }
        Ok(())
    }

    fn record_escalation(&self, log: &EscalationLog) -> Result<()> {
        self.escalations.lock().expect("poisoned").push(log.clone());
        Ok(())
    }

    fn record_custom_category(&self, name: &str, _example: &str) -> Result<()> {
        let mut cats = self.custom_categories.lock().expect("poisoned");
        let now = chrono::Utc::now();
        if let Some(existing) = cats.iter_mut().find(|c| c.category_name == name) {
            existing.occurrence_count += 1;
            existing.last_seen = now;
        } else {
            cats.push(CustomCategoryStats {
                category_name: name.to_string(),
                occurrence_count: 1,
                first_seen: now,
                last_seen: now,
            });
        }
        Ok(())
    }

    fn query_patterns(&self, task_type: &str) -> Result<Vec<StoredPattern>> {
        let patterns = self.patterns.lock().expect("poisoned");
        Ok(patterns
            .iter()
            .filter(|p| p.task_type == task_type)
            .cloned()
            .collect())
    }

    fn get_escalation_success_rate(&self, _error: &ErrorCategory) -> Result<Option<f64>> {
        Ok(None)
    }

    fn promotion_candidates(&self, min_occurrences: u32) -> Result<Vec<CustomCategoryStats>> {
        let cats = self.custom_categories.lock().expect("poisoned");
        Ok(cats
            .iter()
            .filter(|c| c.occurrence_count >= min_occurrences)
            .cloned()
            .collect())
    }
}
```

- [ ] **Step 6: Export from lib.rs**

Add `pub mod failure_store;` to `lib.rs`.

- [ ] **Step 7: Run test**

Run: `cargo test -p rustycode-orchestration --test failure_store_test`

Expected: 3 tests pass.

- [ ] **Step 8: Create SeedPatternsLoader**

Create `crates/rustycode-orchestration/src/failure_store/seed_loader.rs`:

```rust
use anyhow::Result;
use std::path::Path;

use super::FailurePatternStore;
use crate::error_signal::ErrorCategory;

/// Loads initial failure patterns from a YAML seed file into the store.
pub struct SeedPatternsLoader;

#[derive(Debug, serde::Deserialize)]
struct SeedPattern {
    task_type: String,
    step_index: u8,
    error_signal: String,
    suggested_fix: Option<String>,
    alternative_approach: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SeedFile {
    patterns: Vec<SeedPattern>,
}

impl SeedPatternsLoader {
    /// Load seed patterns from a YAML file into the given failure store.
    pub fn load_from_file(store: &dyn FailurePatternStore, path: &Path) -> Result<usize> {
        let content = std::fs::read_to_string(path)?;
        let file: SeedFile = serde_yaml::from_str(&content)?;
        let mut loaded = 0;
        for pattern in &file.patterns {
            let fp = super::FailurePattern {
                task_type: pattern.task_type.clone(),
                step_index: pattern.step_index,
                error_category: ErrorCategory::Custom(pattern.error_signal.clone()),
                suggested_fix: pattern.suggested_fix.clone(),
                alternative_approach: pattern.alternative_approach.clone(),
                tier_failed: None,
            };
            if store.record_failure(&fp).is_ok() {
                loaded += 1;
            }
        }
        Ok(loaded)
    }
}
```

Create `crates/rustycode-orchestration/rules/seed_patterns.yaml` with common patterns:

```yaml
# Seed failure patterns loaded on first run
patterns:
  - task_type: rust_refactoring
    step_index: 2
    error_signal: "error[E0599]"
    suggested_fix: "Add missing use statement for the method"
  - task_type: rust_refactoring
    step_index: 2
    error_signal: "error[E0433]"
    suggested_fix: "Check module path and add correct use statement"
  - task_type: data_etl
    step_index: 3
    error_signal: "Permission denied"
    suggested_fix: "Check file permissions, may need chmod or sudo"
  - task_type: data_etl
    step_index: 1
    error_signal: "No such file"
    suggested_fix: "Verify file path exists, check for typos"
```

Update `failure_store/mod.rs` to export `seed_loader` module.

- [ ] **Step 9: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add FailurePatternStore with SQLite, memory backends, and seed pattern loader"
```

---

### Task 6: OrchestrationConfig (YAML)

**Files:**
- Create: `crates/rustycode-orchestration/src/config.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/config_test.rs`:

```rust
use rustycode_orchestration::config::OrchestrationConfig;

#[test]
fn test_load_default_config() {
    let yaml = r#"
models:
  tier_2:
    - name: claude-haiku-4-5
      provider: anthropic
      cost_per_1m_tokens_input: 0.80
      cost_per_1m_tokens_output: 4.00
      context_window: 200000
  tier_3:
    - name: claude-sonnet-4-6
      provider: anthropic
      cost_per_1m_tokens_input: 3.00
      cost_per_1m_tokens_output: 15.00
      context_window: 200000
  tier_4:
    - name: claude-opus-4-7
      provider: anthropic
      supports_extended_thinking: true
      max_thinking_tokens: 31999
      context_window: 200000

escalation:
  tier_2:
    max_attempts: 2
    critical_errors: [SyntaxError, TypeError, CompileError, LogicError]
  tier_3:
    max_attempts: 2
  tier_4:
    max_attempts: 1

budget:
  total_max_usd: 0.50
  tier_2_max_usd: 0.05
  tier_3_max_usd: 0.30
  tier_4_max_usd: 0.20
  warn_threshold_pct: 80
  burst_multiplier: 2.0

hallucination:
  detection_window: 3

failure_store:
  backend: sqlite
  retention_days: 90
  promotion_threshold: 10
"#;
    let cfg: OrchestrationConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(cfg.models.tier_2.len(), 1);
    assert_eq!(cfg.budget.total_max_usd, 0.50);
    assert_eq!(cfg.escalation.tier_2.max_attempts, 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test config_test`

Expected: FAIL (module doesn't exist).

- [ ] **Step 3: Implement config.rs**

Create `crates/rustycode-orchestration/src/config.rs`:

```rust
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{OrchestrationError, Result};
use crate::error_signal::ErrorCategory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub models: ModelsConfig,
    pub escalation: EscalationConfig,
    pub budget: BudgetConfig,
    #[serde(default)]
    pub hallucination: HallucinationConfig,
    #[serde(default)]
    pub failure_store: FailureStoreConfig,
    #[serde(default)]
    pub verification_gate: VerificationGateConfig,
}

impl OrchestrationConfig {
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let cfg: Self = serde_yaml::from_str(&contents)?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.models.tier_2.is_empty() && self.models.tier_3.is_empty() && self.models.tier_4.is_empty() {
            return Err(OrchestrationError::Config("no models configured".into()));
        }
        if self.budget.total_max_usd <= 0.0 {
            return Err(OrchestrationError::Config(
                "budget.total_max_usd must be > 0".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsConfig {
    #[serde(default)]
    pub tier_2: Vec<ModelSpec>,
    #[serde(default)]
    pub tier_3: Vec<ModelSpec>,
    #[serde(default)]
    pub tier_4: Vec<ModelSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub cost_per_1m_tokens_input: f64,
    #[serde(default)]
    pub cost_per_1m_tokens_output: f64,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    #[serde(default)]
    pub supports_extended_thinking: bool,
    #[serde(default)]
    pub max_thinking_tokens: Option<u32>,
}

fn default_context_window() -> usize {
    128_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    pub tier_2: TierEscalation,
    pub tier_3: TierEscalation,
    pub tier_4: TierEscalation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierEscalation {
    pub max_attempts: u8,
    #[serde(default)]
    pub critical_errors: Vec<ErrorCategory>,
    #[serde(default)]
    pub recoverable_errors: Vec<ErrorCategory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    pub total_max_usd: f64,
    pub tier_2_max_usd: f64,
    pub tier_3_max_usd: f64,
    pub tier_4_max_usd: f64,
    #[serde(default = "default_warn_threshold")]
    pub warn_threshold_pct: u8,
    #[serde(default)]
    pub burst_enabled_for: Vec<String>,
    #[serde(default = "default_burst_multiplier")]
    pub burst_multiplier: f64,
}

fn default_warn_threshold() -> u8 { 80 }
fn default_burst_multiplier() -> f64 { 2.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HallucinationConfig {
    #[serde(default = "default_detection_window")]
    pub detection_window: u8,
    #[serde(default = "default_hallucination_action")]
    pub action: String,
}

fn default_detection_window() -> u8 { 3 }
fn default_hallucination_action() -> String { "abandon".to_string() }

impl Default for HallucinationConfig {
    fn default() -> Self {
        Self {
            detection_window: default_detection_window(),
            action: default_hallucination_action(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureStoreConfig {
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    #[serde(default = "default_promotion_threshold")]
    pub promotion_threshold: u32,
}

fn default_backend() -> String { "sqlite".to_string() }
fn default_retention() -> u32 { 90 }
fn default_promotion_threshold() -> u32 { 10 }

impl Default for FailureStoreConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            path: None,
            retention_days: default_retention(),
            promotion_threshold: default_promotion_threshold(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationGateConfig {
    #[serde(default)]
    pub rules_dir: Option<String>,
    #[serde(default = "default_uncertain_action")]
    pub default_action_on_uncertain: String,
}

fn default_uncertain_action() -> String { "pass".to_string() }
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod config;` to `lib.rs`.

- [ ] **Step 5: Run test**

Run: `cargo test -p rustycode-orchestration --test config_test`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add OrchestrationConfig YAML loader with validation"
```

---

### Task 7: EscalationRouter

**Files:**
- Create: `crates/rustycode-orchestration/src/escalation_router.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/escalation_router_test.rs`:

```rust
use rustycode_orchestration::escalation_router::{
    DefaultEscalationRouter, EscalationDecision, EscalationRouter,
};
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorSignal};
use rustycode_orchestration::state_machine::{TaskContext, TaskPhase};
use rustycode_orchestration::failure_store::MemoryFailureStore;
use std::sync::Arc;

fn make_error(cat: ErrorCategory) -> ErrorSignal {
    ErrorSignal::new(cat, Some(1), "test error".into(), "step-1".into(), "bash".into())
}

#[test]
fn test_tier2_critical_error_escalates() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.01).unwrap();
    ctx.increment_attempt();

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::CompileError));
    assert!(matches!(decision, EscalationDecision::Escalate { next_tier: 3, .. }));
}

#[test]
fn test_tier2_non_critical_retries() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.01).unwrap();
    ctx.increment_attempt();

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::Custom("Unknown".into())));
    assert!(matches!(decision, EscalationDecision::Retry));
}

#[test]
fn test_tier2_max_attempts_escalates() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.01).unwrap();
    ctx.increment_attempt();
    ctx.increment_attempt();

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::Custom("X".into())));
    assert!(matches!(decision, EscalationDecision::Escalate { next_tier: 3, .. }));
}

#[test]
fn test_tier4_exhausted_abandons() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.0).unwrap();
    ctx.increment_tier();
    ctx.increment_tier();  // Now tier 4

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::CompileError));
    assert!(matches!(decision, EscalationDecision::Abandon { .. }));
}

#[test]
fn test_budget_exceeded_abandons() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.51).unwrap();  // Over budget
    ctx.increment_attempt();

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::CompileError));
    assert!(matches!(decision, EscalationDecision::Abandon { .. }));
}

#[test]
fn test_budget_warning_at_80pct() {
    let store = Arc::new(MemoryFailureStore::new());
    let router = DefaultEscalationRouter::new(store);
    let mut ctx = TaskContext::new("t".into(), 0.50);
    ctx.transition(TaskPhase::Executing, 0.41).unwrap();  // 82% used
    ctx.increment_attempt();

    let decision = router.should_escalate(&ctx, &make_error(ErrorCategory::Custom("X".into())));
    assert!(matches!(decision, EscalationDecision::WarnBudget { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test escalation_router_test`

Expected: FAIL.

- [ ] **Step 3: Implement escalation_router.rs**

Create `crates/rustycode-orchestration/src/escalation_router.rs`:

```rust
use std::sync::Arc;

use crate::error_signal::{ErrorCategory, ErrorSignal};
use crate::execution_trace::ExecutionTrace;
use crate::failure_store::FailurePatternStore;
use crate::state_machine::TaskContext;

#[derive(Debug, Clone)]
pub enum EscalationDecision {
    Retry,
    Escalate { next_tier: u8, reason: String },
    Abandon { reason: String },
    WarnBudget { remaining_usd: f64 },
}

pub trait EscalationRouter: Send + Sync {
    fn should_escalate(
        &self,
        ctx: &TaskContext,
        error: &ErrorSignal,
    ) -> EscalationDecision;
}

pub struct DefaultEscalationRouter {
    failure_store: Arc<dyn FailurePatternStore>,
    hallucination_window: u8,
    budget_warn_threshold: f64,  // 0.0 to 1.0 (e.g., 0.80)
}

impl DefaultEscalationRouter {
    pub fn new(failure_store: Arc<dyn FailurePatternStore>) -> Self {
        Self {
            failure_store,
            hallucination_window: 3,
            budget_warn_threshold: 0.80,
        }
    }

    pub fn with_hallucination_window(mut self, window: u8) -> Self {
        self.hallucination_window = window;
        self
    }

    pub fn with_warn_threshold(mut self, threshold: f64) -> Self {
        self.budget_warn_threshold = threshold;
        self
    }
}

impl EscalationRouter for DefaultEscalationRouter {
    fn should_escalate(
        &self,
        ctx: &TaskContext,
        error: &ErrorSignal,
    ) -> EscalationDecision {
        // Hard stops
        if detect_hallucination(&ctx.execution_trace, self.hallucination_window) {
            return EscalationDecision::Abandon {
                reason: "hallucination_loop_detected".to_string(),
            };
        }
        if ctx.cost_used >= ctx.max_budget {
            return EscalationDecision::Abandon {
                reason: format!("budget_exceeded ({:.2} >= {:.2})", ctx.cost_used, ctx.max_budget),
            };
        }
        if ctx.cost_used >= self.budget_warn_threshold * ctx.max_budget {
            return EscalationDecision::WarnBudget {
                remaining_usd: ctx.budget_remaining(),
            };
        }

        // Per-tier logic
        match ctx.current_tier {
            2 => escalate_tier2(ctx, &error.category),
            3 => escalate_tier3(ctx, &error.category),
            4 => EscalationDecision::Abandon {
                reason: "tier4_exhausted".to_string(),
            },
            _ => EscalationDecision::Retry,
        }
    }
}

fn is_critical(category: &ErrorCategory) -> bool {
    matches!(
        category,
        ErrorCategory::SyntaxError
            | ErrorCategory::CompileError
            | ErrorCategory::TypeError
            | ErrorCategory::LogicError
            | ErrorCategory::ContextLengthExceeded
    )
}

fn escalate_tier2(ctx: &TaskContext, category: &ErrorCategory) -> EscalationDecision {
    // Recoverable errors: retry first before escalating
    if matches!(category, ErrorCategory::PermissionDenied | ErrorCategory::DiskFull)
        && ctx.attempt_count < 2
    {
        return EscalationDecision::Retry;
    }
    if is_critical(category) {
        return EscalationDecision::Escalate {
            next_tier: 3,
            reason: format!("tier2_critical_error:{:?}", category),
        };
    }
    if ctx.attempt_count >= 2 {
        return EscalationDecision::Escalate {
            next_tier: 3,
            reason: "tier2_max_attempts".to_string(),
        };
    }
    EscalationDecision::Retry
}

fn escalate_tier3(ctx: &TaskContext, category: &ErrorCategory) -> EscalationDecision {
    if is_critical(category) || ctx.attempt_count >= 2 {
        return EscalationDecision::Escalate {
            next_tier: 4,
            reason: format!("tier3_escalate:{:?}", category),
        };
    }
    EscalationDecision::Retry
}

/// Detect if the last `window` entries in the trace are identical tool calls.
fn detect_hallucination(trace: &ExecutionTrace, window: u8) -> bool {
    if trace.steps.len() < window as usize {
        return false;
    }
    let last = trace.last_n_tool_calls(window as usize);
    if last.len() < 2 {
        return false;
    }
    let first = &last[0];
    last.iter().all(|e| {
        e.tool_name == first.tool_name
            && e.tool_args == first.tool_args
            && e.output == first.output
    })
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod escalation_router;` to `lib.rs`.

- [ ] **Step 5: Run test**

Run: `cargo test -p rustycode-orchestration --test escalation_router_test`

Expected: 6 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add DefaultEscalationRouter with per-tier logic"
```

---

### Task 8: ModelRegistry (Capability-Based Selection)

**Files:**
- Create: `crates/rustycode-orchestration/src/model_registry.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/model_registry_test.rs`:

```rust
use rustycode_orchestration::config::ModelSpec;
use rustycode_orchestration::model_registry::ModelRegistry;

fn spec(name: &str, provider: &str, input: f64, output: f64) -> ModelSpec {
    ModelSpec {
        name: name.into(),
        provider: provider.into(),
        cost_per_1m_tokens_input: input,
        cost_per_1m_tokens_output: output,
        context_window: 128_000,
        supports_extended_thinking: false,
        max_thinking_tokens: None,
    }
}

#[test]
fn test_registry_picks_tier_from_config_order() {
    let registry = ModelRegistry::new(
        vec![spec("haiku", "anthropic", 0.80, 4.0), spec("gpt-3.5", "openai", 0.50, 1.5)],
        vec![spec("sonnet", "anthropic", 3.0, 15.0)],
        vec![spec("opus", "anthropic", 15.0, 75.0)],
    );
    assert_eq!(registry.pick_for_tier(2).unwrap().name, "haiku");
    assert_eq!(registry.pick_for_tier(3).unwrap().name, "sonnet");
    assert_eq!(registry.pick_for_tier(4).unwrap().name, "opus");
}

#[test]
fn test_registry_graceful_degradation_single_model() {
    // Only Haiku available - should be used for all tiers
    let registry = ModelRegistry::new(
        vec![spec("haiku", "anthropic", 0.80, 4.0)],
        vec![],
        vec![],
    );
    assert_eq!(registry.pick_for_tier(2).unwrap().name, "haiku");
    assert_eq!(registry.pick_for_tier(3).unwrap().name, "haiku");
    assert_eq!(registry.pick_for_tier(4).unwrap().name, "haiku");
}

#[test]
fn test_registry_falls_back_tier3_missing_uses_next_available() {
    // Tier 3 missing, should fall back to tier 4 for tier 3 requests? Or tier 2?
    // Spec says: pick cheapest that meets reasoning requirement.
    // With no tier 3, tier 3 request should use strongest available (tier 4).
    let registry = ModelRegistry::new(
        vec![spec("haiku", "anthropic", 0.80, 4.0)],
        vec![],
        vec![spec("opus", "anthropic", 15.0, 75.0)],
    );
    assert_eq!(registry.pick_for_tier(2).unwrap().name, "haiku");
    assert_eq!(registry.pick_for_tier(3).unwrap().name, "opus");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustycode-orchestration --test model_registry_test`

Expected: FAIL.

- [ ] **Step 3: Implement model_registry.rs**

Create `crates/rustycode-orchestration/src/model_registry.rs`:

```rust
use crate::config::ModelSpec;

pub struct ModelRegistry {
    tier_2: Vec<ModelSpec>,
    tier_3: Vec<ModelSpec>,
    tier_4: Vec<ModelSpec>,
}

impl ModelRegistry {
    pub fn new(tier_2: Vec<ModelSpec>, tier_3: Vec<ModelSpec>, tier_4: Vec<ModelSpec>) -> Self {
        Self { tier_2, tier_3, tier_4 }
    }

    /// Pick the preferred model for the requested tier, with graceful degradation.
    ///
    /// Fallback order:
    /// - Tier 2 request: tier_2 → tier_3 → tier_4
    /// - Tier 3 request: tier_3 → tier_4 → tier_2
    /// - Tier 4 request: tier_4 → tier_3 → tier_2
    pub fn pick_for_tier(&self, tier: u8) -> Option<&ModelSpec> {
        let primary: &[ModelSpec] = match tier {
            2 => &self.tier_2,
            3 => &self.tier_3,
            4 => &self.tier_4,
            _ => return None,
        };
        if let Some(m) = primary.first() {
            return Some(m);
        }
        // Graceful degradation
        match tier {
            2 => self.tier_3.first().or_else(|| self.tier_4.first()),
            3 => self.tier_4.first().or_else(|| self.tier_2.first()),
            4 => self.tier_3.first().or_else(|| self.tier_2.first()),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod model_registry;` to `lib.rs`.

- [ ] **Step 5: Run test**

Run: `cargo test -p rustycode-orchestration --test model_registry_test`

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add ModelRegistry with graceful degradation"
```

---

## Phase B: Component Pipeline (Tasks 9-14)

### Task 9: TaskDecomposer Trait & LLM Implementation

**Files:**
- Create: `crates/rustycode-orchestration/src/decomposer.rs`

Research: review how `rustycode-llm::provider::LLMProvider` is used in `crates/rustycode-tui/src/app/streaming/response.rs` to understand the call pattern for non-streaming single-shot LLM calls.

- [ ] **Step 1: Write the failing test** (using a mock)

Create `crates/rustycode-orchestration/tests/decomposer_test.rs`:

```rust
use async_trait::async_trait;
use rustycode_orchestration::decomposer::{
    DecomposedTask, DecompositionContext, TaskDecomposer,
};
use rustycode_orchestration::types::{Difficulty, OutputType, Step};

struct FakeDecomposer {
    response: String,
}

#[async_trait]
impl TaskDecomposer for FakeDecomposer {
    async fn decompose(
        &self,
        _task: &str,
        _context: &DecompositionContext,
    ) -> anyhow::Result<DecomposedTask> {
        Ok(DecomposedTask {
            original_task: "test".into(),
            task_category: "test_category".into(),
            estimated_difficulty: Difficulty::Easy,
            steps: vec![
                Step {
                    id: "s1".into(),
                    index: 0,
                    description: "Install R".into(),
                    expected_output_type: OutputType::Command,
                    suggested_tool: Some("bash".into()),
                    retry_on_failure: true,
                },
                Step {
                    id: "s2".into(),
                    index: 1,
                    description: "Run statistical test".into(),
                    expected_output_type: OutputType::Data,
                    suggested_tool: Some("bash".into()),
                    retry_on_failure: true,
                },
            ],
        })
    }
}

#[tokio::test]
async fn test_decompose_produces_steps() {
    let decomposer = FakeDecomposer { response: "".into() };
    let ctx = DecompositionContext::new();
    let result = decomposer.decompose("statistical analysis task", &ctx).await.unwrap();
    assert_eq!(result.steps.len(), 2);
    assert_eq!(result.steps[0].index, 0);
    assert_eq!(result.task_category, "test_category");
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL (types don't exist).

- [ ] **Step 3: Implement decomposer.rs**

Create `crates/rustycode-orchestration/src/decomposer.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::{Difficulty, Step};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecomposedTask {
    pub original_task: String,
    pub task_category: String,
    pub estimated_difficulty: Difficulty,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Default)]
pub struct DecompositionContext {
    pub historical_patterns: Vec<String>,
    pub workspace_description: Option<String>,
}

impl DecompositionContext {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
pub trait TaskDecomposer: Send + Sync {
    async fn decompose(
        &self,
        task: &str,
        context: &DecompositionContext,
    ) -> anyhow::Result<DecomposedTask>;
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod decomposer;`.

- [ ] **Step 5: Run test**

Expected: PASS.

- [ ] **Step 6: Add LlmDecomposer stub**

Append to `decomposer.rs`:

```rust
use std::sync::Arc;
use rustycode_llm::provider::LLMProvider;

pub struct LlmDecomposer {
    provider: Arc<dyn LLMProvider>,
    model_name: String,
}

impl LlmDecomposer {
    pub fn new(provider: Arc<dyn LLMProvider>, model_name: String) -> Self {
        Self { provider, model_name }
    }

    fn build_prompt(task: &str, context: &DecompositionContext) -> String {
        let patterns_section = if context.historical_patterns.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nHistorical failure patterns to consider:\n{}",
                context.historical_patterns.join("\n- ")
            )
        };
        format!(
            r#"Break the following task into 5-10 concrete executable steps.
Each step should be small enough that a simple AI model can execute it with one tool call.

Task:
{task}
{patterns_section}

Respond ONLY with valid JSON matching this schema:
{{
  "task_category": "<short lowercase category like 'rust_refactoring', 'data_etl'>",
  "estimated_difficulty": "Easy" | "Medium" | "Hard",
  "steps": [
    {{
      "id": "<uuid>",
      "index": 0,
      "description": "<what the step does>",
      "expected_output_type": "File" | "Command" | "Query" | "Code" | "Data" | "Verification",
      "suggested_tool": "bash" | "read_file" | "write_file" | "grep" | null,
      "retry_on_failure": true
    }}
  ]
}}"#
        )
    }
}

#[async_trait]
impl TaskDecomposer for LlmDecomposer {
    async fn decompose(
        &self,
        task: &str,
        context: &DecompositionContext,
    ) -> anyhow::Result<DecomposedTask> {
        let _prompt = Self::build_prompt(task, context);
        // TODO: wire actual LLM call in Task C (pipeline integration)
        // For now this stub returns an error to make the integration point explicit.
        anyhow::bail!("LlmDecomposer not yet wired to provider; see pipeline task")
    }
}
```

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add TaskDecomposer trait and LlmDecomposer stub"
```

---

### Task 10: PlanRefiner

**Files:**
- Create: `crates/rustycode-orchestration/src/plan_refiner.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/plan_refiner_test.rs`:

```rust
use async_trait::async_trait;
use rustycode_orchestration::decomposer::DecomposedTask;
use rustycode_orchestration::plan_refiner::{PlanRefiner, RefinementResult};
use rustycode_orchestration::failure_store::StoredPattern;
use rustycode_orchestration::types::{Difficulty, Step, OutputType};

struct StubRefiner {
    approve_always: bool,
}

#[async_trait]
impl PlanRefiner for StubRefiner {
    async fn refine(
        &self,
        _plan: &DecomposedTask,
        _patterns: &[StoredPattern],
        _trace: Option<&rustycode_orchestration::execution_trace::ExecutionTrace>,
    ) -> anyhow::Result<RefinementResult> {
        if self.approve_always {
            Ok(RefinementResult::Approve)
        } else {
            Ok(RefinementResult::Reject { reason: "no".into(), suggested_alternative: None })
        }
    }
}

fn fake_plan() -> DecomposedTask {
    DecomposedTask {
        original_task: "t".into(),
        task_category: "c".into(),
        estimated_difficulty: Difficulty::Easy,
        steps: vec![Step {
            id: "s1".into(), index: 0, description: "d".into(),
            expected_output_type: OutputType::Command,
            suggested_tool: None, retry_on_failure: true,
        }],
    }
}

#[tokio::test]
async fn test_approve() {
    let refiner = StubRefiner { approve_always: true };
    let plan = fake_plan();
    let result = refiner.refine(&plan, &[], None).await.unwrap();
    assert!(matches!(result, RefinementResult::Approve));
}

#[tokio::test]
async fn test_reject() {
    let refiner = StubRefiner { approve_always: false };
    let plan = fake_plan();
    let result = refiner.refine(&plan, &[], None).await.unwrap();
    assert!(matches!(result, RefinementResult::Reject { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Implement plan_refiner.rs**

Create `crates/rustycode-orchestration/src/plan_refiner.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::decomposer::DecomposedTask;
use crate::execution_trace::ExecutionTrace;
use crate::failure_store::StoredPattern;
use crate::types::Step;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefinementResult {
    Approve,
    Modify { updated_steps: Vec<Step>, reasoning: String },
    Reject { reason: String, suggested_alternative: Option<DecomposedTask> },
}

#[async_trait]
pub trait PlanRefiner: Send + Sync {
    async fn refine(
        &self,
        plan: &DecomposedTask,
        patterns: &[StoredPattern],
        trace: Option<&ExecutionTrace>,
    ) -> anyhow::Result<RefinementResult>;
}

/// Trivial refiner: always approves. Useful as a default before wiring LLM.
pub struct ApproveAllRefiner;

#[async_trait]
impl PlanRefiner for ApproveAllRefiner {
    async fn refine(
        &self,
        _plan: &DecomposedTask,
        _patterns: &[StoredPattern],
        _trace: Option<&ExecutionTrace>,
    ) -> anyhow::Result<RefinementResult> {
        Ok(RefinementResult::Approve)
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod plan_refiner;`.

- [ ] **Step 5: Run test**

Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add PlanRefiner trait with ApproveAllRefiner default"
```

---

### Task 11: Orchestrator (Tier 2)

**Files:**
- Create: `crates/rustycode-orchestration/src/orchestrator.rs`

The orchestrator executes a single step by calling an LLM with a minimal prompt and dispatching the resulting tool call. Wires to `rustycode-tools::ToolRegistry` for execution.

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/orchestrator_test.rs`:

```rust
use async_trait::async_trait;
use rustycode_orchestration::orchestrator::{Orchestrator, OrchestrationContext, StepResult};
use rustycode_orchestration::types::{Step, OutputType};

struct SuccessOrchestrator;

#[async_trait]
impl Orchestrator for SuccessOrchestrator {
    async fn execute_step(
        &self,
        step: &Step,
        _context: &OrchestrationContext,
    ) -> anyhow::Result<StepResult> {
        Ok(StepResult {
            step_id: step.id.clone(),
            success: true,
            tool_name: "bash".into(),
            tool_args: serde_json::json!({"command": "echo ok"}),
            tool_output: Some("ok\n".into()),
            exit_code: Some(0),
            error_signal: None,
            duration_ms: 42,
            cost_usd: 0.001,
        })
    }
}

#[tokio::test]
async fn test_orchestrator_returns_result() {
    let orch = SuccessOrchestrator;
    let step = Step {
        id: "s1".into(), index: 0, description: "Test step".into(),
        expected_output_type: OutputType::Command,
        suggested_tool: Some("bash".into()), retry_on_failure: true,
    };
    let ctx = OrchestrationContext::default();
    let result = orch.execute_step(&step, &ctx).await.unwrap();
    assert!(result.success);
    assert_eq!(result.tool_name, "bash");
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Implement orchestrator.rs**

Create `crates/rustycode-orchestration/src/orchestrator.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error_signal::ErrorSignal;
use crate::execution_trace::ExecutionTrace;
use crate::types::Step;

#[derive(Debug, Clone, Default)]
pub struct OrchestrationContext {
    pub original_task: String,
    pub plan_notes: Option<String>,
    pub prior_trace: Option<ExecutionTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_id: String,
    pub success: bool,
    pub tool_name: String,
    pub tool_args: serde_json::Value,
    pub tool_output: Option<String>,
    pub exit_code: Option<i32>,
    pub error_signal: Option<ErrorSignal>,
    pub duration_ms: u64,
    pub cost_usd: f64,
}

#[async_trait]
pub trait Orchestrator: Send + Sync {
    async fn execute_step(
        &self,
        step: &Step,
        context: &OrchestrationContext,
    ) -> anyhow::Result<StepResult>;
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod orchestrator;`.

- [ ] **Step 5: Run test**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add Orchestrator trait and StepResult type"
```

---

### Task 12: VerificationGate with Rule Files

**Files:**
- Create: `crates/rustycode-orchestration/src/verification_gate.rs`
- Create: `crates/rustycode-orchestration/rules/default.yaml`
- Create: `crates/rustycode-orchestration/rules/rust_refactoring.yaml`
- Create: `crates/rustycode-orchestration/rules/data_etl.yaml`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/verification_gate_test.rs`:

```rust
use rustycode_orchestration::orchestrator::StepResult;
use rustycode_orchestration::types::{OutputType, Step};
use rustycode_orchestration::verification_gate::{
    HeuristicVerificationGate, VerificationGate, VerificationOutcome,
};

fn make_step() -> Step {
    Step {
        id: "s1".into(),
        index: 0,
        description: "test".into(),
        expected_output_type: OutputType::Command,
        suggested_tool: Some("bash".into()),
        retry_on_failure: true,
    }
}

fn success_result() -> StepResult {
    StepResult {
        step_id: "s1".into(),
        success: true,
        tool_name: "bash".into(),
        tool_args: serde_json::json!({}),
        tool_output: Some("output".into()),
        exit_code: Some(0),
        error_signal: None,
        duration_ms: 10,
        cost_usd: 0.0,
    }
}

#[test]
fn test_success_is_valid() {
    let gate = HeuristicVerificationGate::default();
    let r = gate.verify(&make_step(), &success_result());
    assert!(matches!(r, VerificationOutcome::Valid));
}

#[test]
fn test_failure_exit_code_is_invalid() {
    let gate = HeuristicVerificationGate::default();
    let mut result = success_result();
    result.success = false;
    result.exit_code = Some(1);
    let r = gate.verify(&make_step(), &result);
    assert!(matches!(r, VerificationOutcome::Invalid { .. }));
}

#[test]
fn test_empty_output_for_command_is_uncertain() {
    let gate = HeuristicVerificationGate::default();
    let mut result = success_result();
    result.tool_output = Some(String::new());
    let r = gate.verify(&make_step(), &result);
    assert!(matches!(r, VerificationOutcome::Uncertain { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Implement verification_gate.rs**

Create `crates/rustycode-orchestration/src/verification_gate.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::error_signal::ErrorCategory;
use crate::orchestrator::StepResult;
use crate::types::{OutputType, Step};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationOutcome {
    Valid,
    Invalid { reason: String, category: ErrorCategory },
    Uncertain { reason: String },
}

pub trait VerificationGate: Send + Sync {
    fn verify(&self, step: &Step, result: &StepResult) -> VerificationOutcome;
}

#[derive(Default)]
pub struct HeuristicVerificationGate;

impl VerificationGate for HeuristicVerificationGate {
    fn verify(&self, step: &Step, result: &StepResult) -> VerificationOutcome {
        // Non-zero exit code → Invalid
        if let Some(code) = result.exit_code {
            if code != 0 {
                return VerificationOutcome::Invalid {
                    reason: format!("non-zero exit code: {}", code),
                    category: ErrorCategory::LogicError,
                };
            }
        }
        if !result.success {
            return VerificationOutcome::Invalid {
                reason: "tool reported failure".into(),
                category: ErrorCategory::LogicError,
            };
        }
        // Empty output for command-producing steps is suspicious
        match step.expected_output_type {
            OutputType::Command | OutputType::File | OutputType::Data => {
                match &result.tool_output {
                    Some(s) if s.trim().is_empty() => VerificationOutcome::Uncertain {
                        reason: "empty output for command/file/data step".into(),
                    },
                    _ => VerificationOutcome::Valid,
                }
            }
            _ => VerificationOutcome::Valid,
        }
    }
}
```

- [ ] **Step 4: Create default rule files**

Create `crates/rustycode-orchestration/rules/default.yaml`:

```yaml
task_type: default
rules:
  - description: Tool call must succeed with exit code 0
    check: exit_code
    expected: 0
    on_failure: LogicError
```

Create `crates/rustycode-orchestration/rules/rust_refactoring.yaml`:

```yaml
task_type: rust_refactoring
rules:
  - description: Generated code must not contain unresolved TODO markers
    check: regex_absent
    pattern: '(?i)TODO|FIXME|unimplemented!\(\)'
    on_match: LogicError
  - description: Rust code should compile (checked via cargo check)
    check: command
    command: cargo check
    on_failure: LogicError
```

Create `crates/rustycode-orchestration/rules/data_etl.yaml`:

```yaml
task_type: data_etl
rules:
  - description: Output file must exist and be non-empty
    check: file_exists_nonempty
    path_from: output
    on_failure: LogicError
```

- [ ] **Step 5: Export from lib.rs**

Add `pub mod verification_gate;`.

- [ ] **Step 6: Run test**

Expected: 3 tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add HeuristicVerificationGate with rule file stubs"
```

---

### Task 13: Reasoner (Tier 3) Trait

**Files:**
- Create: `crates/rustycode-orchestration/src/reasoner.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/reasoner_test.rs`:

```rust
use async_trait::async_trait;
use rustycode_orchestration::reasoner::{ReplanResult, Reasoner};
use rustycode_orchestration::execution_trace::ExecutionTrace;
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorSignal};
use rustycode_orchestration::types::{OutputType, Step};

struct StubReasoner;

#[async_trait]
impl Reasoner for StubReasoner {
    async fn replan(
        &self,
        _trace: &ExecutionTrace,
        failed_step: &Step,
        _error: &ErrorSignal,
        _patterns: &[rustycode_orchestration::failure_store::StoredPattern],
    ) -> anyhow::Result<ReplanResult> {
        Ok(ReplanResult {
            updated_step: failed_step.clone(),
            reasoning: "retry with better prompt".into(),
            confidence: 0.8,
            reasoning_quality_score: 4,
        })
    }
}

#[tokio::test]
async fn test_replan_produces_updated_step() {
    let reasoner = StubReasoner;
    let trace = ExecutionTrace::new("t".into());
    let step = Step {
        id: "s1".into(), index: 0, description: "d".into(),
        expected_output_type: OutputType::Command,
        suggested_tool: None, retry_on_failure: true,
    };
    let err = ErrorSignal::new(
        ErrorCategory::CompileError, Some(1),
        "error[E0599]".into(), "s1".into(), "bash".into(),
    );
    let result = reasoner.replan(&trace, &step, &err, &[]).await.unwrap();
    assert_eq!(result.confidence, 0.8);
    assert_eq!(result.reasoning_quality_score, 4);
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Implement reasoner.rs**

Create `crates/rustycode-orchestration/src/reasoner.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error_signal::ErrorSignal;
use crate::execution_trace::ExecutionTrace;
use crate::failure_store::StoredPattern;
use crate::types::Step;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanResult {
    pub updated_step: Step,
    pub reasoning: String,
    pub confidence: f64,
    pub reasoning_quality_score: u8,
}

#[async_trait]
pub trait Reasoner: Send + Sync {
    async fn replan(
        &self,
        trace: &ExecutionTrace,
        failed_step: &Step,
        error: &ErrorSignal,
        patterns: &[StoredPattern],
    ) -> anyhow::Result<ReplanResult>;
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod reasoner;`.

- [ ] **Step 5: Run test**

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add Reasoner trait with ReplanResult"
```

---

### Task 14: DeepThinkerAdapter (Tier 4)

**Files:**
- Create: `crates/rustycode-orchestration/src/deep_thinker_adapter.rs`

This task wraps the existing `rustycode-deep-thinker` crate as a Tier 4 implementation. Uses its `DefaultActivationPolicy` and `auto_invoke_decompose` pattern that already exists in `crates/rustycode-tui/src/services/deep_thinking.rs`.

- [ ] **Step 1: Research existing deep-thinker API**

Read: `crates/rustycode-deep-thinker/src/lib.rs` and `src/activator.rs` to understand the public API.

- [ ] **Step 2: Write the failing test**

Create `crates/rustycode-orchestration/tests/deep_thinker_adapter_test.rs`:

```rust
use async_trait::async_trait;
use rustycode_orchestration::deep_thinker_adapter::{DeepThinker, DeepThinkingContext, DeepThinkingResult};
use rustycode_orchestration::execution_trace::ExecutionTrace;

struct StubDeepThinker;

#[async_trait]
impl DeepThinker for StubDeepThinker {
    async fn solve(
        &self,
        _trace: &ExecutionTrace,
        _context: &DeepThinkingContext,
    ) -> anyhow::Result<DeepThinkingResult> {
        Ok(DeepThinkingResult {
            solution: "attempt the task with approach X".into(),
            confidence: 0.75,
            reasoning_quality_score: 5,
        })
    }
}

#[tokio::test]
async fn test_deep_thinker_returns_solution() {
    let thinker = StubDeepThinker;
    let trace = ExecutionTrace::new("t".into());
    let ctx = DeepThinkingContext {
        original_task: "test".into(),
        decomposed_plan_summary: "1. do X; 2. do Y".into(),
        all_failures_summary: "compile errors at step 2".into(),
        tier3_attempts_summary: "retried once with type fix, still failed".into(),
    };
    let result = thinker.solve(&trace, &ctx).await.unwrap();
    assert!(result.confidence > 0.0);
    assert_eq!(result.reasoning_quality_score, 5);
}
```

- [ ] **Step 3: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 4: Implement deep_thinker_adapter.rs**

Create `crates/rustycode-orchestration/src/deep_thinker_adapter.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::execution_trace::ExecutionTrace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepThinkingContext {
    pub original_task: String,
    pub decomposed_plan_summary: String,
    pub all_failures_summary: String,
    pub tier3_attempts_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepThinkingResult {
    pub solution: String,
    pub confidence: f64,
    pub reasoning_quality_score: u8,
}

#[async_trait]
pub trait DeepThinker: Send + Sync {
    async fn solve(
        &self,
        trace: &ExecutionTrace,
        context: &DeepThinkingContext,
    ) -> anyhow::Result<DeepThinkingResult>;
}

/// Wraps `rustycode-deep-thinker` for Tier 4 invocation.
/// Wire up with real provider in Phase C pipeline integration.
pub struct RustyCodeDeepThinker {
    // placeholder - wire real deep-thinker components later
}

impl RustyCodeDeepThinker {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for RustyCodeDeepThinker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DeepThinker for RustyCodeDeepThinker {
    async fn solve(
        &self,
        _trace: &ExecutionTrace,
        context: &DeepThinkingContext,
    ) -> anyhow::Result<DeepThinkingResult> {
        // TODO: wire rustycode-deep-thinker::RealExecutor with full graph-of-thoughts
        // For now, return a placeholder solution that makes the integration point explicit
        Ok(DeepThinkingResult {
            solution: format!(
                "Placeholder: Review trace and attempt task '{}' using deep reasoning.",
                context.original_task
            ),
            confidence: 0.5,
            reasoning_quality_score: 3,
        })
    }
}
```

- [ ] **Step 5: Export from lib.rs**

Add `pub mod deep_thinker_adapter;`.

- [ ] **Step 6: Run test**

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add DeepThinker trait and stub RustyCodeDeepThinker adapter"
```

---

## Phase C: Pipeline Integration (Tasks 15-18)

### Task 15: OrchestrationMetrics

**Files:**
- Create: `crates/rustycode-orchestration/src/metrics.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/rustycode-orchestration/tests/metrics_test.rs`:

```rust
use rustycode_orchestration::metrics::OrchestrationMetrics;
use rustycode_orchestration::types::TaskOutcome;

#[test]
fn test_metrics_new_initializes_empty() {
    let m = OrchestrationMetrics::new("task-1".into(), "rust_refactoring".into());
    assert_eq!(m.task_id, "task-1");
    assert_eq!(m.task_category, "rust_refactoring");
    assert!(m.cost_breakdown.is_empty());
    assert_eq!(m.steps_succeeded, 0);
}

#[test]
fn test_record_tier_cost() {
    let mut m = OrchestrationMetrics::new("t".into(), "c".into());
    m.record_tier_cost(2, 0.01);
    m.record_tier_cost(2, 0.005);
    m.record_tier_cost(3, 0.20);
    assert!((m.cost_breakdown[&2] - 0.015).abs() < 1e-9);
    assert!((m.cost_breakdown[&3] - 0.20).abs() < 1e-9);
    assert!((m.total_cost() - 0.215).abs() < 1e-9);
}

#[test]
fn test_finalize_sets_outcome() {
    let mut m = OrchestrationMetrics::new("t".into(), "c".into());
    m.finalize(TaskOutcome::SuccessAtTier(2), 1500);
    assert!(matches!(m.final_outcome, Some(TaskOutcome::SuccessAtTier(2))));
    assert_eq!(m.total_duration_ms, 1500);
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL.

- [ ] **Step 3: Implement metrics.rs**

Create `crates/rustycode-orchestration/src/metrics.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::TaskOutcome;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationMetrics {
    pub task_id: String,
    pub task_category: String,
    pub total_duration_ms: u64,
    pub cost_breakdown: HashMap<u8, f64>,
    pub attempts_per_tier: HashMap<u8, u8>,
    pub final_outcome: Option<TaskOutcome>,
    pub escalation_reasons: Vec<String>,
    pub reasoning_quality_score: Option<u8>,
    pub hallucination_detected: bool,
    pub budget_warnings_emitted: u8,
    pub steps_succeeded: u8,
    pub steps_failed: u8,
}

impl OrchestrationMetrics {
    pub fn new(task_id: String, task_category: String) -> Self {
        Self {
            task_id,
            task_category,
            total_duration_ms: 0,
            cost_breakdown: HashMap::new(),
            attempts_per_tier: HashMap::new(),
            final_outcome: None,
            escalation_reasons: Vec::new(),
            reasoning_quality_score: None,
            hallucination_detected: false,
            budget_warnings_emitted: 0,
            steps_succeeded: 0,
            steps_failed: 0,
        }
    }

    pub fn record_tier_cost(&mut self, tier: u8, cost_usd: f64) {
        *self.cost_breakdown.entry(tier).or_insert(0.0) += cost_usd;
    }

    pub fn record_attempt(&mut self, tier: u8) {
        *self.attempts_per_tier.entry(tier).or_insert(0) += 1;
    }

    pub fn record_escalation(&mut self, reason: String) {
        self.escalation_reasons.push(reason);
    }

    pub fn record_step_success(&mut self) {
        self.steps_succeeded = self.steps_succeeded.saturating_add(1);
    }

    pub fn record_step_failure(&mut self) {
        self.steps_failed = self.steps_failed.saturating_add(1);
    }

    pub fn record_budget_warning(&mut self) {
        self.budget_warnings_emitted = self.budget_warnings_emitted.saturating_add(1);
    }

    pub fn record_hallucination(&mut self) {
        self.hallucination_detected = true;
    }

    pub fn set_reasoning_quality(&mut self, score: u8) {
        self.reasoning_quality_score = Some(score);
    }

    pub fn finalize(&mut self, outcome: TaskOutcome, duration_ms: u64) {
        self.final_outcome = Some(outcome);
        self.total_duration_ms = duration_ms;
    }

    pub fn total_cost(&self) -> f64 {
        self.cost_breakdown.values().sum()
    }
}
```

- [ ] **Step 4: Export from lib.rs**

Add `pub mod metrics;`.

- [ ] **Step 5: Run test**

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add OrchestrationMetrics for per-task tracking"
```

---

### Task 16: OrchestrationPipeline (Main Entry Point)

**Files:**
- Create: `crates/rustycode-orchestration/src/pipeline.rs`

This is the **central integration task**. It wires together all the components from Tasks 1-15 into a single `OrchestrationPipeline::run()` entry point that takes a task description and returns an `OrchestrationMetrics` + `TaskOutcome`.

- [ ] **Step 1: Write the failing integration test**

Create `crates/rustycode-orchestration/tests/pipeline_integration_test.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use rustycode_orchestration::decomposer::{DecomposedTask, DecompositionContext, TaskDecomposer};
use rustycode_orchestration::deep_thinker_adapter::{
    DeepThinker, DeepThinkingContext, DeepThinkingResult,
};
use rustycode_orchestration::error_signal::{ErrorCategory, ErrorSignal};
use rustycode_orchestration::execution_trace::ExecutionTrace;
use rustycode_orchestration::failure_store::{MemoryFailureStore, StoredPattern};
use rustycode_orchestration::orchestrator::{
    OrchestrationContext, Orchestrator, StepResult,
};
use rustycode_orchestration::pipeline::{OrchestrationPipeline, PipelineComponents};
use rustycode_orchestration::plan_refiner::{ApproveAllRefiner, PlanRefiner, RefinementResult};
use rustycode_orchestration::reasoner::{ReplanResult, Reasoner};
use rustycode_orchestration::types::{Difficulty, OutputType, Step, TaskOutcome};
use rustycode_orchestration::verification_gate::HeuristicVerificationGate;
use rustycode_orchestration::escalation_router::DefaultEscalationRouter;

// Mock decomposer: 2 easy steps
struct MockDecomposer;
#[async_trait]
impl TaskDecomposer for MockDecomposer {
    async fn decompose(&self, _task: &str, _ctx: &DecompositionContext) -> anyhow::Result<DecomposedTask> {
        Ok(DecomposedTask {
            original_task: "test".into(),
            task_category: "test".into(),
            estimated_difficulty: Difficulty::Easy,
            steps: vec![
                Step { id: "s1".into(), index: 0, description: "step 1".into(),
                       expected_output_type: OutputType::Command, suggested_tool: Some("bash".into()),
                       retry_on_failure: true },
                Step { id: "s2".into(), index: 1, description: "step 2".into(),
                       expected_output_type: OutputType::Command, suggested_tool: Some("bash".into()),
                       retry_on_failure: true },
            ],
        })
    }
}

// Mock orchestrator: always succeeds
struct MockOrchestratorSuccess;
#[async_trait]
impl Orchestrator for MockOrchestratorSuccess {
    async fn execute_step(&self, step: &Step, _ctx: &OrchestrationContext) -> anyhow::Result<StepResult> {
        Ok(StepResult {
            step_id: step.id.clone(), success: true,
            tool_name: "bash".into(), tool_args: serde_json::json!({}),
            tool_output: Some("ok".into()), exit_code: Some(0),
            error_signal: None, duration_ms: 10, cost_usd: 0.001,
        })
    }
}

struct MockReasoner;
#[async_trait]
impl Reasoner for MockReasoner {
    async fn replan(&self, _t: &ExecutionTrace, step: &Step, _e: &ErrorSignal, _p: &[StoredPattern]) -> anyhow::Result<ReplanResult> {
        Ok(ReplanResult {
            updated_step: step.clone(),
            reasoning: "retry".into(), confidence: 0.8, reasoning_quality_score: 3,
        })
    }
}

struct MockDeepThinker;
#[async_trait]
impl DeepThinker for MockDeepThinker {
    async fn solve(&self, _t: &ExecutionTrace, _c: &DeepThinkingContext) -> anyhow::Result<DeepThinkingResult> {
        Ok(DeepThinkingResult { solution: "solve".into(), confidence: 0.9, reasoning_quality_score: 4 })
    }
}

#[tokio::test]
async fn test_happy_path_all_steps_succeed_at_tier2() {
    let store = Arc::new(MemoryFailureStore::new());
    let components = PipelineComponents {
        decomposer: Arc::new(MockDecomposer),
        plan_refiner: Arc::new(ApproveAllRefiner),
        orchestrator: Arc::new(MockOrchestratorSuccess),
        verification_gate: Arc::new(HeuristicVerificationGate::default()),
        reasoner: Arc::new(MockReasoner),
        deep_thinker: Arc::new(MockDeepThinker),
        escalation_router: Arc::new(DefaultEscalationRouter::new(store.clone())),
        failure_store: store,
    };
    let pipeline = OrchestrationPipeline::new(components, 0.50);
    let metrics = pipeline.run("test task").await.unwrap();

    assert!(matches!(metrics.final_outcome, Some(TaskOutcome::SuccessAtTier(2))));
    assert_eq!(metrics.steps_succeeded, 2);
    assert_eq!(metrics.steps_failed, 0);
    assert!(metrics.total_cost() < 0.05);
}
```

- [ ] **Step 2: Run test to verify it fails**

Expected: FAIL (pipeline.rs doesn't exist).

- [ ] **Step 3: Implement pipeline.rs**

Create `crates/rustycode-orchestration/src/pipeline.rs`:

```rust
use std::sync::Arc;
use std::time::Instant;

use tracing::{info, warn};

use crate::decomposer::{DecompositionContext, TaskDecomposer};
use crate::deep_thinker_adapter::{DeepThinker, DeepThinkingContext};
use crate::error::{OrchestrationError, Result};
use crate::error_signal::ErrorSignal;
use crate::escalation_router::{EscalationDecision, EscalationRouter};
use crate::execution_trace::TraceEntry;
use crate::failure_store::FailurePatternStore;
use crate::metrics::OrchestrationMetrics;
use crate::orchestrator::{OrchestrationContext, Orchestrator};
use crate::plan_refiner::{PlanRefiner, RefinementResult};
use crate::reasoner::Reasoner;
use crate::state_machine::{TaskContext, TaskPhase};
use crate::types::{Step, TaskOutcome};
use crate::verification_gate::{VerificationGate, VerificationOutcome};

pub struct PipelineComponents {
    pub decomposer: Arc<dyn TaskDecomposer>,
    pub plan_refiner: Arc<dyn PlanRefiner>,
    pub orchestrator: Arc<dyn Orchestrator>,
    pub verification_gate: Arc<dyn VerificationGate>,
    pub reasoner: Arc<dyn Reasoner>,
    pub deep_thinker: Arc<dyn DeepThinker>,
    pub escalation_router: Arc<dyn EscalationRouter>,
    pub failure_store: Arc<dyn FailurePatternStore>,
}

pub struct OrchestrationPipeline {
    components: PipelineComponents,
    max_budget: f64,
}

impl OrchestrationPipeline {
    pub fn new(components: PipelineComponents, max_budget: f64) -> Self {
        Self { components, max_budget }
    }

    /// Run the full orchestration pipeline for a task.
    pub async fn run(&self, task: &str) -> Result<OrchestrationMetrics> {
        let start = Instant::now();
        let task_id = uuid::Uuid::new_v4().to_string();

        // Step 1: Decompose
        let decomp_context = DecompositionContext::new();
        let plan = self
            .components
            .decomposer
            .decompose(task, &decomp_context)
            .await
            .map_err(OrchestrationError::Other)?;

        let mut metrics = OrchestrationMetrics::new(task_id.clone(), plan.task_category.clone());

        // Step 2: Plan Refiner (initial review)
        let patterns = self
            .components
            .failure_store
            .query_patterns(&plan.task_category)?;
        let plan = match self
            .components
            .plan_refiner
            .refine(&plan, &patterns, None)
            .await
            .map_err(OrchestrationError::Other)?
        {
            RefinementResult::Approve => plan,
            RefinementResult::Modify { updated_steps, reasoning } => {
                info!(reasoning = %reasoning, "Plan Refiner modified steps");
                crate::decomposer::DecomposedTask { steps: updated_steps, ..plan }
            }
            RefinementResult::Reject { reason, suggested_alternative: Some(alt) } => {
                warn!(reason = %reason, "Plan Refiner rejected; using alternative");
                alt
            }
            RefinementResult::Reject { reason, suggested_alternative: None } => {
                metrics.finalize(
                    TaskOutcome::Abandoned { reason: format!("plan_rejected:{}", reason) },
                    start.elapsed().as_millis() as u64,
                );
                return Ok(metrics);
            }
        };

        // Step 3: Initialize task context & transition to Executing
        let mut ctx = TaskContext::new(task_id, self.max_budget);
        ctx.transition(TaskPhase::Executing, 0.0)?;

        // Step 4: Execute each step
        for step in &plan.steps {
            if ctx.is_terminal() {
                break;
            }
            let outcome = self.execute_step_with_retries(step, &mut ctx, &mut metrics, &plan.task_category).await?;
            match outcome {
                StepExecutionOutcome::Success => metrics.record_step_success(),
                StepExecutionOutcome::Abandoned(reason) => {
                    metrics.record_step_failure();
                    ctx.transition(TaskPhase::Abandoned, 0.0)?;
                    metrics.finalize(
                        TaskOutcome::Abandoned { reason: reason.clone() },
                        start.elapsed().as_millis() as u64,
                    );
                    return Ok(metrics);
                }
            }
        }

        // All steps succeeded
        ctx.transition(TaskPhase::Success, 0.0)?;
        metrics.finalize(
            TaskOutcome::SuccessAtTier(ctx.current_tier),
            start.elapsed().as_millis() as u64,
        );
        Ok(metrics)
    }

    async fn execute_step_with_retries(
        &self,
        step: &Step,
        ctx: &mut TaskContext,
        metrics: &mut OrchestrationMetrics,
        task_category: &str,
    ) -> Result<StepExecutionOutcome> {
        loop {
            ctx.increment_attempt();
            metrics.record_attempt(ctx.current_tier);

            let orch_ctx = OrchestrationContext {
                original_task: String::new(),
                plan_notes: None,
                prior_trace: Some(ctx.execution_trace.clone()),
            };
            let result = self
                .components
                .orchestrator
                .execute_step(step, &orch_ctx)
                .await
                .map_err(OrchestrationError::Other)?;
            metrics.record_tier_cost(ctx.current_tier, result.cost_usd);
            ctx.cost_used += result.cost_usd;

            // Append to trace
            let trace_entry = if let Some(err) = &result.error_signal {
                TraceEntry::new_failure(
                    step.id.clone(),
                    step.index,
                    ctx.current_tier,
                    result.tool_name.clone(),
                    result.tool_args.clone(),
                    result.tool_output.clone().unwrap_or_default(),
                    result.exit_code,
                    err.clone(),
                    result.cost_usd,
                )
            } else {
                TraceEntry::new_success(
                    step.id.clone(),
                    step.index,
                    ctx.current_tier,
                    result.tool_name.clone(),
                    result.tool_args.clone(),
                    result.tool_output.clone().unwrap_or_default(),
                    result.exit_code,
                    result.cost_usd,
                )
            };
            ctx.execution_trace.append(trace_entry);

            // Verification Gate
            let verify = self.components.verification_gate.verify(step, &result);
            match verify {
                VerificationOutcome::Valid => return Ok(StepExecutionOutcome::Success),
                VerificationOutcome::Invalid { reason, category } => {
                    let signal = ErrorSignal::new(
                        category.clone(), result.exit_code, reason.clone(),
                        step.id.clone(), result.tool_name.clone(),
                    );
                    // Record failure pattern
                    let _ = self.components.failure_store.record_failure(&crate::failure_store::FailurePattern {
                        task_type: task_category.to_string(),
                        step_index: step.index,
                        error_category: category,
                        suggested_fix: None,
                        alternative_approach: None,
                        tier_failed: format!("Tier{}", ctx.current_tier),
                    });

                    // Decide escalation
                    let decision = self.components.escalation_router.should_escalate(ctx, &signal);
                    match decision {
                        EscalationDecision::Retry => continue,
                        EscalationDecision::Escalate { next_tier, reason } => {
                            info!(next_tier, reason, "Escalating");
                            metrics.record_escalation(reason);
                            ctx.transition(TaskPhase::Refining, 0.0)?;
                            ctx.increment_tier();
                            ctx.transition(TaskPhase::Executing, 0.0)?;
                            continue;
                        }
                        EscalationDecision::Abandon { reason } => {
                            return Ok(StepExecutionOutcome::Abandoned(reason));
                        }
                        EscalationDecision::WarnBudget { remaining_usd } => {
                            warn!(remaining_usd, "Budget approaching 80%");
                            metrics.record_budget_warning();
                            // Proceed but note the warning
                            continue;
                        }
                    }
                }
                VerificationOutcome::Uncertain { reason } => {
                    warn!(reason, "Verification uncertain; treating as success");
                    return Ok(StepExecutionOutcome::Success);
                }
            }
        }
    }
}

enum StepExecutionOutcome {
    Success,
    Abandoned(String),
}
```

- [ ] **Step 4: Add DryRun execution mode**

In `crates/rustycode-orchestration/src/pipeline.rs`, add to `OrchestrationPipeline`:

```rust
/// When enabled, skips real tool execution and logs intended tool calls
/// for planning analysis. Useful for testing decomposition quality
/// without burning LLM tokens on execution.
pub fn with_dry_run(mut self) -> Self {
    self.dry_run = true;
    self
}
```

Modify `run()` to check `self.dry_run` before calling `orchestrator.execute_step()` — in dry-run mode, produce a `StepResult` with mock output and `"DRY_RUN"` in the tool_name.

Add a test:
```rust
#[tokio::test]
async fn test_dry_run_logs_without_execution() {
    let store = Arc::new(MemoryFailureStore::new());
    let components = PipelineComponents {
        decomposer: Arc::new(MockDecomposer),
        plan_refiner: Arc::new(ApproveAllRefiner),
        orchestrator: Arc::new(MockOrchestratorSuccess),
        verification_gate: Arc::new(HeuristicVerificationGate::default()),
        reasoner: Arc::new(MockReasoner),
        deep_thinker: Arc::new(MockDeepThinker),
        escalation_router: Arc::new(DefaultEscalationRouter::new(store.clone())),
        failure_store: store,
    };
    let pipeline = OrchestrationPipeline::new(components, 0.50).with_dry_run();
    let metrics = pipeline.run("dry run task").await.unwrap();

    assert!(metrics.steps_succeeded > 0);
    assert_eq!(metrics.total_cost(), 0.0, "Dry run should cost nothing");
}
```

Run: `cargo test -p rustycode-orchestration --test pipeline_integration_test`
Expected: All tests pass including new dry-run test.

- [ ] **Step 5: Export from lib.rs**

Add `pub mod pipeline;`.

- [ ] **Step 6: Run test**

Run: `cargo test -p rustycode-orchestration --test pipeline_integration_test`

Expected: PASS.

- [ ] **Step 7: Run full test suite**

Run: `cargo test -p rustycode-orchestration`

Expected: All tests pass.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -p rustycode-orchestration --all-targets -- -D warnings`

Expected: Clean, no warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add OrchestrationPipeline with dry-run mode wiring all components"
```

---

### Task 17: LLM-Backed Implementations (Decomposer, Refiner, Reasoner)

**Files:**
- Modify: `crates/rustycode-orchestration/src/decomposer.rs`
- Modify: `crates/rustycode-orchestration/src/plan_refiner.rs`
- Modify: `crates/rustycode-orchestration/src/reasoner.rs`

These tasks wire up actual LLM calls to `rustycode-llm::provider::LLMProvider`. Each implementation follows the same pattern: build a prompt, call the provider, parse JSON output.

- [ ] **Step 1: Research the provider call pattern**

Read `crates/rustycode-llm/src/provider.rs` to understand how to construct `CompletionRequest` for a non-streaming call. Read `crates/rustycode-tui/src/app/streaming/response.rs:300-400` for an example.

- [ ] **Step 2: Write integration test with a stub provider**

Create `crates/rustycode-orchestration/tests/llm_integration_test.rs`:

```rust
// Uses a stub LLMProvider to verify LlmDecomposer produces valid DecomposedTask
// from a JSON response. Real API calls live in the e2e test phase.

use rustycode_orchestration::decomposer::{DecompositionContext, LlmDecomposer, TaskDecomposer};
// TODO: Add stub LLMProvider once rustycode-llm supports trait mocking.
// Until then this test file remains marked #[ignore].

#[tokio::test]
#[ignore = "awaiting stub LLMProvider for unit testing"]
async fn test_llm_decomposer_parses_json_response() {
    // Placeholder: wire up after rustycode-llm exposes a mock provider.
}
```

- [ ] **Step 3: Implement LlmDecomposer JSON parsing**

In `decomposer.rs`, replace the `bail!` stub with a real implementation:

```rust
#[async_trait]
impl TaskDecomposer for LlmDecomposer {
    async fn decompose(
        &self,
        task: &str,
        context: &DecompositionContext,
    ) -> anyhow::Result<DecomposedTask> {
        let prompt = Self::build_prompt(task, context);
        let request = rustycode_llm::provider::CompletionRequest {
            model: self.model_name.clone(),
            messages: vec![rustycode_llm::provider::ChatMessage::user(prompt)],
            max_tokens: Some(2048),
            temperature: Some(0.2),
            ..Default::default()
        };
        let response = self.provider.complete(request).await?;
        let text = response.text.ok_or_else(|| anyhow::anyhow!("empty response"))?;

        // Extract JSON (models sometimes wrap with ```json ... ```)
        let json_str = extract_json_block(&text);
        let partial: PartialDecomposed = serde_json::from_str(&json_str)?;

        // Fill in UUIDs if missing
        let steps: Vec<Step> = partial.steps.into_iter().map(|s| Step {
            id: if s.id.is_empty() { uuid::Uuid::new_v4().to_string() } else { s.id },
            index: s.index,
            description: s.description,
            expected_output_type: s.expected_output_type,
            suggested_tool: s.suggested_tool,
            retry_on_failure: s.retry_on_failure,
        }).collect();

        Ok(DecomposedTask {
            original_task: task.to_string(),
            task_category: partial.task_category,
            estimated_difficulty: partial.estimated_difficulty,
            steps,
        })
    }
}

#[derive(Deserialize)]
struct PartialDecomposed {
    task_category: String,
    estimated_difficulty: crate::types::Difficulty,
    steps: Vec<PartialStep>,
}

#[derive(Deserialize)]
struct PartialStep {
    #[serde(default)]
    id: String,
    index: u8,
    description: String,
    expected_output_type: crate::types::OutputType,
    #[serde(default)]
    suggested_tool: Option<String>,
    #[serde(default = "default_retry")]
    retry_on_failure: bool,
}

fn default_retry() -> bool { true }

fn extract_json_block(text: &str) -> String {
    // Strip ```json ... ``` fences if present
    if let Some(start) = text.find("```json") {
        if let Some(end_rel) = text[start+7..].find("```") {
            return text[start+7..start+7+end_rel].trim().to_string();
        }
    }
    if let Some(start) = text.find("```") {
        if let Some(end_rel) = text[start+3..].find("```") {
            return text[start+3..start+3+end_rel].trim().to_string();
        }
    }
    text.trim().to_string()
}
```

Update `use` imports at the top of decomposer.rs:

```rust
use serde::{Deserialize, Serialize};
```

- [ ] **Step 4: Implement LlmPlanRefiner**

Append to `plan_refiner.rs`:

```rust
use std::sync::Arc;
use rustycode_llm::provider::LLMProvider;

pub struct LlmPlanRefiner {
    provider: Arc<dyn LLMProvider>,
    model_name: String,
}

impl LlmPlanRefiner {
    pub fn new(provider: Arc<dyn LLMProvider>, model_name: String) -> Self {
        Self { provider, model_name }
    }
}

#[async_trait]
impl PlanRefiner for LlmPlanRefiner {
    async fn refine(
        &self,
        plan: &DecomposedTask,
        patterns: &[StoredPattern],
        trace: Option<&ExecutionTrace>,
    ) -> anyhow::Result<RefinementResult> {
        let prompt = format!(
            r#"You are a Plan Refiner. Review this decomposed plan against historical failure patterns.

Plan:
{plan_json}

Historical patterns for this task type ({count} found):
{patterns_summary}

{trace_section}

Respond ONLY with valid JSON:
- {{"decision": "approve"}} if the plan looks safe
- {{"decision": "modify", "reasoning": "...", "updated_steps": [...]}} if you want to change steps
- {{"decision": "reject", "reason": "..."}} if the plan cannot succeed
"#,
            plan_json = serde_json::to_string_pretty(plan)?,
            count = patterns.len(),
            patterns_summary = patterns.iter()
                .map(|p| format!("- step {}: {:?} ({}x)", p.step_index, p.error_category, p.occurrence_count))
                .collect::<Vec<_>>().join("\n"),
            trace_section = trace.map(|t| format!("Prior ExecutionTrace: {} steps", t.steps.len())).unwrap_or_default(),
        );
        let request = rustycode_llm::provider::CompletionRequest {
            model: self.model_name.clone(),
            messages: vec![rustycode_llm::provider::ChatMessage::user(prompt)],
            max_tokens: Some(1024),
            temperature: Some(0.3),
            ..Default::default()
        };
        let response = self.provider.complete(request).await?;
        let text = response.text.unwrap_or_default();
        // Simple heuristic: if "approve" mentioned and "reject" isn't, approve
        if text.contains("\"approve\"") {
            return Ok(RefinementResult::Approve);
        }
        if text.contains("\"reject\"") {
            return Ok(RefinementResult::Reject {
                reason: text.clone(),
                suggested_alternative: None,
            });
        }
        // Default: approve (safe fallback)
        Ok(RefinementResult::Approve)
    }
}
```

- [ ] **Step 5: Implement LlmReasoner**

Append to `reasoner.rs`:

```rust
use std::sync::Arc;
use rustycode_llm::provider::LLMProvider;

pub struct LlmReasoner {
    provider: Arc<dyn LLMProvider>,
    model_name: String,
}

impl LlmReasoner {
    pub fn new(provider: Arc<dyn LLMProvider>, model_name: String) -> Self {
        Self { provider, model_name }
    }
}

#[async_trait]
impl Reasoner for LlmReasoner {
    async fn replan(
        &self,
        trace: &ExecutionTrace,
        failed_step: &Step,
        error: &ErrorSignal,
        patterns: &[StoredPattern],
    ) -> anyhow::Result<ReplanResult> {
        let prompt = format!(
            r#"You are a medium-reasoning task planner. A step just failed:

Failed step: {step_desc}
Error: {error_cat:?} — {error_msg}

Prior execution trace ({trace_len} steps):
{trace_summary}

Historical patterns ({patterns_count}):
{patterns_summary}

Analyze the failure and propose a revised step. Show your reasoning, then rate
your reasoning quality (1-5).

Respond ONLY with valid JSON:
{{
  "updated_step": {{ ...Step fields... }},
  "reasoning": "<why this fix should work>",
  "confidence": 0.0-1.0,
  "reasoning_quality_score": 1-5
}}
"#,
            step_desc = failed_step.description,
            error_cat = error.category,
            error_msg = error.message,
            trace_len = trace.steps.len(),
            trace_summary = trace.steps.iter().rev().take(3)
                .map(|e| format!("  - {} -> exit {:?}", e.tool_name, e.exit_code))
                .collect::<Vec<_>>().join("\n"),
            patterns_count = patterns.len(),
            patterns_summary = patterns.iter().take(3)
                .map(|p| format!("  - {:?} at step {} ({}x)", p.error_category, p.step_index, p.occurrence_count))
                .collect::<Vec<_>>().join("\n"),
        );
        let request = rustycode_llm::provider::CompletionRequest {
            model: self.model_name.clone(),
            messages: vec![rustycode_llm::provider::ChatMessage::user(prompt)],
            max_tokens: Some(1024),
            temperature: Some(0.4),
            ..Default::default()
        };
        let response = self.provider.complete(request).await?;
        let text = response.text.unwrap_or_default();
        // Simple fallback: if we can't parse JSON, reuse failed_step
        let result: ReplanResult = serde_json::from_str(&crate::decomposer::extract_json_block(&text))
            .unwrap_or(ReplanResult {
                updated_step: failed_step.clone(),
                reasoning: text,
                confidence: 0.5,
                reasoning_quality_score: 3,
            });
        Ok(result)
    }
}
```

Make `extract_json_block` `pub(crate)` in decomposer.rs.

- [ ] **Step 6: Run build**

Run: `cargo build -p rustycode-orchestration`

Expected: Clean build.

- [ ] **Step 7: Run all tests**

Run: `cargo test -p rustycode-orchestration`

Expected: All tests pass.

- [ ] **Step 8: Run clippy**

Run: `cargo clippy -p rustycode-orchestration --all-targets -- -D warnings`

Expected: No warnings.

- [ ] **Step 9: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): wire LlmDecomposer, LlmPlanRefiner, LlmReasoner to provider"
```

---

### Task 18: Tool Execution Orchestrator (Tier 2 Real Implementation)

**Files:**
- Modify: `crates/rustycode-orchestration/src/orchestrator.rs`

Wires up the real Tier 2 orchestrator that calls the LLM and dispatches tool calls to `rustycode-tools::ToolRegistry`.

- [ ] **Step 1: Research tool registry API**

Read `crates/rustycode-tools/src/lib.rs` and `src/executor.rs` to understand `ToolRegistry::execute()` signature.

- [ ] **Step 2: Append LlmOrchestrator to orchestrator.rs**

```rust
use std::sync::Arc;
use std::time::Instant;
use rustycode_llm::provider::LLMProvider;
use rustycode_tools::ToolRegistry;

use crate::error_signal::ErrorClassifier;

pub struct LlmOrchestrator {
    provider: Arc<dyn LLMProvider>,
    model_name: String,
    tool_registry: Arc<ToolRegistry>,
    classifier: ErrorClassifier,
}

impl LlmOrchestrator {
    pub fn new(
        provider: Arc<dyn LLMProvider>,
        model_name: String,
        tool_registry: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            provider,
            model_name,
            tool_registry,
            classifier: ErrorClassifier::default(),
        }
    }

    fn build_prompt(step: &Step, context: &OrchestrationContext) -> String {
        let trace_summary = match &context.prior_trace {
            Some(t) if !t.steps.is_empty() => format!(
                "\n\nPrior step outputs (most recent 3):\n{}",
                t.steps.iter().rev().take(3)
                    .map(|e| format!("  {} -> {}", e.tool_name, e.output.lines().take(2).collect::<Vec<_>>().join(" / ")))
                    .collect::<Vec<_>>().join("\n"),
            ),
            _ => String::new(),
        };
        format!(
            r#"You are a task executor. Execute this step exactly.

Step: {desc}
Expected output type: {out_type:?}
Suggested tool: {tool:?}
{trace_summary}

Output ONLY a JSON tool call matching this schema:
{{"tool": "bash|read_file|write_file|grep", "args": {{ ... }}}}
No explanation."#,
            desc = step.description,
            out_type = step.expected_output_type,
            tool = step.suggested_tool,
        )
    }
}

#[async_trait]
impl Orchestrator for LlmOrchestrator {
    async fn execute_step(
        &self,
        step: &Step,
        context: &OrchestrationContext,
    ) -> anyhow::Result<StepResult> {
        let start = Instant::now();
        let prompt = Self::build_prompt(step, context);
        let request = rustycode_llm::provider::CompletionRequest {
            model: self.model_name.clone(),
            messages: vec![rustycode_llm::provider::ChatMessage::user(prompt)],
            max_tokens: Some(512),
            temperature: Some(0.2),
            ..Default::default()
        };
        let response = self.provider.complete(request).await?;
        let cost_usd = response.cost_usd.unwrap_or(0.0);
        let text = response.text.unwrap_or_default();
        let json_str = crate::decomposer::extract_json_block(&text);

        #[derive(serde::Deserialize)]
        struct ToolCall {
            tool: String,
            args: serde_json::Value,
        }
        let call: ToolCall = serde_json::from_str(&json_str)?;

        // Execute via tool registry (simplified; real impl would use ToolContext)
        let cwd = std::env::current_dir()?;
        let ctx = rustycode_tools::ToolContext::new(cwd);
        let tool_output = match self.tool_registry.get(&call.tool) {
            Some(tool) => match tool.execute(call.args.clone(), &ctx) {
                Ok(out) => Ok(out),
                Err(e) => Err(e.to_string()),
            },
            None => Err(format!("Unknown tool: {}", call.tool)),
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        match tool_output {
            Ok(output) => Ok(StepResult {
                step_id: step.id.clone(),
                success: true,
                tool_name: call.tool,
                tool_args: call.args,
                tool_output: Some(output.text),
                exit_code: Some(0),
                error_signal: None,
                duration_ms,
                cost_usd,
            }),
            Err(msg) => {
                let category = self.classifier.classify(&msg, 1);
                let signal = ErrorSignal::new(
                    category, Some(1), msg.clone(),
                    step.id.clone(), call.tool.clone(),
                );
                Ok(StepResult {
                    step_id: step.id.clone(),
                    success: false,
                    tool_name: call.tool,
                    tool_args: call.args,
                    tool_output: Some(msg),
                    exit_code: Some(1),
                    error_signal: Some(signal),
                    duration_ms,
                    cost_usd,
                })
            }
        }
    }
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p rustycode-orchestration`

Expected: Clean build. If `ToolContext::new()` or `ToolRegistry::get()` signatures differ, adjust per actual API.

- [ ] **Step 4: Run existing tests**

Run: `cargo test -p rustycode-orchestration`

Expected: All existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add LlmOrchestrator wiring tool registry and LLM provider"
```

---

## Phase C-2: Testing & Production Readiness (Tasks 19-22)

### Task 19: End-to-End Mock Test

**Files:**
- Modify: `crates/rustycode-orchestration/tests/pipeline_integration_test.rs`

Extend the integration test with scenarios covering escalation paths.

- [ ] **Step 1: Add escalation test**

Append to `pipeline_integration_test.rs`:

```rust
// Mock orchestrator: always fails with CompileError
struct MockOrchestratorAlwaysFails;
#[async_trait]
impl Orchestrator for MockOrchestratorAlwaysFails {
    async fn execute_step(&self, step: &Step, _ctx: &OrchestrationContext) -> anyhow::Result<StepResult> {
        let err = ErrorSignal::new(
            ErrorCategory::CompileError, Some(101),
            "error[E0599]: no method".into(),
            step.id.clone(), "bash".into(),
        );
        Ok(StepResult {
            step_id: step.id.clone(), success: false,
            tool_name: "bash".into(), tool_args: serde_json::json!({}),
            tool_output: Some("error".into()), exit_code: Some(101),
            error_signal: Some(err), duration_ms: 10, cost_usd: 0.01,
        })
    }
}

#[tokio::test]
async fn test_escalation_through_all_tiers_abandons_at_tier4() {
    let store = Arc::new(MemoryFailureStore::new());
    let components = PipelineComponents {
        decomposer: Arc::new(MockDecomposer),
        plan_refiner: Arc::new(ApproveAllRefiner),
        orchestrator: Arc::new(MockOrchestratorAlwaysFails),
        verification_gate: Arc::new(HeuristicVerificationGate::default()),
        reasoner: Arc::new(MockReasoner),
        deep_thinker: Arc::new(MockDeepThinker),
        escalation_router: Arc::new(DefaultEscalationRouter::new(store.clone())),
        failure_store: store,
    };
    let pipeline = OrchestrationPipeline::new(components, 0.50);
    let metrics = pipeline.run("failing task").await.unwrap();

    assert!(matches!(metrics.final_outcome, Some(TaskOutcome::Abandoned { .. })));
    assert!(!metrics.escalation_reasons.is_empty());
}
```

- [ ] **Step 2: Run test**

Run: `cargo test -p rustycode-orchestration --test pipeline_integration_test`

Expected: Both tests pass (happy path + escalation).

- [ ] **Step 3: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "test(orchestration): add escalation-to-abandon integration test"
```

---

### Task 20: Terminal-Bench Task Runner (E2E harness)

**Files:**
- Create: `crates/rustycode-orchestration/examples/run_terminal_bench_task.rs`

A runnable example that loads a single terminal-bench task description and runs it through the pipeline with a real LLM provider. Marked as example so it doesn't run in CI.

- [ ] **Step 1: Create example file**

Create `crates/rustycode-orchestration/examples/run_terminal_bench_task.rs`:

```rust
//! Runs a single terminal-bench task through the orchestration pipeline.
//!
//! Usage:
//!   cargo run -p rustycode-orchestration --example run_terminal_bench_task -- \
//!     "Install R and run t-test on data.csv"

use std::sync::Arc;

use rustycode_orchestration::decomposer::LlmDecomposer;
use rustycode_orchestration::deep_thinker_adapter::RustyCodeDeepThinker;
use rustycode_orchestration::escalation_router::DefaultEscalationRouter;
use rustycode_orchestration::failure_store::{MemoryFailureStore, SqliteFailureStore};
use rustycode_orchestration::orchestrator::LlmOrchestrator;
use rustycode_orchestration::pipeline::{OrchestrationPipeline, PipelineComponents};
use rustycode_orchestration::plan_refiner::ApproveAllRefiner;
use rustycode_orchestration::reasoner::LlmReasoner;
use rustycode_orchestration::verification_gate::HeuristicVerificationGate;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let task = std::env::args().nth(1).unwrap_or_else(|| {
        "Install R and compute a t-test on data.csv".to_string()
    });

    // Load LLM provider from env (API keys via env vars)
    let (_provider_type, model, cfg) = rustycode_llm::load_provider_config_from_env()?;
    let provider = rustycode_llm::provider::build_provider(&cfg)?;
    let provider = Arc::from(provider);

    // Use in-memory failure store for this example
    let store = Arc::new(MemoryFailureStore::new());

    // Use basic ToolRegistry
    let tool_registry = Arc::new(rustycode_tools::ToolRegistry::default_registry());

    let components = PipelineComponents {
        decomposer: Arc::new(LlmDecomposer::new(provider.clone(), model.clone())),
        plan_refiner: Arc::new(ApproveAllRefiner),
        orchestrator: Arc::new(LlmOrchestrator::new(provider.clone(), model.clone(), tool_registry)),
        verification_gate: Arc::new(HeuristicVerificationGate::default()),
        reasoner: Arc::new(LlmReasoner::new(provider.clone(), model.clone())),
        deep_thinker: Arc::new(RustyCodeDeepThinker::new()),
        escalation_router: Arc::new(DefaultEscalationRouter::new(store.clone())),
        failure_store: store,
    };

    let pipeline = OrchestrationPipeline::new(components, 0.50);
    let metrics = pipeline.run(&task).await?;

    println!("\n=== Orchestration Metrics ===");
    println!("Outcome: {:?}", metrics.final_outcome);
    println!("Duration: {} ms", metrics.total_duration_ms);
    println!("Total cost: ${:.4}", metrics.total_cost());
    println!("Steps succeeded: {}", metrics.steps_succeeded);
    println!("Steps failed: {}", metrics.steps_failed);
    println!("Escalations: {:?}", metrics.escalation_reasons);

    Ok(())
}
```

- [ ] **Step 2: Build the example**

Run: `cargo build -p rustycode-orchestration --example run_terminal_bench_task`

Expected: Build succeeds. May need minor adjustments based on actual `rustycode_llm::load_provider_config_from_env()` signature and `ToolRegistry::default_registry()`.

- [ ] **Step 3: Run with a real task (manual)**

Run: `cargo run -p rustycode-orchestration --example run_terminal_bench_task -- "list files in current directory"`

Expected: Non-zero exit OK if no API key configured. With a key, should produce metrics output.

- [ ] **Step 4: Add README example section**

Modify `crates/rustycode-orchestration/README.md` to add:

```markdown
## Examples

Run a single task through the pipeline:

```bash
cargo run -p rustycode-orchestration --example run_terminal_bench_task -- "your task"
```

Requires a provider-configured environment (see rustycode-llm docs).
```

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestration/
git commit -m "feat(orchestration): add runnable terminal-bench example"
```

---

### Task 21: Wire Orchestration into TUI Deep Thinking Service

**Files:**
- Modify: `crates/rustycode-tui/src/services/deep_thinking.rs`

Replace the current `<system-reminder>` prompt-injection path with an optional orchestration pipeline invocation gated by a config flag.

- [ ] **Step 1: Read existing deep_thinking.rs to understand integration points**

Read `crates/rustycode-tui/src/services/deep_thinking.rs` fully.

- [ ] **Step 2: Add orchestration-enabled path**

This is a careful surgical change. Add a new public function (do NOT remove the existing one):

```rust
/// Analyzes a user message and, if orchestration is enabled, routes through
/// the tiered orchestration pipeline instead of the prompt-injection path.
/// Falls back to `analyze_and_transform` if orchestration is disabled or fails.
pub async fn analyze_and_transform_with_orchestration(
    content: &str,
    orchestration_enabled: bool,
) -> DeepThinkingResult {
    if !orchestration_enabled {
        return analyze_and_transform(content);
    }
    // TODO: load OrchestrationConfig, build PipelineComponents, run pipeline.
    // For this task we only add the entry point; wiring happens in a follow-up.
    analyze_and_transform(content)
}
```

- [ ] **Step 3: Run TUI tests**

Run: `cargo test -p rustycode-tui`

Expected: Existing tests still pass.

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-tui/
git commit -m "feat(tui): add orchestration-aware entry point to deep_thinking service"
```

---

### Task 22: Documentation & Final Polish

**Files:**
- Modify: `crates/rustycode-orchestration/README.md`
- Create: `docs/architecture/tiered-orchestration.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Expand README with architecture summary**

Expand `crates/rustycode-orchestration/README.md` with:
- Architecture diagram (copy from spec)
- Usage example
- Config reference
- Integration points

- [ ] **Step 2: Create docs/architecture/tiered-orchestration.md**

Write `docs/architecture/tiered-orchestration.md` with:
- Summary of the problem being solved
- Component relationships diagram
- How to extend with new tiers / error categories / verification rules
- Performance notes
- Link to spec and plan

- [ ] **Step 3: Update CLAUDE.md**

Modify `/Users/nat/dev/rustycode/CLAUDE.md` — add `rustycode-orchestration` to the workspace crate list in the "Repository Structure" section.

- [ ] **Step 4: Run final full build**

Run: `cargo build --workspace --all-targets`

Expected: Clean workspace build.

- [ ] **Step 5: Run full test suite**

Run: `cargo test --workspace`

Expected: All tests pass.

- [ ] **Step 6: Run workspace clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/rustycode-orchestration/README.md docs/architecture/tiered-orchestration.md CLAUDE.md
git commit -m "docs(orchestration): add architecture doc and update README/CLAUDE.md"
```

---

## Summary

**Total Tasks: 25** (22 original + 3 migration)

- **Phase A (Foundation)**: Tasks 1-8 — types, state machine, error handling, failure store (with seed patterns), config, escalation router, model registry
- **Phase B (Components)**: Tasks 9-14 — decomposer, plan refiner, orchestrator, verification gate, reasoner, deep thinker adapter
- **Phase C (Integration)**: Tasks 15-18 — metrics, pipeline (with dry-run mode), LLM-backed impls, tool execution orchestrator
- **Phase C-2 (Production)**: Tasks 19-22 — E2E tests, terminal-bench runner, TUI integration, docs
- **Phase D (Migration)**: Tasks 23-25 — shadow mode, incremental cutover, legacy deprecation

Each task is TDD-driven (write failing test → implement → pass → commit) with explicit file paths, code, commands, and expected outcomes.

**Estimated timeline**: 6-8 weeks for a skilled developer new to the codebase; 3-4 weeks for a contributor familiar with `rustycode-llm` and `rustycode-tools`.

---

## Phase D: Transition & Migration Plan

> **Note:** Execute Phase D after Phases A-C are fully tested and verified.

### Task 23: Shadow Mode

- [ ] **Step 1: Run orchestration pipeline alongside existing execution**
  - For a subset of terminal-bench tasks, run both legacy and new pipeline
  - Compare `ExecutionTrace` outputs against legacy logs
  - Log comparison metrics to `OrchestrationMetrics`

### Task 24: Incremental Cutover

- [ ] **Step 1: Route "Easy" task categories to OrchestrationPipeline**
  - Transition easy categories (e.g., `data_etl`) to the new pipeline
  - Keep "Complex" tasks (e.g., `scientific_computing`) on legacy until PlanRefiner performance is verified

### Task 25: Legacy Deprecation

- [ ] **Step 1: Route all single-task requests through OrchestrationPipeline**
  - Monitor `OrchestrationMetrics` to ensure parity or improvement
  - Remove legacy orchestration code paths
  - Archive legacy test suites once E2E coverage is confirmed

**Next steps after plan approval**: Execute tasks sequentially (Task 1 → Task 2 → ...) — Phase A must complete before Phase B; Phase C requires Phase B; Phase D assumes Phase C is working.
