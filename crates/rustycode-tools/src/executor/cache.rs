//! Tool result caching system.
//!
//! This module provides:
//! - In-memory caching of tool execution results
//! - File-based cache invalidation for file operations
//! - TTL-based expiration for non-deterministic tools
//! - Thread-safe concurrent access with LRU eviction
//! - Performance monitoring and statistics

use lru::LruCache;
use parking_lot::RwLock;
use rustycode_protocol::tool_names as tn;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Cache key for tool results
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    pub tool_name: String,
    pub arguments_hash: u64, // Faster u64 hash instead of string
}

impl CacheKey {
    /// Create a cache key from tool name and arguments
    /// Uses fast FNV-style hashing instead of SHA-256 for performance
    pub fn new(tool_name: String, arguments: &serde_json::Value) -> Self {
        // Use a faster hashing approach for cache keys
        // We combine tool_name and a hash of arguments
        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        arguments.to_string().hash(&mut hasher);
        let arguments_hash = hasher.finish();

        Self {
            tool_name,
            arguments_hash,
        }
    }
}

/// Cache entry with expiration and metadata
#[derive(Debug, Clone)]
struct CacheEntry {
    result: CachedToolResult,
    cached_at: Instant,
    cached_at_system: SystemTime, // System time for file modification comparison
    ttl: Duration,
    dependencies: Vec<PathBuf>, // Files this result depends on for invalidation
    size_bytes: usize,          // Track memory usage
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }

    fn is_valid(&self) -> bool {
        if self.is_expired() {
            return false;
        }

        // Check if any dependencies have been modified
        for dep in &self.dependencies {
            if let Ok(metadata) = std::fs::metadata(dep) {
                if let Ok(modified) = metadata.modified() {
                    if modified > self.cached_at_system {
                        return false;
                    }
                }
            } else {
                // File doesn't exist anymore, invalidate cache
                return false;
            }
        }

        true
    }
}

/// Cached tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedToolResult {
    pub output: String,
    pub structured: Option<serde_json::Value>,
    pub success: bool,
    pub error: Option<String>,
}

impl CachedToolResult {
    /// Estimate memory usage of this result
    fn estimate_size(&self) -> usize {
        let output_size = self.output.len();
        let structured_size = self
            .structured
            .as_ref()
            .map(|v| v.to_string().len())
            .unwrap_or(0);
        let error_size = self.error.as_ref().map(|e| e.len()).unwrap_or(0);

        output_size + structured_size + error_size + 32 // Base overhead
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// Maximum number of entries in the cache
    pub max_entries: usize,
    /// Whether to enable file dependency tracking
    pub track_file_dependencies: bool,
    /// Maximum memory usage in bytes (None = unlimited)
    pub max_memory_bytes: Option<usize>,
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_mins(5), // 5 minutes
            max_entries: 1000,
            track_file_dependencies: true,
            max_memory_bytes: Some(100 * 1024 * 1024), // 100 MB default
            enable_metrics: true,
        }
    }
}

/// Cache performance metrics
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub total_puts: usize,
    pub current_memory_bytes: usize,
    pub current_entries: usize,
}

impl CacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64) / (total as f64)
        }
    }

    pub fn avg_entry_size(&self) -> usize {
        self.current_memory_bytes
            .checked_div(self.current_entries)
            .unwrap_or(0)
    }
}

/// Tool result cache with LRU eviction
pub struct ToolCache {
    entries: Arc<RwLock<LruCache<CacheKey, CacheEntry>>>,
    config: CacheConfig,
    metrics: Arc<RwLock<CacheMetrics>>,
}

impl ToolCache {
    pub fn new(config: CacheConfig) -> Self {
        let capacity =
            NonZeroUsize::new(config.max_entries).unwrap_or(NonZeroUsize::new(1000).unwrap());
        Self {
            entries: Arc::new(RwLock::new(LruCache::new(capacity))),
            config,
            metrics: Arc::new(RwLock::new(CacheMetrics::default())),
        }
    }

    pub fn new_with_defaults() -> Self {
        Self::new(CacheConfig::default())
    }

    /// Get a cached result if available and valid
    ///
    /// Uses `LruCache::get()` (write lock) to promote the entry to most-recently-used,
    /// ensuring correct LRU eviction behavior.
    pub async fn get(&self, key: &CacheKey) -> Option<CachedToolResult> {
        let mut entries = self.entries.write();
        let entry = entries.get(key)?;

        if entry.is_valid() {
            // Update metrics
            if self.config.enable_metrics {
                let mut metrics = self.metrics.write();
                metrics.hits += 1;
            }

            Some(entry.result.clone())
        } else {
            entries.pop(key);
            None
        }
    }

    /// Check if cache contains key and increment miss counter if not
    pub async fn get_or_track_miss(&self, key: &CacheKey) -> Option<CachedToolResult> {
        let result = self.get(key).await;
        if result.is_none() && self.config.enable_metrics {
            let mut metrics = self.metrics.write();
            metrics.misses += 1;
        }
        result
    }

    /// Store a result in the cache
    pub async fn put(
        &self,
        key: CacheKey,
        result: CachedToolResult,
        dependencies: Vec<PathBuf>,
        ttl: Option<Duration>,
    ) {
        let now = Instant::now();
        let now_system = SystemTime::now();
        let result_size = result.estimate_size();
        let deps_size: usize = dependencies.iter().map(|p| p.as_os_str().len()).sum();

        let entry = CacheEntry {
            result,
            cached_at: now,
            cached_at_system: now_system,
            ttl: ttl.unwrap_or(self.config.default_ttl),
            dependencies,
            size_bytes: result_size + deps_size,
        };

        let mut entries = self.entries.write();

        // Check memory limit before inserting
        if let Some(max_memory) = self.config.max_memory_bytes {
            let mut current_memory = self.metrics.read().current_memory_bytes;

            // Reject entry if it's larger than the entire cache
            if entry.size_bytes > max_memory {
                // Don't insert, just update metrics
                if self.config.enable_metrics {
                    let mut metrics = self.metrics.write();
                    metrics.total_puts += 1;
                    metrics.evictions += 1; // Count as eviction since we couldn't store it
                }
                return;
            }

            // Evict entries if needed to stay under memory limit
            while current_memory + entry.size_bytes > max_memory && !entries.is_empty() {
                if let Some((_, evicted)) = entries.pop_lru() {
                    current_memory -= evicted.size_bytes;
                    if self.config.enable_metrics {
                        self.metrics.write().evictions += 1;
                    }
                } else {
                    break;
                }
            }
        }

        // Insert the new entry
        let was_full = entries.len() >= entries.cap().get();
        entries.put(key.clone(), entry);

        // Update metrics
        if self.config.enable_metrics {
            let mut metrics = self.metrics.write();
            metrics.total_puts += 1;
            if was_full {
                metrics.evictions += 1;
            }

            // Recalculate memory and entry count
            metrics.current_memory_bytes = entries.iter().map(|(_, e)| e.size_bytes).sum();
            metrics.current_entries = entries.len();
        }
    }

    /// Extract file paths from tool arguments for dependency tracking
    pub fn extract_file_dependencies(
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Vec<PathBuf> {
        let mut deps = Vec::new();

        if let Some(obj) = arguments.as_object() {
            // For grep/glob, handle specially to avoid duplicates
            if tool_name == tn::GREP || tool_name == tn::GLOB {
                // Extract pattern directory
                if let Some(pattern) = obj.get("pattern").and_then(|v| v.as_str()) {
                    if let Some(parent) = PathBuf::from(pattern).parent() {
                        if !parent.as_os_str().is_empty() {
                            deps.push(parent.to_path_buf());
                        }
                    }
                }
                // Extract path
                if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                    deps.push(PathBuf::from(path));
                }
            } else {
                // For other tools, extract from common file-related keys
                let file_keys = ["path", "file", "file_path", "filepath", "src", "dest"];
                for key in file_keys {
                    if let Some(value) = obj.get(key) {
                        if let Some(path_str) = value.as_str() {
                            deps.push(PathBuf::from(path_str));
                        }
                    }
                }
            }
        }

        // Remove duplicates while preserving order
        deps.sort();
        deps.dedup();
        deps
    }

    /// Invalidate all cache entries
    pub async fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();

        if self.config.enable_metrics {
            let mut metrics = self.metrics.write();
            metrics.current_entries = 0;
            metrics.current_memory_bytes = 0;
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        let entries = self.entries.read();
        let valid_count = entries.iter().filter(|(_, e)| e.is_valid()).count();
        let expired_count = entries.len() - valid_count;

        let metrics = self.metrics.read();

        CacheStats {
            total_entries: entries.len(),
            valid_entries: valid_count,
            expired_entries: expired_count,
            metrics: metrics.clone(),
        }
    }

    /// Get cache metrics
    pub fn metrics(&self) -> CacheMetrics {
        self.metrics.read().clone()
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.write();
        let current_memory = metrics.current_memory_bytes;
        let current_entries = metrics.current_entries;
        *metrics = CacheMetrics {
            current_memory_bytes: current_memory,
            current_entries,
            ..Default::default()
        };
    }

    /// Prune invalid entries from the cache
    pub async fn prune(&self) -> usize {
        let mut entries = self.entries.write();
        let mut pruned = 0;
        let mut keys_to_remove = Vec::new();

        for (key, entry) in entries.iter() {
            if !entry.is_valid() {
                keys_to_remove.push(key.clone());
            }
        }

        for key in keys_to_remove {
            entries.pop(&key);
            pruned += 1;
        }

        // Update metrics
        if self.config.enable_metrics {
            let mut metrics = self.metrics.write();
            metrics.current_memory_bytes = entries.iter().map(|(_, e)| e.size_bytes).sum();
            metrics.current_entries = entries.len();
        }

        pruned
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub metrics: CacheMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn cache_returns_stored_result() {
        let cache = ToolCache::new_with_defaults();
        let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));

        let result = CachedToolResult {
            output: "test output".to_string(),
            structured: None,
            success: true,
            error: None,
        };

        cache.put(key.clone(), result.clone(), vec![], None).await;

        let retrieved = cache.get(&key).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().output, "test output");
    }

    #[tokio::test]
    async fn cache_returns_none_for_nonexistent_key() {
        let cache = ToolCache::new_with_defaults();
        let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));

        let retrieved = cache.get(&key).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn cache_respects_ttl() {
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
        assert!(cache.get(&key).await.is_some());

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be invalid after TTL
        assert!(cache.get(&key).await.is_none());
    }

    #[tokio::test]
    async fn cache_includes_file_dependencies() {
        let deps = ToolCache::extract_file_dependencies("Read", &json!({"path": "/tmp/test.txt"}));
        assert_eq!(deps, vec![PathBuf::from("/tmp/test.txt")]);

        let deps = ToolCache::extract_file_dependencies(
            "Write",
            &json!({"file": "/tmp/out.txt", "content": "test"}),
        );
        assert_eq!(deps, vec![PathBuf::from("/tmp/out.txt")]);
    }

    #[tokio::test]
    async fn cache_returns_stats() {
        let cache = ToolCache::new_with_defaults();

        let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));
        let result = CachedToolResult {
            output: "test output".to_string(),
            structured: None,
            success: true,
            error: None,
        };

        cache.put(key.clone(), result, vec![], None).await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.valid_entries, 1);
        assert_eq!(stats.expired_entries, 0);
    }

    #[tokio::test]
    async fn cache_tracks_metrics() {
        let config = CacheConfig {
            enable_metrics: true,
            ..CacheConfig::default()
        };
        let cache = ToolCache::new(config);

        let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));
        let result = CachedToolResult {
            output: "test output".to_string(),
            structured: None,
            success: true,
            error: None,
        };

        cache.put(key.clone(), result.clone(), vec![], None).await;

        // Cache hit
        cache.get(&key).await;

        // Cache miss - use get_get_or_track_miss to register miss
        let missing_key = CacheKey::new("other_tool".to_string(), &json!({"arg": 2}));
        cache.get_or_track_miss(&missing_key).await;

        let metrics = cache.metrics();
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.total_puts, 1);
        assert!(metrics.hit_rate() > 0.0);
    }

    #[tokio::test]
    async fn cache_clears_all_entries() {
        let cache = ToolCache::new_with_defaults();

        let key = CacheKey::new("test_tool".to_string(), &json!({"arg": "value"}));
        let result = CachedToolResult {
            output: "test output".to_string(),
            structured: None,
            success: true,
            error: None,
        };

        cache.put(key.clone(), result, vec![], None).await;
        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn cache_prunes_expired_entries() {
        let config = CacheConfig {
            default_ttl: Duration::from_millis(100),
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

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Prune expired entries
        let pruned = cache.prune().await;
        assert!(pruned > 0);

        let stats = cache.stats().await;
        assert_eq!(stats.valid_entries, 0);
    }
}
