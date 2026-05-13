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
use rustycode_agent_runtime::{
    AgentConfig, AgentEvents, AgentResult, AgentSession, ApprovalDecision, StoppedReason,
};
use rustycode_llm::provider::{ChatMessage, LLMProvider, MessageContent, MessageRole};
use rustycode_tools::ToolRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// SilentEvents — discards streaming deltas; only collects final text

/// Event sink that absorbs agent events and forwards UI-relevant ones to the bus.
struct BusAgentEvents {
    _bus: crate::bus::BusHandle,
    _task_id: String,
}

impl BusAgentEvents {
    fn new(bus: crate::bus::BusHandle, task_id: String) -> Self {
        Self {
            _bus: bus,
            _task_id: task_id,
        }
    }
}

#[async_trait::async_trait]
impl AgentEvents for BusAgentEvents {
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

// BridgeEvents — streaming events + interactive approval via PipelineInteraction

struct BridgeEvents {
    _bus: crate::bus::BusHandle,
    interaction: Arc<dyn crate::pipeline::PipelineInteraction>,
    _task_id: String,
    _step_id: String,
}

impl BridgeEvents {
    fn new(
        bus: crate::bus::BusHandle,
        interaction: Arc<dyn crate::pipeline::PipelineInteraction>,
        task_id: impl Into<String>,
        step_id: impl Into<String>,
    ) -> Self {
        Self {
            _bus: bus,
            interaction,
            _task_id: task_id.into(),
            _step_id: step_id.into(),
        }
    }
}

#[async_trait::async_trait]
impl AgentEvents for BridgeEvents {
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
            "BridgeEvents: done"
        );
    }
}

// EventForwarder — forwards EventMsg to OrchestrationEvent bus
#[allow(dead_code)]
struct EventForwarder {
    bus: crate::bus::BusHandle,
    task_id: String,
    step_id: String,
    pending_tools: HashMap<String, (String, String)>, // id -> (name, input_json)
}

impl EventForwarder {
    fn new(bus: crate::bus::BusHandle, task_id: String, step_id: String) -> Self {
        Self {
            bus,
            task_id,
            step_id,
            pending_tools: HashMap::new(),
        }
    }

    fn handle_event(&mut self, msg: rustycode_protocol::EventMsg) {
        use rustycode_protocol::EventMsg;

        match msg {
            EventMsg::TextDelta { delta } => {
                self.bus.publish(OrchestrationEvent::TextDelta {
                    task_id: self.task_id.clone(),
                    content: delta.clone(),
                });
                self.bus.publish(OrchestrationEvent::StreamDelta {
                    task_id: self.task_id.clone(),
                    content: delta,
                });
            }
            EventMsg::ThinkingDelta { delta } => {
                self.bus.publish(OrchestrationEvent::ThinkingDelta {
                    task_id: self.task_id.clone(),
                    content: delta,
                });
            }
            EventMsg::ToolCallStarted {
                tool_name,
                tool_id,
                input,
            } => {
                let input_str = input.to_string();
                self.pending_tools
                    .insert(tool_id.clone(), (tool_name.clone(), input_str.clone()));
                self.bus.publish(OrchestrationEvent::ToolCallStarted {
                    task_id: self.task_id.clone(),
                    step_id: self.step_id.clone(),
                    tool_id,
                    tool_name,
                    input_preview: if input_str.len() > 500 {
                        format!("{}…", &input_str[..500])
                    } else {
                        input_str
                    },
                });
            }
            EventMsg::ToolInputDelta { tool_id, delta } => {
                if let Some((_, input)) = self.pending_tools.get_mut(&tool_id) {
                    input.push_str(&delta);
                }
                self.bus.publish(OrchestrationEvent::ToolInputDelta {
                    task_id: self.task_id.clone(),
                    tool_id,
                    chunk: delta,
                });
            }
            EventMsg::ToolExecStarted { tool_name, tool_id } => {
                let input_json = self
                    .pending_tools
                    .get(&tool_id)
                    .map(|(_, input)| input.clone())
                    .unwrap_or_default();

                self.bus.publish(OrchestrationEvent::ToolExecutionStarted {
                    task_id: self.task_id.clone(),
                    tool: tool_name,
                    args: input_json,
                });
            }
            EventMsg::ToolExecCompleted {
                tool_id,
                tool_name,
                success,
                output,
                ..
            } => {
                self.bus.publish(OrchestrationEvent::ToolExecutionFinished {
                    task_id: self.task_id.clone(),
                    tool: tool_name.clone(),
                    result: output.clone(),
                });
                self.bus.publish(OrchestrationEvent::ToolCallCompleted {
                    task_id: self.task_id.clone(),
                    step_id: self.step_id.clone(),
                    tool_id: tool_id.clone(),
                    tool_name,
                    success,
                    output_preview: if output.len() > 500 {
                        format!("{}…", &output[..500])
                    } else {
                        output
                    },
                });
                self.pending_tools.remove(&tool_id);
            }
            EventMsg::TokenUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
            } => {
                self.bus.publish(OrchestrationEvent::TokenUsage {
                    task_id: self.task_id.clone(),
                    input_tokens,
                    output_tokens,
                });
                if cache_read_tokens > 0 || cache_creation_tokens > 0 {
                    self.bus.publish(OrchestrationEvent::CacheUsage {
                        task_id: self.task_id.clone(),
                        cache_read_tokens,
                        cache_creation_tokens,
                    });
                }
            }
            _ => {
                tracing::debug!(event = ?msg, "unhandled EventMsg variant in handle_event");
            }
        }
    }
}

// AgentSessionExecutor

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
        let tools = self.tool_registry.list();
        rustycode_tools_api::build_canonical_tool_schemas(&tools)
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
        use std::sync::Arc;

        let mut session = AgentSession::new(self.config.clone(), &self.cwd);

        // Wire a sync adapter over the async mailbox router for send_message.
        let mailbox = crate::mailbox_router::MailboxRouter::new(self.bus.clone());
        let sender = crate::mailbox_sender::MailboxSender::new(mailbox);
        session = session.with_message_sender(Arc::new(sender));

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

        // Subscribe to unified events for forwarding to bus
        let mut event_rx = session.subscribe();
        let mut forwarder = EventForwarder::new(
            self.bus.clone(),
            task_id.to_string(),
            "pipeline-step".to_string(),
        );

        let _forwarder_handle = tokio::spawn(async move {
            while let Ok(msg) = event_rx.recv().await {
                forwarder.handle_event(msg);
            }
        });

        // Still use legacy events for synchronous approval/question gating
        // (will be replaced by Op submission in Phase 1C)
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
    #[allow(clippy::unwrap_used)]
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
    }
}
