//! AST pipeline phase progress widget
//!
//! Tracks the 6-phase AST pipeline (Classify → Research → Skeleton → Expand → Execute → Verify)
//! and provides rendering helpers for the status bar.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// The 6 canonical AST pipeline phases.
pub const AST_PHASE_NAMES: [&str; 6] = [
    "Classify", "Research", "Skeleton", "Expand", "Execute", "Verify",
];

/// State for tracking AST pipeline progress within the TUI.
#[derive(Clone, Debug, Default)]
pub struct AstPhaseState {
    pub active: bool,
    pub phase: String,
    pub phase_index: usize,
    pub total_phases: usize,
    pub task_summary: String,
    pub phase_elapsed_ms: u64,
    pub total_elapsed_ms: u64,
    pub milestones_completed: usize,
    pub milestones_total: usize,
    pub success: bool,
}

impl AstPhaseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Activate tracking for a specific AST phase.
    pub fn activate(&mut self, phase: &str, phase_index: usize, task_summary: &str) {
        self.active = true;
        self.phase = phase.to_string();
        self.phase_index = phase_index;
        self.total_phases = AST_PHASE_NAMES.len();
        self.task_summary = task_summary.to_string();
        self.phase_elapsed_ms = 0;
        self.success = false;
    }

    /// Update milestone progress within the current phase.
    pub fn update_milestones(&mut self, completed: usize, total: usize) {
        self.milestones_completed = completed;
        self.milestones_total = total;
    }

    /// Update elapsed time.
    pub fn update_elapsed(&mut self, total_elapsed_ms: u64) {
        self.total_elapsed_ms = total_elapsed_ms;
    }

    /// Mark the pipeline as completed successfully.
    pub fn complete(&mut self) {
        self.success = true;
    }

    /// Deactivate tracking — pipeline finished or was cancelled.
    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.total_phases == 0 {
            return 0.0;
        }
        let phase_base = self.phase_index as f64 / self.total_phases as f64;
        let milestone_increment = if self.milestones_total > 0 {
            let milestone_frac = self.milestones_completed as f64 / self.milestones_total as f64;
            milestone_frac / self.total_phases as f64
        } else {
            0.0
        };
        (phase_base + milestone_increment).min(1.0)
    }

    pub fn progress_bar(&self, width: usize) -> String {
        if width == 0 {
            return String::new();
        }
        let fraction = self.progress_fraction();
        let filled = (fraction * width as f64).round() as usize;
        let filled = filled.min(width);
        let empty = width - filled;
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }

    pub fn phase_dot_indicator(&self) -> String {
        let total = self.total_phases.max(1);
        let dots: Vec<char> = (0..total)
            .map(|i| match i.cmp(&self.phase_index) {
                std::cmp::Ordering::Less => '✓',
                std::cmp::Ordering::Equal => '●',
                std::cmp::Ordering::Greater => '○',
            })
            .collect();
        let mut s = String::new();
        for (i, &dot) in dots.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push(dot);
        }
        s
    }

    pub fn status_color(&self) -> Color {
        if self.success {
            Color::Green
        } else {
            match self.phase_index {
                0 => Color::Cyan,
                1 => Color::Blue,
                2 => Color::Magenta,
                3 => Color::Yellow,
                4 => Color::Rgb(255, 165, 0),
                5 => Color::Green,
                _ => Color::Cyan,
            }
        }
    }
}

/// Widget for rendering AST phase progress in an overlay or panel.
pub struct AstPhaseWidget<'a> {
    pub state: &'a AstPhaseState,
    pub anim_frame: usize,
}

impl<'a> AstPhaseWidget<'a> {
    pub fn new(state: &'a AstPhaseState, anim_frame: usize) -> Self {
        Self { state, anim_frame }
    }

    pub fn render(self, frame: &mut Frame, area: Rect) {
        if !self.state.active || area.width == 0 || area.height == 0 {
            return;
        }

        let color = self.state.status_color();
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame_idx = (self.anim_frame / 5) % frames.len();

        let mut lines = Vec::with_capacity(5);

        let header = Line::from(vec![
            Span::styled(
                format!("{} AST Pipeline ", frames[frame_idx]),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(self.state.phase_dot_indicator(), Style::default().fg(color)),
        ]);
        lines.push(header);

        let phase_label = format!(
            "Phase {}/{}: {}",
            self.state.phase_index + 1,
            self.state.total_phases,
            self.state.phase,
        );
        lines.push(Line::from(Span::styled(
            phase_label,
            Style::default().fg(Color::White),
        )));

        if !self.state.task_summary.is_empty() {
            let max_len = area.width.saturating_sub(3) as usize;
            let display_summary = if self.state.task_summary.len() > max_len && max_len > 0 {
                let end = self.state.task_summary.floor_char_boundary(max_len);
                format!("{}...", &self.state.task_summary[..end])
            } else {
                self.state.task_summary.clone()
            };
            lines.push(Line::from(Span::styled(
                display_summary,
                Style::default().fg(Color::DarkGray),
            )));
        }

        let bar_width = (area.width as usize)
            .saturating_sub(4)
            .min(area.width as usize / 3);
        if bar_width > 0 {
            let bar = self.state.progress_bar(bar_width);
            let pct = (self.state.progress_fraction() * 100.0).round() as usize;
            lines.push(Line::from(vec![
                Span::styled(bar, Style::default().fg(color)),
                Span::styled(format!(" {}%", pct), Style::default().fg(Color::White)),
            ]));
        }

        if self.state.milestones_total > 0 {
            lines.push(Line::from(Span::styled(
                format!(
                    "Milestones: {}/{}",
                    self.state.milestones_completed, self.state.milestones_total,
                ),
                Style::default().fg(Color::DarkGray),
            )));
        }

        if self.state.total_elapsed_ms > 0 {
            lines.push(Line::from(Span::styled(
                format_elapsed(self.state.total_elapsed_ms),
                Style::default().fg(Color::DarkGray),
            )));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }
}

fn format_elapsed(ms: u64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m {}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state_is_inactive() {
        let state = AstPhaseState::new();
        assert!(!state.is_active());
        assert!(!state.active);
    }

    #[test]
    fn test_progress_fraction_first_phase() {
        let state = AstPhaseState {
            active: true,
            phase: "Classify".into(),
            phase_index: 0,
            total_phases: 6,
            ..Default::default()
        };
        assert_eq!(state.progress_fraction(), 0.0);
    }

    #[test]
    fn test_progress_fraction_with_milestones() {
        let state = AstPhaseState {
            active: true,
            phase: "Classify".into(),
            phase_index: 0,
            total_phases: 6,
            milestones_completed: 5,
            milestones_total: 10,
            ..Default::default()
        };
        let frac = state.progress_fraction();
        assert!(frac > 0.0);
        assert!(frac < 1.0);
    }

    #[test]
    fn test_progress_bar_filled() {
        let state = AstPhaseState {
            active: true,
            phase: "Verify".into(),
            phase_index: 5,
            total_phases: 6,
            milestones_completed: 10,
            milestones_total: 10,
            ..Default::default()
        };
        let bar = state.progress_bar(10);
        assert_eq!(bar, "██████████");
    }

    #[test]
    fn test_progress_bar_empty() {
        let state = AstPhaseState {
            active: true,
            phase: "Classify".into(),
            phase_index: 0,
            total_phases: 6,
            ..Default::default()
        };
        let bar = state.progress_bar(10);
        assert_eq!(bar, "░░░░░░░░░░");
    }

    #[test]
    fn test_phase_dot_indicator() {
        let state = AstPhaseState {
            active: true,
            phase: "Expand".into(),
            phase_index: 3,
            total_phases: 6,
            ..Default::default()
        };
        let dots = state.phase_dot_indicator();
        assert!(dots.contains('✓'));
        assert!(dots.contains('●'));
        assert!(dots.contains('○'));
    }

    #[test]
    fn test_format_elapsed_seconds() {
        assert_eq!(format_elapsed(5000), "5s");
        assert_eq!(format_elapsed(90000), "1m 30s");
    }

    #[test]
    fn test_status_color_varies_by_phase() {
        let c0 = AstPhaseState {
            phase_index: 0,
            ..Default::default()
        }
        .status_color();
        let c5 = AstPhaseState {
            phase_index: 5,
            ..Default::default()
        }
        .status_color();
        assert_ne!(c0, c5);
    }

    #[test]
    fn test_activate_sets_active_and_phase() {
        let mut state = AstPhaseState::new();
        assert!(!state.is_active());

        state.activate("Research", 1, "Fix the bug in parser");
        assert!(state.is_active());
        assert_eq!(state.phase, "Research");
        assert_eq!(state.phase_index, 1);
        assert_eq!(state.total_phases, AST_PHASE_NAMES.len());
        assert_eq!(state.task_summary, "Fix the bug in parser");
    }

    #[test]
    fn test_update_milestones() {
        let mut state = AstPhaseState::new();
        state.activate("Execute", 4, "task");
        state.update_milestones(3, 7);
        assert_eq!(state.milestones_completed, 3);
        assert_eq!(state.milestones_total, 7);
    }

    #[test]
    fn test_complete_sets_success() {
        let mut state = AstPhaseState::new();
        state.activate("Verify", 5, "task");
        assert!(!state.success);
        state.complete();
        assert!(state.success);
        assert!(state.is_active()); // still active until deactivate
    }

    #[test]
    fn test_deactivate_resets_to_default() {
        let mut state = AstPhaseState::new();
        state.activate("Expand", 3, "complex task");
        state.update_milestones(5, 10);
        state.update_elapsed(30000);
        assert!(state.is_active());

        state.deactivate();
        assert!(!state.is_active());
        assert!(!state.active);
        assert_eq!(state.phase, "");
        assert_eq!(state.phase_index, 0);
        assert_eq!(state.milestones_completed, 0);
        assert_eq!(state.milestones_total, 0);
        assert_eq!(state.total_elapsed_ms, 0);
    }

    #[test]
    fn test_progress_fraction_after_activate_and_milestones() {
        let mut state = AstPhaseState::new();
        state.activate("Execute", 4, "task");
        state.update_milestones(5, 10);
        let frac = state.progress_fraction();
        // Phase 4/6 base = 4/6 ≈ 0.667, plus milestone increment
        assert!(frac > 0.6);
        assert!(frac < 1.0);
    }
}
