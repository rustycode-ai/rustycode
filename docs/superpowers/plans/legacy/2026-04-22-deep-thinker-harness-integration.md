# Deep-Thinker & Harness Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Deep-Thinker into the harness execution layer so each harness can invoke structured reasoning when needed, with full context and config control.

**Architecture:** OrchestraExecutor accepts a TaskRoutingDecision (not just harness enum) and dispatches to harness implementations. Each harness handles its own orchestration pattern (retries, parallelism, sessions) and optionally calls DeepThinkerExecutor for structured reasoning sub-tasks. All harnesses return a structured ExecutionResult with conclusions, graphs, and next-workflow guidance.

**Tech Stack:** Tokio async, RustyCode protocol types, Deep-Thinker crate, Anyhow error handling

---

## File Structure

### New Files
- `crates/rustycode-orchestra/src/harnesses/mod.rs` - Harness module exports and types
- `crates/rustycode-orchestra/src/harnesses/direct.rs` - Direct (one-shot) harness
- `crates/rustycode-orchestra/src/harnesses/ultrawork.rs` - Ultrawork (retry/progress) harness
- `crates/rustycode-orchestra/src/harnesses/omo.rs` - Omo (parallel analysis) harness
- `crates/rustycode-orchestra/src/harnesses/sparv.rs` - Sparv (long-lived session) harness
- `crates/rustycode-protocol/src/executor_result.rs` - ExecutionResult type (shared cross-crate)
- `crates/rustycode-orchestra/tests/harness_integration_test.rs` - Integration tests

### Modified Files
- `crates/rustycode-orchestra/src/orchestra_executor.rs` - Update signature, implement dispatch
- `crates/rustycode-orchestra/src/service/orchestra_service.rs` - Pass full decision to executor
- `crates/rustycode-orchestra/src/lib.rs` - Export harness module
- `crates/rustycode-protocol/src/lib.rs` - Export ExecutionResult

---

## Tasks

### Task 1: Define ExecutionResult Type

**Files:**
- Create: `crates/rustycode-protocol/src/executor_result.rs`
- Modify: `crates/rustycode-protocol/src/lib.rs`

The ExecutionResult is the unified return type from any harness. It carries conclusions, reasoning artifacts, and next-workflow guidance.

- [ ] **Step 1: Write ExecutionResult test**

```rust
// crates/rustycode-protocol/tests/executor_result_test.rs
#[test]
fn test_execution_result_creation() {
    let result = ExecutionResult {
        harness_used: TaskHarness::Direct,
        conclusion: "Task completed successfully".to_string(),
        confidence: 0.85,
        execution_time_ms: 1500,
        verified_artifacts: vec!["artifact1".to_string()],
        reasoning_graph: None,
        metacognitive_actions: vec![],
        next_workflow: Some(TaskHarness::Ultrawork),
        error: None,
    };
    
    assert_eq!(result.harness_used, TaskHarness::Direct);
    assert_eq!(result.confidence, 0.85);
    assert!(result.error.is_none());
}

#[test]
fn test_execution_result_with_error() {
    let result = ExecutionResult {
        harness_used: TaskHarness::Omo,
        conclusion: "Partial completion".to_string(),
        confidence: 0.5,
        execution_time_ms: 3000,
        verified_artifacts: vec![],
        reasoning_graph: None,
        metacognitive_actions: vec![],
        next_workflow: None,
        error: Some("Analysis failed".to_string()),
    };
    
    assert!(result.error.is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p rustycode-protocol executor_result_creation -- --nocapture
```

Expected output: `error[E0433]: cannot find type 'ExecutionResult' in this scope`

- [ ] **Step 3: Implement ExecutionResult**

```rust
// crates/rustycode-protocol/src/executor_result.rs
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::task_routing::TaskHarness;

/// Result returned by any harness after execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Which harness executed this task
    pub harness_used: TaskHarness,
    
    /// Final conclusion or output
    pub conclusion: String,
    
    /// Confidence in the conclusion (0.0-1.0)
    pub confidence: f64,
    
    /// How long execution took (milliseconds)
    pub execution_time_ms: u64,
    
    /// Verified artifacts (files, decisions, code, etc.)
    pub verified_artifacts: Vec<String>,
    
    /// Full reasoning graph from Deep-Thinker (if used)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_graph: Option<String>,  // Serialized JSON for now
    
    /// Metacognitive actions taken during execution
    pub metacognitive_actions: Vec<String>,
    
    /// If execution succeeded and a new workflow is suggested
    pub next_workflow: Option<TaskHarness>,
    
    /// If execution failed, the error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ExecutionResult {
    pub fn success(harness: TaskHarness, conclusion: String, confidence: f64) -> Self {
        Self {
            harness_used: harness,
            conclusion,
            confidence,
            execution_time_ms: 0,
            verified_artifacts: vec![],
            reasoning_graph: None,
            metacognitive_actions: vec![],
            next_workflow: None,
            error: None,
        }
    }
    
    pub fn failure(harness: TaskHarness, error: String) -> Self {
        Self {
            harness_used: harness,
            conclusion: String::new(),
            confidence: 0.0,
            execution_time_ms: 0,
            verified_artifacts: vec![],
            reasoning_graph: None,
            metacognitive_actions: vec![],
            next_workflow: None,
            error: Some(error),
        }
    }
    
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
```

- [ ] **Step 4: Update exports**

```rust
// crates/rustycode-protocol/src/lib.rs - add at top level
pub mod executor_result;
pub use executor_result::ExecutionResult;
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p rustycode-protocol executor_result -- --nocapture
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-protocol/src/executor_result.rs crates/rustycode-protocol/src/lib.rs
git commit -m "feat: add ExecutionResult type for harness outputs"
```

---

### Task 2: Create Harness Module Structure

**Files:**
- Create: `crates/rustycode-orchestra/src/harnesses/mod.rs`
- Create: `crates/rustycode-orchestra/src/harnesses/direct.rs`
- Modify: `crates/rustycode-orchestra/src/orchestra_executor.rs`
- Modify: `crates/rustycode-orchestra/src/service/orchestra_service.rs`
- Modify: `crates/rustycode-orchestra/src/lib.rs`

Define the harness trait and the Direct harness implementation (simplest case).

- [ ] **Step 1: Write test for Direct harness**

```rust
// crates/rustycode-orchestra/tests/harness_integration_test.rs
#[tokio::test]
async fn test_direct_harness_simple_execution() {
    use rustycode_protocol::task_routing::{TaskHarness, TaskRoutingDecision};
    use rustycode_protocol::ExecutionResult;
    
    let decision = TaskRoutingDecision {
        intent: Default::default(),
        confidence: 0.9,
        action: Default::default(),
        workflow: Default::default(),
        harness: TaskHarness::Direct,
        thinking: Default::default(),
        execution_plan: Default::default(),
        team: Default::default(),
        agent: Default::default(),
        skills: vec![],
        missing_info: vec![],
    };
    
    let executor = OrchestraExecutor::new();
    let result = executor.execute(decision).await.expect("execution should succeed");
    
    assert_eq!(result.harness_used, TaskHarness::Direct);
    assert!(result.is_success());
    assert!(!result.conclusion.is_empty());
}
```

(Rest of Task 2 steps as in the plan document above...)

---

### Task 3-8: (See full plan document for detailed steps)

---

## Summary

**Tasks:** 8 major implementation tasks
**Tests:** 40+ unit and integration tests
**Commits:** 8 atomic commits per task
**Total Files Modified/Created:** 11+

Start with Task 1. After completion, proceed to Task 2, etc.
