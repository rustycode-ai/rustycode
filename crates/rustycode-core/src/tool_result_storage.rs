//! Tool result spill-to-disk storage with caching.
//!
//! When tool outputs exceed a size threshold, the full result is written to a
//! temporary file and only a compact preview is kept in the conversation context.
//! This prevents context bloat from large command outputs (build logs, file trees,
//! grep results) while preserving the complete output for the user to inspect.
//!
//! ## Tool Result Caching
//!
//! Caches tool results by content hash to avoid re-executing identical calls.
//! Cache key: `tool_name + sha256(params)`. Supports TTL-based expiry and LRU eviction.
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_core::tool_result_storage::{ToolResultStorage, ToolResultCache, CacheConfig};
//!
//! let mut storage = ToolResultStorage::new(base_dir, session_id);
//! let cache = ToolResultCache::new(CacheConfig {
//!     max_entries: 1000,
//!     ttl_seconds: 300, // 5 minutes
//! });
//!
//! // Check cache before executing tool
//! if let Some(cached) = cache.get("read_file", &params) {
//!     // Use cached result
//! } else {
//!     // Execute tool, then cache result
//!     let result = execute_tool(...);
//!     cache.insert("read_file", &params, &result);
//! }
//! ```

use lru::LruCache;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Default maximum size (in bytes) before a tool result is spilled to disk.
const DEFAULT_PERSISTENCE_THRESHOLD: usize = 50_000; // 50 KB

/// Maximum preview length kept in context (in bytes).
const PREVIEW_BUDGET: usize = 2_000; // 2 KB

/// Maximum per-message aggregate tool result budget (in bytes).
const DEFAULT_PER_MESSAGE_BUDGET: usize = 200_000; // 200 KB

/// Manages spill-to-disk storage for large tool results within a session.
#[derive(Debug)]
pub struct ToolResultStorage {
    /// Directory where persisted results are stored.
    storage_dir: PathBuf,
    /// Size threshold (bytes) above which results are spilled to disk.
    persistence_threshold: usize,
    /// Per-message budget for aggregate tool result content.
    per_message_budget: usize,
    /// Tracks the cumulative size of tool results in the current message turn.
    current_message_bytes: usize,
    /// IDs of tool results that have been persisted (to maintain cache stability).
    persisted_ids: Vec<String>,
}

impl ToolResultStorage {
    /// Create a new storage manager for a session.
    ///
    /// Results are stored in `{base_dir}/{session_id}/tool-results/`.
    pub fn new(base_dir: &Path, session_id: &str) -> Self {
        let storage_dir = base_dir.join(session_id).join("tool-results");

        Self {
            storage_dir,
            persistence_threshold: DEFAULT_PERSISTENCE_THRESHOLD,
            per_message_budget: DEFAULT_PER_MESSAGE_BUDGET,
            current_message_bytes: 0,
            persisted_ids: Vec::new(),
        }
    }

    /// Set a custom persistence threshold.
    pub fn with_threshold(mut self, bytes: usize) -> Self {
        self.persistence_threshold = bytes;
        self
    }

    /// Set a custom per-message budget.
    pub fn with_per_message_budget(mut self, bytes: usize) -> Self {
        self.per_message_budget = bytes;
        self
    }

    /// Process a tool result, potentially spilling it to disk.
    ///
    /// Returns a `ProcessedResult` containing either the original content
    /// (if small enough) or a preview + file path (if spilled to disk).
    pub fn process_result(
        &mut self,
        tool_use_id: &str,
        tool_name: &str,
        content: &str,
    ) -> ProcessedResult {
        let content_bytes = content.len();
        self.current_message_bytes += content_bytes;

        // Check if this result should be persisted
        let exceeds_threshold = content_bytes > self.persistence_threshold;
        let message_over_budget = self.current_message_bytes > self.per_message_budget;

        if !exceeds_threshold && !message_over_budget {
            return ProcessedResult::InContext {
                content: content.to_string(),
            };
        }

        // Already persisted — skip re-persisting
        if self.persisted_ids.contains(&tool_use_id.to_string()) {
            return ProcessedResult::InContext {
                content: content.to_string(),
            };
        }

        // Spill to disk
        match self.persist_to_disk(tool_use_id, tool_name, content) {
            Ok(file_path) => {
                let preview = generate_preview(content, PREVIEW_BUDGET);
                self.persisted_ids.push(tool_use_id.to_string());

                ProcessedResult::Persisted {
                    preview,
                    file_path,
                    original_size: content_bytes,
                }
            }
            Err(e) => {
                // If persistence fails, truncate in-context as fallback
                let truncated = generate_preview(content, PREVIEW_BUDGET);
                ProcessedResult::Fallback {
                    preview: truncated,
                    original_size: content_bytes,
                    error: e.to_string(),
                }
            }
        }
    }

    /// Reset the per-message byte counter (call at the start of each turn).
    pub fn reset_message_budget(&mut self) {
        self.current_message_bytes = 0;
    }

    /// Write the full result to disk.
    fn persist_to_disk(
        &self,
        tool_use_id: &str,
        tool_name: &str,
        content: &str,
    ) -> Result<PathBuf, StorageError> {
        // Ensure storage directory exists
        fs::create_dir_all(&self.storage_dir)
            .map_err(|e| StorageError::Io(self.storage_dir.clone(), e))?;

        // Sanitize tool_use_id for filename
        let safe_id = tool_use_id.replace(['/', '\\', '.'], "_");

        let ext = if content.trim_start().starts_with('{') || content.trim_start().starts_with('[')
        {
            "json"
        } else {
            "txt"
        };

        let filename = format!("{}-{}.{}", tool_name, safe_id, ext);
        let file_path = self.storage_dir.join(&filename);

        // Use atomic write to avoid corruption
        let tmp_path = file_path.with_extension(format!("{}.tmp", ext));
        fs::write(&tmp_path, content).map_err(|e| StorageError::Io(tmp_path.clone(), e))?;
        fs::rename(&tmp_path, &file_path).map_err(|e| StorageError::Io(file_path.clone(), e))?;

        Ok(file_path)
    }

    /// Read a persisted result from disk.
    pub fn read_persisted(&self, file_path: &Path) -> Result<String, StorageError> {
        fs::read_to_string(file_path).map_err(|e| StorageError::Io(file_path.to_path_buf(), e))
    }

    /// Clean up all persisted results for this session.
    pub fn cleanup(&self) -> Result<(), StorageError> {
        if self.storage_dir.exists() {
            fs::remove_dir_all(&self.storage_dir)
                .map_err(|e| StorageError::Io(self.storage_dir.clone(), e))?;
        }
        Ok(())
    }

    /// Get the storage directory path.
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    /// Number of persisted results.
    pub fn persisted_count(&self) -> usize {
        self.persisted_ids.len()
    }
}

/// The result of processing a tool output.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ProcessedResult {
    /// Content is small enough to keep in context as-is.
    InContext { content: String },
    /// Content was persisted to disk; preview kept in context.
    Persisted {
        preview: String,
        file_path: PathBuf,
        original_size: usize,
    },
    /// Persistence failed; truncated preview in context.
    Fallback {
        preview: String,
        original_size: usize,
        error: String,
    },
}

impl ProcessedResult {
    /// Format the result for inclusion in the conversation context.
    pub fn to_context_string(&self) -> String {
        match self {
            Self::InContext { content } => content.clone(),
            Self::Persisted {
                preview,
                file_path,
                original_size,
            } => {
                let size_str = format_size(*original_size);
                format!(
                    "<persisted-output path=\"{}\" size=\"{}\">\n{}\n\n[... {} total — full output saved to {}]\n</persisted-output>",
                    file_path.display(),
                    size_str,
                    preview,
                    size_str,
                    file_path.display(),
                )
            }
            Self::Fallback {
                preview,
                original_size,
                error,
            } => {
                let size_str = format_size(*original_size);
                format!(
                    "{}\n\n[... {} total — truncated (storage error: {})]",
                    preview, size_str, error,
                )
            }
        }
    }

    /// Check if the result was persisted to disk.
    pub fn is_persisted(&self) -> bool {
        matches!(self, Self::Persisted { .. })
    }

    /// Get the file path if persisted.
    pub fn file_path(&self) -> Option<&Path> {
        match self {
            Self::Persisted { file_path, .. } => Some(file_path),
            _ => None,
        }
    }
}

/// Generate a preview of the given content, up to `budget` bytes.
///
/// Tries to break at a newline boundary for readability.
fn generate_preview(content: &str, budget: usize) -> String {
    if content.len() <= budget {
        return content.to_string();
    }

    // Find a good break point near the budget limit
    let bytes = content.as_bytes();
    let mut end = budget.min(bytes.len());

    // Try to break at a newline
    if let Some(pos) = bytes[..end].iter().rposition(|&b| b == b'\n') {
        end = pos;
    }

    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Format a byte count for human-readable display.
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} bytes", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Errors from tool result storage operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    #[error("IO error for {0}: {1}")]
    Io(PathBuf, std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_storage() -> (tempfile::TempDir, ToolResultStorage) {
        let tmp = tempfile::tempdir().unwrap();
        let storage = ToolResultStorage::new(tmp.path(), "test-session");
        (tmp, storage)
    }

    #[test]
    fn small_result_stays_in_context() {
        let (_tmp, mut storage) = setup_storage();
        let result = storage.process_result("tool_1", "read_file", "hello world");
        assert!(matches!(result, ProcessedResult::InContext { .. }));
        assert_eq!(result.to_context_string(), "hello world");
    }

    #[test]
    fn large_result_spills_to_disk() {
        let (_tmp, mut storage) = setup_storage();
        let large_content = "x".repeat(60_000); // 60 KB
        let result = storage.process_result("tool_2", "bash", &large_content);

        assert!(result.is_persisted());
        let path = result.file_path().unwrap().to_path_buf();
        assert!(path.exists());

        // Verify full content is on disk
        let disk_content = fs::read_to_string(&path).unwrap();
        assert_eq!(disk_content.len(), 60_000);

        // Verify context string contains preview + metadata
        let ctx = result.to_context_string();
        assert!(ctx.contains("<persisted-output"));
        assert!(ctx.contains("full output saved to"));
    }

    #[test]
    fn persisted_result_can_be_read_back() {
        let (_tmp, mut storage) = setup_storage();
        let content = "y".repeat(100_000);
        let result = storage.process_result("tool_3", "bash", &content);

        if let ProcessedResult::Persisted { file_path, .. } = &result {
            let read_back = storage.read_persisted(file_path).unwrap();
            assert_eq!(read_back, content);
        }
    }

    #[test]
    fn preview_truncates_at_newline() {
        let preview = generate_preview("line1\nline2\nline3\nline4", 12);
        // Should break at newline before the budget
        assert!(preview.contains("line1"));
        assert!(!preview.contains("line3"));
    }

    #[test]
    fn message_budget_triggers_persistence() {
        let (_tmp, mut storage) = setup_storage();
        // Manually set low budget via struct fields
        storage.persistence_threshold = 1_000_000; // 1MB — won't trigger per-result
        storage.per_message_budget = 100;

        let result = storage.process_result("tool_4", "read", &"a".repeat(50));
        assert!(matches!(result, ProcessedResult::InContext { .. }));

        // Second result pushes total over budget — should spill despite being small
        let result2 = storage.process_result("tool_5", "read", &"b".repeat(80));
        assert!(result2.is_persisted());
    }

    #[test]
    fn reset_message_budget_clears_counter() {
        let (_tmp, mut storage) = setup_storage();
        storage.current_message_bytes = 500_000;
        storage.reset_message_budget();
        assert_eq!(storage.current_message_bytes, 0);
    }

    #[test]
    fn cleanup_removes_storage_dir() {
        let (_tmp, mut storage) = setup_storage();
        let _ = storage.process_result("tool_6", "bash", &"z".repeat(60_000));
        assert!(storage.storage_dir.exists());

        storage.cleanup().unwrap();
        assert!(!storage.storage_dir.exists());
    }

    #[test]
    fn json_content_gets_json_extension() {
        let (_tmp, mut storage) = setup_storage();
        let large_json = format!("{{\"data\": \"{}\"}}", "x".repeat(60_000));
        let result = storage.process_result("tool_7", "glob", &large_json);

        if let ProcessedResult::Persisted { file_path, .. } = &result {
            assert!(file_path.extension().unwrap() == "json");
        }
    }

    #[test]
    fn format_size_human_readable() {
        assert_eq!(format_size(500), "500 bytes");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
    }

    #[test]
    fn same_id_not_persisted_twice() {
        let (_tmp, mut storage) = setup_storage();
        let content = "a".repeat(60_000);

        let result1 = storage.process_result("tool_8", "bash", &content);
        assert!(result1.is_persisted());

        // Same ID again — should return InContext (already persisted)
        let result2 = storage.process_result("tool_8", "bash", &content);
        assert!(matches!(result2, ProcessedResult::InContext { .. }));
    }
}

// ── Tool Result Caching ─────────────────────────────────────────────────────

/// Configuration for tool result caching
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries in the cache
    pub max_entries: usize,
    /// Time-to-live for cached entries
    pub ttl: Duration,
    /// Minimum content size to cache (avoid caching trivial results)
    pub min_size_to_cache: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            ttl: Duration::from_mins(5), // 5 minutes
            min_size_to_cache: 100,      // Only cache results > 100 bytes
        }
    }
}

/// Cached tool result with metadata
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// The cached content
    pub content: String,
    /// When this result was cached
    pub cached_at: Instant,
    /// Token count estimate (content.len() / 4)
    pub token_count: usize,
    /// Cache key hash (for debugging/inspection)
    pub cache_key: String,
}

impl CachedResult {
    /// Check if the cached result has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }

    /// Get the age of this cached result
    pub fn age(&self) -> Duration {
        self.cached_at.elapsed()
    }
}

/// Cache for tool results to avoid re-executing identical calls
///
/// Cache key is computed as: `sha256(tool_name + ":" + params_json)`
///
/// ## Example
///
/// ```ignore
/// let mut cache = ToolResultCache::new(CacheConfig::default());
///
/// // Before executing a tool, check cache
/// let params = serde_json::json!({"path": "src/main.rs"});
/// if let Some(cached) = cache.get("read_file", &params) {
///     // Use cached result - saved tool execution time and tokens
///     println!("Cache hit! Saved ~{} tokens", cached.token_count);
/// } else {
///     // Execute tool and cache result
///     let result = execute_tool("read_file", &params)?;
///     cache.insert("read_file", &params, &result);
/// }
/// ```
#[derive(Debug)]
pub struct ToolResultCache {
    /// LRU cache for automatic eviction
    cache: LruCache<String, CachedResult>,
    /// Configuration
    config: CacheConfig,
    /// Statistics
    stats: CacheStats,
}

/// Cache statistics for monitoring effectiveness
#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: usize,
    /// Number of cache misses
    pub misses: usize,
    /// Number of entries inserted
    pub inserts: usize,
    /// Number of entries evicted due to TTL
    pub ttl_evictions: usize,
    /// Number of entries evicted due to LRU
    pub lru_evictions: usize,
    /// Total tokens saved (sum of hit token counts)
    pub tokens_saved: usize,
}

impl CacheStats {
    /// Get the cache hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        (self.hits as f64 / total as f64) * 100.0
    }

    /// Reset all statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

impl ToolResultCache {
    /// Create a new tool result cache with the given configuration
    pub fn new(config: CacheConfig) -> Self {
        let capacity = std::num::NonZeroUsize::new(config.max_entries)
            .unwrap_or_else(|| std::num::NonZeroUsize::new(1000).expect("1000 is non-zero"));
        let cache = LruCache::new(capacity);

        Self {
            cache,
            config,
            stats: CacheStats::default(),
        }
    }

    /// Compute cache key from tool name and parameters
    pub fn compute_cache_key(tool_name: &str, params: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        let key_string = format!("{}:{}", tool_name, params);
        hasher.update(key_string.as_bytes());
        hasher
            .finalize()
            .iter()
            .fold(String::with_capacity(64), |mut acc, byte| {
                use std::fmt::Write;
                let _ = write!(acc, "{byte:02x}");
                acc
            })
    }

    /// Get a cached result if available and not expired
    pub fn get(&mut self, tool_name: &str, params: &serde_json::Value) -> Option<&CachedResult> {
        let key = Self::compute_cache_key(tool_name, params);

        // Check if entry exists and is not expired
        let is_expired = self
            .cache
            .get(&key)
            .is_some_and(|r| r.is_expired(self.config.ttl));

        if is_expired {
            self.cache.pop(&key);
            self.stats.ttl_evictions = self.stats.ttl_evictions.saturating_add(1);
            self.stats.misses = self.stats.misses.saturating_add(1);
            return None;
        }

        if let Some(result) = self.cache.get(&key) {
            self.stats.hits = self.stats.hits.saturating_add(1);
            self.stats.tokens_saved = self.stats.tokens_saved.saturating_add(result.token_count);
            return Some(result);
        }

        self.stats.misses = self.stats.misses.saturating_add(1);
        None
    }

    /// Insert a result into the cache
    pub fn insert(&mut self, tool_name: &str, params: &serde_json::Value, content: &str) -> bool {
        // Skip if too small to be worth caching
        if content.len() < self.config.min_size_to_cache {
            return false;
        }

        let key = Self::compute_cache_key(tool_name, params);
        let token_count = content.len() / 4; // Rough estimate

        let cached_result = CachedResult {
            content: content.to_string(),
            cached_at: Instant::now(),
            token_count,
            cache_key: key.clone(),
        };

        // Check if we're evicting an existing entry (LRU eviction)
        let was_evicted = self.cache.push(key.clone(), cached_result).is_some();
        if was_evicted {
            self.stats.lru_evictions = self.stats.lru_evictions.saturating_add(1);
        }

        self.stats.inserts = self.stats.inserts.saturating_add(1);
        true
    }

    /// Remove a specific entry from the cache
    pub fn remove(&mut self, tool_name: &str, params: &serde_json::Value) -> bool {
        let key = Self::compute_cache_key(tool_name, params);
        self.cache.pop(&key).is_some()
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get the number of cached entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Get mutable reference to statistics
    pub fn stats_mut(&mut self) -> &mut CacheStats {
        &mut self.stats
    }

    /// Get all cache keys (for inspection/debugging)
    pub fn keys(&self) -> Vec<&String> {
        self.cache.iter().map(|(k, _)| k).collect()
    }

    /// Remove expired entries
    pub fn remove_expired(&mut self) -> usize {
        let expired_keys: Vec<String> = self
            .cache
            .iter()
            .filter(|(_, v)| v.is_expired(self.config.ttl))
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_keys.len();
        for key in expired_keys {
            self.cache.pop(&key);
            self.stats.ttl_evictions = self.stats.ttl_evictions.saturating_add(1);
        }

        count
    }

    /// Get estimated memory usage (bytes)
    pub fn estimated_memory_bytes(&self) -> usize {
        self.cache
            .iter()
            .map(|(_, v)| v.content.len() + size_of::<CachedResult>())
            .sum()
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let mut cache = ToolResultCache::new(CacheConfig {
            max_entries: 100,
            ttl: Duration::from_mins(1),
            min_size_to_cache: 10,
        });

        let params = serde_json::json!({"path": "src/main.rs"});

        // Miss on first lookup
        assert!(cache.get("read_file", &params).is_none());
        assert_eq!(cache.stats().misses, 1);

        // Insert and verify hit
        cache.insert("read_file", &params, "fn main() { println!(\"hello\"); }");
        assert!(cache.get("read_file", &params).is_some());
        assert_eq!(cache.stats().hits, 1);
    }

    #[test]
    fn test_cache_key_computation() {
        let params1 = serde_json::json!({"path": "src/main.rs"});
        let params2 = serde_json::json!({"path": "src/lib.rs"});

        let key1 = ToolResultCache::compute_cache_key("read_file", &params1);
        let key2 = ToolResultCache::compute_cache_key("read_file", &params2);
        let key3 = ToolResultCache::compute_cache_key("read_file", &params1);

        // Same inputs produce same keys
        assert_eq!(key1, key3);

        // Different inputs produce different keys
        assert_ne!(key1, key2);

        // Different tools produce different keys
        let key4 = ToolResultCache::compute_cache_key("grep", &params1);
        assert_ne!(key1, key4);
    }

    #[test]
    fn test_min_size_to_cache() {
        let mut cache = ToolResultCache::new(CacheConfig {
            max_entries: 100,
            ttl: Duration::from_mins(1),
            min_size_to_cache: 100,
        });

        let params = serde_json::json!({});

        // Small content should not be cached
        assert!(!cache.insert("read_file", &params, "small"));
        assert_eq!(cache.stats().inserts, 0);

        // Large content should be cached
        assert!(cache.insert("read_file", &params, &"x".repeat(200)));
        assert_eq!(cache.stats().inserts, 1);
    }

    #[test]
    fn test_cache_ttl() {
        let mut cache = ToolResultCache::new(CacheConfig {
            max_entries: 100,
            ttl: Duration::from_millis(50), // Very short TTL for testing
            min_size_to_cache: 10,
        });

        let params = serde_json::json!({});
        cache.insert("read_file", &params, "content-data-for-testing"); // > 10 bytes

        // Immediately accessible
        assert!(cache.get("read_file", &params).is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(100));

        // Should be expired now
        assert!(cache.get("read_file", &params).is_none());
        assert!(cache.stats().ttl_evictions > 0);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = ToolResultCache::new(CacheConfig {
            max_entries: 3,
            ttl: Duration::from_hours(1), // Long TTL
            min_size_to_cache: 10,
        });

        // Insert 3 entries
        cache.insert("tool1", &serde_json::json!({}), &"a".repeat(100));
        cache.insert("tool2", &serde_json::json!({}), &"b".repeat(100));
        cache.insert("tool3", &serde_json::json!({}), &"c".repeat(100));

        assert_eq!(cache.len(), 3);

        // Insert 4th entry - should evict oldest (tool1)
        cache.insert("tool4", &serde_json::json!({}), &"d".repeat(100));

        assert_eq!(cache.len(), 3);
        assert!(cache.get("tool1", &serde_json::json!({})).is_none()); // Evicted
        assert!(cache.get("tool2", &serde_json::json!({})).is_some());
        assert!(cache.get("tool3", &serde_json::json!({})).is_some());
        assert!(cache.get("tool4", &serde_json::json!({})).is_some());
        assert!(cache.stats().lru_evictions > 0);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = ToolResultCache::new(CacheConfig::default());
        let params = serde_json::json!({"x": 1});

        cache.insert("test", &params, &"data".repeat(100));

        // Multiple hits
        cache.get("test", &params);
        cache.get("test", &params);
        cache.get("test", &params);

        assert_eq!(cache.stats().hits, 3);
        assert_eq!(cache.stats().misses, 0);
        assert_eq!(cache.stats().inserts, 1);

        // Verify hit rate
        assert!((cache.stats().hit_rate() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_remove_expired() {
        let mut cache = ToolResultCache::new(CacheConfig {
            max_entries: 100,
            ttl: Duration::from_millis(50),
            min_size_to_cache: 10,
        });

        // Insert several entries
        for i in 0..5 {
            cache.insert(
                &format!("tool{}", i),
                &serde_json::json!({}),
                &"x".repeat(100),
            );
        }

        assert_eq!(cache.len(), 5);

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(100));

        // Remove expired
        let removed = cache.remove_expired();
        assert_eq!(removed, 5);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_estimated_memory() {
        let mut cache = ToolResultCache::new(CacheConfig::default());
        let content = "x".repeat(1000);

        cache.insert("test", &serde_json::json!({}), &content);

        // Should be at least the content size
        assert!(cache.estimated_memory_bytes() >= 1000);
    }

    #[test]
    fn test_clear() {
        let mut cache = ToolResultCache::new(CacheConfig::default());

        for i in 0..10 {
            cache.insert(
                &format!("tool{}", i),
                &serde_json::json!({}),
                &"x".repeat(100),
            );
        }

        assert_eq!(cache.len(), 10);
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_token_count_estimation() {
        let mut cache = ToolResultCache::new(CacheConfig {
            min_size_to_cache: 10,
            ..Default::default()
        });

        let content = "hello world ".repeat(100); // 1200 chars
        cache.insert("test", &serde_json::json!({}), &content);

        let cached = cache.get("test", &serde_json::json!({})).unwrap();
        // Token count is roughly len/4
        assert_eq!(cached.token_count, content.len() / 4);
    }
}
