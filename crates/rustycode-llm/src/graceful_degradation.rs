//! Graceful Degradation Support
//!
//! Enables RustyCode to gracefully degrade when LLM APIs fail, returning partial
//! results instead of complete failure. Provides error classification, fallback
//! strategies, and offline mode support.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, warn};

// ─── Error Classification ───────────────────────────────────────────────────

/// Severity level for API errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorSeverity {
    /// Transient error that may succeed on retry
    Transient,
    /// Permanent error that will not recover
    Permanent,
    /// Network-related error (could be either)
    Network,
}

/// Classified error kind for determining recovery strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Rate limit exceeded (429, 529)
    RateLimit,
    /// Authentication failed (401, 403)
    AuthError,
    /// Network connectivity issue
    NetworkError,
    /// Model not available or doesn't exist
    ModelUnavailable,
    /// Context too long for model
    ContextTooLong,
    /// Invalid request format (400)
    InvalidRequest,
    /// Request timeout
    Timeout,
    /// Unknown error type
    Unknown,
}

impl ErrorKind {
    /// Get the severity level for this error kind
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            ErrorKind::RateLimit => ErrorSeverity::Transient,
            ErrorKind::NetworkError => ErrorSeverity::Network,
            ErrorKind::Timeout => ErrorSeverity::Transient,
            ErrorKind::AuthError => ErrorSeverity::Permanent,
            ErrorKind::InvalidRequest => ErrorSeverity::Permanent,
            ErrorKind::ContextTooLong => ErrorSeverity::Permanent,
            ErrorKind::ModelUnavailable => ErrorSeverity::Permanent,
            ErrorKind::Unknown => ErrorSeverity::Network,
            #[allow(unreachable_patterns)]
            _ => ErrorSeverity::Network,
        }
    }

    /// Check if this error should be retried
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.severity(),
            ErrorSeverity::Transient | ErrorSeverity::Network
        )
    }

    /// Get user-friendly message for this error
    pub fn user_message(&self) -> &str {
        match self {
            ErrorKind::RateLimit => "API rate limited. Please wait a moment and try again.",
            ErrorKind::AuthError => "API authentication failed. Please check your API key.",
            ErrorKind::NetworkError => "Network connection issue. Check your internet.",
            ErrorKind::ModelUnavailable => "Model is temporarily unavailable. Try another model.",
            ErrorKind::ContextTooLong => "Input too long for model. Please reduce context size.",
            ErrorKind::InvalidRequest => "Invalid request. Please check your input.",
            ErrorKind::Timeout => "Request timed out. Please try again.",
            ErrorKind::Unknown => "An unknown error occurred. Please try again.",
            #[allow(unreachable_patterns)]
            _ => "An error occurred. Please try again.",
        }
    }

    /// Get recovery suggestion
    pub fn recovery_suggestion(&self) -> &str {
        match self {
            ErrorKind::RateLimit => "Retry in a few seconds",
            ErrorKind::AuthError => "Check and refresh your API key",
            ErrorKind::NetworkError => "Check internet connection and retry",
            ErrorKind::ModelUnavailable => "Try a different model",
            ErrorKind::ContextTooLong => "Reduce input size and retry",
            ErrorKind::InvalidRequest => "Review and fix the request",
            ErrorKind::Timeout => "Increase timeout or retry with smaller input",
            ErrorKind::Unknown => "Check logs and retry",
            #[allow(unreachable_patterns)]
            _ => "Retry the operation",
        }
    }
}

/// Error classifier for determining recovery strategies
pub struct ErrorClassifier;

impl ErrorClassifier {
    /// Classify an error string into a typed error kind
    pub fn classify(error_msg: &str) -> ErrorKind {
        let msg_lower = error_msg.to_lowercase();

        // Check for rate limit errors
        if msg_lower.contains("429")
            || msg_lower.contains("529")
            || msg_lower.contains("rate limit")
            || msg_lower.contains("too many requests")
            || msg_lower.contains("overloaded")
        {
            return ErrorKind::RateLimit;
        }

        // Check for auth errors
        if msg_lower.contains("401")
            || msg_lower.contains("403")
            || msg_lower.contains("unauthorized")
            || msg_lower.contains("api key")
            || msg_lower.contains("authentication")
            || msg_lower.contains("forbidden")
        {
            return ErrorKind::AuthError;
        }

        // Check for context too long
        if msg_lower.contains("context")
            && (msg_lower.contains("limit")
                || msg_lower.contains("too long")
                || msg_lower.contains("maximum")
                || msg_lower.contains("exceeded"))
        {
            return ErrorKind::ContextTooLong;
        }

        // Check for timeout
        if msg_lower.contains("timeout") || msg_lower.contains("timed out") {
            return ErrorKind::Timeout;
        }

        // Check for network errors
        if msg_lower.contains("connection")
            || msg_lower.contains("network")
            || msg_lower.contains("dns")
            || msg_lower.contains("unreachable")
        {
            return ErrorKind::NetworkError;
        }

        // Check for model unavailable
        if msg_lower.contains("model")
            && (msg_lower.contains("not found")
                || msg_lower.contains("unavailable")
                || msg_lower.contains("does not exist"))
        {
            return ErrorKind::ModelUnavailable;
        }

        // Check for invalid request
        if msg_lower.contains("400")
            || msg_lower.contains("invalid")
            || msg_lower.contains("bad request")
        {
            return ErrorKind::InvalidRequest;
        }

        ErrorKind::Unknown
    }
}

// ─── Partial Results ────────────────────────────────────────────────────────

/// Metadata about degraded operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationMetadata {
    /// Whether the result is degraded (incomplete)
    pub is_degraded: bool,
    /// Error kind that caused degradation
    pub error_kind: Option<ErrorKind>,
    /// User-friendly error message
    pub error_message: Option<String>,
    /// Recovery suggestion
    pub recovery_suggestion: Option<String>,
    /// Whether result came from cache
    pub from_cache: bool,
    /// Whether operation was offline
    pub offline_mode: bool,
    /// Timestamp when degradation occurred
    pub degraded_at: Option<String>,
}

impl DegradationMetadata {
    /// Create a new, non-degraded metadata
    pub fn ok() -> Self {
        Self {
            is_degraded: false,
            error_kind: None,
            error_message: None,
            recovery_suggestion: None,
            from_cache: false,
            offline_mode: false,
            degraded_at: None,
        }
    }

    /// Create degraded metadata from error kind
    pub fn degraded(error_kind: ErrorKind) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            is_degraded: true,
            error_kind: Some(error_kind.clone()),
            error_message: Some(error_kind.user_message().to_string()),
            recovery_suggestion: Some(error_kind.recovery_suggestion().to_string()),
            from_cache: false,
            offline_mode: false,
            degraded_at: Some(now),
        }
    }

    /// Mark that this result came from cache
    pub fn from_cache_fallback(mut self) -> Self {
        self.from_cache = true;
        self
    }

    /// Mark that this result came from offline mode
    pub fn from_offline(mut self) -> Self {
        self.offline_mode = true;
        self
    }

    /// Builder for DegradationMetadata
    pub fn builder() -> DegradationMetadataBuilder {
        DegradationMetadataBuilder::new()
    }
}

/// Builder for DegradationMetadata
#[allow(dead_code)] // Kept for future use
pub struct DegradationMetadataBuilder {
    is_degraded: bool,
    error_kind: Option<ErrorKind>,
    error_message: Option<String>,
    recovery_suggestion: Option<String>,
    from_cache: bool,
    offline_mode: bool,
    degraded_at: Option<String>,
}

impl DegradationMetadataBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            is_degraded: false,
            error_kind: None,
            error_message: None,
            recovery_suggestion: None,
            from_cache: false,
            offline_mode: false,
            degraded_at: None,
        }
    }

    /// Set degraded flag
    pub fn degraded(mut self, degraded: bool) -> Self {
        self.is_degraded = degraded;
        self
    }

    /// Set error kind
    pub fn error_kind(mut self, kind: ErrorKind) -> Self {
        self.error_kind = Some(kind);
        self
    }

    /// Set from cache
    pub fn from_cache(mut self, from_cache: bool) -> Self {
        self.from_cache = from_cache;
        self
    }

    /// Set offline mode
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline_mode = offline;
        self
    }

    /// Build the metadata
    pub fn build(self) -> DegradationMetadata {
        let degraded_at = if self.is_degraded {
            Some(chrono::Local::now().to_rfc3339())
        } else {
            None
        };

        DegradationMetadata {
            is_degraded: self.is_degraded,
            error_kind: self.error_kind.clone(),
            error_message: self
                .error_kind
                .as_ref()
                .map(|k| k.user_message().to_string()),
            recovery_suggestion: self
                .error_kind
                .as_ref()
                .map(|k| k.recovery_suggestion().to_string()),
            from_cache: self.from_cache,
            offline_mode: self.offline_mode,
            degraded_at,
        }
    }
}

impl Default for DegradationMetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Partial result wrapper for degraded operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResult<T> {
    /// Available data
    pub available: Option<T>,
    /// Unavailable features and their reasons
    pub unavailable: HashMap<String, String>,
    /// Degradation metadata
    pub metadata: DegradationMetadata,
}

impl<T> PartialResult<T> {
    /// Create a successful partial result
    pub fn success(data: T) -> Self {
        Self {
            available: Some(data),
            unavailable: HashMap::new(),
            metadata: DegradationMetadata::ok(),
        }
    }

    /// Create a partial result from degradation
    pub fn degraded(data: Option<T>, error_kind: ErrorKind) -> Self {
        Self {
            available: data,
            unavailable: HashMap::new(),
            metadata: DegradationMetadata::degraded(error_kind),
        }
    }

    /// Add an unavailable feature
    pub fn unavailable(mut self, feature: impl Into<String>, reason: impl Into<String>) -> Self {
        self.unavailable.insert(feature.into(), reason.into());
        self
    }

    /// Check if any data is available
    pub fn has_data(&self) -> bool {
        self.available.is_some()
    }

    /// Get whether this result is degraded
    pub fn is_degraded(&self) -> bool {
        self.metadata.is_degraded
    }
}

// ─── Retry Strategy ─────────────────────────────────────────────────────────

/// Exponential backoff retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Base delay in milliseconds
    pub base_delay_ms: u64,
    /// Maximum backoff delay
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 2_000,
            max_delay_ms: 16_000,
        }
    }
}

impl RetryConfig {
    /// Create a custom retry configuration
    pub fn new(max_attempts: u32, base_delay_ms: u64) -> Self {
        Self {
            max_attempts,
            base_delay_ms,
            max_delay_ms: 16_000,
        }
    }

    /// Calculate delay for a given attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay = self.base_delay_ms * (2_u64.pow(attempt));
        let capped = delay.min(self.max_delay_ms);
        Duration::from_millis(capped)
    }
}

// ─── Degradation Handler ────────────────────────────────────────────────────

/// Handles graceful degradation when APIs fail
pub struct DegradationHandler {
    retry_config: RetryConfig,
    offline_mode: bool,
}

impl DegradationHandler {
    /// Create a new degradation handler
    pub fn new() -> Self {
        Self {
            retry_config: RetryConfig::default(),
            offline_mode: false,
        }
    }

    /// Set custom retry configuration
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Enable offline mode
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline_mode = offline;
        self
    }

    /// Check if offline mode is active
    pub fn is_offline(&self) -> bool {
        self.offline_mode
    }

    /// Classify an error and determine retry strategy
    pub fn classify_error(&self, error_msg: &str) -> ErrorKind {
        ErrorClassifier::classify(error_msg)
    }

    /// Check if an error is retryable
    pub fn is_retryable(&self, error_kind: &ErrorKind) -> bool {
        error_kind.is_retryable()
    }

    /// Create degradation metadata from error
    pub fn create_degradation(&self, error_kind: ErrorKind) -> DegradationMetadata {
        debug!(
            "Creating degradation metadata for error kind: {:?}",
            error_kind
        );
        DegradationMetadata::degraded(error_kind)
    }

    /// Calculate next retry delay
    pub fn next_retry_delay(&self, attempt: u32) -> Duration {
        self.retry_config.delay_for_attempt(attempt)
    }

    /// Get retry configuration
    pub fn retry_config(&self) -> &RetryConfig {
        &self.retry_config
    }

    /// Log degradation event
    pub fn log_degradation(&self, error_kind: &ErrorKind, attempt: u32) {
        warn!(
            error_kind = ?error_kind,
            attempt = attempt,
            "API call degraded: {}",
            error_kind.user_message()
        );
    }

    /// Log recovery attempt
    pub fn log_recovery_attempt(&self, error_kind: &ErrorKind, attempt: u32, delay_ms: u64) {
        info!(
            error_kind = ?error_kind,
            attempt = attempt,
            delay_ms = delay_ms,
            "Retrying after degradation"
        );
    }
}

impl Default for DegradationHandler {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Error Classification Tests

    #[test]
    fn test_classify_rate_limit_429() {
        let kind = ErrorClassifier::classify("HTTP 429: too many requests");
        assert_eq!(kind, ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_rate_limit_529() {
        let kind = ErrorClassifier::classify("HTTP 529: Service Unavailable");
        assert_eq!(kind, ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_rate_limit_text() {
        let kind = ErrorClassifier::classify("Rate limit exceeded. Please wait.");
        assert_eq!(kind, ErrorKind::RateLimit);
    }

    #[test]
    fn test_classify_auth_401() {
        let kind = ErrorClassifier::classify("HTTP 401: Unauthorized");
        assert_eq!(kind, ErrorKind::AuthError);
    }

    #[test]
    fn test_classify_auth_403() {
        let kind = ErrorClassifier::classify("HTTP 403: Forbidden");
        assert_eq!(kind, ErrorKind::AuthError);
    }

    #[test]
    fn test_classify_auth_api_key() {
        let kind = ErrorClassifier::classify("Invalid API key provided");
        assert_eq!(kind, ErrorKind::AuthError);
    }

    #[test]
    fn test_classify_timeout() {
        let kind = ErrorClassifier::classify("Request timed out after 30 seconds");
        assert_eq!(kind, ErrorKind::Timeout);
    }

    #[test]
    fn test_classify_network() {
        let kind = ErrorClassifier::classify("Connection refused to api.example.com");
        assert_eq!(kind, ErrorKind::NetworkError);
    }

    #[test]
    fn test_classify_context_too_long() {
        let kind = ErrorClassifier::classify("Context limit exceeded: 4096 tokens maximum");
        assert_eq!(kind, ErrorKind::ContextTooLong);
    }

    #[test]
    fn test_classify_context_too_long_variant() {
        let kind = ErrorClassifier::classify("Input context too long for model");
        assert_eq!(kind, ErrorKind::ContextTooLong);
    }

    #[test]
    fn test_classify_model_unavailable() {
        let kind = ErrorClassifier::classify("Model gpt-5 not found");
        assert_eq!(kind, ErrorKind::ModelUnavailable);
    }

    #[test]
    fn test_classify_invalid_request() {
        let kind = ErrorClassifier::classify("HTTP 400: Bad Request");
        assert_eq!(kind, ErrorKind::InvalidRequest);
    }

    #[test]
    fn test_classify_unknown() {
        let kind = ErrorClassifier::classify("Something weird happened");
        assert_eq!(kind, ErrorKind::Unknown);
    }

    // Error Severity Tests

    #[test]
    fn test_rate_limit_is_transient() {
        assert_eq!(ErrorKind::RateLimit.severity(), ErrorSeverity::Transient);
    }

    #[test]
    fn test_auth_error_is_permanent() {
        assert_eq!(ErrorKind::AuthError.severity(), ErrorSeverity::Permanent);
    }

    #[test]
    fn test_network_error_is_network() {
        assert_eq!(ErrorKind::NetworkError.severity(), ErrorSeverity::Network);
    }

    #[test]
    fn test_timeout_is_retryable() {
        assert!(ErrorKind::Timeout.is_retryable());
    }

    #[test]
    fn test_auth_error_not_retryable() {
        assert!(!ErrorKind::AuthError.is_retryable());
    }

    #[test]
    fn test_context_too_long_not_retryable() {
        assert!(!ErrorKind::ContextTooLong.is_retryable());
    }

    // Partial Result Tests

    #[test]
    fn test_partial_result_success() {
        let result = PartialResult::success("data".to_string());
        assert!(result.has_data());
        assert!(!result.is_degraded());
        assert_eq!(result.available, Some("data".to_string()));
    }

    #[test]
    fn test_partial_result_degraded() {
        let result = PartialResult::<String>::degraded(None, ErrorKind::NetworkError);
        assert!(!result.has_data());
        assert!(result.is_degraded());
    }

    #[test]
    fn test_partial_result_with_unavailable() {
        let result = PartialResult::success("partial_data".to_string())
            .unavailable("llm_summary", "API unavailable");
        assert!(result.has_data());
        assert_eq!(result.unavailable.len(), 1);
        assert!(result.unavailable.contains_key("llm_summary"));
    }

    #[test]
    fn test_degradation_metadata_ok() {
        let metadata = DegradationMetadata::ok();
        assert!(!metadata.is_degraded);
        assert!(metadata.error_kind.is_none());
    }

    #[test]
    fn test_degradation_metadata_degraded() {
        let metadata = DegradationMetadata::degraded(ErrorKind::RateLimit);
        assert!(metadata.is_degraded);
        assert_eq!(metadata.error_kind, Some(ErrorKind::RateLimit));
        assert!(metadata.degraded_at.is_some());
    }

    #[test]
    fn test_degradation_metadata_from_cache() {
        let metadata = DegradationMetadata::degraded(ErrorKind::NetworkError).from_cache_fallback();
        assert!(metadata.from_cache);
    }

    // Retry Config Tests

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 2_000);
    }

    #[test]
    fn test_retry_backoff_exponential() {
        let config = RetryConfig::default();
        let d0 = config.delay_for_attempt(0);
        let d1 = config.delay_for_attempt(1);
        let d2 = config.delay_for_attempt(2);

        assert_eq!(d0, Duration::from_secs(2));
        assert_eq!(d1, Duration::from_secs(4));
        assert_eq!(d2, Duration::from_secs(8));
    }

    #[test]
    fn test_retry_backoff_capped() {
        let config = RetryConfig::default();
        let d10 = config.delay_for_attempt(10);
        assert!(d10 <= Duration::from_secs(16));
    }

    // Degradation Handler Tests

    #[test]
    fn test_handler_new() {
        let handler = DegradationHandler::new();
        assert!(!handler.is_offline());
    }

    #[test]
    fn test_handler_offline_mode() {
        let handler = DegradationHandler::new().offline(true);
        assert!(handler.is_offline());
    }

    #[test]
    fn test_handler_classify_error() {
        let handler = DegradationHandler::new();
        let kind = handler.classify_error("HTTP 429: rate limited");
        assert_eq!(kind, ErrorKind::RateLimit);
    }

    #[test]
    fn test_handler_is_retryable() {
        let handler = DegradationHandler::new();
        assert!(handler.is_retryable(&ErrorKind::Timeout));
        assert!(!handler.is_retryable(&ErrorKind::AuthError));
    }

    #[test]
    fn test_handler_custom_retry_config() {
        let config = RetryConfig::new(5, 1_000);
        let handler = DegradationHandler::new().with_retry_config(config);
        assert_eq!(handler.retry_config().max_attempts, 5);
        assert_eq!(handler.retry_config().base_delay_ms, 1_000);
    }
}
