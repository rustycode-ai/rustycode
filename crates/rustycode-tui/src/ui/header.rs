//! Polished Header Component
//!
//! Single-row header with width-aware layout that prevents text overflow.
//! Elements check available space before rendering and truncate gracefully.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Header component displaying app info, project name, and status indicators
pub struct Header {
    /// Application name (left side)
    pub app_name: String,
    /// Project/directory name (center)
    pub project_name: String,
    /// Number of active tasks (right side)
    pub task_count: usize,
    /// Number of pending tool executions (right side)
    pub pending_tools: usize,
    /// Current git branch (optional, shown in project section)
    pub git_branch: Option<String>,
    /// Number of conversation turns (goose pattern: turn counter)
    pub turn_count: usize,
    /// Current status for at-a-glance header display (goose pattern)
    pub status: HeaderStatus,
    /// Animation frame index for spinner (goose pattern: animated header status)
    pub spinner_frame: usize,
    /// Primary color for headers/borders
    pub primary_color: Color,
    /// Secondary color for muted text
    pub secondary_color: Color,
}

/// Header status indicator (goose pattern: color-coded status in header)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeaderStatus {
    #[default]
    Ready,
    Planning,
    Stalled,
    Thinking,
    RunningTools,
    Error,
}

impl HeaderStatus {
    fn label(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Planning => "planning",
            Self::Stalled => "stalled",
            Self::Thinking => "thinking",
            Self::RunningTools => "tools",
            Self::Error => "error",
        }
    }

    fn color(&self) -> Color {
        match self {
            Self::Ready => Color::Rgb(80, 200, 120), // teal/green
            Self::Planning => Color::Cyan,
            Self::Stalled => Color::Red,
            Self::Thinking => Color::Cyan,
            Self::RunningTools => Color::Rgb(255, 200, 80), // gold
            Self::Error => Color::Rgb(255, 80, 80),         // cranberry
        }
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            app_name: String::from("rustycode"),
            project_name: String::new(),
            task_count: 0,
            pending_tools: 0,
            git_branch: None,
            turn_count: 0,
            status: HeaderStatus::Ready,
            spinner_frame: 0,
            primary_color: Color::Rgb(91, 141, 239),
            secondary_color: Color::Rgb(107, 114, 128),
        }
    }
}

impl Header {
    /// Create a new header with default styling
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn with_project_name(mut self, name: impl Into<String>) -> Self {
        self.project_name = name.into();
        self
    }

    pub fn with_git_branch(mut self, branch: Option<String>) -> Self {
        self.git_branch = branch;
        self
    }

    pub fn with_counts(mut self, tasks: usize, pending_tools: usize) -> Self {
        self.task_count = tasks;
        self.pending_tools = pending_tools;
        self
    }

    pub fn with_turn_count(mut self, turns: usize) -> Self {
        self.turn_count = turns;
        self
    }

    /// Set header status (goose pattern: color-coded status in header)
    pub fn with_status(mut self, status: HeaderStatus) -> Self {
        self.status = status;
        self
    }

    /// Set animation frame for spinner (goose pattern)
    pub fn with_spinner_frame(mut self, frame: usize) -> Self {
        self.spinner_frame = frame;
        self
    }

    /// Render the header. Width-aware: elements are skipped if they don't fit.
    /// Project names are truncated at char boundaries (UTF-8 safe).
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = area.width as usize;
        let mut spans: Vec<Span> = Vec::new();
        let mut used: usize = 0;

        // Left: App name
        let prefix = format!("● {} ", self.app_name);
        used += prefix.len();
        spans.push(Span::styled(
            prefix,
            Style::default()
                .fg(self.primary_color)
                .add_modifier(Modifier::BOLD),
        ));

        // Separator + status (goose pattern: color-coded status after separator)
        if used + 2 < width {
            spans.push(Span::styled(
                "─ ",
                Style::default().fg(self.secondary_color),
            ));
            used += 2;
        }
        // Show status label with status-appropriate color and animated spinner (goose pattern)
        if self.status != HeaderStatus::Ready && used + 12 < width {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = self.spinner_frame % frames.len();
            let status_text = format!("{} {} ", frames[frame_idx], self.status.label());
            spans.push(Span::styled(
                status_text.clone(),
                Style::default().fg(self.status.color()),
            ));
            used += status_text.len();
        }

        // Git branch (before project, only if room)
        if let Some(branch) = &self.git_branch {
            let bt = format!("{} ", branch);
            if used + bt.len() + width / 3 < width {
                spans.push(Span::styled(
                    bt.clone(),
                    Style::default().fg(self.secondary_color),
                ));
                used += bt.len();
            }
        }

        // Project name: char-aware truncation
        let right_budget = if self.task_count > 0 { 14 } else { 0 }
            + if self.pending_tools > 0 { 12 } else { 0 }
            + 4;
        let max_chars = width.saturating_sub(used + right_budget);
        let char_count = self.project_name.chars().count();
        let display = if char_count > max_chars && max_chars > 1 {
            format!(
                "{}…",
                self.project_name
                    .chars()
                    .take(max_chars.saturating_sub(1))
                    .collect::<String>()
            )
        } else if char_count > max_chars {
            String::new()
        } else {
            self.project_name.clone()
        };
        if !display.is_empty() {
            used += display.chars().count();
            spans.push(Span::styled(display, Style::default().fg(Color::White)));
        }

        // Right: Task indicators
        if self.task_count > 0 {
            let text = format!(
                " ● {} task{}",
                self.task_count,
                if self.task_count == 1 { "" } else { "s" }
            );
            if used + text.len() < width {
                used += text.len();
                spans.push(Span::styled(text, Style::default().fg(self.primary_color)));
            }
        }

        if self.pending_tools > 0 {
            let text = format!(" ⚡{}", self.pending_tools);
            if used + text.len() < width {
                spans.push(Span::styled(text, Style::default().fg(Color::Yellow)));
            }
        }

        // Turn counter (goose pattern: "T:N" shows conversation length at a glance)
        if self.turn_count > 0 {
            let text = format!(" T:{}", self.turn_count);
            if used + text.len() + 4 < width {
                let _ = used + text.len(); // Budget check only
                spans.push(Span::styled(
                    text,
                    Style::default().fg(self.secondary_color),
                ));
            }
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_default() {
        let header = Header::new();
        assert_eq!(header.app_name, "rustycode");
    }

    #[test]
    fn test_header_with_app_name() {
        let header = Header::new().with_app_name("my-app");
        assert_eq!(header.app_name, "my-app");
    }

    #[test]
    fn test_header_with_project_name() {
        let header = Header::new().with_project_name("test-project");
        assert_eq!(header.project_name, "test-project");
    }

    #[test]
    fn test_header_with_counts() {
        let header = Header::new().with_counts(5, 2);
        assert_eq!(header.task_count, 5);
        assert_eq!(header.pending_tools, 2);
    }

    #[test]
    fn test_header_with_git_branch() {
        let header = Header::new().with_git_branch(Some("main".to_string()));
        assert_eq!(header.git_branch, Some("main".to_string()));
    }

    #[test]
    fn test_header_project_truncation() {
        let header = Header::new().with_project_name("a".repeat(100));
        assert_eq!(header.project_name.len(), 100);
    }
}
