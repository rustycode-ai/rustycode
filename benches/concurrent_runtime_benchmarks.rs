// Benchmarks for concurrent runtime operations
//
// These benchmarks demonstrate the performance gains from using
// tokio::task::JoinSet for concurrent tool execution.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rustycode_protocol::{SessionId, ToolCall};
use rustycode_runtime::{AsyncRuntime, ConcurrentConfig};
use std::path::PathBuf;
use std::time::Duration;
use tokio::runtime::Runtime;

fn setup_test_runtime() -> (AsyncRuntime, PathBuf, SessionId) {
    let temp_dir = std::env::temp_dir().join(format!("rustycode-bench-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let data_dir = temp_dir.join("data");
    let skills_dir = temp_dir.join("skills");
    let memory_dir = temp_dir.join("memory");
    std::fs::create_dir_all(&skills_dir).unwrap();
    std::fs::create_dir_all(&memory_dir).unwrap();

    std::fs::write(
        temp_dir.join(".rustycode.toml"),
        format!(
            "data_dir = \"{}\"\nskills_dir = \"{}\"\nmemory_dir = \"{}\"\nlsp_servers = []\n",
            data_dir.display(),
            skills_dir.display(),
            memory_dir.display()
        ),
    )
    .unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let runtime = rt.block_on(AsyncRuntime::load(&temp_dir)).unwrap();
    let session_id = SessionId::new();

    (runtime, temp_dir, session_id)
}

fn create_tool_calls(count: usize) -> Vec<ToolCall> {
    (0..count)
        .map(|i| ToolCall {
            call_id: format!("call-{}", i),
            name: format!("tool_{}", i),
            arguments: serde_json::json!({"index": i}),
        })
        .collect()
}

fn bench_sequential_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (runtime, temp_dir, session_id) = setup_test_runtime();

    let mut group = c.benchmark_group("sequential_tool_execution");

    for num_tools in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("tools", num_tools),
            num_tools,
            |b, &num_tools| {
                let calls = create_tool_calls(num_tools);

                b.iter(|| {
                    rt.block_on(async {
                        for call in calls.clone() {
                            let _ = runtime.execute_tool(&session_id, call, &temp_dir).await;
                        }
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_concurrent_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (runtime, temp_dir, session_id) = setup_test_runtime();

    let mut group = c.benchmark_group("concurrent_tool_execution");

    for num_tools in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::new("tools", num_tools),
            num_tools,
            |b, &num_tools| {
                let calls = create_tool_calls(num_tools);

                b.iter(|| {
                    rt.block_on(async {
                        let _ = runtime
                            .execute_tools_concurrent(&session_id, calls.clone(), &temp_dir)
                            .await;
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_concurrent_with_custom_config(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (runtime, temp_dir, session_id) = setup_test_runtime();

    let mut group = c.benchmark_group("concurrent_custom_config");

    for max_concurrency in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("max_concurrency", max_concurrency),
            max_concurrency,
            |b, &max_concurrency| {
                let calls = create_tool_calls(50);
                let config = ConcurrentConfig::default()
                    .with_max_concurrency(max_concurrency)
                    .with_timeout(Duration::from_secs(60));

                b.iter(|| {
                    rt.block_on(async {
                        let _ = runtime
                            .execute_tools_concurrent_with_config(
                                &session_id,
                                calls.clone(),
                                &temp_dir,
                                config.clone(),
                            )
                            .await;
                    })
                });
            },
        );
    }

    group.finish();
}

fn bench_timeout_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (runtime, temp_dir, session_id) = setup_test_runtime();

    let mut group = c.benchmark_group("timeout_handling");

    group.bench_function("short_timeout", |b| {
        let calls = create_tool_calls(10);
        let config = ConcurrentConfig::default().with_timeout(Duration::from_millis(1));

        b.iter(|| {
            rt.block_on(async {
                let _ = runtime
                    .execute_tools_concurrent_with_config(
                        &session_id,
                        calls.clone(),
                        &temp_dir,
                        config.clone(),
                    )
                    .await;
            })
        });
    });

    group.bench_function("long_timeout", |b| {
        let calls = create_tool_calls(10);
        let config = ConcurrentConfig::default().with_timeout(Duration::from_secs(60));

        b.iter(|| {
            rt.block_on(async {
                let _ = runtime
                    .execute_tools_concurrent_with_config(
                        &session_id,
                        calls.clone(),
                        &temp_dir,
                        config.clone(),
                    )
                    .await;
            })
        });
    });

    group.finish();
}

fn bench_error_handling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (runtime, temp_dir, session_id) = setup_test_runtime();

    let mut group = c.benchmark_group("error_handling");

    group.bench_function("continue_on_error", |b| {
        let calls = create_tool_calls(10);
        let config = ConcurrentConfig::default().with_continue_on_error(true);

        b.iter(|| {
            rt.block_on(async {
                let _ = runtime
                    .execute_tools_concurrent_with_config(
                        &session_id,
                        calls.clone(),
                        &temp_dir,
                        config.clone(),
                    )
                    .await;
            })
        });
    });

    group.bench_function("stop_on_error", |b| {
        let calls = create_tool_calls(10);
        let config = ConcurrentConfig::default().with_continue_on_error(false);

        b.iter(|| {
            rt.block_on(async {
                let _ = runtime
                    .execute_tools_concurrent_with_config(
                        &session_id,
                        calls.clone(),
                        &temp_dir,
                        config.clone(),
                    )
                    .await;
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sequential_execution,
    bench_concurrent_execution,
    bench_concurrent_with_custom_config,
    bench_timeout_handling,
    bench_error_handling
);
criterion_main!(benches);
