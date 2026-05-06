//! Comprehensive error display with suggestions
//!
//! Provides rich error presentation with:
//! - Clear error messages with context
//! - Suggested fixes and actions
//! - Stack traces and debugging info
//! - Helpful next steps

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use std::time::Duration;

/// Error severity level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorSeverity {
    /// Critical error that prevents operation
    Critical,
    /// Error that affects current operation
    Error,
    /// Warning that might affect operation
    Warning,
    /// Information about potential issues
    Info,
}

impl ErrorSeverity {
    /// Get color for this severity
    pub fn color(&self) -> Color {
        match self {
            Self::Critical => Color::Red,
            Self::Error => Color::LightRed,
            Self::Warning => Color::Yellow,
            Self::Info => Color::Cyan,
        }
    }

    /// Get icon for this severity
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Critical => "🔴",
            Self::Error => "✗",
            Self::Warning => "⚠",
            Self::Info => "ℹ",
        }
    }
}

/// Error suggestion for fixing the issue
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ErrorSuggestion {
    /// Suggested action
    pub action: String,
    /// Command or shortcut to execute (if applicable)
    pub shortcut: Option<String>,
    /// Description of what this does
    pub description: Option<String>,
}

impl ErrorSuggestion {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            shortcut: None,
            description: None,
        }
    }

    /// Add a keyboard shortcut
    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Add a description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// Comprehensive error display
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ErrorDisplay {
    /// Error severity
    pub severity: ErrorSeverity,
    /// Error title
    pub title: String,
    /// Detailed error message
    pub message: String,
    /// Optional error code
    pub code: Option<String>,
    /// Optional path or file that caused the error
    pub path: Option<String>,
    /// Optional cause or context
    pub cause: Option<String>,
    /// Suggestions for fixing the error
    pub suggestions: Vec<ErrorSuggestion>,
    /// Optional stack trace (for debugging)
    pub stack_trace: Option<Vec<String>>,
    /// Whether to show technical details
    pub show_details: bool,
    /// Auto-dismiss timeout (None for manual dismiss)
    pub timeout: Option<Duration>,
}

impl ErrorDisplay {
    pub fn new(severity: ErrorSeverity, title: impl Into<String>) -> Self {
        Self {
            severity,
            title: title.into(),
            message: String::new(),
            code: None,
            path: None,
            cause: None,
            suggestions: Vec::new(),
            stack_trace: None,
            show_details: false,
            timeout: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Set an error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    /// Add a suggestion
    pub fn add_suggestion(mut self, suggestion: ErrorSuggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Add multiple suggestions
    pub fn with_suggestions(mut self, suggestions: Vec<ErrorSuggestion>) -> Self {
        self.suggestions = suggestions;
        self
    }

    /// Set stack trace
    pub fn with_stack_trace(mut self, trace: Vec<String>) -> Self {
        self.stack_trace = Some(trace);
        self
    }

    /// Show technical details
    pub fn with_details(mut self, show: bool) -> Self {
        self.show_details = show;
        self
    }

    /// Set auto-dismiss timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Convenience: Create a file not found error
    pub fn file_not_found(path: impl Into<String>) -> Self {
        let path = path.into();
        Self::new(ErrorSeverity::Error, "File Not Found")
            .with_message(format!("The file '{}' could not be found", path))
            .with_path(&path)
            .with_suggestions(vec![
                ErrorSuggestion::new("Check the file path")
                    .with_description("Verify the path is correct"),
                ErrorSuggestion::new("List directory")
                    .with_shortcut("/ls")
                    .with_description("See files in current directory"),
            ])
    }

    /// Convenience: Create a permission denied error
    pub fn permission_denied(path: impl Into<String>) -> Self {
        let path = path.into();
        Self::new(ErrorSeverity::Error, "Permission Denied")
            .with_message(format!("Permission denied for '{}'", path))
            .with_path(&path)
            .with_suggestions(vec![
                ErrorSuggestion::new("Check file permissions"),
                ErrorSuggestion::new("Use YOLO mode")
                    .with_shortcut("/yolo")
                    .with_description("Skip permission prompts (dangerous)"),
            ])
    }

    /// Convenience: Create a tool execution error
    pub fn tool_error(tool_name: impl Into<String>, error: impl Into<String>) -> Self {
        let tool = tool_name.into();
        let error = error.into();
        Self::new(ErrorSeverity::Error, format!("Tool Error: {}", tool))
            .with_message(error)
            .with_suggestions(vec![
                ErrorSuggestion::new("Retry the operation").with_shortcut("Ctrl+R"),
                ErrorSuggestion::new("Check tool logs").with_shortcut("/logs"),
            ])
    }

    /// Convenience: Create a network error
    pub fn network_error(message: impl Into<String>) -> Self {
        Self::new(ErrorSeverity::Warning, "Network Error")
            .with_message(message.into())
            .with_suggestions(vec![
                ErrorSuggestion::new("Check internet connection"),
                ErrorSuggestion::new("Retry the request").with_shortcut("Ctrl+R"),
                ErrorSuggestion::new("Switch provider")
                    .with_shortcut("/model")
                    .with_description("Try a different LLM provider"),
            ])
    }

    /// Convenience: Create a session error
    pub fn session_error(message: impl Into<String>) -> Self {
        Self::new(ErrorSeverity::Critical, "Session Error")
            .with_message(message.into())
            .with_suggestions(vec![
                ErrorSuggestion::new("Load backup session").with_shortcut("/load session-backup"),
                ErrorSuggestion::new("Start new session")
                    .with_shortcut("/clear")
                    .with_description("Clear current session"),
                ErrorSuggestion::new("View logs")
                    .with_description("Check: ~/.local/share/rustycode/logs/".to_string()),
            ])
    }

    /// Render the error display
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        // Clear the area first
        frame.render_widget(Clear, area);

        let color = self.severity.color();
        let icon = self.severity.icon();

        // Build the error content
        let mut lines = vec![
            // Title line with icon
            Line::from(vec![
                Span::styled(icon, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(
                    &self.title,
                    Style::default()
                        .fg(color)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                ),
            ]),
            Line::from(""), // Empty line
        ];

        // Error message
        if !self.message.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Error: ", Style::default().fg(Color::Red)),
                Span::styled(&self.message, Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(""));
        }

        // Error code
        if let Some(code) = &self.code {
            lines.push(Line::from(vec![
                Span::styled("Code: ", Style::default().fg(Color::Yellow)),
                Span::styled(code, Style::default().fg(Color::White)),
            ]));
        }

        // Path
        if let Some(path) = &self.path {
            lines.push(Line::from(vec![
                Span::styled("Path: ", Style::default().fg(Color::Yellow)),
                Span::styled(path, Style::default().fg(Color::White)),
            ]));
        }

        // Cause
        if let Some(cause) = &self.cause {
            lines.push(Line::from(vec![
                Span::styled("Cause: ", Style::default().fg(Color::Yellow)),
                Span::styled(cause, Style::default().fg(Color::Gray)),
            ]));
        }

        if !self.message.is_empty()
            || self.code.is_some()
            || self.path.is_some()
            || self.cause.is_some()
        {
            lines.push(Line::from("")); // Empty line
        }

        // Suggestions
        if !self.suggestions.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "Suggestions:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )]));
            lines.push(Line::from(""));

            for suggestion in self.suggestions.iter() {
                let mut spans = vec![
                    Span::styled("  • ", Style::default().fg(Color::Gray)),
                    Span::styled(&suggestion.action, Style::default().fg(Color::White)),
                ];

                if let Some(shortcut) = &suggestion.shortcut {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        format!("[{}]", shortcut),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(ratatui::style::Modifier::BOLD),
                    ));
                }

                lines.push(Line::from(spans));

                if let Some(desc) = &suggestion.description {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(desc, Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
        }

        // Stack trace (if showing details)
        if self.show_details {
            if let Some(trace) = &self.stack_trace {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Stack Trace:",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(ratatui::style::Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                for line in trace {
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(line, Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
        }

        // Help text at bottom
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().fg(Color::Gray)),
            Span::styled(
                "Enter/Esc",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(" dismiss", Style::default().fg(Color::Gray)),
            Span::raw("] "),
            Span::styled("[", Style::default().fg(Color::Gray)),
            Span::styled(
                "d",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(" details", Style::default().fg(Color::Gray)),
            Span::raw("]"),
        ]));

        // Create the paragraph widget
        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(color))
                    .title(" Error "),
            )
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }
}

/// Error display manager
///
/// Manages error display lifecycle and user interaction.
#[non_exhaustive]
pub struct ErrorManager {
    current_error: Option<ErrorDisplay>,
    history: Vec<ErrorDisplay>,
    max_history: usize,
    created_at: Option<std::time::Instant>,
}

const ERROR_AUTO_DISMISS: Duration = Duration::from_secs(10);

impl ErrorManager {
    pub fn new() -> Self {
        Self {
            current_error: None,
            history: Vec::new(),
            max_history: 50,
            created_at: None,
        }
    }

    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    pub fn show(&mut self, error: ErrorDisplay) {
        if let Some(current) = self.current_error.take() {
            self.history.push(current);
        }

        if self.history.len() > self.max_history {
            self.history.drain(0..self.history.len() - self.max_history);
        }

        self.created_at = Some(std::time::Instant::now());
        self.current_error = Some(error);
    }

    pub fn dismiss(&mut self) {
        self.current_error = None;
        self.created_at = None;
    }

    pub fn toggle_details(&mut self) {
        if let Some(error) = &mut self.current_error {
            error.show_details = !error.show_details;
        }
    }

    pub fn current(&self) -> Option<&ErrorDisplay> {
        self.current_error.as_ref()
    }

    pub fn is_showing(&mut self) -> bool {
        if let Some(created) = self.created_at {
            if created.elapsed() > ERROR_AUTO_DISMISS {
                self.current_error = None;
                self.created_at = None;
                return false;
            }
        }
        self.current_error.is_some()
    }

    /// Get error history
    pub fn history(&self) -> &[ErrorDisplay] {
        &self.history
    }

    /// Render the current error (if any)
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if let Some(error) = &self.current_error {
            error.render(frame, area);
        }
    }
}

impl Default for ErrorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_creation() {
        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test Error");
        assert_eq!(error.title, "Test Error");
        assert_eq!(error.severity, ErrorSeverity::Error);
        assert!(error.suggestions.is_empty());
    }

    #[test]
    fn test_error_display_with_message() {
        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test").with_message("Error message");
        assert_eq!(error.message, "Error message");
    }

    #[test]
    fn test_error_display_with_code() {
        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test").with_code("ERR_001");
        assert_eq!(error.code.as_deref(), Some("ERR_001"));
    }

    #[test]
    fn test_error_display_with_path() {
        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test").with_path("/path/to/file");
        assert_eq!(error.path.as_deref(), Some("/path/to/file"));
    }

    #[test]
    fn test_error_suggestion_creation() {
        let suggestion = ErrorSuggestion::new("Retry")
            .with_shortcut("Ctrl+R")
            .with_description("Try again");

        assert_eq!(suggestion.action, "Retry");
        assert_eq!(suggestion.shortcut.as_deref(), Some("Ctrl+R"));
        assert_eq!(suggestion.description.as_deref(), Some("Try again"));
    }

    #[test]
    fn test_error_display_with_suggestions() {
        let suggestion = ErrorSuggestion::new("Fix it").with_shortcut("Ctrl+F");

        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test").add_suggestion(suggestion);

        assert_eq!(error.suggestions.len(), 1);
        assert_eq!(error.suggestions[0].action, "Fix it");
    }

    #[test]
    fn test_error_convenience_constructors() {
        let file_error = ErrorDisplay::file_not_found("/test/path");
        assert_eq!(file_error.title, "File Not Found");
        assert!(file_error.path.as_ref().unwrap().contains("test/path"));

        let perm_error = ErrorDisplay::permission_denied("/restricted");
        assert_eq!(perm_error.title, "Permission Denied");

        let tool_error = ErrorDisplay::tool_error("read_file", "Failed");
        assert!(tool_error.title.contains("read_file"));

        let net_error = ErrorDisplay::network_error("Connection failed");
        assert_eq!(net_error.title, "Network Error");

        let sess_error = ErrorDisplay::session_error("Corrupted");
        assert_eq!(sess_error.title, "Session Error");
    }

    #[test]
    fn test_error_manager_creation() {
        let mut manager = ErrorManager::new();
        assert!(!manager.is_showing());
        assert_eq!(manager.history().len(), 0);
    }

    #[test]
    fn test_error_manager_show() {
        let mut manager = ErrorManager::new();
        let error = ErrorDisplay::new(ErrorSeverity::Error, "Test");

        manager.show(error);
        assert!(manager.is_showing());
        assert!(manager.current().is_some());
    }

    #[test]
    fn test_error_manager_dismiss() {
        let mut manager = ErrorManager::new();
        manager.show(ErrorDisplay::new(ErrorSeverity::Error, "Test"));

        assert!(manager.is_showing());
        manager.dismiss();
        assert!(!manager.is_showing());
    }

    #[test]
    fn test_error_manager_toggle_details() {
        let mut manager = ErrorManager::new();
        manager.show(ErrorDisplay::new(ErrorSeverity::Error, "Test").with_details(false));

        assert!(!manager.current().unwrap().show_details);
        manager.toggle_details();
        assert!(manager.current().unwrap().show_details);
    }

    #[test]
    fn test_error_manager_history() {
        let mut manager = ErrorManager::new();

        manager.show(ErrorDisplay::new(ErrorSeverity::Error, "Error 1"));
        manager.show(ErrorDisplay::new(ErrorSeverity::Error, "Error 2"));
        manager.show(ErrorDisplay::new(ErrorSeverity::Error, "Error 3"));

        assert_eq!(manager.history().len(), 2);
        assert_eq!(manager.history()[0].title, "Error 1");
        assert_eq!(manager.history()[1].title, "Error 2");
    }

    #[test]
    fn test_error_severity_colors() {
        assert_eq!(ErrorSeverity::Critical.color(), Color::Red);
        assert_eq!(ErrorSeverity::Error.color(), Color::LightRed);
        assert_eq!(ErrorSeverity::Warning.color(), Color::Yellow);
        assert_eq!(ErrorSeverity::Info.color(), Color::Cyan);
    }

    #[test]
    fn test_error_severity_icons() {
        assert_eq!(ErrorSeverity::Critical.icon(), "🔴");
        assert_eq!(ErrorSeverity::Error.icon(), "✗");
        assert_eq!(ErrorSeverity::Warning.icon(), "⚠");
        assert_eq!(ErrorSeverity::Info.icon(), "ℹ");
    }

    #[test]
    fn test_error_manager_default() {
        let mut manager = ErrorManager::default();
        assert!(!manager.is_showing());
        assert_eq!(manager.history().len(), 0);
    }
}
