//! Polished Footer Component
//!
//! Single-row footer displaying session info, task summary, and model info.
//! Following the redesigned TUI spec for clean, reference-style information display.

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Footer component displaying session metadata
pub struct Footer {
    /// Session duration (e.g., "2h 34m")
    pub session_duration: String,
    /// Task summary (e.g., "✓5 ☐3")
    pub task_summary: String,
    /// Current model name (e.g., "sonnet-4.5")
    pub model_name: String,
    /// Session cost in USD (0.0 = not tracked or free)
    pub session_cost: f64,
    /// Budget limit in USD (None = no limit set)
    pub budget_limit: Option<f64>,
    /// Primary color for accents
    pub primary_color: Color,
    /// Muted color for text
    pub muted_color: Color,
}

impl Default for Footer {
    fn default() -> Self {
        Self {
            session_duration: String::from("0m"),
            task_summary: String::new(),
            model_name: String::from("sonnet-4.5"),
            session_cost: 0.0,
            budget_limit: None,
            primary_color: Color::Rgb(91, 141, 239),
            muted_color: Color::Rgb(107, 114, 128),
        }
    }
}

impl Footer {
    /// Create a new footer with default styling
    pub fn new() -> Self {
        Self::default()
    }

    /// Set session duration
    pub fn with_session_duration(mut self, duration: impl Into<String>) -> Self {
        self.session_duration = duration.into();
        self
    }

    /// Set task summary
    pub fn with_task_summary(mut self, summary: impl Into<String>) -> Self {
        self.task_summary = summary.into();
        self
    }

    /// Set model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_name = model.into();
        self
    }

    /// Set session cost
    pub fn with_session_cost(mut self, cost: f64) -> Self {
        self.session_cost = cost;
        self
    }

    /// Set budget limit
    pub fn with_budget_limit(mut self, limit: Option<f64>) -> Self {
        self.budget_limit = limit;
        self
    }

    /// Format cost for display
    fn format_cost(cost: f64) -> String {
        if cost < 0.001 {
            String::new()
        } else if cost < 0.01 {
            format!("${:.4}", cost)
        } else if cost < 1.0 {
            format!("${:.3}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    /// Format duration from seconds into human-readable string
    pub fn format_duration(secs: u64) -> String {
        let mins = secs / 60;
        let hours = mins / 60;
        let mins = mins % 60;
        let secs = secs % 60;

        if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        }
    }

    /// Render the footer. Width-aware: sections are skipped if they don't fit.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = area.width as usize;
        let mut spans: Vec<Span> = Vec::new();
        let mut used: usize = 0;

        // Section 1: Session duration (always shown, compact)
        let session = format!(" {} ", self.session_duration);
        used += session.len();
        spans.push(Span::styled(session, Style::default().fg(Color::White)));

        // Divider + tasks (only if room)
        let task_part = if self.task_summary.is_empty() {
            String::new()
        } else {
            format!(" {} ", self.task_summary)
        };
        let task_section = format!(" │ {} ", task_part.trim());

        // Cost display (with budget indicator if set)
        let cost_str = Self::format_cost(self.session_cost);
        let cost_section = if cost_str.is_empty() {
            String::new()
        } else if let Some(limit) = self.budget_limit {
            let pct = if limit > 0.0 {
                ((self.session_cost / limit) * 100.0).round() as usize
            } else {
                0
            };
            format!(" │ {} ({}% of ${:.0}) ", cost_str, pct.min(999), limit)
        } else {
            format!(" │ {} ", cost_str)
        };

        if used + task_section.len() + cost_section.len() + self.model_name.len() + 12 < width {
            used += task_section.len();
            spans.push(Span::styled(
                task_section,
                Style::default().fg(self.muted_color),
            ));
            used += cost_section.len();
            spans.push(Span::styled(
                cost_section,
                Style::default().fg(Color::Yellow),
            ));
        } else if used + task_section.len() + self.model_name.len() + 12 < width {
            used += task_section.len();
            spans.push(Span::styled(
                task_section,
                Style::default().fg(self.muted_color),
            ));
        }

        // Model (right-aligned if room)
        let model_part = format!(" {} ", self.model_name);
        if used + model_part.len() + 4 < width {
            // Fill gap
            let gap = width.saturating_sub(used + model_part.len() + 2);
            if gap > 0 {
                spans.push(Span::styled(
                    " ".repeat(gap),
                    Style::default().fg(self.muted_color),
                ));
            }
            spans.push(Span::styled(
                model_part,
                Style::default().fg(Color::DarkGray),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footer_default() {
        let footer = Footer::new();
        assert_eq!(footer.session_duration, "0m");
        assert_eq!(footer.model_name, "sonnet-4.5");
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(Footer::format_duration(0), "0s");
        assert_eq!(Footer::format_duration(45), "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(Footer::format_duration(90), "1m 30s");
        assert_eq!(Footer::format_duration(125), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(Footer::format_duration(3661), "1h 1m");
        assert_eq!(Footer::format_duration(9000), "2h 30m");
    }

    #[test]
    fn test_footer_with_session_duration() {
        let footer = Footer::new().with_session_duration("1h 23m");
        assert_eq!(footer.session_duration, "1h 23m");
    }

    #[test]
    fn test_footer_with_task_summary() {
        let footer = Footer::new().with_task_summary("✓5 ☐3");
        assert_eq!(footer.task_summary, "✓5 ☐3");
    }

    #[test]
    fn test_footer_with_model() {
        let footer = Footer::new().with_model("opus-4.5");
        assert_eq!(footer.model_name, "opus-4.5");
    }
}
