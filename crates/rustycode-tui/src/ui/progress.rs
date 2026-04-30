//! Progress tracking for long-running operations
//!
//! Provides progress bars, percentage completion, and ETA calculation
//! for tool execution and other long operations.

// Complete implementation - pending integration with tool execution display
#![allow(dead_code)]

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::time::{Duration, Instant};

/// Progress information for a long-running operation
#[derive(Clone, Debug)]
pub struct Progress {
    /// Current progress value
    pub current: usize,
    /// Total target value
    pub total: usize,
    /// Human-readable message
    pub message: String,
    /// Optional additional context
    pub context: Option<String>,
}

impl Progress {
    /// Create a new progress tracker
    pub fn new(current: usize, total: usize, message: impl Into<String>) -> Self {
        Self {
            current,
            total,
            message: message.into(),
            context: None,
        }
    }

    /// Update the current progress
    pub fn update(&mut self, current: usize) {
        self.current = current.min(self.total);
    }

    /// Add context information
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    /// Calculate completion percentage (0-100)
    pub fn percentage(&self) -> usize {
        if self.total == 0 {
            return 0;
        }
        ((self.current as f32 / self.total as f32) * 100.0).min(100.0) as usize
    }

    /// Check if progress is complete
    pub fn is_complete(&self) -> bool {
        self.current >= self.total
    }
}

/// Tool execution with progress tracking
#[derive(Clone, Debug)]
pub struct ToolProgress {
    /// Tool name
    pub name: String,
    /// Current status
    pub status: ToolStatus,
    /// Start time
    pub start_time: Instant,
    /// End time (if complete)
    pub end_time: Option<Instant>,
    /// Progress tracker (optional)
    pub progress: Option<Progress>,
    /// Result/error message
    pub result: Option<String>,
    /// Number of tokens used (for LLM tools)
    pub tokens_used: usize,
}

/// Status of a tool execution
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolStatus {
    /// Tool is pending (queued but not started)
    Pending,
    /// Tool is currently running
    Running,
    /// Tool completed successfully
    Complete,
    /// Tool failed with an error
    Failed,
    /// Tool was cancelled by user
    Cancelled,
}

impl ToolProgress {
    /// Create a new tool progress tracker
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name,
            status: ToolStatus::Pending,
            start_time: Instant::now(),
            end_time: None,
            progress: None,
            result: None,
            tokens_used: 0,
        }
    }

    /// Start the tool execution
    pub fn start(&mut self) {
        self.status = ToolStatus::Running;
        self.start_time = Instant::now();
    }

    /// Update progress (if tracking)
    pub fn update_progress(&mut self, current: usize, total: usize) {
        if let Some(ref mut progress) = self.progress {
            progress.update(current);
        } else {
            self.progress = Some(Progress::new(current, total, "Processing"));
        }
    }

    /// Complete the tool successfully
    pub fn complete(&mut self, result: impl Into<String>) {
        self.status = ToolStatus::Complete;
        self.end_time = Some(Instant::now());
        self.result = Some(result.into());
    }

    /// Mark the tool as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = ToolStatus::Failed;
        self.end_time = Some(Instant::now());
        self.result = Some(error.into());
    }

    /// Cancel the tool
    pub fn cancel(&mut self) {
        self.status = ToolStatus::Cancelled;
        self.end_time = Some(Instant::now());
        self.result = Some("Cancelled by user".to_string());
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Duration {
        if let Some(end) = self.end_time {
            end.duration_since(self.start_time)
        } else {
            self.start_time.elapsed()
        }
    }

    /// Format elapsed time as a human-readable string
    pub fn format_elapsed(&self) -> String {
        let elapsed = self.elapsed();
        let secs = elapsed.as_secs_f32();
        if secs < 1.0 {
            format!("{}ms", elapsed.as_millis())
        } else if secs < 60.0 {
            format!("{:.1}s", secs)
        } else {
            let mins = (secs / 60.0) as u32;
            let rem_secs = secs % 60.0;
            format!("{}m {:.1}s", mins, rem_secs)
        }
    }

    /// Calculate ETA based on progress
    pub fn estimate_eta(&self) -> Option<Duration> {
        let progress = self.progress.as_ref()?;
        if progress.current == 0 || progress.is_complete() {
            return None;
        }

        let elapsed = self.elapsed();
        let rate = elapsed.as_secs_f32() / progress.current as f32;
        let remaining = progress.total.saturating_sub(progress.current);
        Some(Duration::from_secs_f32(rate * remaining as f32))
    }

    /// Format ETA as human-readable string
    pub fn format_eta(&self) -> Option<String> {
        let eta = self.estimate_eta()?;
        let secs = eta.as_secs_f32();
        if secs < 1.0 {
            Some(format!("ETA: {}ms", eta.as_millis()))
        } else if secs < 60.0 {
            Some(format!("ETA: {:.1}s", secs))
        } else {
            let mins = (secs / 60.0) as u32;
            let rem_secs = secs % 60.0;
            Some(format!("ETA: {}m {:.1}s", mins, rem_secs))
        }
    }
}

/// Renderer for progress bars and indicators
pub struct ProgressRenderer {
    /// Width of progress bar in characters
    bar_width: usize,
}

impl ProgressRenderer {
    /// Create a new progress renderer
    pub fn new(bar_width: usize) -> Self {
        Self { bar_width }
    }

    /// Create with default width (20 characters)
    pub fn default_width() -> Self {
        Self::new(20)
    }

    /// Render a progress bar as a string
    pub fn render_bar(&self, progress: &Progress) -> String {
        let pct = progress.percentage();
        let filled = (pct * self.bar_width / 100).min(self.bar_width);
        let empty = self.bar_width.saturating_sub(filled);

        let bar = "│".repeat(filled) + &" ".repeat(empty);
        format!("[{}] {}%", bar, pct)
    }

    /// Render progress with timing information
    pub fn render_with_timing(&self, progress: &ToolProgress) -> String {
        let status_icon = match progress.status {
            ToolStatus::Pending => "⏸",
            ToolStatus::Running => "⏳",
            ToolStatus::Complete => "✅",
            ToolStatus::Failed => "❌",
            ToolStatus::Cancelled => "⚠",
        };

        if let Some(ref prog) = progress.progress {
            let bar = self.render_bar(prog);
            let elapsed = progress.format_elapsed();
            let eta = progress.format_eta().unwrap_or_default();

            format!(
                "{} {} {} | {} - {} {}",
                status_icon, progress.name, bar, prog.message, elapsed, eta
            )
        } else {
            format!(
                "{} {} - {}",
                status_icon,
                progress.name,
                progress.format_elapsed()
            )
        }
    }

    /// Render progress as a ratatui Line
    pub fn render_line(&self, progress: &ToolProgress) -> Line<'_> {
        let color = match progress.status {
            ToolStatus::Pending => Color::Yellow,
            ToolStatus::Running => Color::Cyan,
            ToolStatus::Complete => Color::Green,
            ToolStatus::Failed => Color::Red,
            ToolStatus::Cancelled => Color::Gray,
        };

        let text = self.render_with_timing(progress);
        Line::from(vec![Span::styled(text, Style::default().fg(color))])
    }

    /// Render progress as a ratatui Widget
    pub fn render_widget(&self, progress: &ToolProgress) -> Paragraph<'_> {
        let line = self.render_line(progress);
        Paragraph::new(vec![line])
    }
}

impl Default for ProgressRenderer {
    fn default() -> Self {
        Self::default_width()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_creation() {
        let progress = Progress::new(5, 10, "Processing files");
        assert_eq!(progress.current, 5);
        assert_eq!(progress.total, 10);
        assert_eq!(progress.message, "Processing files");
        assert_eq!(progress.percentage(), 50);
    }

    #[test]
    fn test_progress_update() {
        let mut progress = Progress::new(0, 100, "Loading");
        assert_eq!(progress.percentage(), 0);

        progress.update(50);
        assert_eq!(progress.percentage(), 50);

        progress.update(100);
        assert_eq!(progress.percentage(), 100);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_progress_context() {
        let progress = Progress::new(1, 2, "Test").with_context("Additional info");
        assert_eq!(progress.context.as_deref(), Some("Additional info"));
    }

    #[test]
    fn test_tool_progress_lifecycle() {
        let mut tool = ToolProgress::new("read_file");

        assert_eq!(tool.status, ToolStatus::Pending);

        tool.start();
        assert_eq!(tool.status, ToolStatus::Running);

        tool.complete("Success");
        assert_eq!(tool.status, ToolStatus::Complete);
        assert_eq!(tool.result.as_deref(), Some("Success"));
        assert!(tool.end_time.is_some());
    }

    #[test]
    fn test_tool_progress_failure() {
        let mut tool = ToolProgress::new("failing_tool");
        tool.start();
        tool.fail("File not found");
        assert_eq!(tool.status, ToolStatus::Failed);
        assert!(tool
            .result
            .as_ref()
            .expect("Tool result should exist")
            .contains("File not found"));
    }

    #[test]
    fn test_tool_progress_cancellation() {
        let mut tool = ToolProgress::new("long_tool");
        tool.start();
        tool.cancel();
        assert_eq!(tool.status, ToolStatus::Cancelled);
    }

    #[test]
    fn test_elapsed_time() {
        let tool = ToolProgress::new("test");
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = tool.format_elapsed();
        assert!(elapsed.contains("ms") || elapsed.contains("s"));
    }

    #[test]
    fn test_eta_calculation() {
        let mut tool = ToolProgress::new("test");
        tool.start();
        tool.update_progress(5, 10);

        // Give some time to establish rate
        std::thread::sleep(Duration::from_millis(50));
        tool.update_progress(6, 10);

        let eta = tool.format_eta();
        assert!(
            eta.is_some(),
            "Should calculate ETA when progress is being made"
        );
    }

    #[test]
    fn test_progress_renderer() {
        let renderer = ProgressRenderer::new(10);
        let progress = Progress::new(5, 10, "Processing");

        let bar = renderer.render_bar(&progress);
        assert!(bar.contains("["));
        assert!(bar.contains("50%"));
        assert!(bar.contains("│"));
    }

    #[test]
    fn test_progress_render_with_timing() {
        let renderer = ProgressRenderer::default_width();
        let mut tool = ToolProgress::new("test_tool");
        tool.start();
        tool.update_progress(3, 10);

        let rendered = renderer.render_with_timing(&tool);
        assert!(rendered.contains("test_tool"));
        assert!(rendered.contains("⏳"));
    }

    #[test]
    fn test_tool_status_icons() {
        let renderer = ProgressRenderer::default_width();

        for (status, expected_icon) in [
            (ToolStatus::Pending, "⏸"),
            (ToolStatus::Running, "⏳"),
            (ToolStatus::Complete, "✅"),
            (ToolStatus::Failed, "❌"),
            (ToolStatus::Cancelled, "⚠"),
        ] {
            let mut tool = ToolProgress::new("test");
            tool.status = status;
            let rendered = renderer.render_with_timing(&tool);
            assert!(
                rendered.contains(expected_icon),
                "Expected icon {} for status {:?}, got: {}",
                expected_icon,
                status,
                rendered
            );
        }
    }
}
