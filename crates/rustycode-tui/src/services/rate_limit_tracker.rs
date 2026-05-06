//! Rate limit status tracker for TUI status bar
//!
//! Parses `x-ratelimit-*` headers from LLM API responses and provides
//! formatted status strings with color-coding thresholds.

use chrono::{DateTime, Utc};

/// Tracks rate limit state extracted from LLM response headers.
///
/// Displayed in the TUI status bar when data is available, showing
/// remaining calls, usage percentage, and time until reset.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RateLimitTracker {
    /// Remaining requests in the current window (from `x-ratelimit-remaining`)
    remaining: Option<u32>,
    /// Total requests allowed in the current window (from `x-ratelimit-limit`)
    limit: Option<u32>,
    /// When the rate limit window resets (from `x-ratelimit-reset`, Unix timestamp)
    reset_at: Option<DateTime<Utc>>,
}

impl RateLimitTracker {
    /// Update tracker from HTTP response headers.
    ///
    /// Extracts `x-ratelimit-remaining`, `x-ratelimit-limit`, and
    /// `x-ratelimit-reset` headers. Missing headers leave existing values unchanged.
    pub fn update_from_headers(&mut self, headers: &reqwest::header::HeaderMap) {
        if let Some(val) = parse_header::<u32>(headers, "x-ratelimit-remaining") {
            self.remaining = Some(val);
        }
        if let Some(val) = parse_header::<u32>(headers, "x-ratelimit-limit") {
            self.limit = Some(val);
        }
        if let Some(val) = parse_header::<u64>(headers, "x-ratelimit-reset") {
            self.reset_at = DateTime::from_timestamp(val as i64, 0);
        }
    }

    /// Format a human-readable status string for the status bar.
    ///
    /// Returns `None` when no rate limit data is available.
    /// Example output: `"Rate: 42/60 (70%) — resets in 3m"`
    pub fn format_status(&self) -> Option<String> {
        let remaining = self.remaining?;
        let limit = self.limit?;

        let used = limit.saturating_sub(remaining);
        let pct = if limit > 0 {
            (used as f64 / limit as f64 * 100.0).round() as u8
        } else {
            0
        };

        let reset_suffix = match self.reset_at {
            Some(reset_at) => {
                let secs = (reset_at - Utc::now())
                    .num_seconds()
                    .max(0) as u64;
                if secs == 0 {
                    String::new()
                } else if secs < 60 {
                    format!(" — resets in {}s", secs)
                } else {
                    format!(" — resets in {}m", secs / 60)
                }
            }
            None => String::new(),
        };

        Some(format!("Rate: {}/{} ({}%){}", remaining, limit, pct, reset_suffix))
    }

    /// Returns usage percentage (0-100) for color coding, or `None` if no data.
    pub fn usage_percent(&self) -> Option<u8> {
        let remaining = self.remaining?;
        let limit = self.limit?;
        if limit == 0 {
            return Some(100);
        }
        let used = limit.saturating_sub(remaining);
        Some(((used as f64 / limit as f64) * 100.0).round() as u8)
    }

    /// Returns `true` when usage exceeds 80% of the allowed limit.
    pub fn is_approaching_limit(&self) -> bool {
        self.usage_percent().map_or(false, |pct| pct >= 80)
    }

    /// Returns `true` when we have enough data to display a status.
    pub fn has_data(&self) -> bool {
        self.remaining.is_some() && self.limit.is_some()
    }

    /// Reset all tracked state (e.g., on provider switch).
    pub fn clear(&mut self) {
        self.remaining = None;
        self.limit = None;
        self.reset_at = None;
    }
}

/// Parse a header value as a numeric type.
fn parse_header<T: std::str::FromStr>(headers: &reqwest::header::HeaderMap, name: &str) -> Option<T> {
    let key = reqwest::header::HeaderName::from_bytes(name.as_bytes()).ok()?;
    headers.get(key)?.to_str().ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(remaining: Option<&str>, limit: Option<&str>, reset: Option<&str>) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        if let Some(v) = remaining { h.insert("x-ratelimit-remaining", v.parse().unwrap()); }
        if let Some(v) = limit { h.insert("x-ratelimit-limit", v.parse().unwrap()); }
        if let Some(v) = reset { h.insert("x-ratelimit-reset", v.parse().unwrap()); }
        h
    }

    #[test]
    fn test_update_from_headers() {
        let mut t = RateLimitTracker::default();
        let ts = Utc::now().timestamp() + 180;
        t.update_from_headers(&headers(Some("42"), Some("60"), Some(&ts.to_string())));
        assert_eq!(t.remaining, Some(42));
        assert_eq!(t.limit, Some(60));
        assert!(t.reset_at.is_some());
    }

    #[test]
    fn test_format_status_with_data() {
        let mut t = RateLimitTracker::default();
        let ts = Utc::now().timestamp() + 200;
        t.update_from_headers(&headers(Some("42"), Some("60"), Some(&ts.to_string())));
        let s = t.format_status().expect("should have status");
        assert!(s.starts_with("Rate: 42/60"));
        assert!(s.contains("30%"));
        assert!(s.contains("resets in"), "status: {}", s);
    }

    #[test]
    fn test_format_status_no_data() {
        assert!(RateLimitTracker::default().format_status().is_none());
        // Only remaining, no limit → also None
        let mut t = RateLimitTracker::default();
        t.update_from_headers(&headers(Some("42"), None, None));
        assert!(t.format_status().is_none());
    }

    #[test]
    fn test_approaching_limit() {
        let mut t = RateLimitTracker::default();
        // 85% used (9/60 remaining) → approaching
        t.update_from_headers(&headers(Some("9"), Some("60"), None));
        assert!(t.is_approaching_limit());
        // 30% used (42/60) → not approaching
        t.update_from_headers(&headers(Some("42"), Some("60"), None));
        assert!(!t.is_approaching_limit());
        // Exactly 80% used (12/60) → approaching
        t.update_from_headers(&headers(Some("12"), Some("60"), None));
        assert!(t.is_approaching_limit());
    }

    #[test]
    fn test_reset_time_formatting() {
        let mut t = RateLimitTracker::default();
        // Seconds format
        let ts = Utc::now().timestamp() + 45;
        t.update_from_headers(&headers(Some("30"), Some("60"), Some(&ts.to_string())));
        let s = t.format_status().unwrap();
        assert!(s.contains("45s") || s.contains("44s"), "status: {}", s);
        // Minutes format (185s = 3m)
        let ts = Utc::now().timestamp() + 185;
        t.update_from_headers(&headers(Some("30"), Some("60"), Some(&ts.to_string())));
        let s = t.format_status().unwrap();
        assert!(s.contains("resets in 3m"), "status: {}", s);
    }

    #[test]
    fn test_clear_and_usage_percent() {
        let mut t = RateLimitTracker::default();
        assert!(t.usage_percent().is_none());
        t.update_from_headers(&headers(Some("30"), Some("60"), None));
        assert_eq!(t.usage_percent(), Some(50));
        assert!(t.has_data());
        t.clear();
        assert!(!t.has_data());
        assert!(t.format_status().is_none());
        // Zero limit edge case
        t.update_from_headers(&headers(Some("0"), Some("0"), None));
        assert_eq!(t.usage_percent(), Some(100));
    }
}
