#[derive(Debug, Clone, Default)]
pub struct ContextUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub context_limit: usize,
}

impl ContextUsage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, input_tokens: usize, output_tokens: usize) {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
    }

    pub fn set_limit(&mut self, limit: usize) {
        self.context_limit = limit;
    }

    pub fn total_tokens(&self) -> usize {
        self.input_tokens.saturating_add(self.output_tokens)
    }

    pub fn percentage(&self) -> usize {
        if self.context_limit == 0 {
            return 0;
        }
        let pct =
            ((self.total_tokens() as f64 / self.context_limit as f64) * 100.0).round() as usize;
        pct.min(100)
    }

    pub fn format_bar(&self, width: usize) -> String {
        if self.context_limit == 0 {
            // No limit known, just show token counts
            return format!("ctx {} {}", format_tokens(self.total_tokens()), "used");
        }

        let percentage = self.percentage().min(100);
        let bar_width = width.clamp(8, 20);
        let filled = ((percentage as f64 / 100.0) * bar_width as f64).round() as usize;

        let bar = crate::app::render::brutalist_helpers::progress_bar(
            bar_width,
            filled,
            crate::app::render::brutalist_helpers::PROGRESS_CHARS_CONTEXT.0,
            crate::app::render::brutalist_helpers::PROGRESS_CHARS_CONTEXT.1,
        );

        format!(
            "ctx [{}] {}% {}/{}",
            bar,
            percentage,
            format_tokens(self.total_tokens()),
            format_tokens(self.context_limit),
        )
    }

    pub fn color_level(&self) -> UsageLevel {
        let pct = self.percentage();
        if pct < MEDIUM_USAGE_THRESHOLD {
            UsageLevel::Low
        } else if pct < HIGH_USAGE_THRESHOLD {
            UsageLevel::Medium
        } else {
            UsageLevel::High
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageLevel {
    Low,    // < MEDIUM_USAGE_THRESHOLD
    Medium, // < HIGH_USAGE_THRESHOLD
    High,   // >= HIGH_USAGE_THRESHOLD
}

pub const MEDIUM_USAGE_THRESHOLD: usize = 50;
pub const HIGH_USAGE_THRESHOLD: usize = 85;

fn format_tokens(n: usize) -> String {
    crate::app::render::brutalist_helpers::format_tokens_compact(n)
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
