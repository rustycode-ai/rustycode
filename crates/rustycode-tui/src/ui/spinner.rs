//! Animated spinner component for async operations

#![allow(dead_code)]

use super::animator::AnimationFrame;
use crate::theme::ThemeColors;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Spinner style variants
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SpinnerStyle {
    /// Simple dots animation
    #[default]
    Dots,
    /// Rotating line (|/-\)
    Line,
    /// Arc animation (◐ ◓ ◑ ◒)
    Arc,
    /// Pulse animation (● ○)
    Pulse,
    /// Arrow animation (▹ ▸)
    Arrow,
    /// Clock animation (🕐 🕑 🕒)
    Clock,
}

/// Spinner status with associated color
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinnerStatus {
    /// Operation is in progress (animating)
    Working,
    /// Operation completed successfully
    Done,
    /// Operation encountered an error
    Error,
    /// Operation was cancelled
    Cancelled,
}

impl SpinnerStatus {
    pub fn color(&self) -> Color {
        match self {
            SpinnerStatus::Working => Color::Cyan,
            SpinnerStatus::Done => Color::Green,
            SpinnerStatus::Error => Color::Red,
            SpinnerStatus::Cancelled => Color::Gray,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            SpinnerStatus::Working => "⏳",
            SpinnerStatus::Done => "✓",
            SpinnerStatus::Error => "✗",
            SpinnerStatus::Cancelled => "⚠",
        }
    }
}

/// Animated spinner for async operations
#[derive(Clone, Debug)]
pub struct Spinner {
    style: SpinnerStyle,
    status: SpinnerStatus,
    label: Option<String>,
    reduced_motion: bool,
}

impl Spinner {
    pub fn new(style: SpinnerStyle) -> Self {
        Self {
            style,
            status: SpinnerStatus::Working,
            label: None,
            reduced_motion: false,
        }
    }

    /// Create with default style (dots)
    pub fn default_style() -> Self {
        Self::new(SpinnerStyle::default())
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_reduced_motion(mut self, reduced: bool) -> Self {
        self.reduced_motion = reduced;
        self
    }

    pub fn with_status(mut self, status: SpinnerStatus) -> Self {
        self.status = status;
        self
    }

    fn get_char(&self, frame: &AnimationFrame) -> char {
        if self.reduced_motion || !matches!(self.status, SpinnerStatus::Working) {
            return self.status.icon().chars().next().unwrap_or('⏳');
        }

        match self.style {
            SpinnerStyle::Dots => match frame.progress_frame % 4 {
                0 => '⠋',
                1 => '⠙',
                2 => '⠹',
                _ => '⠸',
            },
            SpinnerStyle::Line => match frame.progress_frame % 4 {
                0 => '|',
                1 => '/',
                2 => '-',
                _ => '\\',
            },
            SpinnerStyle::Arc => match frame.progress_frame % 4 {
                0 => '◐',
                1 => '◓',
                2 => '◑',
                _ => '◒',
            },
            SpinnerStyle::Pulse => {
                if frame.is_active {
                    '●'
                } else {
                    '○'
                }
            }
            SpinnerStyle::Arrow => {
                if frame.is_active {
                    '▹'
                } else {
                    '▸'
                }
            }
            SpinnerStyle::Clock => match frame.progress_frame % 4 {
                0 => '🕐',
                1 => '🕑',
                2 => '🕒',
                _ => '🕓',
            },
        }
    }

    fn get_dots(&self, frame: &AnimationFrame) -> &'static str {
        if self.reduced_motion || !matches!(self.status, SpinnerStatus::Working) {
            return "";
        }
        frame.dots
    }

    /// Render the spinner as a ratatui Line
    pub fn render_line(
        &self,
        frame: &AnimationFrame,
        theme_colors: Option<&ThemeColors>,
    ) -> Line<'_> {
        let color = theme_colors
            .map(|tc| tc.spinner_status_color(&self.status))
            .unwrap_or_else(|| self.status.color());
        let char = self.get_char(frame);
        let dots = self.get_dots(frame);

        let mut spans = vec![Span::styled(char.to_string(), Style::default().fg(color))];

        if let Some(ref label) = self.label {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                label.clone(),
                Style::default().fg(theme_colors.map(|tc| tc.muted).unwrap_or(Color::Gray)),
            ));
            if !dots.is_empty() {
                spans.push(Span::styled(dots.to_string(), Style::default().fg(color)));
            }
        }

        Line::from(spans)
    }

    /// Render the spinner as a ratatui Widget
    pub fn render_widget(
        &self,
        frame: &AnimationFrame,
        theme_colors: Option<&ThemeColors>,
    ) -> Paragraph<'_> {
        let line = self.render_line(frame, theme_colors);
        Paragraph::new(vec![line])
    }

    /// Render just the spinner character (no label)
    pub fn render_char(
        &self,
        frame: &AnimationFrame,
        theme_colors: Option<&ThemeColors>,
    ) -> Span<'_> {
        let color = theme_colors
            .map(|tc| tc.spinner_status_color(&self.status))
            .unwrap_or_else(|| self.status.color());
        let char = self.get_char(frame);
        Span::styled(char.to_string(), Style::default().fg(color))
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::default_style()
    }
}

/// Convenience builders for common spinner patterns
impl Spinner {
    /// Create a "thinking" spinner (for AI generation)
    pub fn thinking() -> Self {
        Self::new(SpinnerStyle::Pulse).with_label("Thinking")
    }

    /// Create a "loading" spinner (for file operations)
    pub fn loading(what: impl Into<String>) -> Self {
        Self::new(SpinnerStyle::Dots).with_label(format!("Loading {}", what.into()))
    }

    /// Create a "working" spinner (for tool execution)
    pub fn working(tool_name: impl Into<String>) -> Self {
        Self::new(SpinnerStyle::Line).with_label(format!("Executing {}", tool_name.into()))
    }

    /// Create a "network" spinner (for API requests)
    pub fn network() -> Self {
        Self::new(SpinnerStyle::Arc).with_label("Connecting")
    }

    /// Create a completed spinner
    pub fn completed(message: impl Into<String>) -> Self {
        Self::new(SpinnerStyle::Dots)
            .with_status(SpinnerStatus::Done)
            .with_label(message.into())
    }

    /// Create an error spinner
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(SpinnerStyle::Dots)
            .with_status(SpinnerStatus::Error)
            .with_label(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_creation() {
        let spinner = Spinner::new(SpinnerStyle::Dots);
        assert_eq!(spinner.style, SpinnerStyle::Dots);
        assert_eq!(spinner.status, SpinnerStatus::Working);
        assert!(spinner.label.is_none());
    }

    #[test]
    fn test_spinner_with_label() {
        let spinner = Spinner::default().with_label("Test");
        assert_eq!(spinner.label.as_deref(), Some("Test"));
    }

    #[test]
    fn test_spinner_with_status() {
        let spinner = Spinner::default().with_status(SpinnerStatus::Done);
        assert_eq!(spinner.status, SpinnerStatus::Done);
    }

    #[test]
    fn test_spinner_styles() {
        for style in [
            SpinnerStyle::Dots,
            SpinnerStyle::Line,
            SpinnerStyle::Arc,
            SpinnerStyle::Pulse,
            SpinnerStyle::Arrow,
            SpinnerStyle::Clock,
        ] {
            let spinner = Spinner::new(style);
            assert_eq!(spinner.style, style);
        }
    }

    #[test]
    fn test_spinner_status_colors() {
        assert_eq!(SpinnerStatus::Working.color(), Color::Cyan);
        assert_eq!(SpinnerStatus::Done.color(), Color::Green);
        assert_eq!(SpinnerStatus::Error.color(), Color::Red);
        assert_eq!(SpinnerStatus::Cancelled.color(), Color::Gray);
    }

    #[test]
    fn test_spinner_render_char() {
        let spinner = Spinner::new(SpinnerStyle::Dots);
        let frame = AnimationFrame::default();
        let span = spinner.render_char(&frame, None);
        assert_eq!(span.content.chars().count(), 1);
    }

    #[test]
    fn test_spinner_render_line() {
        let spinner = Spinner::default().with_label("Test");
        let frame = AnimationFrame::default();
        let line = spinner.render_line(&frame, None);
        assert!(!line.spans.is_empty());
        assert!(line.spans.iter().any(|s| s.content.contains("Test")));
    }

    #[test]
    fn test_spinner_reduced_motion() {
        let spinner = Spinner::default()
            .with_reduced_motion(true)
            .with_label("Test");

        let frame = AnimationFrame::default();
        let line = spinner.render_line(&frame, None);

        // In reduced motion, should show static icon
        let has_label = line.spans.iter().any(|s| s.content.contains("Test"));
        assert!(has_label);
    }

    #[test]
    fn test_spinner_convenience_constructors() {
        let thinking = Spinner::thinking();
        assert!(thinking.label.as_ref().unwrap().contains("Thinking"));

        let loading = Spinner::loading("file");
        assert!(loading.label.as_ref().unwrap().contains("Loading file"));

        let working = Spinner::working("Read");
        assert!(working.label.as_ref().unwrap().contains("Executing Read"));

        let completed = Spinner::completed("Done");
        assert_eq!(completed.status, SpinnerStatus::Done);
        assert!(completed.label.as_ref().unwrap().contains("Done"));

        let error = Spinner::error("Failed");
        assert_eq!(error.status, SpinnerStatus::Error);
        assert!(error.label.as_ref().unwrap().contains("Failed"));
    }

    #[test]
    fn test_spinner_default() {
        let spinner = Spinner::default();
        assert_eq!(spinner.style, SpinnerStyle::Dots);
    }
}
