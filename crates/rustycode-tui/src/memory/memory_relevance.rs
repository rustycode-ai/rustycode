//! Memory relevance scoring for automatic injection
//!
//! This module provides intelligent scoring of memories against user queries
//! to determine which memories should be automatically injected into conversations.

// Complete implementation - pending integration with memory auto-injection
#![allow(dead_code)]

use crate::memory_auto::AutoMemory;
use chrono::Utc;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;

/// Default relevance threshold for memory injection
pub const DEFAULT_RELEVANCE_THRESHOLD: f64 = 0.7;

/// Maximum number of memories to inject
pub const DEFAULT_MAX_INJECTIONS: usize = 5;

/// Scoring timeout in milliseconds
pub const SCORING_TIMEOUT_MS: u64 = 50;

/// Score cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    score: f64,
    timestamp: std::time::Instant,
}

/// Score cache to avoid repeated calculations
pub struct ScoreCache {
    cache: Arc<Mutex<HashMap<String, CacheEntry>>>,
    ttl: std::time::Duration,
}

impl ScoreCache {
    /// Create a new score cache with TTL
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: std::time::Duration::from_secs(ttl_secs),
        }
    }

    /// Get cached score if available and not expired
    pub fn get(&self, key: &str) -> Option<f64> {
        let cache = self.cache.lock().ok()?;
        let entry = cache.get(key)?;

        if entry.timestamp.elapsed() < self.ttl {
            Some(entry.score)
        } else {
            None
        }
    }

    /// Set cached score
    pub fn set(&self, key: String, score: f64) {
        if let Ok(mut cache) = self.cache.lock() {
            // Clean up expired entries
            let now = std::time::Instant::now();
            cache.retain(|_, entry| now.duration_since(entry.timestamp) < self.ttl);

            // Add new entry
            cache.insert(
                key,
                CacheEntry {
                    score,
                    timestamp: now,
                },
            );
        }
    }

    /// Clear all cached scores
    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
}

impl Default for ScoreCache {
    fn default() -> Self {
        Self::new(300) // 5 minutes default TTL
    }
}

/// Global score cache
static SCORE_CACHE: once_cell::sync::Lazy<ScoreCache> =
    once_cell::sync::Lazy::new(ScoreCache::default);

/// Score a memory's relevance to a query
///
/// # Arguments
///
/// * `query` - The user's message/query
/// * `memory` - The auto-memory to score
///
/// # Returns
///
/// A relevance score between 0.0 and 1.0
///
/// # Scoring Factors
///
/// 1. **Exact keyword match** (0.5 weight): If query contains memory key
/// 2. **Semantic similarity** (0.3 weight): Word overlap between query and memory value
/// 3. **Importance weighting** (multiplier): Memory importance score (0.0-1.0)
/// 4. **Recent access boost** (0.1 bonus): If accessed in last 24 hours
///
/// # Performance
///
/// Results are cached for 5 minutes to avoid repeated calculations.
///
/// # Examples
///
/// ```rust,ignore
/// use rustycode_tui::memory_relevance::score_relevance;
/// use rustycode_tui::memory_auto::{AutoMemory, MemoryType};
///
/// let memory = AutoMemory::new("theme", "dark mode preference", MemoryType::Preference);
/// let score = score_relevance("What's my theme preference?", &memory);
/// assert!(score > 0.7); // High relevance
/// ```
pub fn score_relevance(query: &str, memory: &AutoMemory) -> f64 {
    // Create cache key from query and memory ID
    let cache_key = format!("{}:{}", query.to_lowercase(), memory.id);

    // Check cache first
    if let Some(cached_score) = SCORE_CACHE.get(&cache_key) {
        return cached_score;
    }

    let mut score = 0.0;

    // Convert to lowercase for case-insensitive matching
    let query_lower = query.to_lowercase();
    let memory_key_lower = memory.key.to_lowercase();
    let memory_value_lower = memory.value.to_lowercase();

    // Factor 1: Exact keyword match (high confidence)
    if query_lower.contains(&memory_key_lower) {
        score += 0.5;
    }

    // Factor 2: Semantic similarity (approximate with word overlap)
    let query_words: HashSet<_> = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2) // Skip short words
        .collect();

    let memory_words: HashSet<_> = memory_value_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect();

    if !query_words.is_empty() {
        let overlap = query_words.intersection(&memory_words).count();

        let overlap_ratio = overlap as f64 / query_words.len() as f64;
        score += overlap_ratio * 0.3;
    }

    // Factor 3: Importance weighting
    score *= memory.importance;

    // Factor 4: Recent access boost
    let hours_since_access = (Utc::now() - memory.accessed_at).num_hours();
    if hours_since_access < 24 {
        score += 0.1;
    }

    // Cap at 1.0
    let final_score = score.min(1.0);

    // Cache the result
    SCORE_CACHE.set(cache_key, final_score);

    final_score
}

/// Score multiple memories against a query
///
/// # Arguments
///
/// * `query` - The user's message/query
/// * `memories` - Slice of auto-memories to score
///
/// # Returns
///
/// Vec of (memory, score) tuples sorted by score (highest first)
pub fn score_memories(query: &str, memories: &[AutoMemory]) -> Vec<(AutoMemory, f64)> {
    let start = std::time::Instant::now();

    let mut scored: Vec<(AutoMemory, f64)> = memories
        .iter()
        .map(|memory| {
            let score = score_relevance(query, memory);
            (memory.clone(), score)
        })
        .filter(|(_, score)| *score > 0.0) // Only keep relevant
        .collect();

    // Sort by score (highest first)
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let elapsed = start.elapsed();
    if elapsed.as_millis() > SCORING_TIMEOUT_MS as u128 {
        tracing::warn!(
            "Memory scoring took {}ms (exceeded {}ms threshold)",
            elapsed.as_millis(),
            SCORING_TIMEOUT_MS
        );
    }

    scored
}

/// Get top N relevant memories for a query
///
/// # Arguments
///
/// * `query` - The user's message/query
/// * `memories` - Slice of auto-memories
/// * `threshold` - Minimum relevance score (0.0-1.0)
/// * `max_results` - Maximum number of memories to return
///
/// # Returns
///
/// Vec of (memory, score) tuples for memories above threshold
pub fn get_relevant_memories(
    query: &str,
    memories: &[AutoMemory],
    threshold: f64,
    max_results: usize,
) -> Vec<(AutoMemory, f64)> {
    let scored = score_memories(query, memories);

    scored
        .into_iter()
        .filter(|(_, score)| *score >= threshold)
        .take(max_results)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_auto::{AutoMemory, MemoryType};

    #[test]
    fn test_exact_keyword_match() {
        let memory = AutoMemory::new("theme", "dark mode", MemoryType::Preference);
        let score = score_relevance("What's my theme?", &memory);

        // Should have high score due to keyword match
        assert!(score > 0.5);
    }

    #[test]
    fn test_word_overlap() {
        let memory = AutoMemory::new("preference", "dark mode enabled", MemoryType::Preference);
        let score = score_relevance("I like dark themes", &memory);

        // Should have moderate score due to word overlap ("dark")
        assert!(score > 0.0);
        assert!(score < 0.5); // No exact keyword match
    }

    #[test]
    fn test_importance_weighting() {
        let mut preference = AutoMemory::new("theme", "dark mode", MemoryType::Preference);
        preference.importance = 0.9;

        let mut context = AutoMemory::new("file", "main.rs", MemoryType::Context);
        context.importance = 0.5;

        let pref_score = score_relevance("theme preference", &preference);
        let ctx_score = score_relevance("file context", &context);

        // Preference should score higher due to importance
        assert!(pref_score > ctx_score);
    }

    #[test]
    fn test_recent_access_boost() {
        let mut memory = AutoMemory::new("theme", "dark mode", MemoryType::Preference);
        memory.accessed_at = Utc::now(); // Just accessed

        let score = score_relevance("theme", &memory);

        // Should get recent access boost
        assert!(score > 0.5);
    }

    #[test]
    fn test_score_capping() {
        let mut memory = AutoMemory::new("theme", "dark mode", MemoryType::Preference);
        memory.importance = 1.0;

        let score = score_relevance("theme theme theme", &memory);

        // Should be capped at 1.0
        assert!(score <= 1.0);
    }

    #[test]
    fn test_score_memories_sorting() {
        let memories = vec![
            AutoMemory::new("unrelated", "other thing", MemoryType::Context),
            AutoMemory::new("theme", "dark mode", MemoryType::Preference),
            AutoMemory::new("model", "claude", MemoryType::Preference),
        ];

        let scored = score_memories("theme preference", &memories);

        // Should be sorted by score (highest first)
        assert_eq!(scored[0].0.key, "theme");
    }

    #[test]
    fn test_get_relevant_memories_threshold() {
        let memories = vec![
            AutoMemory::new("unrelated", "other", MemoryType::Context),
            AutoMemory::new("theme", "dark mode", MemoryType::Preference),
        ];

        let relevant = get_relevant_memories("theme", &memories, 0.5, 5);

        // Should only return memories above threshold
        assert!(relevant.len() <= 2);
        assert!(relevant.iter().all(|(_, score)| *score >= 0.5));
    }

    #[test]
    fn test_get_relevant_memories_limit() {
        let memories = vec![
            AutoMemory::new("theme", "dark", MemoryType::Preference),
            AutoMemory::new("model", "claude", MemoryType::Preference),
            AutoMemory::new("mode", "ask", MemoryType::Context),
        ];

        let relevant = get_relevant_memories("mode", &memories, 0.0, 2);

        // Should limit to max_results
        assert!(relevant.len() <= 2);
    }

    #[test]
    fn test_no_match_returns_zero() {
        let memory = AutoMemory::new("theme", "dark mode", MemoryType::Preference);
        let score = score_relevance("completely unrelated query", &memory);

        // Should have low score
        assert!(score < 0.3);
    }
}
