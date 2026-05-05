//! Unified session management for orchestration.
//!
//! Wraps task context with persistence, lifecycle management, and provider-agnostic
//! conversation state.

use crate::state_machine::TaskContext;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub messages: Vec<NeutralMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutralMessage {
    pub role: String, // "system", "user", "assistant"
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub max_context_tokens: usize,
    pub compression_threshold_pct: f64,
    pub persistence_enabled: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 128_000,
            compression_threshold_pct: 0.8,
            persistence_enabled: true,
        }
    }
}

pub struct OrchestrationSession {
    pub context: TaskContext,
    pub conversation: ConversationState,
    pub config: SessionConfig,
}

impl OrchestrationSession {
    pub fn new(task_id: String, request: String) -> Self {
        Self {
            context: TaskContext::new(task_id, request),
            conversation: ConversationState {
                messages: Vec::new(),
            },
            config: SessionConfig::default(),
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str) {
        self.conversation.messages.push(NeutralMessage {
            role: role.into(),
            content: content.into(),
        });
    }

    pub fn messages(&self) -> Vec<(String, String)> {
        self.conversation
            .messages
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect()
    }
}
