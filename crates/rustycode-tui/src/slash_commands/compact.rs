//! Compaction slash command
//!
//! Provides manual control over context compaction:
//! - `/compact` - Manual compaction
//! - `/compact show` - Show what would be compacted
//! - `/compact status` - Show token usage
//! - `/compact threshold <0-100>` - Set warning threshold
//! - `/compact aggressive` - Aggressive compaction (keep last 20)
//! - `/compact conservative` - Conservative compaction (keep last 100)

use crate::compaction::{compact_context, CompactionPreview, CompactionStrategy, ContextMonitor};
use crate::ui::message::Message;
use anyhow::Result;

/// Handle compact command
pub fn handle_compact_command(
    args: &[String],
    messages: &[Message],
    context_monitor: &ContextMonitor,
) -> Result<CompactAction> {
    match args.first().map(|s| s.as_str()) {
        None | Some("") => Ok(CompactAction::Compact),
        Some("show") => {
            let preview = CompactionPreview::new(
                context_monitor.current_tokens,
                context_monitor.max_tokens,
                messages,
                CompactionStrategy::Balanced,
            );
            Ok(CompactAction::ShowPreview(preview))
        }
        Some("status") => {
            let fmt = |n: usize| -> String {
                if n >= 1_000_000 { format!("{:.1}M", n as f64 / 1_000_000.0) }
                else if n >= 1_000 { format!("{:.0}k", n as f64 / 1_000.0) }
                else { n.to_string() }
            };
            let pct = context_monitor.usage_percentage() * 100.0;
            let bar_width = 20;
            let filled = if pct > 0.0 {
                ((pct / 100.0) * bar_width as f64).ceil() as usize
            } else {
                0
            }.min(bar_width);
            let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width - filled));
            let color = if pct < 50.0 { "green" } else if pct < 80.0 { "yellow" } else { "red" };
            let status = format!(
                "Context: {} / {} ({}%)\n[{}] {}\nRemaining: {} tokens\nThreshold: {:.0}%",
                fmt(context_monitor.current_tokens),
                fmt(context_monitor.max_tokens),
                pct as usize,
                bar, color,
                fmt(context_monitor.remaining_tokens()),
                context_monitor.warning_threshold * 100.0
            );
            Ok(CompactAction::ShowStatus(status))
        }
        Some("threshold") => {
            if args.len() < 2 {
                return Ok(CompactAction::Error(
                    "Usage: /compact threshold <0-100>".to_string(),
                ));
            }
            let threshold = args[1].parse::<f64>();
            match threshold {
                Ok(t) if (0.0..=100.0).contains(&t) => {
                    Ok(CompactAction::SetThreshold(t / 100.0))
                }
                _ => Ok(CompactAction::Error(
                    "Threshold must be between 0 and 100".to_string(),
                )),
            }
        }
        Some("aggressive") => Ok(CompactAction::SetStrategy(CompactionStrategy::Aggressive)),
        Some("conservative") => {
            Ok(CompactAction::SetStrategy(CompactionStrategy::Conservative))
        }
        Some("balanced") => Ok(CompactAction::SetStrategy(CompactionStrategy::Balanced)),
        Some(cmd) => Ok(CompactAction::Error(format!(
            "Unknown compact command: {}\nValid options: show, status, threshold, aggressive, conservative, balanced",
            cmd
        ))),
    }
}

/// Action to take after processing compact command
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum CompactAction {
    /// Perform compaction
    Compact,
    /// Show compaction preview
    ShowPreview(CompactionPreview),
    /// Show token status
    ShowStatus(String),
    /// Set warning threshold
    SetThreshold(f64),
    /// Set compaction strategy
    SetStrategy(CompactionStrategy),
    /// Error message
    Error(String),
}

impl CompactAction {
    /// Get display message for this action
    pub fn display_message(&self) -> Option<String> {
        match self {
            CompactAction::Compact => {
                Some("💾 Compacting context to stay within limits...".to_string())
            }
            CompactAction::ShowPreview(preview) => Some(preview.format()),
            CompactAction::ShowStatus(status) => Some(status.clone()),
            CompactAction::SetThreshold(threshold) => Some(format!(
                "✓ Warning threshold set to {:.1}%",
                threshold * 100.0
            )),
            CompactAction::SetStrategy(strategy) => {
                Some(format!("✓ Compaction strategy set to {:?}", strategy))
            }
            CompactAction::Error(msg) => Some(format!("⚠ {}", msg)),
        }
    }

    /// Check if this action requires confirmation
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, CompactAction::Compact | CompactAction::ShowPreview(_))
    }
}

/// Execute compaction with given strategy
pub fn execute_compaction(
    messages: Vec<Message>,
    strategy: CompactionStrategy,
) -> Result<Vec<Message>> {
    let compacted = compact_context(messages, strategy);
    Ok(compacted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::message::MessageRole;

    fn create_test_message(role: MessageRole, content: &str) -> Message {
        Message::new(role, content.to_string())
    }

    #[test]
    fn test_handle_compact_command_default() {
        let messages = vec![create_test_message(MessageRole::User, "Test")];
        let monitor = ContextMonitor::new(100_000, 0.8);

        let action = handle_compact_command(&[], &messages, &monitor).unwrap();
        assert!(matches!(action, CompactAction::Compact));
    }

    #[test]
    fn test_handle_compact_command_show() {
        let messages: Vec<Message> = (0..100)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();
        let monitor = ContextMonitor::new(100_000, 0.8);

        let action = handle_compact_command(&["show".to_string()], &messages, &monitor).unwrap();
        assert!(matches!(action, CompactAction::ShowPreview(_)));
    }

    #[test]
    fn test_handle_compact_command_status() {
        let messages = vec![create_test_message(MessageRole::User, "Test")];
        let mut monitor = ContextMonitor::new(100_000, 0.8);
        monitor.update(&messages);

        let action = handle_compact_command(&["status".to_string()], &messages, &monitor).unwrap();
        assert!(matches!(action, CompactAction::ShowStatus(_)));
        if let CompactAction::ShowStatus(status) = action {
            assert!(status.contains("Context:"));
        }
    }

    #[test]
    fn test_handle_compact_command_threshold() {
        let messages = vec![create_test_message(MessageRole::User, "Test")];
        let monitor = ContextMonitor::new(100_000, 0.8);

        let action = handle_compact_command(
            &["threshold".to_string(), "75".to_string()],
            &messages,
            &monitor,
        )
        .unwrap();
        assert!(matches!(action, CompactAction::SetThreshold(0.75)));
    }

    #[test]
    fn test_handle_compact_command_threshold_invalid() {
        let messages = vec![create_test_message(MessageRole::User, "Test")];
        let monitor = ContextMonitor::new(100_000, 0.8);

        let action = handle_compact_command(
            &["threshold".to_string(), "150".to_string()],
            &messages,
            &monitor,
        )
        .unwrap();
        assert!(matches!(action, CompactAction::Error(_)));
    }

    #[test]
    fn test_handle_compact_command_aggressive() {
        let messages = vec![create_test_message(MessageRole::User, "Test")];
        let monitor = ContextMonitor::new(100_000, 0.8);

        let action =
            handle_compact_command(&["aggressive".to_string()], &messages, &monitor).unwrap();
        assert!(matches!(
            action,
            CompactAction::SetStrategy(CompactionStrategy::Aggressive)
        ));
    }

    #[test]
    fn test_compact_action_display_message() {
        let action = CompactAction::Compact;
        assert!(action.display_message().is_some());
        assert!(action.display_message().unwrap().contains("Compacting"));

        let action = CompactAction::SetThreshold(0.9);
        // Format is {:.1}% which gives "90.0%" not "90%"
        assert!(action.display_message().unwrap().contains("90.0%"));
    }

    #[test]
    fn test_compact_action_requires_confirmation() {
        let action = CompactAction::Compact;
        assert!(action.requires_confirmation());

        let action = CompactAction::SetThreshold(0.8);
        assert!(!action.requires_confirmation());
    }

    #[test]
    fn test_execute_compaction() {
        let messages: Vec<Message> = (0..60)
            .map(|i| create_test_message(MessageRole::User, &format!("Message {}", i)))
            .collect();

        let result = execute_compaction(messages, CompactionStrategy::Balanced).unwrap();
        assert!(result.len() <= 51); // 50 recent + 1 summary
    }
}
