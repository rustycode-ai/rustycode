// ── Recovery Configuration ─────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for retry behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_attempts: u32,

    /// Initial backoff duration.
    pub initial_backoff: Duration,

    /// Maximum backoff duration.
    pub max_backoff: Duration,

    /// Backoff multiplier for exponential backoff.
    pub backoff_multiplier: f64,

    /// Whether to add jitter to backoff durations.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Calculate backoff duration for a given attempt number.
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let base_ms = self.initial_backoff.as_millis() as f64;
        let exponential = self.backoff_multiplier.powi(attempt as i32);
        let backoff_ms = (base_ms * exponential).min(self.max_backoff.as_millis() as f64);

        let mut duration = Duration::from_millis(backoff_ms as u64);

        // Add jitter if enabled
        if self.jitter {
            let jitter_ms = (backoff_ms * 0.1) as u64; // 10% jitter
            let random_jitter = if jitter_ms == 0 {
                0
            } else {
                rand::random::<u64>() % (2 * jitter_ms)
            };
            let jitter_adjusted = jitter_ms.abs_diff(random_jitter);
            duration = duration.saturating_add(Duration::from_millis(jitter_adjusted));
        }

        duration
    }
}

/// Configuration for the recovery engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Retry configuration.
    pub retry: RetryConfig,

    /// Whether to enable automatic recovery.
    pub enabled: bool,

    /// Maximum total time to spend on recovery attempts.
    pub max_recovery_duration: Duration,

    /// Whether to log recovery attempts.
    pub log_recovery: bool,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            retry: RetryConfig::default(),
            enabled: true,
            max_recovery_duration: Duration::from_secs(30),
            log_recovery: true,
        }
    }
}

impl RecoveryConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum retry attempts.
    pub fn with_max_attempts(mut self, max: u32) -> Self {
        self.retry.max_attempts = max;
        self
    }

    /// Set the initial backoff duration.
    pub fn with_initial_backoff(mut self, duration: Duration) -> Self {
        self.retry.initial_backoff = duration;
        self
    }

    /// Enable or disable automatic recovery.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set the maximum recovery duration.
    pub fn with_max_recovery_duration(mut self, duration: Duration) -> Self {
        self.max_recovery_duration = duration;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_backoff_duration() {
        let config = RetryConfig::default();

        // Exponential backoff (without jitter, it's deterministic)
        let backoff_0 = config.backoff_duration(0);
        let backoff_1 = config.backoff_duration(1);
        let backoff_2 = config.backoff_duration(2);

        // With jitter, exact comparisons are tricky, but we can check general trends
        assert!(backoff_0.as_millis() > 0);
        assert!(backoff_1.as_millis() > 0);
        assert!(backoff_2.as_millis() > 0);

        // Check that backoff durations are reasonable (not astronomical)
        assert!(backoff_0 < Duration::from_secs(1));
        assert!(backoff_2 < Duration::from_secs(10));
    }

    #[test]
    fn test_recovery_config_builder() {
        let config = RecoveryConfig::new()
            .with_max_attempts(5)
            .with_initial_backoff(Duration::from_millis(200))
            .with_enabled(false)
            .with_max_recovery_duration(Duration::from_mins(1));

        assert_eq!(config.retry.max_attempts, 5);
        assert_eq!(config.retry.initial_backoff, Duration::from_millis(200));
        assert!(!config.enabled);
        assert_eq!(config.max_recovery_duration, Duration::from_mins(1));
    }

    #[test]
    fn test_retry_config_default_values() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_backoff, Duration::from_millis(100));
        assert_eq!(config.max_backoff, Duration::from_secs(5));
        assert_eq!(config.backoff_multiplier, 2.0);
        assert!(config.jitter);
    }

    #[test]
    fn test_retry_config_backoff_is_capped() {
        let config = RetryConfig::default();
        // Very high attempt number should not exceed max_backoff by much
        let backoff = config.backoff_duration(100);
        // With jitter, it could be slightly over max_backoff
        assert!(
            backoff <= Duration::from_secs(6),
            "backoff should be near max_backoff"
        );
    }

    #[test]
    fn test_retry_config_no_jitter_deterministic() {
        let config = RetryConfig {
            jitter: false,
            ..RetryConfig::default()
        };
        let b0 = config.backoff_duration(0);
        let b1 = config.backoff_duration(1);
        let b2 = config.backoff_duration(2);
        // Without jitter, should be deterministic and exponentially increasing
        assert!(b1 > b0);
        assert!(b2 > b1);
        // b0 = 100ms * 2^0 = 100ms
        assert_eq!(b0, Duration::from_millis(100));
        // b1 = 100ms * 2^1 = 200ms
        assert_eq!(b1, Duration::from_millis(200));
    }

    #[test]
    fn test_recovery_config_default_values() {
        let config = RecoveryConfig::default();
        assert!(config.enabled);
        assert!(config.log_recovery);
        assert_eq!(config.max_recovery_duration, Duration::from_secs(30));
        assert_eq!(config.retry.max_attempts, 3);
    }

    #[test]
    fn test_recovery_config_new_equals_default() {
        let new_config = RecoveryConfig::new();
        let default_config = RecoveryConfig::default();
        assert_eq!(new_config.enabled, default_config.enabled);
        assert_eq!(
            new_config.retry.max_attempts,
            default_config.retry.max_attempts
        );
    }

    #[test]
    fn test_recovery_config_serialization() {
        let config = RecoveryConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let decoded: RecoveryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.retry.max_attempts, 3);
        assert!(decoded.enabled);
    }

    #[test]
    fn test_retry_config_serialization() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 3.0,
            jitter: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: RetryConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.max_attempts, 5);
        assert_eq!(decoded.initial_backoff, Duration::from_millis(200));
        assert!(!decoded.jitter);
    }
}
