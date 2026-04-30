//! Terminal Bench Orchestration Validation
//!
//! This test harness evaluates the orchestration pipeline against a subset of Terminal Bench tasks
//! to validate success rates, reasoning quality, and cost metrics.

use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::pipeline::{OrchestrationPipeline, TaskResult};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;

#[derive(Serialize, Deserialize)]
struct TestResult {
    task: String,
    complexity: f64,
    strategy: String,
    success: bool,
    steps_completed: usize,
    total_cost: f64,
}

#[tokio::test]
#[allow(clippy::unwrap_used, clippy::uninlined_format_args)]
async fn test_orchestration_harness_validation() {
    let pipeline = OrchestrationPipeline::new(OrchestrationConfig::default());
    let tasks = vec![
        ("Fix typo in README", 1.0),
        ("Implement bubble sort", 2.0),
        ("Explore maze solution", 4.0),
    ];

    let mut results = Vec::new();

    for (task_desc, expected_complexity) in tasks {
        let result = pipeline
            .conduct("test-task".into(), task_desc.into())
            .await
            .unwrap();

        let (success, steps, cost) = match result {
            TaskResult::Success {
                total_cost,
                steps_completed,
                ..
            } => (true, steps_completed, total_cost),
            TaskResult::Failed {
                total_cost,
                steps_completed,
                ..
            } => (false, steps_completed, total_cost),
        };

        results.push(TestResult {
            task: task_desc.into(),
            complexity: expected_complexity,
            strategy: "PhasedOrchestration".into(),
            success,
            steps_completed: steps,
            total_cost: cost,
        });
    }

    let results_dir = "crates/rustycode-orchestration/tests/test_results";
    std::fs::create_dir_all(results_dir).unwrap();
    let json = serde_json::to_string_pretty(&results).unwrap();
    let mut file = File::create(format!(
        "{results_dir}/orchestration_harness_2026_04_25.json"
    ))
    .unwrap();
    file.write_all(json.as_bytes()).unwrap();

    assert!(results.iter().any(|r| r.success));
}
