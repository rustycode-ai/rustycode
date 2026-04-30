//! Rate limit countdown handler
//!
//! Manages rate limit state, countdown display, and auto-retry logic.
//! Uses goose-inspired exponential backoff with jitter.

use std::time::Instant;

/// Maximum number of automatic retries before giving up
const MAX_AUTO_RETRIES: usize = 5;

/// Base delay for exponential backoff (seconds)
const BACKOFF_BASE_SECS: u64 = 2;

/// Maximum backoff cap (seconds)
const BACKOFF_MAX_SECS: u64 = 60;

/// Rate limit state and countdown management
pub struct RateLimitHandler {
    /// When the rate limit expires
    pub until: Option<Instant>,
    /// Index of the rate limit message in the messages vector
    pub message_index: Option<usize>,
    /// The last message that was sent (for retry)
    pub last_message: Option<String>,
    /// Number of retry attempts (accumulates across retries, not reset on countdown expiry)
    pub retry_count: usize,
    /// Whether auto-retry was cancelled by user
    pub auto_retry_cancelled: bool,
}

impl RateLimitHandler {
    /// Create a new rate limit handler
    pub fn new() -> Self {
        Self {
            until: None,
            message_index: None,
            last_message: None,
            retry_count: 0,
            auto_retry_cancelled: false,
        }
    }

    /// Calculate exponential backoff delay for current retry count.
    ///
    /// Formula: min(BACKOFF_BASE * 2^retry, BACKOFF_MAX) + jitter
    /// Jitter: ±20% of the base delay to avoid thundering herd (goose pattern).
    pub fn backoff_delay_secs(&self) -> u64 {
        let base = BACKOFF_BASE_SECS.saturating_mul(
            1u64.checked_shl(self.retry_count as u32)
                .unwrap_or(u64::MAX),
        );
        let capped = base.min(BACKOFF_MAX_SECS);
        // Simple jitter: add 0-20% of capped value using retry_count as pseudo-random seed
        let jitter = (capped / 5) * ((self.retry_count as u64 * 7 + 3) % 5) / 4;
        capped + jitter
    }

    /// Set rate limit state with exponential backoff delay
    pub fn set_rate_limit(
        &mut self,
        message_index: usize,
        last_message: String,
        server_retry_after: Option<u64>,
    ) {
        let delay_secs = server_retry_after.unwrap_or_else(|| self.backoff_delay_secs());
        self.until = Some(Instant::now() + std::time::Duration::from_secs(delay_secs));
        self.message_index = Some(message_index);
        self.last_message = Some(last_message);
        self.auto_retry_cancelled = false;
    }

    /// Set rate limit state with explicit timeout (backwards compat)
    pub fn set_rate_limit_with_until(
        &mut self,
        until: Instant,
        message_index: usize,
        last_message: String,
    ) {
        self.until = Some(until);
        self.message_index = Some(message_index);
        self.last_message = Some(last_message);
        self.retry_count = 0;
        self.auto_retry_cancelled = false;
    }

    /// Cancel auto-retry (user pressed Escape)
    pub fn cancel_auto_retry(&mut self) {
        self.auto_retry_cancelled = true;
    }

    /// Check if rate limit has expired and update countdown message
    /// Returns (expired, needs_update, new_content)
    pub fn update_countdown(&mut self) -> Option<String> {
        let until = self.until?;

        let remaining = until.saturating_duration_since(Instant::now());
        let remaining_secs = remaining.as_secs();

        if remaining_secs == 0 {
            // Rate limit expired
            let was_cancelled = self.auto_retry_cancelled;
            let retries_exhausted = self.retry_count >= MAX_AUTO_RETRIES;
            self.until = None;
            self.message_index = None;
            self.auto_retry_cancelled = false;

            // Don't reset retry_count here — it must accumulate so exponential
            // backoff actually escalates and the max-retry check works.

            return Some(if was_cancelled || retries_exhausted {
                // Prevent auto-retry when user cancelled or retries exhausted
                self.last_message = None;
                if retries_exhausted {
                    format!(
                        "Gave up after {} retries - press Enter to try again manually",
                        MAX_AUTO_RETRIES
                    )
                } else {
                    "Ready - press Enter to retry".to_string()
                }
            } else if self.last_message.is_some() {
                "Auto-retrying now...".to_string()
            } else {
                "Ready - press Enter to retry".to_string()
            });
        }

        // Build countdown message
        let retry_info = if self.retry_count > 1 {
            format!(
                " (retry #{}, next backoff: {}s)",
                self.retry_count,
                self.backoff_delay_secs()
            )
        } else {
            String::new()
        };

        let error_type = "Rate limited";

        let content = if self.auto_retry_cancelled {
            format!(
                "{} - Waiting {}s...{} (press Enter to retry)",
                error_type, remaining_secs, retry_info
            )
        } else {
            format!(
                "{} - Auto-retrying in {}s...{} (Esc to cancel)",
                error_type, remaining_secs, retry_info
            )
        };

        Some(content)
    }

    /// Check if auto-retry should happen
    pub fn should_auto_retry(&self) -> bool {
        self.until.is_none()
            && self.last_message.is_some()
            && !self.auto_retry_cancelled
            && self.retry_count < MAX_AUTO_RETRIES
    }

    /// Get the last message for retry
    pub fn take_last_message(&mut self) -> Option<String> {
        self.last_message.take()
    }

    /// Clear rate limit state
    pub fn clear(&mut self) {
        self.until = None;
        self.message_index = None;
        self.last_message = None;
        self.retry_count = 0;
        self.auto_retry_cancelled = false;
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

impl Default for RateLimitHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay_escelates() {
        let handler = RateLimitHandler::new();
        // retry_count=0: 2s base
        assert_eq!(handler.backoff_delay_secs(), 2);
    }

    #[test]
    fn test_backoff_delay_caps_at_max() {
        let mut handler = RateLimitHandler::new();
        handler.retry_count = 10;
        // 2^10 = 1024, capped at 60
        assert!(handler.backoff_delay_secs() <= BACKOFF_MAX_SECS + BACKOFF_MAX_SECS / 5);
    }

    #[test]
    fn test_should_auto_retry() {
        let mut handler = RateLimitHandler::new();
        assert!(!handler.should_auto_retry());

        handler.last_message = Some("test".to_string());
        assert!(handler.should_auto_retry());

        handler.retry_count = MAX_AUTO_RETRIES;
        assert!(!handler.should_auto_retry());
    }

    #[test]
    fn test_increment_retry() {
        let mut handler = RateLimitHandler::new();
        assert_eq!(handler.retry_count, 0);
        handler.increment_retry();
        assert_eq!(handler.retry_count, 1);
    }
}
