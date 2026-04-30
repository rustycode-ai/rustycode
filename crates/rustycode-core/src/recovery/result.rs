// ── Recovery Result and Logging ───────────────────────────────────────────────

use super::strategy::RecoveryStrategy;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Log entry for a recovery attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryLogEntry {
    /// Timestamp of the recovery attempt.
    pub timestamp: DateTime<Utc>,

    /// Attempt number.
    pub attempt: u32,

    /// Recovery strategy used.
    pub strategy: RecoveryStrategy,

    /// Error that triggered recovery (if any).
    pub error: Option<String>,

    /// Action taken.
    pub action: String,

    /// Duration of this recovery attempt.
    pub duration: Duration,

    /// Whether this attempt was successful.
    pub success: bool,
}

impl RecoveryLogEntry {
    /// Create a new recovery log entry.
    pub fn new(
        attempt: u32,
        strategy: RecoveryStrategy,
        error: Option<String>,
        action: String,
        duration: Duration,
        success: bool,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            attempt,
            strategy,
            error,
            action,
            duration,
            success,
        }
    }

    /// Format the log entry for display.
    pub fn format(&self) -> String {
        let status = if self.success { "✓" } else { "✗" };
        format!(
            "{} [Attempt {}] {} - {} ({}ms) - {}",
            status,
            self.attempt,
            self.strategy,
            self.action,
            self.duration.as_millis(),
            self.error.as_deref().unwrap_or("No error")
        )
    }
}

/// Result of a recovery attempt.
pub struct RecoveryResult<T> {
    /// The final result (successful or last error).
    pub result: anyhow::Result<T>,

    /// The recovery strategy that was used.
    pub strategy_used: RecoveryStrategy,

    /// Number of attempts made (including retries).
    pub attempts: u32,

    /// Total time spent on recovery.
    pub duration: Duration,

    /// Recovery log entries.
    pub log: Vec<RecoveryLogEntry>,
}

impl<T> RecoveryResult<T> {
    /// Create a new recovery result.
    pub fn new(
        result: anyhow::Result<T>,
        strategy_used: RecoveryStrategy,
        attempts: u32,
        duration: Duration,
        log: Vec<RecoveryLogEntry>,
    ) -> Self {
        Self {
            result,
            strategy_used,
            attempts,
            duration,
            log,
        }
    }

    /// Check if recovery was successful.
    pub fn is_success(&self) -> bool {
        self.result.is_ok()
    }

    /// Get the recovery strategy that was used.
    pub fn strategy_used(&self) -> RecoveryStrategy {
        self.strategy_used
    }

    /// Get the number of attempts made.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Get the total duration of recovery attempts.
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// Get the recovery log.
    pub fn log(&self) -> &[RecoveryLogEntry] {
        &self.log
    }

    /// Extract the successful result.
    pub fn into_result(self) -> anyhow::Result<T> {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_log_entry_format() {
        let entry = RecoveryLogEntry::new(
            1,
            RecoveryStrategy::Retry,
            Some("Test error".to_string()),
            "Test action".to_string(),
            Duration::from_millis(100),
            true,
        );

        let formatted = entry.format();
        assert!(formatted.contains("✓"));
        assert!(formatted.contains("Attempt 1"));
        assert!(formatted.contains("Retry"));
        assert!(formatted.contains("Test action"));
        assert!(formatted.contains("100ms"));
    }

    #[test]
    fn test_recovery_result_accessors() {
        let result: RecoveryResult<()> = RecoveryResult::new(
            Ok(()),
            RecoveryStrategy::Retry,
            3,
            Duration::from_millis(500),
            vec![],
        );

        assert!(result.is_success());
        assert_eq!(result.strategy_used(), RecoveryStrategy::Retry);
        assert_eq!(result.attempts(), 3);
        assert_eq!(result.duration(), Duration::from_millis(500));
        assert!(result.log().is_empty());
        assert!(result.into_result().is_ok());
    }

    #[test]
    fn test_recovery_result_failure() {
        let result: RecoveryResult<()> = RecoveryResult::new(
            Err(anyhow::anyhow!("recovery failed")),
            RecoveryStrategy::Abort,
            5,
            Duration::from_secs(2),
            vec![],
        );

        assert!(!result.is_success());
        assert_eq!(result.attempts(), 5);
        assert_eq!(result.duration(), Duration::from_secs(2));
        assert!(result.into_result().is_err());
    }

    #[test]
    fn test_recovery_log_entry_failure_format() {
        let entry = RecoveryLogEntry::new(
            3,
            RecoveryStrategy::Fallback,
            None,
            "used fallback approach".to_string(),
            Duration::from_millis(250),
            false,
        );

        let formatted = entry.format();
        assert!(formatted.contains("✗"), "failed entry should show X");
        assert!(formatted.contains("Attempt 3"));
        assert!(formatted.contains("Fallback"));
        assert!(formatted.contains("250ms"));
        assert!(formatted.contains("No error"));
    }

    #[test]
    fn test_recovery_log_entry_timestamp() {
        let entry = RecoveryLogEntry::new(
            1,
            RecoveryStrategy::Retry,
            None,
            "test".to_string(),
            Duration::from_millis(0),
            true,
        );
        // Timestamp should be near now
        let diff = Utc::now().signed_duration_since(entry.timestamp);
        assert!(diff.num_seconds() <= 1);
    }

    #[test]
    fn test_recovery_result_with_log_entries() {
        let log = vec![
            RecoveryLogEntry::new(
                1,
                RecoveryStrategy::Retry,
                Some("timeout".to_string()),
                "retried".to_string(),
                Duration::from_millis(100),
                false,
            ),
            RecoveryLogEntry::new(
                2,
                RecoveryStrategy::Retry,
                None,
                "retried".to_string(),
                Duration::from_millis(200),
                true,
            ),
        ];

        let result: RecoveryResult<String> = RecoveryResult::new(
            Ok("success".to_string()),
            RecoveryStrategy::Retry,
            2,
            Duration::from_millis(300),
            log,
        );

        assert!(result.is_success());
        assert_eq!(result.log().len(), 2);
        assert!(!result.log()[0].success);
        assert!(result.log()[1].success);
    }

    #[test]
    fn test_recovery_log_entry_serialization() {
        let entry = RecoveryLogEntry::new(
            1,
            RecoveryStrategy::Fallback,
            Some("error msg".to_string()),
            "used fallback".to_string(),
            Duration::from_millis(50),
            true,
        );

        let json = serde_json::to_string(&entry).unwrap();
        let decoded: RecoveryLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.attempt, 1);
        assert!(decoded.success);
        assert_eq!(decoded.error, Some("error msg".to_string()));
    }

    #[test]
    fn test_recovery_result_into_result_extracts_value() {
        let result: RecoveryResult<i32> = RecoveryResult::new(
            Ok(42),
            RecoveryStrategy::Skip,
            1,
            Duration::from_millis(10),
            vec![],
        );

        let inner = result.into_result().unwrap();
        assert_eq!(inner, 42);
    }
}
