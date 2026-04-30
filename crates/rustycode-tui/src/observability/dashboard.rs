use crate::observability::metrics_display::{format_duration, format_tokens, progress_bar};
/// Dashboard widget for displaying system health, session progress, and token budget
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};
use rustycode_observability::SessionMetrics;

/// Dashboard widget for displaying real-time observability data
#[derive(Clone)]
pub struct DashboardWidget {
    session_id: String,
    current_metrics: Option<SessionMetrics>,
    elapsed_time: f64,
    token_budget_total: u64,
}

impl DashboardWidget {
    /// Create a new dashboard widget
    pub fn new(session_id: String) -> Self {
        DashboardWidget {
            session_id,
            current_metrics: None,
            elapsed_time: 0.0,
            token_budget_total: 2000, // Default budget
        }
    }

    /// Create a new dashboard widget with custom token budget
    pub fn with_budget(session_id: String, token_budget: u64) -> Self {
        DashboardWidget {
            session_id,
            current_metrics: None,
            elapsed_time: 0.0,
            token_budget_total: token_budget,
        }
    }

    /// Update metrics from a SessionMetrics instance
    pub fn update_metrics(&mut self, metrics: &SessionMetrics) {
        self.current_metrics = Some(metrics.clone());
        self.elapsed_time = metrics.elapsed_secs();
    }

    /// Set the token budget
    pub fn set_token_budget(&mut self, budget: u64) {
        self.token_budget_total = budget;
    }

    /// Render the dashboard as a Paragraph
    pub fn render(&self) -> Paragraph<'_> {
        let content = self.format_dashboard();

        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title("Dashboard"))
            .style(Style::default().fg(Color::Gray))
    }

    /// Format the complete dashboard content
    fn format_dashboard(&self) -> Vec<Line<'_>> {
        let mut lines = Vec::new();

        // Session info line
        lines.push(Line::from(vec![
            Span::styled("Session: ", Style::default().fg(Color::Cyan)),
            Span::raw(self.session_id.clone()),
        ]));

        // Elapsed time line
        let elapsed_str = format_duration(self.elapsed_time);
        lines.push(Line::from(vec![
            Span::styled("Elapsed: ", Style::default().fg(Color::Cyan)),
            Span::raw(elapsed_str),
        ]));

        // Metrics section
        if let Some(ref metrics) = self.current_metrics {
            let tokens_used = metrics.total_tokens.value();
            let tokens_remaining = self.token_budget_total.saturating_sub(tokens_used);

            // Token budget line
            let token_str = format_tokens(tokens_used, tokens_remaining);
            lines.push(Line::from(vec![
                Span::styled("Tokens: ", Style::default().fg(Color::Cyan)),
                Span::raw(token_str),
            ]));

            // Progress bar for tokens
            let percent = if self.token_budget_total > 0 {
                (tokens_used as f64 / self.token_budget_total as f64) * 100.0
            } else {
                0.0
            };

            let progress = progress_bar(percent, 20);
            let progress_style = if percent > 80.0 {
                Style::default().fg(Color::Red)
            } else if percent > 50.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };

            lines.push(Line::from(vec![Span::styled(progress, progress_style)]));

            // Task metrics
            let total_tasks = metrics.total_tasks.value();
            let completed_tasks = metrics.completed_tasks.value();
            let active_tasks = metrics.active_tasks.value() as u64;

            lines.push(Line::from(vec![
                Span::styled("Tasks: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!(
                    "{} total, {} completed, {} active",
                    total_tasks, completed_tasks, active_tasks
                )),
            ]));

            // Error count
            let errors = metrics.total_errors.value();
            if errors > 0 {
                lines.push(Line::from(vec![
                    Span::styled("Errors: ", Style::default().fg(Color::Red)),
                    Span::raw(errors.to_string()),
                ]));
            }
        }

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_creation() {
        let dashboard = DashboardWidget::new("session-123".to_string());
        assert_eq!(dashboard.session_id, "session-123");
        assert_eq!(dashboard.token_budget_total, 2000);
        assert!(dashboard.current_metrics.is_none());
    }

    #[test]
    fn test_dashboard_with_budget() {
        let dashboard = DashboardWidget::with_budget("session-456".to_string(), 5000);
        assert_eq!(dashboard.session_id, "session-456");
        assert_eq!(dashboard.token_budget_total, 5000);
    }

    #[test]
    fn test_dashboard_set_budget() {
        let mut dashboard = DashboardWidget::new("session-789".to_string());
        dashboard.set_token_budget(3000);
        assert_eq!(dashboard.token_budget_total, 3000);
    }

    #[test]
    fn test_dashboard_update_metrics() {
        let mut dashboard = DashboardWidget::new("session-123".to_string());
        let metrics = SessionMetrics::new();

        // Record some data
        metrics.record_task(100, 0.5);
        metrics.record_completion();

        dashboard.update_metrics(&metrics);
        assert!(dashboard.current_metrics.is_some());
        assert!(dashboard.elapsed_time >= 0.0);
    }

    #[test]
    fn test_dashboard_format_without_metrics() {
        let dashboard = DashboardWidget::new("session-test".to_string());
        let lines = dashboard.format_dashboard();

        // Should have at least session and elapsed time lines
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_dashboard_format_with_metrics() {
        let mut dashboard = DashboardWidget::new("session-test".to_string());
        let metrics = SessionMetrics::new();

        metrics.record_task(500, 1.0);
        metrics.record_completion();
        metrics.set_active_tasks(2);

        dashboard.update_metrics(&metrics);
        let lines = dashboard.format_dashboard();

        // Should have multiple lines: session, elapsed, tokens, progress, tasks, etc.
        assert!(lines.len() > 3);
    }

    #[test]
    fn test_dashboard_render_no_panic() {
        let dashboard = DashboardWidget::new("session-test".to_string());
        // Should not panic on render
        let _ = dashboard.render();
    }

    #[test]
    fn test_dashboard_render_with_metrics_no_panic() {
        let mut dashboard = DashboardWidget::new("session-test".to_string());
        let metrics = SessionMetrics::new();

        metrics.record_task(100, 0.5);
        metrics.record_error();
        dashboard.update_metrics(&metrics);

        // Should not panic on render
        let _ = dashboard.render();
    }

    #[test]
    fn test_dashboard_clone() {
        let dashboard1 = DashboardWidget::new("session-clone".to_string());
        let dashboard2 = dashboard1.clone();

        assert_eq!(dashboard1.session_id, dashboard2.session_id);
        assert_eq!(dashboard1.token_budget_total, dashboard2.token_budget_total);
    }

    #[test]
    fn test_dashboard_zero_token_budget() {
        let mut dashboard = DashboardWidget::with_budget("session-zero".to_string(), 0);
        let metrics = SessionMetrics::new();

        metrics.record_task(10, 0.1);
        dashboard.update_metrics(&metrics);

        // Should handle zero budget gracefully
        let lines = dashboard.format_dashboard();
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_dashboard_high_token_usage() {
        let mut dashboard = DashboardWidget::with_budget("session-high".to_string(), 1000);
        let metrics = SessionMetrics::new();

        // Use 95% of budget
        metrics.record_task(950, 1.0);
        dashboard.update_metrics(&metrics);

        let lines = dashboard.format_dashboard();
        // Should still render without panic
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_dashboard_over_token_budget() {
        let mut dashboard = DashboardWidget::with_budget("session-over".to_string(), 1000);
        let metrics = SessionMetrics::new();

        // Use more than budget
        metrics.record_task(1500, 1.0);
        dashboard.update_metrics(&metrics);

        let lines = dashboard.format_dashboard();
        // Should handle over-budget gracefully
        assert!(!lines.is_empty());
    }
}
