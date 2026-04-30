#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! `RustyCode` Load Testing Framework
//!
//! A comprehensive, high-performance load testing framework for testing system behavior under load.
//!
//! ## Features
//!
//! - **Flexible Scenario Definition**: Define custom load test scenarios with configurable parameters
//! - **Concurrent Request Generation**: Generate load with thousands of concurrent requests
//! - **Ramp-Up Strategies**: Linear, stepped, and custom ramp-up patterns
//! - **Response Time Tracking**: High-precision timing with microsecond granularity
//! - **Percentile Calculation**: Accurate p50, p90, p95, p99, and p999 calculations
//! - **Report Generation**: JSON, terminal, and HTML output formats
//! - **Real-Time Monitoring**: Track progress during test execution
//! - **Error Classification**: Categorize failures by type and frequency
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use rustycode_load::*;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Define a simple load test
//!     let scenario = LoadScenario::builder()
//!         .name("API Load Test")
//!         .concurrent_users(100)
//!         .duration(Duration::from_mins(1))
//!         .ramp_up(RampUpStrategy::Linear {
//!             duration: Duration::from_secs(10),
//!         })
//!         .request_generator(|user_id| {
//!             // Generate requests for each user
//!             LoadRequest::http(
//!                 format!("https://api.example.com/data/{}", user_id),
//!                 reqwest::Method::GET,
//!             )
//!         })
//!         .build()?;
//!
//!     // Run the load test
//!     let runner = LoadTestRunner::new();
//!     let results = runner.run(scenario).await?;
//!
//!     // Print results
//!     results.print_summary();
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! The framework is organized into several modules:
//!
//! - **scenario**: Define load test scenarios with configurable parameters
//! - **executor**: Execute load tests with concurrent request generation
//! - **metrics**: Collect and analyze performance metrics
//! - **report**: Generate reports in multiple formats
//! - **`ramp_up`**: Configure ramp-up strategies
//! - **request**: Define custom request types
//! - **error**: Error handling and classification
//!
//! ## Advanced Usage
//!
//! ### Custom Request Types
//!
//! ```ignore
//! use rustycode_load::*;
//!
//! let custom_request = LoadRequest::custom(|context| async move {
//!     // Execute custom logic
//!     let start = std::time::Instant::now();
//!     // ... perform operation ...
//!     let duration = start.elapsed();
//!
//!     Ok(LoadResult::success(duration))
//! });
//! ```
//!
//! ### Ramp-Up Strategies
//!
//! ```ignore
//! use rustycode_load::*;
//! use std::time::Duration;
//!
//! // Linear ramp-up
//! let linear = RampUpStrategy::Linear {
//!     duration: Duration::from_secs(30),
//! };
//!
//! // Stepped ramp-up
//! let stepped = RampUpStrategy::Stepped {
//!     steps: 5,
//!     step_duration: Duration::from_secs(10),
//! };
//!
//! // Immediate (no ramp-up)
//! let immediate = RampUpStrategy::Immediate;
//! ```
//!
//! ### Response Time Thresholds
//!
//! ```ignore
//! use rustycode_load::*;
//!
//! let scenario = LoadScenario::builder()
//!     .name("SLA Test")
//!     .response_time_threshold(ResponseTimeThreshold {
//!         p50: Duration::from_millis(100),
//!         p90: Duration::from_millis(200),
//!         p95: Duration::from_millis(300),
//!         p99: Duration::from_millis(500),
//!     })
//!     .build()?;
//! ```
//!
//! ### Real-Time Monitoring
//!
//! ```ignore
//! use rustycode_load::*;
//! use tokio::sync::mpsc;
//!
//! let (progress_tx, mut progress_rx) = mpsc::channel(100);
//!
//! // Spawn progress monitoring task
//! tokio::spawn(async move {
//!     while let Some(progress) = progress_rx.recv().await {
//!         println!("Progress: {:.1}%", progress.percent_complete());
//!     }
//! });
//!
//! let runner = LoadTestRunner::new()
//!     .with_progress_channel(progress_tx);
//! ```

pub mod error;
pub mod executor;
pub mod metrics;
pub mod ramp_up;
pub mod report;
pub mod request;
pub mod scenario;

// Re-exports for convenience
pub use error::{LoadTestError, Result};
pub use executor::{LoadTestConfig, LoadTestRunner};
pub use metrics::{ErrorMetrics, LoadTestResults, ResponseTimeMetrics, ThroughputMetrics};
pub use ramp_up::RampUpStrategy;
pub use report::{ReportFormat, ReportGenerator};
pub use request::{LoadRequest, LoadResult};
pub use scenario::{LoadScenario, LoadScenarioBuilder};

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default configuration values
pub mod defaults {
    use std::time::Duration;

    /// Default number of concurrent users
    pub const DEFAULT_CONCURRENT_USERS: usize = 10;

    /// Default test duration
    pub const DEFAULT_DURATION: Duration = Duration::from_mins(1);

    /// Default ramp-up duration
    pub const DEFAULT_RAMP_UP_DURATION: Duration = Duration::from_secs(10);

    /// Default request timeout
    pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

    /// Default think time between requests
    pub const DEFAULT_THINK_TIME: Duration = Duration::from_millis(100);

    /// Default metrics collection interval
    pub const DEFAULT_METRICS_INTERVAL: Duration = Duration::from_secs(1);

    /// Default maximum error rate for SLA
    pub const DEFAULT_MAX_ERROR_RATE: f64 = 0.01; // 1%
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_defaults() {
        assert_eq!(defaults::DEFAULT_CONCURRENT_USERS, 10);
        assert_eq!(
            defaults::DEFAULT_DURATION,
            std::time::Duration::from_mins(1)
        );
    }
}
