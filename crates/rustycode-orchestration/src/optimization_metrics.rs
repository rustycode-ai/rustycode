//! Unified metrics for orchestration optimizations.
//!
//! Tracks execution time savings, token savings from summarization and caching,
//! cache hit rates, and model routing distribution across tiers.

use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};

/// Unified metrics for orchestration optimizations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationMetrics {
    // Execution time metrics
    /// Total wall-clock execution time across all operations.
    pub total_execution_time_ms: u64,
    /// Hypothetical sequential execution time (baseline for parallel savings).
    pub sequential_execution_time_ms: u64,
    /// Actual parallel execution time.
    pub parallel_execution_time_ms: u64,

    // Token metrics
    /// Total input tokens before any optimization.
    pub total_input_tokens: usize,
    /// Input tokens after summarization.
    pub summarized_input_tokens: usize,
    /// Tokens saved by summarization.
    pub tokens_saved_by_summarization: usize,
    /// Tokens served from cache (no re-computation).
    pub cache_hit_tokens: usize,
    /// Aggregate tokens saved across all optimization strategies.
    pub total_tokens_saved: usize,

    // Cache metrics
    /// Number of cache hits.
    pub cache_hits: u64,
    /// Number of cache misses.
    pub cache_misses: u64,
    /// Cache hit rate as a fraction (0.0..1.0).
    pub cache_hit_rate: f64,

    // Model routing metrics
    /// Number of calls routed to the Musician tier (fast/cheap).
    pub musician_calls: u64,
    /// Number of calls routed to the Editor tier (moderate).
    pub editor_calls: u64,
    /// Number of calls routed to the Composer tier (capable/expensive).
    pub composer_calls: u64,
}

impl Default for OptimizationMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizationMetrics {
    /// Create a new metrics instance with all counters zeroed.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            total_execution_time_ms: 0,
            sequential_execution_time_ms: 0,
            parallel_execution_time_ms: 0,
            total_input_tokens: 0,
            summarized_input_tokens: 0,
            tokens_saved_by_summarization: 0,
            cache_hit_tokens: 0,
            total_tokens_saved: 0,
            cache_hits: 0,
            cache_misses: 0,
            cache_hit_rate: 0.0,
            musician_calls: 0,
            editor_calls: 0,
            composer_calls: 0,
        }
    }

    /// Recalculate cache hit rate from hits and misses.
    pub fn compute_cache_hit_rate(&mut self) {
        let total = self.cache_hits.saturating_add(self.cache_misses);
        if total == 0 {
            self.cache_hit_rate = 0.0;
        } else {
            self.cache_hit_rate = self.cache_hits as f64 / total as f64;
        }
    }

    /// Percentage of execution time saved by parallelization.
    ///
    /// Returns 0.0 when there is no sequential baseline.
    #[must_use]
    pub fn time_savings_percent(&self) -> f64 {
        if self.sequential_execution_time_ms == 0 {
            return 0.0;
        }
        let sequential = self.sequential_execution_time_ms as f64;
        let parallel = self.parallel_execution_time_ms as f64;
        (sequential - parallel) / sequential * 100.0
    }

    /// Percentage of tokens saved across all optimization strategies.
    ///
    /// Returns 0.0 when there are no input tokens to compare against.
    #[must_use]
    pub fn token_savings_percent(&self) -> f64 {
        if self.total_input_tokens == 0 {
            return 0.0;
        }
        self.total_tokens_saved as f64 / self.total_input_tokens as f64 * 100.0
    }

    /// Total number of model calls across all tiers.
    #[must_use]
    pub const fn total_model_calls(&self) -> u64 {
        self.musician_calls
            .saturating_add(self.editor_calls)
            .saturating_add(self.composer_calls)
    }

    /// Record a parallel-vs-sequential execution comparison.
    pub const fn record_execution(&mut self, sequential_ms: u64, parallel_ms: u64) {
        self.sequential_execution_time_ms = self
            .sequential_execution_time_ms
            .saturating_add(sequential_ms);
        self.parallel_execution_time_ms =
            self.parallel_execution_time_ms.saturating_add(parallel_ms);
        self.total_execution_time_ms = self.total_execution_time_ms.saturating_add(parallel_ms);
    }

    /// Record a single cache lookup result.
    pub fn record_cache_result(&mut self, hit: bool, tokens_saved: usize) {
        if hit {
            self.cache_hits = self.cache_hits.saturating_add(1);
            self.cache_hit_tokens = self.cache_hit_tokens.saturating_add(tokens_saved);
        } else {
            self.cache_misses = self.cache_misses.saturating_add(1);
        }
        self.total_tokens_saved = self.total_tokens_saved.saturating_add(tokens_saved);
        self.compute_cache_hit_rate();
    }

    /// Record a summarization pass.
    pub const fn record_summarization(&mut self, original_tokens: usize, summarized_tokens: usize) {
        let saved = original_tokens.saturating_sub(summarized_tokens);
        self.total_input_tokens = self.total_input_tokens.saturating_add(original_tokens);
        self.summarized_input_tokens = self
            .summarized_input_tokens
            .saturating_add(summarized_tokens);
        self.tokens_saved_by_summarization =
            self.tokens_saved_by_summarization.saturating_add(saved);
        self.total_tokens_saved = self.total_tokens_saved.saturating_add(saved);
    }

    /// Record a model call routed to a specific tier.
    pub const fn record_model_call(&mut self, tier: ExecutionTier) {
        match tier {
            ExecutionTier::Musician => {
                self.musician_calls = self.musician_calls.saturating_add(1);
            }
            ExecutionTier::Editor => {
                self.editor_calls = self.editor_calls.saturating_add(1);
            }
            ExecutionTier::Composer => {
                self.composer_calls = self.composer_calls.saturating_add(1);
            }
            ExecutionTier::Thinking => {
                // Thinking tier is tracked under Composer for routing metrics.
                self.composer_calls = self.composer_calls.saturating_add(1);
            }
        }
    }

    /// Generate a human-readable summary report.
    #[must_use]
    pub fn report(&self) -> String {
        format!(
            "Optimization Metrics Report\n\
             ============================\n\
             Execution Time:\n\
               Sequential baseline: {}ms\n\
               Parallel actual:     {}ms\n\
               Time saved:          {:.1}%\n\
               Total wall-clock:    {}ms\n\
             \n\
             Token Savings:\n\
               Total input tokens:        {}\n\
               Summarized input tokens:   {}\n\
               Saved by summarization:    {}\n\
               Cache hit tokens:          {}\n\
               Total tokens saved:        {} ({:.1}%)\n\
             \n\
             Cache Performance:\n\
               Hits:   {}\n\
               Misses: {}\n\
               Hit rate: {:.1}%\n\
             \n\
             Model Routing:\n\
               Musician calls: {}\n\
               Editor calls:   {}\n\
               Composer calls: {}\n\
               Total calls:    {}",
            self.sequential_execution_time_ms,
            self.parallel_execution_time_ms,
            self.time_savings_percent(),
            self.total_execution_time_ms,
            self.total_input_tokens,
            self.summarized_input_tokens,
            self.tokens_saved_by_summarization,
            self.cache_hit_tokens,
            self.total_tokens_saved,
            self.token_savings_percent(),
            self.cache_hits,
            self.cache_misses,
            self.cache_hit_rate * 100.0,
            self.musician_calls,
            self.editor_calls,
            self.composer_calls,
            self.total_model_calls(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_new_metrics_all_zeros() {
        let metrics = OptimizationMetrics::new();
        assert_eq!(metrics.total_execution_time_ms, 0);
        assert_eq!(metrics.sequential_execution_time_ms, 0);
        assert_eq!(metrics.parallel_execution_time_ms, 0);
        assert_eq!(metrics.total_input_tokens, 0);
        assert_eq!(metrics.summarized_input_tokens, 0);
        assert_eq!(metrics.tokens_saved_by_summarization, 0);
        assert_eq!(metrics.cache_hit_tokens, 0);
        assert_eq!(metrics.total_tokens_saved, 0);
        assert_eq!(metrics.cache_hits, 0);
        assert_eq!(metrics.cache_misses, 0);
        assert_eq!(metrics.cache_hit_rate, 0.0);
        assert_eq!(metrics.musician_calls, 0);
        assert_eq!(metrics.editor_calls, 0);
        assert_eq!(metrics.composer_calls, 0);
    }

    #[test]
    fn test_compute_cache_hit_rate() {
        let mut metrics = OptimizationMetrics::new();
        metrics.cache_hits = 7;
        metrics.cache_misses = 3;
        metrics.compute_cache_hit_rate();
        let expected = 7.0 / 10.0;
        assert!(
            (metrics.cache_hit_rate - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            metrics.cache_hit_rate
        );
    }

    #[test]
    fn test_cache_hit_rate_no_data() {
        let mut metrics = OptimizationMetrics::new();
        metrics.compute_cache_hit_rate();
        assert_eq!(metrics.cache_hit_rate, 0.0);
    }

    #[test]
    fn test_time_savings_percent() {
        let mut metrics = OptimizationMetrics::new();
        metrics.sequential_execution_time_ms = 1000;
        metrics.parallel_execution_time_ms = 400;
        let pct = metrics.time_savings_percent();
        assert!((pct - 60.0).abs() < 1e-9, "expected 60.0%, got {pct}%");
    }

    #[test]
    fn test_time_savings_no_baseline() {
        let metrics = OptimizationMetrics::new();
        assert_eq!(metrics.time_savings_percent(), 0.0);
    }

    #[test]
    fn test_token_savings_percent() {
        let mut metrics = OptimizationMetrics::new();
        metrics.total_input_tokens = 1000;
        metrics.total_tokens_saved = 300;
        let pct = metrics.token_savings_percent();
        assert!((pct - 30.0).abs() < 1e-9, "expected 30.0%, got {pct}%");
    }

    #[test]
    fn test_token_savings_no_input() {
        let metrics = OptimizationMetrics::new();
        assert_eq!(metrics.token_savings_percent(), 0.0);
    }

    #[test]
    fn test_record_execution() {
        let mut metrics = OptimizationMetrics::new();
        metrics.record_execution(100, 40);
        assert_eq!(metrics.sequential_execution_time_ms, 100);
        assert_eq!(metrics.parallel_execution_time_ms, 40);
        assert_eq!(metrics.total_execution_time_ms, 40);

        // Second recording accumulates.
        metrics.record_execution(200, 80);
        assert_eq!(metrics.sequential_execution_time_ms, 300);
        assert_eq!(metrics.parallel_execution_time_ms, 120);
        assert_eq!(metrics.total_execution_time_ms, 120);
    }

    #[test]
    fn test_record_cache_result() {
        let mut metrics = OptimizationMetrics::new();

        // Record a hit.
        metrics.record_cache_result(true, 500);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 0);
        assert_eq!(metrics.cache_hit_tokens, 500);
        assert_eq!(metrics.total_tokens_saved, 500);
        assert!(
            (metrics.cache_hit_rate - 1.0).abs() < 1e-9,
            "expected 1.0, got {}",
            metrics.cache_hit_rate
        );

        // Record a miss.
        metrics.record_cache_result(false, 0);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
        assert_eq!(metrics.total_tokens_saved, 500);
        assert!(
            (metrics.cache_hit_rate - 0.5).abs() < 1e-9,
            "expected 0.5, got {}",
            metrics.cache_hit_rate
        );
    }

    #[test]
    fn test_record_summarization() {
        let mut metrics = OptimizationMetrics::new();
        metrics.record_summarization(1000, 600);
        assert_eq!(metrics.total_input_tokens, 1000);
        assert_eq!(metrics.summarized_input_tokens, 600);
        assert_eq!(metrics.tokens_saved_by_summarization, 400);
        assert_eq!(metrics.total_tokens_saved, 400);

        // Second summarization accumulates.
        metrics.record_summarization(500, 400);
        assert_eq!(metrics.total_input_tokens, 1500);
        assert_eq!(metrics.summarized_input_tokens, 1000);
        assert_eq!(metrics.tokens_saved_by_summarization, 500);
        assert_eq!(metrics.total_tokens_saved, 500);
    }

    #[test]
    fn test_record_model_call_all_tiers() {
        let mut metrics = OptimizationMetrics::new();

        metrics.record_model_call(ExecutionTier::Musician);
        assert_eq!(metrics.musician_calls, 1);
        assert_eq!(metrics.total_model_calls(), 1);

        metrics.record_model_call(ExecutionTier::Editor);
        assert_eq!(metrics.editor_calls, 1);
        assert_eq!(metrics.total_model_calls(), 2);

        metrics.record_model_call(ExecutionTier::Composer);
        assert_eq!(metrics.composer_calls, 1);
        assert_eq!(metrics.total_model_calls(), 3);

        // Thinking tier is tracked under Composer.
        metrics.record_model_call(ExecutionTier::Thinking);
        assert_eq!(metrics.composer_calls, 2);
        assert_eq!(metrics.total_model_calls(), 4);
    }

    #[test]
    fn test_total_model_calls() {
        let mut metrics = OptimizationMetrics::new();
        assert_eq!(metrics.total_model_calls(), 0);

        metrics.musician_calls = 5;
        metrics.editor_calls = 3;
        metrics.composer_calls = 2;
        assert_eq!(metrics.total_model_calls(), 10);
    }

    #[test]
    fn test_report_formatting() {
        let mut metrics = OptimizationMetrics::new();
        metrics.record_execution(1000, 400);
        metrics.record_cache_result(true, 200);
        metrics.record_cache_result(false, 0);
        metrics.record_summarization(500, 300);
        metrics.record_model_call(ExecutionTier::Musician);
        metrics.record_model_call(ExecutionTier::Composer);

        let report = metrics.report();
        assert!(report.contains("Optimization Metrics Report"));
        assert!(report.contains("Sequential baseline: 1000ms"));
        assert!(report.contains("Parallel actual:     400ms"));
        assert!(report.contains("Time saved:          60.0%"));
        assert!(report.contains("Total input tokens:        500"));
        assert!(report.contains("Saved by summarization:    200"));
        assert!(report.contains("Cache hit tokens:          200"));
        assert!(report.contains("Total tokens saved:        400"));
        assert!(report.contains("Hits:   1"));
        assert!(report.contains("Misses: 1"));
        assert!(report.contains("Hit rate: 50.0%"));
        assert!(report.contains("Musician calls: 1"));
        assert!(report.contains("Composer calls: 1"));
        assert!(report.contains("Total calls:    2"));
    }
}
