//! Rate limiting using token bucket algorithm
//!
//! This module provides a flexible rate limiter that supports multiple rate limits
//! simultaneously (e.g., requests per minute and tokens per minute). It uses the
//! token bucket algorithm which allows for bursts while maintaining long-term rate limits.
//!
//! # Example
//!
//! ```no_run
//! use rustycode_llm::RateLimiter;
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create a rate limiter with 10 RPM and 10000 TPM
//!     let mut limiter = RateLimiter::builder()
//!         .requests_per_minute(10)
//!         .tokens_per_minute(10000)
//!         .build();
//!
//!     // Acquire permission to make a request
//!     limiter.rate_limit(100).await;
//!     println!("Request allowed!");
//! }
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Rate limit type for different metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RateLimitType {
    /// Requests per time window
    Requests,
    /// Tokens per time window
    Tokens,
}

/// Configuration for a single rate limit
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum number of units allowed per window
    pub max_units: u64,
    /// Time window duration
    pub window_duration: Duration,
    /// Type of rate limit
    pub limit_type: RateLimitType,
}

impl RateLimitConfig {
    /// Create a new rate limit configuration
    ///
    /// # Arguments
    ///
    /// * `max_units` - Maximum units allowed per time window
    /// * `window_duration` - Duration of the time window
    /// * `limit_type` - Type of rate limit (requests or tokens)
    pub fn new(max_units: u64, window_duration: Duration, limit_type: RateLimitType) -> Self {
        Self {
            max_units,
            window_duration,
            limit_type,
        }
    }

    /// Create a requests-per-minute limit
    pub fn requests_per_minute(rpm: u64) -> Self {
        Self::new(rpm, Duration::from_mins(1), RateLimitType::Requests)
    }

    /// Create a requests-per-second limit
    pub fn requests_per_second(rps: u64) -> Self {
        Self::new(rps, Duration::from_secs(1), RateLimitType::Requests)
    }

    /// Create a tokens-per-minute limit
    pub fn tokens_per_minute(tpm: u64) -> Self {
        Self::new(tpm, Duration::from_mins(1), RateLimitType::Tokens)
    }

    /// Create a tokens-per-second limit
    pub fn tokens_per_second(tps: u64) -> Self {
        Self::new(tps, Duration::from_secs(1), RateLimitType::Tokens)
    }
}

/// Token bucket for tracking a single rate limit
#[derive(Debug)]
struct TokenBucket {
    /// Current number of tokens in the bucket
    tokens: f64,
    /// Maximum tokens the bucket can hold
    max_tokens: f64,
    /// Time when tokens were last updated
    last_update: Instant,
    /// Rate at which tokens are replenished (tokens per second)
    refill_rate: f64,
}

impl TokenBucket {
    /// Create a new token bucket
    fn new(max_tokens: u64, window_duration: Duration) -> Self {
        let refill_rate = if window_duration.as_secs_f64() > 0.0 {
            max_tokens as f64 / window_duration.as_secs_f64()
        } else {
            f64::INFINITY
        };
        Self {
            tokens: max_tokens as f64,
            max_tokens: max_tokens as f64,
            last_update: Instant::now(),
            refill_rate,
        }
    }

    /// Replenish tokens based on elapsed time
    fn replenish(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_update = now;
    }

    /// Try to consume the specified number of tokens
    ///
    /// Returns the duration to wait if insufficient tokens, or None if successful
    fn try_consume(&mut self, tokens: u64) -> Option<Duration> {
        self.replenish();

        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            None
        } else {
            // Calculate wait time needed
            let needed = tokens as f64 - self.tokens;
            let wait_secs = needed / self.refill_rate;
            Some(Duration::from_secs_f64(wait_secs))
        }
    }

    /// Get current token count
    fn available_tokens(&self) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        (self.tokens + elapsed * self.refill_rate).min(self.max_tokens)
    }

    /// Reset the bucket to full capacity
    fn reset(&mut self) {
        self.tokens = self.max_tokens;
        self.last_update = Instant::now();
    }
}

/// Rate limiter using token bucket algorithm
///
/// The rate limiter supports multiple concurrent rate limits (e.g., RPM and TPM)
/// and will wait until all limits are satisfied before allowing a request.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    /// Inner state protected by mutex for thread safety
    inner: Arc<Mutex<RateLimiterInner>>,
}

/// Inner state of the rate limiter
#[derive(Debug)]
struct RateLimiterInner {
    /// Token buckets for each rate limit
    buckets: Vec<(TokenBucket, RateLimitType)>,
}

impl RateLimiter {
    /// Create a new rate limiter with custom configuration
    ///
    /// # Arguments
    ///
    /// * `limits` - Vector of rate limit configurations
    pub fn new(limits: Vec<RateLimitConfig>) -> Self {
        let buckets = limits
            .into_iter()
            .map(|config| {
                let bucket = TokenBucket::new(config.max_units, config.window_duration);
                (bucket, config.limit_type)
            })
            .collect();

        Self {
            inner: Arc::new(Mutex::new(RateLimiterInner { buckets })),
        }
    }

    /// Create a rate limiter builder for fluent configuration
    pub fn builder() -> RateLimiterBuilder {
        RateLimiterBuilder::new()
    }

    /// Acquire permission to proceed, waiting if necessary
    ///
    /// This method checks all rate limits and waits until the request can proceed.
    /// It automatically consumes tokens for the request based on the limit type.
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of tokens to consume (used for token-based limits)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use rustycode_llm::RateLimiter;
    /// # #[tokio::main]
    /// # async fn main() {
    /// # let mut limiter = RateLimiter::builder().build();
    /// // Wait until rate limit allows the request
    /// limiter.rate_limit(100).await;
    /// # }
    /// ```
    pub async fn rate_limit(&self, tokens: u64) {
        loop {
            let mut inner = self.inner.lock().await;
            let mut max_wait: Option<Duration> = None;

            // Check all buckets and find the maximum wait time
            for (bucket, limit_type) in &mut inner.buckets {
                let tokens_to_consume = match limit_type {
                    RateLimitType::Requests => 1, // Each request consumes 1 request token
                    RateLimitType::Tokens => tokens,
                };

                if let Some(wait_duration) = bucket.try_consume(tokens_to_consume) {
                    max_wait = Some(match max_wait {
                        Some(current_max) => current_max.max(wait_duration),
                        None => wait_duration,
                    });
                }
            }

            drop(inner); // Release lock before sleeping

            match max_wait {
                Some(wait_duration) => {
                    // Add a small buffer to account for processing time
                    let buffered_wait = wait_duration.saturating_add(Duration::from_millis(10));
                    sleep(buffered_wait).await;
                }
                None => break, // All limits satisfied
            }
        }
    }

    /// Check if a request would be allowed without waiting
    ///
    /// # Arguments
    ///
    /// * `tokens` - Number of tokens that would be consumed
    ///
    /// # Returns
    ///
    /// `true` if the request would be allowed immediately, `false` otherwise
    pub async fn try_acquire(&self, tokens: u64) -> bool {
        let mut inner = self.inner.lock().await;

        // First pass: check all buckets have enough tokens
        for (bucket, limit_type) in &inner.buckets {
            let tokens_to_consume = match limit_type {
                RateLimitType::Requests => 1,
                RateLimitType::Tokens => tokens,
            };
            if bucket.available_tokens() < tokens_to_consume as f64 {
                return false;
            }
        }

        // Second pass: consume from all buckets
        for (bucket, limit_type) in &mut inner.buckets {
            let tokens_to_consume = match limit_type {
                RateLimitType::Requests => 1,
                RateLimitType::Tokens => tokens,
            };
            bucket.try_consume(tokens_to_consume);
        }

        true
    }

    /// Get available tokens for a specific limit type
    ///
    /// # Arguments
    ///
    /// * `limit_type` - Type of rate limit to query
    ///
    /// # Returns
    ///
    /// Available tokens as a float, or `None` if limit type not found
    pub async fn available_tokens(&self, limit_type: RateLimitType) -> Option<f64> {
        let inner = self.inner.lock().await;

        inner
            .buckets
            .iter()
            .find(|(_, lt)| *lt == limit_type)
            .map(|(bucket, _)| bucket.available_tokens())
    }

    /// Reset all rate limits to full capacity
    ///
    /// This is primarily useful for testing or manual rate limit reset scenarios.
    pub async fn reset(&self) {
        let mut inner = self.inner.lock().await;

        for (bucket, _) in &mut inner.buckets {
            bucket.reset();
        }
    }

    /// Get the number of active rate limits
    pub async fn limit_count(&self) -> usize {
        let inner = self.inner.lock().await;
        inner.buckets.len()
    }
}

/// Builder for creating rate limiters with fluent API
pub struct RateLimiterBuilder {
    limits: Vec<RateLimitConfig>,
}

impl RateLimiterBuilder {
    /// Create a new builder
    fn new() -> Self {
        Self { limits: Vec::new() }
    }

    /// Add requests-per-minute limit
    pub fn requests_per_minute(mut self, rpm: u64) -> Self {
        self.limits.push(RateLimitConfig::requests_per_minute(rpm));
        self
    }

    /// Add requests-per-second limit
    pub fn requests_per_second(mut self, rps: u64) -> Self {
        self.limits.push(RateLimitConfig::requests_per_second(rps));
        self
    }

    /// Add tokens-per-minute limit
    pub fn tokens_per_minute(mut self, tpm: u64) -> Self {
        self.limits.push(RateLimitConfig::tokens_per_minute(tpm));
        self
    }

    /// Add tokens-per-second limit
    pub fn tokens_per_second(mut self, tps: u64) -> Self {
        self.limits.push(RateLimitConfig::tokens_per_second(tps));
        self
    }

    /// Add a custom rate limit
    pub fn custom_limit(mut self, config: RateLimitConfig) -> Self {
        self.limits.push(config);
        self
    }

    /// Build the rate limiter
    ///
    /// # Panics
    ///
    /// Panics if no rate limits have been configured
    pub fn build(self) -> RateLimiter {
        if self.limits.is_empty() {
            panic!("RateLimiter must have at least one rate limit configured");
        }

        RateLimiter::new(self.limits)
    }
}

impl Default for RateLimiterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_rate_limiter_basic() {
        let limiter = RateLimiter::builder().requests_per_second(2).build();

        let start = Instant::now();

        // Should complete immediately
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() < 100);

        // Second request should also be immediate
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() < 100);

        // Third request should wait (2 RPS means 500ms between requests)
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() >= 400);
    }

    #[tokio::test]
    async fn test_token_bucket_replenishment() {
        let limiter = RateLimiter::builder().requests_per_second(1).build();

        // Consume the single request
        limiter.rate_limit(0).await;

        // Wait for replenishment
        sleep(Duration::from_millis(1100)).await;

        // Should now be available again
        let start = Instant::now();
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() < 100);
    }

    #[tokio::test]
    async fn test_multiple_limits() {
        let limiter = RateLimiter::builder()
            .requests_per_second(10)
            .tokens_per_second(100)
            .build();

        // Make 10 small token requests quickly
        for _ in 0..10 {
            limiter.rate_limit(10).await;
        }

        // Next request should wait due to request limit
        let start = Instant::now();
        limiter.rate_limit(10).await;
        assert!(start.elapsed().as_millis() >= 90);
    }

    #[tokio::test]
    async fn test_token_limit_only() {
        let limiter = RateLimiter::builder().tokens_per_second(50).build();

        let start = Instant::now();

        // First request consumes 50 tokens
        limiter.rate_limit(50).await;
        assert!(start.elapsed().as_millis() < 50);

        // Second request needs to wait for replenishment
        limiter.rate_limit(50).await;
        assert!(start.elapsed().as_millis() >= 950);
    }

    #[tokio::test]
    async fn try_acquire_test() {
        let limiter = RateLimiter::builder().requests_per_second(1).build();

        // First should succeed
        assert!(limiter.try_acquire(0).await);

        // Second should fail without waiting
        assert!(!limiter.try_acquire(0).await);

        // But after waiting, it should succeed
        sleep(Duration::from_millis(1100)).await;
        assert!(limiter.try_acquire(0).await);
    }

    #[tokio::test]
    async fn available_tokens_test() {
        let limiter = RateLimiter::builder()
            .requests_per_second(10)
            .tokens_per_second(100)
            .build();

        let req_tokens = limiter.available_tokens(RateLimitType::Requests).await;
        assert_eq!(req_tokens, Some(10.0));

        let token_tokens = limiter.available_tokens(RateLimitType::Tokens).await;
        assert_eq!(token_tokens, Some(100.0));

        // Consume some tokens
        limiter.rate_limit(50).await;

        let token_tokens = limiter.available_tokens(RateLimitType::Tokens).await;
        assert!(token_tokens.unwrap() < 100.0);
        assert!(token_tokens.unwrap() > 40.0);
    }

    #[tokio::test]
    async fn reset_test() {
        let limiter = RateLimiter::builder().requests_per_second(1).build();

        // Consume the request
        limiter.rate_limit(0).await;

        // Should not be available
        assert!(!limiter.try_acquire(0).await);

        // Reset
        limiter.reset().await;

        // Should now be available
        assert!(limiter.try_acquire(0).await);
    }

    #[tokio::test]
    async fn limit_count_test() {
        let limiter = RateLimiter::builder()
            .requests_per_second(10)
            .tokens_per_second(100)
            .build();

        assert_eq!(limiter.limit_count().await, 2);
    }

    #[tokio::test]
    #[should_panic(expected = "at least one rate limit")]
    async fn test_builder_panic_on_no_limits() {
        RateLimiter::builder().build();
    }

    #[tokio::test]
    async fn test_burst_capacity() {
        let limiter = RateLimiter::builder().requests_per_second(10).build();

        let start = Instant::now();

        // Should be able to make 10 requests immediately (burst capacity)
        for _ in 0..10 {
            limiter.rate_limit(0).await;
        }

        // Should complete quickly (< 100ms) since we're within burst limit
        assert!(start.elapsed().as_millis() < 100);

        // 11th request should wait
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() >= 90);
    }

    #[tokio::test]
    async fn test_custom_limit_config() {
        let custom_config =
            RateLimitConfig::new(5, Duration::from_secs(10), RateLimitType::Requests);

        let limiter = RateLimiter::builder().custom_limit(custom_config).build();

        let start = Instant::now();

        // Consume all 5 requests
        for _ in 0..5 {
            limiter.rate_limit(0).await;
        }
        assert!(start.elapsed().as_millis() < 100);

        // 6th request should wait (2 seconds per request)
        limiter.rate_limit(0).await;
        assert!(start.elapsed().as_millis() >= 1900);
    }

    #[tokio::test]
    async fn test_rate_limit_config_display() {
        let config = RateLimitConfig::requests_per_minute(60);
        assert_eq!(config.max_units, 60);
        assert_eq!(config.window_duration, Duration::from_mins(1));
    }

    #[tokio::test]
    async fn test_concurrent_rate_limits() {
        let limiter = RateLimiter::builder()
            .requests_per_second(2)
            .tokens_per_second(100)
            .build();

        // Spawn multiple concurrent tasks
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let limiter = limiter.clone();
                tokio::spawn(async move {
                    limiter.rate_limit(30).await;
                })
            })
            .collect();

        // Wait for all to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Wait for request bucket to refill (2 RPS, need ~500ms for 1 token)
        sleep(Duration::from_millis(600)).await;

        // Verify we can still make requests after concurrency
        assert!(limiter.try_acquire(0).await || limiter.try_acquire(30).await);
    }
}
