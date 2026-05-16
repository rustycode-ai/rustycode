//! Retry state tracking for task execution.
//!
//! Tracks current retry count, last error, and timing for backoff calculation.
//! Moved here from `rustycode-orchestration::delegation` so that `rustycode-core`
//! can use it without depending on orchestration.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// Error Classification

/// Categorizes errors to determine retry vs. escalation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCategory {
    /// Transient: auto-retry with backoff.
    RateLimit429,
    /// Transient: auto-retry with backoff.
    Timeout,
    /// Transient: auto-retry with backoff.
    ServerError5xx,
    /// Persistent: escalate to parent for decision.
    BadRequest400,
    /// Persistent: escalate to parent for decision.
    InvalidDelegation,
    /// Persistent: escalate to parent for decision.
    PermissionDenied,
    /// Persistent: escalate to parent for decision.
    ContextWindow,
}

impl ErrorCategory {
    /// True if this error should trigger automatic retry with backoff.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimit429 | Self::Timeout | Self::ServerError5xx
        )
    }

    /// True if this error should escalate to parent conversation.
    pub fn is_persistent(self) -> bool {
        !self.is_transient()
    }
}

// RetryState

/// Tracks retry state across a task execution lifecycle.
///
/// Tracks current retry count, last error, and timing. Lives in ExecutionContext
/// and is mutable across the lifetime of a task execution.
#[derive(Debug, Clone)]
pub struct RetryState {
    /// Current retry count for the last error type.
    pub current_error_retries: u32,
    /// Category of the last error encountered (None if no error yet).
    pub last_error: Option<ErrorCategory>,
    /// Timestamp of the last error (used for retry backoff calculation).
    pub last_error_at: Option<SystemTime>,
}

impl Default for RetryState {
    fn default() -> Self {
        Self::new()
    }
}

impl RetryState {
    /// Create a fresh retry state for a new task execution.
    pub fn new() -> Self {
        Self {
            current_error_retries: 0,
            last_error: None,
            last_error_at: None,
        }
    }

    /// Check if this error should be automatically retried.
    ///
    /// Returns true if:
    /// 1. Error is transient (429, timeout, 5xx)
    /// 2. Retries haven't been exhausted yet
    /// 3. Either it's a new error type, or same error with retries remaining
    pub fn should_retry(&self, max_retries_per_error: u32, error: ErrorCategory) -> bool {
        if !error.is_transient() {
            return false;
        }

        match self.last_error {
            None => true,
            Some(last) if last == error => self.current_error_retries < max_retries_per_error,
            Some(_) => true,
        }
    }

    /// Check if this error should escalate to parent conversation.
    ///
    /// Returns true if:
    /// 1. Error is persistent (400, invalid delegation, etc)
    /// 2. Error is transient but retries exhausted
    pub fn should_escalate(&self, max_retries_per_error: u32, error: ErrorCategory) -> bool {
        if error.is_persistent() {
            return true;
        }

        if !error.is_transient() {
            return false;
        }

        match self.last_error {
            Some(last) if last == error => self.current_error_retries >= max_retries_per_error,
            _ => false,
        }
    }

    /// Calculate exponential backoff delay for the next retry (in milliseconds).
    ///
    /// Uses formula: 2^(retry_count - 1) * 1000ms, capped at 32s.
    /// After the first error, retries=1, so backoff=2^0*1000ms=1000ms.
    /// After the second error, retries=2, so backoff=2^1*1000ms=2000ms.
    pub fn next_backoff_ms(&self) -> u64 {
        if self.current_error_retries == 0 {
            return 0;
        }
        let exponent = self.current_error_retries.saturating_sub(1);
        let base_ms = 2_u64.saturating_pow(exponent);
        (base_ms * 1000).min(32_000)
    }

    /// Record that an error occurred and update retry state.
    ///
    /// If this is a different error type, resets the retry counter.
    pub fn record_error(&mut self, error: ErrorCategory) {
        match self.last_error {
            Some(last) if last == error => {
                self.current_error_retries += 1;
            }
            _ => {
                self.current_error_retries = 1;
                self.last_error = Some(error);
            }
        }
        self.last_error_at = Some(SystemTime::now());
    }

    /// Check if sufficient time has passed since last error for the next retry.
    pub fn is_backoff_satisfied(&self) -> bool {
        match self.last_error_at {
            None => true,
            Some(last_time) => {
                let elapsed = last_time.elapsed().unwrap_or_default().as_millis() as u64;
                elapsed >= self.next_backoff_ms()
            }
        }
    }
}
