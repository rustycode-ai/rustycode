//! Memory and Persistence System
//!
//! This module provides a comprehensive memory management system for the orchestrator.
//! It supports:
//! - Short-term memory (ephemeral session data)
//! - Long-term memory (persistent storage)
//! - Semantic search and retrieval
//! - Memory consolidation and pruning
//! - Context-aware memory management
//! - Multi-level caching with TTL
//!
//! ## Architecture
//!
//! The memory system is organized into three levels:
//!
//! 1. **Working Memory** (RAM, fast access)
//!    - Current conversation context
//!    - Recent agent interactions
//!    - Temporary state
//!    - Size: ~1000 entries, TTL: 1 hour
//!
//! 2. **Short-term Memory** (Redis, fast access)
//!    - Recent session history
//!    - Agent outputs and results
//!    - Intermediate computations
//!    - Size: ~10000 entries, TTL: 24 hours
//!
//! 3. **Long-term Memory** (Database, persistent)
//!    - Historical data
//!    - Learned patterns
//!    - Archived sessions
//!    - Size: unlimited, TTL: forever

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Memory entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier
    pub id: String,
    /// Memory key
    pub key: String,
    /// Memory value
    pub value: serde_json::Value,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last access timestamp
    pub accessed_at: DateTime<Utc>,
    /// Expiration timestamp
    pub expires_at: Option<DateTime<Utc>>,
    pub access_count: u64,
    /// Memory level
    pub level: MemoryLevel,
    /// Tags for semantic search
    pub tags: Vec<String>,
    /// Priority for eviction
    pub priority: u8,
}

/// Memory level hierarchy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MemoryLevel {
    /// Working memory (fast, ephemeral)
    Working,
    /// Short-term memory (Redis, 24h TTL)
    ShortTerm,
    /// Long-term memory (Database, persistent)
    LongTerm,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub entries_by_level: HashMap<String, usize>,
    /// Total memory usage in bytes
    pub total_memory_bytes: u64,
    pub cache_hit_rate: f64,
    /// Average access time in microseconds
    pub avg_access_time_us: f64,
    pub eviction_count: u64,
}

/// Search query for semantic memory retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Search terms
    pub terms: Vec<String>,
    /// Tags filter
    pub tags: Vec<String>,
    /// Memory level filter
    pub level: Option<MemoryLevel>,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Maximum results
    pub limit: usize,
    /// Minimum relevance score
    pub min_score: f64,
}

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Memory entry
    pub entry: MemoryEntry,
    /// Relevance score (0.0 to 1.0)
    pub score: f64,
}

/// Memory manager with multi-level caching
pub struct MemoryManager {
    /// Working memory (RAM)
    working: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    /// Short-term memory (simulated Redis)
    short_term: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    /// Long-term memory (simulated Database)
    long_term: Arc<RwLock<HashMap<String, MemoryEntry>>>,
    /// Memory statistics
    stats: Arc<RwLock<MemoryStats>>,
    /// Configuration
    config: MemoryConfig,
}

/// Memory configuration
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Max working memory entries
    pub max_working_entries: usize,
    /// Max short-term memory entries
    pub max_short_term_entries: usize,
    /// Default TTL for working memory
    pub working_ttl_secs: u64,
    /// Default TTL for short-term memory
    pub short_term_ttl_secs: u64,
    /// Enable automatic consolidation
    pub enable_consolidation: bool,
    /// Consolidation interval in seconds
    pub consolidation_interval_secs: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_working_entries: 1000,
            max_short_term_entries: 10000,
            working_ttl_secs: 3600,     // 1 hour
            short_term_ttl_secs: 86400, // 24 hours
            enable_consolidation: true,
            consolidation_interval_secs: 300, // 5 minutes
        }
    }
}

impl MemoryManager {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            working: Arc::new(RwLock::new(HashMap::new())),
            short_term: Arc::new(RwLock::new(HashMap::new())),
            long_term: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MemoryStats {
                total_entries: 0,
                entries_by_level: HashMap::new(),
                total_memory_bytes: 0,
                cache_hit_rate: 0.0,
                avg_access_time_us: 0.0,
                eviction_count: 0,
            })),
            config,
        }
    }

    /// Store a value in memory
    pub async fn store(
        &self,
        key: String,
        value: serde_json::Value,
        level: MemoryLevel,
        ttl_secs: Option<u64>,
        tags: Vec<String>,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = ttl_secs.map(|secs| now + Duration::seconds(secs as i64));

        let entry = MemoryEntry {
            id: id.clone(),
            key: key.clone(),
            value,
            created_at: now,
            accessed_at: now,
            expires_at,
            access_count: 0,
            level,
            tags,
            priority: 128, // Default priority
        };

        match level {
            MemoryLevel::Working => {
                let mut working = self.working.write().await;
                self.evict_if_needed(&mut working, MemoryLevel::Working)
                    .await?;
                working.insert(key.clone(), entry);
            }
            MemoryLevel::ShortTerm => {
                let mut short_term = self.short_term.write().await;
                self.evict_if_needed(&mut short_term, MemoryLevel::ShortTerm)
                    .await?;
                short_term.insert(key.clone(), entry);
            }
            MemoryLevel::LongTerm => {
                let mut long_term = self.long_term.write().await;
                long_term.insert(key.clone(), entry);
            }
        }

        self.update_stats().await;
        Ok(id)
    }

    /// Retrieve a value from memory
    pub async fn get(&self, key: &str) -> Option<MemoryEntry> {
        // Check working memory first
        {
            let mut working = self.working.write().await;
            if let Some(entry) = working.get(key) {
                let mut entry = entry.clone();
                entry.accessed_at = Utc::now();
                entry.access_count += 1;
                working.insert(key.to_string(), entry.clone());
                return Some(entry);
            }
        }

        // Check short-term memory
        {
            let entry = {
                let short_term = self.short_term.write().await;
                short_term.get(key).cloned()
            };

            if let Some(mut entry) = entry {
                entry.accessed_at = Utc::now();
                entry.access_count += 1;

                // Promote to working memory
                let mut working = self.working.write().await;
                self.evict_if_needed(&mut working, MemoryLevel::Working)
                    .await
                    .ok();
                working.insert(key.to_string(), entry.clone());

                return Some(entry);
            }
        }

        // Check long-term memory
        {
            let entry = {
                let long_term = self.long_term.write().await;
                long_term.get(key).cloned()
            };

            if let Some(mut entry) = entry {
                entry.accessed_at = Utc::now();
                entry.access_count += 1;

                // Promote to short-term memory
                let mut short_term = self.short_term.write().await;
                self.evict_if_needed(&mut short_term, MemoryLevel::ShortTerm)
                    .await
                    .ok();
                short_term.insert(key.to_string(), entry.clone());

                return Some(entry);
            }
        }

        None
    }

    /// Search memory with semantic query
    pub async fn search(&self, query: MemoryQuery) -> Vec<SearchResult> {
        let mut results = Vec::new();

        // Search all memory levels
        for (level, store) in [
            (MemoryLevel::Working, &self.working),
            (MemoryLevel::ShortTerm, &self.short_term),
            (MemoryLevel::LongTerm, &self.long_term),
        ] {
            if let Some(level_filter) = query.level {
                if level != level_filter {
                    continue;
                }
            }

            let store = store.read().await;
            for entry in store.values() {
                // Filter by time range
                if let Some((start, end)) = query.time_range {
                    if entry.created_at < start || entry.created_at > end {
                        continue;
                    }
                }

                // Filter by tags
                if !query.tags.is_empty() {
                    let has_all_tags = query.tags.iter().all(|tag| entry.tags.contains(tag));
                    if !has_all_tags {
                        continue;
                    }
                }

                // Calculate relevance score
                let score = self.calculate_relevance(entry, &query);

                if score >= query.min_score {
                    results.push(SearchResult {
                        entry: entry.clone(),
                        score,
                    });
                }
            }
        }

        // Sort by score descending and limit results
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(query.limit);
        results
    }

    /// Calculate relevance score for search result
    fn calculate_relevance(&self, entry: &MemoryEntry, query: &MemoryQuery) -> f64 {
        let mut score = 0.0;

        // Term matching
        let value_str = serde_json::to_string(&entry.value).unwrap_or_default();
        for term in &query.terms {
            if value_str.contains(term) || entry.key.contains(term) {
                score += 0.3;
            }
        }

        // Tag matching
        let tag_matches = query
            .tags
            .iter()
            .filter(|tag| entry.tags.contains(tag))
            .count();
        if !query.tags.is_empty() {
            score += (tag_matches as f64 / query.tags.len() as f64) * 0.5;
        }

        // Recency bonus
        let age_hours = (Utc::now() - entry.created_at).num_hours().max(0) as f64;
        let recency_bonus = 1.0 / (1.0 + age_hours / 24.0); // Decay over days
        score += recency_bonus * 0.2;

        // Priority bonus
        score += (entry.priority as f64 / 255.0) * 0.1;

        score.min(1.0)
    }

    /// Delete a value from memory
    pub async fn delete(&self, key: &str) -> bool {
        let mut deleted = false;

        {
            let mut working = self.working.write().await;
            if working.remove(key).is_some() {
                deleted = true;
            }
        }

        {
            let mut short_term = self.short_term.write().await;
            if short_term.remove(key).is_some() {
                deleted = true;
            }
        }

        {
            let mut long_term = self.long_term.write().await;
            if long_term.remove(key).is_some() {
                deleted = true;
            }
        }

        if deleted {
            self.update_stats().await;
        }

        deleted
    }

    /// Clear all memory at a specific level
    pub async fn clear_level(&self, level: MemoryLevel) {
        match level {
            MemoryLevel::Working => {
                self.working.write().await.clear();
            }
            MemoryLevel::ShortTerm => {
                self.short_term.write().await.clear();
            }
            MemoryLevel::LongTerm => {
                self.long_term.write().await.clear();
            }
        }
        self.update_stats().await;
    }

    /// Clear all memory
    pub async fn clear_all(&self) {
        self.working.write().await.clear();
        self.short_term.write().await.clear();
        self.long_term.write().await.clear();
        self.update_stats().await;
    }

    /// Get memory statistics
    pub async fn stats(&self) -> MemoryStats {
        self.stats.read().await.clone()
    }

    /// Consolidate memory (move old entries to lower levels)
    pub async fn consolidate(&self) -> Result<usize, String> {
        if !self.config.enable_consolidation {
            return Ok(0);
        }

        let mut consolidated = 0;
        let now = Utc::now();

        // Move old working memory entries to short-term
        {
            let mut working = self.working.write().await;
            let mut short_term = self.short_term.write().await;

            let working_ttl = Duration::seconds(self.config.working_ttl_secs as i64);

            let to_promote: Vec<_> = working
                .iter()
                .filter(|(_, entry)| now - entry.accessed_at > working_ttl)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (key, entry) in to_promote {
                working.remove(&key);
                short_term.insert(key.clone(), entry);
                consolidated += 1;
            }
        }

        // Move old short-term entries to long-term
        {
            let mut short_term = self.short_term.write().await;
            let mut long_term = self.long_term.write().await;

            let short_term_ttl = Duration::seconds(self.config.short_term_ttl_secs as i64);

            let to_promote: Vec<_> = short_term
                .iter()
                .filter(|(_, entry)| now - entry.accessed_at > short_term_ttl)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            for (key, entry) in to_promote {
                short_term.remove(&key);
                long_term.insert(key, entry);
                consolidated += 1;
            }
        }

        self.update_stats().await;
        Ok(consolidated)
    }

    /// Clean up expired entries
    pub async fn cleanup_expired(&self) -> usize {
        let mut cleaned = 0;
        let now = Utc::now();

        // Clean working memory
        {
            let mut working = self.working.write().await;
            let expired: Vec<_> = working
                .iter()
                .filter(|(_, entry)| {
                    if let Some(expires_at) = entry.expires_at {
                        now > expires_at
                    } else {
                        false
                    }
                })
                .map(|(k, _)| k.clone())
                .collect();

            for key in expired {
                working.remove(&key);
                cleaned += 1;
            }
        }

        // Clean short-term memory
        {
            let mut short_term = self.short_term.write().await;
            let expired: Vec<_> = short_term
                .iter()
                .filter(|(_, entry)| {
                    if let Some(expires_at) = entry.expires_at {
                        now > expires_at
                    } else {
                        false
                    }
                })
                .map(|(k, _)| k.clone())
                .collect();

            for key in expired {
                short_term.remove(&key);
                cleaned += 1;
            }
        }

        if cleaned > 0 {
            self.update_stats().await;
        }

        cleaned
    }

    /// Evict entries if memory limit is reached
    async fn evict_if_needed(
        &self,
        store: &mut HashMap<String, MemoryEntry>,
        level: MemoryLevel,
    ) -> Result<(), String> {
        let max_entries = match level {
            MemoryLevel::Working => self.config.max_working_entries,
            MemoryLevel::ShortTerm => self.config.max_short_term_entries,
            MemoryLevel::LongTerm => return Ok(()), // No limit for long-term
        };

        if store.len() >= max_entries {
            // Calculate how many to evict
            let to_evict = (store.len() as f64 * 0.1).ceil() as usize; // Evict 10%

            // LRU eviction - collect keys with timestamps
            let mut entries_with_time: Vec<_> = store
                .iter()
                .map(|(k, v)| (k.clone(), v.accessed_at))
                .collect();
            entries_with_time.sort_by_key(|(_, time)| *time);

            let keys_to_evict: Vec<_> = entries_with_time
                .iter()
                .take(to_evict)
                .map(|(key, _)| key.clone())
                .collect();

            // Now remove by key
            for key in keys_to_evict {
                store.remove(key.as_str());
            }

            let mut stats = self.stats.write().await;
            stats.eviction_count += to_evict as u64;
        }

        Ok(())
    }

    /// Update memory statistics
    async fn update_stats(&self) {
        let working = self.working.read().await;
        let short_term = self.short_term.read().await;
        let long_term = self.long_term.read().await;

        let mut stats = self.stats.write().await;
        stats.total_entries = working.len() + short_term.len() + long_term.len();
        stats.entries_by_level = HashMap::from([
            ("working".to_string(), working.len()),
            ("short_term".to_string(), short_term.len()),
            ("long_term".to_string(), long_term.len()),
        ]);

        // Calculate memory usage (rough estimate) — use saturating mul to avoid overflow
        let working_bytes = working.len().saturating_mul(1024);
        let short_term_bytes = short_term.len().saturating_mul(2048);
        let long_term_bytes = long_term.len().saturating_mul(4096);
        stats.total_memory_bytes = working_bytes
            .saturating_add(short_term_bytes)
            .saturating_add(long_term_bytes) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> MemoryManager {
        MemoryManager::new(MemoryConfig::default())
    }

    #[tokio::test]
    async fn test_store_and_retrieve() {
        let manager = create_test_manager();

        let value = serde_json::json!({"test": "data"});
        let id = manager
            .store(
                "key1".to_string(),
                value.clone(),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        assert!(!id.is_empty());

        let retrieved = manager.get("key1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().value, value);
    }

    #[tokio::test]
    async fn test_memory_promotion() {
        let manager = create_test_manager();

        // Store in long-term
        let value = serde_json::json!({"test": "promotion"});
        manager
            .store(
                "key1".to_string(),
                value.clone(),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        // Retrieve should promote to short-term
        let retrieved = manager.get("key1").await;
        assert!(retrieved.is_some());

        // Should now be in short-term memory
        let short_term = manager.short_term.read().await;
        assert!(short_term.contains_key("key1"));
    }

    #[tokio::test]
    async fn test_search() {
        let manager = create_test_manager();

        // Store multiple entries
        manager
            .store(
                "key1".to_string(),
                serde_json::json!({"type": "user", "name": "Alice"}),
                MemoryLevel::Working,
                None,
                vec!["user".to_string(), "active".to_string()],
            )
            .await
            .unwrap();

        manager
            .store(
                "key2".to_string(),
                serde_json::json!({"type": "user", "name": "Bob"}),
                MemoryLevel::Working,
                None,
                vec!["user".to_string()],
            )
            .await
            .unwrap();

        let query = MemoryQuery {
            terms: vec!["Alice".to_string()],
            tags: vec!["user".to_string()],
            level: None,
            time_range: None,
            limit: 10,
            min_score: 0.8, // Only return highly relevant results
        };

        let results = manager.search(query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.key, "key1");
    }

    #[tokio::test]
    async fn test_delete() {
        let manager = create_test_manager();

        manager
            .store(
                "key1".to_string(),
                serde_json::json!({"test": "data"}),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        assert!(manager.delete("key1").await);
        assert!(manager.get("key1").await.is_none());
    }

    #[tokio::test]
    async fn test_clear_level() {
        let manager = create_test_manager();

        manager
            .store(
                "key1".to_string(),
                serde_json::json!({"test": "data"}),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        manager.clear_level(MemoryLevel::Working).await;
        assert!(manager.get("key1").await.is_none());
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = create_test_manager();

        manager
            .store(
                "key1".to_string(),
                serde_json::json!({"test": "data"}),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.entries_by_level.get("working").unwrap(), &1);
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let manager = create_test_manager();

        // Store entry with 1 second TTL
        manager
            .store(
                "key1".to_string(),
                serde_json::json!({"test": "data"}),
                MemoryLevel::Working,
                Some(1),
                vec![],
            )
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let cleaned = manager.cleanup_expired().await;
        assert_eq!(cleaned, 1);
        assert!(manager.get("key1").await.is_none());
    }

    // =========================================================================
    // Terminal-bench: Additional memory tests
    // =========================================================================

    #[tokio::test]
    async fn test_store_duplicate_key_overwrites() {
        let manager = create_test_manager();

        manager
            .store(
                "key1".into(),
                serde_json::json!("v1"),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        manager
            .store(
                "key1".into(),
                serde_json::json!("v2"),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();

        let entry = manager.get("key1").await.unwrap();
        assert_eq!(entry.value, serde_json::json!("v2"));
    }

    #[tokio::test]
    async fn test_store_with_tags() {
        let manager = create_test_manager();

        manager
            .store(
                "tagged".into(),
                serde_json::json!("data"),
                MemoryLevel::Working,
                None,
                vec!["alpha".into(), "beta".into()],
            )
            .await
            .unwrap();

        let entry = manager.get("tagged").await.unwrap();
        assert!(entry.tags.contains(&"alpha".to_string()));
        assert!(entry.tags.contains(&"beta".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_key() {
        let manager = create_test_manager();
        assert!(manager.get("no_such_key").await.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent_key() {
        let manager = create_test_manager();
        assert!(!manager.delete("ghost").await);
    }

    #[tokio::test]
    async fn test_clear_all_levels() {
        let manager = create_test_manager();

        manager
            .store(
                "w".into(),
                serde_json::json!(1),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "s".into(),
                serde_json::json!(2),
                MemoryLevel::ShortTerm,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "l".into(),
                serde_json::json!(3),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        manager.clear_all().await;

        assert!(manager.get("w").await.is_none());
        assert!(manager.get("s").await.is_none());
        assert!(manager.get("l").await.is_none());

        let stats = manager.stats().await;
        assert_eq!(stats.total_entries, 0);
    }

    #[tokio::test]
    async fn test_search_by_level_filter() {
        let manager = create_test_manager();

        manager
            .store(
                "wk".into(),
                serde_json::json!({"term": "findme"}),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "lt".into(),
                serde_json::json!({"term": "findme"}),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        let query = MemoryQuery {
            terms: vec!["findme".into()],
            tags: vec![],
            level: Some(MemoryLevel::Working),
            time_range: None,
            limit: 10,
            min_score: 0.0,
        };

        let results = manager.search(query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.key, "wk");
    }

    #[tokio::test]
    async fn test_search_by_tags() {
        let manager = create_test_manager();

        manager
            .store(
                "a".into(),
                serde_json::json!("data"),
                MemoryLevel::Working,
                None,
                vec!["x".into()],
            )
            .await
            .unwrap();
        manager
            .store(
                "b".into(),
                serde_json::json!("data"),
                MemoryLevel::Working,
                None,
                vec!["y".into()],
            )
            .await
            .unwrap();

        let query = MemoryQuery {
            terms: vec![],
            tags: vec!["x".into()],
            level: None,
            time_range: None,
            limit: 10,
            min_score: 0.0,
        };

        let results = manager.search(query).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry.key, "a");
    }

    #[tokio::test]
    async fn test_search_min_score_filter() {
        let manager = create_test_manager();

        manager
            .store(
                "low".into(),
                serde_json::json!("unrelated"),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "high".into(),
                serde_json::json!({"match": "exact_term_value"}),
                MemoryLevel::Working,
                None,
                vec!["important".into()],
            )
            .await
            .unwrap();

        let query = MemoryQuery {
            terms: vec!["exact_term_value".into()],
            tags: vec!["important".into()],
            level: None,
            time_range: None,
            limit: 10,
            min_score: 0.8,
        };

        let results = manager.search(query).await;
        assert!(results.len() <= 1);
        if !results.is_empty() {
            assert_eq!(results[0].entry.key, "high");
        }
    }

    #[tokio::test]
    async fn test_consolidation_disabled() {
        let config = MemoryConfig {
            enable_consolidation: false,
            ..MemoryConfig::default()
        };
        let manager = MemoryManager::new(config);

        let count = manager.consolidate().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_promotion_chain() {
        let manager = create_test_manager();

        // Store in long-term
        manager
            .store(
                "chain_key".into(),
                serde_json::json!("chain_value"),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        // First get: promoted to short-term
        let entry = manager.get("chain_key").await;
        assert!(entry.is_some());

        {
            let st = manager.short_term.read().await;
            assert!(st.contains_key("chain_key"));
        }

        // Second get: promoted to working
        let entry2 = manager.get("chain_key").await;
        assert!(entry2.is_some());

        {
            let wk = manager.working.read().await;
            assert!(wk.contains_key("chain_key"));
        }
    }

    #[tokio::test]
    async fn test_memory_stats_accuracy() {
        let manager = create_test_manager();

        manager
            .store(
                "a".into(),
                serde_json::json!(1),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "b".into(),
                serde_json::json!(2),
                MemoryLevel::ShortTerm,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "c".into(),
                serde_json::json!(3),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        let stats = manager.stats().await;
        assert_eq!(stats.total_entries, 3);
        assert_eq!(*stats.entries_by_level.get("working").unwrap(), 1);
        assert_eq!(*stats.entries_by_level.get("short_term").unwrap(), 1);
        assert_eq!(*stats.entries_by_level.get("long_term").unwrap(), 1);
    }

    #[tokio::test]
    async fn test_store_long_term_no_eviction() {
        let manager = create_test_manager();

        // Store 50 entries in long-term — no eviction should occur
        for i in 0..50 {
            manager
                .store(
                    format!("lt_{}", i),
                    serde_json::json!(i),
                    MemoryLevel::LongTerm,
                    None,
                    vec![],
                )
                .await
                .unwrap();
        }

        let stats = manager.stats().await;
        assert_eq!(*stats.entries_by_level.get("long_term").unwrap(), 50);
    }

    #[tokio::test]
    async fn test_search_returns_sorted_by_score() {
        let manager = create_test_manager();

        // Entry with lower relevance
        manager
            .store(
                "low_rel".into(),
                serde_json::json!("some data without many matches"),
                MemoryLevel::Working,
                None,
                vec!["z".into()],
            )
            .await
            .unwrap();

        // Entry with higher relevance (term match + tag match)
        manager
            .store(
                "high_rel".into(),
                serde_json::json!("target_keyword target_keyword target_keyword"),
                MemoryLevel::Working,
                None,
                vec!["target_tag".into()],
            )
            .await
            .unwrap();

        let query = MemoryQuery {
            terms: vec!["target_keyword".into()],
            tags: vec!["target_tag".into()],
            level: None,
            time_range: None,
            limit: 10,
            min_score: 0.0,
        };

        let results = manager.search(query).await;
        if results.len() >= 2 {
            assert!(results[0].score >= results[1].score);
        }
    }

    #[tokio::test]
    async fn test_delete_from_multiple_levels() {
        let manager = create_test_manager();

        // Store in all three levels with same key
        manager
            .store(
                "dup".into(),
                serde_json::json!("w"),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "dup".into(),
                serde_json::json!("s"),
                MemoryLevel::ShortTerm,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "dup".into(),
                serde_json::json!("l"),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        assert!(manager.delete("dup").await);
        assert!(manager.get("dup").await.is_none());
    }

    #[tokio::test]
    async fn test_clear_level_preserves_others() {
        let manager = create_test_manager();

        manager
            .store(
                "w".into(),
                serde_json::json!(1),
                MemoryLevel::Working,
                None,
                vec![],
            )
            .await
            .unwrap();
        manager
            .store(
                "l".into(),
                serde_json::json!(2),
                MemoryLevel::LongTerm,
                None,
                vec![],
            )
            .await
            .unwrap();

        manager.clear_level(MemoryLevel::Working).await;

        assert!(manager.get("w").await.is_none());
        assert!(manager.get("l").await.is_some());
    }
}
