# Orchestration Integration Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the integration layer that routes all CLI/TUI task execution through orchestration by default, with a local task classifier to route "mundane" tasks to the fast path and "complex" tasks through the full tiered orchestration system.

**Architecture:** New `rustycode-classifier` module that:
1. Classifies task complexity using local heuristics (length, keywords, patterns)
2. Routes mundane tasks (reads, lists, simple operations) directly to the current execution path (fast, no overhead)
3. Routes complex tasks through orchestration pipeline (decomposition, escalation, learning)
4. Both paths instrumented for shadow mode metrics collection
5. System prompt guidance allows LLM in each tier to self-decide escalation vs. direct handling

**Tech Stack:** Rust 2021, tokio, regex, existing `rustycode-cli`, `rustycode-tui`, `rustycode-orchestration`

---

## File Structure

### New Files
- `crates/rustycode-classification/src/lib.rs` — Module exports
- `crates/rustycode-classification/src/classifier.rs` — LocalTaskClassifier
- `crates/rustycode-classification/src/types.rs` — TaskComplexity enum
- `crates/rustycode-classification/tests/classifier_test.rs` — Unit tests

### Modified Files
- `crates/rustycode-cli/src/main.rs` — Wire orchestration execution
- `crates/rustycode-cli/src/executor.rs` (or equivalent) — Add orchestration path
- `crates/rustycode-tui/src/services/execution.rs` (or equivalent) — Add orchestration path
- `Cargo.toml` (workspace) — Add `rustycode-classification` crate

---

## Phase 1: Task Classifier (Tasks 1-3)

### Task 1: Create Classification Crate Skeleton

**Files:**
- Create: `crates/rustycode-classification/Cargo.toml`
- Create: `crates/rustycode-classification/src/lib.rs`
- Create: `crates/rustycode-classification/README.md`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "rustycode-classification"
version.workspace = true
edition.workspace = true
license = "MIT"
description = "Task complexity classification for orchestration routing"

[dependencies]
anyhow.workspace = true
regex.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
rustycode-orchestration = { path = "../rustycode-orchestration" }

[dev-dependencies]
tokio = { workspace = true, features = ["macros"] }

[lints]
workspace = true
```

- [ ] **Step 2: Create lib.rs**

```rust
//! Task complexity classification for routing through orchestration.
//!
//! Provides LocalTaskClassifier to determine if a task should:
//! - Run through fast path (mundane: reads, lists, simple operations)
//! - Run through full orchestration (complex: multi-step, reasoning-heavy)

pub mod classifier;
pub mod types;

pub use classifier::{LocalTaskClassifier, ClassificationResult};
pub use types::{TaskComplexity, ClassificationReason};
```

- [ ] **Step 3: Create README.md**

```markdown
# rustycode-classification

Local task complexity classifier for orchestration routing.

## Purpose

Classify incoming tasks as "mundane" or "complex" to route:
- **Mundane**: Direct execution (fast path, no thinking overhead)
- **Complex**: Full orchestration pipeline (decomposition, escalation, learning)

## Examples

```rust
let classifier = LocalTaskClassifier::new();
let result = classifier.classify("list files in /tmp");
assert_eq!(result.complexity, TaskComplexity::Mundane);

let result = classifier.classify("refactor this Rust module to use async/await");
assert_eq!(result.complexity, TaskComplexity::Complex);
```
```

- [ ] **Step 4: Add to workspace members**

Modify root `Cargo.toml`:

```toml
members = [
    # ... existing
    "crates/rustycode-classification",
]
```

- [ ] **Step 5: Verify build**

```bash
cargo build -p rustycode-classification
```

Expected: Clean build.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/rustycode-classification/
git commit -m "feat(classification): scaffold rustycode-classification crate"
```

---

### Task 2: Implement LocalTaskClassifier

**Files:**
- Create: `crates/rustycode-classification/src/types.rs`
- Create: `crates/rustycode-classification/src/classifier.rs`
- Create: `crates/rustycode-classification/tests/classifier_test.rs`

- [ ] **Step 1: Write types.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskComplexity {
    Mundane,  // Simple, single-purpose tasks
    Complex,  // Multi-step, reasoning-heavy tasks
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassificationReason {
    KeywordMatch(String),           // e.g., "found 'list' keyword"
    TaskLengthShort,                // Task description < 100 chars
    TaskLengthLong,                 // Task description > 500 chars
    HistoricalPattern,              // Similar tasks in FailurePatternStore
    MultipleTools,                  // Task requires multiple tools
    ReasoningKeywords,              // Words like "refactor", "debug", "optimize"
    Unknown,
}

pub struct ClassificationResult {
    pub complexity: TaskComplexity,
    pub confidence: f64,            // 0.0 to 1.0
    pub reasons: Vec<ClassificationReason>,
}
```

- [ ] **Step 2: Write failing test**

```rust
// tests/classifier_test.rs
use rustycode_classification::{LocalTaskClassifier, TaskComplexity};

#[test]
fn test_simple_list_is_mundane() {
    let classifier = LocalTaskClassifier::new();
    let result = classifier.classify("list files in /tmp");
    assert_eq!(result.complexity, TaskComplexity::Mundane);
}

#[test]
fn test_refactoring_is_complex() {
    let classifier = LocalTaskClassifier::new();
    let result = classifier.classify("refactor this Rust module to use async/await patterns");
    assert_eq!(result.complexity, TaskComplexity::Complex);
}

#[test]
fn test_show_command_is_mundane() {
    let classifier = LocalTaskClassifier::new();
    let result = classifier.classify("show the contents of config.yaml");
    assert_eq!(result.complexity, TaskComplexity::Mundane);
}

#[test]
fn test_debug_is_complex() {
    let classifier = LocalTaskClassifier::new();
    let result = classifier.classify("debug why the database migration is failing");
    assert_eq!(result.complexity, TaskComplexity::Complex);
}
```

- [ ] **Step 3: Run test to verify it fails**

```bash
cargo test -p rustycode-classification --test classifier_test
```

Expected: FAIL (classifier doesn't exist yet).

- [ ] **Step 4: Implement LocalTaskClassifier**

```rust
// src/classifier.rs
use regex::Regex;
use crate::types::{TaskComplexity, ClassificationResult, ClassificationReason};

pub struct LocalTaskClassifier {
    mundane_keywords: Vec<Regex>,
    complex_keywords: Vec<Regex>,
}

impl LocalTaskClassifier {
    pub fn new() -> Self {
        let mundane_keywords = vec![
            Regex::new(r"(?i)\blist\b").unwrap(),
            Regex::new(r"(?i)\bshow\b").unwrap(),
            Regex::new(r"(?i)\bread\b").unwrap(),
            Regex::new(r"(?i)\bcat\b").unwrap(),
            Regex::new(r"(?i)\bcount\b").unwrap(),
            Regex::new(r"(?i)\bgrep\b").unwrap(),
            Regex::new(r"(?i)\bfind\b").unwrap(),
            Regex::new(r"(?i)\bcheck\b").unwrap(),
            Regex::new(r"(?i)\bverify\b").unwrap(),
        ];
        
        let complex_keywords = vec![
            Regex::new(r"(?i)\brefactor\b").unwrap(),
            Regex::new(r"(?i)\bdebug\b").unwrap(),
            Regex::new(r"(?i)\boptimize\b").unwrap(),
            Regex::new(r"(?i)\bdesign\b").unwrap(),
            Regex::new(r"(?i)\barchtect(ure)?\b").unwrap(),
            Regex::new(r"(?i)\bimplement\b").unwrap(),
            Regex::new(r"(?i)\banalyze\b").unwrap(),
            Regex::new(r"(?i)\bfix\b").unwrap(),
            Regex::new(r"(?i)\brewrite\b").unwrap(),
        ];
        
        Self { mundane_keywords, complex_keywords }
    }
    
    pub fn classify(&self, task: &str) -> ClassificationResult {
        let mut reasons = Vec::new();
        let mut complexity = TaskComplexity::Mundane;
        let mut confidence = 0.5;
        
        // Check task length
        if task.len() < 100 {
            reasons.push(ClassificationReason::TaskLengthShort);
            confidence += 0.15;
        } else if task.len() > 500 {
            reasons.push(ClassificationReason::TaskLengthLong);
            complexity = TaskComplexity::Complex;
            confidence += 0.20;
        }
        
        // Check for mundane keywords
        for pattern in &self.mundane_keywords {
            if pattern.is_match(task) {
                reasons.push(ClassificationReason::KeywordMatch(pattern.as_str().to_string()));
                confidence = (confidence + 0.25).min(1.0);
                break;  // One match is enough
            }
        }
        
        // Check for complex keywords
        for pattern in &self.complex_keywords {
            if pattern.is_match(task) {
                reasons.push(ClassificationReason::KeywordMatch(pattern.as_str().to_string()));
                complexity = TaskComplexity::Complex;
                confidence = (confidence + 0.30).min(1.0);
                break;  // One match is enough
            }
        }
        
        // Default: if no strong signals, use length heuristic
        if reasons.is_empty() {
            if task.len() > 300 {
                complexity = TaskComplexity::Complex;
                confidence = 0.6;
            } else {
                complexity = TaskComplexity::Mundane;
                confidence = 0.5;
            }
            reasons.push(ClassificationReason::Unknown);
        }
        
        ClassificationResult { complexity, confidence: confidence.min(1.0), reasons }
    }
}

impl Default for LocalTaskClassifier {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Run test**

```bash
cargo test -p rustycode-classification --test classifier_test
```

Expected: All 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-classification/
git commit -m "feat(classifier): implement LocalTaskClassifier with keyword-based routing"
```

---

### Task 3: Add Historical Pattern Fallback

**Files:**
- Modify: `crates/rustycode-classification/src/classifier.rs`
- Modify: `tests/classifier_test.rs`

**Goal:** Classifier can check FailurePatternStore to see if similar tasks have been seen before as complex/mundane.

- [ ] **Step 1: Add failure_store reference to classifier**

```rust
use std::sync::Arc;
use rustycode_orchestration::failure_patterns::FailurePatternStore;

pub struct LocalTaskClassifier {
    mundane_keywords: Vec<Regex>,
    complex_keywords: Vec<Regex>,
    failure_store: Option<Arc<dyn FailurePatternStore>>,
}

impl LocalTaskClassifier {
    pub fn new() -> Self {
        Self {
            mundane_keywords: /* ... */,
            complex_keywords: /* ... */,
            failure_store: None,
        }
    }
    
    pub fn with_failure_store(mut self, store: Arc<dyn FailurePatternStore>) -> Self {
        self.failure_store = Some(store);
        self
    }
}
```

- [ ] **Step 2: Update classify() to check historical patterns**

```rust
pub fn classify(&self, task: &str) -> ClassificationResult {
    // ... existing keyword + length checks ...
    
    // Check historical patterns if available
    if let Some(ref store) = self.failure_store {
        // Rough task category detection (e.g., "rust_refactoring" from task description)
        if let Ok(patterns) = store.query_patterns("*") {
            // If similar tasks have failed often, mark as complex
            if patterns.len() > 5 {
                complexity = TaskComplexity::Complex;
                confidence += 0.15;
                reasons.push(ClassificationReason::HistoricalPattern);
            }
        }
    }
    
    ClassificationResult { complexity, confidence: confidence.min(1.0), reasons }
}
```

- [ ] **Step 3: Add test for historical pattern**

```rust
#[test]
fn test_classifier_with_failure_store() {
    // TODO: mock FailurePatternStore and verify historical check
}
```

- [ ] **Step 4: Verify build and tests**

```bash
cargo test -p rustycode-classification
```

Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-classification/
git commit -m "feat(classifier): add historical pattern fallback to classification"
```

---

## Phase 2: CLI/TUI Integration (Tasks 4-6)

### Task 4: Wire Orchestration into CLI Execution Path

**Files:**
- Modify: `crates/rustycode-cli/src/main.rs` or executor module
- Modify: `crates/rustycode-cli/Cargo.toml` (add orchestration deps)

**Goal:** Replace current task execution with orchestration-aware routing:
1. Classify task complexity
2. If mundane, run through current fast path
3. If complex, run through orchestration
4. Collect metrics for both

- [ ] **Step 1: Read current CLI execution code**

```bash
head -50 crates/rustycode-cli/src/main.rs
```

Find where tasks are currently executed.

- [ ] **Step 2: Add orchestration dependencies**

Modify `crates/rustycode-cli/Cargo.toml`:

```toml
rustycode-orchestration = { path = "../rustycode-orchestration" }
rustycode-classification = { path = "../rustycode-classification" }
```

- [ ] **Step 3: Add orchestration execution wrapper**

```rust
async fn execute_task_with_orchestration(task: &str) -> anyhow::Result<()> {
    let classifier = LocalTaskClassifier::new();
    let complexity = classifier.classify(task);
    
    tracing::info!("Task classified as: {:?}", complexity.complexity);
    
    match complexity.complexity {
        TaskComplexity::Mundane => {
            // Use fast path
            execute_task_fast(task).await
        }
        TaskComplexity::Complex => {
            // Use orchestration
            let config = OrchestrationConfig::from_env()?;
            let pipeline = build_orchestration_pipeline()?;
            let metrics = pipeline.run(task).await?;
            
            println!("Orchestration metrics: {:?}", metrics);
            Ok(())
        }
    }
}
```

- [ ] **Step 4: Update main CLI entry point**

Replace current `execute()` call with `execute_task_with_orchestration()`.

- [ ] **Step 5: Build and test**

```bash
cargo build -p rustycode-cli
```

Expected: Clean build.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-cli/
git commit -m "feat(cli): wire orchestration into task execution with complexity routing"
```

---

### Task 5: Wire Orchestration into TUI Execution Path

**Files:**
- Modify: `crates/rustycode-tui/src/services/` (execution service)
- Modify: `crates/rustycode-tui/Cargo.toml`

Same approach as Task 4, but for TUI. Classify before running, route accordingly.

- [ ] **Step 1-6: Follow Task 4 pattern**

For TUI: find current task execution handler, add classification + routing before calling it.

Add orchestration to TUI Cargo.toml, implement orchestration-aware executor, wire into TUI event loop.

---

### Task 6: Add System Prompt Guidance for Self-Routing

**Files:**
- Modify: `crates/rustycode-classification/src/prompter.rs` (new file)
- Modify: Task execution wrappers (Tasks 4-5)

**Goal:** Add guidance to LLM system prompts so models in each tier can self-decide escalation.

- [ ] **Step 1: Create system prompt templates**

```rust
pub fn tier_2_system_prompt() -> String {
    r#"You are a task executor. Execute the given step.

If the step is straightforward and within your capabilities, execute it directly.
If you encounter an error or uncertainty, signal for escalation to a more capable assistant.

Return your response in JSON format:
{
  "decision": "execute" | "escalate",
  "reasoning": "why you chose this decision",
  "next_action": "the command/action to run"
}"#.to_string()
}

pub fn tier_3_system_prompt() -> String {
    r#"You are a task planner with access to execution history.

Review the failed step and execution trace. If you can fix it with a simple modification,
do so. If the problem requires deeper reasoning or complete rethinking, escalate to Tier 4.

Return your response in JSON format:
{
  "decision": "replan" | "escalate",
  "updated_step": {...},
  "reasoning": "your analysis"
}"#.to_string()
}
```

- [ ] **Step 2: Inject prompts into execution**

Modify the executor to use tier-appropriate system prompts:

```rust
let system_prompt = match ctx.current_tier {
    2 => tier_2_system_prompt(),
    3 => tier_3_system_prompt(),
    4 => tier_4_system_prompt(),
    _ => "".to_string(),
};

let response = provider.complete(
    model,
    vec![
        // system_prompt as first message
        Message { role: "system", content: system_prompt },
        // task context...
    ],
    None,
).await?;
```

- [ ] **Step 3: Test self-routing logic**

Add test to verify that LLM responses include "decision" field and routing works.

- [ ] **Step 4-5: Verify and commit**

```bash
cargo test -p rustycode-cli
cargo test -p rustycode-tui
git add crates/rustycode-classification/
git commit -m "feat(routing): add system prompts for tier self-decision making"
```

---

## Phase 3: Shadow Mode & Metrics (Tasks 7-8)

### Task 7: Implement Shadow Mode Execution

**Files:**
- Modify: Task 4-6 executors to support dual execution
- Modify: Metrics collection

**Goal:** Run both orchestration AND fast path in parallel for complex tasks, collect metrics from both, compare results.

- [ ] **Step 1: Add shadow mode flag to config**

```rust
pub struct ExecutionConfig {
    pub shadow_mode: bool,  // Run both paths and compare
}
```

- [ ] **Step 2: Implement dual execution**

For complex tasks, when shadow_mode is enabled:

```rust
let (orch_result, fast_result) = tokio::join!(
    orchestration_path(task),
    fast_path(task),
);

// Compare results
let metrics = compare_outcomes(&orch_result, &fast_result);
metrics.log_to_db()?;
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(shadow): implement dual execution for orchestration vs fast path comparison"
```

---

### Task 8: Add Metrics Collection & Reporting

**Files:**
- Create: `crates/rustycode-classification/src/metrics.rs`
- Modify: Execution paths to log metrics

**Goal:** Collect solve rate, cost, latency, escalation frequency for comparison.

- [ ] **Step 1: Define metrics struct**

```rust
pub struct ExecutionMetrics {
    pub task_id: String,
    pub classification: TaskComplexity,
    pub execution_path: ExecutionPath,  // Orchestration vs Fast
    pub outcome: TaskOutcome,
    pub duration_ms: u64,
    pub cost_usd: f64,
    pub escalations: u8,
    pub timestamp: DateTime<Utc>,
}
```

- [ ] **Step 2: Log metrics after each execution**

```rust
let metrics = ExecutionMetrics {
    task_id: format!("task_{}", Uuid::new_v4()),
    classification: complexity.complexity,
    execution_path: ExecutionPath::Orchestration,
    outcome,
    duration_ms,
    cost_usd,
    escalations: trace.escalations_count(),
    timestamp: Utc::now(),
};
metrics.log_to_database()?;
```

- [ ] **Step 3: Implement metrics database writer**

Use SQLite or existing storage to persist metrics.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(metrics): add execution metrics collection for orchestration analysis"
```

---

## Summary

**Total Tasks: 8**

- **Phase 1 (Classifier)**: Tasks 1-3 — Crate scaffold, LocalTaskClassifier, historical patterns
- **Phase 2 (Integration)**: Tasks 4-6 — CLI wiring, TUI wiring, system prompt guidance
- **Phase 3 (Shadow Mode)**: Tasks 7-8 — Dual execution, metrics collection

**Estimated Time:** 3-4 days

**Deliverables:**
- ✅ LocalTaskClassifier with heuristic-based routing
- ✅ CLI/TUI wired to classification + orchestration
- ✅ System prompts for tier self-decision
- ✅ Shadow mode for metrics comparison
- ✅ Integration layer ready for production cutover

**Next Step:** Once complete, proceed to **Migration Path Plan** (cutover strategy + monitoring).
