//! Lifecycle management plugins for agent onboarding and offboarding.

use super::{AgentPlugin, TurnContext};
use crate::provider_context::ProviderContext;
use async_trait::async_trait;
use rustycode_protocol::reasoning_summary::ReasoningSummary;

/// Result produced when the lifecycle plugin completes offboarding.
#[derive(Debug, Clone)]
pub struct OffboardingResult {
    /// Provider context used during the session.
    pub provider_context: Option<ProviderContext>,
    /// Reasoning summary captured at session end.
    pub reasoning_summary: Option<ReasoningSummary>,
    /// Total turns completed.
    pub turns_completed: usize,
}

/// Plugin for agent onboarding and offboarding.
///
/// Captures provider context and reasoning summaries so they can be
/// forwarded to subsequent agents or persisted across compaction.
pub struct LifecyclePlugin {
    /// Context carried forward from parent.
    pub provider_context: Option<ProviderContext>,
    /// Summary of reasoning to be persisted.
    pub handoff_summary: Option<ReasoningSummary>,
    /// Turns completed (incremented by on_tool_result calls).
    turns_completed: usize,
}

impl LifecyclePlugin {
    pub fn new() -> Self {
        Self {
            provider_context: None,
            handoff_summary: None,
            turns_completed: 0,
        }
    }

    /// Set the provider context for this agent run.
    pub fn with_provider_context(mut self, ctx: ProviderContext) -> Self {
        self.provider_context = Some(ctx);
        self
    }

    /// Set an initial reasoning summary from a parent agent.
    pub fn with_handoff_summary(mut self, summary: ReasoningSummary) -> Self {
        self.handoff_summary = Some(summary);
        self
    }

    /// Produce an offboarding result for handoff to the next agent
    /// or for persistence in a CompactionSnapshot.
    pub fn into_offboarding_result(self) -> OffboardingResult {
        OffboardingResult {
            provider_context: self.provider_context,
            reasoning_summary: self.handoff_summary,
            turns_completed: self.turns_completed,
        }
    }
}

impl Default for LifecyclePlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentPlugin for LifecyclePlugin {
    async fn on_start(&mut self, _ctx: &TurnContext) {
        tracing::info!(
            agent_model = ?self.provider_context.as_ref().map(|c| c.model.as_str()),
            has_handoff = self.handoff_summary.is_some(),
            "Agent onboarding: lifecycle plugin initialized"
        );
    }

    async fn on_done(&mut self, ctx: &TurnContext) {
        self.turns_completed = ctx.turn;
        tracing::info!(
            turns = ctx.turn,
            total_input_tokens = ctx.total_input_tokens,
            total_output_tokens = ctx.total_output_tokens,
            "Agent offboarding: lifecycle plugin finalized"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_plugin_has_no_state() {
        let plugin = LifecyclePlugin::new();
        assert!(plugin.provider_context.is_none());
        assert!(plugin.handoff_summary.is_none());
        assert_eq!(plugin.turns_completed, 0);
    }

    #[test]
    fn default_is_same_as_new() {
        let default = LifecyclePlugin::default();
        let new = LifecyclePlugin::new();
        assert!(default.provider_context.is_none());
        assert!(new.provider_context.is_none());
    }

    #[test]
    fn with_provider_context_sets_field() {
        let ctx = ProviderContext::new("anthropic", "claude-sonnet-4-6", "sk-test");
        let plugin = LifecyclePlugin::new().with_provider_context(ctx);
        assert_eq!(
            plugin.provider_context.as_ref().unwrap().provider_name,
            "anthropic"
        );
    }

    #[test]
    fn with_handoff_summary_sets_field() {
        let summary = ReasoningSummary::from_parts(3, 0.85, 0.80, vec![], "sequential", true);
        let plugin = LifecyclePlugin::new().with_handoff_summary(summary);
        assert!(plugin.handoff_summary.is_some());
        assert_eq!(plugin.handoff_summary.as_ref().unwrap().thought_count, 3);
    }

    #[test]
    fn into_offboarding_result_extracts_state() {
        let ctx = ProviderContext::new("openai", "gpt-4", "key");
        let plugin = LifecyclePlugin::new()
            .with_provider_context(ctx)
            .with_handoff_summary(ReasoningSummary::empty());
        let result = plugin.into_offboarding_result();
        assert!(result.provider_context.is_some());
        assert!(result.reasoning_summary.is_some());
        assert_eq!(result.turns_completed, 0);
    }
}
