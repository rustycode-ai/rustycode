//! Retry Policy with Exponential Backoff
//!
//! This module provides comprehensive retry functionality with:
//! - Multiple retry strategies (exponential backoff, linear, fixed)
//! - Configurable retry attempts and delays
//! - Jitter support for thundering herd prevention
//! - Retry condition customization
//! - Attempt tracking and statistics
//! - Integration with circuit breakers
//! - Maximum execution timeout

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Retry strategies
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryStrategy {
    FixedDelay,         // Constant delay between retries
    Linear,             // Linearly increasing delay
    ExponentialBackoff, // Exponential backoff with optional jitter
    Custom,             // Custom delay calculation
}

/// Jitter types for exponential backoff
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum JitterType {
    None,         // No jitter
    Full,         // Full jitter (0 to delay)
    Equal,        // Equal jitter (delay/2 ± delay/2)
    Decorrelated, // Decorrelated jitter
}

/// Retry outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RetryOutcome<T> {
    Success(T),
    Failed(String),
    Timeout,
    MaxAttemptsExceeded,
}

/// Retry attempt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAttempt {
    pub attempt_number: usize,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub will_retry: bool,
}

/// Retry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: usize,
    pub strategy: RetryStrategy,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub multiplier: f64,
    pub jitter_type: JitterType,
    pub jitter_factor: f64,
    pub max_total_duration_ms: Option<u64>,
    pub retryable_errors: Vec<String>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: RetryStrategy::ExponentialBackoff,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            multiplier: 2.0,
            jitter_type: JitterType::Full,
            jitter_factor: 0.5,
            max_total_duration_ms: None,
            retryable_errors: Vec::new(),
        }
    }
}

/// Retry statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStats {
    pub total_attempts: usize,
    pub successful_attempts: usize,
    pub failed_attempts: usize,
    pub total_retries: usize,
    pub average_delay_ms: f64,
    pub total_duration_ms: u64,
    pub last_attempt_at: Option<DateTime<Utc>>,
}

/// Retry policy
pub struct RetryPolicy {
    name: String,
    config: RetryConfig,
    attempts: Arc<RwLock<Vec<RetryAttempt>>>,
    stats: Arc<RwLock<RetryStats>>,
}

impl RetryPolicy {
    pub fn new(name: String, config: RetryConfig) -> Self {
        let stats = RetryStats {
            total_attempts: 0,
            successful_attempts: 0,
            failed_attempts: 0,
            total_retries: 0,
            average_delay_ms: 0.0,
            total_duration_ms: 0,
            last_attempt_at: None,
        };

        Self {
            name,
            config,
            attempts: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(stats)),
        }
    }

    /// Execute operation with retry logic
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> RetryOutcome<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut total_duration_ms = 0u64;

        for attempt in 0..self.config.max_attempts {
            let attempt_start = std::time::Instant::now();
            let attempt_started_at = Utc::now();

            // Check if we've exceeded total duration
            if let Some(max_duration) = self.config.max_total_duration_ms {
                if total_duration_ms >= max_duration {
                    self.record_attempt(
                        attempt,
                        attempt_started_at,
                        Utc::now(),
                        attempt_start.elapsed().as_millis() as u64,
                        Some("Max total duration exceeded".to_string()),
                        false,
                    )
                    .await;
                    return RetryOutcome::Timeout;
                }
            }

            // Execute the operation
            let result = operation().await;
            let duration = attempt_start.elapsed().as_millis() as u64;
            total_duration_ms += duration;

            match result {
                Ok(value) => {
                    self.record_attempt(
                        attempt,
                        attempt_started_at,
                        Utc::now(),
                        duration,
                        None,
                        false,
                    )
                    .await;
                    self.update_stats(true, attempt).await;
                    return RetryOutcome::Success(value);
                }
                Err(err) => {
                    let error_msg = format!("{}", err);
                    let should_retry = self.should_retry(&error_msg, attempt);

                    self.record_attempt(
                        attempt,
                        attempt_started_at,
                        Utc::now(),
                        duration,
                        Some(error_msg.clone()),
                        should_retry,
                    )
                    .await;

                    // Update stats for this attempt
                    self.update_stats(false, attempt).await;

                    if !should_retry {
                        // Check if we've reached max attempts
                        if attempt >= self.config.max_attempts - 1 {
                            return RetryOutcome::MaxAttemptsExceeded;
                        }
                        return RetryOutcome::Failed(error_msg);
                    }

                    // Calculate delay before next retry
                    let delay = self.calculate_delay(attempt).await;

                    // Only wait if this isn't the last attempt
                    if attempt < self.config.max_attempts - 1 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        // This should never be reached since we always return from within the loop
        RetryOutcome::MaxAttemptsExceeded
    }

    /// Calculate delay before next retry
    pub async fn calculate_delay(&self, attempt: usize) -> u64 {
        let base_delay = match self.config.strategy {
            RetryStrategy::FixedDelay => self.config.initial_delay_ms,
            RetryStrategy::Linear => self
                .config
                .initial_delay_ms
                .saturating_mul(attempt as u64 + 1),
            RetryStrategy::ExponentialBackoff => {
                let delay = self.config.initial_delay_ms as f64
                    * self.config.multiplier.powi(attempt as i32);
                delay as u64
            }
            RetryStrategy::Custom => self.config.initial_delay_ms,
        };

        // Apply max delay limit
        let delay = base_delay.min(self.config.max_delay_ms);

        // Apply jitter
        self.apply_jitter(delay).await
    }

    /// Apply jitter to delay
    async fn apply_jitter(&self, delay: u64) -> u64 {
        match self.config.jitter_type {
            JitterType::None => delay,
            JitterType::Full => {
                let random = rand::random::<f64>();
                (delay as f64 * random) as u64
            }
            JitterType::Equal => {
                let random = rand::random::<f64>();
                let half_delay = delay as f64 / 2.0;
                (half_delay + (half_delay * random)) as u64
            }
            JitterType::Decorrelated => {
                let random = rand::random::<f64>();
                let min_delay = (delay as f64 * 0.5) as u64;
                let max_delay = (delay as f64 * 1.5) as u64;
                let range = max_delay - min_delay;
                min_delay + (range as f64 * random) as u64
            }
        }
    }

    /// Check if error should trigger retry
    fn should_retry(&self, error: &str, attempt: usize) -> bool {
        // Check if we've reached max attempts
        if attempt >= self.config.max_attempts - 1 {
            return false;
        }

        // Check if error is retryable
        if !self.config.retryable_errors.is_empty() {
            return self
                .config
                .retryable_errors
                .iter()
                .any(|retryable| error.contains(retryable));
        }

        // Default retry logic - retry on common transient errors
        let retryable_patterns = [
            "timeout",
            "connection refused",
            "connection reset",
            "connection lost",
            "temporary failure",
            "try again",
            "unavailable",
            "rate limit",
        ];

        let error_lower = error.to_lowercase();
        retryable_patterns
            .iter()
            .any(|pattern| error_lower.contains(pattern))
    }

    /// Record retry attempt
    async fn record_attempt(
        &self,
        attempt_number: usize,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        duration_ms: u64,
        error: Option<String>,
        will_retry: bool,
    ) {
        let attempt_record = RetryAttempt {
            attempt_number,
            started_at,
            completed_at,
            duration_ms,
            error,
            will_retry,
        };

        let mut attempts = self.attempts.write().await;
        attempts.push(attempt_record);

        // Keep only last 100 attempts
        let len = attempts.len();
        if len > 100 {
            attempts.drain(0..len - 100);
        }
    }

    /// Update statistics
    async fn update_stats(&self, success: bool, attempt: usize) {
        let mut stats = self.stats.write().await;
        stats.total_attempts += 1;

        if success {
            stats.successful_attempts += 1;
        } else if attempt > 0 {
            // Only count failures for retries, not the initial attempt
            stats.failed_attempts += 1;
        }

        stats.last_attempt_at = Some(Utc::now());

        // Calculate average delay from attempts
        let attempts = self.attempts.read().await;
        if !attempts.is_empty() {
            let total_retries: usize = attempts.iter().map(|a| a.attempt_number).max().unwrap_or(0);
            stats.total_retries = total_retries;

            let total_delay: u64 = attempts
                .iter()
                .filter(|a| a.attempt_number > 0)
                .map(|a| {
                    if let Some(prev) = attempts.get(a.attempt_number - 1) {
                        (a.started_at - prev.completed_at)
                            .num_milliseconds()
                            .unsigned_abs()
                    } else {
                        0
                    }
                })
                .sum();

            let retry_count = attempts.iter().filter(|a| a.attempt_number > 0).count();
            if retry_count > 0 {
                stats.average_delay_ms = total_delay as f64 / retry_count as f64;
            }

            // Calculate total duration
            if let Some(first) = attempts.first() {
                if let Some(last) = attempts.last() {
                    stats.total_duration_ms = (last.completed_at - first.started_at)
                        .num_milliseconds()
                        .unsigned_abs();
                }
            }
        }
    }

    /// Get retry attempts
    pub async fn attempts(&self) -> Vec<RetryAttempt> {
        self.attempts.read().await.clone()
    }

    /// Get recent attempts
    pub async fn recent_attempts(&self, limit: usize) -> Vec<RetryAttempt> {
        let attempts = self.attempts.read().await;
        attempts.iter().rev().take(limit).cloned().collect()
    }

    /// Get statistics
    pub async fn stats(&self) -> RetryStats {
        self.stats.read().await.clone()
    }

    /// Get name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get config
    pub fn config(&self) -> &RetryConfig {
        &self.config
    }

    /// Reset the retry policy
    pub async fn reset(&self) {
        self.attempts.write().await.clear();
        let mut stats = self.stats.write().await;
        *stats = RetryStats {
            total_attempts: 0,
            successful_attempts: 0,
            failed_attempts: 0,
            total_retries: 0,
            average_delay_ms: 0.0,
            total_duration_ms: 0,
            last_attempt_at: None,
        };
    }

    /// Get success rate
    pub async fn success_rate(&self) -> f64 {
        let stats = self.stats.read().await;
        if stats.total_attempts == 0 {
            return 0.0;
        }
        stats.successful_attempts as f64 / stats.total_attempts as f64
    }
}

/// Retry policy registry
pub struct RetryPolicyRegistry {
    policies: Arc<RwLock<Vec<Arc<RetryPolicy>>>>,
}

impl RetryPolicyRegistry {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a retry policy
    pub async fn register(&self, policy: Arc<RetryPolicy>) {
        let mut policies = self.policies.write().await;
        policies.push(policy);
    }

    /// Get a policy by name
    pub async fn policy(&self, name: &str) -> Option<Arc<RetryPolicy>> {
        let policies = self.policies.read().await;
        policies.iter().find(|p| p.name() == name).cloned()
    }

    /// Get all policies
    pub async fn all_policies(&self) -> Vec<Arc<RetryPolicy>> {
        let policies = self.policies.read().await;
        policies.iter().cloned().collect()
    }

    /// Get all stats
    pub async fn all_stats(&self) -> Vec<(String, RetryStats)> {
        let policies = self.policies.read().await;
        let mut stats = Vec::new();

        for policy in policies.iter() {
            let policy_stats = policy.stats().await;
            stats.push((policy.name().to_string(), policy_stats));
        }

        stats
    }
}

impl Default for RetryPolicyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.strategy, RetryStrategy::ExponentialBackoff);
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 30000);
    }

    #[tokio::test]
    async fn test_retry_policy_creation() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test_policy".to_string(), config);

        assert_eq!(policy.name(), "test_policy");
    }

    #[tokio::test]
    async fn test_successful_execution() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test_policy".to_string(), config);

        let mut call_count = 0;
        let operation = || {
            call_count += 1;
            async { Ok::<&str, String>("success") }
        };

        let result = policy.execute(operation).await;
        assert!(matches!(result, RetryOutcome::Success(_)));

        let stats = policy.stats().await;
        assert_eq!(stats.total_attempts, 1);
        assert_eq!(stats.successful_attempts, 1);
    }

    #[tokio::test]
    async fn test_retry_on_failure() {
        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let operation = || {
            let call_count = call_count.clone();
            async move {
                let count = call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if count < 2 {
                    Err::<&str, String>("temporary failure".to_string())
                } else {
                    Ok("success")
                }
            }
        };

        let result = policy.execute(operation).await;
        assert!(matches!(result, RetryOutcome::Success(_)));

        let attempts = policy.attempts().await;
        assert_eq!(attempts.len(), 3);
    }

    #[tokio::test]
    async fn test_max_attempts_exceeded() {
        let config = RetryConfig {
            max_attempts: 2,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        let operation = || async move { Err::<&str, String>("timeout error".to_string()) };

        let result = policy.execute(operation).await;
        assert!(matches!(result, RetryOutcome::MaxAttemptsExceeded));

        let stats = policy.stats().await;
        assert_eq!(stats.total_attempts, 2);
        assert_eq!(stats.failed_attempts, 1);
    }

    #[tokio::test]
    async fn test_exponential_backoff() {
        let config = RetryConfig {
            max_attempts: 5,
            strategy: RetryStrategy::ExponentialBackoff,
            initial_delay_ms: 100,
            multiplier: 2.0,
            max_delay_ms: 1000,
            jitter_type: JitterType::None,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        // Test delay calculation for different attempts
        for attempt in 0..4 {
            let delay = policy.calculate_delay(attempt).await;
            let expected = (100 * 2_u64.pow(attempt as u32)).min(1000);
            assert_eq!(delay, expected);
        }
    }

    #[tokio::test]
    async fn test_linear_backoff() {
        let config = RetryConfig {
            max_attempts: 5,
            strategy: RetryStrategy::Linear,
            initial_delay_ms: 100,
            jitter_type: JitterType::None,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        let delay_0 = policy.calculate_delay(0).await;
        let delay_1 = policy.calculate_delay(1).await;
        let delay_2 = policy.calculate_delay(2).await;

        assert_eq!(delay_0, 100);
        assert_eq!(delay_1, 200);
        assert_eq!(delay_2, 300);
    }

    #[tokio::test]
    async fn test_fixed_delay() {
        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 500,
            jitter_type: JitterType::None,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        let delay_0 = policy.calculate_delay(0).await;
        let delay_1 = policy.calculate_delay(1).await;
        let delay_2 = policy.calculate_delay(2).await;

        assert_eq!(delay_0, 500);
        assert_eq!(delay_1, 500);
        assert_eq!(delay_2, 500);
    }

    #[tokio::test]
    async fn test_linear_backoff_large_attempt_no_panic() {
        let config = RetryConfig {
            max_attempts: 100,
            strategy: RetryStrategy::Linear,
            initial_delay_ms: u64::MAX / 2,
            jitter_type: JitterType::None,
            max_delay_ms: u64::MAX,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_large".to_string(), config);
        let delay = policy.calculate_delay(100).await;
        // The real test is that calculate_delay doesn't panic on large values.
        // Verify saturating behavior: delay should be capped at max_delay_ms.
        assert_eq!(
            delay,
            u64::MAX,
            "large linear backoff should saturate to max_delay_ms"
        );
    }

    #[tokio::test]
    async fn test_should_retry() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test_policy".to_string(), config);

        assert!(policy.should_retry("temporary timeout", 0));
        assert!(policy.should_retry("connection refused", 0));
        assert!(policy.should_retry("try again later", 0));

        // Non-retryable errors
        assert!(!policy.should_retry("invalid argument", 0));
        assert!(!policy.should_retry("not found", 0));
    }

    #[tokio::test]
    async fn test_custom_retryable_errors() {
        let config = RetryConfig {
            retryable_errors: vec!["timeout".to_string(), "custom error".to_string()],
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        assert!(policy.should_retry("timeout occurred", 0));
        assert!(policy.should_retry("custom error message", 0));

        // Other errors should not retry
        assert!(!policy.should_retry("connection refused", 0));
    }

    #[tokio::test]
    async fn test_statistics() {
        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 10,
            ..Default::default()
        };

        let policy = RetryPolicy::new("test_policy".to_string(), config);

        // Execute a successful operation
        let operation = || Box::pin(async { Ok::<&str, String>("success") });

        let _ = policy.execute(operation).await;

        let stats = policy.stats().await;
        assert_eq!(stats.total_attempts, 1);
        assert_eq!(stats.successful_attempts, 1);
        assert_eq!(stats.failed_attempts, 0);
    }

    #[tokio::test]
    async fn test_reset() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test_policy".to_string(), config);

        // Execute an operation
        let operation = || Box::pin(async { Err::<&str, String>("error".to_string()) });

        let _ = policy.execute(operation).await;

        let attempts_before = policy.attempts().await;
        assert!(!attempts_before.is_empty());

        // Reset
        policy.reset().await;

        let attempts_after = policy.attempts().await;
        assert!(attempts_after.is_empty());

        let stats = policy.stats().await;
        assert_eq!(stats.total_attempts, 0);
    }

    #[tokio::test]
    async fn test_registry() {
        let registry = RetryPolicyRegistry::new();

        let policy1 = Arc::new(RetryPolicy::new(
            "policy1".to_string(),
            RetryConfig::default(),
        ));

        let policy2 = Arc::new(RetryPolicy::new(
            "policy2".to_string(),
            RetryConfig::default(),
        ));

        registry.register(policy1.clone()).await;
        registry.register(policy2.clone()).await;

        let retrieved = registry.policy("policy1").await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "policy1");

        let all_policies = registry.all_policies().await;
        assert_eq!(all_policies.len(), 2);
    }

    #[tokio::test]
    async fn test_success_rate() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test_policy".to_string(), config);

        // Execute multiple operations
        for i in 0..5 {
            let operation = || async move {
                if i < 3 {
                    Ok::<&str, String>("success")
                } else {
                    Err::<&str, String>("error".to_string())
                }
            };
            let _ = policy.execute(operation).await;
        }

        let success_rate = policy.success_rate().await;
        assert!(success_rate > 0.0);
        assert!(success_rate <= 1.0);
    }

    // --- New tests below ---

    #[test]
    fn test_retry_outcome_serialization() {
        let success = RetryOutcome::Success(42);
        let json = serde_json::to_string(&success).unwrap();
        let back: RetryOutcome<i32> = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, RetryOutcome::Success(42)));

        let failed = RetryOutcome::<()>::Failed("error msg".to_string());
        let json = serde_json::to_string(&failed).unwrap();
        assert!(json.contains("error msg"));

        let timeout = RetryOutcome::<()>::Timeout;
        let json = serde_json::to_string(&timeout).unwrap();
        assert!(json.contains("Timeout"));

        let max = RetryOutcome::<()>::MaxAttemptsExceeded;
        let json = serde_json::to_string(&max).unwrap();
        assert!(json.contains("MaxAttemptsExceeded"));
    }

    #[test]
    fn test_retry_strategy_serialization_roundtrip() {
        for strategy in [
            RetryStrategy::FixedDelay,
            RetryStrategy::Linear,
            RetryStrategy::ExponentialBackoff,
            RetryStrategy::Custom,
        ] {
            let json = serde_json::to_string(&strategy).unwrap();
            let back: RetryStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(back, strategy);
        }
    }

    #[test]
    fn test_jitter_type_serialization_roundtrip() {
        for jitter in [
            JitterType::None,
            JitterType::Full,
            JitterType::Equal,
            JitterType::Decorrelated,
        ] {
            let json = serde_json::to_string(&jitter).unwrap();
            let back: JitterType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, jitter);
        }
    }

    #[test]
    fn test_retry_config_custom_values() {
        let config = RetryConfig {
            max_attempts: 10,
            strategy: RetryStrategy::Linear,
            initial_delay_ms: 500,
            max_delay_ms: 60000,
            multiplier: 3.0,
            jitter_type: JitterType::Equal,
            jitter_factor: 0.3,
            max_total_duration_ms: Some(120000),
            retryable_errors: vec!["timeout".to_string()],
        };

        let json = serde_json::to_string(&config).unwrap();
        let back: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.max_attempts, 10);
        assert_eq!(back.strategy, RetryStrategy::Linear);
        assert_eq!(back.max_total_duration_ms, Some(120000));
        assert_eq!(back.retryable_errors.len(), 1);
    }

    #[tokio::test]
    async fn test_get_config() {
        let config = RetryConfig {
            max_attempts: 7,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 200,
            ..Default::default()
        };
        let policy = RetryPolicy::new("policy".to_string(), config.clone());
        let retrieved = policy.config();
        assert_eq!(retrieved.max_attempts, 7);
        assert_eq!(retrieved.initial_delay_ms, 200);
    }

    #[tokio::test]
    async fn test_get_recent_attempts() {
        let config = RetryConfig {
            max_attempts: 3,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 10,
            ..Default::default()
        };
        let policy = RetryPolicy::new("test".to_string(), config);

        let call_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let op = || {
            let cc = call_count.clone();
            async move {
                let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Err::<&str, String>("timeout error".to_string())
                } else {
                    Ok("done")
                }
            }
        };
        let _ = policy.execute(op).await;

        let recent = policy.recent_attempts(2).await;
        assert_eq!(recent.len(), 2);
        // Most recent first
        assert!(recent[0].attempt_number >= recent[1].attempt_number);
    }

    #[tokio::test]
    async fn test_success_rate_zero_attempts() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test".to_string(), config);

        let rate = policy.success_rate().await;
        assert_eq!(rate, 0.0);
    }

    #[tokio::test]
    async fn test_retry_stats_initial_values() {
        let config = RetryConfig::default();
        let policy = RetryPolicy::new("test".to_string(), config);

        let stats = policy.stats().await;
        assert_eq!(stats.total_attempts, 0);
        assert_eq!(stats.successful_attempts, 0);
        assert_eq!(stats.failed_attempts, 0);
        assert_eq!(stats.total_retries, 0);
        assert_eq!(stats.average_delay_ms, 0.0);
        assert_eq!(stats.total_duration_ms, 0);
        assert!(stats.last_attempt_at.is_none());
    }

    #[tokio::test]
    async fn test_should_retry_max_attempts_boundary() {
        let config = RetryConfig {
            max_attempts: 3,
            ..Default::default()
        };
        let policy = RetryPolicy::new("test".to_string(), config);

        // Attempt 0 (first try) should retry since max_attempts - 1 = 2
        assert!(policy.should_retry("timeout", 0));
        // Attempt 1 should still retry
        assert!(policy.should_retry("timeout", 1));
        // Attempt 2 (max_attempts - 1) should NOT retry
        assert!(!policy.should_retry("timeout", 2));
    }

    #[tokio::test]
    async fn test_registry_get_missing_policy() {
        let registry = RetryPolicyRegistry::new();
        let result = registry.policy("nonexistent").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_registry_get_all_stats() {
        let registry = RetryPolicyRegistry::new();

        let p1 = Arc::new(RetryPolicy::new("p1".to_string(), RetryConfig::default()));
        let p2 = Arc::new(RetryPolicy::new("p2".to_string(), RetryConfig::default()));

        registry.register(p1).await;
        registry.register(p2).await;

        let all_stats = registry.all_stats().await;
        assert_eq!(all_stats.len(), 2);

        let names: Vec<&str> = all_stats.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"p1"));
        assert!(names.contains(&"p2"));
    }

    #[tokio::test]
    async fn test_custom_strategy_uses_initial_delay() {
        let config = RetryConfig {
            strategy: RetryStrategy::Custom,
            initial_delay_ms: 250,
            jitter_type: JitterType::None,
            ..Default::default()
        };
        let policy = RetryPolicy::new("test".to_string(), config);

        let delay = policy.calculate_delay(0).await;
        assert_eq!(delay, 250);

        let delay = policy.calculate_delay(5).await;
        assert_eq!(delay, 250);
    }

    #[tokio::test]
    async fn test_max_delay_cap() {
        let config = RetryConfig {
            strategy: RetryStrategy::ExponentialBackoff,
            initial_delay_ms: 1000,
            multiplier: 10.0,
            max_delay_ms: 5000,
            jitter_type: JitterType::None,
            ..Default::default()
        };
        let policy = RetryPolicy::new("test".to_string(), config);

        // attempt 0: 1000 * 10^0 = 1000, capped at 5000 -> 1000
        assert_eq!(policy.calculate_delay(0).await, 1000);
        // attempt 1: 1000 * 10^1 = 10000, capped at 5000 -> 5000
        assert_eq!(policy.calculate_delay(1).await, 5000);
        // attempt 2: 1000 * 10^2 = 100000, capped at 5000 -> 5000
        assert_eq!(policy.calculate_delay(2).await, 5000);
    }

    #[tokio::test]
    async fn test_non_retryable_error_stops_early() {
        let config = RetryConfig {
            max_attempts: 5,
            strategy: RetryStrategy::FixedDelay,
            initial_delay_ms: 10,
            ..Default::default()
        };
        let policy = RetryPolicy::new("test".to_string(), config);

        // "invalid argument" is not in default retryable patterns
        let result = policy
            .execute(|| async { Err::<&str, String>("invalid argument".to_string()) })
            .await;

        // Should return Failed, not MaxAttemptsExceeded
        assert!(matches!(result, RetryOutcome::Failed(_)));
    }

    #[test]
    fn test_retry_attempt_serialization() {
        let attempt = RetryAttempt {
            attempt_number: 2,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            duration_ms: 150,
            error: Some("connection refused".to_string()),
            will_retry: true,
        };
        let json = serde_json::to_string(&attempt).unwrap();
        let back: RetryAttempt = serde_json::from_str(&json).unwrap();
        assert_eq!(back.attempt_number, 2);
        assert_eq!(back.duration_ms, 150);
        assert_eq!(back.error, Some("connection refused".to_string()));
        assert!(back.will_retry);
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for retry_policy
    // =========================================================================

    // 1. RetryConfig default values check
    #[test]
    fn retry_config_default_all_fields() {
        let cfg = RetryConfig::default();
        assert_eq!(cfg.max_attempts, 3);
        assert_eq!(cfg.strategy, RetryStrategy::ExponentialBackoff);
        assert_eq!(cfg.initial_delay_ms, 1000);
        assert_eq!(cfg.max_delay_ms, 30000);
        assert!((cfg.multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(cfg.jitter_type, JitterType::Full);
        assert!((cfg.jitter_factor - 0.5).abs() < f64::EPSILON);
        assert!(cfg.max_total_duration_ms.is_none());
        assert!(cfg.retryable_errors.is_empty());
    }

    // 2. RetryConfig clone equal
    #[test]
    fn retry_config_clone_equal() {
        let cfg = RetryConfig {
            max_attempts: 5,
            strategy: RetryStrategy::Linear,
            initial_delay_ms: 200,
            max_delay_ms: 60000,
            multiplier: 1.5,
            jitter_type: JitterType::None,
            jitter_factor: 0.0,
            max_total_duration_ms: Some(300000),
            retryable_errors: vec!["timeout".into()],
        };
        let cloned = cfg.clone();
        assert_eq!(cloned.max_attempts, cfg.max_attempts);
        assert_eq!(cloned.strategy, cfg.strategy);
        assert_eq!(cloned.retryable_errors, cfg.retryable_errors);
    }

    // 3. RetryConfig debug format
    #[test]
    fn retry_config_debug_format() {
        let cfg = RetryConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("max_attempts"));
        assert!(debug.contains("ExponentialBackoff"));
    }

    // 4. RetryStats serde roundtrip
    #[test]
    fn retry_stats_serde_roundtrip() {
        let stats = RetryStats {
            total_attempts: 10,
            successful_attempts: 7,
            failed_attempts: 3,
            total_retries: 6,
            average_delay_ms: 250.0,
            total_duration_ms: 5000,
            last_attempt_at: Some(Utc::now()),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: RetryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_attempts, 10);
        assert_eq!(decoded.successful_attempts, 7);
        assert_eq!(decoded.failed_attempts, 3);
        assert!((decoded.average_delay_ms - 250.0).abs() < f64::EPSILON);
    }

    // 5. RetryStats with zero values serde roundtrip
    #[test]
    fn retry_stats_zero_values_serde() {
        let stats = RetryStats {
            total_attempts: 0,
            successful_attempts: 0,
            failed_attempts: 0,
            total_retries: 0,
            average_delay_ms: 0.0,
            total_duration_ms: 0,
            last_attempt_at: None,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: RetryStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_attempts, 0);
        assert!(decoded.last_attempt_at.is_none());
    }

    // 6. RetryOutcome Success serde roundtrip with different types
    #[test]
    fn retry_outcome_success_string_serde() {
        let outcome = RetryOutcome::Success("test result".to_string());
        let json = serde_json::to_string(&outcome).unwrap();
        let decoded: RetryOutcome<String> = serde_json::from_str(&json).unwrap();
        if let RetryOutcome::Success(val) = decoded {
            assert_eq!(val, "test result");
        } else {
            panic!("Expected Success variant");
        }
    }

    // 7. RetryOutcome Timeout serde roundtrip
    #[test]
    fn retry_outcome_timeout_serde() {
        let outcome = RetryOutcome::<()>::Timeout;
        let json = serde_json::to_string(&outcome).unwrap();
        let decoded: RetryOutcome<()> = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, RetryOutcome::Timeout));
    }

    // 8. RetryAttempt with no error serde roundtrip
    #[test]
    fn retry_attempt_no_error_serde() {
        let attempt = RetryAttempt {
            attempt_number: 1,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            duration_ms: 50,
            error: None,
            will_retry: false,
        };
        let json = serde_json::to_string(&attempt).unwrap();
        let decoded: RetryAttempt = serde_json::from_str(&json).unwrap();
        assert!(decoded.error.is_none());
        assert!(!decoded.will_retry);
    }

    // 9. RetryStrategy debug format
    #[test]
    fn retry_strategy_debug_format() {
        let debug = format!("{:?}", RetryStrategy::ExponentialBackoff);
        assert!(debug.contains("ExponentialBackoff"));
    }

    // 10. JitterType debug format
    #[test]
    fn jitter_type_debug_format() {
        let debug = format!("{:?}", JitterType::Decorrelated);
        assert!(debug.contains("Decorrelated"));
    }

    // 11. RetryConfig with all retryable errors serde roundtrip
    #[test]
    fn retry_config_with_errors_serde() {
        let cfg = RetryConfig {
            retryable_errors: vec!["timeout".into(), "connection reset".into(), "503".into()],
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let decoded: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.retryable_errors.len(), 3);
        assert!(decoded.retryable_errors.contains(&"timeout".to_string()));
    }

    // 12. RetryStats debug format
    #[test]
    fn retry_stats_debug_format() {
        let stats = RetryStats {
            total_attempts: 5,
            successful_attempts: 4,
            failed_attempts: 1,
            total_retries: 3,
            average_delay_ms: 100.0,
            total_duration_ms: 2000,
            last_attempt_at: None,
        };
        let debug = format!("{:?}", stats);
        assert!(debug.contains("total_attempts"));
        assert!(debug.contains("successful_attempts"));
    }

    // 13. RetryOutcome debug format
    #[test]
    fn retry_outcome_debug_format() {
        let outcome = RetryOutcome::<String>::Failed("test error".into());
        let debug = format!("{:?}", outcome);
        assert!(debug.contains("Failed"));
        assert!(debug.contains("test error"));
    }

    // 14. RetryAttempt debug format
    #[test]
    fn retry_attempt_debug_format() {
        let attempt = RetryAttempt {
            attempt_number: 3,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            duration_ms: 200,
            error: Some("timeout".into()),
            will_retry: true,
        };
        let debug = format!("{:?}", attempt);
        assert!(debug.contains("attempt_number"));
        assert!(debug.contains("will_retry"));
    }

    // 15. RetryOutcome clone equal
    #[test]
    fn retry_outcome_clone_equal() {
        let outcome = RetryOutcome::Success(42);
        let cloned = outcome.clone();
        if let RetryOutcome::Success(val) = cloned {
            assert_eq!(val, 42);
        } else {
            panic!("Expected Success variant");
        }

        let failed = RetryOutcome::<()>::Failed("err".into());
        let cloned_failed = failed.clone();
        if let RetryOutcome::Failed(msg) = cloned_failed {
            assert_eq!(msg, "err");
        } else {
            panic!("Expected Failed variant");
        }
    }
}
