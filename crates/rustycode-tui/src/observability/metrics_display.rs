/// Metrics display formatting functions for TUI dashboard
/// Format token count in a human-readable way
/// Examples: 1500 -> "1.5K", 1000000 -> "1.0M"
pub fn format_tokens(used: u64, remaining: u64) -> String {
    let total = used.saturating_add(remaining);
    let percent = if total > 0 {
        (used as f64 / total as f64 * 100.0).round() as u64
    } else {
        0
    };

    let used_str = format_number(used);
    let total_str = format_number(total);

    format!("{} / {} tokens used ({}%)", used_str, total_str, percent)
}

/// Format a duration in seconds to a human-readable time format
/// Examples: 3661 -> "1h 1m 1s", 45 -> "45s"
pub fn format_duration(secs: f64) -> String {
    if secs < 0.0 {
        return "0s".to_string();
    }

    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format task rate as tasks per hour
pub fn format_task_rate(tasks: u64, elapsed: f64) -> String {
    if elapsed <= 0.0 {
        return "0 tasks/hr".to_string();
    }

    let hours = elapsed / 3600.0;
    if hours <= 0.0 {
        return "0 tasks/hr".to_string();
    }

    let rate = tasks as f64 / hours;
    format!("{:.1} tasks/hr", rate)
}

/// Generate a text-based progress bar
/// width: character width of the bar
/// percent: 0.0 to 100.0
pub fn progress_bar(percent: f64, width: u16) -> String {
    let width = width.max(3) as usize; // Minimum width of 3
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width); // Cap at width

    let bar = "█".repeat(filled) + &"░".repeat(width.saturating_sub(filled));
    format!("[{}] {:.0}%", bar, percent)
}

/// Helper function to format large numbers with K/M/B suffixes
fn format_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens_basic() {
        let result = format_tokens(500, 1500);
        assert!(result.contains("500"));
        assert!(result.contains("2.0K"));
        assert!(result.contains("25%"));
    }

    #[test]
    fn test_format_tokens_zero() {
        let result = format_tokens(0, 0);
        assert_eq!(result, "0 / 0 tokens used (0%)");
    }

    #[test]
    fn test_format_tokens_all_used() {
        let result = format_tokens(2000, 0);
        assert!(result.contains("2.0K"));
        assert!(result.contains("100%"));
    }

    #[test]
    fn test_format_tokens_large_numbers() {
        let result = format_tokens(1_500_000, 500_000);
        assert!(result.contains("1.5M"));
        assert!(result.contains("2.0M"));
        assert!(result.contains("75%"));
    }

    #[test]
    fn test_format_duration_seconds() {
        let result = format_duration(45.0);
        assert_eq!(result, "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let result = format_duration(125.0); // 2m 5s
        assert_eq!(result, "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        let result = format_duration(3661.0); // 1h 1m 1s
        assert_eq!(result, "1h 1m 1s");
    }

    #[test]
    fn test_format_duration_zero() {
        let result = format_duration(0.0);
        assert_eq!(result, "0s");
    }

    #[test]
    fn test_format_duration_negative() {
        let result = format_duration(-10.0);
        assert_eq!(result, "0s");
    }

    #[test]
    fn test_format_duration_large() {
        let result = format_duration(86461.0); // 1 day + 1m 1s
        assert_eq!(result, "24h 1m 1s");
    }

    #[test]
    fn test_format_task_rate_zero_elapsed() {
        let result = format_task_rate(10, 0.0);
        assert_eq!(result, "0 tasks/hr");
    }

    #[test]
    fn test_format_task_rate_negative_elapsed() {
        let result = format_task_rate(10, -1.0);
        assert_eq!(result, "0 tasks/hr");
    }

    #[test]
    fn test_format_task_rate_one_hour() {
        let result = format_task_rate(10, 3600.0); // 1 hour
        assert!(result.contains("10.0"));
    }

    #[test]
    fn test_format_task_rate_half_hour() {
        let result = format_task_rate(5, 1800.0); // 30 minutes = 10 tasks/hr
        assert!(result.contains("10.0"));
    }

    #[test]
    fn test_progress_bar_zero() {
        let result = progress_bar(0.0, 10);
        assert!(result.contains("["));
        assert!(result.contains("]"));
        assert!(result.contains("0%"));
    }

    #[test]
    fn test_progress_bar_fifty() {
        let result = progress_bar(50.0, 10);
        assert!(result.contains("50%"));
    }

    #[test]
    fn test_progress_bar_full() {
        let result = progress_bar(100.0, 10);
        assert!(result.contains("100%"));
    }

    #[test]
    fn test_progress_bar_over_100() {
        let result = progress_bar(150.0, 10);
        assert!(result.contains("150%"));
    }

    #[test]
    fn test_progress_bar_small_width() {
        let result = progress_bar(50.0, 1);
        // Should still work with minimum width of 3
        assert!(result.contains("["));
        assert!(result.contains("]"));
    }

    #[test]
    fn test_format_number_small() {
        let result = format_number(500);
        assert_eq!(result, "500");
    }

    #[test]
    fn test_format_number_thousands() {
        let result = format_number(1500);
        assert_eq!(result, "1.5K");
    }

    #[test]
    fn test_format_number_millions() {
        let result = format_number(1_500_000);
        assert_eq!(result, "1.5M");
    }

    #[test]
    fn test_format_number_billions() {
        let result = format_number(1_500_000_000);
        assert_eq!(result, "1.5B");
    }
}
