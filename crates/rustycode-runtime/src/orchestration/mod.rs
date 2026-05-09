//! Unified orchestration for system prompts, intent detection, and context assembly.
//!
//! This service acts as the single source of truth for preparing AI requests,
//! ensuring that TUI, CLI, and Terminal Bench share the exact same logic.

use anyhow::Result;
use rustycode_llm::provider::ChatMessage;

pub mod intent;
pub mod llm_intent;
pub mod prompt;
pub mod routing;

pub use llm_intent::{
    ClassificationSource, EnhancedIntentAssessment, LlmFallbackBudget, LlmIntentClassifier,
};
pub use prompt::PromptOrchestrator;
pub use routing::{
    build_headless_routing_preface, parse_task_routing_handoff, resolve_task_routing,
};

#[derive(Debug)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCall(String, String, String), // id, name, args
    TurnComplete,
}

pub trait AgentSession: Send + Sync {
    fn send_input(
        &mut self,
        messages: Vec<ChatMessage>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;
    fn receive_event(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AgentEvent>> + Send + '_>>;
}
