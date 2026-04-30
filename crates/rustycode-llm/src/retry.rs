//! Retry logic with exponential backoff for API calls
//!
//! This module provides configurable retry logic with:
//! - Exponential backoff with configurable multiplier
//! - Jitter to prevent thundering herd problem
//! - Automatic retryable error detection (5xx, timeouts, connection errors)
//! - Maximum attempt limits with configurable delays
//! - Builder pattern for easy configuration
//!
//! # Features
//!
//! - **Exponential Backoff**: Delay increases exponentially with each retry
//! - **Jitter**: Randomness added to delays to prevent synchronized retries
//! - **Smart Error Detection**: Automatically detects retryable errors
//! - **Configurable Limits**: Control max attempts, base/max delays, and multipliers
//! - **Integration**: Works seamlessly with existing LLM providers
//!
//! # Example
//!
//! ```rust
//! use rustycode_llm::retry::{retry_with_backoff, RetryConfig};
//! use anyhow::Result;
//!
//! async fn fetch_data() -> Result<String> {
//!     // Your API call here
//!     Ok("data".to_string())
//! }
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let config = RetryConfig::new()
//!         .with_max_attempts(5)
//!         .with_base_delay(std::time::Duration::from_millis(100))
//!         .with_max_delay(std::time::Duration::from_secs(30));
//!
//!     let result = retry_with_backoff(config, fetch_data).await?;
//!     println!("Result: {}", result);
//!     Ok(())
//! }
//! ```
//!
//! # Retryable Errors
//!
//! The following errors are automatically retried:
//! - HTTP 5xx errors (500, 502, 503, 504)
//! - HTTP 408 (Request Timeout)
//! - HTTP 429 (Too Many Requests)
//! - Network timeouts and connection errors
//! - Errors containing keywords: "timeout", "connection refused", etc.
//!
//! Non-retryable errors (4xx except 408/429) fail immediately.

use anyhow::{anyhow, Result};
use reqwest::StatusCode;
use std::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: usize,

    /// Base delay between retries (exponential backoff starts here)
    pub base_delay: Duration,

    /// Maximum delay between retries
    pub max_delay: Duration,

    /// Multiplier for exponential backoff (e.g., 2.0 = double each time)
    pub multiplier: f64,

    /// Jitter factor to add randomness (0.0 = no jitter, 1.0 = full jitter)
    pub jitter_factor: f64,

    /// Optional hint from a `retry-after-ms` header to override backoff
    pub retry_after_ms: Option<u64>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            multiplier: 2.0,
            jitter_factor: 0.1,
            retry_after_ms: None,
        }
    }
}

impl RetryConfig {
    /// Create a new RetryConfig with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum number of retry attempts
    pub fn with_max_attempts(mut self, max: usize) -> Self {
        self.max_attempts = max;
        self
    }

    /// Set the base delay between retries
    pub fn with_base_delay(mut self, delay: Duration) -> Self {
        self.base_delay = delay;
        self
    }

    /// Set the maximum delay between retries
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the multiplier for exponential backoff
    pub fn with_multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Set the jitter factor (0.0 to 1.0)
    pub fn with_jitter_factor(mut self, jitter: f64) -> Self {
        if !(0.0..=1.0).contains(&jitter) {
            tracing::warn!("jitter_factor {} out of range [0.0, 1.0], clamping", jitter);
        }
        self.jitter_factor = jitter.clamp(0.0, 1.0);
        self
    }

    /// Calculate delay for a given attempt number with exponential backoff and jitter.
    /// If `retry_after_ms` is set (from an API response header), it takes priority.
    fn calculate_delay(&self, attempt: usize) -> Duration {
        // If the API told us exactly how long to wait, respect that
        if let Some(ms) = self.retry_after_ms {
            return Duration::from_millis(ms);
        }

        let base_millis = self.base_delay.as_millis() as f64;
        let exp = (attempt as u32).saturating_sub(1).min(10) as i32; // cap exponent to avoid overflow
        let exponential_delay = base_millis * self.multiplier.powi(exp);
        let capped_delay = exponential_delay.min(self.max_delay.as_millis() as f64);

        // Add jitter to prevent thundering herd
        // Use time-based entropy combined with attempt number for variation across calls
        let jitter_range = capped_delay * self.jitter_factor;
        let random_jitter = if jitter_range > 0.0 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            let entropy =
                (nanos.wrapping_mul(2654435761) ^ (attempt as u64).wrapping_mul(40503)) % 1000;
            (entropy as f64 / 1000.0) * jitter_range
        } else {
            0.0
        };

        let final_delay = (capped_delay + random_jitter).min(self.max_delay.as_millis() as f64);
        Duration::from_millis(final_delay as u64)
    }

    /// Set a server-provided retry delay from response headers.
    /// Overrides the exponential backoff calculation.
    pub fn with_retry_after_ms(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.max_attempts == 0 {
            return Err(anyhow!("max_attempts must be at least 1"));
        }
        if self.multiplier < 1.0 {
            return Err(anyhow!("multiplier must be at least 1.0"));
        }
        if self.base_delay > self.max_delay {
            return Err(anyhow!("base_delay cannot be greater than max_delay"));
        }
        Ok(())
    }
}

/// Determine if an error is retryable
pub fn is_retryable_error(error: &anyhow::Error) -> bool {
    // Check for reqwest errors
    if let Some(reqwest_err) = error.downcast_ref::<reqwest::Error>() {
        return is_retryable_reqwest_error(reqwest_err);
    }

    // Check error messages for common retryable patterns
    let error_msg = error.to_string().to_lowercase();
    retryable_error_keywords(&error_msg)
}

/// Check if a reqwest error is retryable
fn is_retryable_reqwest_error(err: &reqwest::Error) -> bool {
    if err.is_timeout() {
        return true;
    }

    if err.is_connect() {
        return true;
    }

    if let Some(status) = err.status() {
        return is_retryable_status(status);
    }

    false
}

/// Check if an HTTP status code is retryable
fn is_retryable_status(status: StatusCode) -> bool {
    matches!(
        status.as_u16(),
        408 | // Request Timeout
        429 | // Too Many Requests
        500 | // Internal Server Error
        502 | // Bad Gateway
        503 | // Service Unavailable
        504 | // Gateway Timeout
        507 | // Insufficient Storage
        529 | // Anthropic: API overloaded
        598 | // Network read timeout
        599 // Network connect timeout
    )
}

/// Check error message for retryable keywords
fn retryable_error_keywords(msg: &str) -> bool {
    let retryable_keywords = [
        "timeout",
        "connection refused",
        "connection reset",
        "connection lost",
        "temporary failure",
        "service unavailable",
        "rate limit",
        "too many requests",
        "internal server error",
        "bad gateway",
        "gateway timeout",
        "overloaded",
        "529",
        "network error",
        "failed to send request",
        "error sending request",
        "connection error",
    ];

    retryable_keywords
        .iter()
        .any(|&keyword| msg.contains(keyword))
}

/// Retry an operation with exponential backoff
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry
///
/// # Returns
///
/// * `Ok(T)` - Successful result from the operation
/// * `Err(anyhow::Error)` - Final error after all retries exhausted
///
/// # Example
///
/// ```rust
/// use anyhow::Result;
/// use rustycode_llm::retry::{retry_with_backoff, RetryConfig};
///
/// async fn fetch_data() -> Result<String> {
///     // Your API call here
///     Ok("data".to_string())
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<()> {
///     let config = RetryConfig::new().with_max_attempts(5);
///     let result = retry_with_backoff(config, fetch_data).await?;
///     Ok(())
/// }
/// ```
pub async fn retry_with_backoff<F, Fut, T>(config: RetryConfig, mut operation: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    config.validate()?;

    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        "Operation succeeded on attempt {}/{}",
                        attempt,
                        config.max_attempts
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                last_error = Some(error);

                if attempt < config.max_attempts {
                    let Some(error_ref) = last_error.as_ref() else {
                        unreachable!("last_error was just set on line above");
                    };
                    if is_retryable_error(error_ref) {
                        let mut delay = config.calculate_delay(attempt);
                        // Rate-limited responses need much longer backoff
                        let err_str = error_ref.to_string().to_lowercase();
                        if err_str.contains("rate limit") || err_str.contains("too many requests") {
                            let min_rate_limit_delay = Duration::from_secs(5);
                            if delay < min_rate_limit_delay {
                                delay = min_rate_limit_delay;
                            }
                        }
                        tracing::warn!(
                            "Attempt {}/{} failed, retrying after {:?}: {}",
                            attempt,
                            config.max_attempts,
                            delay,
                            error_ref
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    } else {
                        tracing::error!(
                            "Attempt {}/{} failed with non-retryable error: {}",
                            attempt,
                            config.max_attempts,
                            error_ref
                        );
                        return Err(last_error
                            .take()
                            .unwrap_or_else(|| anyhow!("non-retryable error lost")));
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!("All retry attempts exhausted")))
}

/// Extract retry delay from HTTP response headers.
///
/// Checks in priority order:
/// 1. `retry-after-ms` — milliseconds (used by Anthropic)
/// 2. `retry-after` — seconds (numeric only; HTTP-date parsing requires extra deps)
///
/// Returns `None` if no valid header found.
pub fn extract_retry_after_ms(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    // Priority 1: retry-after-ms (Anthropic-specific)
    if let Some(val) = headers.get("retry-after-ms") {
        if let Ok(s) = val.to_str() {
            if let Ok(ms) = s.parse::<f64>() {
                return Some(ms.ceil() as u64);
            }
        }
    }

    // Priority 2: retry-after (standard HTTP header, numeric seconds)
    if let Some(val) = headers.get("retry-after") {
        if let Ok(s) = val.to_str() {
            if let Ok(secs) = s.parse::<u64>() {
                return Some(secs.saturating_mul(1000));
            }
            // Try as float seconds (e.g., "2.5")
            if let Ok(secs) = s.parse::<f64>() {
                return Some((secs * 1000.0).min(u64::MAX as f64).ceil() as u64);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert_eq!(config.multiplier, 2.0);
        assert_eq!(config.jitter_factor, 0.1);
    }

    #[test]
    fn test_retry_config_builder() {
        let config = RetryConfig::new()
            .with_max_attempts(5)
            .with_base_delay(Duration::from_millis(200))
            .with_max_delay(Duration::from_secs(30))
            .with_multiplier(3.0)
            .with_jitter_factor(0.2);

        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.base_delay, Duration::from_millis(200));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.multiplier, 3.0);
        assert_eq!(config.jitter_factor, 0.2);
    }

    #[test]
    fn test_retry_config_validate() {
        // Valid config
        let config = RetryConfig::default();
        assert!(config.validate().is_ok());

        // Invalid: max_attempts is 0
        let config = RetryConfig::default().with_max_attempts(0);
        assert!(config.validate().is_err());

        // Invalid: multiplier < 1.0
        let config = RetryConfig::default().with_multiplier(0.5);
        assert!(config.validate().is_err());

        // Invalid: base_delay > max_delay
        let config = RetryConfig::default()
            .with_base_delay(Duration::from_secs(10))
            .with_max_delay(Duration::from_secs(1));
        assert!(config.validate().is_err());

        // Invalid jitter_factor > 1.0 is now clamped instead of panicking
        let config = RetryConfig::default().with_jitter_factor(1.5);
        assert!((config.jitter_factor - 1.0).abs() < f64::EPSILON); // clamped to 1.0
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = RetryConfig::new()
            .with_base_delay(Duration::from_millis(100))
            .with_multiplier(2.0)
            .with_max_delay(Duration::from_secs(10))
            .with_jitter_factor(0.0); // No jitter for predictable testing

        let delay1 = config.calculate_delay(1);
        assert_eq!(delay1, Duration::from_millis(100));

        let delay2 = config.calculate_delay(2);
        assert_eq!(delay2, Duration::from_millis(200));

        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3, Duration::from_millis(400));

        let delay4 = config.calculate_delay(4);
        assert_eq!(delay4, Duration::from_millis(800));
    }

    #[test]
    fn test_calculate_delay_capping() {
        let config = RetryConfig::new()
            .with_base_delay(Duration::from_millis(100))
            .with_multiplier(10.0)
            .with_max_delay(Duration::from_millis(500))
            .with_jitter_factor(0.0);

        // Should be capped at max_delay
        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3, Duration::from_millis(500));
    }

    #[test]
    fn test_is_retryable_status() {
        // Retryable status codes
        assert!(is_retryable_status(StatusCode::REQUEST_TIMEOUT));
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::from_u16(529).unwrap()));
        assert!(is_retryable_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable_status(StatusCode::BAD_GATEWAY));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_retryable_status(StatusCode::GATEWAY_TIMEOUT));

        // Non-retryable status codes
        assert!(!is_retryable_status(StatusCode::OK));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
        assert!(!is_retryable_status(StatusCode::UNPROCESSABLE_ENTITY));
    }

    #[test]
    fn test_retryable_error_keywords() {
        // Retryable error messages
        assert!(retryable_error_keywords("request timeout"));
        assert!(retryable_error_keywords("connection refused"));
        assert!(retryable_error_keywords("service unavailable"));
        assert!(retryable_error_keywords("rate limit exceeded"));
        assert!(retryable_error_keywords("api overloaded"));
        assert!(retryable_error_keywords("anthropic 529 overloaded"));
        assert!(retryable_error_keywords("internal server error"));

        // Non-retryable error messages
        assert!(!retryable_error_keywords("invalid request"));
        assert!(!retryable_error_keywords("unauthorized"));
        assert!(!retryable_error_keywords("not found"));
        assert!(!retryable_error_keywords("validation error"));
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_on_first_try() {
        let config = RetryConfig::new();
        let call_count = AtomicUsize::new(0);

        let result = retry_with_backoff(config, || {
            call_count.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, anyhow::Error>("success") }
        })
        .await
        .unwrap();

        assert_eq!(result, "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_success_after_retries() {
        let config = RetryConfig::new()
            .with_max_attempts(5)
            .with_base_delay(Duration::from_millis(10))
            .with_jitter_factor(0.0);

        let call_count = AtomicUsize::new(0);

        let result = retry_with_backoff(config, || {
            let count = call_count.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if count < 3 {
                    Err(anyhow!("temporary failure"))
                } else {
                    Ok::<_, anyhow::Error>("success")
                }
            }
        })
        .await
        .unwrap();

        assert_eq!(result, "success");
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_exhausted() {
        let config = RetryConfig::new()
            .with_max_attempts(3)
            .with_base_delay(Duration::from_millis(10))
            .with_jitter_factor(0.0);

        let call_count = AtomicUsize::new(0);

        let result = retry_with_backoff(config, || {
            call_count.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(anyhow!("temporary failure")) }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_non_retryable() {
        let config = RetryConfig::new().with_max_attempts(5);

        let call_count = AtomicUsize::new(0);

        let result = retry_with_backoff(config, || {
            call_count.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(anyhow!("invalid request")) }
        })
        .await;

        assert!(result.is_err());
        // Should fail immediately on first non-retryable error
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_with_backoff_custom_retryable_error() {
        let config = RetryConfig::new()
            .with_max_attempts(3)
            .with_base_delay(Duration::from_millis(10));

        let call_count = AtomicUsize::new(0);

        let result = retry_with_backoff(config, || {
            let count = call_count.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if count < 2 {
                    Err(anyhow!("connection refused"))
                } else {
                    Ok::<_, anyhow::Error>("recovered")
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_retry_after_ms_overrides_backoff() {
        let config = RetryConfig::new()
            .with_base_delay(Duration::from_millis(100))
            .with_retry_after_ms(5000);

        // All attempts should use the server-provided delay
        assert_eq!(config.calculate_delay(1), Duration::from_secs(5));
        assert_eq!(config.calculate_delay(2), Duration::from_secs(5));
    }

    #[test]
    fn test_extract_retry_after_ms_from_retry_after_ms_header() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after-ms", "5000".parse().unwrap());

        assert_eq!(extract_retry_after_ms(&headers), Some(5000));
    }

    #[test]
    fn test_extract_retry_after_ms_from_retry_after_seconds() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after", "30".parse().unwrap());

        assert_eq!(extract_retry_after_ms(&headers), Some(30_000));
    }

    #[test]
    fn test_extract_retry_after_ms_no_headers() {
        let headers = reqwest::header::HeaderMap::new();
        assert_eq!(extract_retry_after_ms(&headers), None);
    }

    #[test]
    fn test_extract_retry_after_ms_priority() {
        // retry-after-ms takes priority over retry-after
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("retry-after-ms", "2000".parse().unwrap());
        headers.insert("retry-after", "30".parse().unwrap());

        assert_eq!(extract_retry_after_ms(&headers), Some(2000));
    }
}
