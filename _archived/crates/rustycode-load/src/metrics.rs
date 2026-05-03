//! Metrics collection and analysis

use crate::error::ErrorCategory;
use crate::request::LoadResult;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;

/// Metrics collected during a load test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadTestResults {
    /// Scenario name
    pub scenario_name: String,

    /// Test start time
    pub start_time: DateTime<Utc>,

    /// Test end time
    pub end_time: DateTime<Utc>,

    /// Total test duration
    pub total_duration: Duration,

    /// Response time metrics
    pub response_times: ResponseTimeMetrics,

    /// Error metrics
    pub errors: ErrorMetrics,

    /// Throughput metrics
    pub throughput: ThroughputMetrics,

    /// Per-user metrics
    pub per_user_metrics: HashMap<usize, UserMetrics>,

    /// Time series data (samples over time)
    pub time_series: Vec<TimeSeriesSample>,
}

impl LoadTestResults {
    /// Create a new results instance
    pub fn new(scenario_name: String) -> Self {
        let now = Utc::now();
        Self {
            scenario_name,
            start_time: now,
            end_time: now,
            total_duration: Duration::ZERO,
            response_times: ResponseTimeMetrics::default(),
            errors: ErrorMetrics::default(),
            throughput: ThroughputMetrics::default(),
            per_user_metrics: HashMap::new(),
            time_series: Vec::new(),
        }
    }

    /// Add a result to the metrics
    pub fn add_result(&mut self, result: &LoadResult) {
        self.response_times.add_sample(result.duration);

        if result.success {
            self.throughput.successful_requests += 1;
        } else {
            self.throughput.failed_requests += 1;
            self.errors
                .add_error(&result.error.clone().unwrap_or_default());
        }

        self.per_user_metrics
            .entry(result.user_id)
            .or_default()
            .add_result(result);
    }

    /// Finalize the metrics (calculate aggregates)
    pub fn finalize(&mut self) {
        self.throughput.total_requests =
            self.throughput.successful_requests + self.throughput.failed_requests;
        self.throughput.error_rate = if self.throughput.total_requests > 0 {
            #[allow(clippy::cast_precision_loss)]
            let failed = self.throughput.failed_requests as f64;
            #[allow(clippy::cast_precision_loss)]
            let total = self.throughput.total_requests as f64;
            failed / total
        } else {
            0.0
        };
        self.throughput.throughput_per_second = if self.total_duration.is_zero() {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let total = self.throughput.total_requests as f64;
            total / self.total_duration.as_secs_f64()
        };
        self.response_times.finalize();
    }

    /// Print a summary of the results
    pub fn print_summary(&self) {
        println!("\n=== Load Test Results ===");
        println!("Scenario: {}", self.scenario_name);
        println!("Duration: {:?}", self.total_duration);
        println!("\n--- Response Times ---");
        println!("  Min: {:?}", self.response_times.min);
        println!("  Max: {:?}", self.response_times.max);
        println!("  Mean: {:?}", self.response_times.mean);
        println!("  Median (p50): {:?}", self.response_times.p50);
        println!("  p90: {:?}", self.response_times.p90);
        println!("  p95: {:?}", self.response_times.p95);
        println!("  p99: {:?}", self.response_times.p99);
        println!("  p999: {:?}", self.response_times.p999);
        println!("\n--- Throughput ---");
        println!("  Total Requests: {}", self.throughput.total_requests);
        println!("  Successful: {}", self.throughput.successful_requests);
        println!("  Failed: {}", self.throughput.failed_requests);
        println!("  Error Rate: {:.2}%", self.throughput.error_rate * 100.0);
        println!(
            "  Throughput: {:.2} req/s",
            self.throughput.throughput_per_second
        );
        println!("\n--- Errors ---");
        println!("  Total Errors: {}", self.errors.total_errors);
        for (category, count) in &self.errors.by_category {
            println!("  {}: {}", category.name(), count);
        }
    }
}

/// Response time metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTimeMetrics {
    /// Minimum response time
    pub min: Duration,

    /// Maximum response time
    pub max: Duration,

    /// Mean response time
    pub mean: Duration,

    /// Median (p50)
    pub p50: Duration,

    /// 90th percentile
    pub p90: Duration,

    /// 95th percentile
    pub p95: Duration,

    /// 99th percentile
    pub p99: Duration,

    /// 99.9th percentile
    pub p999: Duration,

    /// Standard deviation
    pub std_dev: Duration,

    /// All samples (for percentile calculation)
    samples: Vec<Duration>,
}

impl Default for ResponseTimeMetrics {
    fn default() -> Self {
        Self {
            min: Duration::MAX,
            max: Duration::ZERO,
            mean: Duration::ZERO,
            p50: Duration::ZERO,
            p90: Duration::ZERO,
            p95: Duration::ZERO,
            p99: Duration::ZERO,
            p999: Duration::ZERO,
            std_dev: Duration::ZERO,
            samples: Vec::new(),
        }
    }
}

impl ResponseTimeMetrics {
    /// Add a response time sample
    pub fn add_sample(&mut self, duration: Duration) {
        self.min = std::cmp::min(self.min, duration);
        self.max = std::cmp::max(self.max, duration);
        self.samples.push(duration);
    }

    /// Calculate final metrics
    pub fn finalize(&mut self) {
        if self.samples.is_empty() {
            return;
        }

        self.samples.sort();

        let count = self.samples.len();
        let sum: Duration = self.samples.iter().sum();
        if count > 0 {
            // Use nanosecond arithmetic to avoid Duration / u32 panic on large counts
            let mean_ns = (sum.as_nanos() as u64) / count as u64;
            self.mean = Duration::from_nanos(mean_ns);
        }

        // Calculate percentiles
        self.p50 = self.percentile(0.50);
        self.p90 = self.percentile(0.90);
        self.p95 = self.percentile(0.95);
        self.p99 = self.percentile(0.99);
        self.p999 = self.percentile(0.999);

        // Calculate standard deviation
        #[allow(clippy::cast_precision_loss)]
        let mean_ns = self.mean.as_nanos() as f64;
        #[allow(clippy::cast_precision_loss)]
        let count_f64 = count as f64;
        let variance = self
            .samples
            .iter()
            .map(|&d| {
                #[allow(clippy::cast_precision_loss)]
                let diff = d.as_nanos() as f64 - mean_ns;
                diff * diff
            })
            .sum::<f64>()
            / count_f64;
        self.std_dev = Duration::from_nanos(variance.sqrt() as u64);
    }

    /// Calculate a percentile
    fn percentile(&self, p: f64) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }

        let max_idx = self.samples.len() - 1;
        #[allow(clippy::cast_precision_loss)]
        let index = ((max_idx as f64) * p).round() as usize;
        self.samples[index.min(max_idx)]
    }
}

/// Error metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ErrorMetrics {
    /// Total number of errors
    pub total_errors: usize,

    /// Errors by category
    pub by_category: HashMap<ErrorCategory, usize>,

    /// Errors by message
    pub by_message: HashMap<String, usize>,
}

impl ErrorMetrics {
    /// Add an error
    pub fn add_error(&mut self, error: &str) {
        self.total_errors = self.total_errors.saturating_add(1);

        let category = Self::categorize_error(error);
        *self.by_category.entry(category).or_insert(0) += 1;

        *self.by_message.entry(error.to_string()).or_insert(0) += 1;
    }

    /// Categorize an error message
    fn categorize_error(error: &str) -> ErrorCategory {
        let error_lower = error.to_lowercase();

        if error_lower.contains("timeout") || error_lower.contains("timed out") {
            ErrorCategory::Timeout
        } else if error_lower.contains("connection")
            || error_lower.contains("dns")
            || error_lower.contains("refused")
        {
            ErrorCategory::Network
        } else if error_lower.contains("http") || error_lower.contains("status") {
            ErrorCategory::Http
        } else if error_lower.contains("serialize") || error_lower.contains("parse") {
            ErrorCategory::Serialization
        } else {
            ErrorCategory::Other
        }
    }
}

/// Throughput metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Total requests
    pub total_requests: usize,

    /// Successful requests
    pub successful_requests: usize,

    /// Failed requests
    pub failed_requests: usize,

    /// Error rate (0.0 to 1.0)
    pub error_rate: f64,

    /// Requests per second
    pub throughput_per_second: f64,
}

impl Default for ThroughputMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            error_rate: 0.0,
            throughput_per_second: 0.0,
        }
    }
}

/// Metrics for a single user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMetrics {
    /// User ID
    pub user_id: usize,

    /// Total requests
    pub total_requests: usize,

    /// Successful requests
    pub successful_requests: usize,

    /// Failed requests
    pub failed_requests: usize,

    /// Average response time
    pub avg_response_time: Duration,

    /// Min response time
    pub min_response_time: Duration,

    /// Max response time
    pub max_response_time: Duration,
}

impl Default for UserMetrics {
    fn default() -> Self {
        Self {
            user_id: 0,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_response_time: Duration::ZERO,
            min_response_time: Duration::MAX,
            max_response_time: Duration::ZERO,
        }
    }
}

impl UserMetrics {
    /// Add a result
    pub fn add_result(&mut self, result: &LoadResult) {
        self.user_id = result.user_id;
        self.total_requests = self.total_requests.saturating_add(1);

        if result.success {
            self.successful_requests = self.successful_requests.saturating_add(1);
        } else {
            self.failed_requests = self.failed_requests.saturating_add(1);
        }

        self.min_response_time = std::cmp::min(self.min_response_time, result.duration);
        self.max_response_time = std::cmp::max(self.max_response_time, result.duration);

        // Update average (running average) using nanosecond arithmetic to
        // avoid panic on Duration * u32 overflow when request counts or
        // latencies are large.
        let count = self.total_requests as u64;
        let avg_ns = self.avg_response_time.as_nanos() as u64;
        let new_ns = result.duration.as_nanos() as u64;
        let total_ns = avg_ns.saturating_mul(count - 1).saturating_add(new_ns);
        self.avg_response_time = Duration::from_nanos(total_ns / count);
    }
}

/// A time series sample
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesSample {
    /// Timestamp
    pub timestamp: DateTime<Utc>,

    /// Requests per second at this point
    pub rps: f64,

    /// Average response time
    pub avg_response_time: Duration,

    /// Error rate
    pub error_rate: f64,

    /// Active users
    pub active_users: usize,
}

/// Channel for sending metrics
pub type MetricsSender = mpsc::UnboundedSender<LoadResult>;

/// Channel for receiving metrics
pub type MetricsReceiver = mpsc::UnboundedReceiver<LoadResult>;

/// Create a metrics channel
pub fn metrics_channel() -> (MetricsSender, MetricsReceiver) {
    mpsc::unbounded_channel()
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::uninlined_format_args)]
mod tests {
    use super::*;

    #[test]
    fn test_response_time_metrics() {
        let mut metrics = ResponseTimeMetrics::default();

        metrics.add_sample(Duration::from_millis(100));
        metrics.add_sample(Duration::from_millis(200));
        metrics.add_sample(Duration::from_millis(150));
        metrics.add_sample(Duration::from_millis(300));
        metrics.add_sample(Duration::from_millis(250));

        metrics.finalize();

        assert_eq!(metrics.min, Duration::from_millis(100));
        assert_eq!(metrics.max, Duration::from_millis(300));
        assert!(metrics.mean >= Duration::from_millis(190));
        assert!(metrics.mean <= Duration::from_millis(210));
    }

    #[test]
    fn test_error_metrics() {
        let mut metrics = ErrorMetrics::default();

        metrics.add_error("Connection refused");
        metrics.add_error("Connection refused");
        metrics.add_error("Request timed out");
        metrics.add_error("HTTP 500 Internal Server Error");

        assert_eq!(metrics.total_errors, 4);
        assert_eq!(metrics.by_category[&ErrorCategory::Network], 2);
        assert_eq!(metrics.by_category[&ErrorCategory::Timeout], 1);
        assert_eq!(metrics.by_category[&ErrorCategory::Http], 1);
    }

    #[test]
    fn test_user_metrics() {
        let mut metrics = UserMetrics::default();

        let result1 = LoadResult::success(Duration::from_millis(100)).with_user_id(1);
        let result2 = LoadResult::success(Duration::from_millis(200)).with_user_id(1);
        let result3 =
            LoadResult::error(Duration::from_millis(50), "Error".to_string()).with_user_id(1);

        metrics.add_result(&result1);
        metrics.add_result(&result2);
        metrics.add_result(&result3);

        assert_eq!(metrics.user_id, 1);
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.successful_requests, 2);
        assert_eq!(metrics.failed_requests, 1);
        assert_eq!(metrics.min_response_time, Duration::from_millis(50));
        assert_eq!(metrics.max_response_time, Duration::from_millis(200));
    }

    #[test]
    fn test_load_test_results() {
        let mut results = LoadTestResults::new("Test Scenario".to_string());

        for i in 0..10 {
            let result = LoadResult::success(Duration::from_millis(100 + i * 10))
                .with_user_id((i % 3) as usize);
            results.add_result(&result);
        }

        results.finalize();

        assert_eq!(results.throughput.total_requests, 10);
        assert_eq!(results.throughput.successful_requests, 10);
        assert_eq!(results.throughput.failed_requests, 0);
        assert_eq!(results.throughput.error_rate, 0.0);
    }

    #[test]
    fn test_percentiles() {
        let mut metrics = ResponseTimeMetrics::default();

        // Add 100 samples (1-100 to avoid confusion with 0-indexing)
        for i in 1..=100 {
            metrics.add_sample(Duration::from_millis(i));
        }

        metrics.finalize();

        // With 100 samples (values 1..=100, indices 0..=99):
        // p50: round(99 * 0.5) = 50 → samples[50] = 51ms
        // p90: round(99 * 0.9) = 89 → samples[89] = 90ms
        // p95: round(99 * 0.95) = 94 → samples[94] = 95ms
        // p99: round(99 * 0.99) = 98 → samples[98] = 99ms
        assert_eq!(metrics.p50, Duration::from_millis(51));
        assert_eq!(metrics.p90, Duration::from_millis(90));
        assert_eq!(metrics.p95, Duration::from_millis(95));
        assert_eq!(metrics.p99, Duration::from_millis(99));
    }

    #[test]
    fn test_response_time_metrics_default() {
        let metrics = ResponseTimeMetrics::default();
        assert_eq!(metrics.min, Duration::MAX);
        assert_eq!(metrics.max, Duration::ZERO);
        assert_eq!(metrics.mean, Duration::ZERO);
        assert_eq!(metrics.p50, Duration::ZERO);
        assert_eq!(metrics.std_dev, Duration::ZERO);
    }

    #[test]
    fn test_response_time_metrics_finalize_empty() {
        let mut metrics = ResponseTimeMetrics::default();
        metrics.finalize();
        // Should not panic; values stay default
        assert_eq!(metrics.mean, Duration::ZERO);
    }

    #[test]
    fn test_error_metrics_categorize_timeout() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("Request timed out");
        assert_eq!(metrics.by_category[&ErrorCategory::Timeout], 1);
    }

    #[test]
    fn test_error_metrics_categorize_network() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("dns resolution failed");
        assert_eq!(metrics.by_category[&ErrorCategory::Network], 1);
    }

    #[test]
    fn test_error_metrics_categorize_http() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("http status 500");
        assert_eq!(metrics.by_category[&ErrorCategory::Http], 1);
    }

    #[test]
    fn test_error_metrics_categorize_serialization() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("Failed to serialize body");
        assert_eq!(metrics.by_category[&ErrorCategory::Serialization], 1);
    }

    #[test]
    fn test_error_metrics_categorize_other() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("Unknown glitch");
        assert_eq!(metrics.by_category[&ErrorCategory::Other], 1);
    }

    #[test]
    fn test_error_metrics_by_message() {
        let mut metrics = ErrorMetrics::default();
        metrics.add_error("Connection refused");
        metrics.add_error("Connection refused");
        metrics.add_error("Timeout");
        assert_eq!(metrics.by_message["Connection refused"], 2);
        assert_eq!(metrics.by_message["Timeout"], 1);
    }

    #[test]
    fn test_throughput_metrics_default() {
        let metrics = ThroughputMetrics::default();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
        assert_eq!(metrics.error_rate, 0.0);
        assert_eq!(metrics.throughput_per_second, 0.0);
    }

    #[test]
    fn test_user_metrics_default() {
        let metrics = UserMetrics::default();
        assert_eq!(metrics.user_id, 0);
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.min_response_time, Duration::MAX);
        assert_eq!(metrics.max_response_time, Duration::ZERO);
    }

    #[test]
    fn test_load_test_results_new() {
        let results = LoadTestResults::new("MyTest".to_string());
        assert_eq!(results.scenario_name, "MyTest");
        assert_eq!(results.total_duration, Duration::ZERO);
        assert!(results.per_user_metrics.is_empty());
        assert!(results.time_series.is_empty());
    }

    #[test]
    fn test_load_test_results_add_success_and_error() {
        let mut results = LoadTestResults::new("Mixed".to_string());
        results.add_result(&LoadResult::success(Duration::from_millis(10)));
        results.add_result(&LoadResult::error(
            Duration::from_millis(5),
            "fail".to_string(),
        ));
        assert_eq!(results.throughput.successful_requests, 1);
        assert_eq!(results.throughput.failed_requests, 1);
        assert_eq!(results.errors.total_errors, 1);
    }

    #[test]
    fn test_load_test_results_per_user() {
        let mut results = LoadTestResults::new("PerUser".to_string());
        results.add_result(&LoadResult::success(Duration::from_millis(10)).with_user_id(1));
        results.add_result(&LoadResult::success(Duration::from_millis(20)).with_user_id(1));
        results.add_result(&LoadResult::success(Duration::from_millis(15)).with_user_id(2));
        assert_eq!(results.per_user_metrics.len(), 2);
        assert_eq!(results.per_user_metrics[&1].total_requests, 2);
        assert_eq!(results.per_user_metrics[&2].total_requests, 1);
    }

    #[test]
    fn test_load_test_results_finalize_error_rate() {
        let mut results = LoadTestResults::new("Rates".to_string());
        for _ in 0..8 {
            results.add_result(&LoadResult::success(Duration::from_millis(10)));
        }
        for _ in 0..2 {
            results.add_result(&LoadResult::error(
                Duration::from_millis(5),
                "err".to_string(),
            ));
        }
        results.total_duration = Duration::from_secs(10);
        results.finalize();
        assert_eq!(results.throughput.total_requests, 10);
        assert!((results.throughput.error_rate - 0.2).abs() < 0.001);
        assert!(results.throughput.throughput_per_second > 0.0);
    }

    #[test]
    fn test_response_time_serialization_roundtrip() {
        let mut metrics = ResponseTimeMetrics::default();
        metrics.add_sample(Duration::from_millis(50));
        metrics.add_sample(Duration::from_millis(150));
        metrics.finalize();
        let json = serde_json::to_string(&metrics).unwrap();
        let decoded: ResponseTimeMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.min, metrics.min);
        assert_eq!(decoded.max, metrics.max);
    }

    #[test]
    fn test_throughput_metrics_serialization_roundtrip() {
        let metrics = ThroughputMetrics {
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            error_rate: 0.05,
            throughput_per_second: 10.0,
        };
        let json = serde_json::to_string(&metrics).unwrap();
        let decoded: ThroughputMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_requests, 100);
        assert!((decoded.error_rate - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_time_series_sample_serialization() {
        let sample = TimeSeriesSample {
            timestamp: Utc::now(),
            rps: 42.5,
            avg_response_time: Duration::from_millis(123),
            error_rate: 0.02,
            active_users: 10,
        };
        let json = serde_json::to_string(&sample).unwrap();
        let decoded: TimeSeriesSample = serde_json::from_str(&json).unwrap();
        assert!((decoded.rps - 42.5).abs() < 0.001);
        assert_eq!(decoded.active_users, 10);
    }

    #[test]
    fn test_metrics_channel() {
        let (tx, _rx) = metrics_channel();
        assert!(tx
            .send(LoadResult::success(Duration::from_millis(1)))
            .is_ok());
        drop(tx);
    }

    #[test]
    fn test_user_metrics_running_average_does_not_overflow() {
        // Previously Duration * u32 could panic on overflow when count was large.
        // Now uses u64 nanosecond arithmetic with saturating_mul.
        let mut metrics = UserMetrics::default();
        // Add many results with a large latency to stress the running average
        for _ in 0..1000 {
            metrics.add_result(&LoadResult::success(Duration::from_secs(10)).with_user_id(1));
        }
        assert_eq!(metrics.total_requests, 1000);
        // Average should be ~10s (10_000ms)
        let avg_ms = metrics.avg_response_time.as_millis();
        assert!(
            (9_500..=10_500).contains(&avg_ms),
            "average should be ~10s, got {}ms",
            avg_ms
        );
    }

    #[test]
    fn test_response_time_metrics_finalize_large_sample_count() {
        // Verify that finalize() does not panic with many samples
        // (tests the Duration / count as u32 fix — uses nanosecond arithmetic)
        let mut metrics = ResponseTimeMetrics::default();
        for _ in 0..100_000 {
            metrics.add_sample(Duration::from_millis(10));
        }
        metrics.finalize();
        assert_eq!(metrics.min, Duration::from_millis(10));
        assert_eq!(metrics.max, Duration::from_millis(10));
        assert!(metrics.mean <= Duration::from_millis(11));
    }

    #[test]
    fn test_finalize_zero_requests_no_panic() {
        let mut results = LoadTestResults::new("Empty".to_string());
        results.total_duration = Duration::ZERO;
        results.finalize();
        assert_eq!(results.throughput.error_rate, 0.0);
        assert_eq!(results.throughput.throughput_per_second, 0.0);
    }

    #[test]
    fn test_finalize_zero_duration_no_panic() {
        let mut results = LoadTestResults::new("ZeroDur".to_string());
        results.add_result(&LoadResult::success(Duration::from_millis(10)));
        results.total_duration = Duration::ZERO;
        results.finalize();
        assert_eq!(results.throughput.error_rate, 0.0);
        assert_eq!(results.throughput.throughput_per_second, 0.0);
    }
}
