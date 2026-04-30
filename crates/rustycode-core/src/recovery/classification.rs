// ── Error Classification ───────────────────────────────────────────────────────

use super::strategy::{ErrorCategory, RecoveryStrategy};
use std::collections::HashMap;

/// Detailed classification of an error.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorClassification {
    /// The category of error.
    pub category: ErrorCategory,

    /// Whether this error is retryable.
    pub retryable: bool,

    /// Suggested recovery strategy.
    pub suggested_strategy: RecoveryStrategy,

    /// Estimated probability of success if retried (0.0 to 1.0).
    pub retry_success_probability: f32,

    /// Human-readable description of why this classification was chosen.
    pub reason: String,
}

impl ErrorClassification {
    /// Create a new error classification.
    pub fn new(
        category: ErrorCategory,
        retryable: bool,
        suggested_strategy: RecoveryStrategy,
        reason: String,
    ) -> Self {
        let retry_success_probability = match category {
            ErrorCategory::Transient if retryable => 0.7,
            ErrorCategory::Transient => 0.3,
            ErrorCategory::Validation => 0.0,
            ErrorCategory::ResourceNotFound => 0.0,
            ErrorCategory::Authorization => 0.0,
            ErrorCategory::Critical => 0.0,
            ErrorCategory::Unknown => 0.4,
        };

        Self {
            category,
            retryable,
            suggested_strategy,
            retry_success_probability,
            reason,
        }
    }

    /// Create classification for transient errors.
    pub fn transient(reason: String) -> Self {
        Self::new(
            ErrorCategory::Transient,
            true,
            RecoveryStrategy::Retry,
            reason,
        )
    }

    /// Create classification for validation errors.
    pub fn validation(reason: String) -> Self {
        Self::new(
            ErrorCategory::Validation,
            false,
            RecoveryStrategy::Skip,
            reason,
        )
    }

    /// Create classification for authorization errors.
    pub fn authorization(reason: String) -> Self {
        Self::new(
            ErrorCategory::Authorization,
            false,
            RecoveryStrategy::Abort,
            reason,
        )
    }

    /// Create classification for critical errors.
    pub fn critical(reason: String) -> Self {
        Self::new(
            ErrorCategory::Critical,
            false,
            RecoveryStrategy::Abort,
            reason,
        )
    }

    /// Create classification for unknown errors.
    pub fn unknown(reason: String) -> Self {
        Self::new(
            ErrorCategory::Unknown,
            true, // Assume retryable for unknown errors
            RecoveryStrategy::Retry,
            reason,
        )
    }
}

/// Error classifier using pattern matching and heuristics.
pub struct ErrorClassifier {
    /// Custom patterns for error classification.
    patterns: HashMap<String, ErrorClassification>,

    /// Default classification for unknown errors.
    default_classification: ErrorClassification,
}

impl ErrorClassifier {
    /// Create a new error classifier with default patterns.
    pub fn new() -> Self {
        let mut classifier = Self {
            patterns: HashMap::new(),
            default_classification: ErrorClassification::unknown(
                "No matching pattern found".to_string(),
            ),
        };

        // Add default error patterns
        classifier.add_default_patterns();
        classifier
    }

    /// Add custom error pattern.
    ///
    /// The pattern is a substring that will be matched against error messages.
    pub fn add_pattern(&mut self, pattern: String, classification: ErrorClassification) {
        self.patterns.insert(pattern, classification);
    }

    /// Set the default classification for unknown errors.
    pub fn set_default_classification(&mut self, classification: ErrorClassification) {
        self.default_classification = classification;
    }

    /// Classify an error based on its message and type.
    pub fn classify(&self, error: &anyhow::Error) -> ErrorClassification {
        let error_msg = error.to_string().to_lowercase();

        // Check custom patterns first
        for (pattern, classification) in &self.patterns {
            if error_msg.contains(&pattern.to_lowercase()) {
                return classification.clone();
            }
        }

        // Check for transient errors
        if self.is_transient(&error_msg) {
            return ErrorClassification::transient(
                "Error appears to be transient (network, timeout, temporary failure)".to_string(),
            );
        }

        // Check for validation errors
        if self.is_validation_error(&error_msg) {
            return ErrorClassification::validation(
                "Error appears to be a validation error (invalid data, malformed input)"
                    .to_string(),
            );
        }

        // Check for authorization errors
        if self.is_authorization_error(&error_msg) {
            return ErrorClassification::authorization(
                "Error appears to be an authorization failure (permission denied)".to_string(),
            );
        }

        // Check for critical errors
        if self.is_critical(&error_msg) {
            return ErrorClassification::critical(
                "Error appears to be critical (corruption, security, system failure)".to_string(),
            );
        }

        // Use default classification
        self.default_classification.clone()
    }

    /// Check if error message indicates a transient error.
    fn is_transient(&self, msg: &str) -> bool {
        let transient_indicators = [
            "timeout",
            "timed out",
            "connection refused",
            "connection reset",
            "network",
            "temporary",
            "unavailable",
            "try again",
            "rate limit",
            "too many requests",
            "service unavailable",
            "gateway timeout",
            "deadline exceeded",
        ];

        transient_indicators
            .iter()
            .any(|&indicator| msg.contains(indicator))
    }

    /// Check if error message indicates a validation error.
    fn is_validation_error(&self, msg: &str) -> bool {
        let validation_indicators = [
            "invalid",
            "malformed",
            "validation",
            "format",
            "syntax",
            "parse",
            "not found",
            "does not exist",
            "no such",
        ];

        validation_indicators
            .iter()
            .any(|&indicator| msg.contains(indicator))
    }

    /// Check if error message indicates an authorization error.
    fn is_authorization_error(&self, msg: &str) -> bool {
        let auth_indicators = [
            "permission denied",
            "unauthorized",
            "forbidden",
            "access denied",
            "not allowed",
            "authentication",
        ];

        auth_indicators
            .iter()
            .any(|&indicator| msg.contains(indicator))
    }

    /// Check if error message indicates a critical error.
    fn is_critical(&self, msg: &str) -> bool {
        let critical_indicators = [
            "corruption",
            "corrupt",
            "security",
            "breach",
            "compromised",
            "fatal",
            "panic",
            "internal error",
        ];

        critical_indicators
            .iter()
            .any(|&indicator| msg.contains(indicator))
    }

    /// Add default error patterns.
    fn add_default_patterns(&mut self) {
        // Network/transient errors
        self.add_pattern(
            "ECONNREFUSED".to_string(),
            ErrorClassification::transient("Connection refused - likely transient".to_string()),
        );
        self.add_pattern(
            "ETIMEDOUT".to_string(),
            ErrorClassification::transient("Operation timed out - likely transient".to_string()),
        );

        // File system errors
        self.add_pattern(
            "No such file".to_string(),
            ErrorClassification::new(
                ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "File not found".to_string(),
            ),
        );

        // Permission errors
        self.add_pattern(
            "Permission denied".to_string(),
            ErrorClassification::authorization("Insufficient permissions".to_string()),
        );
    }
}

impl Default for ErrorClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification_creation() {
        let classification = ErrorClassification::transient("Network error".to_string());
        assert_eq!(classification.category, ErrorCategory::Transient);
        assert!(classification.retryable);
        assert_eq!(classification.suggested_strategy, RecoveryStrategy::Retry);

        let classification = ErrorClassification::validation("Invalid input".to_string());
        assert_eq!(classification.category, ErrorCategory::Validation);
        assert!(!classification.retryable);
        assert_eq!(classification.suggested_strategy, RecoveryStrategy::Skip);
    }

    #[test]
    fn test_error_classifier_transient() {
        let classifier = ErrorClassifier::new();
        let error = anyhow::anyhow!("Connection timeout");
        let classification = classifier.classify(&error);

        assert_eq!(classification.category, ErrorCategory::Transient);
        assert!(classification.retryable);
    }

    #[test]
    fn test_error_classifier_validation() {
        let classifier = ErrorClassifier::new();
        let error = anyhow::anyhow!("Invalid JSON format");
        let classification = classifier.classify(&error);

        assert_eq!(classification.category, ErrorCategory::Validation);
        assert!(!classification.retryable);
    }

    #[test]
    fn test_error_classifier_authorization() {
        let classifier = ErrorClassifier::new();
        let error = anyhow::anyhow!("Permission denied");
        let classification = classifier.classify(&error);

        assert_eq!(classification.category, ErrorCategory::Authorization);
        assert!(!classification.retryable);
        assert_eq!(classification.suggested_strategy, RecoveryStrategy::Abort);
    }

    #[test]
    fn test_error_classifier_custom_pattern() {
        let mut classifier = ErrorClassifier::new();
        classifier.add_pattern(
            "ECONNREFUSED".to_string(),
            ErrorClassification::transient("Custom pattern".to_string()),
        );

        let error = anyhow::anyhow!("ECONNREFUSED: Connection refused");
        let classification = classifier.classify(&error);

        assert_eq!(classification.category, ErrorCategory::Transient);
    }
}
