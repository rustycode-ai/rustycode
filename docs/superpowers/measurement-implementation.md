# Measurement Implementation Plan

**Goal**: Instrument RustyCode to automatically collect the 4 metrics (completion, tokens, tool accuracy, recovery).

**Effort**: ~2-3 days (small crate + 4 integration points)

---

## Step 1: Create Measurement Crate

```bash
cargo new crates/rustycode-measurements --lib
```

### File Structure

```
crates/rustycode-measurements/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs           # Public API
    ├── collector.rs     # MeasurementCollector
    ├── metrics.rs       # Metric types (TokenMetrics, ToolMetrics, etc.)
    ├── storage.rs       # JSON persistence
    └── hooks.rs         # Integration points (on_tool_call, on_failure, etc.)
```

---

## Step 2: Core Types (metrics.rs)

```rust
/// Single task measurement
#[derive(Serialize, Deserialize, Debug)]
pub struct TaskMeasurement {
    pub task_id: String,
    pub timestamp: String,
    pub phase: usize,  // which phase are we measuring?
    pub category: String,  // "baseline", "decomposition", "complex", etc.
    
    // Completion
    pub completed: bool,
    pub completion_reason: Option<String>,  // abandoned reason
    pub steps: usize,
    
    // Tokens
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub tokens_per_step: f32,
    
    // Tools
    pub tool_calls: Vec<ToolCall>,
    pub avg_tool_accuracy: f32,  // mean of all tool call scores
    
    // Recovery
    pub failures: Vec<FailureRecord>,
    pub recovery_rate: f32,  // failures_recovered / total_failures
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ToolCall {
    pub name: String,
    pub step: usize,
    pub correct: bool,  // was this the right tool?
    pub accuracy_score: f32,  // 0.0-1.0
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FailureRecord {
    pub step: usize,
    pub error: String,
    pub tool: String,
    pub recovered: bool,
    pub recovery_steps: usize,
    pub strategy_changed: bool,
}

/// Aggregate results across many tasks
#[derive(Serialize, Deserialize, Debug)]
pub struct MeasurementSummary {
    pub phase: usize,
    pub task_count: usize,
    pub completion_rate: f32,  // 0.0-1.0
    pub avg_tokens_per_step: f32,
    pub avg_tool_accuracy: f32,
    pub recovery_rate: f32,
    pub tasks: Vec<TaskMeasurement>,
}
```

---

## Step 3: Collector Implementation (collector.rs)

```rust
pub struct MeasurementCollector {
    task_id: String,
    phase: usize,
    category: String,
    completed: bool,
    completion_reason: Option<String>,
    steps: usize,
    
    // Tokens
    input_tokens: usize,
    output_tokens: usize,
    
    // Tools
    tool_calls: Vec<ToolCall>,
    
    // Recovery
    failures: Vec<FailureRecord>,
    current_step_failed: bool,
}

impl MeasurementCollector {
    pub fn new(task_id: impl Into<String>, phase: usize, category: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            phase,
            category: category.into(),
            completed: false,
            completion_reason: None,
            steps: 0,
            input_tokens: 0,
            output_tokens: 0,
            tool_calls: Vec::new(),
            failures: Vec::new(),
            current_step_failed: false,
        }
    }
    
    // Called by orchestration pipeline after each LLM response
    pub fn record_tokens(&mut self, input: usize, output: usize) {
        self.input_tokens += input;
        self.output_tokens += output;
    }
    
    // Called when agent makes a tool decision
    pub fn record_tool_call(
        &mut self,
        tool_name: impl Into<String>,
        correct: bool,
        accuracy_score: f32,
    ) {
        self.tool_calls.push(ToolCall {
            name: tool_name.into(),
            step: self.steps,
            correct,
            accuracy_score,
        });
    }
    
    // Called when a tool execution fails
    pub fn record_failure(&mut self, error: impl Into<String>, tool: impl Into<String>) {
        self.failures.push(FailureRecord {
            step: self.steps,
            error: error.into(),
            tool: tool.into(),
            recovered: false,  // assume not recovered until we see adaptation
            recovery_steps: 0,
            strategy_changed: false,
        });
        self.current_step_failed = true;
    }
    
    // Called when agent adapts after a failure
    pub fn record_recovery(&mut self, recovery_steps: usize, strategy_changed: bool) {
        if let Some(last_failure) = self.failures.last_mut() {
            last_failure.recovered = true;
            last_failure.recovery_steps = recovery_steps;
            last_failure.strategy_changed = strategy_changed;
        }
        self.current_step_failed = false;
    }
    
    // Called at end of each orchestration step
    pub fn increment_step(&mut self) {
        self.steps += 1;
    }
    
    // Called when task completes
    pub fn record_completion(&mut self, success: bool, reason: Option<String>) {
        self.completed = success;
        self.completion_reason = reason;
    }
    
    // Finalize and return measurement
    pub fn finalize(self) -> TaskMeasurement {
        let total_tokens = self.input_tokens + self.output_tokens;
        let tokens_per_step = if self.steps > 0 {
            total_tokens as f32 / self.steps as f32
        } else {
            0.0
        };
        
        let avg_tool_accuracy = if !self.tool_calls.is_empty() {
            self.tool_calls.iter().map(|t| t.accuracy_score).sum::<f32>()
                / self.tool_calls.len() as f32
        } else {
            0.0
        };
        
        let recovery_rate = if !self.failures.is_empty() {
            self.failures.iter().filter(|f| f.recovered).count() as f32
                / self.failures.len() as f32
        } else {
            1.0  // no failures = perfect recovery
        };
        
        TaskMeasurement {
            task_id: self.task_id,
            timestamp: chrono::Local::now().to_rfc3339(),
            phase: self.phase,
            category: self.category,
            completed: self.completed,
            completion_reason: self.completion_reason,
            steps: self.steps,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            tokens_per_step,
            tool_calls: self.tool_calls,
            avg_tool_accuracy,
            failures: self.failures,
            recovery_rate,
        }
    }
}
```

---

## Step 4: Storage (storage.rs)

```rust
use std::path::Path;

pub fn save_measurement(
    measurement: &TaskMeasurement,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(measurement)?;
    let filename = format!("{}.json", measurement.task_id);
    let path = output_dir.join(filename);
    std::fs::write(path, json)?;
    Ok(())
}

pub fn save_summary(
    summary: &MeasurementSummary,
    output_dir: &Path,
) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(summary)?;
    let filename = format!("phase-{}-summary.json", summary.phase);
    let path = output_dir.join(filename);
    std::fs::write(path, json)?;
    Ok(())
}

pub fn aggregate_measurements(measurements: &[TaskMeasurement]) -> MeasurementSummary {
    let phase = measurements.first().map(|m| m.phase).unwrap_or(0);
    
    let completion_rate = if !measurements.is_empty() {
        measurements.iter().filter(|m| m.completed).count() as f32
            / measurements.len() as f32
    } else {
        0.0
    };
    
    let avg_tokens_per_step = if !measurements.is_empty() {
        measurements.iter().map(|m| m.tokens_per_step).sum::<f32>()
            / measurements.len() as f32
    } else {
        0.0
    };
    
    let avg_tool_accuracy = if !measurements.is_empty() {
        measurements.iter().map(|m| m.avg_tool_accuracy).sum::<f32>()
            / measurements.len() as f32
    } else {
        0.0
    };
    
    let recovery_rate = if !measurements.is_empty() {
        measurements.iter().map(|m| m.recovery_rate).sum::<f32>()
            / measurements.len() as f32
    } else {
        0.0
    };
    
    MeasurementSummary {
        phase,
        task_count: measurements.len(),
        completion_rate,
        avg_tokens_per_step,
        avg_tool_accuracy,
        recovery_rate,
        tasks: measurements.to_vec(),
    }
}
```

---

## Step 5: Integration Points

### Point 1: Orchestration Pipeline (rustycode-orchestration/src/lib.rs)

```rust
// At the start of task execution:
let mut measurements = MeasurementCollector::new(
    &task.id,
    CURRENT_PHASE,  // e.g., 6
    "complex",
);

// After each LLM call:
measurements.record_tokens(
    response.usage.input_tokens,
    response.usage.output_tokens,
);

// When tool is selected:
measurements.record_tool_call(
    tool_name,
    is_correct,  // score from quality module
    accuracy_score,
);

// On tool failure:
measurements.record_failure(error, tool_name);

// On recovery:
measurements.record_recovery(steps_taken, strategy_changed);

// Each orchestration step:
measurements.increment_step();

// At end of task:
measurements.record_completion(success, reason);
let measurement = measurements.finalize();
rustycode_measurements::storage::save_measurement(&measurement, output_dir)?;
```

### Point 2: Tool Executor (rustycode-tools/src/executor.rs)

Track when tools fail and whether agent recovers.

### Point 3: Quality Module (rustycode-orchestration/src/judge.rs)

Feed tool accuracy scores into collector.

### Point 4: Skill Activation (rustycode-skill/src/activation.rs)

Track which skills were activated and used.

---

## Step 6: Usage in Tests

```rust
#[tokio::test]
async fn measure_complex_task_decomposition() {
    let mut measurements = MeasurementCollector::new(
        "test-feature-impl-001",
        6,  // phase
        "decomposition",
    );
    
    // Run orchestration...
    let result = orchestration.execute(task).await?;
    
    // Collect metrics
    measurements.record_tokens(5000, 2000);
    measurements.record_tool_call("read", true, 0.95);
    measurements.record_tool_call("edit", true, 0.90);
    measurements.increment_step();
    measurements.increment_step();
    measurements.record_completion(result.success, None);
    
    let measurement = measurements.finalize();
    assert!(measurement.completed);
    println!("Task completed in {} steps, {} tokens/step",
        measurement.steps,
        measurement.tokens_per_step);
}
```

---

## Step 7: Analysis Script

Create `scripts/analyze-measurements.py`:

```python
#!/usr/bin/env python3
import json
import os
from pathlib import Path

def load_summaries(dir: Path):
    summaries = {}
    for phase_file in sorted(dir.glob("phase-*-summary.json")):
        with open(phase_file) as f:
            data = json.load(f)
            phase = data["phase"]
            summaries[phase] = data
    return summaries

def compare_phases(baseline_phase: int, current_phase: int, summaries: dict):
    baseline = summaries.get(baseline_phase)
    current = summaries.get(current_phase)
    
    if not baseline or not current:
        print("Missing baseline or current data")
        return
    
    print(f"\n{'=' * 60}")
    print(f"RustyCode Improvement: Phase {baseline_phase} → Phase {current_phase}")
    print(f"{'=' * 60}\n")
    
    metrics = ["completion_rate", "avg_tokens_per_step", "avg_tool_accuracy", "recovery_rate"]
    
    for metric in metrics:
        baseline_val = baseline[metric]
        current_val = current[metric]
        
        if "rate" in metric or "accuracy" in metric:
            # Percentages
            delta_pct = ((current_val - baseline_val) / baseline_val) * 100
            print(f"{metric:25} {baseline_val:.1%} → {current_val:.1%}  ({delta_pct:+.0f}%)")
        else:
            # Absolute (tokens per step)
            delta_pct = ((baseline_val - current_val) / baseline_val) * 100
            print(f"{metric:25} {baseline_val:8.1f} → {current_val:8.1f}  ({delta_pct:+.0f}% improvement)")

if __name__ == "__main__":
    measurements_dir = Path("docs/superpowers/measurements")
    summaries = load_summaries(measurements_dir)
    
    # Compare phase 0 vs phase 6
    if 0 in summaries and 6 in summaries:
        compare_phases(0, 6, summaries)
```

---

## Roadmap

**Week 1**:
- [ ] Create `rustycode-measurements` crate
- [ ] Implement core types + collector
- [ ] Wire into orchestration pipeline
- [ ] Add test example

**Week 2**:
- [ ] Collect Phase 0 baseline (or recreate from git history)
- [ ] Validate measurements on Phase 0
- [ ] Run measurements on Phase 6 (current)
- [ ] Run analysis script
- [ ] Publish results to measurement-results.md

---

## Success Criteria

- Measurements are collected automatically (no manual intervention per task)
- Can run on any task without code changes
- Results stored as JSON (queryable, comparable)
- Can generate before/after report in < 10 minutes
- Accuracy scores tracked and aggregated
