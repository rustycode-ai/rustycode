//! Context usage tracking and visualization
//!
//! Tracks token usage across the conversation and provides
//! a compact progress bar for the TUI footer.
//!
//! Inspired by goose's `display_context_usage` pattern.

/// Context usage state
#[derive(Debug, Clone, Default)]
pub struct ContextUsage {
    /// Total input tokens used
    pub input_tokens: usize,
    /// Total output tokens used
    pub output_tokens: usize,
    /// Context window limit (if known)
    pub context_limit: usize,
}

impl ContextUsage {
    /// Create new empty context usage
    pub fn new() -> Self {
        Self::default()
    }

    /// Update with new token counts from a response
    pub fn update(&mut self, input_tokens: usize, output_tokens: usize) {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
    }

    /// Set the context window limit
    pub fn set_limit(&mut self, limit: usize) {
        self.context_limit = limit;
    }

    /// Get total tokens used
    pub fn total_tokens(&self) -> usize {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    /// Get usage percentage (0-100), clamped at 100
    pub fn percentage(&self) -> usize {
        if self.context_limit == 0 {
            return 0;
        }
        let pct =
            ((self.total_tokens() as f64 / self.context_limit as f64) * 100.0).round() as usize;
        pct.min(100)
    }

    /// Format a compact progress bar for the footer
    ///
    /// Returns a string like "ctx [████████░░░░] 42% 8.2k/20k"
    pub fn format_bar(&self, width: usize) -> String {
        if self.context_limit == 0 {
            // No limit known, just show token counts
            return format!("ctx {} {}", format_tokens(self.total_tokens()), "used");
        }

        let percentage = self.percentage().min(100);
        let bar_width = width.clamp(8, 20);
        let filled = ((percentage as f64 / 100.0) * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);

        let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(empty));

        format!(
            "ctx [{}] {}% {}/{}",
            bar,
            percentage,
            format_tokens(self.total_tokens()),
            format_tokens(self.context_limit),
        )
    }

    /// Get the color level for the current usage
    /// Returns (filled_color, threshold_name)
    pub fn color_level(&self) -> UsageLevel {
        let pct = self.percentage();
        if pct < 50 {
            UsageLevel::Low
        } else if pct < 85 {
            UsageLevel::Medium
        } else {
            UsageLevel::High
        }
    }
}

/// Usage level for color coding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UsageLevel {
    Low,    // < 50% - green
    Medium, // < 85% - yellow
    High,   // >= 85% - red
}

/// Format token count for display (e.g., "8.2k", "1.5M")
fn format_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_context_usage() {
        let usage = ContextUsage::new();
        assert_eq!(usage.total_tokens(), 0);
        assert_eq!(usage.percentage(), 0);
    }

    #[test]
    fn test_update_tokens() {
        let mut usage = ContextUsage::new();
        usage.update(5000, 1000);
        assert_eq!(usage.total_tokens(), 6000);
    }

    #[test]
    fn test_percentage_calculation() {
        let mut usage = ContextUsage::new();
        usage.set_limit(100_000);
        usage.update(50_000, 0);
        assert_eq!(usage.percentage(), 50);
    }

    #[test]
    fn test_percentage_clamped() {
        let mut usage = ContextUsage::new();
        usage.set_limit(1000);
        usage.update(5000, 5000);
        assert_eq!(usage.percentage(), 100);
    }

    #[test]
    fn test_format_bar_no_limit() {
        let usage = ContextUsage::new();
        let bar = usage.format_bar(20);
        assert!(bar.contains("ctx"));
    }

    #[test]
    fn test_format_bar_with_limit() {
        let mut usage = ContextUsage::new();
        usage.set_limit(200_000);
        usage.update(80_000, 4_000);
        let bar = usage.format_bar(15);
        assert!(bar.contains('━'));
        assert!(bar.contains('╌'));
        assert!(bar.contains("84.0k/200.0k"));
    }

    #[test]
    fn test_color_levels() {
        let mut usage = ContextUsage::new();
        usage.set_limit(100_000);

        usage.update(30_000, 0);
        assert_eq!(usage.color_level(), UsageLevel::Low);

        usage.update(70_000, 0);
        assert_eq!(usage.color_level(), UsageLevel::Medium);

        usage.update(90_000, 0);
        assert_eq!(usage.color_level(), UsageLevel::High);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(1_500_000), "1.5M");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1.0k");
    }
}
