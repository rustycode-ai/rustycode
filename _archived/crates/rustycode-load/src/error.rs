//! Error types for the load testing framework

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// Load testing framework errors
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LoadTestError {
    /// Invalid scenario configuration
    #[error("Invalid scenario configuration: {0}")]
    InvalidScenario(String),

    /// Request execution failed
    #[error("Request execution failed: {0}")]
    RequestFailed(String),

    /// Request timeout
    #[error("Request timed out after {0:?}")]
    RequestTimeout(Duration),

    /// Connection error
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// HTTP error
    #[error("HTTP error {status}: {message}")]
    HttpError { status: u16, message: String },

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Runtime error
    #[error("Runtime error: {0}")]
    RuntimeError(String),

    /// Metrics collection error
    #[error("Metrics collection error: {0}")]
    MetricsError(String),

    /// Report generation error
    #[error("Report generation error: {0}")]
    ReportError(String),

    /// Cancellation requested
    #[error("Load test cancelled")]
    Cancelled,
}

/// Result type for load testing operations
pub type Result<T> = std::result::Result<T, LoadTestError>;

/// Error classification for metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// Network-level errors (DNS, connection refused, etc.)
    Network,

    /// HTTP errors (4xx, 5xx)
    Http,

    /// Timeout errors
    Timeout,

    /// Application-level errors (validation, business logic)
    Application,

    /// Serialization errors
    Serialization,

    /// Other uncategorized errors
    Other,
}

impl ErrorCategory {
    /// Categorize an error based on its content
    pub const fn from_error(error: &LoadTestError) -> Self {
        match error {
            LoadTestError::ConnectionError(_) => Self::Network,
            LoadTestError::RequestTimeout(_) => Self::Timeout,
            LoadTestError::HttpError { .. } => Self::Http,
            LoadTestError::SerializationError(_) => Self::Serialization,
            _ => Self::Other,
        }
    }

    /// Get a human-readable name
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Network => "Network",
            Self::Http => "HTTP",
            Self::Timeout => "Timeout",
            Self::Application => "Application",
            Self::Serialization => "Serialization",
            Self::Other => "Other",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category_classification() {
        let conn_err = LoadTestError::ConnectionError("refused".to_string());
        assert_eq!(ErrorCategory::from_error(&conn_err), ErrorCategory::Network);

        let timeout_err = LoadTestError::RequestTimeout(Duration::from_secs(30));
        assert_eq!(
            ErrorCategory::from_error(&timeout_err),
            ErrorCategory::Timeout
        );

        let http_err = LoadTestError::HttpError {
            status: 500,
            message: "Internal Server Error".to_string(),
        };
        assert_eq!(ErrorCategory::from_error(&http_err), ErrorCategory::Http);
    }

    #[test]
    fn test_error_category_names() {
        assert_eq!(ErrorCategory::Network.name(), "Network");
        assert_eq!(ErrorCategory::Http.name(), "HTTP");
        assert_eq!(ErrorCategory::Timeout.name(), "Timeout");
    }

    #[test]
    fn test_load_test_error_display() {
        let err = LoadTestError::InvalidScenario("bad config".into());
        assert!(err.to_string().contains("bad config"));

        let err = LoadTestError::RequestFailed("connection reset".into());
        assert!(err.to_string().contains("connection reset"));

        let err = LoadTestError::RuntimeError("thread panicked".into());
        assert!(err.to_string().contains("thread panicked"));

        let err = LoadTestError::MetricsError("overflow".into());
        assert!(err.to_string().contains("overflow"));

        let err = LoadTestError::ReportError("disk full".into());
        assert!(err.to_string().contains("disk full"));

        let err = LoadTestError::SerializationError("invalid json".into());
        assert!(err.to_string().contains("invalid json"));

        let err = LoadTestError::Cancelled;
        assert!(err.to_string().contains("cancelled"));
    }

    #[test]
    fn test_http_error_fields() {
        let err = LoadTestError::HttpError {
            status: 429,
            message: "Rate limited".into(),
        };
        assert!(err.to_string().contains("429"));
        assert!(err.to_string().contains("Rate limited"));
    }

    #[test]
    fn test_timeout_error_duration() {
        let err = LoadTestError::RequestTimeout(Duration::from_secs(5));
        let msg = err.to_string();
        assert!(msg.contains('5'));
    }

    #[test]
    fn test_error_category_from_serialization() {
        let err = LoadTestError::SerializationError("parse error".into());
        assert_eq!(
            ErrorCategory::from_error(&err),
            ErrorCategory::Serialization
        );
    }

    #[test]
    fn test_error_category_from_other() {
        let err = LoadTestError::InvalidScenario("x".into());
        assert_eq!(ErrorCategory::from_error(&err), ErrorCategory::Other);

        let err = LoadTestError::RuntimeError("x".into());
        assert_eq!(ErrorCategory::from_error(&err), ErrorCategory::Other);
    }

    #[test]
    fn test_error_category_all_names() {
        assert_eq!(ErrorCategory::Application.name(), "Application");
        assert_eq!(ErrorCategory::Serialization.name(), "Serialization");
        assert_eq!(ErrorCategory::Other.name(), "Other");
    }

    #[test]
    fn test_error_category_serde_roundtrip() {
        for cat in &[
            ErrorCategory::Network,
            ErrorCategory::Http,
            ErrorCategory::Timeout,
            ErrorCategory::Application,
            ErrorCategory::Serialization,
            ErrorCategory::Other,
        ] {
            let json = serde_json::to_string(cat).unwrap();
            let decoded: ErrorCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(*cat, decoded);
        }
    }
}
