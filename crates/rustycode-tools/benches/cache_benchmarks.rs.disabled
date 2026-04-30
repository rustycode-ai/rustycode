// Criterion benchmarks for tool caching performance
//
// Run with: cargo bench --package rustycode-tools --bench cache_benchmarks

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rustycode_tools::cache::{CacheConfig, CacheKey, CachedToolResult, ToolCache};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::runtime::Runtime;

/// Benchmark cache hit performance (reads)
fn bench_cache_hits(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = ToolCache::new_with_defaults();

    // Pre-populate cache
    let keys: Vec<CacheKey> = (0..100)
        .map(|i| CacheKey::new(format!("tool_{}", i), &json!({"arg": i})))
        .collect();

    rt.block_on(async {
        for key in &keys {
            let result = CachedToolResult {
                output: format!("output for {}", key.tool_name),
                structured: None,
                success: true,
                error: None,
            };
            cache.put(key.clone(), result, vec![], None).await;
        }
    });

    let mut group = c.benchmark_group("cache_hits");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let key = &keys[i as usize];
                        black_box(cache.get(key).await);
                    }
                });
            });
        });
    }

    group.finish();
}

/// Benchmark cache miss performance
fn bench_cache_misses(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = ToolCache::new_with_defaults();

    let mut group = c.benchmark_group("cache_misses");

    for size in [10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                rt.block_on(async {
                    for i in 0..size {
                        let key =
                            CacheKey::new(format!("nonexistent_tool_{}", i), &json!({"arg": i}));
                        black_box(cache.get(&key).await);
                    }
                });
            });
        });
    }

    group.finish();
}

/// Benchmark cache insertion performance
fn bench_cache_insertions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cache_insertions");

    for size in [10, 50, 100, 500].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let cache = ToolCache::new_with_defaults();
                rt.block_on(async {
                    for i in 0..size {
                        let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
                        let result = CachedToolResult {
                            output: format!("output {}", i),
                            structured: None,
                            success: true,
                            error: None,
                        };
                        cache.put(key, result, vec![], None).await;
                    }
                });
            });
        });
    }

    group.finish();
}

/// Benchmark cache key generation (hashing performance)
fn bench_cache_key_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_key_generation");

    // Small arguments
    group.bench_function("small_args", |b| {
        b.iter(|| {
            let args = json!({"path": "/tmp/file.txt", "line": 42});
            black_box(CacheKey::new("read_file".to_string(), black_box(&args)));
        });
    });

    // Medium arguments
    group.bench_function("medium_args", |b| {
        b.iter(|| {
            let args = json!({
                "pattern": "*.rs",
                "path": "/src",
                "exclude": ["target", "node_modules"],
                "max_results": 100
            });
            black_box(CacheKey::new("glob".to_string(), black_box(&args)));
        });
    });

    // Large arguments
    group.bench_function("large_args", |b| {
        b.iter(|| {
            let args = json!({
                "query": "SELECT * FROM users WHERE active = true",
                "joins": ["profiles", "settings"],
                "limit": 1000,
                "offset": 0
            });
            black_box(CacheKey::new("execute_sql".to_string(), black_box(&args)));
        });
    });

    group.finish();
}

/// Benchmark cache eviction performance
fn bench_cache_eviction(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("cache_eviction");

    // Small cache (100 entries)
    group.bench_function("small_cache", |b| {
        b.iter(|| {
            let config = CacheConfig {
                max_entries: 100,
                ..Default::default()
            };
            let cache = ToolCache::new(config);
            rt.block_on(async {
                // Insert 200 entries to trigger eviction
                for i in 0..200 {
                    let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
                    let result = CachedToolResult {
                        output: format!("output {}", i),
                        structured: None,
                        success: true,
                        error: None,
                    };
                    cache.put(key, result, vec![], None).await;
                }
            });
        });
    });

    // Large cache (1000 entries)
    group.bench_function("large_cache", |b| {
        b.iter(|| {
            let config = CacheConfig {
                max_entries: 1000,
                ..Default::default()
            };
            let cache = ToolCache::new(config);
            rt.block_on(async {
                // Insert 2000 entries to trigger eviction
                for i in 0..2000 {
                    let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
                    let result = CachedToolResult {
                        output: format!("output {}", i),
                        structured: None,
                        success: true,
                        error: None,
                    };
                    cache.put(key, result, vec![], None).await;
                }
            });
        });
    });

    group.finish();
}

/// Benchmark cache statistics collection
fn bench_cache_stats(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = ToolCache::new_with_defaults();

    // Pre-populate cache
    rt.block_on(async {
        for i in 0..100 {
            let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
            let result = CachedToolResult {
                output: format!("output {}", i),
                structured: None,
                success: true,
                error: None,
            };
            cache.put(key, result, vec![], None).await;
        }
    });

    c.bench_function("cache_stats", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(cache.stats().await);
            });
        });
    });
}

/// Benchmark file dependency extraction
fn bench_dependency_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("dependency_extraction");

    group.bench_function("single_file", |b| {
        b.iter(|| {
            let args = json!({"path": "/tmp/file.txt"});
            black_box(ToolCache::extract_file_dependencies("read_file", &args));
        });
    });

    group.bench_function("glob_pattern", |b| {
        b.iter(|| {
            let args = json!({"pattern": "src/**/*.rs", "exclude": ["target/**"]});
            black_box(ToolCache::extract_file_dependencies("glob", &args));
        });
    });

    group.bench_function("grep_search", |b| {
        b.iter(|| {
            let args = json!({
                "pattern": "TODO",
                "path": "src",
                "file_pattern": "*.rs"
            });
            black_box(ToolCache::extract_file_dependencies("grep", &args));
        });
    });

    group.finish();
}

/// Real-world scenario: repeated tool execution with caching
fn bench_real_world_scenario(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = ToolCache::new_with_defaults();

    let mut group = c.benchmark_group("real_world");

    // Simulate reading the same file multiple times
    group.bench_function("repeated_read_file", |b| {
        let key = CacheKey::new(
            "read_file".to_string(),
            &json!({"path": "/tmp/config.toml"}),
        );
        let result = CachedToolResult {
            output: "[settings]\nkey = \"value\"".to_string(),
            structured: None,
            success: true,
            error: None,
        };

        rt.block_on(async {
            cache
                .put(
                    key.clone(),
                    result,
                    vec![PathBuf::from("/tmp/config.toml")],
                    None,
                )
                .await;
        });

        b.iter(|| {
            rt.block_on(async {
                black_box(cache.get(&key).await);
            });
        });
    });

    // Simulate pattern matching with different patterns
    group.bench_function("pattern_matching_variations", |b| {
        let patterns = ["*.rs", "*.toml", "*.md"];
        let keys: Vec<CacheKey> = patterns
            .iter()
            .map(|p| CacheKey::new("glob".to_string(), &json!({"pattern": p})))
            .collect();

        rt.block_on(async {
            for key in &keys {
                let result = CachedToolResult {
                    output: format!("matches for {:?}", key),
                    structured: None,
                    success: true,
                    error: None,
                };
                cache.put(key.clone(), result, vec![], None).await;
            }
        });

        b.iter(|| {
            rt.block_on(async {
                for key in &keys {
                    black_box(cache.get(key).await);
                }
            });
        });
    });

    group.finish();
}

/// Benchmark TTL-based expiration
fn bench_ttl_expiration(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("ttl_expiration");

    group.bench_function("short_ttl_checks", |b| {
        let config = CacheConfig {
            default_ttl: Duration::from_millis(100),
            ..Default::default()
        };
        let cache = ToolCache::new(config);
        let key = CacheKey::new("test".to_string(), &json!({"arg": 1}));

        rt.block_on(async {
            let result = CachedToolResult {
                output: "test".to_string(),
                structured: None,
                success: true,
                error: None,
            };
            cache.put(key.clone(), result, vec![], None).await;
        });

        b.iter(|| {
            rt.block_on(async {
                black_box(cache.get(&key).await);
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_hits,
    bench_cache_misses,
    bench_cache_insertions,
    bench_cache_key_generation,
    bench_cache_eviction,
    bench_cache_stats,
    bench_dependency_extraction,
    bench_real_world_scenario,
    bench_ttl_expiration
);

criterion_main!(benches);
