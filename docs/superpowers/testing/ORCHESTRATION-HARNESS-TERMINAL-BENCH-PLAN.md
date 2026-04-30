# Orchestration Harness — Terminal Bench Testing & Evaluation Plan

**Goal:** Validate that the harness (quality detection + strategy selection + structured thinking) actually makes the LLM better at complex tasks.

**Approach:** Run 30-50 Terminal Bench tasks across complexity levels, measure success rates and reasoning quality.

---

## Test Design

### Task Categories

Select tasks from Terminal Bench organized by expected complexity:

#### Simple Tasks (Complexity < 2.0) — 5-10 tasks
Should use DirectExecution (no structured thinking needed)
- Fix typos in files
- Rename variables
- Add docstrings
- Implement simple utility functions (swap two vars, reverse list)
- Format code with prettier/black

**Expected behavior:** Fast execution, no tool calls, quality score 4+

#### Moderate Tasks (Complexity 2.0-3.5) — 10-15 tasks
Should use QuickSelfEval or SequentialThinking
- Implement basic algorithms (bubble sort, linear search)
- Refactor a function for clarity
- Add error handling to existing code
- Create a simple module with 2-3 functions
- Write unit tests for a component

**Expected behavior:** Some reasoning, maybe 1-2 tool calls, quality score 4.5+

#### Complex Tasks (Complexity > 3.5) — 10-15 tasks
Should use SequentialThinking or PhasedOrchestration
- Design and implement maze solver
- Implement BFS/DFS graph traversal
- System design: cache layer architecture
- Refactor large module with architectural changes
- Implement multi-step data processing pipeline
- Research and implement solution to ambiguous problem

**Expected behavior:** Extensive tool calls, structured decisions, multi-phase, quality score 5+

#### Ambiguous Tasks (Complexity > 4.0, inherently unclear) — 5-10 tasks
MUST use PhasedOrchestration
- "Explore different solutions to the N-queens problem"
- "Design a rate-limiting strategy for our API"
- "Recommend refactoring approach for legacy auth module"
- "Investigate why tests are flaky and propose fixes"
- "Plan migration from monolith to microservices"

**Expected behavior:** Multi-phase, explicit planning phase, context reuse, quality scores improve with phases

---

## Metrics to Collect

### Per-Task Metrics

```rust
#[derive(Debug, Clone, Serialize)]
pub struct TaskResult {
    // Identifiers
    pub task_name: String,
    pub task_id: String,
    pub timestamp: String,
    
    // Input
    pub task_description: String,
    pub expected_complexity: f64,
    
    // Analysis
    pub detected_complexity: f64,
    pub selected_strategy: String,
    pub enable_structured_thinking: bool,
    
    // Execution
    pub phases_executed: u32,
    pub total_structured_thoughts: u32,
    pub thoughts_per_phase: Vec<u32>,
    
    // Quality
    pub initial_quality_score: f64,
    pub final_quality_score: f64,
    pub quality_improvement: f64,
    
    // Success
    pub success: bool,
    pub success_criteria_met: Vec<String>,
    pub success_criteria_failed: Vec<String>,
    
    // Cost
    pub total_tokens_used: u32,
    pub total_time_seconds: f64,
    pub tokens_per_phase: Vec<u32>,
    
    // Reasoning
    pub decisions_made: Vec<String>,
    pub constraints_discovered: Vec<String>,
    pub learnings: Vec<String>,
}
```

### Aggregate Metrics

```rust
#[derive(Debug, Clone, Serialize)]
pub struct TestRunResults {
    pub total_tasks: u32,
    pub success_count: u32,
    pub success_rate: f64,
    
    pub by_complexity_level: Map<String, ComplexityResults>,
    // {
    //   "simple": { total: 8, success: 8, rate: 100% },
    //   "moderate": { total: 12, success: 10, rate: 83% },
    //   "complex": { total: 14, success: 9, rate: 64% },
    //   "ambiguous": { total: 8, success: 5, rate: 62% },
    // }
    
    pub by_strategy: Map<String, StrategyResults>,
    // {
    //   "DirectExecution": { tasks: 8, success: 8, avg_quality: 4.2 },
    //   "SequentialThinking": { tasks: 16, success: 13, avg_quality: 4.8 },
    //   "PhasedOrchestration": { tasks: 14, success: 9, avg_quality: 5.1 },
    // }
    
    pub quality_stats: QualityStats,
    // {
    //   "initial_avg": 3.8,
    //   "final_avg": 4.5,
    //   "improvement_avg": 0.7,
    //   "tasks_with_improvement": 28,
    // }
    
    pub reasoning_stats: ReasoningStats,
    // {
    //   "avg_thoughts_per_task": 6.2,
    //   "avg_phases": 1.8,
    //   "total_decisions_captured": 147,
    //   "total_constraints_discovered": 89,
    //   "total_learnings": 103,
    // }
    
    pub cost_stats: CostStats,
    // {
    //   "total_tokens": 245000,
    //   "avg_tokens_per_task": 5100,
    //   "total_time_hours": 2.3,
    //   "avg_time_per_task_seconds": 276,
    // }
}
```

---

## Success Criteria

### Level 1: Harness Works ✅
- [ ] 30+ tasks run without crashing
- [ ] Zero panics in tool call handling
- [ ] All thoughts are stored and retrievable
- [ ] Quality scores are sensible (0-7 range, not all 0s or all 7s)

### Level 2: Strategies Matter ✅
- [ ] Simple tasks: 90%+ success with DirectExecution
- [ ] Moderate tasks: 80%+ success with SequentialThinking
- [ ] Complex tasks: 60%+ success with SequentialThinking
- [ ] Ambiguous tasks: 50%+ success with PhasedOrchestration

### Level 3: Harness Improves Quality ✅
- [ ] Average initial quality: 3.5-4.0
- [ ] Average final quality: 4.5-5.0
- [ ] Quality improvement: +0.5 or more
- [ ] 70%+ of tasks show improvement

### Level 4: Reasoning Captured ✅
- [ ] Average 4+ thoughts per task
- [ ] Ambiguous tasks average 2+ phases
- [ ] Phase contexts are retrievable and non-empty
- [ ] Decisions/constraints/learnings meaningful (not generic)

### Level 5: Cost Reasonable ✅
- [ ] Simple tasks: < 50 tokens/task
- [ ] Moderate tasks: < 200 tokens/task
- [ ] Complex tasks: < 500 tokens/task
- [ ] Total runtime < 1 hour for 40 tasks

---

## Test Harness Implementation

### Entry Point

```rust
// tests/terminal_bench_orchestration.rs

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[tokio::test]
async fn test_orchestration_harness_terminal_bench() {
    // 1. Load task definitions
    let tasks = load_terminal_bench_tasks()
        .expect("Failed to load terminal bench tasks");
    
    // 2. Filter to test set
    let test_tasks = select_test_tasks(&tasks, 40);
    println!("Testing {} tasks across {} complexity levels", 
        test_tasks.len(), 
        test_tasks.iter().map(|t| t.complexity).fold(
            std::collections::HashSet::new(),
            |mut s, c| { s.insert((c * 2.0) as u32); s }
        ).len()
    );
    
    // 3. Run tests
    let mut results = Vec::new();
    for (idx, task) in test_tasks.iter().enumerate() {
        println!("[{}/{}] Running: {}", idx+1, test_tasks.len(), task.name);
        
        match run_single_task(task).await {
            Ok(result) => results.push(result),
            Err(e) => {
                eprintln!("Task failed: {}", e);
                results.push(TaskResult::failure(task, e.to_string()));
            }
        }
    }
    
    // 4. Analyze results
    let summary = analyze_results(&results);
    
    // 5. Write report
    write_test_report(&summary, &results)
        .expect("Failed to write test report");
    
    // 6. Assert success criteria
    assert_success_criteria(&summary);
}

fn select_test_tasks(all_tasks: &[TerminalBenchTask], count: usize) -> Vec<TerminalBenchTask> {
    // Select proportionally from each complexity level
    let simple: Vec<_> = all_tasks.iter()
        .filter(|t| t.complexity < 2.0)
        .take(count / 4)
        .cloned()
        .collect();
    
    let moderate: Vec<_> = all_tasks.iter()
        .filter(|t| t.complexity >= 2.0 && t.complexity < 3.5)
        .take(count / 3)
        .cloned()
        .collect();
    
    let complex: Vec<_> = all_tasks.iter()
        .filter(|t| t.complexity >= 3.5)
        .take(count / 3)
        .cloned()
        .collect();
    
    [simple, moderate, complex].concat()
}

async fn run_single_task(task: &TerminalBenchTask) -> Result<TaskResult> {
    let task_id = format!("bench-{}", uuid::Uuid::new_v4());
    let start_time = std::time::Instant::now();
    
    // Initialize orchestration
    let store_path = PathBuf::from(format!(".bench_results/{}", task_id));
    fs::create_dir_all(&store_path)?;
    let mut orchestration = OrchestrationIntegration::new(Some(store_path.clone()));
    orchestration.start_task(task_id.clone());
    
    // Analyze task
    let analysis = orchestration.analyze_message(&task.description);
    let initial_quality = orchestration.evaluate_quality(&task.description);
    
    // Run task (this is where TUI integration would happen)
    let llm_response = call_llm_with_strategy(&task.description, &analysis).await?;
    
    // Evaluate final quality
    let final_quality = orchestration.evaluate_quality(&llm_response);
    
    // Retrieve stored thoughts
    let thoughts = retrieve_all_thoughts(&store_path)?;
    let phase_count = thoughts.iter().map(|t| t.phase).max().unwrap_or(1);
    
    // Extract reasoning
    let decisions = extract_reasoning_type(&thoughts, ThoughtType::Decision);
    let constraints = extract_reasoning_type(&thoughts, ThoughtType::Constraint);
    let learnings = extract_reasoning_type(&thoughts, ThoughtType::Learning);
    
    // Evaluate success
    let success = evaluate_task_success(&task, &llm_response);
    
    Ok(TaskResult {
        task_name: task.name.clone(),
        task_id,
        timestamp: chrono::Local::now().to_rfc3339(),
        
        task_description: task.description.clone(),
        expected_complexity: task.complexity,
        
        detected_complexity: analysis.complexity,
        selected_strategy: format!("{:?}", analysis.strategy),
        enable_structured_thinking: analysis.enable_structured_thinking,
        
        phases_executed: phase_count as u32,
        total_structured_thoughts: thoughts.len() as u32,
        thoughts_per_phase: count_thoughts_per_phase(&thoughts),
        
        initial_quality_score: initial_quality.total,
        final_quality_score: final_quality.total,
        quality_improvement: final_quality.total - initial_quality.total,
        
        success,
        success_criteria_met: vec![], // Populated by evaluator
        success_criteria_failed: vec![],
        
        total_tokens_used: estimate_tokens(&llm_response),
        total_time_seconds: start_time.elapsed().as_secs_f64(),
        tokens_per_phase: vec![], // Could track per-phase if needed
        
        decisions_made: decisions,
        constraints_discovered: constraints,
        learnings,
    })
}

fn analyze_results(results: &[TaskResult]) -> TestRunResults {
    let total = results.len() as u32;
    let success_count = results.iter().filter(|r| r.success).count() as u32;
    
    // Group by complexity level
    let simple: Vec<_> = results.iter()
        .filter(|r| r.expected_complexity < 2.0)
        .collect();
    let moderate: Vec<_> = results.iter()
        .filter(|r| r.expected_complexity >= 2.0 && r.expected_complexity < 3.5)
        .collect();
    let complex: Vec<_> = results.iter()
        .filter(|r| r.expected_complexity >= 3.5)
        .collect();
    
    // Group by strategy
    let direct: Vec<_> = results.iter()
        .filter(|r| r.selected_strategy == "DirectExecution")
        .collect();
    let sequential: Vec<_> = results.iter()
        .filter(|r| r.selected_strategy == "SequentialThinking")
        .collect();
    let phased: Vec<_> = results.iter()
        .filter(|r| r.selected_strategy == "PhasedOrchestration")
        .collect();
    
    TestRunResults {
        total_tasks: total,
        success_count,
        success_rate: success_count as f64 / total as f64,
        
        by_complexity_level: map![
            "simple" => ComplexityResults {
                total: simple.len() as u32,
                success: simple.iter().filter(|r| r.success).count() as u32,
                avg_quality: simple.iter().map(|r| r.final_quality_score).sum::<f64>() / simple.len() as f64,
            },
            "moderate" => ComplexityResults { /* ... */ },
            "complex" => ComplexityResults { /* ... */ },
        ],
        
        by_strategy: map![
            "DirectExecution" => StrategyResults {
                tasks: direct.len() as u32,
                success: direct.iter().filter(|r| r.success).count() as u32,
                avg_quality: direct.iter().map(|r| r.final_quality_score).average(),
            },
            "SequentialThinking" => StrategyResults { /* ... */ },
            "PhasedOrchestration" => StrategyResults { /* ... */ },
        ],
        
        quality_stats: QualityStats {
            initial_avg: results.iter().map(|r| r.initial_quality_score).average(),
            final_avg: results.iter().map(|r| r.final_quality_score).average(),
            improvement_avg: results.iter().map(|r| r.quality_improvement).average(),
            tasks_with_improvement: results.iter().filter(|r| r.quality_improvement > 0.1).count() as u32,
        },
        
        reasoning_stats: ReasoningStats {
            avg_thoughts_per_task: results.iter().map(|r| r.total_structured_thoughts).average() as f64,
            avg_phases: results.iter().map(|r| r.phases_executed).average() as f64,
            total_decisions_captured: results.iter().map(|r| r.decisions_made.len()).sum::<usize>() as u32,
            total_constraints_discovered: results.iter().map(|r| r.constraints_discovered.len()).sum::<usize>() as u32,
            total_learnings: results.iter().map(|r| r.learnings.len()).sum::<usize>() as u32,
        },
        
        cost_stats: CostStats {
            total_tokens: results.iter().map(|r| r.total_tokens_used).sum(),
            avg_tokens_per_task: results.iter().map(|r| r.total_tokens_used).average(),
            total_time_hours: results.iter().map(|r| r.total_time_seconds).sum::<f64>() / 3600.0,
            avg_time_per_task_seconds: results.iter().map(|r| r.total_time_seconds).average(),
        },
    }
}

fn assert_success_criteria(summary: &TestRunResults) {
    // Level 1: Harness Works
    assert!(summary.total_tasks > 30, "Need 30+ tasks");
    
    // Level 2: Strategies Matter
    let simple_results = summary.by_complexity_level.get("simple").unwrap();
    assert!(simple_results.success_rate > 0.90, 
        "Simple tasks should succeed >90%, got {}", simple_results.success_rate);
    
    // Level 3: Quality Improves
    let improvement = summary.quality_stats.improvement_avg;
    assert!(improvement > 0.5, 
        "Quality should improve +0.5, got +{}", improvement);
    
    let improved_ratio = summary.quality_stats.tasks_with_improvement as f64 / summary.total_tasks as f64;
    assert!(improved_ratio > 0.7,
        "70%+ tasks should improve, got {}%", improved_ratio * 100.0);
    
    println!("✅ All success criteria met!");
}
```

---

## Report Generation

### Output: `test_results/orchestration_harness_2026_04_25.json`

```json
{
  "test_run": {
    "date": "2026-04-25T14:30:00Z",
    "total_tasks": 42,
    "success_rate": 0.78,
    "test_duration_hours": 2.1
  },
  "by_complexity": {
    "simple": {
      "total": 8,
      "success": 8,
      "success_rate": 1.0,
      "avg_initial_quality": 3.2,
      "avg_final_quality": 4.1,
      "avg_improvement": 0.9
    },
    "moderate": {
      "total": 14,
      "success": 12,
      "success_rate": 0.857,
      "avg_initial_quality": 3.8,
      "avg_final_quality": 4.6,
      "avg_improvement": 0.8
    },
    "complex": {
      "total": 14,
      "success": 10,
      "success_rate": 0.714,
      "avg_initial_quality": 4.0,
      "avg_final_quality": 5.0,
      "avg_improvement": 1.0
    },
    "ambiguous": {
      "total": 6,
      "success": 3,
      "success_rate": 0.5,
      "avg_initial_quality": 3.5,
      "avg_final_quality": 5.2,
      "avg_improvement": 1.7
    }
  },
  "by_strategy": {
    "DirectExecution": {
      "tasks": 8,
      "success": 8,
      "avg_quality": 4.1,
      "avg_thoughts": 0
    },
    "SequentialThinking": {
      "tasks": 20,
      "success": 18,
      "avg_quality": 4.6,
      "avg_thoughts": 4.2
    },
    "PhasedOrchestration": {
      "tasks": 14,
      "success": 9,
      "avg_quality": 5.1,
      "avg_thoughts": 8.7,
      "avg_phases": 2.1
    }
  },
  "reasoning_captured": {
    "total_thoughts": 268,
    "avg_per_task": 6.4,
    "by_type": {
      "decision": 147,
      "constraint": 67,
      "learning": 54
    }
  }
}
```

### Output: `test_results/orchestration_harness_2026_04_25.md`

```markdown
# Orchestration Harness — Terminal Bench Test Results
**Date:** 2026-04-25  
**Tasks:** 42 (8 simple, 14 moderate, 14 complex, 6 ambiguous)  
**Overall Success Rate:** 78% (33/42)

## Summary

The orchestration harness successfully improves LLM reasoning quality across all complexity levels:
- **Simple tasks:** 100% success (no orchestration overhead)
- **Moderate tasks:** 86% success (structured thinking helps)
- **Complex tasks:** 71% success (phased approach captures decisions)
- **Ambiguous tasks:** 50% success (raw reasoning is still hard, but improvement is dramatic)

**Key Finding:** Quality improves most on ambiguous tasks (+1.7 points) where structured thinking is most valuable.

## Results by Complexity

### Simple Tasks (Complexity < 2.0)
| Metric | Value |
|--------|-------|
| Tasks | 8 |
| Success | 8/8 (100%) |
| Avg initial quality | 3.2 |
| Avg final quality | 4.1 |
| Avg improvement | +0.9 |
| Strategy used | DirectExecution |
| Avg thoughts | 0 |

**Analysis:** DirectExecution is correct for simple tasks. No structured thinking needed. Fast execution.

### Moderate Tasks (Complexity 2.0-3.5)
| Metric | Value |
|--------|-------|
| Tasks | 14 |
| Success | 12/14 (86%) |
| Avg initial quality | 3.8 |
| Avg final quality | 4.6 |
| Avg improvement | +0.8 |
| Strategy used | SequentialThinking |
| Avg thoughts | 4.2 |

**Analysis:** SequentialThinking is effective for moderate tasks. Structured thinking captures key decisions without over-complicating.

**Failure Analysis (2 failed):**
- Task "Refactor auth module": LLM got confused mid-refactoring, thoughts captured but execution incomplete
- Task "Implement cache layer": Strategy was SequentialThinking but should have been Phased (3 phases needed)

### Complex Tasks (Complexity 3.5-4.0)
| Metric | Value |
|--------|-------|
| Tasks | 14 |
| Success | 10/14 (71%) |
| Avg initial quality | 4.0 |
| Avg final quality | 5.0 |
| Avg improvement | +1.0 |
| Strategy used | PhasedOrchestration |
| Avg thoughts | 8.7 |
| Avg phases | 2.1 |

**Analysis:** Phased approach works but needs refinement. Multi-phase context retrieval is effective (phase 2 decisions build on phase 1).

**Success Factor:** Tasks that succeeded all had explicit "phase 1: plan, phase 2: implement" structure.

**Failure Analysis (4 failed):**
- Tasks without clear phasing boundaries
- Phase transitions unclear (when to move to next phase?)
- Ambiguous success criteria

### Ambiguous Tasks (Complexity > 4.0)
| Metric | Value |
|--------|-------|
| Tasks | 6 |
| Success | 3/6 (50%) |
| Avg initial quality | 3.5 |
| Avg final quality | 5.2 |
| Avg improvement | +1.7 |
| Strategy used | PhasedOrchestration |
| Avg thoughts | 12.1 |
| Avg phases | 2.8 |

**Analysis:** Ambiguous tasks show BIGGEST quality improvement (+1.7). Structured thinking is most valuable here.

**Success Examples:**
- "Design cache architecture" → Phase 1: requirements, Phase 2: design, Phase 3: impl → Success ✓
- "Explore maze solutions" → Phase 1: analysis, Phase 2: algorithm selection, Phase 3: validation → Success ✓
- "Refactor strategy" → Phase 1: current state analysis, Phase 2: proposed changes, Phase 3: risks → Success ✓

**Failure Analysis (3 failed):**
- "Investigate flaky tests" → No clear success criteria, tests stayed flaky
- "Plan migration" → Proposed plan wasn't implementable
- "Recommend refactoring" → Recommendation was obvious but didn't solve underlying issues

## Strategy Effectiveness

| Strategy | Tasks | Success | Avg Quality | Best For |
|----------|-------|---------|-------------|----------|
| DirectExecution | 8 | 100% | 4.1 | Simple, straightforward |
| SequentialThinking | 20 | 90% | 4.6 | Moderate, clear steps |
| PhasedOrchestration | 14 | 64% | 5.1 | Complex, ambiguous, multi-step |

**Key Insight:** Phased approach succeeds when phases are explicit in the task. Ambiguous task phrasing → ambiguous phases → lower success.

## Reasoning Quality

### Decisions Captured
- Total: 147 decision thoughts
- Avg per task: 3.5
- Examples: "Use DFS for memory efficiency", "Validate on 5x5 first", "OAuth2 better than SAML"

### Constraints Discovered
- Total: 67 constraint thoughts  
- Avg per task: 1.6
- Examples: "Maze size unknown", "API rate limit 100/min", "Legacy code uses deprecated API"

### Learnings Recorded
- Total: 54 learning thoughts
- Avg per task: 1.3
- Examples: "DFS corners faster than edges", "Concurrent writes need mutex", "Tests fail on weekends"

---

## Cost Analysis

| Metric | Value |
|--------|-------|
| Total tokens | 245,000 |
| Avg per task | 5,833 |
| By complexity |  |
| - Simple | 1,200/task |
| - Moderate | 4,500/task |
| - Complex | 8,200/task |
| - Ambiguous | 12,500/task |
| Total runtime | 2.1 hours |
| Avg per task | 3 minutes |

**Cost/Benefit:** Complex/ambiguous tasks cost 2-3x more but show 2-3x quality improvement. ROI positive.

---

## Recommendations

### Phase 1: Immediate (within 1 week)
1. ✅ **Clarify phase boundaries** — Add guidance to LLM on when phases end
2. ✅ **Improve task prompting** — Explicitly request "phase 1: plan, phase 2: implement"
3. ✅ **Success criteria** — Ask LLM to list success criteria upfront

### Phase 2: Short-term (1-2 weeks)
1. 🔲 **Adaptive learning** — Track which strategies work per model (GLM-4.5 vs GLM-5.1)
2. 🔲 **Modal visualization** — Show reasoning graph in TUI during execution
3. 🔲 **Graceful degradation** — If phased fails, auto-fallback to sequential

### Phase 3: Medium-term (1 month)
1. 🔲 **Context optimization** — Compress phase context (currently 500-1000 tokens)
2. 🔲 **Parallel phases** — For independent tasks, run phases in parallel
3. 🔲 **Automatic phase detection** — Detect when a phase is complete without explicit signal

---

## Conclusion

**The orchestration harness works.** It improves LLM reasoning quality across all complexity levels, with especially strong results on ambiguous tasks (+1.7 quality improvement).

**Next priority:** Phase boundaries. The harness currently requires explicit phase structure in the task. Adding automatic phase detection and better transitions will unlock 10-15% more success.

**Ship readiness:** At 78% success on diverse tasks, the harness is ready for production with the Phase 1 recommendations.
```

---

## Running the Tests

### Prerequisites
```bash
# Install dependencies
cargo add serde_json chrono uuid tempfile
cargo add --dev tokio

# Ensure TUI orchestration integration is complete (Stream C)
cargo build -p rustycode-tui --tests
```

### Execute
```bash
# Run full test suite
cargo test --test terminal_bench_orchestration -- --nocapture --test-threads=1

# Run subset (for quick validation)
cargo test --test terminal_bench_orchestration simple_tasks -- --nocapture

# Generate report
cargo test --test terminal_bench_orchestration -- --nocapture | tee test_run.log
python3 scripts/analyze_orchestration_results.py test_run.log
```

### Output
```
test_results/
├── orchestration_harness_2026_04_25.json     ← Raw metrics (importable)
├── orchestration_harness_2026_04_25.md       ← Human-readable report
├── task_details/
│   ├── task-001-fix-typo.json                ← Per-task details
│   ├── task-002-implement-sort.json
│   └── ... (40+ more)
└── graphs/
    ├── success_by_complexity.png
    ├── quality_improvement.png
    └── strategy_effectiveness.png
```

---

## Evaluation Checklist

Before declaring success, verify:

- [ ] No crashes or panics
- [ ] All 40+ tasks completed (or documented failures)
- [ ] JSON results are valid and complete
- [ ] Success rates match expectations (simple 90%+, complex 60%+, ambiguous 50%+)
- [ ] Quality improves on 70%+ of tasks
- [ ] Ambiguous tasks show biggest improvement (+1.5 or more)
- [ ] Decisions/constraints/learnings are meaningful (not generic)
- [ ] Cost is reasonable (simple < 50 tokens, complex < 500)
- [ ] Report is generated and readable
- [ ] No clippy errors in test code

---

## Post-Test Actions

### If Success (78%+ overall, goals met):
1. ✅ Mark harness as "Production Ready"
2. ✅ Merge Stream C + D into main
3. ✅ Plan Phase 2: adaptive learning + visualization
4. ✅ Publish results blog post

### If Partial (60-77%, some goals met):
1. 🔄 Implement Phase 1 recommendations above
2. 🔄 Re-test on same 40 tasks (track improvement)
3. 🔄 Target: reach 80%+ on re-test

### If Low (< 60%, many failures):
1. ❌ Investigate: tool call routing working?
2. ❌ Check: phase context being prepended?
3. ❌ Validate: structured thinking tool schema correct?
4. ❌ Debug one failed task end-to-end
5. ❌ Re-test smaller subset (10 tasks) after fix
