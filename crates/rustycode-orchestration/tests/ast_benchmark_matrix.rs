//! AST Benchmark Matrix — 13 tasks × 3 complexity levels × 3 repetitions.
//!
//! Exercises all three complexity paths (Trivial / Moderate / Complex) through
//! the full AST pipeline using a `CapturingRunner` mock (no real LLM calls).
//! Measures classification accuracy, route selection, step count, and wall-clock
//! time. Asserts determinism: identical input always produces identical output.

#![allow(
    unknown_lints,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::print_literal,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::ptr_arg,
    clippy::format_in_format_args,
    clippy::let_and_return,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::let_unit_value,
    clippy::unused_async,
    clippy::doc_markdown,
    clippy::unnecessary_lazy_evaluations
)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rustycode_orchestration::ast::{
    AstConfig, AstPipeline, ComplexityLevel, ExecutionStep, PhaseRoute, StepEvidence, StepRunner,
    ToolHarness, VerificationStatus,
};

// Capturing runner — records every step execution for metrics

/// A `StepRunner` that records calls and returns simulated success results.
struct CapturingRunner {
    calls: Arc<Mutex<Vec<(usize, String)>>>,
    exit_codes: HashMap<String, i32>,
}

impl CapturingRunner {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
            exit_codes: HashMap::new(),
        }
    }
}

impl StepRunner for CapturingRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        self.calls
            .lock()
            .unwrap()
            .push((step_index, step.action.clone()));

        let exit_code = self.exit_codes.get(&step.action).copied().unwrap_or(0);
        StepEvidence {
            step_index,
            command_run: step.expected_command.clone(),
            exit_code,
            stdout_summary: if exit_code == 0 {
                "ok".into()
            } else {
                String::new()
            },
            stderr_summary: if exit_code != 0 {
                "error".into()
            } else {
                String::new()
            },
            changed_files: if exit_code == 0 {
                step.file_targets.clone()
            } else {
                vec![]
            },
            verification_passed: step.verification_command.as_ref().map(|_| exit_code == 0),
        }
    }
}

// Helpers

fn tmp_dir() -> PathBuf {
    tempfile::tempdir().unwrap().path().to_path_buf()
}

fn pipeline_config(workspace: &PathBuf) -> AstConfig {
    AstConfig {
        ledger_dir: workspace.join(".ast"),
        skip_research_for_trivial: true,
        rolling_wave_batch_size: 2,
        max_recovery_retries: 2,
        harness: ToolHarness::ClaudeCode,
        controller: None,
    }
}

// Benchmark task definitions (13 tasks, 3 complexity levels)

/// A single benchmark task with expected classification.
struct BenchmarkTask {
    id: &'static str,
    description: &'static str,
    expected_complexity: ComplexityLevel,
}

/// Metrics collected from a single pipeline run.
struct BenchmarkResult {
    task_id: String,
    repetition: usize,
    expected_complexity: ComplexityLevel,
    actual_complexity: ComplexityLevel,
    classification_matched: bool,
    route: PhaseRoute,
    steps_executed: usize,
    milestones_created: usize,
    elapsed_ms: u128,
    success: bool,
}

/// 13 benchmark tasks designed to trigger predictable complexity classification.
///
/// Keyword choices are aligned with the classifier in `classifier.rs`:
/// - **Trivial**: "typo", "rename", "bump version", "fix lint" (≤ 8 words)
/// - **Moderate**: "add unit", "handle", "extend", "write test", "add support"
/// - **Complex**: "implement", "refactor", "migrate", "integrate"
const BENCHMARK_TASKS: &[BenchmarkTask] = &[
    // ── Trivial tasks (4): simple, single-file, no ambiguity ──────────────
    BenchmarkTask {
        id: "T01",
        description: "Fix typo in the README",
        expected_complexity: ComplexityLevel::Trivial,
    },
    BenchmarkTask {
        id: "T02",
        description: "Rename variable x to count",
        expected_complexity: ComplexityLevel::Trivial,
    },
    BenchmarkTask {
        id: "T03",
        description: "Bump version to 2.1",
        expected_complexity: ComplexityLevel::Trivial,
    },
    BenchmarkTask {
        id: "T04",
        description: "Update comment in main",
        expected_complexity: ComplexityLevel::Trivial,
    },
    // ── Moderate tasks (5): multi-step, some planning needed ──────────────
    BenchmarkTask {
        id: "M01",
        description: "Add unit tests for the parser module with edge cases",
        expected_complexity: ComplexityLevel::Moderate,
    },
    BenchmarkTask {
        id: "M02",
        description: "Handle errors for the API client with retries",
        expected_complexity: ComplexityLevel::Moderate,
    },
    BenchmarkTask {
        id: "M03",
        description: "Extend the database connection pool to support async",
        expected_complexity: ComplexityLevel::Moderate,
    },
    BenchmarkTask {
        id: "M04",
        description: "Write test for the session handler module",
        expected_complexity: ComplexityLevel::Moderate,
    },
    BenchmarkTask {
        id: "M05",
        description: "Add support for listing active sessions",
        expected_complexity: ComplexityLevel::Moderate,
    },
    // ── Complex tasks (4): multi-module, architectural decisions ──────────
    BenchmarkTask {
        id: "C01",
        description: "Implement a plugin system for extending the tool registry with dynamic loading",
        expected_complexity: ComplexityLevel::Complex,
    },
    BenchmarkTask {
        id: "C02",
        description: "Refactor the data pipeline to support multi-agent orchestration and dependency resolution",
        expected_complexity: ComplexityLevel::Complex,
    },
    BenchmarkTask {
        id: "C03",
        description: "Migrate the caching layer to a distributed system with consistency guarantees",
        expected_complexity: ComplexityLevel::Complex,
    },
    BenchmarkTask {
        id: "C04",
        description: "Integrate a security audit system that detects vulnerabilities and generates reports",
        expected_complexity: ComplexityLevel::Complex,
    },
];

// Main benchmark matrix test

#[test]
fn benchmark_matrix_13_tasks_3_repetitions() {
    let mut results: Vec<BenchmarkResult> = Vec::new();

    for task in BENCHMARK_TASKS {
        for rep in 0..3 {
            let workspace = tmp_dir();
            let config = pipeline_config(&workspace);
            let runner = CapturingRunner::new();
            let step_tracker = runner.calls.clone();

            let mut pipeline = AstPipeline::with_runner(config, workspace, runner);

            let start = std::time::Instant::now();
            let run_result = pipeline.run(task.description);
            let elapsed = start.elapsed();

            let snapshot = pipeline.snapshot();

            let (actual_complexity, route, steps, milestones, success) = match run_result {
                Ok(_status) => {
                    let assessment = snapshot.assessment.as_ref();
                    let complexity = assessment
                        .map(|a| a.complexity)
                        .unwrap_or(ComplexityLevel::Trivial);
                    let route = assessment
                        .map(|a| a.route.clone())
                        .unwrap_or(PhaseRoute::DirectExecute);
                    let steps = step_tracker.lock().unwrap().len();
                    let milestones = snapshot.completed_milestones.len();
                    (complexity, route, steps, milestones, true)
                }
                Err(_) => {
                    let steps = step_tracker.lock().unwrap().len();
                    (
                        ComplexityLevel::Trivial,
                        PhaseRoute::DirectExecute,
                        steps,
                        0,
                        false,
                    )
                }
            };

            let classification_matched = matches!(
                (&task.expected_complexity, &actual_complexity),
                (ComplexityLevel::Trivial, ComplexityLevel::Trivial)
                    | (ComplexityLevel::Moderate, ComplexityLevel::Moderate)
                    | (ComplexityLevel::Complex, ComplexityLevel::Complex)
            );

            results.push(BenchmarkResult {
                task_id: task.id.to_string(),
                repetition: rep,
                expected_complexity: task.expected_complexity,
                actual_complexity,
                classification_matched,
                route,
                steps_executed: steps,
                milestones_created: milestones,
                elapsed_ms: elapsed.as_millis(),
                success,
            });
        }
    }

    // ── Print summary table ───────────────────────────────────────────────
    println!(
        "\n=== AST Benchmark Matrix Results ({}/39 runs) ===",
        results.len()
    );
    println!(
        "{:<5} {:<4} {:<9} {:<9} {:<6} {:<6} {:<6} {:<17} {}",
        "Task", "Rep", "Expected", "Actual", "Match", "Steps", "Miles", "Route", "OK"
    );
    for r in &results {
        println!(
            "{:<5} {:<4} {:<9?} {:<9?} {:<6} {:<6} {:<6} {:<17?} {}",
            r.task_id,
            r.repetition,
            r.expected_complexity,
            r.actual_complexity,
            r.classification_matched,
            r.steps_executed,
            r.milestones_created,
            r.route,
            r.success
        );
    }

    // ── Aggregate stats ───────────────────────────────────────────────────
    let total = results.len();
    let successes: usize = results.iter().filter(|r| r.success).count();
    let matches: usize = results.iter().filter(|r| r.classification_matched).count();
    let avg_steps = results.iter().map(|r| r.steps_executed).sum::<usize>() as f64 / total as f64;
    let avg_ms = results.iter().map(|r| r.elapsed_ms as f64).sum::<f64>() / total as f64;

    println!(
        "\nSummary: {}/{} succeeded, {}/{} classified correctly, avg {:.1} steps, avg {:.1}ms",
        successes, total, matches, total, avg_steps, avg_ms
    );

    // ── Assertions ────────────────────────────────────────────────────────
    assert_eq!(
        results.len(),
        39,
        "Should have 39 total runs (13 tasks × 3 reps)"
    );

    // Classification must be deterministic for same task
    for task in BENCHMARK_TASKS {
        let task_results: Vec<_> = results.iter().filter(|r| r.task_id == task.id).collect();
        assert_eq!(
            task_results.len(),
            3,
            "Should have 3 reps for task {}",
            task.id
        );

        let first = task_results[0].actual_complexity;
        for r in &task_results[1..] {
            assert_eq!(
                r.actual_complexity, first,
                "Classification must be deterministic for task {} (got {:?} vs {:?})",
                task.id, r.actual_complexity, first
            );
        }
    }
}

// Targeted: trivial tasks use DirectExecute route

#[test]
fn trivial_tasks_use_direct_execute_route() {
    let trivial_tasks: Vec<&BenchmarkTask> = BENCHMARK_TASKS
        .iter()
        .filter(|t| matches!(t.expected_complexity, ComplexityLevel::Trivial))
        .collect();

    assert!(
        !trivial_tasks.is_empty(),
        "Should have at least one trivial task"
    );

    for task in &trivial_tasks {
        let workspace = tmp_dir();
        let config = pipeline_config(&workspace);
        let runner = CapturingRunner::new();
        let mut pipeline = AstPipeline::with_runner(config, workspace, runner);

        let status = pipeline.run(task.description).unwrap();
        assert_eq!(
            status,
            VerificationStatus::Pass,
            "Task {} should pass",
            task.id
        );

        let snapshot = pipeline.snapshot();
        let assessment = snapshot
            .assessment
            .as_ref()
            .unwrap_or_else(|| panic!("Task {} should have an assessment", task.id));
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Trivial,
            "Task {} should classify as Trivial",
            task.id
        );
        assert_eq!(
            assessment.route,
            PhaseRoute::DirectExecute,
            "Task {} (Trivial) should use DirectExecute route",
            task.id
        );
    }
}

// Targeted: complex tasks use RollingWave route

#[test]
fn complex_tasks_use_rolling_wave_route() {
    let complex_tasks: Vec<&BenchmarkTask> = BENCHMARK_TASKS
        .iter()
        .filter(|t| matches!(t.expected_complexity, ComplexityLevel::Complex))
        .collect();

    assert!(
        !complex_tasks.is_empty(),
        "Should have at least one complex task"
    );

    for task in &complex_tasks {
        let workspace = tmp_dir();
        let config = pipeline_config(&workspace);
        let runner = CapturingRunner::new();
        let mut pipeline = AstPipeline::with_runner(config, workspace, runner);

        let status = pipeline.run(task.description).unwrap();
        assert_eq!(
            status,
            VerificationStatus::Pass,
            "Task {} should pass",
            task.id
        );

        let snapshot = pipeline.snapshot();
        let assessment = snapshot
            .assessment
            .as_ref()
            .unwrap_or_else(|| panic!("Task {} should have an assessment", task.id));
        assert_eq!(
            assessment.complexity,
            ComplexityLevel::Complex,
            "Task {} should classify as Complex",
            task.id
        );
        assert_eq!(
            assessment.route,
            PhaseRoute::RollingWave,
            "Task {} (Complex) should use RollingWave route",
            task.id
        );
    }
}

// Targeted: classification is deterministic (10 reps, same result each time)

#[test]
fn classification_is_deterministic() {
    let task = "Fix typo in the README";
    let mut complexities = Vec::new();
    let mut routes = Vec::new();

    for _ in 0..10 {
        let workspace = tmp_dir();
        let mut pipeline = AstPipeline::new(workspace);
        pipeline.classify(task).unwrap();
        let snapshot = pipeline.snapshot();
        let assessment = snapshot.assessment.as_ref().unwrap();
        complexities.push(assessment.complexity);
        routes.push(assessment.route.clone());
    }

    let first_complexity = complexities[0];
    for (i, c) in complexities.iter().enumerate() {
        assert_eq!(
            *c, first_complexity,
            "Repetition {i}: classification must be identical"
        );
    }

    let first_route = routes[0].clone();
    for (i, r) in routes.iter().enumerate() {
        assert_eq!(r, &first_route, "Repetition {i}: route must be identical");
    }

    // Verify it's actually Trivial for this specific input
    assert_eq!(first_complexity, ComplexityLevel::Trivial);
    assert_eq!(first_route, PhaseRoute::DirectExecute);
}
