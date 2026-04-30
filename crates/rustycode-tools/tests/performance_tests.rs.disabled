//! Performance and Benchmarks Tests
//!
//! Tests performance characteristics:
//! - Tool execution time benchmarks
//! - Concurrent tool execution
//! - Memory usage profiling
//! - Cache performance

use rustycode_tools::cache::{CacheConfig, CacheKey, CachedToolResult, ToolCache};
use rustycode_tools::{BashTool, ReadFileTool, Tool, ToolContext};
use serde_json::json;
use std::fs;
use std::time::Instant;
use tempfile::TempDir;

#[test]
fn test_read_file_performance() {
    // Benchmark ReadFileTool performance
    let temp_dir = TempDir::new().unwrap();

    // Create a test file
    let test_file = temp_dir.path().join("test.txt");
    let content = "Line content\n".repeat(1000); // 1000 lines
    fs::write(&test_file, &content).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Warm up
    for _ in 0..3 {
        let _ = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
    }

    // Benchmark
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let result = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
        assert!(result.is_ok());
    }

    let duration = start.elapsed();
    let avg_time = duration.as_millis() as f64 / iterations as f64;

    println!("ReadFileTool average: {:.2}ms per call", avg_time);

    // Should be reasonably fast (less than 50ms average; debug builds are slower)
    assert!(
        avg_time < 50.0,
        "ReadFileTool should be fast (avg {:.2}ms)",
        avg_time
    );
}

#[test]
fn test_bash_tool_performance() {
    // Benchmark BashTool performance
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);
    let tool = BashTool;

    let start = Instant::now();
    let result = tool.execute(json!({"command": "echo test"}), &ctx);
    assert!(result.is_ok(), "BashTool should execute echo successfully");

    let duration = start.elapsed();

    println!("BashTool single call: {:.2}ms", duration.as_millis());

    assert!(
        duration.as_millis() < 3000,
        "BashTool should be reasonably fast (took {}ms)",
        duration.as_millis()
    );
}

#[test]
fn test_cache_performance() {
    // Benchmark cache operations
    let cache = ToolCache::new_with_defaults();

    let _key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));
    let result = CachedToolResult {
        output: "test output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    // Benchmark put operations
    let start = Instant::now();
    let iterations = 1000;

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..iterations {
            let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
            cache.put(key, result.clone(), vec![], None).await;
        }
    });

    let duration = start.elapsed();
    let avg_time = duration.as_micros() as f64 / iterations as f64;

    println!("Cache put average: {:.2}μs per operation", avg_time);

    // Cache puts should be very fast
    assert!(
        avg_time < 1000.0,
        "Cache puts should be fast (avg {:.2}μs)",
        avg_time
    );

    // Benchmark get operations
    let start = Instant::now();

    rt.block_on(async {
        for i in 0..iterations {
            let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
            let _ = cache.get(&key).await;
        }
    });

    let duration = start.elapsed();
    let avg_time = duration.as_micros() as f64 / iterations as f64;

    println!("Cache get average: {:.2}μs per operation", avg_time);

    // Cache gets should be extremely fast
    assert!(
        avg_time < 500.0,
        "Cache gets should be very fast (avg {:.2}μs)",
        avg_time
    );
}

#[test]
fn test_concurrent_tool_execution() {
    // Test concurrent tool execution performance
    let temp_dir = TempDir::new().unwrap();

    // Create multiple test files
    for i in 0..10 {
        let file = temp_dir.path().join(format!("file_{}.txt", i));
        fs::write(&file, format!("content {}", i)).unwrap();
    }

    let ctx = ToolContext::new(&temp_dir);

    // Sequential execution
    let start = Instant::now();
    for i in 0..10 {
        let tool = ReadFileTool;
        let file = temp_dir.path().join(format!("file_{}.txt", i));
        let result = tool.execute(json!({"path": file.to_str().unwrap()}), &ctx);
        assert!(result.is_ok());
    }
    let sequential_time = start.elapsed();

    println!("Sequential execution: {:?}", sequential_time);

    // Concurrent execution using threads
    let start = Instant::now();
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let temp_dir = temp_dir.path().to_path_buf();
            std::thread::spawn(move || {
                let ctx = ToolContext::new(&temp_dir);
                let tool = ReadFileTool;
                let file = temp_dir.join(format!("file_{}.txt", i));
                tool.execute(json!({"path": file.to_str().unwrap()}), &ctx)
            })
        })
        .collect();

    for handle in handles {
        let result = handle.join().unwrap();
        assert!(result.is_ok());
    }
    let concurrent_time = start.elapsed();

    println!("Concurrent execution: {:?}", concurrent_time);

    // Concurrent should be faster or similar (may vary by system)
    // We don't assert strict inequality due to system variability
    println!(
        "Speedup: {:.2}x",
        sequential_time.as_secs_f64() / concurrent_time.as_secs_f64()
    );
}

#[test]
fn test_cache_hit_rate_performance() {
    // Test cache hit rate impact on performance
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    let content = "A".repeat(10000);
    fs::write(&test_file, &content).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Without cache
    let start = Instant::now();
    for _ in 0..100 {
        let _ = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
    }
    let uncached_time = start.elapsed();

    println!("100 uncached reads: {:?}", uncached_time);

    // With cache (using ToolCache directly)
    let cache = ToolCache::new_with_defaults();
    let file_path_str = test_file.to_str().unwrap().to_string();
    let key = CacheKey::new("read_file".to_string(), &json!({"path": file_path_str}));

    let rt = tokio::runtime::Runtime::new().unwrap();

    // Prime cache
    rt.block_on(async {
        let result = CachedToolResult {
            output: content.clone(),
            structured: None,
            success: true,
            error: None,
        };
        cache.put(key.clone(), result, vec![], None).await;
    });

    // Benchmark cache hits
    let start = Instant::now();
    rt.block_on(async {
        for _ in 0..100 {
            let _ = cache.get(&key).await;
        }
    });
    let cached_time = start.elapsed();

    println!("100 cached reads: {:?}", cached_time);

    // Cached should be significantly faster
    let speedup = uncached_time.as_secs_f64() / cached_time.as_secs_f64();
    println!("Cache speedup: {:.2}x", speedup);

    assert!(
        speedup > 2.0,
        "Cache should provide significant speedup ({:.2}x)",
        speedup
    );
}

#[test]
fn test_memory_usage_with_large_files() {
    // Test memory usage when handling large files
    let temp_dir = TempDir::new().unwrap();
    let large_file = temp_dir.path().join("large.txt");

    // Create a large file (1MB)
    let content = "X".repeat(1024 * 1024);
    fs::write(&large_file, &content).unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Read the file
    let start = Instant::now();
    let result = tool.execute(json!({"path": large_file.to_str().unwrap()}), &ctx);
    let duration = start.elapsed();

    assert!(result.is_ok());

    let output = result.unwrap();

    // Should complete in reasonable time
    assert!(
        duration.as_millis() < 1000,
        "Large file read should be fast ({:?})",
        duration
    );

    // Output size should be reasonable
    assert!(output.text.len() >= content.len());

    println!("Large file (1MB) read in {:?}", duration);
}

#[test]
fn test_memory_usage_with_many_cache_entries() {
    // Test memory usage with many cache entries
    let cache = ToolCache::new_with_defaults();
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Add many entries
    let start = Instant::now();
    rt.block_on(async {
        for i in 0..1000 {
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
    let duration = start.elapsed();

    println!("Added 1000 cache entries in {:?}", duration);

    // Check memory usage
    let metrics = rt.block_on(async { cache.get_metrics() });
    println!("Memory usage: {} bytes", metrics.current_memory_bytes);
    println!("Average entry size: {} bytes", metrics.avg_entry_size());

    // Should be reasonable (less than 10MB for 1000 small entries)
    assert!(
        metrics.current_memory_bytes < 10 * 1024 * 1024,
        "Memory usage should be reasonable"
    );
}

#[test]
fn test_scalability_with_file_operations() {
    // Test scalability with increasing file operations
    let temp_dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(&temp_dir);

    let sizes = vec![10, 100, 1000];
    let mut times = Vec::new();

    for size in sizes {
        // Create files
        for i in 0..size {
            let file = temp_dir
                .path()
                .join(format!("batch_{}_file_{}.txt", size, i));
            fs::write(&file, format!("content {}", i)).unwrap();
        }

        // Measure read time
        let start = Instant::now();
        for i in 0..size {
            let tool = ReadFileTool;
            let file = temp_dir
                .path()
                .join(format!("batch_{}_file_{}.txt", size, i));
            let result = tool.execute(json!({"path": file.to_str().unwrap()}), &ctx);
            assert!(result.is_ok());
        }
        let duration = start.elapsed();
        times.push((size, duration));

        println!("Read {} files in {:?}", size, duration);
    }

    // Check that scaling is reasonable (roughly linear)
    let (size1, time1) = &times[0];
    let (size2, time2) = &times[1];

    let time_per_file_1 = time1.as_secs_f64() / *size1 as f64;
    let time_per_file_2 = time2.as_secs_f64() / *size2 as f64;

    println!(
        "Time per file ({} ops): {:.3}ms",
        size1,
        time_per_file_1 * 1000.0
    );
    println!(
        "Time per file ({} ops): {:.3}ms",
        size2,
        time_per_file_2 * 1000.0
    );

    // Time per file shouldn't increase dramatically
    let ratio = time_per_file_2 / time_per_file_1;
    assert!(
        ratio < 5.0,
        "Time per file shouldn't increase dramatically (ratio: {:.2})",
        ratio
    );
}

#[test]
fn test_tool_selector_performance() {
    // Test ToolSelector performance
    use rustycode_tools::{ToolProfile, ToolSelector};

    let _selector = ToolSelector::new();

    // Benchmark profile detection
    let prompts = vec![
        "Show me the code",
        "Create a new file",
        "Fix the bug",
        "Deploy to prod",
        "What is this?",
        "Add feature X",
        "Debug this error",
        "Run tests",
    ];

    let start = Instant::now();
    let iterations = 10000;

    for _ in 0..iterations {
        for prompt in &prompts {
            let _ = ToolProfile::from_prompt(prompt);
        }
    }

    let duration = start.elapsed();
    let avg_time = duration.as_nanos() as f64 / (iterations * prompts.len()) as f64;

    println!("Profile detection: {:.2}ns per call", avg_time);

    // Should be fast (less than 1ms per call in debug builds)
    assert!(
        avg_time < 1_000_000.0,
        "Profile detection should be fast ({:.2}ns)",
        avg_time
    );
}

#[test]
fn test_concurrent_cache_access() {
    // Test cache performance under concurrent access
    let cache = std::sync::Arc::new(ToolCache::new_with_defaults());

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn 10 concurrent tasks
    for i in 0..10 {
        let cache = cache.clone();
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                for j in 0..100 {
                    let key = CacheKey::new(format!("tool_{}_{}", i, j), &json!({"arg": j}));
                    let result = CachedToolResult {
                        output: format!("output {}", j),
                        structured: None,
                        success: true,
                        error: None,
                    };
                    let key_clone = key.clone();
                    cache.put(key, result, vec![], None).await;
                    let _ = cache.get(&key_clone).await;
                }
            });
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.join().unwrap();
    }

    let duration = start.elapsed();

    println!("Concurrent cache access (1000 operations): {:?}", duration);

    // Should be fast even with concurrency
    assert!(
        duration.as_millis() < 5000,
        "Concurrent access should be fast ({:?})",
        duration
    );
}

#[test]
fn test_cache_memory_eviction_performance() {
    // Test performance of cache eviction under memory pressure
    let config = CacheConfig {
        max_memory_bytes: Some(100 * 1024), // 100KB limit
        ..Default::default()
    };
    let cache = ToolCache::new(config);
    let rt = tokio::runtime::Runtime::new().unwrap();

    // Add entries that will trigger eviction
    let start = Instant::now();
    rt.block_on(async {
        for i in 0..1000 {
            let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
            let result = CachedToolResult {
                output: "A".repeat(10 * 1024), // 10KB each
                structured: None,
                success: true,
                error: None,
            };
            cache.put(key, result, vec![], None).await;
        }
    });
    let duration = start.elapsed();

    println!(
        "Cache with eviction (1000 entries, 100KB limit): {:?}",
        duration
    );

    // Should handle eviction efficiently
    assert!(
        duration.as_millis() < 5000,
        "Eviction should be efficient ({:?})",
        duration
    );

    // Verify memory limit is respected
    let metrics = cache.get_metrics();
    assert!(
        metrics.current_memory_bytes <= 100 * 1024,
        "Memory limit should be enforced"
    );
}

#[test]
fn test_performance_regression_detection() {
    // Detect performance regressions by comparing against baseline
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let ctx = ToolContext::new(&temp_dir);
    let tool = ReadFileTool;

    // Warm up
    for _ in 0..5 {
        let _ = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
    }

    // Measure baseline
    let iterations = 100;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = tool.execute(json!({"path": test_file.to_str().unwrap()}), &ctx);
    }
    let baseline = start.elapsed();

    println!("Baseline: {:?} for {} iterations", baseline, iterations);
    let baseline_avg = baseline.as_micros() as f64 / iterations as f64;

    // This is a regression test - in CI, you'd compare against historical baselines
    // For now, just ensure it's not abnormally slow
    assert!(
        baseline_avg < 10_000.0,
        "Performance regression detected: {:.2}μs avg (baseline < 10ms)",
        baseline_avg
    );
}
