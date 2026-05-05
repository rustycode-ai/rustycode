use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Counter metric for monotonically increasing values
/// Uses `Arc<AtomicU64>` for lock-free counting
#[derive(Clone)]
pub struct Counter {
    value: Arc<AtomicU64>,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Increment the counter by 1
    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the counter by a specific amount
    pub fn inc_by(&self, amount: u64) {
        self.value.fetch_add(amount, Ordering::Relaxed);
    }

    /// Get the current value
    pub fn value(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Reset the counter to 0
    pub fn reset(&self) {
        self.value.store(0, Ordering::Relaxed);
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// Gauge metric for point-in-time values
/// Uses `Arc<RwLock<f64>>` for thread-safe mutations
#[derive(Clone)]
pub struct Gauge {
    value: Arc<RwLock<f64>>,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            value: Arc::new(RwLock::new(0.0)),
        }
    }

    /// Set the gauge to a specific value
    pub fn set(&self, value: f64) {
        *self.value.write() = value;
    }

    /// Get the current value
    pub fn value(&self) -> f64 {
        *self.value.read()
    }

    /// Increment the gauge by a delta
    pub fn inc_by(&self, delta: f64) {
        *self.value.write() += delta;
    }

    /// Decrement the gauge by a delta
    pub fn dec_by(&self, delta: f64) {
        *self.value.write() -= delta;
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

/// Histogram for recording and analyzing distributions
/// Stores values in a circular buffer with configurable size
#[derive(Clone)]
pub struct Histogram {
    values: Arc<RwLock<VecDeque<f64>>>,
    max_size: usize,
}

impl Histogram {
    pub fn new(max_size: usize) -> Self {
        Self {
            values: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
        }
    }

    /// Record a value in the histogram
    pub fn record(&self, value: f64) {
        let mut values = self.values.write();
        if values.len() >= self.max_size {
            values.pop_front();
        }
        values.push_back(value);
    }

    /// Get statistics about the recorded values
    pub fn stats(&self) -> HistogramStats {
        let values = self.values.read();

        if values.is_empty() {
            return HistogramStats {
                count: 0,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                p50: 0.0,
                p95: 0.0,
                p99: 0.0,
            };
        }

        let count = values.len() as u64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let sum: f64 = values.iter().sum();
        let mean = sum / count as f64;

        // Calculate percentiles
        let mut sorted: Vec<f64> = values.iter().copied().collect();
        sorted.sort_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p50 = percentile(&sorted, 50.0);
        let p95 = percentile(&sorted, 95.0);
        let p99 = percentile(&sorted, 99.0);

        HistogramStats {
            count,
            min,
            max,
            mean,
            p50,
            p95,
            p99,
        }
    }

    /// Get the number of recorded values
    pub fn count(&self) -> usize {
        self.values.read().len()
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Statistics calculated from a histogram
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HistogramStats {
    pub count: u64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

/// Calculate a percentile from a sorted array
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }

    let max_idx = sorted.len() - 1;
    let index = (p / 100.0) * max_idx as f64;
    // Clamp to valid range to guard against NaN or out-of-range p values
    let lower = (index.floor().max(0.0) as usize).min(max_idx);
    let upper = (index.ceil().max(0.0) as usize).min(max_idx);

    if lower == upper {
        sorted[lower]
    } else {
        let weight = index - lower as f64;
        sorted[lower].mul_add(1.0 - weight, sorted[upper] * weight)
    }
}

/// Metrics for a single session
/// Aggregates counters, gauges, and histograms for various metrics
#[derive(Clone)]
pub struct SessionMetrics {
    // Counters
    pub total_tokens: Counter,
    pub total_tasks: Counter,
    pub total_errors: Counter,
    pub completed_tasks: Counter,

    // Gauges
    pub active_tasks: Gauge,
    pub last_error_time: Gauge,

    // Histograms
    pub execution_times: Histogram,
    pub tokens_per_task: Histogram,
    pub error_recovery_times: Histogram,

    // Session timing
    start_time: std::time::Instant,
}

impl SessionMetrics {
    pub fn new() -> Self {
        Self {
            total_tokens: Counter::new(),
            total_tasks: Counter::new(),
            total_errors: Counter::new(),
            completed_tasks: Counter::new(),
            active_tasks: Gauge::new(),
            last_error_time: Gauge::new(),
            execution_times: Histogram::new(10000),
            tokens_per_task: Histogram::new(10000),
            error_recovery_times: Histogram::new(1000),
            start_time: std::time::Instant::now(),
        }
    }

    /// Get elapsed time in seconds since session start
    pub fn elapsed_secs(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    /// Calculate tokens per second
    pub fn tokens_per_sec(&self) -> f64 {
        let elapsed = self.elapsed_secs();
        if elapsed > 0.0 {
            self.total_tokens.value() as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Record a task execution
    pub fn record_task(&self, tokens: u64, duration_secs: f64) {
        self.total_tasks.inc();
        self.total_tokens.inc_by(tokens);
        self.execution_times.record(duration_secs);
        self.tokens_per_task.record(tokens as f64);
    }

    /// Record an error
    pub fn record_error(&self) {
        self.total_errors.inc();
    }

    /// Record a task completion
    pub fn record_completion(&self) {
        self.completed_tasks.inc();
    }

    /// Set the number of active tasks
    pub fn set_active_tasks(&self, count: u64) {
        self.active_tasks.set(count as f64);
    }

    /// Get execution time statistics
    pub fn execution_time_stats(&self) -> HistogramStats {
        self.execution_times.stats()
    }

    /// Get tokens per task statistics
    pub fn tokens_per_task_stats(&self) -> HistogramStats {
        self.tokens_per_task.stats()
    }

    /// Get error recovery time statistics
    pub fn error_recovery_time_stats(&self) -> HistogramStats {
        self.error_recovery_times.stats()
    }
}

impl Default for SessionMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increment() {
        let counter = Counter::new();
        assert_eq!(counter.value(), 0);

        counter.inc();
        assert_eq!(counter.value(), 1);

        counter.inc();
        assert_eq!(counter.value(), 2);
    }

    #[test]
    fn test_counter_increment_by() {
        let counter = Counter::new();
        counter.inc_by(5);
        assert_eq!(counter.value(), 5);

        counter.inc_by(10);
        assert_eq!(counter.value(), 15);
    }

    #[test]
    fn test_counter_reset() {
        let counter = Counter::new();
        counter.inc_by(100);
        assert_eq!(counter.value(), 100);

        counter.reset();
        assert_eq!(counter.value(), 0);
    }

    #[test]
    fn test_counter_clone() {
        let counter1 = Counter::new();
        counter1.inc_by(5);

        let counter2 = counter1.clone();
        assert_eq!(counter2.value(), 5);

        counter2.inc();
        assert_eq!(counter1.value(), 6);
    }

    #[test]
    fn test_gauge_set() {
        let gauge = Gauge::new();
        assert_eq!(gauge.value(), 0.0);

        gauge.set(42.5);
        assert_eq!(gauge.value(), 42.5);

        gauge.set(-10.0);
        assert_eq!(gauge.value(), -10.0);
    }

    #[test]
    fn test_gauge_inc_by() {
        let gauge = Gauge::new();
        gauge.set(10.0);

        gauge.inc_by(5.5);
        assert_eq!(gauge.value(), 15.5);

        gauge.inc_by(-3.0);
        assert_eq!(gauge.value(), 12.5);
    }

    #[test]
    fn test_gauge_dec_by() {
        let gauge = Gauge::new();
        gauge.set(20.0);

        gauge.dec_by(5.0);
        assert_eq!(gauge.value(), 15.0);

        gauge.dec_by(10.0);
        assert_eq!(gauge.value(), 5.0);
    }

    #[test]
    fn test_gauge_clone() {
        let gauge1 = Gauge::new();
        gauge1.set(42.0);

        let gauge2 = gauge1.clone();
        assert_eq!(gauge2.value(), 42.0);

        gauge2.set(100.0);
        assert_eq!(gauge1.value(), 100.0);
    }

    #[test]
    fn test_histogram_record() {
        let histogram = Histogram::new(100);
        assert_eq!(histogram.count(), 0);

        histogram.record(10.0);
        assert_eq!(histogram.count(), 1);

        histogram.record(20.0);
        histogram.record(30.0);
        assert_eq!(histogram.count(), 3);
    }

    #[test]
    fn test_histogram_max_size() {
        let histogram = Histogram::new(3);

        histogram.record(1.0);
        histogram.record(2.0);
        histogram.record(3.0);
        assert_eq!(histogram.count(), 3);

        histogram.record(4.0);
        assert_eq!(histogram.count(), 3);

        let stats = histogram.stats();
        assert_eq!(stats.min, 2.0);
        assert_eq!(stats.max, 4.0);
    }

    #[test]
    fn test_histogram_stats_empty() {
        let histogram: Histogram = Histogram::new(100);
        let stats = histogram.stats();

        assert_eq!(stats.count, 0);
        assert_eq!(stats.min, 0.0);
        assert_eq!(stats.max, 0.0);
        assert_eq!(stats.mean, 0.0);
    }

    #[test]
    fn test_histogram_stats_single() {
        let histogram = Histogram::new(100);
        histogram.record(42.0);

        let stats = histogram.stats();
        assert_eq!(stats.count, 1);
        assert_eq!(stats.min, 42.0);
        assert_eq!(stats.max, 42.0);
        assert_eq!(stats.mean, 42.0);
        assert_eq!(stats.p50, 42.0);
        assert_eq!(stats.p95, 42.0);
        assert_eq!(stats.p99, 42.0);
    }

    #[test]
    fn test_histogram_percentiles() {
        let histogram = Histogram::new(100);
        for i in 1..=100 {
            histogram.record(i as f64);
        }

        let stats = histogram.stats();
        assert_eq!(stats.count, 100);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max, 100.0);
        assert_eq!(stats.mean, 50.5);

        // Verify percentiles are in expected ranges
        assert!(stats.p50 >= 45.0 && stats.p50 <= 55.0);
        assert!(stats.p95 >= 90.0 && stats.p95 <= 100.0);
        assert!(stats.p99 >= 98.0 && stats.p99 <= 100.0);
    }

    #[test]
    fn test_histogram_clone() {
        let histogram1 = Histogram::new(100);
        histogram1.record(10.0);
        histogram1.record(20.0);

        let histogram2 = histogram1.clone();
        assert_eq!(histogram2.count(), 2);

        histogram2.record(30.0);
        assert_eq!(histogram1.count(), 3);
    }

    #[test]
    fn test_session_metrics_creation() {
        let metrics = SessionMetrics::new();

        assert_eq!(metrics.total_tokens.value(), 0);
        assert_eq!(metrics.total_tasks.value(), 0);
        assert_eq!(metrics.total_errors.value(), 0);
        assert_eq!(metrics.completed_tasks.value(), 0);
        assert_eq!(metrics.active_tasks.value(), 0.0);
    }

    #[test]
    fn test_session_metrics_record_task() {
        let metrics = SessionMetrics::new();

        metrics.record_task(100, 0.5);

        assert_eq!(metrics.total_tokens.value(), 100);
        assert_eq!(metrics.total_tasks.value(), 1);
        assert_eq!(metrics.execution_times.count(), 1);
        assert_eq!(metrics.tokens_per_task.count(), 1);
    }

    #[test]
    fn test_session_metrics_record_error() {
        let metrics = SessionMetrics::new();

        metrics.record_error();
        assert_eq!(metrics.total_errors.value(), 1);

        metrics.record_error();
        assert_eq!(metrics.total_errors.value(), 2);
    }

    #[test]
    fn test_session_metrics_record_completion() {
        let metrics = SessionMetrics::new();

        metrics.record_completion();
        assert_eq!(metrics.completed_tasks.value(), 1);

        metrics.record_completion();
        assert_eq!(metrics.completed_tasks.value(), 2);
    }

    #[test]
    fn test_session_metrics_active_tasks() {
        let metrics = SessionMetrics::new();

        metrics.set_active_tasks(5);
        assert_eq!(metrics.active_tasks.value(), 5.0);

        metrics.set_active_tasks(10);
        assert_eq!(metrics.active_tasks.value(), 10.0);
    }

    #[test]
    fn test_session_metrics_elapsed_secs() {
        let metrics = SessionMetrics::new();
        let elapsed = metrics.elapsed_secs();

        // Should be a small positive number since we just created it
        assert!(elapsed >= 0.0);
        assert!(elapsed < 1.0);
    }

    #[test]
    fn test_session_metrics_tokens_per_sec() {
        let metrics = SessionMetrics::new();
        metrics.record_task(100, 0.5);

        let tokens_per_sec = metrics.tokens_per_sec();
        // Should be > 0 since we have tokens and elapsed time
        assert!(tokens_per_sec > 0.0);
    }

    #[test]
    fn test_session_metrics_execution_time_stats() {
        let metrics = SessionMetrics::new();

        metrics.record_task(100, 0.5);
        metrics.record_task(200, 1.0);
        metrics.record_task(150, 0.7);

        let stats = metrics.execution_time_stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, 0.5);
        assert_eq!(stats.max, 1.0);
        assert!(stats.mean > 0.7 && stats.mean < 0.8);
    }

    #[test]
    fn test_session_metrics_tokens_per_task_stats() {
        let metrics = SessionMetrics::new();

        metrics.record_task(100, 0.5);
        metrics.record_task(200, 1.0);
        metrics.record_task(150, 0.7);

        let stats = metrics.tokens_per_task_stats();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min, 100.0);
        assert_eq!(stats.max, 200.0);
        assert!(stats.mean > 140.0 && stats.mean < 160.0);
    }

    #[test]
    fn test_session_metrics_clone() {
        let metrics1 = SessionMetrics::new();
        metrics1.record_task(100, 0.5);

        let metrics2 = metrics1.clone();
        assert_eq!(metrics2.total_tokens.value(), 100);

        metrics2.record_task(50, 0.3);
        assert_eq!(metrics1.total_tokens.value(), 150);
    }

    #[test]
    fn test_histogram_percentile_accuracy() {
        let histogram = Histogram::new(1000);

        // Record 0-99
        for i in 0..100 {
            histogram.record(i as f64);
        }

        let stats = histogram.stats();

        // With 0-99, p50 should be around 49-50
        assert!(stats.p50 >= 48.0 && stats.p50 <= 51.0);
        // p95 should be around 94-95
        assert!(stats.p95 >= 93.0 && stats.p95 <= 96.0);
        // p99 should be around 98-99
        assert!(stats.p99 >= 97.0 && stats.p99 <= 100.0);
    }
}
