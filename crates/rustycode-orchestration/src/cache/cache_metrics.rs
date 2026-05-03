//! Cache hit/miss metrics for prompt caching.

// ─── CacheMetrics ────────────────────────────────────────────────────────────

/// Accumulates cache hit/miss statistics.
///
/// Thread-safety is the caller's responsibility (wrap in `Mutex` if needed).
#[derive(Debug, Clone)]
pub struct CacheMetrics {
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Total tokens saved by cache hits.
    pub total_tokens_saved: usize,
}

impl CacheMetrics {
    /// Create an empty metrics collector.
    pub const fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            total_tokens_saved: 0,
        }
    }

    /// Return the cache hit rate as a value between 0.0 and 1.0.
    ///
    /// Returns 0.0 when no hits or misses have been recorded.
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits.saturating_add(self.misses);
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }

    /// Reset all counters to zero.
    pub const fn reset(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.total_tokens_saved = 0;
    }
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_hit_rate_calculation() {
        let mut metrics = CacheMetrics::new();
        metrics.hits = 3;
        metrics.misses = 1;
        let rate = metrics.hit_rate();
        assert!((rate - 0.75).abs() < 0.001, "expected ~0.75, got {rate}");
    }

    #[test]
    fn test_hit_rate_zero_total() {
        let metrics = CacheMetrics::new();
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_reset_clears_all() {
        let mut metrics = CacheMetrics::new();
        metrics.hits = 10;
        metrics.misses = 5;
        metrics.total_tokens_saved = 1000;
        metrics.reset();
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.total_tokens_saved, 0);
    }
}
