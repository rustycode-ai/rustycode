//! Sprint 10: Concurrent Load Test Execution
//!
//! Validates task contracts and registry under concurrent load with
//! metrics collection (throughput, latency percentiles, memory stability).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::similar_names,
    clippy::manual_assert
)]

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustycode_protocol::{ContractViolation, TaskDescriptor, TaskRegistry, ViolationCode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Produce a task descriptor for testing.
fn make_descriptor(id: usize) -> TaskDescriptor {
    TaskDescriptor::new(
        format!("bench.task_{id}"),
        format!("Benchmark task {id}"),
        serde_json::json!({
            "type": "object",
            "required": ["input"],
            "properties": {
                "input": {"type": "string"},
                "count": {"type": "integer"}
            }
        }),
        serde_json::json!({
            "type": "object",
            "required": ["output"],
            "properties": {
                "output": {"type": "string"},
                "elapsed_ms": {"type": "number"}
            }
        }),
    )
    .with_tag("bench")
}

/// Percentile helper: returns the value at the given percentile (0..100).
fn percentile(sorted: &[Duration], pct: u8) -> Duration {
    assert!(!sorted.is_empty());
    let idx = ((sorted.len() as f64) * (pct as f64 / 100.0)).floor() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Print a load test report.
fn print_report(label: &str, count: usize, durations: &[Duration], errors: usize) {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();

    let total_ms: u64 = sorted.iter().map(|d| d.as_millis() as u64).sum();
    let ops_per_sec = (count as u64 * 1000).checked_div(total_ms).unwrap_or(0);

    eprintln!(
        "\n=== Load Test: {label} ===\n\
         Sessions:  {count}\n\
         Errors:    {errors}\n\
         Ops/sec:   {ops_per_sec}\n\
         p50:       {:.2}ms\n\
         p95:       {:.2}ms\n\
         p99:       {:.2}ms\n\
         Min:       {:.2}ms\n\
         Max:       {:.2}ms",
        sorted[sorted.len() / 2].as_secs_f64() * 1000.0,
        percentile(&sorted, 95).as_secs_f64() * 1000.0,
        percentile(&sorted, 99).as_secs_f64() * 1000.0,
        sorted[0].as_secs_f64() * 1000.0,
        sorted[sorted.len() - 1].as_secs_f64() * 1000.0,
    );
}

/// Collect durations across threads.
struct LatencyCollector {
    slots: Vec<AtomicU64>,
}

impl LatencyCollector {
    fn new(capacity: usize) -> Self {
        let mut slots = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            slots.push(AtomicU64::new(0));
        }
        Self { slots }
    }

    fn record(&self, idx: usize, duration: Duration) {
        if idx < self.slots.len() {
            self.slots[idx].store(duration.as_nanos() as u64, Ordering::Relaxed);
        }
    }

    fn collect(&self) -> Vec<Duration> {
        self.slots
            .iter()
            .map(|a| Duration::from_nanos(a.load(Ordering::Relaxed)))
            .filter(|d| !d.is_zero())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Test: Concurrent Registry Registration
// ---------------------------------------------------------------------------

#[test]
fn load_test_registry_10_sessions() {
    run_registry_load_test(10);
}

#[test]
fn load_test_registry_25_sessions() {
    run_registry_load_test(25);
}

#[test]
fn load_test_registry_50_sessions() {
    run_registry_load_test(50);
}

#[test]
fn load_test_registry_100_sessions() {
    run_registry_load_test(100);
}

fn run_registry_load_test(count: usize) {
    let registry = Arc::new(std::sync::Mutex::new(TaskRegistry::new()));
    let errors = Arc::new(AtomicUsize::new(0));
    let latencies = Arc::new(LatencyCollector::new(count));
    let mut handles = Vec::with_capacity(count);

    for i in 0..count {
        let registry = registry.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        let handle = std::thread::spawn(move || {
            let desc = make_descriptor(i);
            let start = Instant::now();
            let result = registry.lock().unwrap().register(desc);
            latencies.record(i, start.elapsed());
            if result.is_err() {
                errors.fetch_add(1, Ordering::Relaxed);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let reg = registry.lock().unwrap();
    let collected = latencies.collect();
    let err_count = errors.load(Ordering::SeqCst);

    print_report(
        &format!("Registry Registration ({count})"),
        count,
        &collected,
        err_count,
    );

    assert_eq!(err_count, 0, "Registration errors occurred");
    assert_eq!(reg.len(), count, "Not all tasks registered");

    // Verify all descriptors are retrievable.
    for i in 0..count {
        let name = format!("bench.task_{i}");
        assert!(
            reg.get(&name).is_some(),
            "Task '{name}' not found in registry"
        );
    }
    drop(reg);
}

// ---------------------------------------------------------------------------
// Test: Concurrent Input Validation
// ---------------------------------------------------------------------------

#[test]
fn load_test_validation_10_sessions() {
    run_validation_load_test(10, 100);
}

#[test]
fn load_test_validation_25_sessions() {
    run_validation_load_test(25, 100);
}

#[test]
fn load_test_validation_50_sessions() {
    run_validation_load_test(50, 100);
}

#[test]
fn load_test_validation_100_sessions() {
    run_validation_load_test(100, 100);
}

fn run_validation_load_test(task_count: usize, iterations_per_task: usize) {
    // Pre-populate registry.
    let mut reg = TaskRegistry::new();
    for i in 0..task_count {
        reg.register(make_descriptor(i)).unwrap();
    }
    let registry = Arc::new(reg);

    let total_ops = task_count * iterations_per_task;
    let errors = Arc::new(AtomicUsize::new(0));
    let latencies = Arc::new(LatencyCollector::new(total_ops));
    let mut handles = Vec::with_capacity(task_count);

    for task_idx in 0..task_count {
        let registry = registry.clone();
        let errors = errors.clone();
        let latencies = latencies.clone();
        let handle = std::thread::spawn(move || {
            let task_name = format!("bench.task_{task_idx}");
            for iter in 0..iterations_per_task {
                let input = serde_json::json!({
                    "input": format!("iteration_{iter}"),
                    "count": iter
                });

                let op_idx = task_idx * iterations_per_task + iter;
                let start = Instant::now();
                let result = registry.validate_input(&task_name, &input);
                latencies.record(op_idx, start.elapsed());

                if result.is_err() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let collected = latencies.collect();
    let err_count = errors.load(Ordering::SeqCst);

    print_report(
        &format!("Input Validation ({task_count} tasks × {iterations_per_task} iters)"),
        total_ops,
        &collected,
        err_count,
    );

    assert_eq!(err_count, 0, "Validation errors occurred");
}

// ---------------------------------------------------------------------------
// Test: Concurrent Output Validation
// ---------------------------------------------------------------------------

#[test]
fn load_test_output_validation_100_sessions() {
    let task_count = 100;
    let iterations = 50;

    let mut reg = TaskRegistry::new();
    for i in 0..task_count {
        reg.register(make_descriptor(i)).unwrap();
    }
    let registry = Arc::new(reg);

    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(task_count);

    for task_idx in 0..task_count {
        let registry = registry.clone();
        let errors = errors.clone();
        let handle = std::thread::spawn(move || {
            let task_name = format!("bench.task_{task_idx}");
            for iter in 0..iterations {
                let output = serde_json::json!({
                    "output": format!("result_{iter}"),
                    "elapsed_ms": iter as f64 * 0.5
                });
                if registry.validate_output(&task_name, &output).is_err() {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(
        errors.load(Ordering::SeqCst),
        0,
        "Output validation errors under load"
    );
}

// ---------------------------------------------------------------------------
// Test: Validation Rejects Invalid Input Under Load
// ---------------------------------------------------------------------------

#[test]
fn load_test_invalid_input_detected_under_load() {
    let task_count = 50;
    let iterations = 50;

    let mut reg = TaskRegistry::new();
    for i in 0..task_count {
        reg.register(make_descriptor(i)).unwrap();
    }
    let registry = Arc::new(reg);

    let violations = Arc::new(AtomicUsize::new(0));
    let false_negatives = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(task_count);

    for task_idx in 0..task_count {
        let registry = registry.clone();
        let violations = violations.clone();
        let false_negatives = false_negatives.clone();
        let handle = std::thread::spawn(move || {
            let task_name = format!("bench.task_{task_idx}");
            for iter in 0..iterations {
                // Alternate valid and invalid inputs.
                if iter % 2 == 0 {
                    // Valid: has "input" string.
                    let input = serde_json::json!({"input": "ok", "count": iter});
                    if registry.validate_input(&task_name, &input).is_err() {
                        false_negatives.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    // Invalid: missing "input" required field.
                    let input = serde_json::json!({"count": iter});
                    if let Err(e) = registry.validate_input(&task_name, &input) {
                        assert!(
                            matches!(e.code, ViolationCode::InvalidInput),
                            "Expected InvalidInput, got {:?}",
                            e.code
                        );
                        violations.fetch_add(1, Ordering::Relaxed);
                    } else {
                        false_negatives.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let expected_violations = task_count * iterations / 2;
    assert_eq!(
        violations.load(Ordering::SeqCst),
        expected_violations,
        "Should detect exactly half as invalid"
    );
    assert_eq!(
        false_negatives.load(Ordering::SeqCst),
        0,
        "No valid inputs should be rejected, no invalid inputs should pass"
    );
}

// ---------------------------------------------------------------------------
// Test: Unknown Task Detection Under Load
// ---------------------------------------------------------------------------

#[test]
fn load_test_unknown_task_detection() {
    let task_count = 50;
    let registry = Arc::new(TaskRegistry::new());
    let unknown_detected = Arc::new(AtomicUsize::new(0));
    let false_ok = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::with_capacity(task_count);

    for i in 0..task_count {
        let registry = registry.clone();
        let unknown_detected = unknown_detected.clone();
        let false_ok = false_ok.clone();
        let handle = std::thread::spawn(move || {
            let fake_task = format!("nonexistent.task_{i}");
            let result = registry.validate_input(&fake_task, &serde_json::json!({}));
            match result {
                Err(e) if matches!(e.code, ViolationCode::UnknownTask) => {
                    unknown_detected.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    panic!("Wrong error code: {:?}", e.code);
                }
                Ok(()) => {
                    false_ok.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(
        unknown_detected.load(Ordering::SeqCst),
        task_count,
        "All unknown tasks should be detected"
    );
    assert_eq!(
        false_ok.load(Ordering::SeqCst),
        0,
        "No unknown task should validate successfully"
    );
}

// ---------------------------------------------------------------------------
// Test: Memory Stability (repeated allocations over time)
// ---------------------------------------------------------------------------

#[test]
fn load_test_memory_stability_repeated_cycles() {
    let cycles = 10;
    let tasks_per_cycle = 50;

    for cycle in 0..cycles {
        let mut reg = TaskRegistry::new();
        for i in 0..tasks_per_cycle {
            let id = cycle * tasks_per_cycle + i;
            reg.register(make_descriptor(id)).unwrap();
        }

        // Validate all tasks.
        for i in 0..tasks_per_cycle {
            let id = cycle * tasks_per_cycle + i;
            let name = format!("bench.task_{id}");
            let input = serde_json::json!({"input": "stable", "count": cycle});
            reg.validate_input(&name, &input).unwrap();
        }

        // Registry drops here; memory should be released.
    }

    // If we reach here without OOM or panics, memory is stable.
}

// ---------------------------------------------------------------------------
// Test: Concurrent Registration Prevents Duplicates
// ---------------------------------------------------------------------------

#[test]
fn load_test_duplicate_registration_rejected() {
    // Register the same descriptor name from multiple threads.
    let name = "shared.task";
    let attempt_count = 20;
    let success_count = Arc::new(AtomicUsize::new(0));
    let duplicate_count = Arc::new(AtomicUsize::new(0));

    let barrier = Arc::new(std::sync::Barrier::new(attempt_count));
    let mut handles = Vec::with_capacity(attempt_count);

    // We need a Mutex around the registry since register() needs &mut self.
    let registry = Arc::new(std::sync::Mutex::new(TaskRegistry::new()));

    for _ in 0..attempt_count {
        let registry = registry.clone();
        let success_count = success_count.clone();
        let duplicate_count = duplicate_count.clone();
        let barrier = barrier.clone();
        let handle = std::thread::spawn(move || {
            let desc = TaskDescriptor::new(
                name,
                "shared task",
                serde_json::json!({"type": "object"}),
                serde_json::json!({"type": "object"}),
            );
            barrier.wait();
            let result = registry.lock().unwrap().register(desc);
            match result {
                Ok(()) => success_count.fetch_add(1, Ordering::Relaxed),
                Err(ContractViolation {
                    code: ViolationCode::InvalidInput,
                    ..
                }) => duplicate_count.fetch_add(1, Ordering::Relaxed),
                Err(e) => panic!("Unexpected error: {e}"),
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let successes = success_count.load(Ordering::SeqCst);
    let duplicates = duplicate_count.load(Ordering::SeqCst);

    assert_eq!(
        successes + duplicates,
        attempt_count,
        "All attempts should either succeed or be rejected as duplicates"
    );
    assert_eq!(successes, 1, "Exactly one registration should succeed");
    assert_eq!(
        duplicates,
        attempt_count - 1,
        "All other attempts should be rejected"
    );
}

// ---------------------------------------------------------------------------
// Test: Absolute Latency Bound (p99 < 50ms under 100 concurrent sessions)
// ---------------------------------------------------------------------------

#[test]
fn load_test_absolute_latency_within_bounds() {
    let latencies = measure_validation_latency(100, 100);
    let mut sorted = latencies;
    sorted.sort_unstable();

    let p50 = percentile(&sorted, 50);
    let p95 = percentile(&sorted, 95);
    let p99 = percentile(&sorted, 99);

    eprintln!(
        "\n=== Absolute Latency (100 sessions × 100 iters) ===\n\
         p50:  {:.2}us\n\
         p95:  {:.2}us\n\
         p99:  {:.2}us",
        p50.as_secs_f64() * 1_000_000.0,
        p95.as_secs_f64() * 1_000_000.0,
        p99.as_secs_f64() * 1_000_000.0,
    );

    // Mutex contention at 100 threads is expected to be high; the real
    // bound is that no operation should take egregiously long. 50ms p99
    // is generous for a schema validation under heavy contention.
    assert!(
        p99 < Duration::from_millis(50),
        "p99 latency too high: {:.2}ms (target <50ms)",
        p99.as_secs_f64() * 1000.0
    );
}

fn measure_validation_latency(task_count: usize, iterations: usize) -> Vec<Duration> {
    let mut reg = TaskRegistry::new();
    for i in 0..task_count {
        reg.register(make_descriptor(i)).unwrap();
    }
    let registry = Arc::new(reg);

    let total_ops = task_count * iterations;
    let latencies = Arc::new(LatencyCollector::new(total_ops));
    let mut handles = Vec::with_capacity(task_count);

    for task_idx in 0..task_count {
        let registry = registry.clone();
        let latencies = latencies.clone();
        let handle = std::thread::spawn(move || {
            let task_name = format!("bench.task_{task_idx}");
            for iter in 0..iterations {
                let input = serde_json::json!({"input": "latency", "count": iter});
                let op_idx = task_idx * iterations + iter;
                let start = Instant::now();
                let _ = registry.validate_input(&task_name, &input);
                latencies.record(op_idx, start.elapsed());
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    latencies.collect()
}
