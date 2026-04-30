//! Shared formatting utilities for slash commands.
//!
//! Provides consistent output formatting across all slash commands.

/// Create a header box for command output.
pub fn header_box(title: &str, width: usize) -> String {
    let border = "═".repeat(width);
    let padding = (width.saturating_sub(title.len())) / 2;
    let left_pad = " ".repeat(padding);
    let right_pad = " ".repeat(width.saturating_sub(padding + title.len()));
    format!(
        "{}\n{}{}{}\n{}\n\n",
        border, left_pad, title, right_pad, border
    )
}

/// Create a footer separator.
pub fn footer_separator(width: usize) -> String {
    let separator = "─".repeat(width);
    format!("\n{}\n", separator)
}

/// Format a list item with an optional marker.
pub fn list_item(marker: &str, text: &str) -> String {
    format!("{} {}\n", marker, text)
}

/// Format a command hint with emoji.
pub fn command_hint(command: &str, description: &str) -> String {
    format!("  {:<30} - {}\n", command, description)
}

/// Format a success message with emoji.
pub fn success(message: &str) -> String {
    format!("✓ {}\n", message)
}

/// Format an error message with emoji.
pub fn error(message: &str) -> String {
    format!("❌ {}\n", message)
}

/// Format an info message with emoji.
pub fn info(message: &str) -> String {
    format!("ℹ️ {}\n", message)
}

/// Format a warning message with emoji.
pub fn warning(message: &str) -> String {
    format!("⚠️ {}\n", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_box() {
        let header = header_box("Test", 20);
        assert!(header.contains("Test"));
        assert!(header.contains("═"));
    }

    #[test]
    fn test_success() {
        assert_eq!(success("Done"), "✓ Done\n");
    }

    #[test]
    fn test_error() {
        assert_eq!(error("Failed"), "❌ Failed\n");
    }

    #[test]
    fn test_command_hint() {
        let hint = command_hint("help", "Show help");
        assert!(hint.contains("help"));
        assert!(hint.contains("Show help"));
    }
}
