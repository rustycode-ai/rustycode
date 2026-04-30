# Orchestration Crate Build & Completion Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the partially-implemented rustycode-orchestration crate, resolve build errors, complete missing components, and ensure all spec requirements are code-complete and tested.

**Architecture:** The crate follows a "Symphony" metaphor with four tiers:
- **Conductor** (orchestrator.rs / pipeline.rs) — manages task lifecycle, tier routing
- **Musician** (musician.rs / Tier 2) — executes steps with simple models (Haiku-class)
- **Editor** (editor.rs / Tier 3) — replans failed steps with medium models (Sonnet-class)
- **Composer** (composer.rs / Tier 4) — deep reasoning with advanced models (Opus + extended thinking)

**Current Status:** Crate exists with ~30 Rust files but has 2 build errors + incomplete implementations:
1. ErrorCategory missing `Custom` variant (spec requires it for extensibility)
2. Signal move semantics error in musician.rs (signal moved into entry, then used)

**Tech Stack:** Rust 2021, tokio async, rusqlite (SQLite), serde_yaml, thiserror, anyhow, existing `rustycode-llm`, `rustycode-tools`, `rustycode-deep-thinker`

---

## File Structure Overview

### Core Orchestration Files (Exist, Need Fixes)
- `src/conductor.rs` — Main pipeline orchestrator (partially done, has compile errors)
- `src/pipeline.rs` — Public API for running tasks through all tiers
- `src/musician.rs` — Tier 2 executor (has signal move bug)
- `src/editor.rs` — Tier 3 replanner (needs completion)
- `src/composer.rs` — Tier 4 deep thinker adapter (needs completion)
- `src/escalation_router.rs` — Escalation decision logic (needs completion)

### Infrastructure (Exist, Need Fixes)
- `src/error.rs` — ErrorCategory enum (missing Custom variant)
- `src/task_context.rs` — TaskContext & TaskPhase (exists)
- `src/execution_trace.rs` — ExecutionTrace (exists)
- `src/failure_patterns/` — FailurePatternStore trait + SQLite impl (exists)
- `src/config.rs` — YAML config loading (exists)
- `src/model_registry.rs` — Model selection (incomplete)

### Verification & Validation (Exist, Need Fixes)
- `src/verification_gates.rs` — Rule-based output validation (incomplete)
- `src/task_decomposer.rs` — Task breakdown (incomplete)
- `src/plan_refiner.rs` — Plan review against patterns (incomplete)
- `src/reasoner.rs` — Tier 3 replanning logic (incomplete)

### Tests (Exist, Need Expansion)
- `tests/pipeline_integration_test.rs` — Full pipeline tests (incomplete)
- `tests/failure_store_test.rs` — Storage tests (exists)
- `tests/error_classifier_test.rs` — Error classification (exists)
- `tests/execution_trace_test.rs` — Trace tests (exists)
- `tests/edge_cases_test.rs` — Edge case tests (incomplete)
- `tests/full_pipeline_test.rs` — E2E pipeline test (incomplete)

---

## Phase 1: Resolve Build Errors (Tasks 1-2)

### Task 1: Fix ErrorCategory Custom Variant

**Files:**
- Modify: `crates/rustycode-orchestration/src/error.rs:105-150`

**Problem:** ErrorCategory enum is missing the `Custom(String)` variant that the spec requires. Code in failure_store/sqlite.rs tries to use it and fails.

- [ ] **Step 1: Read ErrorCategory definition**

Run: `grep -A 30 "pub enum ErrorCategory" crates/rustycode-orchestration/src/error.rs`

Expected: See current variants (Transient, Recoverable, etc.) but no Custom variant.

- [ ] **Step 2: Add Custom variant to ErrorCategory**

Modify `crates/rustycode-orchestration/src/error.rs` — find the ErrorCategory enum and add:

```rust
pub enum ErrorCategory {
    // Standard error categories from spec
    SyntaxError,
    CompileError,
    TypeError,
    LogicError,
    PermissionDenied,
    DiskFull,
    ToolTimeout,
    ContextLengthExceeded,
    
    // Infrastructure errors
    Configuration,
    Authentication,
    ResourceExhaustion,
    Validation,
    Internal,
    Transient,
    Recoverable,
    Permanent,
    
    // Extensible for unknown errors seen in FailurePatternStore
    Custom(String),
    
    // Legacy/deprecated
    Fatal,
    Unknown,
}
```

(Keep existing variants, just ensure `Custom(String)` is present and well-ordered.)

- [ ] **Step 3: Verify serialization**

Ensure ErrorCategory derives `Serialize, Deserialize`. At the top of the enum definition, check for:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorCategory {
    ...
}
```

If not present, add it.

- [ ] **Step 4: Update OrchestrationError::to_category() to handle Custom**

Find the `to_category()` impl and ensure it has a case for any new variants. For `Custom`, map to `ErrorCategory::Internal` or keep as-is if it's already in the match.

- [ ] **Step 5: Verify failure_store/sqlite.rs compiles**

Now that Custom exists, the decode_category should compile. Run:

```bash
cargo build -p rustycode-orchestration 2>&1 | grep "^error"
```

Expected: The `ErrorCategory::Custom("Unknown".into())` error should be gone.

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/src/error.rs
git commit -m "fix: add Custom variant to ErrorCategory enum for extensibility"
```

---

### Task 2: Fix Signal Move Error in Musician

**Files:**
- Modify: `crates/rustycode-orchestration/src/musician.rs:38-54`

**Problem:** Line 38 creates `signal`, line 47 moves it into `TraceEntry::new_failure()`, but line 52 tries to use `signal.category` again.

- [ ] **Step 1: Read the problematic code section**

Run: `sed -n '35,55p' crates/rustycode-orchestration/src/musician.rs`

Expected: See signal created at 38, moved at 47, used at 52.

- [ ] **Step 2: Clone signal before moving**

Modify the code to clone signal before moving:

```rust
let signal = ErrorClassifier::create_signal(&error, context);
let entry = TraceEntry::new_failure(
    step.id.clone(),
    step_index,
    ctx.current_tier,
    tool_name.to_string(),
    serde_json::json!({"step": step.description}),
    tool_output.to_string(),
    exit_code,
    signal.clone(),  // Clone before move
    cost_usd,
);
trace.append(entry);
return Ok(StepResult::Failed {
    error_category: signal.category,  // Now signal is still available
    output: tool_output.to_string(),
});
```

- [ ] **Step 3: Verify ErrorSignal is Clone**

Check that ErrorSignal derives Clone:

```bash
grep "^pub struct ErrorSignal" crates/rustycode-orchestration/src/error.rs -A 2
```

Expected: Should show `#[derive(..., Clone, ...)]` or similar.

If not, add Clone to the derive list.

- [ ] **Step 4: Build and verify**

```bash
cargo build -p rustycode-orchestration 2>&1 | grep "^error"
```

Expected: No errors mentioning `signal` or move.

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestration/src/musician.rs
git commit -m "fix: clone signal before moving into TraceEntry to allow reuse"
```

---

## Phase 2: Complete Core Components (Tasks 3-8)

### Task 3: Complete TaskDecomposer Implementation

**Files:**
- Modify: `crates/rustycode-orchestration/src/task_decomposer.rs`
- Modify: `crates/rustycode-orchestration/tests/full_pipeline_test.rs` (add tests)

**Goal:** Ensure TaskDecomposer trait is fully defined and LLM implementation is complete.

- [ ] **Step 1: Read current TaskDecomposer code**

Run: `head -100 crates/rustycode-orchestration/src/task_decomposer.rs`

Expected: See trait definition and/or implementation.

- [ ] **Step 2: Verify trait signature matches spec §3.1**

Check that the trait has:
```rust
pub trait TaskDecomposer: Send + Sync {
    async fn decompose(&self, task: &str, context: &DecompositionContext) -> Result<DecomposedTask>;
}
```

If missing, add it.

- [ ] **Step 3: Write failing decomposer test**

Create test in `tests/full_pipeline_test.rs`:

```rust
#[tokio::test]
async fn test_decomposer_breaks_task_into_steps() {
    let decomposer = LlmDecomposer::new(mock_provider(), "test-model");
    let context = DecompositionContext::default();
    let result = decomposer.decompose("List files in /tmp", &context).await.unwrap();
    
    assert!(result.steps.len() > 0);
    assert!(!result.steps[0].description.is_empty());
}
```

- [ ] **Step 4: Implement LlmDecomposer if missing**

Check if `struct LlmDecomposer` exists. If not, add:

```rust
pub struct LlmDecomposer {
    provider: Arc<dyn LLMProvider>,
    model: String,
}

impl LlmDecomposer {
    pub fn new(provider: Arc<dyn LLMProvider>, model: String) -> Self {
        Self { provider, model }
    }
}

#[async_trait]
impl TaskDecomposer for LlmDecomposer {
    async fn decompose(&self, task: &str, _context: &DecompositionContext) -> Result<DecomposedTask> {
        // Build prompt asking LLM to break task into 5-10 steps
        let prompt = format!(
            "Break this task into 5-10 executable steps:\n{}\nReturn as JSON.",
            task
        );
        
        // Call LLM
        let response = self.provider.complete(
            self.model.clone(),
            vec![/* prompt message */],
            None,
        ).await?;
        
        // Parse response into DecomposedTask
        Ok(DecomposedTask {
            original_task: task.to_string(),
            task_category: "unknown".to_string(),
            steps: vec![],
            estimated_difficulty: Difficulty::Medium,
        })
    }
}
```

- [ ] **Step 5: Run test**

```bash
cargo test -p rustycode-orchestration --test full_pipeline_test test_decomposer
```

Expected: Test passes (or fails with reasonable mock provider limitations).

- [ ] **Step 6: Commit**

```bash
git add crates/rustycode-orchestration/src/task_decomposer.rs tests/
git commit -m "feat(decomposer): complete TaskDecomposer trait and LlmDecomposer impl"
```

---

### Task 4: Complete PlanRefiner Implementation

**Files:**
- Modify: `crates/rustycode-orchestration/src/plan_refiner.rs`
- Modify: `tests/full_pipeline_test.rs` (add tests)

Similar structure to Task 3:
1. Verify PlanRefiner trait matches spec §3.2
2. Write test for refinement logic
3. Implement LlmPlanRefiner with call to LLM for pattern review
4. Test that refiner rejects/modifies plans based on FailurePatternStore
5. Commit

- [ ] **Step 1-5: Follow Task 3 pattern**

Use same approach: read current code, verify against spec, write test, implement, commit.

Expected: Refiner calls FailurePatternStore to get historical patterns, then prompts LLM to review/modify plan.

---

### Task 5: Complete Editor (Tier 3) Implementation

**Files:**
- Modify: `crates/rustycode-orchestration/src/editor.rs`
- Modify: `tests/full_pipeline_test.rs` (add escalation tests)

**Goal:** Ensure Editor (Tier 3 Reasoner) is fully implemented.

- [ ] **Step 1: Verify Reasoner trait signature**

Check:
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
```

- [ ] **Step 2: Implement LlmReasoner**

Editor should call LLM with:
- ExecutionTrace (all previous steps)
- Failed step description
- Error signal + category
- Historical patterns from FailurePatternStore

LLM suggests next steps to try.

- [ ] **Step 3: Write test for failed step replanning**

```rust
#[tokio::test]
async fn test_editor_replans_on_compile_error() {
    let reasoner = LlmReasoner::new(mock_provider(), "sonnet-model");
    let trace = ExecutionTrace::new("task-1");
    let failed_step = Step { /* ... */ };
    let error = ErrorSignal { category: ErrorCategory::CompileError, ... };
    let patterns = vec![];
    
    let result = reasoner.replan(&trace, &failed_step, &error, &patterns).await.unwrap();
    assert!(!result.updated_step.description.is_empty());
}
```

- [ ] **Step 4-5: Implement and commit**

Following Task 3 pattern.

---

### Task 6: Complete Composer (Tier 4) Deep-Thinker Adapter

**Files:**
- Modify: `crates/rustycode-orchestration/src/composer.rs`
- Modify: `crates/rustycode-orchestration/src/deep_thinker_adapter.rs`
- Modify: `tests/full_pipeline_test.rs` (add deep-thinker tests)

**Goal:** Ensure Composer wraps rustycode-deep-thinker and handles extended thinking scenarios.

- [ ] **Step 1: Verify Composer exists and imports deep-thinker**

```bash
head -20 crates/rustycode-orchestration/src/composer.rs
```

Should import `rustycode_deep_thinker`.

- [ ] **Step 2: Define DeepThinkingContext struct if missing**

```rust
pub struct DeepThinkingContext {
    pub original_task: String,
    pub decomposed_plan: DecomposedTask,
    pub all_failures: Vec<FailurePattern>,
    pub tier3_attempts: Vec<ReplanResult>,
}
```

- [ ] **Step 3: Implement Composer as wrapper**

```rust
pub struct Composer {
    provider: Arc<dyn LLMProvider>,
}

#[async_trait]
impl DeepThinker for Composer {
    async fn solve(
        &self,
        trace: &ExecutionTrace,
        context: &DeepThinkingContext,
    ) -> Result<DeepThinkingResult> {
        // Build comprehensive prompt with all context
        // Enable extended thinking
        // Call rustycode-deep-thinker or direct LLM with extended thinking
        // Return solution
    }
}
```

- [ ] **Step 4: Add test for deep-thinker invocation**

```rust
#[tokio::test]
#[ignore]  // Requires real provider for extended thinking
async fn test_composer_invokes_deep_thinker() {
    // Test that Composer can call deep-thinker with extended thinking enabled
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-orchestration/src/composer.rs crates/rustycode-orchestration/src/deep_thinker_adapter.rs
git commit -m "feat(composer): complete Tier 4 deep-thinker adapter with extended thinking"
```

---

### Task 7: Complete EscalationRouter Implementation

**Files:**
- Modify: `crates/rustycode-orchestration/src/escalation_router.rs`
- Modify: `tests/full_pipeline_test.rs` (add escalation tests)

**Goal:** Implement escalation decision logic per spec §5.2-5.4.

- [ ] **Step 1: Verify EscalationRouter trait**

```rust
pub trait EscalationRouter: Send + Sync {
    fn should_escalate(
        &self,
        context: &TaskContext,
        error_signal: &ErrorSignal,
    ) -> EscalationDecision;
}
```

- [ ] **Step 2: Implement escalation matrix**

```rust
pub fn should_escalate(&self, context: &TaskContext, error: &ErrorSignal) -> EscalationDecision {
    // Hard stops (any state)
    if self.hallucination_detected(&context.execution_trace) {
        return EscalationDecision::Abandon { reason: "hallucination_loop".into() };
    }
    if context.cost_used >= context.max_budget {
        return EscalationDecision::Abandon { reason: "budget_exceeded".into() };
    }
    
    // Per-tier escalation logic
    match context.current_tier {
        2 => {
            // Tier 2 escalates on critical errors or after 2 attempts
            match error.category {
                ErrorCategory::SyntaxError
                | ErrorCategory::CompileError
                | ErrorCategory::TypeError => {
                    EscalationDecision::Escalate { next_tier: 3, reason: "critical_error".into() }
                }
                _ if context.attempt_count >= 2 => {
                    EscalationDecision::Escalate { next_tier: 3, reason: "max_attempts".into() }
                }
                _ => EscalationDecision::Retry,
            }
        }
        3 => {
            // Tier 3 escalates more aggressively
            if matches!(error.category, ErrorCategory::SyntaxError | ErrorCategory::CompileError)
                || context.attempt_count >= 2
            {
                EscalationDecision::Escalate { next_tier: 4, reason: "escalate_to_composer".into() }
            } else {
                EscalationDecision::Retry
            }
        }
        4 => {
            // Tier 4 is final
            EscalationDecision::Abandon { reason: "tier4_exhausted".into() }
        }
        _ => EscalationDecision::Retry,
    }
}
```

- [ ] **Step 3: Write escalation tests**

Test each tier's escalation logic:

```rust
#[test]
fn test_tier2_escalates_on_compile_error() {
    let router = DefaultEscalationRouter::new(mock_store());
    let ctx = TaskContext::new("t", 0.50);
    let error = ErrorSignal { category: ErrorCategory::CompileError, ... };
    
    let decision = router.should_escalate(&ctx, &error);
    assert!(matches!(decision, EscalationDecision::Escalate { next_tier: 3, .. }));
}
```

- [ ] **Step 4-5: Implement and commit**

---

### Task 8: Complete VerificationGate Implementation

**Files:**
- Modify: `crates/rustycode-orchestration/src/verification_gates.rs`
- Create: `crates/rustycode-orchestration/rules/default.yaml`
- Create: `crates/rustycode-orchestration/rules/rust_refactoring.yaml`
- Create: `crates/rustycode-orchestration/rules/data_etl.yaml`
- Modify: `tests/full_pipeline_test.rs` (add rule validation tests)

**Goal:** Complete VerificationGate with rule file loading and execution.

- [ ] **Step 1: Verify VerificationGate trait**

```rust
pub trait VerificationGate: Send + Sync {
    fn verify(&self, step: &Step, result: &StepResult) -> VerificationOutcome;
}
```

- [ ] **Step 2: Implement rule file loader**

```rust
pub struct RuleFileVerificationGate {
    rules_dir: PathBuf,
    rules_by_task_type: HashMap<String, Vec<Rule>>,
}

impl RuleFileVerificationGate {
    pub fn new(rules_dir: &Path) -> Result<Self> {
        let mut rules_by_task_type = HashMap::new();
        // Load all .yaml files from rules_dir
        // Parse each file into Rule structs
        Ok(Self { rules_dir: rules_dir.to_path_buf(), rules_by_task_type })
    }
}

#[async_trait]
impl VerificationGate for RuleFileVerificationGate {
    fn verify(&self, step: &Step, result: &StepResult) -> VerificationOutcome {
        // Get rules for task type (or use default)
        // Check each rule against result
        // Return Valid, Invalid, or Uncertain
    }
}
```

- [ ] **Step 3: Create default rule files**

Create `crates/rustycode-orchestration/rules/default.yaml`:

```yaml
task_type: default
rules:
  - description: Exit code must be 0 or 1
    check: exit_code
    valid_codes: [0, 1]
    on_failure: LogicError
```

Create `crates/rustycode-orchestration/rules/rust_refactoring.yaml`:

```yaml
task_type: rust_refactoring
rules:
  - description: Generated code must parse as valid Rust
    check: syntax
    syntax_validator: rustc
    on_failure: LogicError
  - description: Must not contain TODO comments in critical sections
    check: regex
    pattern: '(?i)TODO|FIXME'
    on_match: Invalid
```

Create `crates/rustycode-orchestration/rules/data_etl.yaml`:

```yaml
task_type: data_etl
rules:
  - description: Output file must exist
    check: file_exists
    on_failure: LogicError
  - description: Output must be valid JSON or CSV
    check: format_validation
    format: json_or_csv
    on_failure: LogicError
```

- [ ] **Step 4: Write rule loading test**

```rust
#[test]
fn test_rule_file_loading() {
    let gate = RuleFileVerificationGate::new(Path::new("crates/rustycode-orchestration/rules")).unwrap();
    assert!(gate.rules_by_task_type.contains_key("default"));
    assert!(gate.rules_by_task_type.contains_key("rust_refactoring"));
}
```

- [ ] **Step 5-6: Implement and commit**

---

## Phase 3: Integration & Testing (Tasks 9-10)

### Task 9: Complete Conductor/Pipeline Integration

**Files:**
- Modify: `crates/rustycode-orchestration/src/conductor.rs`
- Modify: `crates/rustycode-orchestration/src/pipeline.rs`
- Modify: `tests/full_pipeline_test.rs`

**Goal:** Ensure Conductor correctly routes tasks through all tiers and manages state machine.

- [ ] **Step 1: Verify pipeline entry point**

Check that `pipeline.rs` exports:

```rust
pub async fn run_orchestration(
    task: &str,
    config: OrchestrationConfig,
) -> Result<OrchestrationMetrics> {
    // 1. Decompose task
    // 2. Refine plan
    // 3. Run Tier 2 (Musician)
    // 4. If success, return metrics
    // 5. If failed, escalate to Tier 3 (Editor)
    // 6. If still failed, escalate to Tier 4 (Composer)
    // 7. Return final metrics with all details
}
```

- [ ] **Step 2: Implement full pipeline flow**

```rust
pub struct OrchestrationPipeline {
    decomposer: Arc<dyn TaskDecomposer>,
    plan_refiner: Arc<dyn PlanRefiner>,
    musician: Arc<dyn Musician>,  // Tier 2
    editor: Arc<dyn Editor>,       // Tier 3
    composer: Arc<dyn Composer>,   // Tier 4
    escalation_router: Arc<dyn EscalationRouter>,
}

impl OrchestrationPipeline {
    pub async fn run(&self, task: &str) -> Result<OrchestrationMetrics> {
        // Execute full orchestration
    }
}
```

- [ ] **Step 3: Write full pipeline test**

```rust
#[tokio::test]
async fn test_full_pipeline_happy_path() {
    let pipeline = OrchestrationPipeline::new(
        mock_decomposer(),
        mock_refiner(),
        mock_musician_success(),
        mock_editor(),
        mock_composer(),
        mock_router(),
    );
    
    let metrics = pipeline.run("simple task").await.unwrap();
    assert_eq!(metrics.final_outcome, Some(TaskOutcome::SuccessAtTier(2)));
}
```

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-orchestration/src/conductor.rs crates/rustycode-orchestration/src/pipeline.rs
git commit -m "feat(conductor): complete full pipeline orchestration with tier routing"
```

---

### Task 10: Comprehensive Test Suite & Build Verification

**Files:**
- Modify: `tests/full_pipeline_test.rs` (add all remaining tests)
- Verify: All code compiles cleanly

**Goal:** Ensure full test coverage and zero build warnings.

- [ ] **Step 1: Run full test suite**

```bash
cargo test -p rustycode-orchestration --lib
```

Expected: All tests pass.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -p rustycode-orchestration -- -D warnings
```

Expected: No warnings.

- [ ] **Step 3: Build in release mode**

```bash
cargo build -p rustycode-orchestration --release
```

Expected: Clean build.

- [ ] **Step 4: Document any remaining TODOs**

Search for TODO/FIXME:

```bash
grep -r "TODO\|FIXME" crates/rustycode-orchestration/src/ | wc -l
```

If any, update issue tracker or comments to track cleanup.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "test(orchestration): add comprehensive test suite and verify clean build"
```

---

## Summary

**Total Tasks: 10**

- **Phase 1 (Build Fixes)**: Tasks 1-2 — Resolve 2 compiler errors (Custom variant, signal move)
- **Phase 2 (Core Components)**: Tasks 3-8 — Complete 6 major components (Decomposer, Refiner, Editor, Composer, Router, VerificationGate)
- **Phase 3 (Integration)**: Tasks 9-10 — Complete pipeline, full testing, clean build

**Estimated Time:** 3-4 days for experienced Rust developer; 5-7 days for new to codebase

**Deliverables:**
- ✅ All spec requirements implemented in code
- ✅ Zero build errors / warnings
- ✅ Comprehensive test suite (>90% coverage)
- ✅ Orchestration crate ready for integration with CLI/TUI

**Next Step:** Once this plan is complete, proceed to **Integration Layer Plan** (classifier + routing).
