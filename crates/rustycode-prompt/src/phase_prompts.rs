//! Explore-Plan-Act prompt fragments.
//!
//! These prompts are intentionally small and phase-specific so the caller can
//! compose them with the layered prompt builder.

use crate::TemplateError;
use rustycode_protocol::ExecutionPhase;

/// Phase-specific prompt bundle.
#[derive(Debug, Clone)]
pub struct PhasePromptBuilder {
    explore: String,
    plan: String,
    act: String,
}

impl PhasePromptBuilder {
    /// Create a prompt builder from the built-in templates.
    #[must_use]
    pub fn new() -> Self {
        Self {
            explore: include_str!("../prompts/explore.txt").to_string(),
            plan: include_str!("../prompts/plan.txt").to_string(),
            act: include_str!("../prompts/act.txt").to_string(),
        }
    }

    /// Render the prompt for a given phase.
    pub fn render(&self, phase: ExecutionPhase, task: &str, plan_summary: Option<&str>) -> String {
        #[allow(clippy::match_same_arms)]
        let template = match phase {
            ExecutionPhase::Explore => &self.explore,
            ExecutionPhase::Plan => &self.plan,
            ExecutionPhase::Act => &self.act,
            _ => &self.explore,
        };

        let mut rendered = template.replace("{{task}}", task.trim());
        rendered = rendered.replace("{{phase}}", phase.label());
        rendered = rendered.replace("{{plan_summary}}", plan_summary.unwrap_or("").trim());
        rendered.trim().to_string()
    }

    /// Render only the phase fragment without a task.
    pub fn render_phase_only(&self, phase: ExecutionPhase) -> String {
        self.render(phase, "", None)
    }
}

impl Default for PhasePromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience helper for rendering a phase prompt.
pub fn render_phase_prompt(
    phase: ExecutionPhase,
    task: &str,
    plan_summary: Option<&str>,
) -> Result<String, TemplateError> {
    if task.trim().is_empty() {
        return Err(TemplateError::MissingVariable("task".to_string()));
    }

    Ok(PhasePromptBuilder::new().render(phase, task, plan_summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_prompt_mentions_exploration() {
        let prompt = PhasePromptBuilder::new().render(ExecutionPhase::Explore, "task", None);
        assert!(prompt.contains("Explore"));
        assert!(prompt.contains("task"));
    }

    #[test]
    fn plan_prompt_mentions_plan() {
        let prompt =
            PhasePromptBuilder::new().render(ExecutionPhase::Plan, "task", Some("summary"));
        assert!(prompt.contains("Plan"));
        assert!(prompt.contains("summary"));
    }

    #[test]
    fn act_prompt_mentions_act() {
        let prompt = PhasePromptBuilder::new().render(ExecutionPhase::Act, "task", Some("summary"));
        assert!(prompt.contains("Act"));
    }
}
