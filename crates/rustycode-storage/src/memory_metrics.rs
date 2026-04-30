//! Memory Effectiveness Metrics
//!
//! Tracks metrics related to memory system performance, including capture rates,
//! retrieval precision, and effectiveness of different memory types.
//!
//! ## Features
//!
//! - **Comprehensive Metrics**: Track sessions, events, summaries, vector/keyword memories
//! - **Query Recording**: Record each memory query with results and relevance feedback
//! - **Effectiveness Reports**: Generate aggregated reports on memory system performance
//! - **Persistence**: Save and load metrics from files
//!
//! ## Example
//!
//! ```rust,no_run
//! use rustycode_storage::memory_metrics::{MemoryMetrics, QueryRecord};
//! use std::path::Path;
//!
//! # fn main() -> anyhow::Result<()> {
//! // Create or load metrics
//! let mut metrics = MemoryMetrics::load_from_file(Path::new("memory_metrics.json"))
//!     .unwrap_or_default();
//!
//! // Record a memory query
//! metrics.record_query("find auth code", 5);
//!
//! // Record memory usage
//! metrics.record_memory_used("memory_123");
//!
//! // Generate report
//! let report = metrics.generate_report();
//!
//! // Save metrics
//! metrics.save_to_file(Path::new("memory_metrics.json"))?;
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Tracks memory system effectiveness metrics
///
/// This struct maintains counters and statistics for various aspects of the
/// memory system, including capture rates, retrieval performance, and
/// memory type utilization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    /// Number of sessions captured for memory
    pub sessions_captured: u64,

    /// Number of events captured for memory
    pub events_captured: u64,

    /// Number of summaries generated
    pub summaries_generated: u64,

    /// Number of vector-based memories stored
    pub vector_memories_stored: usize,

    /// Number of keyword-based memories stored
    pub keyword_memories_stored: usize,

    /// Number of patterns learned from sessions
    pub patterns_learned: usize,

    /// Total number of memory queries performed
    pub memory_queries: u64,

    /// Total number of memories retrieved across all queries
    pub memories_retrieved: u64,

    /// Number of memories injected into context
    pub memories_injected: u64,

    /// Number of memories used successfully (boosted)
    pub memories_boosted: u64,

    /// Number of memories pruned (never used)
    pub memories_pruned: u64,

    /// Average number of results returned per query
    pub avg_results_per_query: f32,

    /// Retrieval precision: ratio of relevant results to total retrieved
    pub retrieval_precision: f32,

    /// Session continuity score: how well memory maintains context
    pub session_continuity_score: f32,

    /// Query history for detailed analysis
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub query_history: Vec<QueryRecord>,

    /// Memory usage tracking: `memory_id` -> `use_count`
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub memory_usage: HashMap<String, u64>,

    /// When metrics tracking started
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub started_at: DateTime<Utc>,

    /// When metrics were last updated
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub last_updated: DateTime<Utc>,
}

/// Records a single memory query for analysis
///
/// Stores details about the query, results, and feedback on relevance
/// for calculating precision metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRecord {
    /// The query text or search terms
    pub query_text: String,

    /// Type of memory queried ("vector", "keyword", "hybrid")
    pub memory_type: String,

    /// Number of results returned
    pub results_count: usize,

    /// Number of results marked as relevant (feedback)
    pub relevant_count: usize,

    /// When the query was executed
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
}

impl QueryRecord {
    /// Create a new query record
    ///
    /// # Arguments
    ///
    /// * `query_text` - The text that was searched
    /// * `memory_type` - Type of memory system used
    /// * `results_count` - Number of results returned
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::QueryRecord;
    ///
    /// let record = QueryRecord::new(
    ///     "authentication code".to_string(),
    ///     "vector".to_string(),
    ///     5,
    /// );
    /// ```
    pub fn new(query_text: String, memory_type: String, results_count: usize) -> Self {
        Self {
            query_text,
            memory_type,
            results_count,
            relevant_count: 0,
            timestamp: Utc::now(),
        }
    }

    /// Mark results as relevant for precision calculation
    ///
    /// # Arguments
    ///
    /// * `count` - Number of relevant results from this query
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::QueryRecord;
    ///
    /// let mut record = QueryRecord::new(
    ///     "find auth".to_string(),
    ///     "keyword".to_string(),
    ///     5,
    /// );
    /// record.mark_relevant(3);
    /// assert_eq!(record.relevant_count, 3);
    /// ```
    pub fn mark_relevant(&mut self, count: usize) {
        self.relevant_count = count.min(self.results_count);
    }

    /// Calculate precision for this specific query
    ///
    /// Returns the ratio of relevant results to total results
    pub fn precision(&self) -> f32 {
        if self.results_count == 0 {
            return 0.0;
        }
        self.relevant_count as f32 / self.results_count as f32
    }
}

/// Aggregated effectiveness report for a time period
///
/// Provides a snapshot of memory system performance with key metrics
/// and insights for optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEffectivenessReport {
    /// Start of the reporting period
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub period_start: DateTime<Utc>,

    /// End of the reporting period
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub period_end: DateTime<Utc>,

    /// Aggregated metrics for the period
    pub total_metrics: MemoryMetrics,

    /// Most frequently used memories: (`memory_id`, `use_count`)
    pub top_memories_used: Vec<(String, u64)>,

    /// Memory IDs that were never used
    pub unused_memories: Vec<String>,

    /// Average session coverage score
    pub average_session_coverage: f32,
}

impl Default for MemoryMetrics {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            sessions_captured: 0,
            events_captured: 0,
            summaries_generated: 0,
            vector_memories_stored: 0,
            keyword_memories_stored: 0,
            patterns_learned: 0,
            memory_queries: 0,
            memories_retrieved: 0,
            memories_injected: 0,
            memories_boosted: 0,
            memories_pruned: 0,
            avg_results_per_query: 0.0,
            retrieval_precision: 0.0,
            session_continuity_score: 0.0,
            query_history: Vec::new(),
            memory_usage: HashMap::new(),
            started_at: now,
            last_updated: now,
        }
    }
}

impl MemoryMetrics {
    /// Create a new metrics instance with current timestamp
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    ///
    /// let metrics = MemoryMetrics::new();
    /// assert_eq!(metrics.sessions_captured, 0);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a memory query
    ///
    /// Increments query counters and stores the query record for analysis.
    ///
    /// # Arguments
    ///
    /// * `query` - The query text
    /// * `results_count` - Number of results returned
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    ///
    /// let mut metrics = MemoryMetrics::new();
    /// metrics.record_query("authentication", 5);
    /// assert_eq!(metrics.memory_queries, 1);
    /// ```
    pub fn record_query(&mut self, query: impl Into<String>, results_count: usize) {
        let query_text = query.into();
        self.memory_queries = self.memory_queries.saturating_add(1);
        self.memories_retrieved = self.memories_retrieved.saturating_add(results_count as u64);

        let record = QueryRecord::new(query_text, "hybrid".to_string(), results_count);
        self.query_history.push(record);

        self.update_averages();
        self.last_updated = Utc::now();
    }

    /// Record a memory query with specific memory type
    ///
    /// Like `record_query` but specifies which memory type was used.
    ///
    /// # Arguments
    ///
    /// * `query` - The query text
    /// * `memory_type` - Type of memory ("vector", "keyword", "hybrid")
    /// * `results_count` - Number of results returned
    pub fn record_query_with_type(
        &mut self,
        query: impl Into<String>,
        memory_type: impl Into<String>,
        results_count: usize,
    ) {
        let query_text = query.into();
        let memory_type = memory_type.into();
        self.memory_queries = self.memory_queries.saturating_add(1);
        self.memories_retrieved = self.memories_retrieved.saturating_add(results_count as u64);

        let record = QueryRecord::new(query_text, memory_type, results_count);
        self.query_history.push(record);

        self.update_averages();
        self.last_updated = Utc::now();
    }

    /// Record feedback on query relevance
    ///
    /// Updates the most recent query with relevance feedback for
    /// precision calculation.
    ///
    /// # Arguments
    ///
    /// * `relevant_count` - Number of results that were actually relevant
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    ///
    /// let mut metrics = MemoryMetrics::new();
    /// metrics.record_query("test", 5);
    /// metrics.record_relevance_feedback(3);
    /// ```
    pub fn record_relevance_feedback(&mut self, relevant_count: usize) {
        if let Some(record) = self.query_history.last_mut() {
            record.mark_relevant(relevant_count);
            self.update_averages();
        }
    }

    /// Record that a memory was used
    ///
    /// Increments the usage counter for a specific memory.
    ///
    /// # Arguments
    ///
    /// * `memory_id` - Unique identifier for the memory
    pub fn record_memory_used(&mut self, memory_id: impl Into<String>) {
        let id = memory_id.into();
        let count = self.memory_usage.entry(id).or_insert(0);
        *count += 1;
        self.memories_injected = self.memories_injected.saturating_add(1);
        self.last_updated = Utc::now();
    }

    /// Record that a memory was boosted (used successfully)
    ///
    /// Increments the boosted counter for memories that provided value.
    ///
    /// # Arguments
    ///
    /// * `memory_id` - Unique identifier for the memory
    pub fn record_memory_boosted(&mut self, memory_id: impl Into<String>) {
        let id = memory_id.into();
        let count = self.memory_usage.entry(id).or_insert(0);
        *count += 1;
        self.memories_boosted = self.memories_boosted.saturating_add(1);
        self.last_updated = Utc::now();
    }

    /// Record that a memory was pruned
    ///
    /// Increments the counter for memories removed due to non-use.
    ///
    /// # Arguments
    ///
    /// * `memory_id` - Unique identifier for the pruned memory
    pub fn record_memory_pruned(&mut self, memory_id: impl Into<String>) {
        self.memories_pruned += 1;
        self.memory_usage.remove(&memory_id.into());
        self.last_updated = Utc::now();
    }

    /// Record session capture
    ///
    /// Increments the sessions captured counter.
    pub fn record_session_captured(&mut self) {
        self.sessions_captured += 1;
        self.last_updated = Utc::now();
    }

    /// Record event capture
    ///
    /// Increments the events captured counter.
    pub fn record_event_captured(&mut self) {
        self.events_captured += 1;
        self.last_updated = Utc::now();
    }

    /// Record summary generation
    ///
    /// Increments the summaries generated counter.
    pub fn record_summary_generated(&mut self) {
        self.summaries_generated += 1;
        self.last_updated = Utc::now();
    }

    /// Record vector memory storage
    ///
    /// Increments the vector memories stored counter.
    pub fn record_vector_memory_stored(&mut self) {
        self.vector_memories_stored += 1;
        self.last_updated = Utc::now();
    }

    /// Record keyword memory storage
    ///
    /// Increments the keyword memories stored counter.
    pub fn record_keyword_memory_stored(&mut self) {
        self.keyword_memories_stored += 1;
        self.last_updated = Utc::now();
    }

    /// Record pattern learning
    ///
    /// Increments the patterns learned counter.
    pub fn record_pattern_learned(&mut self) {
        self.patterns_learned += 1;
        self.last_updated = Utc::now();
    }

    /// Generate an effectiveness report
    ///
    /// Creates a comprehensive report with top memories, unused memories,
    /// and aggregated statistics.
    ///
    /// # Example
    ///
    /// ```
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    ///
    /// let mut metrics = MemoryMetrics::new();
    /// metrics.record_query("test", 5);
    /// metrics.record_memory_used("mem_1");
    /// metrics.record_memory_used("mem_1");
    /// metrics.record_memory_used("mem_2");
    ///
    /// let report = metrics.generate_report();
    /// assert_eq!(report.total_metrics.memory_queries, 1);
    /// ```
    pub fn generate_report(&self) -> MemoryEffectivenessReport {
        let mut top_memories: Vec<(String, u64)> = self
            .memory_usage
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        top_memories.sort_by_key(|b| std::cmp::Reverse(b.1));
        top_memories.truncate(10);

        let unused_memories: Vec<String> = self
            .memory_usage
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let coverage = self.calculate_session_coverage();

        MemoryEffectivenessReport {
            period_start: self.started_at,
            period_end: self.last_updated,
            total_metrics: self.clone(),
            top_memories_used: top_memories,
            unused_memories,
            average_session_coverage: coverage,
        }
    }

    /// Save metrics to a JSON file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to save the metrics file
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or file writing fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    /// use std::path::Path;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let metrics = MemoryMetrics::new();
    /// metrics.save_to_file(Path::new("metrics.json"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("failed to serialize memory metrics")?;
        std::fs::write(path, json)
            .with_context(|| format!("failed to write metrics to {}", path.display()))?;
        Ok(())
    }

    /// Load metrics from a JSON file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the metrics file
    ///
    /// # Errors
    ///
    /// Returns an error if file reading or deserialization fails
    ///
    /// # Example
    ///
    /// ```no_run
    /// use rustycode_storage::memory_metrics::MemoryMetrics;
    /// use std::path::Path;
    ///
    /// # fn main() -> anyhow::Result<()> {
    /// let metrics = MemoryMetrics::load_from_file(Path::new("metrics.json"))?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let json = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read metrics from {}", path.display()))?;
        let metrics: Self =
            serde_json::from_str(&json).context("failed to deserialize memory metrics")?;
        Ok(metrics)
    }

    /// Calculate retrieval precision
    ///
    /// Returns the ratio of relevant results to total retrieved results
    /// based on feedback recorded in query history.
    pub fn calculate_retrieval_precision(&self) -> f32 {
        let total_retrieved: usize = self
            .query_history
            .iter()
            .fold(0usize, |acc, q| acc.saturating_add(q.results_count));
        let total_relevant: usize = self
            .query_history
            .iter()
            .fold(0usize, |acc, q| acc.saturating_add(q.relevant_count));

        if total_retrieved == 0 {
            return 0.0;
        }
        total_relevant as f32 / total_retrieved as f32
    }

    /// Calculate session coverage score
    ///
    /// Returns a score (0.0 to 1.0) indicating how well memories cover
    /// session content. Higher scores indicate better coverage.
    pub fn calculate_session_coverage(&self) -> f32 {
        if self.sessions_captured == 0 {
            return 0.0;
        }

        let total_memories = self.vector_memories_stored + self.keyword_memories_stored;
        if total_memories == 0 {
            return 0.0;
        }

        let used_memories = self.memory_usage.values().filter(|v| **v > 0).count();
        let usage_ratio = used_memories as f32 / total_memories.max(1) as f32;

        let boost_ratio = if self.memories_injected > 0 {
            self.memories_boosted as f32 / self.memories_injected as f32
        } else {
            0.0
        };

        f32::midpoint(usage_ratio, boost_ratio)
    }

    /// Update average metrics after new data
    fn update_averages(&mut self) {
        if self.memory_queries > 0 {
            self.avg_results_per_query =
                self.memories_retrieved as f32 / self.memory_queries as f32;
        }
        self.retrieval_precision = self.calculate_retrieval_precision();
        self.session_continuity_score = self.calculate_session_coverage();
    }
}

/// Calculate retrieval precision from a slice of query records
///
/// Helper function for external precision calculations.
///
/// # Arguments
///
/// * `queries` - Slice of query records to analyze
///
/// # Returns
///
/// Precision as a float between 0.0 and 1.0
///
/// # Example
///
/// ```
/// use rustycode_storage::memory_metrics::{QueryRecord, calculate_retrieval_precision};
///
/// let queries = vec![
///     QueryRecord { query_text: "test".to_string(), memory_type: "vector".to_string(), results_count: 10, relevant_count: 7, timestamp: chrono::Utc::now() },
/// ];
/// let precision = calculate_retrieval_precision(&queries);
/// assert!((precision - 0.7).abs() < 0.01);
/// ```
pub fn calculate_retrieval_precision(queries: &[QueryRecord]) -> f32 {
    let total_retrieved: usize = queries.iter().map(|q| q.results_count).sum();
    let total_relevant: usize = queries.iter().map(|q| q.relevant_count).sum();

    if total_retrieved == 0 {
        return 0.0;
    }
    total_relevant as f32 / total_retrieved as f32
}

/// Calculate session coverage score from metrics
///
/// Helper function for external coverage calculations.
///
/// # Arguments
///
/// * `sessions_captured` - Number of sessions
/// * `total_memories` - Total memories stored
/// * `used_memories` - Count of memories that were used
/// * `memories_injected` - Total memories injected
/// * `memories_boosted` - Memories that provided value
///
/// # Returns
///
/// Coverage score between 0.0 and 1.0
///
/// # Example
///
/// ```
/// use rustycode_storage::memory_metrics::calculate_session_coverage;
///
/// let coverage = calculate_session_coverage(10, 50, 40, 30, 25);
/// assert!(coverage > 0.0 && coverage <= 1.0);
/// ```
pub fn calculate_session_coverage(
    sessions_captured: u64,
    total_memories: usize,
    used_memories: usize,
    memories_injected: u64,
    memories_boosted: u64,
) -> f32 {
    if sessions_captured == 0 || total_memories == 0 {
        return 0.0;
    }

    let usage_ratio = used_memories as f32 / total_memories as f32;

    let boost_ratio = if memories_injected > 0 {
        memories_boosted as f32 / memories_injected as f32
    } else {
        0.0
    };

    f32::midpoint(usage_ratio, boost_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_metrics() {
        let metrics = MemoryMetrics::default();
        assert_eq!(metrics.sessions_captured, 0);
        assert_eq!(metrics.memory_queries, 0);
        assert_eq!(metrics.retrieval_precision, 0.0);
    }

    #[test]
    fn test_new_metrics() {
        let metrics = MemoryMetrics::new();
        assert_eq!(metrics.sessions_captured, 0);
        assert!(metrics.started_at <= Utc::now());
    }

    #[test]
    fn test_record_query() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_query("test query", 5);

        assert_eq!(metrics.memory_queries, 1);
        assert_eq!(metrics.memories_retrieved, 5);
        assert_eq!(metrics.avg_results_per_query, 5.0);
        assert_eq!(metrics.query_history.len(), 1);
    }

    #[test]
    fn test_record_query_with_type() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_query_with_type("auth", "vector", 3);

        assert_eq!(metrics.memory_queries, 1);
        let record = &metrics.query_history[0];
        assert_eq!(record.memory_type, "vector");
    }

    #[test]
    fn test_relevance_feedback() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_query("test", 10);
        metrics.record_relevance_feedback(7);

        assert_eq!(metrics.query_history[0].relevant_count, 7);
        assert!((metrics.retrieval_precision - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_record_memory_used() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_memory_used("mem_123");
        metrics.record_memory_used("mem_123");
        metrics.record_memory_used("mem_456");

        assert_eq!(metrics.memories_injected, 3);
        assert_eq!(metrics.memory_usage.get("mem_123"), Some(&2));
        assert_eq!(metrics.memory_usage.get("mem_456"), Some(&1));
    }

    #[test]
    fn test_record_memory_boosted() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_memory_boosted("mem_1");
        metrics.record_memory_boosted("mem_1");

        assert_eq!(metrics.memories_boosted, 2);
        assert_eq!(metrics.memory_usage.get("mem_1"), Some(&2));
    }

    #[test]
    fn test_record_memory_pruned() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_memory_used("mem_old");
        assert!(metrics.memory_usage.contains_key("mem_old"));

        metrics.record_memory_pruned("mem_old");
        assert_eq!(metrics.memories_pruned, 1);
        assert!(!metrics.memory_usage.contains_key("mem_old"));
    }

    #[test]
    fn test_session_counters() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_session_captured();
        metrics.record_event_captured();
        metrics.record_summary_generated();

        assert_eq!(metrics.sessions_captured, 1);
        assert_eq!(metrics.events_captured, 1);
        assert_eq!(metrics.summaries_generated, 1);
    }

    #[test]
    fn test_memory_storage_counters() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_vector_memory_stored();
        metrics.record_vector_memory_stored();
        metrics.record_keyword_memory_stored();

        assert_eq!(metrics.vector_memories_stored, 2);
        assert_eq!(metrics.keyword_memories_stored, 1);
    }

    #[test]
    fn test_generate_report() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_query("test", 5);
        metrics.record_memory_used("mem_1");
        metrics.record_memory_used("mem_1");
        metrics.record_memory_used("mem_2");
        metrics.record_memory_used("mem_unused");

        let report = metrics.generate_report();

        assert_eq!(report.total_metrics.memory_queries, 1);
        assert_eq!(report.top_memories_used.len(), 3);
        assert_eq!(report.top_memories_used[0].0, "mem_1");
        assert_eq!(report.top_memories_used[0].1, 2);
    }

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("metrics.json");

        let mut metrics = MemoryMetrics::new();
        metrics.record_query("test", 5);
        metrics.record_memory_used("mem_1");

        metrics.save_to_file(&path).unwrap();
        let loaded = MemoryMetrics::load_from_file(&path).unwrap();

        assert_eq!(loaded.memory_queries, 1);
        assert_eq!(loaded.memories_retrieved, 5);
        assert_eq!(loaded.memory_usage.get("mem_1"), Some(&1));
    }

    #[test]
    fn test_calculate_retrieval_precision() {
        let queries = vec![
            QueryRecord {
                query_text: "q1".to_string(),
                memory_type: "vector".to_string(),
                results_count: 10,
                relevant_count: 7,
                timestamp: Utc::now(),
            },
            QueryRecord {
                query_text: "q2".to_string(),
                memory_type: "keyword".to_string(),
                results_count: 5,
                relevant_count: 3,
                timestamp: Utc::now(),
            },
        ];

        let precision = calculate_retrieval_precision(&queries);
        assert!((precision - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_calculate_session_coverage() {
        let coverage = calculate_session_coverage(10, 100, 80, 50, 40);
        let expected = ((80.0 / 100.0) + (40.0 / 50.0)) / 2.0;
        assert!((coverage - expected).abs() < 0.01);
    }

    #[test]
    fn test_zero_coverage_cases() {
        assert_eq!(calculate_session_coverage(0, 100, 80, 50, 40), 0.0);
        assert_eq!(calculate_session_coverage(10, 0, 0, 0, 0), 0.0);
    }

    #[test]
    fn test_query_record_precision() {
        let record = QueryRecord {
            query_text: "test".to_string(),
            memory_type: "vector".to_string(),
            results_count: 10,
            relevant_count: 8,
            timestamp: Utc::now(),
        };

        assert!((record.precision() - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_query_record_zero_precision() {
        let record = QueryRecord {
            query_text: "test".to_string(),
            memory_type: "vector".to_string(),
            results_count: 0,
            relevant_count: 0,
            timestamp: Utc::now(),
        };

        assert_eq!(record.precision(), 0.0);
    }

    #[test]
    fn test_retrieval_precision_updates() {
        let mut metrics = MemoryMetrics::new();
        metrics.record_query("q1", 10);
        metrics.record_relevance_feedback(8);
        metrics.record_query("q2", 5);
        metrics.record_relevance_feedback(3);

        let expected = (8.0 + 3.0) / (10.0 + 5.0);
        assert!((metrics.retrieval_precision - expected).abs() < 0.01);
    }

    #[test]
    fn test_memory_effectiveness_report_serialization() {
        let report = MemoryEffectivenessReport {
            period_start: Utc::now(),
            period_end: Utc::now(),
            total_metrics: MemoryMetrics::new(),
            top_memories_used: vec![("mem_1".to_string(), 5)],
            unused_memories: vec!["unused".to_string()],
            average_session_coverage: 0.75,
        };

        let json = serde_json::to_string(&report).unwrap();
        let deserialized: MemoryEffectivenessReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.top_memories_used.len(), 1);
        assert!((deserialized.average_session_coverage - 0.75).abs() < 0.01);
    }
}
