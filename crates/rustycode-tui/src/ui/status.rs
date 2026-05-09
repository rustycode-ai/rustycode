//! Status bar and status indicators for RustyCode TUI

#![allow(dead_code)]

use ratatui::style::{Color, Style};
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::animator::AnimationFrame;
use super::progress::ToolProgress;

// Import compaction types for token display
use crate::memory::compaction::{ContextMonitor, UsageColor};
use unicode_width::UnicodeWidthStr;

/// Configuration for status display
#[derive(Clone, Debug)]
pub struct StatusConfig {
    pub animations_enabled: bool,
    /// Show tool indicators inline
    pub show_tool_indicators: bool,
    /// Show progress bars for operations >1 second
    pub show_progress_bars: bool,
    /// Show elapsed time for tools
    pub show_elapsed_time: bool,
    pub show_thinking_indicator: bool,
    /// Reduced motion mode (accessibility)
    pub reduced_motion: bool,
}

impl Default for StatusConfig {
    fn default() -> Self {
        Self {
            animations_enabled: true,
            show_tool_indicators: true,
            show_progress_bars: true,
            show_elapsed_time: true,
            show_thinking_indicator: false,
            reduced_motion: false,
        }
    }
}

/// Application status indicator
#[derive(Clone, Debug)]
pub struct StatusIndicator {
    /// Icon representing the status
    pub icon: &'static str,
    /// Status text description
    pub text: String,
    pub animating: bool,
    /// Color for the indicator
    pub color: Color,
    /// Optional detailed message
    pub detail: Option<String>,
}

impl StatusIndicator {
    pub fn new(icon: &'static str, text: impl Into<String>, animating: bool, color: Color) -> Self {
        Self {
            icon,
            text: text.into(),
            animating,
            color,
            detail: None,
        }
    }

    /// Add detail message
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Ready status (idle, waiting for input)
    pub fn ready() -> Self {
        Self::new("[OK]", "Ready", false, Color::Green)
    }

    /// Thinking status (AI is generating)
    pub fn thinking() -> Self {
        Self::new("[*]", "Thinking", true, Color::Cyan)
    }

    /// Executing tool status
    pub fn executing(tool_name: &str) -> Self {
        Self::new(
            "[W]",
            format!("Executing: {}", tool_name),
            true,
            Color::Magenta,
        )
    }

    /// Error status
    pub fn error(message: &str) -> Self {
        Self::new("[!]", format!("Error: {}", message), false, Color::Red)
    }

    /// Loading status
    pub fn loading(what: &str) -> Self {
        Self::new("[...]", format!("Loading {}", what), true, Color::Yellow)
    }

    /// Cancelling status
    pub fn cancelling() -> Self {
        Self::new("[!]", "Cancelling...", true, Color::Yellow)
    }

    /// Cancelled status
    pub fn cancelled() -> Self {
        Self::new("[!]", "Cancelled", false, Color::Gray)
    }

    /// Render the indicator with animation frame
    pub fn render(&self, anim: &AnimationFrame) -> Line<'_> {
        let icon = if self.animating {
            anim.cursor.to_string()
        } else {
            self.icon.to_string()
        };

        let mut spans = vec![
            Span::styled(icon, Style::default().fg(self.color)),
            Span::raw(" "),
            Span::styled(&self.text, Style::default().fg(Color::Gray)),
        ];

        // Add detail if available
        if let Some(detail) = &self.detail {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("({})", detail),
                Style::default().fg(Color::DarkGray),
            ));
        }

        Line::from(spans)
    }
}

/// Status bar widget
///
/// Displays current application state at the bottom of the screen.
/// Always visible and shows the most important status information.
pub struct StatusBar {
    /// Status configuration
    config: StatusConfig,
    /// Show token usage in status bar
    show_token_usage: bool,
}

impl StatusBar {
    pub fn new(config: StatusConfig) -> Self {
        Self {
            config,
            show_token_usage: true,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(StatusConfig::default())
    }

    /// Get the current configuration (for testing)
    #[cfg(test)]
    pub fn config(&self) -> &StatusConfig {
        &self.config
    }

    /// Create with reduced motion enabled
    pub fn reduced_motion() -> Self {
        Self {
            config: StatusConfig {
                reduced_motion: true,
                animations_enabled: false,
                ..Default::default()
            },
            show_token_usage: true,
        }
    }

    /// Set whether to show token usage
    pub fn with_token_usage(mut self, show: bool) -> Self {
        self.show_token_usage = show;
        self
    }

    /// Render the status bar
    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        status: &Status,
        anim: &AnimationFrame,
        context_monitor: Option<&ContextMonitor>,
    ) {
        let indicator = status.indicator();

        // Build status line
        let mut spans = vec![
            // Status icon (animated if applicable)
            Span::styled(
                if indicator.animating && self.config.animations_enabled {
                    anim.cursor.to_string()
                } else {
                    indicator.icon.to_string()
                },
                Style::default().fg(indicator.color),
            ),
            // Space separator
            Span::raw(" "),
            // Status text
            Span::styled(indicator.text.clone(), Style::default().fg(Color::Gray)),
        ];

        // Add tool count and progress if tools are running
        if let Status::ExecutingTools {
            remaining_tools,
            total_tools,
            progress_percentage,
            ..
        } = status
        {
            let completed = total_tools.saturating_sub(*remaining_tools);
            if *total_tools > 0 {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("({}/{})", completed, total_tools),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            // Add progress bar or percentage
            if let Some(pct) = progress_percentage {
                spans.push(Span::raw(" "));
                // Small 10-char progress bar
                let filled: usize = (pct * 10 / 100).min(10);
                let bar = "█".repeat(filled) + &"░".repeat(10usize.saturating_sub(filled));
                spans.push(Span::styled(
                    format!("[{}] {}%", bar, pct),
                    Style::default().fg(Color::Cyan),
                ));
            }
        }

        // Add token usage if enabled and monitor provided
        if self.show_token_usage {
            if let Some(monitor) = context_monitor {
                spans.push(Span::raw(" │ "));

                let usage_pct = (monitor.usage_percentage() * 100.0).min(999.0) as usize;

                // Color: green <50%, yellow <80%, red <100%, bright red >=100%
                let token_color = if usage_pct >= 100 {
                    Color::LightRed
                } else {
                    match monitor.usage_color() {
                        UsageColor::Green => Color::Green,
                        UsageColor::Yellow => Color::Yellow,
                        UsageColor::Red => Color::Red,
                    }
                };

                let fmt_tokens = |n: usize| -> String {
                    if n >= 1_000_000 {
                        format!("{:.1}M", n as f64 / 1_000_000.0)
                    } else if n >= 1_000 {
                        format!("{:.1}k", n as f64 / 1_000.0)
                    } else {
                        n.to_string()
                    }
                };

                let current = fmt_tokens(monitor.current_tokens);
                let max = fmt_tokens(monitor.max_tokens);

                let bar_width = 10usize;
                let filled =
                    ((usage_pct.min(100) as f64 / 100.0) * bar_width as f64).round() as usize;
                let filled = filled.min(bar_width);
                let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(bar_width - filled));

                spans.push(Span::styled(
                    format!("ctx [{}] {}% {}/{}", bar, usage_pct, current, max),
                    Style::default().fg(token_color),
                ));
            }
        }

        let line = Line::from(spans);

        let line = if line.width() > area.width.saturating_sub(2) as usize {
            let max_w = area.width.saturating_sub(5) as usize;
            let mut truncated_spans = Vec::new();
            let mut used = 0;
            for span in line.spans {
                let sw = span.width();
                if used + sw > max_w {
                    let remaining = max_w.saturating_sub(used);
                    if remaining > 3 {
                        truncated_spans.push(Span::raw(
                            span.content
                                .chars()
                                .take(remaining.saturating_sub(3))
                                .collect::<String>(),
                        ));
                    }
                    truncated_spans.push(Span::styled(
                        "…".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                    break;
                }
                truncated_spans.push(span);
                used += sw;
            }
            Line::from(truncated_spans)
        } else {
            line
        };

        let bar = Paragraph::new(vec![line]).block(
            Block::default()
                .borders(Borders::ALL | Borders::LEFT)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        f.render_widget(bar, area);
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Application status
///
/// Represents the current high-level state of the application.
#[derive(Clone, Debug)]
pub enum Status {
    /// Ready and waiting for input
    Ready,
    /// AI is thinking/generating response
    Thinking {
        /// Number of chunks received
        chunks_received: usize,
    },
    /// Executing one or more tools
    ExecutingTools {
        /// Number of tools remaining
        remaining_tools: usize,
        /// Total number of tools
        total_tools: usize,
        /// Current tool name
        current_tool: Option<String>,
        /// Progress percentage (0-100)
        progress_percentage: Option<usize>,
    },
    /// Tool execution encountered an error
    ToolError {
        /// Error message
        error: String,
        /// Tool that failed
        tool_name: Option<String>,
    },
    /// Loading workspace/context
    Loading {
        /// What is being loaded
        what: String,
    },
    /// Cancelling current operation
    Cancelling,
    /// General error state
    Error {
        /// Error message
        message: String,
        /// Suggested fixes
        suggestions: Vec<String>,
    },
}

impl Status {
    pub fn indicator(&self) -> StatusIndicator {
        match self {
            Status::Ready => StatusIndicator::ready(),
            Status::Thinking { .. } => StatusIndicator::thinking(),
            Status::ExecutingTools {
                current_tool,
                progress_percentage,
                ..
            } => {
                let mut indicator = if let Some(tool) = current_tool {
                    StatusIndicator::executing(tool)
                } else {
                    StatusIndicator::executing("tool")
                };
                // Add progress percentage as detail
                if let Some(pct) = progress_percentage {
                    indicator = indicator.with_detail(format!("{}%", pct));
                }
                indicator
            }
            Status::ToolError { error, .. } => StatusIndicator::error(error),
            Status::Loading { what } => StatusIndicator::loading(what),
            Status::Cancelling => StatusIndicator::cancelling(),
            Status::Error { message, .. } => StatusIndicator::error(message),
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Status::Ready => Color::Green,
            Status::Thinking { .. } => Color::Cyan,
            Status::ExecutingTools { .. } => Color::Magenta,
            Status::ToolError { .. } => Color::Red,
            Status::Loading { .. } => Color::Yellow,
            Status::Cancelling => Color::Yellow,
            Status::Error { .. } => Color::Red,
        }
    }

    pub fn is_animating(&self) -> bool {
        matches!(
            self,
            Status::Thinking { .. }
                | Status::ExecutingTools { .. }
                | Status::Loading { .. }
                | Status::Cancelling
        )
    }

    /// Get a text description suitable for screen readers
    pub fn accessible_text(&self) -> String {
        match self {
            Status::Ready => "Ready, waiting for input".to_string(),
            Status::Thinking { chunks_received } => {
                format!("Thinking, received {} chunks", chunks_received)
            }
            Status::ExecutingTools {
                remaining_tools,
                total_tools,
                current_tool,
                progress_percentage,
            } => {
                let progress = progress_percentage
                    .map(|p| format!(", {}% complete", p))
                    .unwrap_or_default();
                if let Some(tool) = current_tool {
                    format!(
                        "Executing tool {} ({}/{}{}), {} tools remaining",
                        tool,
                        total_tools.saturating_sub(*remaining_tools),
                        total_tools,
                        progress,
                        remaining_tools
                    )
                } else {
                    format!(
                        "Executing tools ({}/{}{}), {} remaining",
                        total_tools.saturating_sub(*remaining_tools),
                        total_tools,
                        progress,
                        remaining_tools
                    )
                }
            }
            Status::ToolError { error, tool_name } => {
                if let Some(tool) = tool_name {
                    format!("Tool {} encountered error: {}", tool, error)
                } else {
                    format!("Tool error: {}", error)
                }
            }
            Status::Loading { what } => format!("Loading {}", what),
            Status::Cancelling => "Cancelling operation".to_string(),
            Status::Error { message, .. } => format!("Error: {}", message),
        }
    }
}

/// Collection of tool execution states
#[derive(Clone, Debug, Default)]
pub struct ToolExecutions {
    /// Active tool executions
    pub tools: Vec<ToolProgress>,
}

impl ToolExecutions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new tool execution
    pub fn add(&mut self, tool: ToolProgress) {
        self.tools.push(tool);
    }

    pub fn running_count(&self) -> usize {
        self.tools
            .iter()
            .filter(|t| t.status == super::progress::ToolStatus::Running)
            .count()
    }

    pub fn current_tool(&self) -> Option<&ToolProgress> {
        self.tools
            .iter()
            .find(|t| t.status == super::progress::ToolStatus::Running)
    }

    pub fn most_recent(&self) -> Option<&ToolProgress> {
        self.tools.last()
    }

    /// Update tool progress by name
    pub fn update_progress(&mut self, name: &str, current: usize, total: usize) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.update_progress(current, total);
        }
    }

    /// Complete a tool by name
    pub fn complete(&mut self, name: &str, result: String) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.complete(result);
        }
    }

    /// Fail a tool by name
    pub fn fail(&mut self, name: &str, error: String) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.fail(error);
        }
    }

    /// Cancel a tool by name
    pub fn cancel(&mut self, name: &str) {
        if let Some(tool) = self.tools.iter_mut().find(|t| t.name == name) {
            tool.cancel();
        }
    }

    /// Clear completed tools older than a certain count
    pub fn trim_completed(&mut self, keep_last: usize) {
        // Collect indices of completed tools to remove
        let completed_indices: Vec<usize> = self
            .tools
            .iter()
            .enumerate()
            .filter(|(_, tool)| {
                matches!(
                    tool.status,
                    super::progress::ToolStatus::Complete
                        | super::progress::ToolStatus::Failed
                        | super::progress::ToolStatus::Cancelled
                )
            })
            .map(|(i, _)| i)
            .collect();

        // If we have more completed tools than we want to keep, remove the oldest ones
        if completed_indices.len() > keep_last {
            let remove_count = completed_indices.len() - keep_last;
            let indices_to_remove: Vec<usize> =
                completed_indices.into_iter().take(remove_count).collect();

            // Remove in reverse order to avoid index shifting issues
            for i in indices_to_remove.into_iter().rev() {
                self.tools.remove(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_indicator_creation() {
        let indicator = StatusIndicator::new("[*]", "Testing", false, Color::Yellow);
        assert_eq!(indicator.icon, "[*]");
        assert_eq!(indicator.text, "Testing");
        assert!(!indicator.animating);
        assert_eq!(indicator.color, Color::Yellow);
    }

    #[test]
    fn test_status_indicator_defaults() {
        assert!(matches!(StatusIndicator::ready().icon, "[OK]"));
        assert!(matches!(StatusIndicator::thinking().icon, "[*]"));
        assert!(matches!(StatusIndicator::error("test").icon, "[!]"));
        assert!(StatusIndicator::thinking().animating);
        assert!(!StatusIndicator::ready().animating);
    }

    #[test]
    fn test_status_indicator_rendering() {
        let indicator = StatusIndicator::thinking();
        let anim = AnimationFrame::default();

        let line = indicator.render(&anim);
        // Should contain the animated cursor
        assert_eq!(line.spans[0].content, anim.cursor.to_string());
    }

    #[test]
    fn test_status_ready() {
        let status = Status::Ready;
        assert!(!status.is_animating());
        assert_eq!(status.color(), Color::Green);
        assert_eq!(status.accessible_text(), "Ready, waiting for input");
    }

    #[test]
    fn test_status_thinking() {
        let status = Status::Thinking { chunks_received: 5 };
        assert!(status.is_animating());
        assert_eq!(status.color(), Color::Cyan);
        assert!(status.accessible_text().contains("5 chunks"));
    }

    #[test]
    fn test_status_executing_tools() {
        let status = Status::ExecutingTools {
            remaining_tools: 2,
            total_tools: 5,
            current_tool: Some("Read".to_string()),
            progress_percentage: Some(60),
        };
        assert!(status.is_animating());
        assert_eq!(status.color(), Color::Magenta);
        assert!(status.accessible_text().contains("Read"));
        assert!(status.accessible_text().contains("60%"));
    }

    #[test]
    fn test_status_error() {
        let status = Status::Error {
            message: "File not found".to_string(),
            suggestions: vec!["Check the path".to_string()],
        };
        assert!(!status.is_animating());
        assert_eq!(status.color(), Color::Red);
        assert!(status.accessible_text().contains("File not found"));
    }

    #[test]
    fn test_status_bar_default() {
        let bar = StatusBar::default();
        assert!(bar.config.animations_enabled);
        assert!(!bar.config.reduced_motion);
    }

    #[test]
    fn test_status_bar_reduced_motion() {
        let bar = StatusBar::reduced_motion();
        assert!(!bar.config.animations_enabled);
        assert!(bar.config.reduced_motion);
    }

    #[test]
    fn test_tool_executions_lifecycle() {
        let mut tools = ToolExecutions::new();

        // Add a tool
        tools.add(ToolProgress::new("test_tool"));
        assert_eq!(tools.tools.len(), 1);

        // Update progress
        tools.update_progress("test_tool", 5, 10);
        assert_eq!(tools.tools[0].progress.as_ref().unwrap().current, 5);

        // Complete it
        tools.complete("test_tool", "Success".to_string());
        use crate::ui::progress::ToolStatus;
        assert_eq!(tools.tools[0].status, ToolStatus::Complete);
    }

    #[test]
    fn test_tool_executions_running_count() {
        let mut tools = ToolExecutions::new();

        let mut tool1 = ToolProgress::new("tool1");
        tool1.start();
        tools.add(tool1);

        let tool2 = ToolProgress::new("tool2");
        tools.add(tool2); // Still pending

        assert_eq!(tools.running_count(), 1);
    }

    #[test]
    fn test_tool_executions_trim() {
        let mut tools = ToolExecutions::new();

        // Add multiple completed tools
        for i in 0..5 {
            let mut tool = ToolProgress::new(format!("tool{}", i));
            tool.complete(format!("Result {}", i));
            tools.add(tool);
        }

        // Trim to keep only last 2
        tools.trim_completed(2);
        assert_eq!(tools.tools.len(), 2);
        assert_eq!(tools.tools[0].name, "tool3");
        assert_eq!(tools.tools[1].name, "tool4");
    }

    #[test]
    fn test_status_config_default() {
        let config = StatusConfig::default();
        assert!(config.animations_enabled);
        assert!(config.show_tool_indicators);
        assert!(config.show_progress_bars);
        assert!(!config.show_thinking_indicator);
        assert!(!config.reduced_motion);
    }
}
