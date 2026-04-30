// Observability module for TUI dashboard and metrics display
pub mod dashboard;
pub mod metrics_display;

pub use dashboard::DashboardWidget;
pub use metrics_display::{format_duration, format_task_rate, format_tokens, progress_bar};
