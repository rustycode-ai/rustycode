# RustyCode Measurement Framework

**Purpose**: Quantify the impact of Phases 1-6 on complex task-solving capability.

---

## Four Dimensions of Success

### 1. Completion Rate (%)
**Metric**: Tasks completed end-to-end / total tasks attempted

- **What to track**: 
  - Did the agent produce a final output?
  - Did the output meet the acceptance criteria?
  - How many got stuck/abandoned mid-way?

- **Implementation**:
  ```rust
  pub struct TaskResult {
      pub task_id: String,
      pub completed: bool,
      pub steps_taken: usize,
      pub final_output_quality: Option<Quality>,
      pub abandoned_reason: Option<String>,
  }
  ```

- **Target**: Measure baseline (Phase 0) vs. Phase 6 (post-all-features)

---

### 2. Token Efficiency (tokens/step)
**Metric**: Average tokens spent per task step

- **What to track**:
  - Total tokens input + output per task
  - Number of agent turns (steps)
  - Tokens per step = total_tokens / steps

- **Why it matters**: 
  - Low token efficiency = agent thrashing, repeating itself
  - High efficiency = focused, purposeful execution

- **Implementation**:
  ```rust
  pub struct TokenMetrics {
      pub task_id: String,
      pub input_tokens: usize,
      pub output_tokens: usize,
      pub total_tokens: usize,
      pub steps: usize,
      pub tokens_per_step: f32,
  }
  ```

- **Target**: Compare Phase 0 vs Phase 6. Expect 20-40% reduction (better focus with decomposition).

---

### 3. Tool Accuracy (correct choice %)
**Metric**: Right tool selected / tool selections made

- **What to track**:
  - Was the tool appropriate for the goal?
  - Did the tool call sequence make sense?
  - Were tools called out of order?

- **How to measure**:
  - Post-hoc review: "Was tool X the best choice here?"
  - LLM-as-Judge: Score each tool selection 0-1
  - Can compute per task + aggregate

- **Implementation**:
  ```rust
  pub struct ToolAccuracy {
      pub task_id: String,
      pub tool_calls: Vec<ToolCall>,
      pub accuracy_scores: Vec<f32>,  // LLM score per call
      pub avg_accuracy: f32,  // mean of scores
      pub suggestion_count: usize,  // times agent had a better option
  }
  ```

- **Target**: Expect 15-25% improvement with skill-aware tool scoping (Phase 5).

---

### 4. Recovery Quality (graceful degradation %)
**Metric**: % of failures that were handled vs. caused abandonment

- **What to track**:
  - When a tool fails, does the agent:
    - Try alternative approach? ✅
    - Adjust decomposition? ✅
    - Give up? ❌
  - Time to recover (steps between failure and adaptation)

- **Implementation**:
  ```rust
  pub struct FailureRecovery {
      pub task_id: String,
      pub failures: Vec<Failure>,
  }

  pub struct Failure {
      pub step: usize,
      pub tool: String,
      pub error: String,
      pub recovered: bool,  // did agent adapt?
      pub recovery_steps: usize,  // steps to recovery
      pub alternative_strategy: Option<String>,
  }
  ```

- **Target**: Expect 30-50% improvement with adaptive workflows (Phase 2-3).

---

## Test Suite Design

### Category A: Baseline (Phase 0)
Tasks that were solvable BEFORE any new features:
- Simple file operations
- Basic code generation
- Linear 3-5 step procedures

**Run on**: System at git commit before Phase 1 implementation

### Category B: Decomposition (Phase 3)
Tasks requiring multi-step breakdown:
- Implement a feature in an existing codebase
- Refactor across multiple files
- Add tests to untested module

**Expected improvement**: 40-60% completion rate increase

### Category C: Autonomy (Phase 4)
Tasks where tool selection matters:
- Domain-specific coding (Go vs TypeScript)
- Respecting architecture rules
- Following project conventions

**Expected improvement**: 20-30% tool accuracy increase

### Category D: Complex (Phases 3+4+5)
Tasks that need ALL features:
- Design and implement a subsystem
- Fix a bug in an unfamiliar codebase with constraints
- Refactor with automated verification

**Expected improvement**: 50-70% completion rate increase

---

## Baseline Collection (Pre-Phase Implementation)

1. **Checkpoint**: Git commit before Phase 1 implementation
2. **Test on Category A + B + C**: Run all tasks, collect metrics
3. **Store results**: `docs/superpowers/baselines/phase-0.json`

```json
{
  "phase": 0,
  "date": "2026-04-20",
  "results": [
    {
      "task_id": "baseline-001",
      "category": "A",
      "completed": false,
      "steps": 8,
      "total_tokens": 12500,
      "tokens_per_step": 1562.5,
      "tool_accuracy": 0.75,
      "recovery_count": 1
    }
  ],
  "summary": {
    "completion_rate": 0.45,
    "avg_tokens_per_step": 1456.2,
    "avg_tool_accuracy": 0.72,
    "recovery_rate": 0.65
  }
}
```

---

## Current State Collection (Post-Phase Implementation)

After Phase 6 is complete:

1. **Run same test suite** against current system
2. **Collect identical metrics**
3. **Store results**: `docs/superpowers/baselines/phase-6.json`
4. **Compute deltas**: Phase 6 vs Phase 0

```json
{
  "phase": 6,
  "date": "2026-04-26",
  "results": [ /* same structure */ ],
  "summary": {
    "completion_rate": 0.78,
    "avg_tokens_per_step": 987.4,
    "avg_tool_accuracy": 0.89,
    "recovery_rate": 0.92
  },
  "delta_vs_phase_0": {
    "completion_rate": "+73%",
    "tokens_per_step": "-32%",
    "tool_accuracy": "+24%",
    "recovery_rate": "+42%"
  }
}
```

---

## Implementation: Measurement Collector

New crate: `crates/rustycode-measurements/`

```rust
pub struct MeasurementCollector {
    pub task_id: String,
    pub completion: CompletionMetric,
    pub tokens: TokenMetrics,
    pub tools: ToolMetrics,
    pub recovery: RecoveryMetrics,
}

impl MeasurementCollector {
    pub fn new(task_id: String) -> Self { /* ... */ }
    
    // Hook into orchestration pipeline
    pub fn on_tool_call(&mut self, tool: &str, chosen_correctly: bool) { /* ... */ }
    pub fn on_failure(&mut self, error: &str) { /* ... */ }
    pub fn on_recovery(&mut self, recovery_steps: usize) { /* ... */ }
    pub fn on_completion(&mut self, quality: Quality) { /* ... */ }
    
    pub fn finalize(self) -> TaskResult { /* ... */ }
    pub fn to_json(&self) -> String { /* ... */ }
}
```

**Integration points**:
1. `rustycode-orchestration`: Collect completion + token metrics automatically
2. `rustycode-skill/src/quality.rs`: Feed tool accuracy scores
3. `rustycode-tools`: Hook on tool failure/recovery

---

## Baseline Test Suite (Concrete Tasks)

### Phase 0 (Baseline) - Simple tasks
1. **File ops**: Read a JSON file, transform it, write back
2. **Code gen**: Generate a basic Rust function from spec
3. **Linear flow**: Follow 3-step procedure exactly

### Phase 3+ (Decomposition)
1. **Feature impl**: Add a new endpoint to existing API
2. **Refactor**: Extract a utility from duplicated code
3. **Testing**: Add unit tests to an untested module

### Phase 4+ (Domain autonomy)
1. **Language-specific**: Implement same algorithm in Go vs TypeScript
2. **Project rules**: Follow architecture guidelines while coding
3. **Conventions**: Match code style of existing codebase

### Phase 5+ (Tooling)
1. **Tool switching**: Task that needs Bash, then Read, then Edit
2. **Context limits**: Task requiring careful budget management
3. **Skill selection**: Task needing 2-3 different skills in sequence

### Complex (All phases)
1. **Subsystem design**: Design + implement a small system
2. **Bug fix**: Debug and fix in unfamiliar code
3. **Refactor with constraints**: Refactor while maintaining behavior + performance

---

## Measurement Dashboard (Future)

Once data is collected:

```
╔════════════════════════════════════════════════════════════╗
║              RustyCode Capability Improvement               ║
╠════════════════════════════════════════════════════════════╣
║                                                              ║
║  Completion Rate:      45% → 78%  ✅ +73%                  ║
║  Token Efficiency:  1456 → 987    ✅ -32%                  ║
║  Tool Accuracy:      72% → 89%    ✅ +24%                  ║
║  Recovery Rate:       65% → 92%   ✅ +42%                  ║
║                                                              ║
║  By Phase:                                                   ║
║    Phase 1 (Memory):        +12% (small, foundational)     ║
║    Phase 2 (Explore-Plan):  +18% (moderate improvement)    ║
║    Phase 3 (Decompose):     +35% (major breakthrough)      ║
║    Phase 4 (Autonomy):      +15% (niche benefit)           ║
║    Phase 5 (Tooling):       +18% (tool focus benefit)      ║
║    Phase 6 (Quality):       +8%  (validation only)         ║
║                                                              ║
╚════════════════════════════════════════════════════════════╝
```

---

## Next Steps

1. **Define test suite** (concrete tasks in `docs/superpowers/test-suite.md`)
2. **Implement MeasurementCollector** crate
3. **Wire into orchestration** pipeline
4. **Run baseline** on Phase 0 (or recreate it)
5. **Run current** on Phase 6
6. **Analyze deltas** and publish results
