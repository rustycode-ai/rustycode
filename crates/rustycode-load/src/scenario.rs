//! Load test scenario configuration

use crate::error::{LoadTestError, Result};
use crate::ramp_up::RampUpStrategy;
use crate::request::LoadRequest;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Response time thresholds for SLA validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTimeThreshold {
    /// p50 (median) threshold
    pub p50: Duration,

    /// p90 threshold
    pub p90: Duration,

    /// p95 threshold
    pub p95: Duration,

    /// p99 threshold
    pub p99: Duration,
}

impl Default for ResponseTimeThreshold {
    fn default() -> Self {
        Self {
            p50: Duration::from_millis(100),
            p90: Duration::from_millis(200),
            p95: Duration::from_millis(300),
            p99: Duration::from_millis(500),
        }
    }
}

/// Generator function for creating load test requests
pub type RequestGenerator = Arc<dyn Fn(usize) -> LoadRequest + Send + Sync>;

/// A load test scenario
#[derive(Clone)]
pub struct LoadScenario {
    /// Scenario name
    pub name: String,

    /// Scenario description
    pub description: Option<String>,

    /// Number of concurrent users
    pub concurrent_users: usize,

    /// Test duration
    pub duration: Duration,

    /// Ramp-up strategy
    pub ramp_up: RampUpStrategy,

    /// Request generator function
    pub request_generator: RequestGenerator,

    /// Think time between requests (per user)
    pub think_time: Duration,

    /// Request timeout
    pub request_timeout: Duration,

    /// Response time thresholds (optional)
    pub response_time_threshold: Option<ResponseTimeThreshold>,

    /// Maximum error rate for SLA (0.0 to 1.0)
    pub max_error_rate: f64,

    /// Whether to stop on first error
    pub stop_on_error: bool,

    /// Tags for grouping/filtering scenarios
    pub tags: Vec<String>,
}

impl LoadScenario {
    /// Create a new scenario builder
    pub fn builder() -> LoadScenarioBuilder {
        LoadScenarioBuilder::new()
    }

    /// Validate the scenario configuration
    pub fn validate(&self) -> Result<()> {
        if self.concurrent_users == 0 {
            return Err(LoadTestError::InvalidScenario(
                "concurrent_users must be greater than 0".to_string(),
            ));
        }

        if self.duration.is_zero() {
            return Err(LoadTestError::InvalidScenario(
                "duration must be greater than 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.max_error_rate) {
            return Err(LoadTestError::InvalidScenario(
                "max_error_rate must be between 0.0 and 1.0".to_string(),
            ));
        }

        if self.think_time.is_zero() {
            return Err(LoadTestError::InvalidScenario(
                "think_time must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculate total number of requests to execute
    #[allow(clippy::cast_precision_loss)]
    pub fn estimate_total_requests(&self) -> usize {
        let think_ms = self.think_time.as_millis() as f64;
        if think_ms <= 0.0 {
            return 0;
        }
        let requests_per_second = 1000.0 / think_ms;
        let total_seconds = self.duration.as_secs_f64();
        let total = requests_per_second * total_seconds * self.concurrent_users as f64;
        if total >= usize::MAX as f64 {
            usize::MAX
        } else {
            total as usize
        }
    }
}

/// Builder for creating `LoadScenario` instances
pub struct LoadScenarioBuilder {
    name: Option<String>,
    description: Option<String>,
    concurrent_users: Option<usize>,
    duration: Option<Duration>,
    ramp_up: Option<RampUpStrategy>,
    request_generator: Option<RequestGenerator>,
    think_time: Option<Duration>,
    request_timeout: Option<Duration>,
    response_time_threshold: Option<ResponseTimeThreshold>,
    max_error_rate: Option<f64>,
    stop_on_error: Option<bool>,
    tags: Vec<String>,
}

impl LoadScenarioBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            name: None,
            description: None,
            concurrent_users: None,
            duration: None,
            ramp_up: None,
            request_generator: None,
            think_time: None,
            request_timeout: None,
            response_time_threshold: None,
            max_error_rate: None,
            stop_on_error: None,
            tags: Vec::new(),
        }
    }

    /// Set the scenario name
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the scenario description
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the number of concurrent users
    pub const fn concurrent_users(mut self, users: usize) -> Self {
        self.concurrent_users = Some(users);
        self
    }

    /// Set the test duration
    pub const fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// Set the ramp-up strategy
    pub const fn ramp_up(mut self, strategy: RampUpStrategy) -> Self {
        self.ramp_up = Some(strategy);
        self
    }

    /// Set the request generator function
    pub fn request_generator<F>(mut self, generator: F) -> Self
    where
        F: Fn(usize) -> LoadRequest + Send + Sync + 'static,
    {
        self.request_generator = Some(Arc::new(generator));
        self
    }

    /// Set the think time between requests
    pub const fn think_time(mut self, duration: Duration) -> Self {
        self.think_time = Some(duration);
        self
    }

    /// Set the request timeout
    pub const fn request_timeout(mut self, duration: Duration) -> Self {
        self.request_timeout = Some(duration);
        self
    }

    /// Set the response time thresholds
    pub const fn response_time_threshold(mut self, threshold: ResponseTimeThreshold) -> Self {
        self.response_time_threshold = Some(threshold);
        self
    }

    /// Set the maximum error rate
    pub const fn max_error_rate(mut self, rate: f64) -> Self {
        self.max_error_rate = Some(rate);
        self
    }

    /// Set whether to stop on first error
    pub const fn stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = Some(stop);
        self
    }

    /// Add a tag
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Build the scenario
    pub fn build(self) -> Result<LoadScenario> {
        let name = self
            .name
            .ok_or_else(|| LoadTestError::InvalidScenario("name is required".to_string()))?;

        let concurrent_users = self
            .concurrent_users
            .unwrap_or(crate::defaults::DEFAULT_CONCURRENT_USERS);

        let duration = self
            .duration
            .ok_or_else(|| LoadTestError::InvalidScenario("duration is required".to_string()))?;

        let request_generator = self.request_generator.ok_or_else(|| {
            LoadTestError::InvalidScenario("request_generator is required".to_string())
        })?;

        let scenario = LoadScenario {
            name,
            description: self.description,
            concurrent_users,
            duration,
            ramp_up: self.ramp_up.unwrap_or_default(),
            request_generator,
            think_time: self
                .think_time
                .unwrap_or(crate::defaults::DEFAULT_THINK_TIME),
            request_timeout: self
                .request_timeout
                .unwrap_or(crate::defaults::DEFAULT_REQUEST_TIMEOUT),
            response_time_threshold: self.response_time_threshold,
            max_error_rate: self
                .max_error_rate
                .unwrap_or(crate::defaults::DEFAULT_MAX_ERROR_RATE),
            stop_on_error: self.stop_on_error.unwrap_or(false),
            tags: self.tags,
        };

        scenario.validate()?;
        Ok(scenario)
    }
}

impl Default for LoadScenarioBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_builder() {
        let scenario = LoadScenario::builder()
            .name("Test Scenario")
            .concurrent_users(10)
            .duration(Duration::from_mins(1))
            .request_generator(|user_id| {
                LoadRequest::http_get(format!("https://example.com/{user_id}"))
            })
            .build()
            .unwrap();

        assert_eq!(scenario.name, "Test Scenario");
        assert_eq!(scenario.concurrent_users, 10);
        assert_eq!(scenario.duration, Duration::from_mins(1));
    }

    #[test]
    fn test_scenario_validation() {
        // Invalid: zero concurrent users
        let result = LoadScenario::builder()
            .name("Invalid")
            .concurrent_users(0)
            .duration(Duration::from_mins(1))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();

        assert!(result.is_err());

        // Invalid: zero duration
        let result = LoadScenario::builder()
            .name("Invalid")
            .concurrent_users(10)
            .duration(Duration::ZERO)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();

        assert!(result.is_err());

        // Invalid: error rate out of range
        let result = LoadScenario::builder()
            .name("Invalid")
            .concurrent_users(10)
            .duration(Duration::from_mins(1))
            .max_error_rate(1.5)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();

        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_defaults() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();

        assert_eq!(
            scenario.concurrent_users,
            crate::defaults::DEFAULT_CONCURRENT_USERS
        );
        assert_eq!(scenario.think_time, crate::defaults::DEFAULT_THINK_TIME);
        assert_eq!(
            scenario.request_timeout,
            crate::defaults::DEFAULT_REQUEST_TIMEOUT
        );
    }

    #[test]
    fn test_response_time_threshold_default() {
        let threshold = ResponseTimeThreshold::default();
        assert_eq!(threshold.p50, Duration::from_millis(100));
        assert_eq!(threshold.p90, Duration::from_millis(200));
        assert_eq!(threshold.p95, Duration::from_millis(300));
        assert_eq!(threshold.p99, Duration::from_millis(500));
    }

    #[test]
    fn test_estimate_total_requests() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .concurrent_users(10)
            .duration(Duration::from_mins(1))
            .think_time(Duration::from_millis(100))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();

        let estimate = scenario.estimate_total_requests();
        assert!(estimate > 0);
        assert_eq!(estimate, 6000); // 10 users * 60 seconds * 10 requests/second
    }

    #[test]
    fn test_scenario_builder_with_description() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .description("A test scenario")
            .duration(Duration::from_secs(30))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert_eq!(scenario.description, Some("A test scenario".to_string()));
    }

    #[test]
    fn test_scenario_builder_without_description() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_secs(30))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert!(scenario.description.is_none());
    }

    #[test]
    fn test_scenario_builder_with_tags() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_secs(30))
            .tag("smoke")
            .tag("api")
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert_eq!(scenario.tags, vec!["smoke", "api"]);
    }

    #[test]
    fn test_scenario_builder_with_ramp_up() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .ramp_up(RampUpStrategy::Immediate)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert_eq!(scenario.ramp_up, RampUpStrategy::Immediate);
    }

    #[test]
    fn test_scenario_builder_with_stop_on_error() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .stop_on_error(true)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert!(scenario.stop_on_error);
    }

    #[test]
    fn test_scenario_builder_with_response_time_threshold() {
        let threshold = ResponseTimeThreshold {
            p50: Duration::from_millis(50),
            p90: Duration::from_millis(100),
            p95: Duration::from_millis(200),
            p99: Duration::from_millis(500),
        };
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .response_time_threshold(threshold)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert!(scenario.response_time_threshold.is_some());
        let t = scenario.response_time_threshold.unwrap();
        assert_eq!(t.p50, Duration::from_millis(50));
    }

    #[test]
    fn test_scenario_builder_missing_name() {
        let result = LoadScenario::builder()
            .duration(Duration::from_mins(1))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_builder_missing_duration() {
        let result = LoadScenario::builder()
            .name("Test")
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_builder_missing_request_generator() {
        let result = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_validate_zero_think_time() {
        let result = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .think_time(Duration::ZERO)
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_scenario_builder_default_max_error_rate() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert!((scenario.max_error_rate - 0.01).abs() < 0.001);
    }

    #[test]
    fn test_scenario_builder_default_stop_on_error() {
        let scenario = LoadScenario::builder()
            .name("Test")
            .duration(Duration::from_mins(1))
            .request_generator(|_| LoadRequest::http_get("https://example.com".to_string()))
            .build()
            .unwrap();
        assert!(!scenario.stop_on_error);
    }

    #[test]
    fn test_load_scenario_builder_default_impl() {
        let builder = LoadScenarioBuilder::default();
        assert!(builder.name.is_none());
        assert!(builder.tags.is_empty());
    }
}
