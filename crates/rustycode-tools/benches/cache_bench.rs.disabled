// Criterion benchmarks for cache performance
use rustycode_tools::cache::{CacheConfig, CachedToolResult, ToolCache};
use serde_json::json;
use std::time::Duration;

#[allow(dead_code)]
fn create_test_result(size: usize) -> CachedToolResult {
    CachedToolResult {
        output: "x".repeat(size),
        structured: Some(json!({"data": "test"})),
        success: true,
        error: None,
    }
}

#[cfg(test)]
#[allow(dead_code)]
mod cache_benches {
    use super::*;
    use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

    fn bench_cache_write(c: &mut Criterion) {
        let mut group = c.benchmark_group("cache_write");

        for size in [100, 1000, 10000].iter() {
            group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
                let cache = ToolCache::new_with_defaults();
                let result = create_test_result(size);

                b.iter(|| {
                    let key = rustycode_tools::cache::CacheKey::new(
                        "test_tool".to_string(),
                        &json!({"arg": black_box("value")}),
                    );
                    tokio::runtime::Runtime::new().unwrap().block_on(cache.put(
                        key,
                        result.clone(),
                        vec![],
                        None,
                    ));
                });
            });
        }

        group.finish();
    }

    fn bench_cache_read(c: &mut Criterion) {
        let mut group = c.benchmark_group("cache_read");

        // Pre-populate cache
        let cache = ToolCache::new_with_defaults();
        let result = create_test_result(1000);
        let key = rustycode_tools::cache::CacheKey::new(
            "test_tool".to_string(),
            &json!({"arg": "value"}),
        );

        tokio::runtime::Runtime::new().unwrap().block_on(cache.put(
            key.clone(),
            result,
            vec![],
            None,
        ));

        group.bench_function("hit", |b| {
            b.iter(|| {
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(cache.get(black_box(&key)));
            });
        });

        group.finish();
    }

    fn bench_cache_key_generation(c: &mut Criterion) {
        let mut group = c.benchmark_group("cache_key_generation");

        group.bench_function("small_args", |b| {
            b.iter(|| {
                rustycode_tools::cache::CacheKey::new(
                    "read_file".to_string(),
                    &json!({"path": "/test/path.txt"}),
                );
            });
        });

        group.bench_function("large_args", |b| {
            b.iter(|| {
                rustycode_tools::cache::CacheKey::new(
                    "grep".to_string(),
                    &json!({"pattern": "test", "path": "/", "context": 5, "max_matches": 1000}),
                );
            });
        });

        group.finish();
    }

    fn bench_cache_hit_rate(c: &mut Criterion) {
        let mut group = c.benchmark_group("cache_hit_rate");

        group.bench_function("90%_hit_rate", |b| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let cache = ToolCache::new_with_defaults();

            // Pre-populate with 10 items
            for i in 0..10 {
                let key = rustycode_tools::cache::CacheKey::new(
                    format!("tool_{}", i),
                    &json!({"arg": i}),
                );
                let result = create_test_result(100);
                rt.block_on(cache.put(key, result, vec![], None));
            }

            b.iter(|| {
                // 90% cache hits (first 9) + 10% misses (10th)
                for i in 0..10 {
                    let key = rustycode_tools::cache::CacheKey::new(
                        format!("tool_{}", i),
                        &json!({"arg": i}),
                    );
                    rt.block_on(cache.get(&key));
                }
            });
        });

        group.finish();
    }

    fn bench_cache_lru_eviction(c: &mut Criterion) {
        let mut group = c.benchmark_group("cache_lru");

        let config = CacheConfig {
            default_ttl: Duration::from_secs(300),
            max_entries: 100,
            track_file_dependencies: true,
            max_memory_bytes: Some(100 * 1024 * 1024), // 100 MB
            enable_metrics: true,
        };

        group.bench_function("eviction_performance", |b| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let cache = ToolCache::new(config.clone());

            b.iter(|| {
                // Add 150 items to a 100-item cache (causes 50 evictions)
                for i in 0..150 {
                    let key = rustycode_tools::cache::CacheKey::new(
                        format!("tool_{}", i),
                        &json!({"arg": i}),
                    );
                    let result = create_test_result(100);
                    rt.block_on(cache.put(key, result, vec![], None));
                }
            });
        });

        group.finish();
    }

    criterion_group!(
        benches,
        bench_cache_write,
        bench_cache_read,
        bench_cache_key_generation,
        bench_cache_hit_rate,
        bench_cache_lru_eviction
    );
    criterion_main!(benches);
}
