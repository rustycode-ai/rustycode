//! End-to-end tests for Caching Behavior
//!
//! Tests the complete caching system:
//! - Cache hit/miss scenarios
//! - File-based cache invalidation
//! - LRU eviction
//! - Cache statistics (hit rate, memory usage)
//! - Concurrent access

use rustycode_tools::cache::{CacheConfig, CacheKey, CachedToolResult, ToolCache};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_cache_hit_and_miss() {
    // Test basic cache hit and miss scenarios
    let cache = ToolCache::new_with_defaults();
    let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));

    // First get should be a miss
    let result = cache.get(&key).await;
    assert!(result.is_none(), "First access should be a cache miss");

    // Store a result
    let cached_result = CachedToolResult {
        output: "test output".to_string(),
        structured: None,
        success: true,
        error: None,
    };
    cache.put(key.clone(), cached_result, vec![], None).await;

    // Second get should be a hit
    let result = cache.get(&key).await;
    assert!(result.is_some(), "Second access should be a cache hit");
    assert_eq!(result.unwrap().output, "test output");
}

#[tokio::test]
async fn test_cache_ttl_expiration() {
    // Test that cache entries expire after TTL
    let config = CacheConfig {
        default_ttl: Duration::from_millis(100),
        ..Default::default()
    };
    let cache = ToolCache::new(config);
    let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));

    let result = CachedToolResult {
        output: "test output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    cache.put(key.clone(), result, vec![], None).await;

    // Should be valid immediately
    assert!(
        cache.get(&key).await.is_some(),
        "Should be valid immediately"
    );

    // Wait for TTL to expire
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Should be invalid after TTL
    assert!(cache.get(&key).await.is_none(), "Should expire after TTL");
}

#[tokio::test]
async fn test_cache_file_dependency_invalidation() {
    // Test that cache invalidates when dependencies change
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "initial content").unwrap();

    let cache = ToolCache::new_with_defaults();
    let key = CacheKey::new(
        "read_file".to_string(),
        &json!({"path": test_file.to_str().unwrap()}),
    );

    let result = CachedToolResult {
        output: "initial output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    // Store with file dependency
    cache
        .put(key.clone(), result, vec![test_file.clone()], None)
        .await;

    // Should be valid initially
    assert!(
        cache.get(&key).await.is_some(),
        "Should be valid before file change"
    );

    // Wait a bit to ensure different modification time
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Modify the file
    std::fs::write(&test_file, "modified content").unwrap();

    // Cache should be invalidated
    assert!(
        cache.get(&key).await.is_none(),
        "Should be invalid after file modification"
    );
}

#[tokio::test]
async fn test_cache_lru_eviction() {
    // Test LRU eviction when cache is full
    let config = CacheConfig {
        max_entries: 3,
        ..Default::default()
    };
    let cache = ToolCache::new(config);

    // Fill cache to capacity
    for i in 0..3 {
        let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
        let result = CachedToolResult {
            output: format!("output {}", i),
            structured: None,
            success: true,
            error: None,
        };
        cache.put(key, result, vec![], None).await;
    }

    // Access first entry to make it recently used
    let key0 = CacheKey::new("tool_0".to_string(), &json!({"arg": 0}));
    cache.get(&key0).await;

    // Add one more entry (should evict tool_1, the LRU)
    let key3 = CacheKey::new("tool_3".to_string(), &json!({"arg": 3}));
    let result = CachedToolResult {
        output: "output 3".to_string(),
        structured: None,
        success: true,
        error: None,
    };
    cache.put(key3, result, vec![], None).await;

    // tool_0 should still be there (recently accessed)
    assert!(
        cache.get(&key0).await.is_some(),
        "tool_0 should still be cached"
    );

    // tool_1 should be evicted (least recently used)
    let key1 = CacheKey::new("tool_1".to_string(), &json!({"arg": 1}));
    assert!(cache.get(&key1).await.is_none(), "tool_1 should be evicted");
}

#[tokio::test]
async fn test_cache_statistics_tracking() {
    // Test that cache statistics are accurately tracked
    let cache = ToolCache::new_with_defaults();

    // Perform some operations
    let key1 = CacheKey::new("tool_1".to_string(), &json!({"arg": 1}));
    let key2 = CacheKey::new("tool_2".to_string(), &json!({"arg": 2}));

    let result = CachedToolResult {
        output: "output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    // Put one entry
    cache.put(key1.clone(), result.clone(), vec![], None).await;

    // Cache hit
    cache.get(&key1).await;

    // Cache miss
    cache.get_or_track_miss(&key2).await;

    // Get statistics
    let stats = cache.stats().await;
    assert_eq!(stats.total_entries, 1);
    assert_eq!(stats.valid_entries, 1);

    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 1);
    assert_eq!(metrics.misses, 1);
    assert_eq!(metrics.total_puts, 1);

    // Check hit rate
    let hit_rate = metrics.hit_rate();
    assert!(
        hit_rate > 0.0 && hit_rate <= 1.0,
        "Hit rate should be between 0 and 1"
    );
}

#[tokio::test]
async fn test_cache_memory_limit_eviction() {
    // Test that cache respects memory limits
    let config = CacheConfig {
        max_memory_bytes: Some(1000), // Very small limit
        ..Default::default()
    };
    let cache = ToolCache::new(config);

    // Add multiple small entries that together exceed the limit
    // Each entry is ~300 bytes (200 bytes content + overhead)
    for i in 0..5 {
        let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
        let result = CachedToolResult {
            output: "A".repeat(200), // 200 bytes
            structured: None,
            success: true,
            error: None,
        };
        cache.put(key, result, vec![], None).await;
    }

    // Cache should evict entries to stay under limit
    let metrics = cache.get_metrics();
    assert!(
        metrics.current_memory_bytes <= 1000,
        "Memory usage {} should stay under limit 1000",
        metrics.current_memory_bytes
    );

    // Should have evicted some entries
    assert!(
        metrics.evictions > 0,
        "Should have evicted entries to stay under memory limit"
    );
}

#[tokio::test]
async fn test_cache_key_uniqueness() {
    // Test that different arguments produce different cache keys
    let cache = ToolCache::new_with_defaults();

    let key1 = CacheKey::new("tool".to_string(), &json!({"arg": "value1"}));
    let key2 = CacheKey::new("tool".to_string(), &json!({"arg": "value2"}));

    let result1 = CachedToolResult {
        output: "output1".to_string(),
        structured: None,
        success: true,
        error: None,
    };
    let result2 = CachedToolResult {
        output: "output2".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    cache.put(key1.clone(), result1, vec![], None).await;
    cache.put(key2.clone(), result2, vec![], None).await;

    // Both should be stored separately
    assert!(cache.get(&key1).await.is_some());
    assert!(cache.get(&key2).await.is_some());
}

#[tokio::test]
async fn test_cache_clear() {
    // Test cache clearing
    let cache = ToolCache::new_with_defaults();

    // Add some entries
    for i in 0..5 {
        let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
        let result = CachedToolResult {
            output: format!("output {}", i),
            structured: None,
            success: true,
            error: None,
        };
        cache.put(key, result, vec![], None).await;
    }

    // Verify entries exist
    let stats = cache.stats().await;
    assert_eq!(stats.total_entries, 5);

    // Clear cache
    cache.clear().await;

    // Verify cache is empty
    let stats = cache.stats().await;
    assert_eq!(stats.total_entries, 0);
}

#[tokio::test]
async fn test_cache_prune_expired() {
    // Test pruning of expired entries
    let config = CacheConfig {
        default_ttl: Duration::from_millis(50),
        ..Default::default()
    };
    let cache = ToolCache::new(config);

    // Add multiple entries
    for i in 0..5 {
        let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
        let result = CachedToolResult {
            output: format!("output {}", i),
            structured: None,
            success: true,
            error: None,
        };
        cache.put(key, result, vec![], None).await;
    }

    // Wait for expiration
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Prune expired entries
    let pruned = cache.prune().await;
    assert!(pruned > 0, "Should prune some expired entries");

    // Verify no valid entries remain
    let stats = cache.stats().await;
    assert_eq!(stats.valid_entries, 0);
}

#[tokio::test]
async fn test_cache_file_dependency_extraction() {
    // Test extraction of file dependencies from tool arguments
    let test_cases = vec![
        (
            "read_file",
            json!({"path": "/tmp/test.txt"}),
            vec![PathBuf::from("/tmp/test.txt")],
        ),
        (
            "write_file",
            json!({"file": "/tmp/out.txt", "content": "test"}),
            vec![PathBuf::from("/tmp/out.txt")],
        ),
        (
            "grep",
            json!({"pattern": "test", "path": "/src"}),
            vec![PathBuf::from("/src")],
        ),
    ];

    for (tool_name, args, expected_deps) in test_cases {
        let deps = ToolCache::extract_file_dependencies(tool_name, &args);
        assert_eq!(
            deps, expected_deps,
            "Dependencies for {} should match expected",
            tool_name
        );
    }
}

#[tokio::test]
async fn test_cache_concurrent_access() {
    // Test concurrent cache access
    let cache = std::sync::Arc::new(ToolCache::new_with_defaults());
    let mut handles = vec![];

    // Spawn multiple concurrent tasks
    for i in 0..10 {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            let key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
            let result = CachedToolResult {
                output: format!("output {}", i),
                structured: None,
                success: true,
                error: None,
            };

            // Put and get
            cache_clone.put(key.clone(), result, vec![], None).await;
            cache_clone.get(&key).await
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_some(), "Concurrent access should succeed");
    }

    // Verify all entries are cached
    let stats = cache.stats().await;
    assert_eq!(stats.total_entries, 10);
}

#[tokio::test]
async fn test_cache_hit_rate_calculation() {
    // Test hit rate calculation
    let cache = ToolCache::new_with_defaults();

    // Perform operations with known hit/miss pattern
    let key = CacheKey::new("tool".to_string(), &json!({"arg": "value"}));
    let result = CachedToolResult {
        output: "output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    // 2 hits
    cache.put(key.clone(), result.clone(), vec![], None).await;
    cache.get(&key).await;
    cache.get(&key).await;

    // 3 misses
    for i in 0..3 {
        let missing_key = CacheKey::new(format!("tool_{}", i), &json!({"arg": i}));
        cache.get_or_track_miss(&missing_key).await;
    }

    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 2);
    assert_eq!(metrics.misses, 3);

    let hit_rate = metrics.hit_rate();
    assert!(
        (hit_rate - 0.4).abs() < 0.01,
        "Hit rate should be ~40% (2/5)"
    );
}

#[tokio::test]
async fn test_cache_average_entry_size() {
    // Test average entry size calculation
    let cache = ToolCache::new_with_defaults();

    // Add entries with different sizes
    let key1 = CacheKey::new("tool_1".to_string(), &json!({"arg": 1}));
    let result1 = CachedToolResult {
        output: "A".repeat(100),
        structured: None,
        success: true,
        error: None,
    };

    let key2 = CacheKey::new("tool_2".to_string(), &json!({"arg": 2}));
    let result2 = CachedToolResult {
        output: "B".repeat(200),
        structured: None,
        success: true,
        error: None,
    };

    cache.put(key1, result1, vec![], None).await;
    cache.put(key2, result2, vec![], None).await;

    let metrics = cache.get_metrics();
    let avg_size = metrics.avg_entry_size();

    assert!(avg_size > 0, "Average size should be positive");
    // Should be between 100 and 200 (plus overhead)
    assert!(
        (100..=250).contains(&avg_size),
        "Average size should be reasonable"
    );
}

#[tokio::test]
async fn test_cache_custom_ttl_per_entry() {
    // Test custom TTL per cache entry
    let cache = ToolCache::new_with_defaults();

    let key1 = CacheKey::new("tool_1".to_string(), &json!({"arg": 1}));
    let key2 = CacheKey::new("tool_2".to_string(), &json!({"arg": 2}));

    let result = CachedToolResult {
        output: "output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    // Entry with short TTL
    cache
        .put(
            key1.clone(),
            result.clone(),
            vec![],
            Some(Duration::from_millis(50)),
        )
        .await;

    // Entry with long TTL
    cache
        .put(key2.clone(), result, vec![], Some(Duration::from_secs(10)))
        .await;

    // Both valid immediately
    assert!(cache.get(&key1).await.is_some());
    assert!(cache.get(&key2).await.is_some());

    // Wait for first to expire
    tokio::time::sleep(Duration::from_millis(100)).await;

    // First should be expired
    assert!(cache.get(&key1).await.is_none());

    // Second should still be valid
    assert!(cache.get(&key2).await.is_some());
}

#[tokio::test]
async fn test_cache_with_missing_dependency_files() {
    // Test cache invalidation when dependency files are deleted
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "content").unwrap();

    let cache = ToolCache::new_with_defaults();
    let key = CacheKey::new(
        "read_file".to_string(),
        &json!({"path": test_file.to_str().unwrap()}),
    );

    let result = CachedToolResult {
        output: "output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    cache
        .put(key.clone(), result, vec![test_file.clone()], None)
        .await;

    // Should be valid initially
    assert!(cache.get(&key).await.is_some());

    // Delete the dependency file
    std::fs::remove_file(&test_file).unwrap();

    // Cache should be invalidated
    assert!(
        cache.get(&key).await.is_none(),
        "Should be invalid when file deleted"
    );
}

#[tokio::test]
async fn test_cache_metrics_reset() {
    // Test resetting cache metrics
    let cache = ToolCache::new_with_defaults();

    // Generate some activity
    let key = CacheKey::new("tool".to_string(), &json!({"arg": "value"}));
    let result = CachedToolResult {
        output: "output".to_string(),
        structured: None,
        success: true,
        error: None,
    };

    cache.put(key.clone(), result, vec![], None).await;
    cache.get(&key).await;
    cache
        .get_or_track_miss(&CacheKey::new("other".to_string(), &json!({})))
        .await;

    // Verify metrics are non-zero
    let metrics = cache.get_metrics();
    assert!(metrics.hits > 0 || metrics.misses > 0);

    // Reset metrics
    cache.reset_metrics();

    // Verify metrics are reset
    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 0);
    assert_eq!(metrics.misses, 0);
    assert_eq!(metrics.total_puts, 0);
}
