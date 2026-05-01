//! `AgentSessionExecutor` — implements [`ToolExecutor`] by delegating to
//! [`AgentSession::run()`].
//!
//! When Musician "plays a step", this executor builds a single-user-message
//! conversation and runs the full LLM↔tool loop via `AgentSession`.  The
//! orchestration layer handles tiered planning; the agent handles the thin
//! tool-calling loop.

use crate::bus::OrchestrationEvent;
use crate::error::{OrchestrationError, Result};
use crate::musician::ToolExecutor;
use crate::types::StepResult;
use rustycode_agent::{
    AgentConfig, AgentEvents, AgentResult, AgentSession, ApprovalDecision, StoppedReason,
};
use rustycode_llm::provider::{ChatMessage, LLMProvider, MessageContent, MessageRole};
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_tools::ToolRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// SilentEvents — discards streaming deltas; only collects final text
// ---------------------------------------------------------------------------

/// Event sink that absorbs agent events and forwards UI-relevant ones to the bus.
struct BusAgentEvents {
    bus: crate::bus::BusHandle,
    task_id: String,
    final_text: String,
    /// Pending tools buffer to assemble input from streaming deltas.
    pending_tools: HashMap<String, (String, String)>, // id -> (name, input_json)
}

impl BusAgentEvents {
    fn new(bus: crate::bus::BusHandle, task_id: String) -> Self {
        Self {
            bus,
            task_id,
            final_text: String::new(),
            pending_tools: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl AgentEvents for BusAgentEvents {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta { content } => {
                self.final_text.push_str(&content);
                self.bus.publish(OrchestrationEvent::StreamDelta {
                    task_id: self.task_id.clone(),
                    content,
                });
            }
            StreamEvent::ToolCallStarted { id, name } => {
                // Initialize pending tool entry with empty input
                self.pending_tools.insert(id, (name, String::new()));
            }
            StreamEvent::ToolInputDelta { id, chunk } => {
                if let Some((_, input)) = self.pending_tools.get_mut(&id) {
                    input.push_str(&chunk);
                }
            }
            StreamEvent::ToolExecStarted { id, name: _ } => {
                // Input assembly complete — publish ToolExecutionStarted with accumulated args
                if let Some((tool_name, input_json)) = self.pending_tools.get(&id) {
                    self.bus.publish(OrchestrationEvent::ToolExecutionStarted {
                        task_id: self.task_id.clone(),
                        tool: tool_name.clone(),
                        args: input_json.clone(),
                    });
                } else {
                    tracing::warn!(tool_id = %id, "ToolExecStarted without pending tool");
                }
            }
            StreamEvent::ToolExecCompleted {
                id,
                name,
                output,
                is_error: _,
            } => {
                // Prefer the name from the event; fall back to pending_tools lookup
                let tool_name = if name.is_empty() {
                    self.pending_tools
                        .get(&id)
                        .map_or_else(|| "unknown".to_string(), |(n, _)| n.clone())
                } else {
                    name
                };
                self.bus.publish(OrchestrationEvent::ToolExecutionFinished {
                    task_id: self.task_id.clone(),
                    tool: tool_name,
                    result: output,
                });
                self.pending_tools.remove(&id);
            }
            StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
            } => {
                self.bus.publish(OrchestrationEvent::TokenUsage {
                    task_id: self.task_id.clone(),
                    input_tokens,
                    output_tokens,
                });
            }
            // Other events are ignored for bus
            StreamEvent::ThinkingDelta { .. }
            | StreamEvent::Done
            | _ => {}
        }
    }

    async fn on_approval_needed(
        &mut self,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> ApprovalDecision {
        // Auto-approve in orchestration mode
        ApprovalDecision::AutoApproved
    }

    async fn on_question(&mut self, _question: &str, _options: &[String]) -> Option<String> {
        None
    }

    async fn on_done(&mut self, result: &AgentResult) {
        tracing::debug!(
            reason = ?result.stopped_reason,
            input_tokens = result.total_input_tokens,
            output_tokens = result.total_output_tokens,
            "AgentSessionExecutor: done"
        );
    }
}

// ---------------------------------------------------------------------------
// BridgeEvents — streaming events + interactive approval via PipelineInteraction
// ---------------------------------------------------------------------------

struct BridgeEvents {
    bus: crate::bus::BusHandle,
    interaction: Arc<dyn crate::pipeline::PipelineInteraction>,
    task_id: String,
    step_id: String,
    final_text: String,
    tool_call_counter: usize,
    pending_tools: HashMap<String, (String, String)>, // id -> (name, input_json)
}

impl BridgeEvents {
    fn new(
        bus: crate::bus::BusHandle,
        interaction: Arc<dyn crate::pipeline::PipelineInteraction>,
        task_id: impl Into<String>,
        step_id: impl Into<String>,
    ) -> Self {
        Self {
            bus,
            interaction,
            task_id: task_id.into(),
            step_id: step_id.into(),
            final_text: String::new(),
            tool_call_counter: 0,
            pending_tools: HashMap::new(),
        }
    }

    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}…", &s[..max_len.saturating_sub(1)])
        }
    }
}

#[async_trait::async_trait]
impl AgentEvents for BridgeEvents {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta { content } => {
                self.final_text.push_str(&content);
                self.bus.publish(OrchestrationEvent::TextDelta {
                    task_id: self.task_id.clone(),
                    content,
                });
            }
            StreamEvent::ThinkingDelta { content } => {
                self.bus.publish(OrchestrationEvent::ThinkingDelta {
                    task_id: self.task_id.clone(),
                    content,
                });
            }
            StreamEvent::ToolCallStarted { id, name } => {
                // Initialize pending tool entry with empty input buffer
                self.pending_tools.insert(id, (name, String::new()));
            }
            StreamEvent::ToolInputDelta { id, chunk } => {
                if let Some((_, input)) = self.pending_tools.get_mut(&id) {
                    input.push_str(&chunk);
                }
                self.bus.publish(OrchestrationEvent::ToolInputDelta {
                    task_id: self.task_id.clone(),
                    tool_id: id,
                    chunk,
                });
            }
            StreamEvent::ToolExecStarted { id, name: _ } => {
                // Called when tool input fully assembled and execution begins
                if let Some((tool_name, input_json)) = self.pending_tools.get(&id) {
                    self.tool_call_counter += 1;
                    self.bus.publish(OrchestrationEvent::ToolCallStarted {
                        task_id: self.task_id.clone(),
                        step_id: self.step_id.clone(),
                        tool_id: id.clone(),
                        tool_name: tool_name.clone(),
                        input_preview: Self::truncate(input_json, 500),
                    });
                } else {
                    tracing::warn!(tool_id = %id, "ToolExecStarted without pending tool");
                }
            }
            StreamEvent::ToolExecCompleted {
                id,
                name,
                output,
                is_error: _,
            } => {
                // Prefer the name from the event; fall back to pending_tools lookup
                let tool_name = if name.is_empty() {
                    self.pending_tools
                        .get(&id)
                        .map_or_else(|| "unknown".to_string(), |(n, _)| n.clone())
                } else {
                    name
                };
                self.bus.publish(OrchestrationEvent::ToolCallCompleted {
                    task_id: self.task_id.clone(),
                    step_id: self.step_id.clone(),
                    tool_id: id.clone(),
                    tool_name,
                    success: true,
                    output_preview: Self::truncate(&output, 500),
                });
                self.pending_tools.remove(&id);
            }
            StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
            } => {
                self.bus.publish(OrchestrationEvent::TokenUsage {
                    task_id: self.task_id.clone(),
                    input_tokens,
                    output_tokens,
                });
            }
            StreamEvent::CacheUsage {
                cache_read_tokens,
                cache_creation_tokens,
            } => {
                self.bus.publish(OrchestrationEvent::CacheUsage {
                    task_id: self.task_id.clone(),
                    cache_read_tokens,
                    cache_creation_tokens,
                });
            }
            StreamEvent::Done | _ => {}
        }
    }

    async fn on_approval_needed(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalDecision {
        let tool = tool_name.to_string();
        let input = input.clone();
        self.interaction.request_approval(&tool, &input).await
    }

    async fn on_question(&mut self, _question: &str, _options: &[String]) -> Option<String> {
        None
    }

    async fn on_done(&mut self, result: &AgentResult) {
        tracing::debug!(
            reason = ?result.stopped_reason,
            tool_calls = self.tool_call_counter,
            "BridgeEvents: done"
        );
    }
}

// ---------------------------------------------------------------------------
// AgentSessionExecutor
// ---------------------------------------------------------------------------

/// Wraps an [`AgentSession`] to implement the orchestration [`ToolExecutor`]
/// trait.
///
/// Each call to [`execute`](ToolExecutor::execute) builds a fresh
/// conversation with the step description as the user message, then runs the
/// agent loop to completion.
pub struct AgentSessionExecutor {
    provider: Arc<dyn LLMProvider>,
    tool_registry: Arc<ToolRegistry>,
    system_prompt: String,
    model: String,
    config: AgentConfig,
    cwd: PathBuf,
    bus: crate::bus::BusHandle,
    interaction: Arc<tokio::sync::Mutex<Option<Arc<dyn crate::pipeline::PipelineInteraction>>>>,
}

impl AgentSessionExecutor {
    pub fn new(
        provider: Arc<dyn LLMProvider>,
        tool_registry: Arc<ToolRegistry>,
        system_prompt: impl Into<String>,
        model: impl Into<String>,
        cwd: impl Into<PathBuf>,
        bus: crate::bus::BusHandle,
    ) -> Self {
        Self {
            provider,
            tool_registry,
            system_prompt: system_prompt.into(),
            model: model.into(),
            config: AgentConfig::default(),
            cwd: cwd.into(),
            bus,
            interaction: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Override the default [`AgentConfig`].
    pub const fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_interaction(
        mut self,
        interaction: Arc<tokio::sync::Mutex<Option<Arc<dyn crate::pipeline::PipelineInteraction>>>>,
    ) -> Self {
        self.interaction = interaction;
        self
    }

    /// Build the tool schemas JSON array from the registry, in the format
    /// expected by [`AgentSession::run()`].
    fn build_tool_schemas(&self) -> Vec<serde_json::Value> {
        self.tool_registry
            .list()
            .into_iter()
            .map(|info| {
                let mut schema = serde_json::json!({
                    "name": info.name,
                    "description": info.description,
                    "input_schema": info.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &info.name,
                    matches!(info.permission, rustycode_tools::ToolPermission::Read),
                ) {
                    schema["annotations"] = annotations;
                }
                schema
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ToolExecutor for AgentSessionExecutor {
    async fn execute(
        &self,
        task_id: &str,
        _tool_name: &str,
        input: &str,
        _allowed_tools: &[&'static str],
        model: &str,
    ) -> Result<StepResult> {
        let mut session = AgentSession::new(self.config.clone(), &self.cwd);
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(input.to_string()),
        }];

        let schemas = self.build_tool_schemas();
        let effective_model = if model.is_empty() { &self.model } else { model };

        let interaction_guard = self.interaction.lock().await;
        let interaction_opt: Option<Arc<dyn crate::pipeline::PipelineInteraction>> =
            interaction_guard.clone();
        drop(interaction_guard);

        let agent_result = if let Some(interaction) = interaction_opt {
            let mut events =
                BridgeEvents::new(self.bus.clone(), interaction, task_id, "pipeline-step");
            session
                .run(
                    self.provider.as_ref(),
                    effective_model,
                    &self.system_prompt,
                    messages,
                    &schemas,
                    self.tool_registry.as_ref(),
                    &mut events,
                )
                .await
                .map_err(|e| {
                    OrchestrationError::ToolExecution(format!("agent session error: {e}"))
                })?
        } else {
            let mut events = BusAgentEvents::new(self.bus.clone(), task_id.to_string());
            session
                .run(
                    self.provider.as_ref(),
                    effective_model,
                    &self.system_prompt,
                    messages,
                    &schemas,
                    self.tool_registry.as_ref(),
                    &mut events,
                )
                .await
                .map_err(|e| {
                    OrchestrationError::ToolExecution(format!("agent session error: {e}"))
                })?
        };

        let exit_code = match agent_result.stopped_reason {
            StoppedReason::NoToolCalls => Some(0),
            StoppedReason::MaxTurnsReached => {
                tracing::warn!("AgentSession hit max turns for step");
                Some(0)
            }
            StoppedReason::TimeoutExceeded => {
                tracing::warn!("AgentSession timed out for step");
                Some(1)
            }
        };

        Ok(StepResult {
            output: agent_result.final_text,
            exit_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tool_schemas_empty_registry() {
        let registry = Arc::new(ToolRegistry::new());
        let executor = AgentSessionExecutor::new(
            Arc::new(rustycode_llm::mock::MockProvider::from_text("ok")),
            registry,
            "system",
            "model",
            "/tmp",
            crate::bus::BusHandle::new(16),
        );
        let schemas = executor.build_tool_schemas();
        assert!(schemas.is_empty());
    }

    #[test]
    fn test_build_tool_schemas_with_tool() {
        use rustycode_tools::{Tool, ToolContext, ToolOutput};

        struct EchoTool;
        impl Tool for EchoTool {
            fn name(&self) -> &'static str {
                "echo"
            }
            fn description(&self) -> &'static str {
                "Echoes input"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}})
            }
            fn execute(
                &self,
                _params: serde_json::Value,
                _ctx: &ToolContext,
            ) -> anyhow::Result<ToolOutput> {
                Ok(ToolOutput::text("echo"))
            }
        }

        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);

        let executor = AgentSessionExecutor::new(
            Arc::new(rustycode_llm::mock::MockProvider::from_text("ok")),
            Arc::new(registry),
            "system",
            "model",
            "/tmp",
            crate::bus::BusHandle::new(16),
        );
        let schemas = executor.build_tool_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["name"], "echo");
    }

    #[test]
    fn test_agent_session_executor_model_fallback() {
        let registry = Arc::new(ToolRegistry::new());
        let executor = AgentSessionExecutor::new(
            Arc::new(rustycode_llm::mock::MockProvider::from_text("ok")),
            registry,
            "system",
            "default-model",
            "/tmp",
            crate::bus::BusHandle::new(16),
        );
        // When model param is empty, should use configured default
        assert_eq!(executor.model, "default-model");
    }

    #[test]
    fn test_bus_events_collects_text() {
        use rustycode_protocol::stream_event::StreamEvent;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut events = BusAgentEvents::new(crate::bus::BusHandle::new(16), "t1".to_string());
        rt.block_on(events.on_event(StreamEvent::TextDelta {
            content: "hello ".into(),
        }));
        rt.block_on(events.on_event(StreamEvent::TextDelta {
            content: "world".into(),
        }));
        assert_eq!(events.final_text, "hello world");
    }
}
