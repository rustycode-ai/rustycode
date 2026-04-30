// Benchmark optimizations to measure performance improvements
//
// Run with: cargo bench --package rustycode-tools --bench optimization_benchmarks

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use regex::Regex;
use rustycode_tools::cache::{CacheKey, ToolCache};
use serde_json::json;
use tokio::runtime::Runtime;

/// Benchmark regex compilation with caching vs without
fn bench_regex_caching(c: &mut Criterion) {
    let mut group = c.benchmark_group("regex_optimization");

    // Test patterns commonly used in grep
    let patterns = vec![
        r"TODO",
        r"FIXME",
        r"\bfn\s+\w+",
        r"impl\s+\w+",
        r"pub\s+fn\s+\w+",
        r"#\[derive\(.+\)\]",
        r"use\s+.*;",
    ];

    // Benchmark cached regex performance (simulated with pre-compilation)
    group.bench_function("cached_regex", |b| {
        // Pre-compile patterns to simulate cache hit
        let cached_patterns: Vec<Regex> = patterns.iter().map(|p| Regex::new(p).unwrap()).collect();

        b.iter(|| {
            for (_pattern, regex) in patterns.iter().zip(cached_patterns.iter()) {
                // Use the pre-compiled regex
                black_box(regex.is_match("test string"));
            }
        });
    });

    // Benchmark uncached regex performance for comparison
    group.bench_function("uncached_regex", |b| {
        b.iter(|| {
            for pattern in &patterns {
                let re = black_box(Regex::new(pattern).unwrap());
                black_box(re.is_match("test string"));
            }
        });
    });

    group.finish();
}

/// Benchmark string allocation patterns
fn bench_string_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("string_optimizations");

    // String cloning pattern
    group.bench_function("string_clone", |b| {
        let s = String::from("test_tool_name");
        b.iter(|| {
            let _ = black_box(s.clone());
        });
    });

    // String reference pattern (no allocation)
    group.bench_function("string_ref", |b| {
        let s = String::from("test_tool_name");
        b.iter(|| {
            let _: &str = black_box(s.as_str());
        });
    });

    // String with capacity
    group.bench_function("string_with_capacity", |b| {
        b.iter(|| {
            let mut s = String::with_capacity(20);
            s.push_str("test");
            s.push_str("_tool");
            s.push_str("_name");
            black_box(s);
        });
    });

    group.finish();
}

/// Benchmark profile detection optimization
fn bench_profile_detection(c: &mut Criterion) {
    let mut group = c.benchmark_group("profile_detection");

    let prompts = vec![
        "Show me the structure of the codebase",
        "Create a new function to handle errors",
        "Debug this failing test",
        "Deploy the application to production",
        "What files are in the src directory?",
    ];

    for prompt in &prompts {
        group.bench_with_input(
            BenchmarkId::new("detect_profile", prompt.len()),
            prompt,
            |b, prompt| {
                b.iter(|| {
                    black_box(rustycode_tools::ToolProfile::from_prompt(black_box(prompt)));
                });
            },
        );
    }

    group.finish();
}

/// Benchmark cache hit rates with optimized key generation
fn bench_cache_key_optimization(c: &mut Criterion) {
    let _rt = Runtime::new().unwrap();
    let _cache = ToolCache::new_with_defaults();

    let mut group = c.benchmark_group("cache_key_optimization");

    // Small arguments
    group.bench_function("small_args_cached", |b| {
        let args = json!({"path": "/tmp/file.txt", "line": 42});
        b.iter(|| {
            black_box(CacheKey::new("read_file".to_string(), black_box(&args)));
        });
    });

    // Medium arguments
    group.bench_function("medium_args_cached", |b| {
        let args = json!({
            "pattern": "*.rs",
            "path": "/src",
            "exclude": ["target", "node_modules"],
            "max_results": 100
        });
        b.iter(|| {
            black_box(CacheKey::new("glob".to_string(), black_box(&args)));
        });
    });

    // Large arguments
    group.bench_function("large_args_cached", |b| {
        let args = json!({
            "query": "SELECT * FROM users WHERE active = true",
            "joins": ["profiles", "settings"],
            "limit": 1000,
            "offset": 0
        });
        b.iter(|| {
            black_box(CacheKey::new("execute_sql".to_string(), black_box(&args)));
        });
    });

    group.finish();
}

/// Benchmark memory usage patterns
fn bench_memory_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_optimizations");

    // Vector cloning
    group.bench_function("vec_clone", |b| {
        let v: Vec<String> = vec![
            "tool1".to_string(),
            "tool2".to_string(),
            "tool3".to_string(),
        ];
        b.iter(|| {
            let _ = black_box(v.clone());
        });
    });

    // Vector iteration with references
    group.bench_function("vec_iter_ref", |b| {
        let v: Vec<String> = vec![
            "tool1".to_string(),
            "tool2".to_string(),
            "tool3".to_string(),
        ];
        b.iter(|| {
            for s in v.iter() {
                black_box(s);
            }
        });
    });

    // HashMap vs FxHashMap performance comparison
    use std::collections::HashMap;

    group.bench_function("hashmap_get", |b| {
        let mut map = HashMap::new();
        for i in 0..100 {
            map.insert(format!("key_{}", i), i);
        }
        b.iter(|| {
            for i in 0..100 {
                black_box(map.get(&format!("key_{}", i)));
            }
        });
    });

    group.finish();
}

/// Benchmark combined workflow scenarios
fn bench_optimized_workflow(c: &mut Criterion) {
    let _rt = Runtime::new().unwrap();
    let _cache = ToolCache::new_with_defaults();

    let mut group = c.benchmark_group("optimized_workflow");

    // Simulate repeated grep operations with pattern caching
    group.bench_function("repeated_grep_with_cache", |b| {
        let pattern = r"\bfn\s+\w+";
        let re = Regex::new(pattern).unwrap(); // Pre-compile to simulate cache

        b.iter(|| {
            // Use the pre-compiled regex
            let lines = vec![
                "fn test_function() {",
                "pub fn another_function() {",
                "impl MyStruct {",
            ];

            for line in lines {
                black_box(re.is_match(line));
            }
        });
    });

    // Simulate tool profile detection with caching
    group.bench_function("profile_detection_repeated", |b| {
        let prompts = vec![
            "What is the structure?",
            "Create a new feature",
            "Debug this error",
            "Deploy to production",
        ];

        b.iter(|| {
            for prompt in &prompts {
                black_box(rustycode_tools::ToolProfile::from_prompt(prompt));
            }
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_regex_caching,
    bench_string_allocations,
    bench_profile_detection,
    bench_cache_key_optimization,
    bench_memory_patterns,
    bench_optimized_workflow
);

criterion_main!(benches);
