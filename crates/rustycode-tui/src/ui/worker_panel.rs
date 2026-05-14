//! Worker status panel for the TUI.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use rustycode_orchestration::worker_registry::{Worker, WorkerEvent, WorkerStatus};

/// Display state for a single tool call.
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub tool_name: String,
    pub target: String,
}

/// Display state for a single worker.
#[derive(Debug, Clone)]
struct WorkerDisplay {
    worker_id: String,
    status: WorkerStatus,
    task: String,
    recent_tools: Vec<ToolCallDisplay>,
}

/// Worker status panel for the TUI.
///
/// Receives worker updates and renders a live worker dashboard.
#[derive(Clone)]
pub struct WorkerPanel {
    pub visible: bool,
    /// Worker display states.
    workers: Vec<WorkerDisplay>,
}

impl WorkerPanel {
    pub fn new() -> Self {
        Self {
            visible: false,
            workers: Vec::new(),
        }
    }

    /// Toggle panel visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Update panel from worker registry snapshot.
    pub fn update_from_workers(&mut self, workers: &[Worker]) {
        self.workers = workers
            .iter()
            .map(|w| {
                let recent_tools: Vec<ToolCallDisplay> = w
                    .events
                    .iter()
                    .rev()
                    .filter_map(|e| match e {
                        WorkerEvent::ToolCalled {
                            tool_name, target, ..
                        } => Some(ToolCallDisplay {
                            tool_name: tool_name.clone(),
                            target: target.clone(),
                        }),
                        _ => None,
                    })
                    .take(5)
                    .collect();

                WorkerDisplay {
                    worker_id: w.worker_id.clone(),
                    status: w.status,
                    task: w
                        .task_description
                        .clone()
                        .unwrap_or_else(|| "No task".to_string()),
                    recent_tools,
                }
            })
            .collect();
    }

    /// Get count of workers by status.
    pub fn count_by_status(&self, status: WorkerStatus) -> usize {
        self.workers.iter().filter(|w| w.status == status).count()
    }

    /// Get total worker count.
    pub fn total_workers(&self) -> usize {
        self.workers.len()
    }

    /// Build the panel content lines.
    pub fn build_content(&self) -> Vec<Line<'_>> {
        let mut lines = Vec::new();

        // Header with stats
        let total = self.total_workers();
        let running = self.count_by_status(WorkerStatus::Running);
        let spawning = self.count_by_status(WorkerStatus::Spawning)
            + self.count_by_status(WorkerStatus::ReadyForPrompt);
        let finished = self.count_by_status(WorkerStatus::Finished);
        let failed = self.count_by_status(WorkerStatus::Failed);

        lines.push(Line::from(vec![
            Span::styled(
                "Workers",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("Total: {}", total),
                Style::default().fg(Color::White),
            ),
        ]));

        // Status summary
        let mut status_spans = Vec::new();
        if spawning > 0 {
            status_spans.push(Span::styled(
                format!("◌ {}", spawning),
                Style::default().fg(Color::Yellow),
            ));
            status_spans.push(Span::styled("  ", Style::default()));
        }
        if running > 0 {
            status_spans.push(Span::styled(
                format!("⟳ {}", running),
                Style::default().fg(Color::Green),
            ));
            status_spans.push(Span::styled("  ", Style::default()));
        }
        if finished > 0 {
            status_spans.push(Span::styled(
                format!("✓ {}", finished),
                Style::default().fg(Color::Cyan),
            ));
            status_spans.push(Span::styled("  ", Style::default()));
        }
        if failed > 0 {
            status_spans.push(Span::styled(
                format!("✗ {}", failed),
                Style::default().fg(Color::Red),
            ));
        }

        if !status_spans.is_empty() {
            lines.push(Line::from(status_spans));
        }

        // Separator
        lines.push(Line::from(Span::styled(
            "─".repeat(40),
            Style::default().fg(Color::DarkGray),
        )));

        // Worker list
        if self.workers.is_empty() {
            lines.push(Line::from(Span::styled(
                "No workers spawned yet",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            // Sort: Running/Spawning first, then Finished, then Failed
            let mut sorted_workers: Vec<&WorkerDisplay> = self.workers.iter().collect();
            sorted_workers.sort_by(|a, b| {
                let status_order = |s: &WorkerStatus| match s {
                    WorkerStatus::Spawning | WorkerStatus::ReadyForPrompt => 0,
                    WorkerStatus::Running => 1,
                    WorkerStatus::Finished => 2,
                    WorkerStatus::Failed => 3,
                    #[allow(unreachable_patterns)]
                    _ => 4,
                };
                status_order(&a.status).cmp(&status_order(&b.status))
            });

            for worker in sorted_workers {
                let icon = match worker.status {
                    WorkerStatus::Spawning | WorkerStatus::ReadyForPrompt => "◌",
                    WorkerStatus::Running => "⟳",
                    WorkerStatus::Finished => "✓",
                    WorkerStatus::Failed => "✗",
                    #[allow(unreachable_patterns)]
                    _ => "?",
                };

                let status_color = match worker.status {
                    WorkerStatus::Spawning | WorkerStatus::ReadyForPrompt => Color::Yellow,
                    WorkerStatus::Running => Color::Green,
                    WorkerStatus::Finished => Color::Cyan,
                    WorkerStatus::Failed => Color::Red,
                    #[allow(unreachable_patterns)]
                    _ => Color::Gray,
                };

                let task_width = unicode_width::UnicodeWidthStr::width(worker.task.as_str());
                let task_display = if task_width > 30 {
                    crate::unicode::truncate_display(&worker.task, 30)
                } else {
                    worker.task.clone()
                };

                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(status_color)),
                    Span::styled(
                        format!("{:<12} ", worker.worker_id),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!("{:<8} ", status_str(worker.status)),
                        Style::default().fg(status_color),
                    ),
                    Span::styled(task_display, Style::default().fg(Color::DarkGray)),
                ]));

                // Show recent tool calls under each worker
                let tool_count = worker.recent_tools.len();
                for (i, tc) in worker.recent_tools.iter().enumerate() {
                    let is_last = i == tool_count - 1;
                    let prefix = if is_last { "  └─ " } else { "  ├─ " };
                    let tool_target = truncate_str(&tc.target, 24);
                    lines.push(Line::from(vec![
                        Span::styled(prefix, Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("{} ", tc.tool_name),
                            Style::default().fg(Color::Rgb(140, 160, 180)),
                        ),
                        Span::styled(tool_target, Style::default().fg(Color::DarkGray)),
                    ]));
                }
            }
        }

        lines
    }

    /// Render the panel to a buffer.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let _content = self.build_content();

        // Brutalist-style rendering: delegate to the Widget impl
        let panel = self.clone();
        Widget::render(panel, area, buf);
    }
}

impl Default for WorkerPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for WorkerPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 5 {
            return;
        }

        let content = self.build_content();

        // Brutalist-style rendering: heavy left border, no surrounding box
        let mut brutalist_content = Vec::new();

        // Top border with title
        let title = " Workers ";
        let side_space = (area.width as usize).saturating_sub(title.len() + 2);
        let left_pad = side_space / 2;
        let right_pad = side_space - left_pad;
        let top_border = format!(
            "╺{}{}{}╸",
            "━".repeat(left_pad),
            title,
            "━".repeat(right_pad),
        );
        brutalist_content.push(Line::from(Span::styled(
            top_border,
            Style::default().fg(Color::Rgb(100, 180, 255)),
        )));

        // Wrap each content line with brutalist left border
        for line in &content {
            let mut spans = vec![Span::styled(
                "▐ ",
                Style::default().fg(Color::Rgb(100, 180, 255)),
            )];
            spans.extend(line.spans.iter().cloned());
            brutalist_content.push(Line::from(spans));
        }

        // Bottom border
        let bottom_border = format!("╺{}╸", "━".repeat((area.width as usize).saturating_sub(2)));
        brutalist_content.push(Line::from(Span::styled(
            bottom_border,
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(brutalist_content)
            .style(Style::default().fg(Color::Gray).bg(Color::Rgb(20, 20, 30)));

        paragraph.render(area, buf);
    }
}

fn status_str(status: WorkerStatus) -> &'static str {
    match status {
        WorkerStatus::Spawning | WorkerStatus::ReadyForPrompt => "Spawning",
        WorkerStatus::Running => "Running",
        WorkerStatus::Finished => "Done",
        WorkerStatus::Failed => "Failed",
        #[allow(unreachable_patterns)]
        _ => "Unknown",
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    let display_width = unicode_width::UnicodeWidthStr::width(s);
    if display_width <= max_len {
        return s.to_string();
    }
    let target_width = max_len.saturating_sub(3);
    let mut acc_width = 0;
    let mut truncate_at = 0;
    for (i, c) in s.char_indices() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if acc_width + cw > target_width {
            break;
        }
        acc_width += cw;
        truncate_at = i + c.len_utf8();
    }
    format!("{}...", &s[..truncate_at])
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_orchestration::worker_registry::Worker;

    #[test]
    fn worker_panel_starts_hidden() {
        let panel = WorkerPanel::new();
        assert!(!panel.visible);
        assert_eq!(panel.total_workers(), 0);
    }

    #[test]
    fn toggle_visibility() {
        let mut panel = WorkerPanel::new();
        panel.toggle();
        assert!(panel.visible);
        panel.toggle();
        assert!(!panel.visible);
    }

    #[test]
    fn update_from_workers() {
        let mut panel = WorkerPanel::new();
        let workers = vec![
            Worker {
                worker_id: "wkr_001".to_string(),
                status: WorkerStatus::Running,
                cwd: "/tmp".to_string(),
                task_id: Some("t1".to_string()),
                task_description: Some("Fix auth".to_string()),
                trust_gate_cleared: false,
                last_error: None,
                result_summary: None,
                created_at: 1000,
                updated_at: 1000,
                events: vec![
                    WorkerEvent::ToolCalled {
                        tool_name: "Read".to_string(),
                        target: "bash.rs".to_string(),
                        timestamp: 1001,
                    },
                    WorkerEvent::ToolCalled {
                        tool_name: "Grep".to_string(),
                        target: "panic".to_string(),
                        timestamp: 1002,
                    },
                ],
            },
            Worker {
                worker_id: "wkr_002".to_string(),
                status: WorkerStatus::Finished,
                cwd: "/tmp".to_string(),
                task_id: Some("t2".to_string()),
                task_description: Some("Add API".to_string()),
                trust_gate_cleared: false,
                last_error: None,
                result_summary: Some("Done".to_string()),
                created_at: 1000,
                updated_at: 1000,
                events: vec![],
            },
        ];

        panel.update_from_workers(&workers);
        assert_eq!(panel.total_workers(), 2);
        assert_eq!(panel.count_by_status(WorkerStatus::Running), 1);
        assert_eq!(panel.count_by_status(WorkerStatus::Finished), 1);
        // First worker should have 2 tool calls extracted
        assert_eq!(panel.workers[0].recent_tools.len(), 2);
        assert_eq!(panel.workers[0].recent_tools[0].tool_name, "Grep");
        // Second worker has no tool calls
        assert!(panel.workers[1].recent_tools.is_empty());
    }

    #[test]
    fn build_content_empty() {
        let panel = WorkerPanel::new();
        let content = panel.build_content();
        // Should have at least a header and "no workers" message
        assert!(!content.is_empty());
    }

    #[test]
    fn build_content_with_workers() {
        let mut panel = WorkerPanel::new();
        let workers = vec![Worker {
            worker_id: "wkr_001".to_string(),
            status: WorkerStatus::Running,
            cwd: "/tmp".to_string(),
            task_id: Some("t1".to_string()),
            task_description: Some("Test task".to_string()),
            trust_gate_cleared: false,
            last_error: None,
            result_summary: None,
            created_at: 1000,
            updated_at: 1000,
            events: vec![WorkerEvent::ToolCalled {
                tool_name: "Read".to_string(),
                target: "src/main.rs".to_string(),
                timestamp: 1001,
            }],
        }];

        panel.update_from_workers(&workers);
        let content = panel.build_content();
        // Should have header, status summary, separator, worker line, and tool call
        assert!(content.len() >= 5);
    }
}
