//! Stateful prompt cache manager with SHA-256 change detection.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use super::cache_metrics::CacheMetrics;

// ─── Constants ───────────────────────────────────────────────────────────────

/// Cache key for the system prompt entry.
const SYSTEM_PROMPT_KEY: &str = "__system_prompt__";
/// Cache key for the tool definitions entry.
const TOOL_DEFS_KEY: &str = "__tool_definitions__";
/// Rough approximation: 1 token equals 4 characters.
const CHARS_PER_TOKEN: usize = 4;

// ─── CachedItem ──────────────────────────────────────────────────────────────

/// A single cached content entry with its SHA-256 hash and metadata.
#[derive(Debug, Clone)]
pub struct CachedItem {
    /// Original content string.
    content: String,
    /// SHA-256 hex digest of `content`.
    hash: String,
    /// Estimated token count for `content`.
    token_estimate: usize,
    /// When this entry was cached.
    #[allow(dead_code)]
    cached_at: DateTime<Utc>,
}

// ─── PromptCacheManager ──────────────────────────────────────────────────────

/// Tracks what system prompts and tool definitions are cached, using SHA-256
/// hashing for change detection.
///
/// Thread-safety is the caller's responsibility (wrap in `Mutex` if needed).
#[derive(Debug, Clone)]
pub struct PromptCacheManager {
    cached_items: HashMap<String, CachedItem>,
    cache_metrics: CacheMetrics,
}

impl PromptCacheManager {
    pub fn new() -> Self {
        Self {
            cached_items: HashMap::new(),
            cache_metrics: CacheMetrics::new(),
        }
    }

    // ── Cache operations ────────────────────────────────────────────────────

    /// Cache the system prompt, computing its SHA-256 hash and token estimate.
    pub fn cache_system_prompt(&mut self, prompt: &str) {
        let hash = compute_hash(prompt);
        let token_estimate = self.estimate_tokens(prompt);
        self.cached_items.insert(
            SYSTEM_PROMPT_KEY.to_string(),
            CachedItem {
                content: prompt.to_string(),
                hash,
                token_estimate,
                cached_at: Utc::now(),
            },
        );
    }

    /// Cache tool definitions by joining them and computing a combined hash.
    pub fn cache_tool_definitions(&mut self, tools: &[&str]) {
        let joined = tools.join("\n");
        let hash = compute_hash(&joined);
        let token_estimate = self.estimate_tokens(&joined);
        self.cached_items.insert(
            TOOL_DEFS_KEY.to_string(),
            CachedItem {
                content: joined,
                hash,
                token_estimate,
                cached_at: Utc::now(),
            },
        );
    }

    // ── Query methods ───────────────────────────────────────────────────────

    /// Whether a system prompt is currently cached.
    pub fn is_system_prompt_cached(&self) -> bool {
        self.cached_items.contains_key(SYSTEM_PROMPT_KEY)
    }

    /// Whether tool definitions are currently cached.
    pub fn is_tool_defs_cached(&self) -> bool {
        self.cached_items.contains_key(TOOL_DEFS_KEY)
    }

    /// Number of individual tool definitions in the cached entry.
    ///
    /// Returns 0 if tool definitions have not been cached.
    pub fn cached_tool_count(&self) -> usize {
        self.cached_items.get(TOOL_DEFS_KEY).map_or(0, |item| {
            item.content.lines().filter(|line| !line.is_empty()).count()
        })
    }

    /// Return the SHA-256 hash of the cached system prompt, if any.
    pub fn system_prompt_hash(&self) -> Option<String> {
        self.cached_items
            .get(SYSTEM_PROMPT_KEY)
            .map(|item| item.hash.clone())
    }

    // ── Token estimation ────────────────────────────────────────────────────

    /// Rough token estimate: 1 token per 4 characters.
    pub const fn estimate_tokens(&self, content: &str) -> usize {
        content.len() / CHARS_PER_TOKEN
    }

    /// Alias for [`Self::estimate_tokens`], used for cache-savings calculations.
    pub const fn estimate_cache_tokens(&self, content: &str) -> usize {
        self.estimate_tokens(content)
    }

    /// Sum of estimated tokens across all cached entries.
    pub fn total_cached_tokens(&self) -> usize {
        self.cached_items
            .values()
            .map(|item| item.token_estimate)
            .sum()
    }

    // ── Metrics ─────────────────────────────────────────────────────────────

    /// Return a reference to the accumulated cache metrics.
    pub const fn metrics(&self) -> &CacheMetrics {
        &self.cache_metrics
    }

    /// Record a cache hit with the number of tokens saved.
    pub const fn record_cache_hit(&mut self, tokens_saved: usize) {
        self.cache_metrics.hits += 1;
        self.cache_metrics.total_tokens_saved = self
            .cache_metrics
            .total_tokens_saved
            .saturating_add(tokens_saved);
    }

    /// Record a cache miss.
    pub const fn record_cache_miss(&mut self) {
        self.cache_metrics.misses += 1;
    }

    // ── Invalidation ────────────────────────────────────────────────────────

    /// Clear all cached entries.
    pub fn invalidate_all(&mut self) {
        self.cached_items.clear();
    }

    /// Remove only the system prompt entry.
    pub fn invalidate_system_prompt(&mut self) {
        self.cached_items.remove(SYSTEM_PROMPT_KEY);
    }

    /// Remove only the tool definitions entry.
    pub fn invalidate_tool_definitions(&mut self) {
        self.cached_items.remove(TOOL_DEFS_KEY);
    }

    // ── Content change detection ────────────────────────────────────────────

    /// Check whether `content` matches the cached hash for `key`.
    ///
    /// Returns `false` if no entry exists for `key` or if the hashes differ.
    pub fn is_content_cached(&self, key: &str, content: &str) -> bool {
        self.cached_items
            .get(key)
            .is_some_and(|item| item.hash == compute_hash(content))
    }
}

impl Default for PromptCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Compute a SHA-256 hex digest of `input`.
fn compute_hash(input: &str) -> String {
    rustycode_protocol::crypto::sha256_hex(input.as_bytes())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_manager_tracks_cached_content() {
        let mut mgr = PromptCacheManager::new();

        // Nothing cached initially
        assert!(!mgr.is_system_prompt_cached());
        assert!(!mgr.is_tool_defs_cached());

        mgr.cache_system_prompt("You are a helpful assistant.");
        mgr.cache_tool_definitions(&["tool_a", "tool_b", "tool_c"]);

        assert!(mgr.is_system_prompt_cached());
        assert!(mgr.is_tool_defs_cached());
        assert_eq!(mgr.cached_tool_count(), 3);
        assert!(mgr.system_prompt_hash().is_some());
    }

    #[test]
    fn test_cache_tokens_calculated_correctly() {
        let mgr = PromptCacheManager::new();

        // "abcdefghij" = 10 chars => 10 / 4 = 2 tokens
        assert_eq!(mgr.estimate_tokens("abcdefghij"), 2);

        // Empty string => 0 tokens
        assert_eq!(mgr.estimate_tokens(""), 0);

        // Alias should produce the same result
        assert_eq!(
            mgr.estimate_tokens("abcdefghij"),
            mgr.estimate_cache_tokens("abcdefghij")
        );
    }

    #[test]
    fn test_cache_invalidates_on_content_change() {
        let mut mgr = PromptCacheManager::new();

        mgr.cache_system_prompt("version 1");
        let hash_v1 = mgr.system_prompt_hash().unwrap();

        mgr.cache_system_prompt("version 2");
        let hash_v2 = mgr.system_prompt_hash().unwrap();

        assert_ne!(hash_v1, hash_v2, "hash should change when content changes");
    }

    #[test]
    fn test_is_content_cached_detects_changes() {
        let mut mgr = PromptCacheManager::new();
        mgr.cache_system_prompt("original content");

        assert!(
            mgr.is_content_cached(SYSTEM_PROMPT_KEY, "original content"),
            "same content should match"
        );
        assert!(
            !mgr.is_content_cached(SYSTEM_PROMPT_KEY, "changed content"),
            "different content should not match"
        );
        assert!(
            !mgr.is_content_cached("nonexistent_key", "anything"),
            "missing key should return false"
        );
    }

    #[test]
    fn test_invalidate_all_clears_everything() {
        let mut mgr = PromptCacheManager::new();
        mgr.cache_system_prompt("system prompt");
        mgr.cache_tool_definitions(&["tool_a"]);
        assert!(mgr.is_system_prompt_cached());
        assert!(mgr.is_tool_defs_cached());

        mgr.invalidate_all();
        assert!(!mgr.is_system_prompt_cached());
        assert!(!mgr.is_tool_defs_cached());
        assert_eq!(mgr.total_cached_tokens(), 0);
    }

    #[test]
    fn test_metrics_tracking() {
        let mut mgr = PromptCacheManager::new();

        mgr.record_cache_hit(100);
        mgr.record_cache_hit(200);
        mgr.record_cache_miss();

        let metrics = mgr.metrics();
        assert_eq!(metrics.hits, 2);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.total_tokens_saved, 300);

        // Hit rate: 2 / 3
        let rate = metrics.hit_rate();
        assert!((rate - 0.666_7).abs() < 0.01, "expected ~0.667, got {rate}");
    }
}
