/// Example usage of the observability dashboard widget
///
/// This example demonstrates how to integrate the DashboardWidget with SessionMetrics:
///
/// ```ignore
/// use rustycode_tui::observability::DashboardWidget;
/// use rustycode_observability::SessionMetrics;
/// use ratatui::prelude::*;
///
/// // Create a dashboard widget for a session
/// let mut dashboard = DashboardWidget::new("session-abc-123".to_string());
/// dashboard.set_token_budget(5000);
///
/// // Create and populate metrics
/// let metrics = SessionMetrics::new();
/// metrics.record_task(250, 0.5);  // 250 tokens, 0.5 seconds
/// metrics.record_task(300, 0.75); // 300 tokens, 0.75 seconds
/// metrics.record_completion();
/// metrics.set_active_tasks(3);
///
/// // Update dashboard with metrics
/// dashboard.update_metrics(&metrics);
///
/// // Render the dashboard
/// let paragraph = dashboard.render();
///
/// // The paragraph can be rendered to a Rect within your Ratatui app
/// // frame.render_widget(paragraph, area);
/// ```
///
/// The dashboard displays:
/// - Session ID
/// - Elapsed time since session start (formatted as "1h 23m 45s")
/// - Token budget usage ("550 / 5.0K tokens used (11%)")
/// - Progress bar with color coding:
///   - Green: < 50%
///   - Yellow: 50-80%
///   - Red: > 80%
/// - Task metrics (total, completed, active)
/// - Error count (if any errors occurred)

// This file exists for documentation purposes
