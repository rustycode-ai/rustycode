//! /stats command — display session statistics
//!
//! Shows token usage, cost, turn count, context usage, and tool execution stats
//! in a compact, readable format. Inspired by goose's display_cost_usage pattern.

/// Result of the /stats command
pub struct StatsResult {
    /// The formatted stats string to display as a system message
    pub display: String,
}

/// Session statistics for the /stats command
pub struct SessionStats {
    /// Total input tokens
    pub input_tokens: usize,
    /// Total output tokens
    pub output_tokens: usize,
    /// Session cost in USD
    pub cost_usd: f64,
    /// Number of user turns (messages)
    pub turn_count: usize,
    /// Number of tool executions
    pub tool_count: usize,
    /// Number of tool failures
    pub tool_failures: usize,
    /// Context usage percentage (0-100)
    pub context_percentage: usize,
    /// Context tokens used
    pub context_tokens: usize,
    /// Context token limit
    pub context_limit: usize,
    /// Model name
    pub model: String,
    /// Session duration in seconds
    pub duration_secs: u64,
}

/// Handle the /stats command
pub fn handle_stats_command(stats: &SessionStats) -> StatsResult {
    let fmt_tokens = |n: usize| -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.0}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    };

    let fmt_cost = |c: f64| -> String {
        if c < 0.01 {
            format!("${:.4}", c)
        } else {
            format!("${:.2}", c)
        }
    };

    let fmt_duration = |s: u64| -> String {
        if s < 60 {
            format!("{}s", s)
        } else if s < 3600 {
            format!("{}m {}s", s / 60, s % 60)
        } else {
            format!("{}h {}m", s / 3600, (s % 3600) / 60)
        }
    };

    // Context bar (goose pattern)
    let bar_width: usize = 20;
    let filled = ((stats.context_percentage as f64 / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled.min(bar_width));
    let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(empty));

    let mut lines = Vec::new();

    lines.push("Session Statistics".to_string());
    lines.push("─────────────────".to_string());

    // Model and duration
    lines.push(format!(
        "Model: {}  │  Duration: {}",
        stats.model,
        fmt_duration(stats.duration_secs)
    ));

    // Token usage
    lines.push(format!(
        "Tokens: ↑{} in  ↓{} out  │  Total: {}",
        fmt_tokens(stats.input_tokens),
        fmt_tokens(stats.output_tokens),
        fmt_tokens(stats.input_tokens + stats.output_tokens)
    ));

    // Cost
    lines.push(format!(
        "Cost: {}  │  Turns: {}",
        fmt_cost(stats.cost_usd),
        stats.turn_count
    ));

    // Context usage with bar
    lines.push(format!(
        "Context: [{}] {}% ({}/{})",
        bar,
        stats.context_percentage,
        fmt_tokens(stats.context_tokens),
        fmt_tokens(stats.context_limit)
    ));

    // Tool stats
    let tool_success_rate = if stats.tool_count > 0 {
        let success = stats.tool_count.saturating_sub(stats.tool_failures);
        format!("{:.0}%", (success as f64 / stats.tool_count as f64) * 100.0)
    } else {
        "N/A".to_string()
    };
    lines.push(format!(
        "Tools: {} total  │  {} failed  │  {} success rate",
        stats.tool_count, stats.tool_failures, tool_success_rate
    ));

    StatsResult {
        display: lines.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_basic() {
        let stats = SessionStats {
            input_tokens: 50_000,
            output_tokens: 5_000,
            cost_usd: 0.42,
            turn_count: 12,
            tool_count: 30,
            tool_failures: 2,
            context_percentage: 55,
            context_tokens: 55_000,
            context_limit: 100_000,
            model: "claude-sonnet-4-6".to_string(),
            duration_secs: 300,
        };

        let result = handle_stats_command(&stats);
        assert!(result.display.contains("Session Statistics"));
        assert!(result.display.contains("50k"));
        assert!(result.display.contains("5k"));
        assert!(result.display.contains("$0.42"));
        assert!(result.display.contains("12"));
        assert!(result.display.contains("30 total"));
        assert!(result.display.contains("55%"));
        assert!(result.display.contains("5m 0s"));
    }

    #[test]
    fn test_stats_small_numbers() {
        let stats = SessionStats {
            input_tokens: 500,
            output_tokens: 100,
            cost_usd: 0.005,
            turn_count: 2,
            tool_count: 0,
            tool_failures: 0,
            context_percentage: 0,
            context_tokens: 600,
            context_limit: 200_000,
            model: "claude-sonnet-4-6".to_string(),
            duration_secs: 30,
        };

        let result = handle_stats_command(&stats);
        assert!(result.display.contains("500"));
        assert!(result.display.contains("$0.0050"));
        assert!(result.display.contains("N/A"));
    }

    #[test]
    fn test_stats_high_context() {
        let stats = SessionStats {
            input_tokens: 150_000,
            output_tokens: 30_000,
            cost_usd: 2.50,
            turn_count: 45,
            tool_count: 100,
            tool_failures: 5,
            context_percentage: 90,
            context_tokens: 180_000,
            context_limit: 200_000,
            model: "claude-sonnet-4-6".to_string(),
            duration_secs: 3661,
        };

        let result = handle_stats_command(&stats);
        assert!(result.display.contains("150k"));
        assert!(result.display.contains("95%")); // success rate
        assert!(result.display.contains("1h 1m"));
        assert!(result.display.contains("90%"));
    }
}
