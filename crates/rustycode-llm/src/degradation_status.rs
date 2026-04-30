//! Status Integration for Graceful Degradation
//!
//! Provides status indicators and user-facing messages for degraded operations.
//! Integrates with Task 8 status system.

use crate::graceful_degradation::{DegradationMetadata, ErrorKind};
use serde::{Deserialize, Serialize};
use std::fmt;

// ─── Degradation Status ─────────────────────────────────────────────────────

/// Status of an operation considering degradation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OperationStatus {
    /// Operation succeeded fully
    Success,
    /// Operation succeeded partially (degraded)
    PartialSuccess,
    /// Operation failed completely
    Failed,
    /// Operation is retrying after failure
    Retrying,
    /// Operation is operating in offline mode
    Offline,
}

impl fmt::Display for OperationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OperationStatus::Success => write!(f, "Success"),
            OperationStatus::PartialSuccess => write!(f, "Partial Success"),
            OperationStatus::Failed => write!(f, "Failed"),
            OperationStatus::Retrying => write!(f, "Retrying"),
            OperationStatus::Offline => write!(f, "Offline"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "Unknown"),
        }
    }
}

/// Status indicator for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIndicator {
    /// Operation status
    pub status: OperationStatus,
    /// User-facing message
    pub message: String,
    /// Recovery suggestion
    pub suggestion: Option<String>,
    /// Whether result came from cache
    pub from_cache: bool,
    /// Whether operating in offline mode
    pub offline_mode: bool,
    /// Timestamp of last update
    pub updated_at: String,
    /// Icon/badge for UI
    pub badge: String,
}

impl StatusIndicator {
    /// Create a success indicator
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            status: OperationStatus::Success,
            message: message.into(),
            suggestion: None,
            from_cache: false,
            offline_mode: false,
            updated_at: chrono::Local::now().to_rfc3339(),
            badge: "✓".to_string(),
        }
    }

    /// Create a partial success indicator
    pub fn partial_success(message: impl Into<String>, from_cache: bool) -> Self {
        Self {
            status: OperationStatus::PartialSuccess,
            message: message.into(),
            suggestion: Some(
                "Some features unavailable. Using cached or local results.".to_string(),
            ),
            from_cache,
            offline_mode: false,
            updated_at: chrono::Local::now().to_rfc3339(),
            badge: "⚠".to_string(),
        }
    }

    /// Create a failed indicator
    pub fn failed(message: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self {
            status: OperationStatus::Failed,
            message: message.into(),
            suggestion: Some(suggestion.into()),
            from_cache: false,
            offline_mode: false,
            updated_at: chrono::Local::now().to_rfc3339(),
            badge: "✗".to_string(),
        }
    }

    /// Create a retrying indicator
    pub fn retrying(attempt: u32, total_attempts: u32) -> Self {
        Self {
            status: OperationStatus::Retrying,
            message: format!("Retrying... (attempt {}/{})", attempt, total_attempts),
            suggestion: Some("Please wait while the operation is retried.".to_string()),
            from_cache: false,
            offline_mode: false,
            updated_at: chrono::Local::now().to_rfc3339(),
            badge: "↻".to_string(),
        }
    }

    /// Create an offline mode indicator
    pub fn offline(message: impl Into<String>) -> Self {
        Self {
            status: OperationStatus::Offline,
            message: message.into(),
            suggestion: Some(
                "Operating in offline mode with local-only functionality.".to_string(),
            ),
            from_cache: false,
            offline_mode: true,
            updated_at: chrono::Local::now().to_rfc3339(),
            badge: "⊙".to_string(),
        }
    }

    /// Create indicator from degradation metadata
    pub fn from_degradation(metadata: &DegradationMetadata) -> Self {
        if !metadata.is_degraded {
            return StatusIndicator::success("Operation completed successfully");
        }

        let message = metadata
            .error_message
            .clone()
            .unwrap_or_else(|| "Operation degraded".to_string());

        let mut indicator = if metadata.from_cache {
            StatusIndicator::partial_success(message, true)
        } else {
            StatusIndicator::failed(
                message,
                metadata
                    .recovery_suggestion
                    .clone()
                    .unwrap_or_else(|| "Please try again.".to_string()),
            )
        };

        indicator.offline_mode = metadata.offline_mode;
        indicator.updated_at = metadata
            .degraded_at
            .clone()
            .unwrap_or_else(|| chrono::Local::now().to_rfc3339());

        indicator
    }

    /// Get the status bar text
    pub fn status_bar_text(&self) -> String {
        format!("{} {} | {}", self.badge, self.status, self.message)
    }

    /// Get detailed status text
    pub fn detailed_text(&self) -> String {
        let mut text = format!("{}: {}\n", self.status, self.message);

        if let Some(suggestion) = &self.suggestion {
            text.push_str(&format!("Suggestion: {}\n", suggestion));
        }

        if self.from_cache {
            text.push_str("(Using cached results)\n");
        }

        if self.offline_mode {
            text.push_str("(Offline mode - local only)\n");
        }

        text
    }

    /// Check if status indicates success
    pub fn is_success(&self) -> bool {
        matches!(
            self.status,
            OperationStatus::Success | OperationStatus::PartialSuccess
        )
    }

    /// Check if status indicates failure
    pub fn is_failure(&self) -> bool {
        matches!(self.status, OperationStatus::Failed)
    }

    /// Check if status indicates degradation
    pub fn is_degraded(&self) -> bool {
        matches!(
            self.status,
            OperationStatus::PartialSuccess | OperationStatus::Offline
        ) || self.from_cache
    }
}

// ─── Degradation Report ────────────────────────────────────────────────────

/// Report of degraded operation for logging and debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationReport {
    /// Operation name
    pub operation: String,
    /// Error kind if applicable
    pub error_kind: Option<ErrorKind>,
    /// What was available
    pub available: Vec<String>,
    /// What was unavailable
    pub unavailable: Vec<(String, String)>,
    /// Recovery actions taken
    pub recovery_actions: Vec<String>,
    /// Whether recovery succeeded
    pub recovered: bool,
    /// Timestamp
    pub timestamp: String,
}

impl DegradationReport {
    /// Create a new degradation report
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            error_kind: None,
            available: Vec::new(),
            unavailable: Vec::new(),
            recovery_actions: Vec::new(),
            recovered: false,
            timestamp: chrono::Local::now().to_rfc3339(),
        }
    }

    /// Add error kind
    pub fn with_error(mut self, error_kind: ErrorKind) -> Self {
        self.error_kind = Some(error_kind);
        self
    }

    /// Mark what was available
    pub fn available(mut self, features: Vec<String>) -> Self {
        self.available = features;
        self
    }

    /// Add unavailable feature
    pub fn unavailable(mut self, feature: impl Into<String>, reason: impl Into<String>) -> Self {
        self.unavailable.push((feature.into(), reason.into()));
        self
    }

    /// Record recovery action
    pub fn recovery_action(mut self, action: impl Into<String>) -> Self {
        self.recovery_actions.push(action.into());
        self
    }

    /// Mark as recovered
    pub fn recovered(mut self, recovered: bool) -> Self {
        self.recovered = recovered;
        self
    }

    /// Get summary text
    pub fn summary(&self) -> String {
        let mut text = format!("Operation: {}\n", self.operation);

        if let Some(error_kind) = &self.error_kind {
            text.push_str(&format!("Error: {:?}\n", error_kind));
        }

        if !self.available.is_empty() {
            text.push_str("Available:\n");
            for item in &self.available {
                text.push_str(&format!("  - {}\n", item));
            }
        }

        if !self.unavailable.is_empty() {
            text.push_str("Unavailable:\n");
            for (feature, reason) in &self.unavailable {
                text.push_str(&format!("  - {} ({})\n", feature, reason));
            }
        }

        if !self.recovery_actions.is_empty() {
            text.push_str("Recovery Actions:\n");
            for action in &self.recovery_actions {
                text.push_str(&format!("  - {}\n", action));
            }
        }

        text.push_str(&format!("Recovered: {}\n", self.recovered));

        text
    }
}

// ─── User Guidance ────────────────────────────────────────────────────────

/// User guidance for recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryGuidance {
    /// Immediate action to take
    pub immediate_action: String,
    /// Why this action
    pub reason: String,
    /// Estimated recovery time
    pub estimated_time: Option<String>,
    /// Additional resources
    pub resources: Vec<String>,
}

impl RecoveryGuidance {
    /// Create guidance for rate limit error
    pub fn rate_limit() -> Self {
        Self {
            immediate_action: "Wait a few seconds before retrying".to_string(),
            reason: "API is temporarily overloaded".to_string(),
            estimated_time: Some("2-30 seconds".to_string()),
            resources: vec![
                "Check API status page".to_string(),
                "Reduce request frequency".to_string(),
                "Increase timeout".to_string(),
            ],
        }
    }

    /// Create guidance for auth error
    pub fn auth_error() -> Self {
        Self {
            immediate_action: "Check and refresh your API key".to_string(),
            reason: "API authentication failed".to_string(),
            estimated_time: None,
            resources: vec![
                "Verify API key is correct".to_string(),
                "Check API key permissions".to_string(),
                "Regenerate API key if necessary".to_string(),
            ],
        }
    }

    /// Create guidance for network error
    pub fn network_error() -> Self {
        Self {
            immediate_action: "Check your internet connection".to_string(),
            reason: "Network connectivity issue detected".to_string(),
            estimated_time: Some("Varies".to_string()),
            resources: vec![
                "Verify internet connection".to_string(),
                "Check firewall settings".to_string(),
                "Try alternate network if available".to_string(),
            ],
        }
    }

    /// Create guidance for timeout
    pub fn timeout() -> Self {
        Self {
            immediate_action: "Increase timeout or retry with smaller input".to_string(),
            reason: "Request took too long to complete".to_string(),
            estimated_time: Some("Retry after timeout period".to_string()),
            resources: vec![
                "Reduce input size".to_string(),
                "Increase timeout setting".to_string(),
                "Check API service status".to_string(),
            ],
        }
    }

    /// Get formatted guidance text
    pub fn formatted(&self) -> String {
        let mut text = format!("Immediate Action: {}\n", self.immediate_action);
        text.push_str(&format!("Reason: {}\n", self.reason));

        if let Some(time) = &self.estimated_time {
            text.push_str(&format!("Estimated Recovery Time: {}\n", time));
        }

        if !self.resources.is_empty() {
            text.push_str("Additional Resources:\n");
            for resource in &self.resources {
                text.push_str(&format!("  • {}\n", resource));
            }
        }

        text
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graceful_degradation::ErrorKind as DegradationErrorKind;

    #[test]
    fn test_operation_status_display() {
        assert_eq!(OperationStatus::Success.to_string(), "Success");
        assert_eq!(
            OperationStatus::PartialSuccess.to_string(),
            "Partial Success"
        );
        assert_eq!(OperationStatus::Failed.to_string(), "Failed");
    }

    #[test]
    fn test_status_indicator_success() {
        let indicator = StatusIndicator::success("All good");
        assert_eq!(indicator.status, OperationStatus::Success);
        assert_eq!(indicator.badge, "✓");
        assert!(indicator.is_success());
        assert!(!indicator.is_failure());
    }

    #[test]
    fn test_status_indicator_partial_success() {
        let indicator = StatusIndicator::partial_success("Partial", false);
        assert_eq!(indicator.status, OperationStatus::PartialSuccess);
        assert!(indicator.is_success());
        assert!(indicator.is_degraded());
    }

    #[test]
    fn test_status_indicator_failed() {
        let indicator = StatusIndicator::failed("Failed operation", "Try again");
        assert_eq!(indicator.status, OperationStatus::Failed);
        assert!(indicator.is_failure());
    }

    #[test]
    fn test_status_indicator_retrying() {
        let indicator = StatusIndicator::retrying(2, 3);
        assert_eq!(indicator.status, OperationStatus::Retrying);
        assert!(indicator.message.contains("2/3"));
    }

    #[test]
    fn test_status_indicator_offline() {
        let indicator = StatusIndicator::offline("Working offline");
        assert_eq!(indicator.status, OperationStatus::Offline);
        assert!(indicator.offline_mode);
    }

    #[test]
    fn test_status_bar_text() {
        let indicator = StatusIndicator::success("Complete");
        let text = indicator.status_bar_text();
        assert!(text.contains("✓"));
        assert!(text.contains("Success"));
        assert!(text.contains("Complete"));
    }

    #[test]
    fn test_degradation_report_new() {
        let report = DegradationReport::new("test_op");
        assert_eq!(report.operation, "test_op");
        assert!(!report.recovered);
    }

    #[test]
    fn test_degradation_report_builder() {
        let report = DegradationReport::new("api_call")
            .with_error(DegradationErrorKind::RateLimit)
            .available(vec!["cached_data".to_string()])
            .unavailable("realtime_update", "rate limited")
            .recovery_action("retry after 5s")
            .recovered(false);

        assert_eq!(report.operation, "api_call");
        assert_eq!(report.available.len(), 1);
        assert_eq!(report.unavailable.len(), 1);
        assert_eq!(report.recovery_actions.len(), 1);
    }

    #[test]
    fn test_recovery_guidance_rate_limit() {
        let guidance = RecoveryGuidance::rate_limit();
        assert!(guidance.immediate_action.contains("Wait"));
        assert!(!guidance.resources.is_empty());
    }

    #[test]
    fn test_recovery_guidance_auth_error() {
        let guidance = RecoveryGuidance::auth_error();
        assert!(guidance.immediate_action.contains("API key"));
    }

    #[test]
    fn test_recovery_guidance_network_error() {
        let guidance = RecoveryGuidance::network_error();
        assert!(guidance.immediate_action.contains("internet"));
    }

    #[test]
    fn test_recovery_guidance_timeout() {
        let guidance = RecoveryGuidance::timeout();
        assert!(
            guidance.immediate_action.contains("timeout")
                || guidance.immediate_action.contains("smaller")
        );
    }

    #[test]
    fn test_recovery_guidance_formatted() {
        let guidance = RecoveryGuidance::rate_limit();
        let text = guidance.formatted();
        assert!(text.contains("Immediate Action"));
        assert!(text.contains("Reason"));
        assert!(text.contains("Resources"));
    }
}
