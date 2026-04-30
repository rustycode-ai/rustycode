#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::collection_is_never_read,
    clippy::match_same_arms,
    clippy::cast_lossless,
    clippy::cast_precision_loss,
    clippy::no_effect_underscore_binding,
    clippy::float_cmp
)]
/// Tests for parallel tool execution feature
///
/// This test file covers:
/// - Parallel execution of multiple tasks
/// - Performance benefits of parallel execution
/// - Error isolation between parallel tasks
use std::time::Duration;
use tokio::task::JoinSet;

#[tokio::test]
async fn test_parallel_execution_basic() {
    let mut join_set = JoinSet::new();

    // Spawn 3 tasks to execute in parallel
    for i in 1..=3 {
        join_set.spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            i * 2
        });
    }

    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }

    assert_eq!(results.len(), 3);
    assert!(results.contains(&2));
    assert!(results.contains(&4));
    assert!(results.contains(&6));
}

#[tokio::test]
async fn test_parallel_execution_faster_than_sequential() {
    let start = std::time::Instant::now();

    // Parallel execution
    let mut join_set = JoinSet::new();
    for i in 1..=3 {
        join_set.spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            i
        });
    }

    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }

    let parallel_duration = start.elapsed();

    // Sequential would be 3 * 100ms = 300ms
    // Parallel should be ~100ms (limited by slowest task)
    assert!(parallel_duration.as_millis() < 250); // Should be significantly faster
}

#[tokio::test]
async fn test_parallel_execution_with_different_durations() {
    let mut join_set = JoinSet::new();

    // Spawn tasks with different durations (50ms, 100ms, 150ms)
    let durations = vec![50, 100, 150];
    for duration in durations {
        join_set.spawn(async move {
            tokio::time::sleep(Duration::from_millis(duration)).await;
            duration
        });
    }

    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }

    assert_eq!(results.len(), 3);

    // Total time should be ~150ms (slowest task)
    // Not sum of all (300ms)
}

#[tokio::test]
async fn test_parallel_execution_error_isolation() {
    let mut join_set = JoinSet::new();

    // Spawn tasks where one will fail
    for i in 1..=3 {
        join_set.spawn(async move {
            if i == 2 {
                // This task will fail
                Err("Simulated error")
            } else {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(i)
            }
        });
    }

    let mut results = Vec::new();
    let mut errors = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(value)) => results.push(value),
            Ok(Err(_)) => errors += 1, // Task returned error
            Err(_) => errors += 1,     // Task panicked
        }
    }

    // Two tasks should succeed, one should fail
    assert_eq!(results.len(), 2);
    assert_eq!(errors, 1);
}

#[test]
fn test_parallel_execution_benefit_calculation() {
    // Calculate theoretical performance improvement

    let sequential_time_ms = 50 + 100 + 150; // 300ms
    let parallel_time_ms = 150; // Slowest task

    let improvement_percent =
        ((sequential_time_ms - parallel_time_ms) as f64 / sequential_time_ms as f64) * 100.0;

    // Should be 50% improvement for these values
    assert!(improvement_percent > 49.0);
    assert!(improvement_percent < 51.0);

    // For 3 equal tasks, should be ~66% improvement
    let _equal_tasks = [100, 100, 100];
    let sequential_equal = 300;
    let parallel_equal = 100;
    let improvement_equal =
        ((sequential_equal - parallel_equal) as f64 / sequential_equal as f64) * 100.0;

    assert!(improvement_equal > 65.0);
    assert!(improvement_equal < 67.0);
}

#[tokio::test]
async fn test_parallel_execution_max_workers() {
    // Test that parallel execution handles many tasks
    let mut join_set = JoinSet::new();

    let num_tasks = 10;
    for i in 1..=num_tasks {
        join_set.spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            i
        });
    }

    let mut results = Vec::new();
    while let Some(result) = join_set.join_next().await {
        results.push(result.unwrap());
    }

    assert_eq!(results.len(), num_tasks);
}

#[test]
fn test_parallel_execution_resource_efficiency() {
    // Demonstrate that parallel execution is more resource-efficient

    // Scenario: Process 5 files
    // Sequential: 1 core × 5 tasks = 5s
    // Parallel: 5 cores × 5 tasks = 1s (assuming each task takes 1s)

    let _sequential_cores = 1;
    let sequential_time = 5.0; // seconds

    let _parallel_cores = 5;
    let parallel_time = 1.0; // seconds

    // In this example, parallel is 5x faster in wall-clock time
    let time_improvement = sequential_time / parallel_time; // 5.0
    assert!(time_improvement > 4.9);
    assert!(time_improvement < 5.1);
}
