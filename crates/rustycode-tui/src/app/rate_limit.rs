//! Rate limiting and retry logic for LLM API calls
//!
//! Handles rate limit detection, exponential backoff, and countdown display.

use std::time::{Duration, Instant, SystemTime};

/// Rate limit state
#[derive(Default)]
pub struct RateLimitState {
    /// When the rate limit will expire
    pub until: Option<Instant>,
    /// Message index showing the countdown
    pub countdown_message_index: Option<usize>,
    /// Number of retries attempted (for exponential backoff)
    pub retry_count: usize,
    /// Last message sent (for retry)
    pub last_message: Option<String>,
}

impl RateLimitState {
    /// Create a new rate limit state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if currently rate limited
    pub fn is_rate_limited(&self) -> bool {
        self.until
            .map(|until| until > Instant::now())
            .unwrap_or(false)
    }

    /// Get remaining seconds until rate limit expires
    pub fn remaining_seconds(&self) -> Option<u64> {
        self.until
            .map(|until| until.saturating_duration_since(Instant::now()).as_secs())
    }

    /// Set a rate limit with exponential backoff
    pub fn set_rate_limit(&mut self) -> Duration {
        // Calculate exponential backoff with jitter (starts at 5s: 5, 10, 20, 40, 60...)
        let base_delay_secs = (5 * 2_usize.pow(self.retry_count as u32)).min(60);
        let jitter = (base_delay_secs as f64 * 0.25) as isize;

        let random_jitter = if jitter > 0 {
            let nanos = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as isize;
            (nanos % (2 * jitter)) - jitter
        } else {
            0
        };

        let delay_secs = (base_delay_secs as isize + random_jitter).max(1) as u64;
        let duration = Duration::from_secs(delay_secs);

        self.until = Some(Instant::now() + duration);

        // Increment retry count for next time
        self.retry_count += 1;

        duration
    }

    /// Reset the rate limit state (on successful request)
    pub fn reset(&mut self) {
        self.until = None;
        self.retry_count = 0;
        self.last_message = None;
    }

    /// Set the last message for retry purposes
    pub fn set_last_message(&mut self, message: String) {
        self.last_message = Some(message);
    }

    /// Get the last message (for retry)
    pub fn take_last_message(&mut self) -> Option<String> {
        self.last_message.take()
    }

    /// Update countdown display
    ///
    /// Returns true if the countdown should still be shown
    pub fn update_countdown(&mut self) -> bool {
        if let Some(until) = self.until {
            if Instant::now() >= until {
                // Rate limit expired
                self.reset();
                false
            } else {
                true
            }
        } else {
            false
        }
    }

    /// Get a human-readable status message
    pub fn status_message(&self) -> Option<String> {
        self.remaining_seconds().map(|secs| {
            format!(
                "⚠️  Rate limited - Auto-retrying in {}s... (or press Enter to retry now)",
                secs
            )
        })
    }

    /// Get the error type based on the error message
    pub fn classify_error(error: &str) -> RateLimitError {
        let error_lower = error.to_lowercase();

        if error_lower.contains("rate limit") || error_lower.contains("streaming_error") {
            RateLimitError::RateLimited
        } else if error_lower.contains("api error") || error_lower.contains("connection") {
            RateLimitError::ConnectionIssue
        } else {
            RateLimitError::TemporaryIssue
        }
    }
}

/// Types of rate limit errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RateLimitError {
    /// Explicit rate limit from API
    RateLimited,
    /// Connection problems
    ConnectionIssue,
    /// Other temporary issues
    TemporaryIssue,
}

impl RateLimitError {
    /// Get display text for this error type
    pub fn display_text(&self) -> &'static str {
        match self {
            RateLimitError::RateLimited => "Rate limited",
            RateLimitError::ConnectionIssue => "Connection issue",
            RateLimitError::TemporaryIssue => "Temporary issue",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_state_default() {
        let state = RateLimitState::default();
        assert!(!state.is_rate_limited());
        assert_eq!(state.remaining_seconds(), None);
    }

    #[test]
    fn test_set_rate_limit() {
        let mut state = RateLimitState::new();
        let duration = state.set_rate_limit();

        assert!(state.is_rate_limited());
        assert!(duration.as_secs() >= 1);
        assert!(state.remaining_seconds().is_some());
    }

    #[test]
    fn test_retry_count_increases() {
        let mut state = RateLimitState::new();

        state.set_rate_limit();
        assert_eq!(state.retry_count, 1);

        state.set_rate_limit();
        assert_eq!(state.retry_count, 2);

        // Exponential backoff - second wait should be longer
        // (but we can't test exact timing easily)
    }

    #[test]
    fn test_reset() {
        let mut state = RateLimitState::new();
        state.set_rate_limit();
        state.set_last_message("test".to_string());

        assert!(state.is_rate_limited());

        state.reset();

        assert!(!state.is_rate_limited());
        assert_eq!(state.retry_count, 0);
        assert!(state.last_message.is_none());
    }

    #[test]
    fn test_classify_error() {
        assert_eq!(
            RateLimitState::classify_error("Rate limit exceeded"),
            RateLimitError::RateLimited
        );
        assert_eq!(
            RateLimitState::classify_error("Connection error"),
            RateLimitError::ConnectionIssue
        );
        assert_eq!(
            RateLimitState::classify_error("Something went wrong"),
            RateLimitError::TemporaryIssue
        );
    }

    #[test]
    fn test_error_display_text() {
        assert_eq!(RateLimitError::RateLimited.display_text(), "Rate limited");
        assert_eq!(
            RateLimitError::ConnectionIssue.display_text(),
            "Connection issue"
        );
        assert_eq!(
            RateLimitError::TemporaryIssue.display_text(),
            "Temporary issue"
        );
    }
}
