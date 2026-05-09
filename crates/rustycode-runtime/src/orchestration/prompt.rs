//! Unified orchestrator for system prompts.
use crate::orchestration::routing::resolve_task_routing;
use anyhow::Result;
use rustycode_config::TaskRoutingConfig;
use rustycode_prompt::{context, TemplateManager};
use rustycode_protocol::agent_protocol::agent_action_schema;

/// Central orchestrator for building system prompts.
///
/// Ensures TUI, CLI, and headless modes share identical prompt assembly logic.
pub struct PromptOrchestrator {
    template_manager: TemplateManager,
}

impl Default for PromptOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptOrchestrator {
    pub fn new() -> Self {
        Self {
            template_manager: TemplateManager::default(),
        }
    }

    /// Build the full system prompt for the given mode and context.
    pub async fn build_system_prompt(
        &self,
        mode: &str,
        query: &str,
        workspace_context: &str,
        is_headless: bool,
        supports_websocket: bool,
        routing: Option<&TaskRoutingConfig>,
    ) -> Result<String> {
        let routing = routing.cloned().unwrap_or_default();
        let decision = resolve_task_routing(query, Some(&routing), !is_headless).await;

        // 2. Base Coding Assistant Prompt
        let base_context = context! {
            "name" => "RustyCode",
            "context" => workspace_context,
            "websocket_available" => supports_websocket.to_string(),
            "intent_suffix" => decision.intent.prompt_suffix(),
            "routing_enabled" => routing.enabled,
            "routing_confidence" => decision.confidence,
            "routing_confidence_threshold" => routing.confidence_threshold,
            "routing_max_clarifying_questions" => routing.max_clarifying_questions,
            "routing_max_research_passes" => routing.max_research_passes,
            "interactive_mode" => !is_headless,
            "routing_intent" => format!("{:?}", decision.intent),
            "routing_workflow" => decision.workflow.to_string(),
            "routing_harness" => decision.harness.to_string(),
            "routing_harness_summary" => decision.harness.summary(),
            "routing_thinking" => decision.thinking.depth.to_string(),
            "routing_thinking_style" => decision.thinking.style.to_string(),
            "routing_thinking_decompose_tasks" => decision.thinking.decompose_tasks.to_string(),
            "routing_thinking_ask_clarifying_questions" => decision.thinking.ask_clarifying_questions.to_string(),
            "routing_thinking_assign_responsibility" => decision.thinking.assign_responsibility.to_string(),
            "routing_thinking_define_dependencies" => decision.thinking.define_dependencies.to_string(),
            "routing_thinking_verify_before_acting" => decision.thinking.verify_before_acting.to_string(),
            "routing_thinking_summary" => decision.thinking.summary(),
            "routing_next_step" => decision.execution_plan.next_step.clone(),
            "routing_responsibilities" => decision.execution_plan.responsibilities.join(", "),
            "routing_dependencies" => decision.execution_plan.dependencies.join(", "),
            "routing_verification" => decision.execution_plan.verification.join(", "),
            "routing_execution_plan" => decision.execution_plan.summary(),
            "routing_team" => decision.team.to_string(),
            "routing_agent" => decision.agent.to_string(),
            "routing_action" => decision.action_label(),
            "routing_summary" => decision.summary(),
            "routing_skills" => decision.skills.join(", "),
            "routing_missing_info" => decision.missing_info.join(", "),
            "routing_handoff_payload" => decision.handoff_payload(
                "Uncertainty remained after research; handing off to the next workflow."
            ).to_json(),
        };
        let base_prompt = self
            .template_manager
            .coding_assistant_prompt(&base_context)?;

        // Wrap with Anthropic Cache Boundaries
        let schema = agent_action_schema();
        let cached_prompt = format!(
            "<anthropic-cache>\n{}\n<agent_action_schema>\n{}\n</agent_action_schema>\n</anthropic-cache>",
            base_prompt, schema
        );

        // 3. Assemble with Mode/Intent/Headless-specific tweaks
        let render_context = context! {
            "coding_assistant_base" => cached_prompt,
            "prompt" => cached_prompt,
            "mode" => mode,
            "intent" => format!("{:?}", decision.intent),
            "intent_suffix" => decision.intent.prompt_suffix(),
            "intent_confidence" => decision.confidence,
            "routing_enabled" => routing.enabled,
            "routing_confidence" => decision.confidence,
            "routing_confidence_threshold" => routing.confidence_threshold,
            "routing_max_clarifying_questions" => routing.max_clarifying_questions,
            "routing_max_research_passes" => routing.max_research_passes,
            "interactive_mode" => !is_headless,
            "routing_intent" => format!("{:?}", decision.intent),
            "routing_workflow" => decision.workflow.to_string(),
            "routing_harness" => decision.harness.to_string(),
            "routing_harness_summary" => decision.harness.summary(),
            "routing_thinking" => decision.thinking.depth.to_string(),
            "routing_thinking_style" => decision.thinking.style.to_string(),
            "routing_thinking_decompose_tasks" => decision.thinking.decompose_tasks.to_string(),
            "routing_thinking_ask_clarifying_questions" => decision.thinking.ask_clarifying_questions.to_string(),
            "routing_thinking_assign_responsibility" => decision.thinking.assign_responsibility.to_string(),
            "routing_thinking_define_dependencies" => decision.thinking.define_dependencies.to_string(),
            "routing_thinking_verify_before_acting" => decision.thinking.verify_before_acting.to_string(),
            "routing_thinking_summary" => decision.thinking.summary(),
            "routing_next_step" => decision.execution_plan.next_step.clone(),
            "routing_responsibilities" => decision.execution_plan.responsibilities.join(", "),
            "routing_dependencies" => decision.execution_plan.dependencies.join(", "),
            "routing_verification" => decision.execution_plan.verification.join(", "),
            "routing_execution_plan" => decision.execution_plan.summary(),
            "routing_team" => decision.team.to_string(),
            "routing_agent" => decision.agent.to_string(),
            "routing_action" => decision.action_label(),
            "routing_summary" => decision.summary(),
            "routing_skills" => decision.skills.join(", "),
            "routing_missing_info" => decision.missing_info.join(", "),
            "routing_handoff_payload" => decision.handoff_payload(
                "Uncertainty remained after research; handing off to the next workflow."
            ).to_json(),
        };

        if is_headless {
            Ok(self
                .template_manager
                .render("system/headless_coding_agent", &render_context)?)
        } else {
            Ok(self
                .template_manager
                .coding_assistant_prompt(&render_context)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interactive_prompt_includes_routing_guidance() {
        let orchestrator = PromptOrchestrator::new();
        let prompt = orchestrator
            .build_system_prompt(
                "auto",
                "help me understand this parser",
                "workspace context",
                false,
                false,
                Some(&TaskRoutingConfig::default()),
            )
            .await
            .expect("prompt should render");

        assert!(prompt.contains("Task Routing"));
        assert!(prompt.contains("Suggested harness"));
        assert!(prompt.contains("Suggested thinking depth"));
        assert!(prompt.contains("Next step"));
        assert!(prompt.contains("clarifying questions"));
        assert!(prompt.contains("Intent Guidance"));
    }

    #[tokio::test]
    async fn headless_prompt_includes_handoff_guidance() {
        let orchestrator = PromptOrchestrator::new();
        let prompt = orchestrator
            .build_system_prompt(
                "auto",
                "help me understand this parser",
                "workspace context",
                true,
                false,
                Some(&TaskRoutingConfig::default()),
            )
            .await
            .expect("prompt should render");

        assert!(prompt.contains("ROUTING DISCIPLINE"));
        assert!(prompt.contains("Harness summary"));
        assert!(prompt.contains("Suggested thinking depth"));
        assert!(prompt.contains("Next step"));
        assert!(prompt.contains("RUSTYTOOL_HANDOFF"));
    }
}
