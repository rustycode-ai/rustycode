//! Team agent timeline panel for the TUI.

use std::time::Instant;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use rustycode_core::team::orchestrator::TeamEvent;

/// Display state for a single agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentState {
    Waiting,
    Active,
    Complete,
    Failed,
}

/// Tracks display state for one agent.
#[derive(Debug, Clone)]
struct AgentDisplay {
    state: AgentState,
    detail: String,
    activation_count: u32,
    /// When the agent was last activated (for elapsed time display).
    activated_at: Option<Instant>,
}

/// Team timeline panel for the TUI.
///
/// Receives `TeamEvent` broadcasts and renders a live agent dashboard.
#[derive(Clone)]
#[non_exhaustive]
pub struct TeamPanel {
    pub visible: bool,
    /// Agent display states keyed by role name.
    agents: Vec<(&'static str, AgentDisplay)>,
    /// Current turn number.
    current_turn: u32,
    /// Maximum turns allowed.
    max_turns: u32,
    current_step: u32,
    /// Builder trust score (0.0–1.0).
    trust: f64,
    /// Task description.
    task: String,
    complete: bool,
    success: bool,
    /// Files modified during execution.
    files_modified: Vec<String>,
}

const ROLES: &[&str] = &["Architect", "Builder", "Skeptic", "Judge", "Scalpel"];

impl TeamPanel {
    pub fn new() -> Self {
        let agents = ROLES
            .iter()
            .map(|&role| {
                (
                    role,
                    AgentDisplay {
                        state: AgentState::Waiting,
                        detail: String::new(),
                        activation_count: 0,
                        activated_at: None,
                    },
                )
            })
            .collect();

        Self {
            visible: false,
            agents,
            current_turn: 0,
            max_turns: 50,
            current_step: 0,
            trust: 0.7,
            task: String::new(),
            complete: false,
            success: false,
            files_modified: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn set_task(&mut self, task: &str) {
        self.task = task.to_string();
    }

    /// Set max turns.
    pub fn set_max_turns(&mut self, max: u32) {
        self.max_turns = max;
    }

    /// Process a team event and update internal state.
    pub fn handle_event(&mut self, event: &TeamEvent) {
        match event {
            TeamEvent::AgentActivated { role, turn, reason } => {
                if let Some(agent) = self.agents.iter_mut().find(|(r, _)| *r == role.as_str()) {
                    agent.1.state = AgentState::Active;
                    agent.1.detail = reason.clone();
                    agent.1.activation_count += 1;
                    agent.1.activated_at = Some(Instant::now());
                }
                self.current_turn = *turn;
            }
            TeamEvent::AgentStateChanged { role, to_state, .. } => {
                if let Some(agent) = self.agents.iter_mut().find(|(r, _)| *r == role.as_str()) {
                    agent.1.detail = to_state.clone();
                }
            }
            TeamEvent::AgentDeactivated { role, reason, .. } => {
                if let Some(agent) = self.agents.iter_mut().find(|(r, _)| *r == role.as_str()) {
                    agent.1.state = if reason.contains("Parse failed") || reason.contains("failed")
                    {
                        AgentState::Failed
                    } else {
                        AgentState::Complete
                    };
                    agent.1.detail = reason.clone();
                }
            }
            TeamEvent::StepCompleted {
                step: _, success, ..
            } => {
                if *success {
                    self.current_step += 1;
                }
                // Reset completed agents to waiting for next step
                for (_, agent) in &mut self.agents {
                    if agent.state == AgentState::Complete {
                        agent.state = AgentState::Waiting;
                        agent.detail.clear();
                    }
                }
            }
            TeamEvent::TaskCompleted {
                success,
                turns,
                files_modified,
                final_trust,
            } => {
                self.complete = true;
                self.success = *success;
                self.current_turn = *turns;
                self.trust = *final_trust;
                self.files_modified = files_modified.clone();
                for (_, agent) in &mut self.agents {
                    if agent.state == AgentState::Active {
                        agent.state = AgentState::Complete;
                    }
                }
            }
            TeamEvent::Insight { role, message } => {
                if let Some(agent) = self.agents.iter_mut().find(|(r, _)| *r == role.as_str()) {
                    agent.1.detail = message.clone();
                }
            }
            TeamEvent::TrustChanged { new_value, .. } => {
                self.trust = *new_value;
            }
            TeamEvent::VerificationPassed { .. } => {
                // Verification passed — trust increase is handled by TrustChanged
            }
            // Phase 4: Event-driven orchestration events (no-op for panel display)
            TeamEvent::CodeChanged { .. }
            | TeamEvent::CompilationFailed { .. }
            | TeamEvent::TestsFailed { .. }
            | TeamEvent::PatternDiscovered { .. }
            | TeamEvent::SecurityIssueDetected { .. }
            | TeamEvent::StructuralDeclarationSet { .. }
            | TeamEvent::PlanAdapted { .. }
            | TeamEvent::SpecialistCreated { .. }
            | TeamEvent::ParallelExecutionRequested { .. } => {}
            #[allow(unreachable_patterns)]
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        for (_, agent) in &mut self.agents {
            agent.state = AgentState::Waiting;
            agent.detail.clear();
            agent.activation_count = 0;
            agent.activated_at = None;
        }
        self.current_turn = 0;
        self.current_step = 0;
        self.trust = 0.7;
        self.complete = false;
        self.success = false;
        self.files_modified.clear();
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn files_modified(&self) -> &[String] {
        &self.files_modified
    }

    pub fn agent_detail(&self, role: &str) -> Option<&str> {
        self.agents
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, a)| a.detail.as_str())
    }

    pub fn agent_state(&self, role: &str) -> Option<AgentState> {
        self.agents
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, a)| a.state)
    }

    pub fn build_content(&self) -> Vec<Line<'_>> {
        let mut lines = Vec::new();

        // Task title (truncated)
        let task_display = if crate::unicode::display_width(&self.task) > 28 {
            format!("{}...", crate::unicode::truncate_display(&self.task, 25))
        } else if self.task.is_empty() {
            "No task".to_string()
        } else {
            self.task.clone()
        };

        lines.push(Line::from(vec![
            Span::styled(
                "Team: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("\"{}\"", task_display),
                Style::default().fg(Color::White),
            ),
        ]));

        // Status bar
        let trust_color = if self.trust >= 0.7 {
            Color::Green
        } else if self.trust >= 0.4 {
            Color::Yellow
        } else {
            Color::Red
        };

        // Status info line
        lines.push(Line::from(vec![
            Span::styled(
                format!("Turn {}/{}", self.current_turn, self.max_turns),
                Style::default().fg(Color::White),
            ),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("Trust: {:.2}", self.trust),
                Style::default().fg(trust_color),
            ),
        ]));

        // Goose-inspired progress stats: total | waiting | active | complete | failed
        let waiting = self
            .agents
            .iter()
            .filter(|(_, a)| a.state == AgentState::Waiting)
            .count();
        let active = self
            .agents
            .iter()
            .filter(|(_, a)| a.state == AgentState::Active)
            .count();
        let complete = self
            .agents
            .iter()
            .filter(|(_, a)| a.state == AgentState::Complete)
            .count();
        let failed = self
            .agents
            .iter()
            .filter(|(_, a)| a.state == AgentState::Failed)
            .count();

        let mut stats_spans = vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{}", self.agents.len()),
                Style::default().fg(Color::White),
            ),
            Span::styled(" total", Style::default().fg(Color::DarkGray)),
        ];
        if active > 0 {
            stats_spans.push(Span::styled(
                format!(" │ ⟳ {}", active),
                Style::default().fg(Color::Green),
            ));
        }
        if complete > 0 {
            stats_spans.push(Span::styled(
                format!(" │ ✓ {}", complete),
                Style::default().fg(Color::Cyan),
            ));
        }
        if failed > 0 {
            stats_spans.push(Span::styled(
                format!(" │ ✗ {}", failed),
                Style::default().fg(Color::Red),
            ));
        }
        if waiting > 0 && !self.complete {
            stats_spans.push(Span::styled(
                format!(" │ ◌ {}", waiting),
                Style::default().fg(Color::DarkGray),
            ));
        }
        lines.push(Line::from(stats_spans));

        // Visual trust bar (10 segments) + step info
        let filled = (self.trust * 10.0) as usize;
        let empty = 10 - filled;
        let bar = format!("{}{}", "━".repeat(filled), "╌".repeat(empty));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(bar, Style::default().fg(trust_color)),
            Span::styled(
                format!("  Step: {}", self.current_step),
                Style::default().fg(Color::White),
            ),
        ]));

        // Separator
        lines.push(Line::from(Span::styled(
            "─".repeat(30),
            Style::default().fg(Color::DarkGray),
        )));

        // Agent rows
        for (role, agent) in &self.agents {
            let (icon, color) = match agent.state {
                AgentState::Waiting => ("◌", Color::DarkGray),
                AgentState::Active => ("⟳", Color::Green),
                AgentState::Complete => ("✓", Color::Cyan),
                AgentState::Failed => ("✗", Color::Red),
            };

            // Show activation count badge if agent has been activated more than once
            let badge = if agent.activation_count > 1 {
                format!("×{}", agent.activation_count)
            } else {
                String::new()
            };

            let detail_display = if agent.detail.is_empty() {
                match agent.state {
                    AgentState::Waiting => "Waiting",
                    AgentState::Active => "Active",
                    AgentState::Complete => "Complete",
                    AgentState::Failed => "Failed",
                }
            } else {
                &agent.detail
            };

            let detail_truncated = if crate::unicode::display_width(detail_display) > 20 {
                format!(
                    "{}...",
                    crate::unicode::truncate_display(detail_display, 17)
                )
            } else {
                detail_display.to_string()
            };

            // Show elapsed time for active agents
            let elapsed_str = if agent.state == AgentState::Active {
                if let Some(start) = agent.activated_at {
                    let secs = start.elapsed().as_secs();
                    if secs >= 60 {
                        format!(" {}m{}s", secs / 60, secs % 60)
                    } else {
                        format!(" {}s", secs)
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(
                    format!("{:<10} ", role),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(badge, Style::default().fg(Color::DarkGray)),
                Span::styled(detail_truncated, Style::default().fg(color)),
                Span::styled(elapsed_str, Style::default().fg(Color::Yellow)),
            ]));
        }

        // Completion status
        if self.complete {
            lines.push(Line::from(Span::styled(
                "─".repeat(30),
                Style::default().fg(Color::DarkGray),
            )));
            let (status_text, status_color) = if self.success {
                ("SUCCESS", Color::Green)
            } else {
                ("FAILED", Color::Red)
            };
            lines.push(Line::from(vec![Span::styled(
                format!("{} in {} turns", status_text, self.current_turn),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            )]));
            // Show modified files
            if !self.files_modified.is_empty() {
                for file in self.files_modified.iter().take(5) {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default().fg(Color::DarkGray)),
                        Span::styled(file, Style::default().fg(Color::Yellow)),
                    ]));
                }
            }
        }

        // Empty state hint
        if !self.complete && self.current_turn == 0 {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Awaiting team events...",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        lines
    }

    /// Get total activation count across all agents.
    pub fn total_activations(&self) -> u32 {
        self.agents.iter().map(|(_, a)| a.activation_count).sum()
    }

    pub fn has_active_agents(&self) -> bool {
        self.agents
            .iter()
            .any(|(_, a)| a.state == AgentState::Active)
    }

    pub fn current_turn(&self) -> u32 {
        self.current_turn
    }

    pub fn trust_value(&self) -> f64 {
        self.trust
    }

    pub fn max_turns(&self) -> u32 {
        self.max_turns
    }

    pub fn active_agent_name(&self) -> Option<String> {
        self.agents
            .iter()
            .find(|(_, a)| a.state == AgentState::Active)
            .map(|(role, _)| role.to_string())
    }
}

impl Default for TeamPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TeamPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 10 || area.height < 5 {
            return;
        }

        let content = self.build_content();

        // Brutalist-style rendering: heavy left border, no surrounding box
        let mut brutalist_content = Vec::new();

        // Top border line
        let top_border = format!("╺{}╸", "━".repeat((area.width as usize).saturating_sub(2)));
        brutalist_content.push(Line::from(Span::styled(
            top_border,
            Style::default().fg(Color::Cyan),
        )));

        // Wrap each content line with brutalist left border
        for line in &content {
            let mut spans = vec![Span::styled("▐ ", Style::default().fg(Color::Cyan))];
            spans.extend(line.spans.iter().cloned());
            brutalist_content.push(Line::from(spans));
        }

        // Bottom border line
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_panel_starts_hidden() {
        let panel = TeamPanel::new();
        assert!(!panel.visible);
        assert_eq!(panel.current_turn, 0);
        assert_eq!(panel.total_activations(), 0);
    }

    #[test]
    fn toggle_visibility() {
        let mut panel = TeamPanel::new();
        panel.toggle();
        assert!(panel.visible);
        panel.toggle();
        assert!(!panel.visible);
    }

    #[test]
    fn handle_agent_activated() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        assert_eq!(panel.agents[1].1.state, AgentState::Active);
        assert_eq!(panel.agents[1].1.activation_count, 1);
        assert_eq!(panel.current_turn, 1);
    }

    #[test]
    fn handle_agent_deactivated() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        panel.handle_event(&TeamEvent::AgentDeactivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Step complete".into(),
        });
        assert_eq!(panel.agents[1].1.state, AgentState::Complete);
    }

    #[test]
    fn handle_deactivated_failed() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        panel.handle_event(&TeamEvent::AgentDeactivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Parse failed".into(),
        });
        assert_eq!(panel.agents[1].1.state, AgentState::Failed);
    }

    #[test]
    fn handle_task_completed() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::TaskCompleted {
            success: true,
            turns: 5,
            files_modified: vec!["src/lib.rs".into()],
            final_trust: 0.95,
        });
        assert!(panel.complete);
        assert!(panel.success);
        assert_eq!(panel.current_turn, 5);
        assert!((panel.trust - 0.95).abs() < 0.01);
    }

    #[test]
    fn step_completed_resets_agents() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        panel.handle_event(&TeamEvent::AgentDeactivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Done".into(),
        });
        assert_eq!(panel.agents[1].1.state, AgentState::Complete);

        panel.handle_event(&TeamEvent::StepCompleted {
            step: 1,
            success: true,
            files: vec![],
        });
        assert_eq!(panel.agents[1].1.state, AgentState::Waiting);
    }

    #[test]
    fn reset_clears_state() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 3,
            reason: "Implementing".into(),
        });
        panel.reset();
        assert_eq!(panel.current_turn, 0);
        assert_eq!(panel.total_activations(), 0);
        assert!(!panel.has_active_agents());
    }

    #[test]
    fn has_active_agents() {
        let mut panel = TeamPanel::new();
        assert!(!panel.has_active_agents());
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        assert!(panel.has_active_agents());
    }

    #[test]
    fn render_does_not_crash() {
        let mut panel = TeamPanel::new();
        panel.set_task("fix the auth bug");
        panel.visible = true;
        panel.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing auth fix".into(),
        });

        let mut buf = Buffer::empty(Rect::new(0, 0, 35, 20));
        panel.render(Rect::new(0, 0, 35, 20), &mut buf);
    }

    #[test]
    fn render_too_small_does_not_crash() {
        let panel = TeamPanel::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 5, 2));
        panel.render(Rect::new(0, 0, 5, 2), &mut buf);
    }

    #[test]
    fn build_content_shows_empty_state() {
        let panel = TeamPanel::new();
        let content = panel.build_content();
        let has_awaiting = content
            .iter()
            .any(|line| line.spans.iter().any(|s| s.content.contains("Awaiting")));
        assert!(has_awaiting);
    }

    #[test]
    fn build_content_shows_success() {
        let mut panel = TeamPanel::new();
        panel.handle_event(&TeamEvent::TaskCompleted {
            success: true,
            turns: 3,
            files_modified: vec![],
            final_trust: 0.9,
        });
        let content = panel.build_content();
        let has_success = content
            .iter()
            .any(|line| line.spans.iter().any(|s| s.content.contains("SUCCESS")));
        assert!(has_success);
    }
}
