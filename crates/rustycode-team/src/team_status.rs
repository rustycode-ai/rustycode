//! Live CLI status renderer for team orchestration.
//!
//! Subscribes to TeamEvent broadcasts and renders a real-time ASCII dashboard
//! to stdout using ANSI escape codes for in-place updates. Works in any terminal.

use std::collections::HashMap;
use std::io::Write;

use super::orchestrator::TeamEvent;

/// Agent state for display purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AgentDisplayState {
    Waiting,
    Active,
    Complete,
    Failed,
}

impl std::fmt::Display for AgentDisplayState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Waiting => write!(f, "Waiting"),
            Self::Active => write!(f, "Active"),
            Self::Complete => write!(f, "Complete"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Tracks display state for a single agent.
#[derive(Debug, Clone)]
struct AgentDisplay {
    state: AgentDisplayState,
    detail: String,
    activation_count: u32,
}

/// Live status renderer that receives TeamEvents and draws an ASCII dashboard.
pub struct TeamStatusRenderer {
    agents: HashMap<String, AgentDisplay>,
    current_turn: u32,
    max_turns: u32,
    current_step: u32,
    trust: f64,
    task: String,
    complete: bool,
    success: bool,
    /// How many lines the dashboard occupies (for ANSI clearing).
    lines_rendered: usize,
}

impl TeamStatusRenderer {
    pub fn new(task: &str, max_turns: u32) -> Self {
        let mut agents = HashMap::new();
        for role in &["Architect", "Builder", "Skeptic", "Judge", "Scalpel"] {
            agents.insert(
                role.to_string(),
                AgentDisplay {
                    state: AgentDisplayState::Waiting,
                    detail: String::new(),
                    activation_count: 0,
                },
            );
        }
        Self {
            agents,
            current_turn: 0,
            max_turns,
            current_step: 0,
            trust: 0.7,
            task: task.to_string(),
            complete: false,
            success: false,
            lines_rendered: 0,
        }
    }

    /// Process a team event and update internal state.
    pub fn handle_event(&mut self, event: &TeamEvent) {
        match event {
            TeamEvent::AgentActivated { role, turn, reason } => {
                if let Some(agent) = self.agents.get_mut(role) {
                    agent.state = AgentDisplayState::Active;
                    agent.detail = reason.clone();
                    agent.activation_count = agent.activation_count.saturating_add(1);
                }
                self.current_turn = *turn;
            }
            TeamEvent::AgentStateChanged { role, to_state, .. } => {
                if let Some(agent) = self.agents.get_mut(role) {
                    agent.detail = to_state.clone();
                }
            }
            TeamEvent::AgentDeactivated { role, reason, .. } => {
                if let Some(agent) = self.agents.get_mut(role) {
                    agent.state = if reason.contains("Parse failed") || reason.contains("failed") {
                        AgentDisplayState::Failed
                    } else {
                        AgentDisplayState::Complete
                    };
                    agent.detail = reason.clone();
                }
            }
            TeamEvent::StepCompleted {
                step: _, success, ..
            } => {
                if *success {
                    self.current_step = self.current_step.saturating_add(1);
                }
                // Reset non-completed agents to waiting for next step
                for agent in self.agents.values_mut() {
                    if agent.state == AgentDisplayState::Complete {
                        agent.state = AgentDisplayState::Waiting;
                        agent.detail.clear();
                    }
                }
            }
            TeamEvent::TaskCompleted {
                success,
                turns,
                final_trust,
                ..
            } => {
                self.complete = true;
                self.success = *success;
                self.current_turn = *turns;
                self.trust = *final_trust;
                for agent in self.agents.values_mut() {
                    if agent.state == AgentDisplayState::Active {
                        agent.state = AgentDisplayState::Complete;
                    }
                }
            }
            TeamEvent::Insight { role, message } => {
                if let Some(agent) = self.agents.get_mut(role) {
                    agent.detail = message.clone();
                }
            }
            // Phase 4: Event-driven orchestration events (no-op for status display)
            TeamEvent::CodeChanged { .. }
            | TeamEvent::CompilationFailed { .. }
            | TeamEvent::TestsFailed { .. }
            | TeamEvent::TrustChanged { .. }
            | TeamEvent::VerificationPassed { .. }
            | TeamEvent::PatternDiscovered { .. }
            | TeamEvent::SecurityIssueDetected { .. }
            | TeamEvent::StructuralDeclarationSet { .. }
            | TeamEvent::PlanAdapted { .. }
            | TeamEvent::SpecialistCreated { .. }
            | TeamEvent::ParallelExecutionRequested { .. }
            | TeamEvent::ToolStarted { .. }
            | TeamEvent::ToolCompleted { .. }
            | TeamEvent::ToolLoopIteration { .. }
            | TeamEvent::AdvisorGuidance { .. }
            | TeamEvent::LLMTextChunk { .. }
            | TeamEvent::LLMThinkingChunk { .. } => {}
        }
    }

    /// Render the dashboard to stdout using ANSI escape codes.
    pub fn render(&mut self) -> std::io::Result<()> {
        let mut stdout = std::io::stdout();

        // Move cursor up to overwrite previous render
        if self.lines_rendered > 0 {
            write!(stdout, "\x1b[{}A", self.lines_rendered)?;
        }

        let mut lines = Vec::new();

        // Header
        let task_display = if self.task.len() > 50 {
            let truncated = match self.task.is_char_boundary(47) {
                true => &self.task[..47],
                false => {
                    let mut b = 47;
                    while b > 0 && !self.task.is_char_boundary(b) {
                        b -= 1;
                    }
                    &self.task[..b]
                }
            };
            format!("{}...", truncated)
        } else {
            self.task.clone()
        };
        lines.push(format!(
            "┌─ Team: \"{}\" ─{}┐",
            task_display,
            "─".repeat(60_usize.saturating_sub(task_display.len() + 12))
        ));

        // Status bar
        let trust_bar = format!("{:.2}", self.trust);
        lines.push(format!(
            "│ Turn {}/{} │ Trust: {} │ Step: {} │",
            self.current_turn, self.max_turns, trust_bar, self.current_step,
        ));

        lines.push(format!("├{}┤", "─".repeat(64)));

        // Agent rows
        let roles = ["Architect", "Builder", "Skeptic", "Judge", "Scalpel"];
        for role in &roles {
            let agent = match self.agents.get(*role) {
                Some(a) => a,
                None => continue,
            };
            let (icon, color) = match agent.state {
                AgentDisplayState::Waiting => ("◌", "\x1b[90m"), // dim
                AgentDisplayState::Active => ("⟳", "\x1b[32m"),  // green
                AgentDisplayState::Complete => ("✓", "\x1b[36m"), // cyan
                AgentDisplayState::Failed => ("✗", "\x1b[31m"),  // red
            };
            let detail_display = if agent.detail.is_empty() {
                agent.state.to_string()
            } else {
                agent.detail.clone()
            };
            let detail_truncated = if detail_display.len() > 30 {
                let truncated = match detail_display.is_char_boundary(27) {
                    true => &detail_display[..27],
                    false => {
                        let mut b = 27;
                        while b > 0 && !detail_display.is_char_boundary(b) {
                            b -= 1;
                        }
                        &detail_display[..b]
                    }
                };
                format!("{}...", truncated)
            } else {
                detail_display
            };
            lines.push(format!(
                "│ {}{}{}\x1b[0m {:<10} {}  {:<30} │",
                color, icon, "\x1b[0m", role, detail_truncated, ""
            ));
        }

        // Footer
        if self.complete {
            let status = if self.success {
                "\x1b[32mSUCCESS\x1b[0m"
            } else {
                "\x1b[31mFAILED\x1b[0m"
            };
            lines.push(format!("├{}┤", "─".repeat(64)));
            lines.push(format!(
                "│ Result: {} in {} turns{}│",
                status,
                self.current_turn,
                " ".repeat(40)
            ));
        }
        lines.push(format!("└{}┘", "─".repeat(64)));

        // Write all lines
        for line in &lines {
            writeln!(stdout, "\x1b[2K{}", line)?; // Clear line + write
        }
        stdout.flush()?;

        self.lines_rendered = lines.len();
        Ok(())
    }

    /// Print a final summary (no ANSI overwrite).
    pub fn print_summary(&self) {
        println!();
        println!("═══ Team Execution Summary ═══");
        println!("Task: {}", self.task);
        println!("Turns: {}", self.current_turn);
        println!("Trust: {:.2}", self.trust);
        println!("Steps completed: {}", self.current_step);
        println!(
            "Result: {}",
            if self.success { "SUCCESS" } else { "FAILED" }
        );

        println!("\nAgent activations:");
        let roles = ["Architect", "Builder", "Skeptic", "Judge", "Scalpel"];
        for role in &roles {
            if let Some(agent) = self.agents.get(*role) {
                println!(
                    "  {:<12} activated {}x — {:?}",
                    role, agent.activation_count, agent.state
                );
            }
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_initializes_with_all_agents_waiting() {
        let renderer = TeamStatusRenderer::new("test task", 50);
        assert_eq!(renderer.agents.len(), 5);
        for agent in renderer.agents.values() {
            assert_eq!(agent.state, AgentDisplayState::Waiting);
        }
    }

    #[test]
    fn handle_agent_activated() {
        let mut renderer = TeamStatusRenderer::new("test", 50);
        renderer.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        assert_eq!(renderer.agents["Builder"].state, AgentDisplayState::Active);
        assert_eq!(renderer.agents["Builder"].activation_count, 1);
    }

    #[test]
    fn handle_agent_deactivated() {
        let mut renderer = TeamStatusRenderer::new("test", 50);
        renderer.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        renderer.handle_event(&TeamEvent::AgentDeactivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Step complete".into(),
        });
        assert_eq!(
            renderer.agents["Builder"].state,
            AgentDisplayState::Complete
        );
    }

    #[test]
    fn handle_task_completed() {
        let mut renderer = TeamStatusRenderer::new("test", 50);
        renderer.handle_event(&TeamEvent::TaskCompleted {
            success: true,
            turns: 5,
            files_modified: vec!["src/lib.rs".into()],
            final_trust: 0.95,
        });
        assert!(renderer.complete);
        assert!(renderer.success);
        assert_eq!(renderer.current_turn, 5);
        assert!((renderer.trust - 0.95).abs() < 0.01);
    }

    #[test]
    fn handle_step_completed_resets_agents() {
        let mut renderer = TeamStatusRenderer::new("test", 50);
        renderer.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing".into(),
        });
        renderer.handle_event(&TeamEvent::AgentDeactivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Done".into(),
        });
        assert_eq!(
            renderer.agents["Builder"].state,
            AgentDisplayState::Complete
        );

        renderer.handle_event(&TeamEvent::StepCompleted {
            step: 1,
            success: true,
            files: vec![],
        });
        assert_eq!(renderer.agents["Builder"].state, AgentDisplayState::Waiting);
    }

    #[test]
    fn render_produces_output() {
        let mut renderer = TeamStatusRenderer::new("fix the auth bug", 50);
        renderer.handle_event(&TeamEvent::AgentActivated {
            role: "Builder".into(),
            turn: 1,
            reason: "Implementing auth fix".into(),
        });
        // Should not panic
        renderer.render().unwrap();
        assert!(renderer.lines_rendered > 0);
    }
}
