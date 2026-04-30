// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! # Multi-Level Caching System
//!
//! High-performance, multi-level caching with L1 (memory) and L2 (disk) tiers.
//!
//! ## Architecture
//!
//! The cache system uses a two-level hierarchy:
//!
//! - **L1 (Memory)**: Fast LRU cache with nanosecond access times
//! - **L2 (Disk)**: Persistent cache with millisecond access times
//!
//! ## Features
//!
//! - **LRU Eviction**: Automatically evicts least recently used items
//! - **TTL Support**: Time-based expiration for cache entries
//! - **Cache Warming**: Pre-populate cache with frequently accessed data
//! - **Statistics**: Track hits, misses, evictions, and effectiveness
//! - **Thread Safety**: Safe concurrent access across threads
//! - **Async Operations**: Non-blocking cache operations
//!
//! ## Usage
//!
//! ```rust
//! use rustycode_core::cache::{MultiLevelCache, CacheConfig};
//! use std::time::Duration;
//! use tokio::runtime::Builder;
//!
//! # fn main() -> anyhow::Result<()> {
//! # let rt = Builder::new_current_thread().enable_all().build()?;
//! # rt.block_on(async {
//! // Configure cache with 1000 item L1 and 5 minute TTL
//! let config = CacheConfig::builder()
//!     .l1_capacity(1000)
//!     .ttl(Duration::from_secs(300))
//!     .build();
//!
//! let cache = MultiLevelCache::new(config)?;
//!
//! // Set value (flows to both L1 and L2)
//! cache.set("user:123", &serde_json::json!({
//!     "name": "Alice",
//!     "email": "alice@example.com"
//! })).await?;
//!
//! // Get from cache (checks L1 first, then L2)
//! if let Some(user) = cache.get("user:123").await? {
//!     println!("User: {}", user);
//! }
//!
//! // Print statistics
//! println!("Cache stats: {:?}", cache.stats());
//! # Ok(())
//! # })
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cache key type
pub type CacheKey = String;

/// Cache value (JSON-serializable)
pub type CacheValue = serde_json::Value;

/// Default cache directory name
const CACHE_DIR_NAME: &str = "cache";

/// Default L1 cache capacity
const DEFAULT_L1_CAPACITY: usize = 1000;

/// Default TTL (5 minutes)
const DEFAULT_TTL_SECS: u64 = 300;

// ============================================================================
// Cache Trait
// ============================================================================

/// Core cache trait defining the cache operations
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    /// Get a value from the cache
    ///
    /// Returns `None` if the key doesn't exist or has expired.
    async fn get(&self, key: &CacheKey) -> Result<Option<CacheValue>>;

    /// Set a value in the cache
    ///
    /// The value will be stored in both L1 and L2 caches.
    async fn set(&self, key: &CacheKey, value: &CacheValue) -> Result<()>;

    /// Invalidate a specific cache entry
    async fn invalidate(&self, key: &CacheKey) -> Result<()>;

    /// Clear all cache entries
    async fn clear(&self) -> Result<()>;

    /// Get cache statistics
    fn stats(&self) -> CacheStats;
}

// ============================================================================
// Cache Configuration
// ============================================================================

/// Configuration for the multi-level cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// L1 cache capacity (number of entries)
    pub l1_capacity: usize,

    /// Time-to-live for cache entries
    pub ttl: std::time::Duration,

    /// Enable/disable L2 disk cache
    pub enable_l2: bool,

    /// Custom cache directory (optional)
    pub cache_dir: Option<PathBuf>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l1_capacity: DEFAULT_L1_CAPACITY,
            ttl: std::time::Duration::from_secs(DEFAULT_TTL_SECS),
            enable_l2: true,
            cache_dir: None,
        }
    }
}

impl CacheConfig {
    /// Create a new cache config builder
    pub fn builder() -> CacheConfigBuilder {
        CacheConfigBuilder::default()
    }

    /// Get the cache directory path
    pub fn get_cache_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = &self.cache_dir {
            Ok(dir.clone())
        } else {
            let mut path = dirs::cache_dir()
                .context("Failed to determine cache directory")?;
            path.push("rustycode");
            path.push(CACHE_DIR_NAME);
            Ok(path)
        }
    }
}

/// Builder for creating cache configurations
#[derive(Default)]
pub struct CacheConfigBuilder {
    config: CacheConfig,
}

impl CacheConfigBuilder {
    /// Set L1 cache capacity
    pub fn l1_capacity(mut self, capacity: usize) -> Self {
        self.config.l1_capacity = capacity;
        self
    }

    /// Set TTL for cache entries
    pub fn ttl(mut self, ttl: std::time::Duration) -> Self {
        self.config.ttl = ttl;
        self
    }

    /// Enable or disable L2 cache
    pub fn enable_l2(mut self, enable: bool) -> Self {
        self.config.enable_l2 = enable;
        self
    }

    /// Set custom cache directory
    pub fn cache_dir(mut self, dir: PathBuf) -> Self {
        self.config.cache_dir = Some(dir);
        self
    }

    /// Build the configuration
    pub fn build(self) -> CacheConfig {
        self.config
    }
}

// ============================================================================
// Cache Statistics
// ============================================================================

/// Statistics tracking cache performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,

    /// Number of entries evicted from L1
    pub evictions: u64,

    /// Current size of L1 cache
    pub l1_size: usize,

    /// Current size of L2 cache
    pub l2_size: usize,

    /// Total number of set operations
    pub sets: u64,

    /// Total number of get operations
    pub gets: u64,

    /// Total number of invalidations
    pub invalidations: u64,
}

impl CacheStats {
    /// Calculate hit rate (0.0 to 1.0)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Calculate miss rate (0.0 to 1.0)
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

impl Default for CacheStats {
    fn default() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
            l1_size: 0,
            l2_size: 0,
            sets: 0,
            gets: 0,
            invalidations: 0,
        }
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

/// A single cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// The cached value
    value: CacheValue,

    /// When the entry was created
    created_at: DateTime<Utc>,

    /// When the entry expires
    expires_at: DateTime<Utc>,

    /// Access count (for LRU tracking)
    access_count: u64,

    /// Last access time
    last_access: DateTime<Utc>,
}

impl CacheEntry {
    /// Create a new cache entry
    fn new(value: CacheValue, ttl: std::time::Duration) -> Self {
        let now = Utc::now();
        Self {
            value,
            created_at: now,
            expires_at: now + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::seconds(300)),
            access_count: 0,
            last_access: now,
        }
    }

    /// Check if the entry has expired
    fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Record an access
    fn record_access(&mut self) {
        self.access_count = self.access_count.saturating_add(1);
        self.last_access = Utc::now();
    }
}

// ============================================================================
// Memory Cache (L1)
// ============================================================================

/// L1 memory cache with LRU eviction
struct MemoryCache {
    /// Cache entries
    entries: HashMap<CacheKey, CacheEntry>,

    /// LRU access order (most recently used at front)
    lru_order: Vec<CacheKey>,

    /// Maximum capacity
    capacity: usize,

    /// Statistics
    stats: CacheStats,
}

impl MemoryCache {
    /// Create a new memory cache
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            lru_order: Vec::with_capacity(capacity),
            capacity,
            stats: CacheStats::default(),
        }
    }

    /// Get a value from the cache
    fn get(&mut self, key: &CacheKey) -> Option<CacheValue> {
        self.stats.gets = self.stats.gets.saturating_add(1);

        if let Some(entry) = self.entries.get_mut(key) {
            if entry.is_expired() {
                // Clone the key before borrowing self again
                let key_clone = key.clone();
                self.remove(&key_clone);
                self.stats.misses = self.stats.misses.saturating_add(1);
                self.stats.l1_size = self.entries.len();
                return None;
            }

            // Clone value before other mutations
            let value = entry.value.clone();

            // Update LRU order
            self.update_lru(key);
            entry.record_access();
            self.stats.hits = self.stats.hits.saturating_add(1);
            self.stats.l1_size = self.entries.len();
            Some(value)
        } else {
            self.stats.misses = self.stats.misses.saturating_add(1);
            self.stats.l1_size = self.entries.len();
            None
        }
    }

    /// Set a value in the cache
    fn set(&mut self, key: CacheKey, value: CacheValue, ttl: std::time::Duration) {
        self.stats.sets = self.stats.sets.saturating_add(1);

        let entry = CacheEntry::new(value, ttl);

        // If at capacity, evict least recently used
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            if let Some(lru_key) = self.lru_order.pop() {
                self.entries.remove(&lru_key);
                self.stats.evictions = self.stats.evictions.saturating_add(1);
            }
        }

        self.entries.insert(key.clone(), entry);
        self.update_lru(&key);
        self.stats.l1_size = self.entries.len();
    }

    /// Invalidate a specific entry
    fn remove(&mut self, key: &CacheKey) {
        self.entries.remove(key);
        self.lru_order.retain(|k| k != key);
        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        self.stats.l1_size = self.entries.len();
    }

    /// Clear all entries
    fn clear(&mut self) {
        self.entries.clear();
        self.lru_order.clear();
        self.stats.l1_size = 0;
    }

    /// Update LRU order (move key to front)
    fn update_lru(&mut self, key: &CacheKey) {
        self.lru_order.retain(|k| k != key);
        self.lru_order.insert(0, key.clone());
    }

    /// Get all keys (for cache warming)
    fn keys(&self) -> Vec<CacheKey> {
        self.entries.keys().cloned().collect()
    }

    /// Get statistics
    fn stats(&self) -> &CacheStats {
        &self.stats
    }
}

// ============================================================================
// Disk Cache (L2)
// ============================================================================

/// L2 disk cache for persistent storage
struct DiskCache {
    /// Cache directory
    cache_dir: PathBuf,

    /// Statistics
    stats: CacheStats,
}

impl DiskCache {
    /// Create a new disk cache
    fn new(cache_dir: PathBuf) -> Result<Self> {
        // Create cache directory if it doesn't exist
        std::fs::create_dir_all(&cache_dir)
            .context("Failed to create cache directory")?;

        Ok(Self {
            cache_dir,
            stats: CacheStats::default(),
        })
    }

    /// Get the file path for a key
    fn get_file_path(&self, key: &CacheKey) -> PathBuf {
        // Use SHA256 hash of key as filename to avoid filesystem issues
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash = hasher.finalize();
        let filename = format!("{:x}.json", hash);
        self.cache_dir.join(filename)
    }

    /// Get a value from disk
    fn get(&mut self, key: &CacheKey) -> Result<Option<CacheValue>> {
        self.stats.gets = self.stats.gets.saturating_add(1);

        let path = self.get_file_path(key);

        if !path.exists() {
            self.stats.misses = self.stats.misses.saturating_add(1);
            return Ok(None);
        }

        // Read and deserialize
        let data = std::fs::read(&path)
            .context("Failed to read cache file")?;

        let entry: CacheEntry = serde_json::from_slice(&data)
            .context("Failed to deserialize cache entry")?;

        if entry.is_expired() {
            // Remove expired entry
            self.remove(key)?;
            self.stats.misses = self.stats.misses.saturating_add(1);
            return Ok(None);
        }

        self.stats.hits = self.stats.hits.saturating_add(1);
        Ok(Some(entry.value))
    }

    /// Set a value on disk
    fn set(&mut self, key: &CacheKey, value: &CacheValue, ttl: std::time::Duration) -> Result<()> {
        self.stats.sets = self.stats.sets.saturating_add(1);

        let entry = CacheEntry::new(value.clone(), ttl);
        let data = serde_json::to_vec(&entry)
            .context("Failed to serialize cache entry")?;

        let path = self.get_file_path(key);
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &data)
            .context("Failed to write cache file")?;
        std::fs::rename(&tmp_path, &path)
            .context("Failed to rename cache file")?;

        self.stats.l2_size = self.count_entries()?;
        Ok(())
    }

    /// Invalidate a specific entry
    fn remove(&mut self, key: &CacheKey) -> Result<()> {
        let path = self.get_file_path(key);

        if path.exists() {
            std::fs::remove_file(&path)
                .context("Failed to remove cache file")?;
        }

        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
        self.stats.l2_size = self.count_entries()?;
        Ok(())
    }

    /// Clear all entries
    fn clear(&mut self) -> Result<()> {
        for entry in std::fs::read_dir(&self.cache_dir)
            .context("Failed to read cache directory")?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            if path.is_file() {
                std::fs::remove_file(&path)
                    .context("Failed to remove cache file")?;
            }
        }

        self.stats.l2_size = 0;
        Ok(())
    }

    /// Count the number of cache entries
    fn count_entries(&self) -> Result<usize> {
        Ok(std::fs::read_dir(&self.cache_dir)
            .context("Failed to read cache directory")?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .count())
    }

    /// Get all keys (requires scanning all files)
    fn keys(&self) -> Result<Vec<CacheKey>> {
        // This is expensive, so we'd need a key index in production
        // For now, return empty vec
        Ok(Vec::new())
    }

    /// Get statistics
    fn stats(&self) -> &CacheStats {
        &self.stats
    }
}

// ============================================================================
// Multi-Level Cache
// ============================================================================

/// Multi-level cache combining L1 (memory) and L2 (disk)
pub struct MultiLevelCache {
    /// L1 memory cache
    l1: Arc<RwLock<MemoryCache>>,

    /// L2 disk cache (optional)
    l2: Arc<RwLock<Option<DiskCache>>>,

    /// Configuration
    config: CacheConfig,

    /// Combined statistics
    stats: Arc<RwLock<CacheStats>>,
}

impl MultiLevelCache {
    /// Create a new multi-level cache
    pub fn new(config: CacheConfig) -> Result<Self> {
        let l2 = if config.enable_l2 {
            let cache_dir = config.get_cache_dir()?;
            Some(DiskCache::new(cache_dir)?)
        } else {
            None
        };

        Ok(Self {
            l1: Arc::new(RwLock::new(MemoryCache::new(config.l1_capacity))),
            l2: Arc::new(RwLock::new(l2)),
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        })
    }

    /// Create with default configuration
    pub fn with_defaults() -> Result<Self> {
        Self::new(CacheConfig::default())
    }

    /// Update combined statistics
    async fn update_stats(&self) {
        let l1 = self.l1.read().await;
        let l1_stats = l1.stats().clone();
        drop(l1);

        let l2 = self.l2.read().await;
        let l2_stats = if let Some(ref l2) = *l2 {
            l2.stats().clone()
        } else {
            CacheStats::default()
        };
        drop(l2);

        let mut stats = self.stats.write().await;
        stats.hits = l1_stats.hits + l2_stats.hits;
        stats.misses = l1_stats.misses + l2_stats.misses;
        stats.evictions = l1_stats.evictions;
        stats.l1_size = l1_stats.l1_size;
        stats.l2_size = l2_stats.l2_size;
        stats.sets = l1_stats.sets + l2_stats.sets;
        stats.gets = l1_stats.gets + l2_stats.gets;
        stats.invalidations = l1_stats.invalidations + l2_stats.invalidations;
    }
}

#[async_trait::async_trait]
impl Cache for MultiLevelCache {
    /// Get a value from the cache (L1 first, then L2)
    async fn get(&self, key: &CacheKey) -> Result<Option<CacheValue>> {
        // Try L1 first
        {
            let mut l1 = self.l1.write().await;
            if let Some(value) = l1.get(key) {
                self.update_stats().await;
                return Ok(Some(value));
            }
        }

        // Try L2
        {
            let mut l2_guard = self.l2.write().await;
            if let Some(ref mut l2) = *l2_guard {
                if let Some(value) = l2.get(key)? {
                    // Promote to L1
                    let mut l1 = self.l1.write().await;
                    l1.set(key.clone(), value.clone(), self.config.ttl);
                    self.update_stats().await;
                    return Ok(Some(value));
                }
            }
        }

        self.update_stats().await;
        Ok(None)
    }

    /// Set a value in both L1 and L2
    async fn set(&self, key: &CacheKey, value: &CacheValue) -> Result<()> {
        // Set in L1
        {
            let mut l1 = self.l1.write().await;
            l1.set(key.clone(), value.clone(), self.config.ttl);
        }

        // Set in L2
        {
            let mut l2_guard = self.l2.write().await;
            if let Some(ref mut l2) = *l2_guard {
                l2.set(key, value, self.config.ttl)?;
            }
        }

        self.update_stats().await;
        Ok(())
    }

    /// Invalidate a cache entry from both levels
    async fn invalidate(&self, key: &CacheKey) -> Result<()> {
        // Remove from L1
        {
            let mut l1 = self.l1.write().await;
            l1.remove(key);
        }

        // Remove from L2
        {
            let mut l2_guard = self.l2.write().await;
            if let Some(ref mut l2) = *l2_guard {
                l2.remove(key)?;
            }
        }

        self.update_stats().await;
        Ok(())
    }

    /// Clear all cache entries from both levels
    async fn clear(&self) -> Result<()> {
        // Clear L1
        {
            let mut l1 = self.l1.write().await;
            l1.clear();
        }

        // Clear L2
        {
            let mut l2_guard = self.l2.write().await;
            if let Some(ref mut l2) = *l2_guard {
                l2.clear()?;
            }
        }

        self.update_stats().await;
        Ok(())
    }

    /// Get cache statistics
    fn stats(&self) -> CacheStats {
        // Try to read the current stats; fall back to empty if lock is contended
        self.stats
            .try_read()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}

impl MultiLevelCache {
    /// Get current statistics (async version)
    pub async fn get_stats(&self) -> CacheStats {
        self.update_stats().await;
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Warm the cache with frequently accessed data
    ///
    /// This strategy pre-loads the cache with data that's likely to be accessed.
    pub async fn warm_cache<F>(&self, key_generator: F) -> Result<()>
    where
        F: Fn() -> Vec<(CacheKey, CacheValue)>,
    {
        let items = key_generator();

        for (key, value) in items {
            self.set(&key, &value).await?;
        }

        tracing::info!("Warmed cache with {} entries", items.len());
        Ok(())
    }

    /// Invalidate cache entries matching a pattern
    ///
    /// Useful for invalidating groups of related entries (e.g., all user:* keys)
    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<()> {
        // Get all L1 keys
        let l1_keys = {
            let l1 = self.l1.read().await;
            l1.keys()
        };

        // Invalidate matching keys
        for key in l1_keys {
            if key.contains(pattern) {
                self.invalidate(&key).await?;
            }
        }

        // Note: L2 pattern matching would require an index
        // For now, we only invalidate from L1
        tracing::info!("Invalidated cache entries matching pattern: {}", pattern);
        Ok(())
    }

    /// Prefetch data that's likely to be needed soon
    ///
    /// This is useful for scenarios where you can predict access patterns.
    pub async fn prefetch<F>(&self, key_fetcher: F) -> Result<()>
    where
        F: Fn() -> Vec<CacheKey>,
    {
        let keys = key_fetcher();

        for key in keys {
            // If key is in L2 but not L1, promote it
            let l2_guard = self.l2.read().await;
            if let Some(ref l2) = *l2_guard {
                if let Ok(Some(value)) = l2.get(&key) {
                    drop(l2_guard);
                    let mut l1 = self.l1.write().await;
                    l1.set(key, value, self.config.ttl);
                }
            }
        }

        tracing::info!("Prefetched {} cache entries", keys.len());
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_memory_cache_basic() {
        let mut cache = MemoryCache::new(10);

        // Set and get
        cache.set(
            "key1".to_string(),
            serde_json::json!("value1"),
            Duration::from_secs(60),
        );

        assert_eq!(
            cache.get(&"key1".to_string()),
            Some(serde_json::json!("value1"))
        );

        // Non-existent key
        assert_eq!(cache.get(&"key2".to_string()), None);

        // Check stats
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().sets, 1);
    }

    #[tokio::test]
    async fn test_memory_cache_lru_eviction() {
        let mut cache = MemoryCache::new(2);

        // Fill cache
        cache.set(
            "key1".to_string(),
            serde_json::json!("value1"),
            Duration::from_secs(60),
        );
        cache.set(
            "key2".to_string(),
            serde_json::json!("value2"),
            Duration::from_secs(60),
        );

        // Access key1 to make it more recent
        cache.get(&"key1".to_string());

        // Add third item, should evict key2
        cache.set(
            "key3".to_string(),
            serde_json::json!("value3"),
            Duration::from_secs(60),
        );

        assert_eq!(
            cache.get(&"key1".to_string()),
            Some(serde_json::json!("value1"))
        );
        assert_eq!(cache.get(&"key2".to_string()), None); // Evicted
        assert_eq!(
            cache.get(&"key3".to_string()),
            Some(serde_json::json!("value3"))
        );

        assert_eq!(cache.stats().evictions, 1);
    }

    #[tokio::test]
    async fn test_memory_cache_invalidation() {
        let mut cache = MemoryCache::new(10);

        cache.set(
            "key1".to_string(),
            serde_json::json!("value1"),
            Duration::from_secs(60),
        );

        assert!(cache.get(&"key1".to_string()).is_some());

        cache.remove(&"key1".to_string());

        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.stats().invalidations, 1);
    }

    #[tokio::test]
    async fn test_cache_entry_expiration() {
        let entry = CacheEntry::new(
            serde_json::json!("value"),
            Duration::from_millis(10),
        );

        assert!(!entry.is_expired());

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(entry.is_expired());
    }

    #[tokio::test]
    async fn test_multi_level_cache_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig::builder()
            .l1_capacity(10)
            .cache_dir(temp_dir.path().to_path_buf())
            .build();

        let cache = MultiLevelCache::new(config).unwrap();

        // Set and get
        cache
            .set(&"key1".to_string(), &serde_json::json!("value1"))
            .await
            .unwrap();

        let value = cache.get(&"key1".to_string()).await.unwrap();
        assert_eq!(value, Some(serde_json::json!("value1")));

        // Non-existent key
        let value = cache.get(&"key2".to_string()).await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_multi_level_cache_invalidation() {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig::builder()
            .l1_capacity(10)
            .cache_dir(temp_dir.path().to_path_buf())
            .build();

        let cache = MultiLevelCache::new(config).unwrap();

        cache
            .set(&"key1".to_string(), &serde_json::json!("value1"))
            .await
            .unwrap();

        assert!(cache.get(&"key1".to_string()).await.unwrap().is_some());

        cache
            .invalidate(&"key1".to_string())
            .await
            .unwrap();

        assert_eq!(cache.get(&"key1".to_string()).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_multi_level_cache_clear() {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig::builder()
            .l1_capacity(10)
            .cache_dir(temp_dir.path().to_path_buf())
            .build();

        let cache = MultiLevelCache::new(config).unwrap();

        // Add multiple entries
        for i in 1..=5 {
            cache
                .set(&format!("key{}", i), &serde_json::json!(i))
                .await
                .unwrap();
        }

        // Clear all
        cache.clear().await.unwrap();

        // Verify all are gone
        for i in 1..=5 {
            assert_eq!(cache.get(&format!("key{}", i)).await.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn test_cache_warming() {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig::builder()
            .l1_capacity(100)
            .cache_dir(temp_dir.path().to_path_buf())
            .build();

        let cache = MultiLevelCache::new(config).unwrap();

        // Warm cache
        cache
            .warm_cache(|| {
                vec![
                    ("user:1".to_string(), serde_json::json!({"name": "Alice"})),
                    ("user:2".to_string(), serde_json::json!({"name": "Bob"})),
                ]
            })
            .await
            .unwrap();

        // Verify warmed entries
        assert_eq!(
            cache.get(&"user:1".to_string()).await.unwrap(),
            Some(serde_json::json!({"name": "Alice"}))
        );
        assert_eq!(
            cache.get(&"user:2".to_string()).await.unwrap(),
            Some(serde_json::json!({"name": "Bob"}))
        );
    }

    #[tokio::test]
    async fn test_cache_stats_hit_rate() {
        let stats = CacheStats {
            hits: 80,
            misses: 20,
            ..Default::default()
        };

        assert_eq!(stats.hit_rate(), 0.8);
        assert_eq!(stats.miss_rate(), 0.2);
    }

    #[tokio::test]
    async fn test_cache_config_builder() {
        let config = CacheConfig::builder()
            .l1_capacity(500)
            .ttl(Duration::from_secs(600))
            .enable_l2(false)
            .build();

        assert_eq!(config.l1_capacity, 500);
        assert_eq!(config.ttl, Duration::from_secs(600));
        assert_eq!(config.enable_l2, false);
    }

    #[tokio::test]
    async fn test_stats_returns_real_data_after_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = CacheConfig::builder()
            .l1_capacity(10)
            .cache_dir(temp_dir.path().to_path_buf())
            .build();

        let cache = MultiLevelCache::new(config).unwrap();

        // Perform some operations
        cache
            .set(&"key1".to_string(), &serde_json::json!("value1"))
            .await
            .unwrap();
        let _ = cache.get(&"key1".to_string()).await.unwrap();
        let _ = cache.get(&"missing".to_string()).await.unwrap();

        // stats() should return real data, not defaults
        let stats = cache.stats();
        assert!(
            stats.sets > 0 || stats.gets > 0,
            "stats() should return real data, not defaults"
        );
    }
}
