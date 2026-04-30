// ── Recovery Strategy and Error Category ──────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::fmt;

/// Recovery strategy to apply when an error occurs.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Retry the operation with exponential backoff.
    Retry,

    /// Skip the failed operation and continue (for non-critical operations).
    Skip,

    /// Abort execution immediately (for critical failures).
    Abort,

    /// Use an alternative implementation or cached result.
    Fallback,
}

impl RecoveryStrategy {
    /// Get all available recovery strategies.
    pub fn all() -> Vec<Self> {
        vec![Self::Retry, Self::Skip, Self::Abort, Self::Fallback]
    }

    /// Check if this strategy allows execution to continue.
    pub fn can_continue(&self) -> bool {
        matches!(self, Self::Skip | Self::Fallback | Self::Retry)
    }

    /// Check if this strategy stops execution.
    pub fn stops_execution(&self) -> bool {
        matches!(self, Self::Abort)
    }
}

impl fmt::Display for RecoveryStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retry => write!(f, "Retry"),
            Self::Skip => write!(f, "Skip"),
            Self::Abort => write!(f, "Abort"),
            Self::Fallback => write!(f, "Fallback"),
        }
    }
}

/// Classification of errors for determining recovery strategy.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Temporary network or I/O errors that can be retried.
    Transient,

    /// Data validation errors (retry won't help).
    Validation,

    /// Missing resources or dependencies.
    ResourceNotFound,

    /// Permission or authorization errors.
    Authorization,

    /// Critical system errors requiring abort.
    Critical,

    /// Unknown or uncategorized errors.
    Unknown,
}

impl ErrorCategory {
    /// Get default recovery strategy for this error category.
    pub fn default_strategy(&self) -> RecoveryStrategy {
        match self {
            Self::Transient => RecoveryStrategy::Retry,
            Self::Validation => RecoveryStrategy::Skip,
            Self::ResourceNotFound => RecoveryStrategy::Fallback,
            Self::Authorization => RecoveryStrategy::Abort,
            Self::Critical => RecoveryStrategy::Abort,
            Self::Unknown => RecoveryStrategy::Retry,
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transient => write!(f, "Transient"),
            Self::Validation => write!(f, "Validation"),
            Self::ResourceNotFound => write!(f, "ResourceNotFound"),
            Self::Authorization => write!(f, "Authorization"),
            Self::Critical => write!(f, "Critical"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_display() {
        assert_eq!(RecoveryStrategy::Retry.to_string(), "Retry");
        assert_eq!(RecoveryStrategy::Skip.to_string(), "Skip");
        assert_eq!(RecoveryStrategy::Abort.to_string(), "Abort");
        assert_eq!(RecoveryStrategy::Fallback.to_string(), "Fallback");
    }

    #[test]
    fn test_recovery_strategy_can_continue() {
        assert!(RecoveryStrategy::Retry.can_continue());
        assert!(RecoveryStrategy::Skip.can_continue());
        assert!(RecoveryStrategy::Fallback.can_continue());
        assert!(!RecoveryStrategy::Abort.can_continue());
    }

    #[test]
    fn test_error_category_default_strategy() {
        assert_eq!(
            ErrorCategory::Transient.default_strategy(),
            RecoveryStrategy::Retry
        );
        assert_eq!(
            ErrorCategory::Validation.default_strategy(),
            RecoveryStrategy::Skip
        );
        assert_eq!(
            ErrorCategory::ResourceNotFound.default_strategy(),
            RecoveryStrategy::Fallback
        );
        assert_eq!(
            ErrorCategory::Authorization.default_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            ErrorCategory::Critical.default_strategy(),
            RecoveryStrategy::Abort
        );
    }

    #[test]
    fn recovery_strategy_serde_roundtrip() {
        let strategies = RecoveryStrategy::all();
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let decoded: RecoveryStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(*s, decoded);
        }
    }

    #[test]
    fn error_category_serde_roundtrip() {
        let categories = [
            ErrorCategory::Transient,
            ErrorCategory::Validation,
            ErrorCategory::ResourceNotFound,
            ErrorCategory::Authorization,
            ErrorCategory::Critical,
            ErrorCategory::Unknown,
        ];
        for c in &categories {
            let json = serde_json::to_string(c).unwrap();
            let decoded: ErrorCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(*c, decoded);
        }
    }

    #[test]
    fn recovery_strategy_all_returns_four() {
        assert_eq!(RecoveryStrategy::all().len(), 4);
    }

    #[test]
    fn recovery_strategy_stops_execution() {
        assert!(RecoveryStrategy::Abort.stops_execution());
        assert!(!RecoveryStrategy::Retry.stops_execution());
        assert!(!RecoveryStrategy::Skip.stops_execution());
        assert!(!RecoveryStrategy::Fallback.stops_execution());
    }

    #[test]
    fn error_category_display() {
        assert_eq!(ErrorCategory::Transient.to_string(), "Transient");
        assert_eq!(ErrorCategory::Validation.to_string(), "Validation");
        assert_eq!(
            ErrorCategory::ResourceNotFound.to_string(),
            "ResourceNotFound"
        );
        assert_eq!(ErrorCategory::Authorization.to_string(), "Authorization");
        assert_eq!(ErrorCategory::Critical.to_string(), "Critical");
        assert_eq!(ErrorCategory::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn error_category_unknown_default_strategy_is_retry() {
        assert_eq!(
            ErrorCategory::Unknown.default_strategy(),
            RecoveryStrategy::Retry
        );
    }
}
