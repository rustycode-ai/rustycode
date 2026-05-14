//! `AgentSession` — the shared thin LLM↔tool loop.
//!
//! No heuristics. No nudges. No behavioral injection.
//! Hard limits only: max turns, wall-clock timeout, context budget.

use anyhow::Result;
use futures::Stream;
use rustycode_llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, EffortLevel, LLMProvider, MessageRole,
    OutputConfig, StreamChunk, ToolChoice,
};
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_protocol::{ContentBlock, EventMsg, MessageContent};
use rustycode_tools::ToolRegistry;
use rustycode_tools_api::MessageSender;

use crate::context::prune_messages;
use crate::intelligence::CodeIntelligence;
use crate::provider_context::ProviderContext;
use crate::tool_exec::{execute_tool, truncate_tool_output};
use crate::turn::{collect_completion_turn, collect_stream_turn};
use rustycode_guard::hooks_expanded::{ExpandedHookDispatcher, LifecycleEvent, LifecycleHook};
use rustycode_tools_api::tiers::{ToolActivationManager, ToolTier};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Hard limits for the agent loop. No behavioral tuning - just caps.
#[derive(Clone)]
pub struct AgentConfig {
    /// Maximum number of LLM↔tool turns (default: 25).
    pub max_turns: usize,
    /// Wall-clock timeout in seconds (default: 900).
    pub timeout_secs: u64,
    /// Maximum bytes for a single tool result before truncation (default: 8000).
    pub max_tool_result_bytes: usize,
    /// LLM temperature (default: 0.2).
    pub temperature: f32,
    /// Effort level for LLM requests (default: None, letting the provider decide).
    pub effort: Option<EffortLevel>,
    /// Maximum output tokens for LLM requests (default: 32768).
    /// Should be set per-model based on provider capabilities.
    pub max_output_tokens: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_turns: 25,
            timeout_secs: 900,
            max_tool_result_bytes: 8_000,
            temperature: 0.2,
            effort: None,
            max_output_tokens: 32_768,
        }
    }
}

impl AgentConfig {
    pub fn from_env() -> Self {
        let timeout_secs = std::env::var("RUSTYCODE_AGENT_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(900);
        let effort = std::env::var("RUSTYCODE_EFFORT_OVERRIDE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .and_then(|s| s.parse::<EffortLevel>().ok());
        let max_output_tokens = std::env::var("RUSTYCODE_MAX_OUTPUT_TOKENS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(32_768);
        Self {
            timeout_secs,
            effort,
            max_output_tokens,
            ..Default::default()
        }
    }

    pub fn with_max_output_tokens(mut self, tokens: u32) -> Self {
        self.max_output_tokens = tokens;
        self
    }
}

/// Returns recommended max_output_tokens for a given model.
/// GLM reasoning models need higher budgets because reasoning tokens share the same pool.
pub fn recommended_max_tokens(model: &str) -> u32 {
    if model.starts_with("glm-5") || model.starts_with("glm-4") {
        65_536
    } else {
        32_768
    }
}

/// Why the agent loop stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoppedReason {
    /// Model stopped calling tools — task complete.
    NoToolCalls,
    /// Hard turn cap reached.
    MaxTurnsReached,
    /// Wall-clock timeout exceeded.
    TimeoutExceeded,
    /// A plugin requested early stop.
    PluginStopped,
}

/// Result of an agent run.
pub struct AgentResult {
    /// Final assistant text (may be empty if the last turn was all tool calls).
    pub final_text: String,
    /// Full conversation history for carry-forward across retries.
    pub messages: Vec<ChatMessage>,
    /// How the loop terminated.
    pub stopped_reason: StoppedReason,
    /// Cumulative token usage.
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
}

impl AgentResult {
    /// Convert this result into a protocol-level [`AgentOutcome`].
    ///
    /// Maps token usage fields directly. Success is `true` unless the loop
    /// hit a hard cap (max turns or timeout). File changes and reasoning
    /// summary are left empty here — callers that have that data fill them.
    pub fn into_outcome(
        &self,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> rustycode_protocol::agent_outcome::AgentOutcome {
        use rustycode_protocol::reasoning_summary::ReasoningSummary;
        use rustycode_protocol::token_usage::TokenUsage;

        let success = !matches!(
            self.stopped_reason,
            StoppedReason::MaxTurnsReached | StoppedReason::TimeoutExceeded
        );

        rustycode_protocol::agent_outcome::AgentOutcome {
            agent_id: agent_id.into(),
            task_id: task_id.into(),
            success,
            output_text: self.final_text.clone(),
            files_changed: vec![],
            usage: TokenUsage {
                input_tokens: self.total_input_tokens,
                output_tokens: self.total_output_tokens,
                cache_read_tokens: self.total_cache_read_tokens,
                cache_creation_tokens: self.total_cache_creation_tokens,
            },
            reasoning_summary: ReasoningSummary::empty(),
        }
    }
}

/// The interface between the agent loop and display / collection layers.
///
/// Each method is async; the agent loop awaits each call before proceeding.
/// Default implementations are provided where sensible.
#[async_trait::async_trait]
pub trait AgentEvents: Send {
    /// Receive a raw streaming event.
    async fn on_event(&mut self, _event: StreamEvent) {}

    /// Request approval for a tool call. Override to prompt user.
    async fn on_approval_needed(
        &mut self,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> ApprovalDecision {
        ApprovalDecision::AutoApproved
    }

    /// Ask the user a multiple-choice question. Override to prompt.
    async fn on_question(&mut self, _question: &str, _options: &[String]) -> Option<String> {
        None
    }

    /// Session completed (after `StreamEvent::Done`). Override to collect final metrics.
    async fn on_done(&mut self, _result: &AgentResult) {}
}

/// A running agent session. Drives behavior; knows nothing about display.
pub struct AgentSession {
    pub config: AgentConfig,
    /// Working directory for tool execution.
    pub cwd: PathBuf,
    /// Optional code intelligence — provides structural understanding.
    /// When present, the agent sees repo map, changes, and file context.
    pub intelligence: Option<Box<dyn CodeIntelligence>>,
    /// Dynamic tool gating manager.
    pub activation: ToolActivationManager,
    /// Lifecycle hook dispatcher.
    pub hooks: ExpandedHookDispatcher,
    /// Optional message sender for inter-agent communication.
    pub message_sender: Option<Arc<dyn MessageSender>>,
    /// Broadcast channel for EventMsg emission (Phase 1B dual emission).
    /// Capacity: 256 events. Subscribers that lag get Lagged notification.
    event_tx: tokio::sync::broadcast::Sender<EventMsg>,
    /// Optional provider/auth/rate-limit context.
    pub provider_context: Option<ProviderContext>,
    /// Inbound command receiver (Op).
    op_rx: Option<tokio::sync::mpsc::UnboundedReceiver<rustycode_protocol::Op>>,
    /// Optional agent plugins (observers/modifiers). Empty = zero overhead.
    plugins: Vec<Box<dyn crate::plugins::AgentPlugin>>,
}

impl AgentSession {
    pub fn new(config: AgentConfig, cwd: impl Into<PathBuf>) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self {
            config,
            cwd: cwd.into(),
            intelligence: None,
            activation: ToolActivationManager::new(),
            hooks: ExpandedHookDispatcher::new(),
            message_sender: None,
            event_tx,
            provider_context: None,
            op_rx: None,
            plugins: Vec::new(),
        }
    }

    /// Attach code intelligence to this session.
    pub fn with_intelligence(mut self, intel: Box<dyn CodeIntelligence>) -> Self {
        self.intelligence = Some(intel);
        self
    }

    /// Wire a hook dispatcher.
    pub fn with_hooks(mut self, hooks: ExpandedHookDispatcher) -> Self {
        self.hooks = hooks;
        self
    }

    /// Set the tool activation tier.
    pub fn with_tier(mut self, tier: ToolTier) -> Self {
        self.activation.promote(tier);
        self
    }

    /// Attach a message sender for inter-agent communication.
    pub fn with_message_sender(mut self, sender: Arc<dyn MessageSender>) -> Self {
        self.message_sender = Some(sender);
        self
    }

    /// Attach an inbound command receiver (Op).
    pub fn with_op_receiver(
        mut self,
        op_rx: tokio::sync::mpsc::UnboundedReceiver<rustycode_protocol::Op>,
    ) -> Self {
        self.op_rx = Some(op_rx);
        self
    }

    /// Attach an agent plugin. Plugins are called at turn boundaries.
    pub fn with_plugin(mut self, plugin: Box<dyn crate::plugins::AgentPlugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    /// Subscribe to EventMsg broadcast.
    /// Returns a receiver that gets all events emitted during agent turns.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<EventMsg> {
        self.event_tx.subscribe()
    }

    /// Get a reference to the broadcast sender (for wiring recorders, etc.)
    pub fn event_sender(&self) -> &tokio::sync::broadcast::Sender<EventMsg> {
        &self.event_tx
    }

    /// Broadcast an EventMsg to all subscribers.
    /// Silently ignores send errors (no subscribers is fine).
    #[allow(dead_code)]
    pub(crate) fn broadcast_event(&self, msg: EventMsg) {
        let _ = self.event_tx.send(msg);
    }

    /// Run the agent loop to completion.
    pub async fn run(
        &mut self,
        provider: &dyn LLMProvider,
        model: &str,
        system: &str,
        messages: Vec<ChatMessage>,
        tools_schema: &[serde_json::Value],
        tool_registry: &ToolRegistry,
        events: &mut dyn AgentEvents,
    ) -> Result<AgentResult> {
        let mut op_rx = self.op_rx.take();

        // Enrich system prompt with repo map if intelligence is available
        #[allow(clippy::option_if_let_else)]
        let enriched_system = match self.intelligence.as_ref() {
            Some(intel) => {
                let repo_map = intel.repo_map(2000);
                if repo_map.is_empty() {
                    system.to_string()
                } else {
                    format!("{system}\n\n# Codebase Structure\n\n{repo_map}")
                }
            }
            None => system.to_string(),
        };

        let result = run_loop(
            provider,
            model,
            &enriched_system,
            messages,
            tools_schema,
            &self.cwd,
            tool_registry,
            self.intelligence.as_deref(),
            &mut self.activation,
            &self.hooks,
            &self.config,
            events,
            self.message_sender.clone(),
            &self.event_tx,
            &mut op_rx,
            &mut self.plugins,
        )
        .await;

        // Restore op_rx if it was taken and session is still alive
        self.op_rx = op_rx;

        result
    }
}

// Internal loop

/// RAII guard that dispatches SessionEnd on drop if not already fired.
struct SessionEndGuard<'a> {
    hooks: &'a ExpandedHookDispatcher,
    model: &'a str,
    message_count: usize,
    fired: bool,
}

impl Drop for SessionEndGuard<'_> {
    fn drop(&mut self) {
        if !self.fired {
            let _ = self.hooks.dispatch(&LifecycleEvent::new(
                LifecycleHook::SessionEnd,
                self.model,
                serde_json::json!({ "total_turns": self.message_count / 2 }),
            ));
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_loop(
    provider: &dyn LLMProvider,
    model: &str,
    system: &str,
    messages: Vec<ChatMessage>,
    tools_schema: &[serde_json::Value],
    cwd: &Path,
    tool_registry: &ToolRegistry,
    intelligence: Option<&dyn CodeIntelligence>,
    activation: &mut ToolActivationManager,
    hooks: &ExpandedHookDispatcher,
    config: &AgentConfig,
    events: &mut dyn AgentEvents,
    message_sender: Option<Arc<dyn MessageSender>>,
    event_tx: &tokio::sync::broadcast::Sender<EventMsg>,
    op_rx: &mut Option<tokio::sync::mpsc::UnboundedReceiver<rustycode_protocol::Op>>,
    plugins: &mut [Box<dyn crate::plugins::AgentPlugin>],
) -> Result<AgentResult> {
    const MAX_RETRIES: usize = 3;

    let max_turns = if config.max_turns == 0 {
        1
    } else {
        config.max_turns
    };
    let timeout_secs = if config.timeout_secs == 0 {
        u64::MAX
    } else {
        config.timeout_secs
    };

    // Dispatch SessionStart hook
    let _ = hooks.dispatch(&LifecycleEvent::new(
        LifecycleHook::SessionStart,
        model,
        serde_json::json!({ "task_count": messages.len() }),
    ));

    let mut session_end_guard = SessionEndGuard {
        hooks,
        model,
        message_count: messages.len(),
        fired: false,
    };

    let mut messages = messages;

    let mut final_text = String::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_cache_read_tokens: u64 = 0;
    let mut total_cache_creation_tokens: u64 = 0;
    let mut stopped_reason = StoppedReason::NoToolCalls;

    let start = std::time::Instant::now();
    let chunk_timeout = Duration::from_mins(2);

    let mut plugin_ctx = crate::plugins::TurnContext {
        turn: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        cwd: cwd.to_path_buf(),
    };
    for plugin in plugins.iter_mut() {
        plugin.on_start(&plugin_ctx).await;
    }

    for turn in 0..max_turns {
        if start.elapsed().as_secs() > timeout_secs {
            tracing::info!("Agent timed out after {}s", start.elapsed().as_secs());
            stopped_reason = StoppedReason::TimeoutExceeded;
            break;
        }

        tracing::info!("AgentSession turn {}", turn + 1);

        // Phase 1B: Dual emission for turn start
        let turn_event = StreamEvent::TurnStarted { turn: turn + 1 };
        events.on_event(turn_event.clone()).await;
        if let Some(msg) = crate::event_convert::stream_event_to_event_msg(turn_event) {
            let _ = event_tx.send(msg);
        }

        // Inject structural context from intelligence on turns > 0
        if turn > 0 {
            if let Some(intel) = intelligence {
                inject_turn_context(&mut messages, intel);
            }
        }

        messages = prune_messages(messages);

        // Filter tools by activation manager
        let active_tools_schema: Vec<serde_json::Value> = tools_schema
            .iter()
            .filter(|t| {
                let name = t["name"].as_str().unwrap_or_default();
                activation.is_tool_allowed(name)
            })
            .cloned()
            .collect();

        tracing::info!(
            active = active_tools_schema.len(),
            total = tools_schema.len(),
            "AgentSession tool filter"
        );

        let mut request = CompletionRequest::new(model.to_string(), messages.clone())
            .with_streaming(true)
            .with_max_tokens(config.max_output_tokens)
            .with_temperature(config.temperature)
            .with_system_prompt(system.to_string())
            .with_tools(active_tools_schema)
            .with_tool_choice(ToolChoice::Auto);

        if let Some(effort_level) = config.effort {
            request = request
                .with_effort(effort_level)
                .with_output_config(OutputConfig {
                    effort: Some(effort_level),
                    format: None,
                });
        }

        let state = match start_turn_with_retry(provider, &mut messages, request, MAX_RETRIES)
            .await
            .map_err(|e| {
                tracing::error!("start_turn_with_retry failed: {e:#}");
                e
            })? {
            TurnSource::Stream(stream) => {
                collect_stream_turn(stream, chunk_timeout, events, event_tx, op_rx).await?
            }
            TurnSource::Completion(response) => {
                collect_completion_turn(response, events, event_tx).await?
            }
        };

        // Accumulate token counts
        total_input_tokens += state.total_input_tokens;
        total_output_tokens += state.total_output_tokens;
        total_cache_read_tokens += state.total_cache_read_tokens;
        total_cache_creation_tokens += state.total_cache_creation_tokens;

        // Handle max_tokens: inject continuation
        if state.stop_reason.as_deref() == Some("max_tokens") && turn + 1 < max_turns {
            tracing::info!("Model hit max_tokens, injecting continuation message");
            if !state.assistant_text.is_empty() {
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Simple(state.assistant_text),
                });
            }
            messages.push(ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Simple(
                    "Your response was truncated. Please continue from where you left off."
                        .to_string(),
                ),
            });
            continue;
        }

        if !state.assistant_text.is_empty() {
            final_text.clone_from(&state.assistant_text);
        }

        // No tool calls — task complete
        if state.tools.is_empty() {
            tracing::info!("Agent finished: no tool calls");
            if state.stop_reason.is_none() {
                let _ = event_tx.send(EventMsg::TurnCompleted {
                    stop_reason: "end_turn".to_string(),
                });
            }
            stopped_reason = StoppedReason::NoToolCalls;
            break;
        }

        // Build assistant message with text + tool_use blocks
        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !state.assistant_text.is_empty() {
            assistant_blocks.push(ContentBlock::text(&state.assistant_text));
        }

        let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
        for tool in &state.tools {
            let input: serde_json::Value = serde_json::from_str(&tool.input_json)
                .unwrap_or_else(|_| serde_json::json!({"_raw": tool.input_json}));

            assistant_blocks.push(ContentBlock::ToolUse {
                id: tool.id.clone(),
                name: tool.name.clone(),
                input: input.clone(),
            });

            // Signal tool execution start
            let stream_event = StreamEvent::ToolExecStarted {
                id: tool.id.clone(),
                name: tool.name.clone(),
            };
            events.on_event(stream_event.clone()).await;
            // Phase 1B: Dual emission — broadcast EventMsg alongside callback
            if let Some(msg) = crate::event_convert::stream_event_to_event_msg(stream_event) {
                let _ = event_tx.send(msg);
            }

            // Final safety check: is tool allowed?
            if !activation.is_tool_allowed(&tool.name) {
                let msg = format!(
                    "Tool '{}' is not allowed in current tier ({:?})",
                    tool.name,
                    activation.current_tier()
                );
                let exec_msg = EventMsg::ToolExecCompleted {
                    tool_id: tool.id.clone(),
                    tool_name: tool.name.clone(),
                    success: false,
                    output: msg.clone(),
                    output_size: msg.len(),
                    duration_ms: 0,
                    exit_code: None,
                };
                let _ = event_tx.send(exec_msg);

                let stream_event = StreamEvent::ToolExecCompleted {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    output: msg.clone(),
                    is_error: true,
                };
                events.on_event(stream_event).await;
                tool_result_blocks.push(ContentBlock::tool_error(&tool.id, &msg));
                continue;
            }

            // Dispatch PreToolUse hook
            let _ = hooks.dispatch(&LifecycleEvent::new(
                LifecycleHook::PreToolUse,
                &tool.name,
                input.clone(),
            ));

            // Phase 1B: Dual emission for approval request
            let op_class = if rustycode_protocol::permission_modes::is_read_only_tool(&tool.name) {
                rustycode_protocol::permission_modes::OperationClass::ReadOnly
            } else if tool.name == rustycode_protocol::tool_names::WRITE
                || tool.name == rustycode_protocol::tool_names::EDIT
            {
                rustycode_protocol::permission_modes::OperationClass::Write
            } else {
                rustycode_protocol::permission_modes::OperationClass::Unknown
            };

            // Phase 1D: Approval gate (prefers Op channel, falls back to legacy callback)
            let _ = event_tx.send(EventMsg::ApprovalRequired {
                tool_name: tool.name.clone(),
                tool_id: tool.id.clone(),
                operation_class: op_class,
                description: format!("Execute tool: {}", tool.name),
                diff: None,
            });
            let decision = if let Some(ref mut rx) = op_rx {
                loop {
                    match rx.recv().await {
                        Some(rustycode_protocol::Op::ApproveTool {
                            tool_id, approved, ..
                        }) if tool_id == tool.id => {
                            if approved {
                                break ApprovalDecision::Approve;
                            }
                            break ApprovalDecision::Reject("rejected by user".to_string());
                        }
                        Some(rustycode_protocol::Op::StopStream) => {
                            tracing::info!("Stream cancelled while awaiting approval");
                            break ApprovalDecision::Reject("cancelled".to_string());
                        }
                        Some(_) => {
                            // Other ops ignored while awaiting approval
                        }
                        None => {
                            tracing::warn!("Op channel closed while awaiting approval");
                            break ApprovalDecision::Reject("channel closed".to_string());
                        }
                    }
                }
            } else {
                events.on_approval_needed(&tool.name, &input).await
            };
            if let ApprovalDecision::Reject(reason) = decision {
                // Phase 1B: Broadcast rejection
                let _ = event_tx.send(EventMsg::ApprovalRejected {
                    tool_id: tool.id.clone(),
                });

                let msg = format!("Tool call rejected: {reason}");
                let exec_msg = EventMsg::ToolExecCompleted {
                    tool_id: tool.id.clone(),
                    tool_name: tool.name.clone(),
                    success: false,
                    output: msg.clone(),
                    output_size: msg.len(),
                    duration_ms: 0,
                    exit_code: None,
                };
                let _ = event_tx.send(exec_msg);

                let stream_event = StreamEvent::ToolExecCompleted {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    output: msg.clone(),
                    is_error: true,
                };
                events.on_event(stream_event).await;
                tool_result_blocks.push(ContentBlock::tool_error(&tool.id, &msg));

                // Dispatch ToolError hook
                let _ = hooks.dispatch(&LifecycleEvent::new(
                    LifecycleHook::ToolError,
                    &tool.name,
                    serde_json::json!({ "error": "rejected", "reason": reason }),
                ));
                continue;
            }

            // Phase 1B: Broadcast approval
            let _ = event_tx.send(EventMsg::ApprovalApproved {
                tool_id: tool.id.clone(),
            });

            // Execute tool with optional message sender for inter-agent communication
            let result = execute_tool(
                cwd,
                &tool.name,
                &tool.input_json,
                tool_registry,
                message_sender.clone(),
            );
            let truncated = truncate_tool_output(&result.output, config.max_tool_result_bytes);
            let error_flag = !result.success;
            let mut plugin_output = truncated.clone();

            for plugin in plugins.iter_mut() {
                plugin
                    .on_tool_result(&tool.name, &tool.id, &input, &mut plugin_output)
                    .await;
            }

            activation.record_use(&tool.name, !error_flag);

            let exec_msg = EventMsg::ToolExecCompleted {
                tool_id: tool.id.clone(),
                tool_name: tool.name.clone(),
                success: result.success,
                output: plugin_output.clone(),
                output_size: plugin_output.len(),
                duration_ms: 0,
                exit_code: result.exit_code,
            };
            let _ = event_tx.send(exec_msg);

            let stream_event = StreamEvent::ToolExecCompleted {
                id: tool.id.clone(),
                name: tool.name.clone(),
                output: plugin_output.clone(),
                is_error: error_flag,
            };
            events.on_event(stream_event).await;

            if error_flag {
                tool_result_blocks.push(ContentBlock::tool_error(&tool.id, &plugin_output));
                let _ = hooks.dispatch(&LifecycleEvent::new(
                    LifecycleHook::ToolError,
                    &tool.name,
                    serde_json::json!({
                        "error": "execution_failed",
                        "exit_code": result.exit_code,
                    }),
                ));
            } else {
                tool_result_blocks.push(ContentBlock::tool_result(&tool.id, &plugin_output));
                let _ = hooks.dispatch(&LifecycleEvent::new(
                    LifecycleHook::PostToolUse,
                    &tool.name,
                    serde_json::json!({
                        "output_len": plugin_output.len(),
                        "exit_code": result.exit_code,
                    }),
                ));
            }
        }

        messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(assistant_blocks),
        });
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(tool_result_blocks),
        });

        // Phase 1B: Ensure TurnCompleted is emitted if not already sent by provider
        if state.stop_reason.is_none() {
            let _ = event_tx.send(EventMsg::TurnCompleted {
                stop_reason: "end_turn".to_string(),
            });
        }

        if state.stop_reason.as_deref() == Some("end_turn") || state.stop_reason.is_none() {
            tracing::info!("Agent finished: end_turn");
            stopped_reason = StoppedReason::NoToolCalls;
            break;
        }

        plugin_ctx.turn = turn + 1;
        plugin_ctx.total_input_tokens = total_input_tokens;
        plugin_ctx.total_output_tokens = total_output_tokens;
        let mut plugin_wants_stop = false;
        for plugin in plugins.iter_mut() {
            if plugin.should_stop(&plugin_ctx).await {
                plugin_wants_stop = true;
            }
        }
        if plugin_wants_stop {
            tracing::info!("Plugin requested early stop after turn {}", turn + 1);
            stopped_reason = StoppedReason::PluginStopped;
            break;
        }
    }

    if stopped_reason != StoppedReason::NoToolCalls
        && stopped_reason != StoppedReason::TimeoutExceeded
        && stopped_reason != StoppedReason::PluginStopped
    {
        stopped_reason = StoppedReason::MaxTurnsReached;
    }

    plugin_ctx.turn = max_turns;
    for plugin in plugins.iter_mut() {
        plugin.on_done(&plugin_ctx).await;
    }

    let result = AgentResult {
        final_text,
        messages,
        stopped_reason,
        total_input_tokens,
        total_output_tokens,
        total_cache_read_tokens,
        total_cache_creation_tokens,
    };

    // Dispatch SessionEnd hook (guard will also dispatch on drop if we error before here)
    session_end_guard.fired = true;
    let _ = hooks.dispatch(&LifecycleEvent::new(
        LifecycleHook::SessionEnd,
        model,
        serde_json::json!({ "total_turns": result.messages.len() / 2 }),
    ));

    let done_event = StreamEvent::Done;
    events.on_event(done_event.clone()).await;
    // Phase 1B: Dual emission — broadcast EventMsg alongside callback
    if let Some(msg) = crate::event_convert::stream_event_to_event_msg(done_event) {
        let _ = event_tx.send(msg);
    }
    events.on_done(&result).await;
    Ok(result)
}

/// Inject structural context from `CodeIntelligence` into messages.
///
/// After each turn, the agent sees what changed and what the impact is.
/// This replaces heuristic nudges — the model sees reality and decides.
fn inject_turn_context(messages: &mut Vec<ChatMessage>, intel: &dyn CodeIntelligence) {
    let changes = intel.changes();
    if changes.is_empty() {
        return;
    }

    let mut parts = Vec::new();

    // Summarize file changes
    let modified: Vec<&str> = changes
        .iter()
        .filter(|c| c.change_type == crate::intelligence::ChangeType::Modified)
        .filter_map(|c| c.path.to_str())
        .collect();
    let created: Vec<&str> = changes
        .iter()
        .filter(|c| c.change_type == crate::intelligence::ChangeType::Created)
        .filter_map(|c| c.path.to_str())
        .collect();

    if !modified.is_empty() {
        parts.push(format!("Files modified: {}", modified.join(", ")));
    }
    if !created.is_empty() {
        parts.push(format!("Files created: {}", created.join(", ")));
    }

    // For modified files, get their outlines so the model sees current state
    for change in &changes {
        if change.change_type == crate::intelligence::ChangeType::Modified {
            if let Some(outline) = intel.file_outline(&change.path) {
                if !outline.is_empty() {
                    let name = change
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default();
                    parts.push(format!("{name} outline:\n{outline}"));
                }
            }

            // Show what depends on this file so the model understands impact
            if let Some(path_str) = change.path.to_str() {
                let deps = intel.get_dependents(path_str);
                if !deps.is_empty() {
                    let dep_names: Vec<&str> =
                        deps.iter().map(|d| d.name.as_str()).take(8).collect();
                    parts.push(format!(
                        "Dependents of {}: {}",
                        change
                            .path
                            .file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default(),
                        dep_names.join(", ")
                    ));
                }
            }
        }
    }

    if !parts.is_empty() {
        let context_msg = format!("[Code context]\n{}", parts.join("\n"));
        // If last message is already User (e.g., tool results), merge to avoid
        // consecutive User messages which the Anthropic API rejects.
        if let Some(last) = messages.last_mut() {
            if last.role == MessageRole::User {
                match &mut last.content {
                    MessageContent::Simple(existing) => {
                        *existing = format!("{existing}\n\n{context_msg}");
                    }
                    MessageContent::Blocks(blocks) => {
                        blocks.push(ContentBlock::text(&context_msg));
                    }
                    _ => {
                        messages.push(ChatMessage {
                            role: MessageRole::User,
                            content: MessageContent::Simple(context_msg),
                        });
                    }
                }
                return;
            }
        }
        messages.push(ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(context_msg),
        });
    }
}

enum TurnSource {
    Stream(Pin<Box<dyn Stream<Item = StreamChunk> + Send>>),
    Completion(CompletionResponse),
}

/// Start a turn with retry on transient errors and context-length recovery.
/// Prefers streaming, but falls back to non-streaming completion if streaming
/// is not available for the provider or model.
async fn start_turn_with_retry(
    provider: &dyn LLMProvider,
    messages: &mut Vec<ChatMessage>,
    request: CompletionRequest,
    max_retries: usize,
) -> Result<TurnSource> {
    if !provider.supports_streaming() {
        tracing::info!("Provider reports no streaming support; using non-streaming completion");
        let response =
            start_completion_with_retry(provider, messages, request, max_retries).await?;
        return Ok(TurnSource::Completion(response));
    }

    match start_stream_with_retry(provider, messages, request.clone(), max_retries).await {
        Ok(stream) => Ok(TurnSource::Stream(stream)),
        Err(err)
            if matches!(
                classify_error(&err.to_string()),
                ErrorClass::StreamingUnsupported
            ) =>
        {
            tracing::info!("Streaming is unavailable, falling back to non-streaming completion");
            let response =
                start_completion_with_retry(provider, messages, request, max_retries).await?;
            Ok(TurnSource::Completion(response))
        }
        Err(err) => Err(err),
    }
}

/// Start an LLM stream with retry on transient errors and context-length recovery.
async fn start_stream_with_retry(
    provider: &dyn LLMProvider,
    messages: &mut Vec<ChatMessage>,
    request: CompletionRequest,
    max_retries: usize,
) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>> {
    let mut final_error: Option<anyhow::Error> = None;

    for attempt in 0..=max_retries {
        let req = rebuild_request(&request, messages, true);

        match provider.complete_stream(req).await {
            Ok(s) => {
                if attempt > 0 {
                    tracing::info!("Stream started on retry attempt {attempt}");
                }
                return Ok(s);
            }
            Err(e) => {
                let err_str = format!("{e}");
                match classify_error(&err_str) {
                    ErrorClass::StreamingUnsupported => {
                        return Err(anyhow::anyhow!("{e}"));
                    }
                    ErrorClass::ContextLength if trim_context(messages) => {
                        continue;
                    }
                    ErrorClass::Transient if attempt < max_retries => {
                        let is_rate_limit = err_str.to_lowercase().contains("rate limit");
                        let delay_ms = if is_rate_limit {
                            let base = 5000u64;
                            base.saturating_mul(1u64 << attempt).min(60_000)
                        } else {
                            1000u64 * (1 << attempt)
                        };
                        tracing::warn!(
                            "Transient stream error (attempt {}/{}): {e}. Retrying in {delay_ms}ms",
                            attempt + 1,
                            max_retries + 1,
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                    _ => {}
                }
                final_error = Some(anyhow::anyhow!("{e}"));
            }
        }
    }

    Err(final_error.unwrap_or_else(|| anyhow::anyhow!("Stream initialization failed")))
}

/// Start a non-streaming completion with retry on transient errors and context-length recovery.
async fn start_completion_with_retry(
    provider: &dyn LLMProvider,
    messages: &mut Vec<ChatMessage>,
    request: CompletionRequest,
    max_retries: usize,
) -> Result<CompletionResponse> {
    let mut final_error: Option<anyhow::Error> = None;

    for attempt in 0..=max_retries {
        let req = rebuild_request(&request, messages, false);

        match provider.complete(req).await {
            Ok(response) => {
                if attempt > 0 {
                    tracing::info!("Completion started on retry attempt {attempt}");
                }
                return Ok(response);
            }
            Err(e) => {
                let err_str = format!("{e}");
                match classify_error(&err_str) {
                    ErrorClass::ContextLength if trim_context(messages) => {
                        continue;
                    }
                    ErrorClass::Transient if attempt < max_retries => {
                        let delay_ms = 1000u64 * (1 << attempt);
                        tracing::warn!(
                            "Transient completion error (attempt {}/{}): {e}. Retrying in {delay_ms}ms",
                            attempt + 1,
                            max_retries + 1,
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                    _ => {}
                }
                final_error = Some(anyhow::anyhow!("{e}"));
            }
        }
    }

    Err(final_error.unwrap_or_else(|| anyhow::anyhow!("Completion initialization failed")))
}

/// Rebuild a `CompletionRequest` with current messages and settings from the original request.
fn rebuild_request(
    original: &CompletionRequest,
    messages: &[ChatMessage],
    streaming: bool,
) -> CompletionRequest {
    let mut request = CompletionRequest::new(original.model.clone(), messages.to_vec())
        .with_streaming(streaming)
        .with_max_tokens(original.max_tokens.unwrap_or(32_768))
        .with_temperature(original.temperature.unwrap_or(0.2))
        .with_system_prompt(original.system_prompt.clone().unwrap_or_default())
        .with_tools(original.tools.clone().unwrap_or_default())
        .with_tool_choice(ToolChoice::Auto);

    if let Some(ref output_config) = original.output_config {
        if let Some(effort_level) = output_config.effort {
            request = request
                .with_effort(effort_level)
                .with_output_config(OutputConfig {
                    effort: Some(effort_level),
                    format: None,
                });
        }
    }

    request
}

/// Classify an error string for retry logic.
enum ErrorClass {
    /// Transient (429, 5xx, timeout, connection) — retry with backoff.
    Transient,
    /// Context length exceeded — aggressively trim messages and retry immediately.
    ContextLength,
    /// Streaming not supported — fall back to non-streaming.
    StreamingUnsupported,
    /// Non-retryable — propagate.
    Fatal,
}

fn classify_error(err: &str) -> ErrorClass {
    let lower = err.to_lowercase();

    if lower.contains("stream")
        && (lower.contains("completion-only")
            || lower.contains("streaming-only")
            || lower.contains("no stream")
            || lower.contains("streaming not supported")
            || lower.contains("streaming unavailable")
            || lower.contains("streaming disabled")
            || lower.contains("streaming not available")
            || lower.contains("unsupported"))
        || lower.contains("completion-only")
    {
        return ErrorClass::StreamingUnsupported;
    }

    if lower.contains("context_length")
        || lower.contains("too many tokens")
        || lower.contains("input too long")
        || lower.contains("maximum context")
        || lower.contains("token limit")
        || lower.contains("reduce the length")
    {
        return ErrorClass::ContextLength;
    }

    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("rate_limit")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("500")
        || lower.contains("timeout")
        || lower.contains("connection")
    {
        return ErrorClass::Transient;
    }

    ErrorClass::Fatal
}

/// Trim conversation history when context length is exceeded.
/// Keeps the first message + a notice + the most recent N messages.
fn extract_user_task(messages: &[ChatMessage]) -> Option<String> {
    messages
        .iter()
        .find(|m| m.role == MessageRole::User)
        .and_then(|m| match &m.content {
            MessageContent::Simple(t) => Some(t.clone()),
            MessageContent::Blocks(blocks) => blocks.iter().find_map(|b| match b {
                ContentBlock::Text { text, .. } => Some(text.clone()),
                _ => None,
            }),
            _ => None,
        })
}

fn trim_context(messages: &mut Vec<ChatMessage>) -> bool {
    const MIN_MESSAGES: usize = 8;
    const KEEP_RECENT: usize = 6;

    if messages.len() <= MIN_MESSAGES {
        return false;
    }

    let total = messages.len();
    let trim_from = total - KEEP_RECENT;

    let original_task = extract_user_task(messages);

    let mut trimmed = Vec::with_capacity(KEEP_RECENT + 2);
    trimmed.push(messages[0].clone());

    let trim_notice = match original_task {
        Some(ref task) if !task.is_empty() => {
            let task_preview: String = task.chars().take(300).collect();
            format!(
                "[Context trimmed due to length. Original task: {task_preview}\nContinue from current state.]"
            )
        }
        _ => "[Context trimmed due to length. Continue from current state.]".to_string(),
    };
    trimmed.push(ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Simple(trim_notice),
    });
    for msg in messages.iter().skip(trim_from) {
        trimmed.push(msg.clone());
    }
    tracing::warn!(
        "Context length exceeded, aggressive trim: {total} → {} messages",
        trimmed.len()
    );
    *messages = trimmed;
    true
}

// Tests

/// An [`AgentEvents`] implementation that does nothing.
/// Useful for headless execution or testing where callbacks aren't needed.
#[allow(dead_code)]
pub struct NoOpEvents;

#[async_trait::async_trait]
impl AgentEvents for NoOpEvents {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_agent_config_defaults() {
        let config = AgentConfig::default();
        assert_eq!(config.max_turns, 25);
        assert_eq!(config.timeout_secs, 900);
        assert_eq!(config.max_tool_result_bytes, 8_000);
    }

    #[test]
    fn test_stopped_reason_equality() {
        assert_eq!(StoppedReason::NoToolCalls, StoppedReason::NoToolCalls);
        assert_ne!(StoppedReason::NoToolCalls, StoppedReason::MaxTurnsReached);
    }

    #[tokio::test]
    async fn test_approval_decision_auto_approved() {
        struct DefaultEvents;
        #[async_trait::async_trait]
        impl AgentEvents for DefaultEvents {
            async fn on_event(&mut self, _: StreamEvent) {}
            async fn on_done(&mut self, _: &AgentResult) {}
        }

        let mut e = DefaultEvents;
        let decision = e.on_approval_needed("Bash", &json!({})).await;
        assert!(matches!(decision, ApprovalDecision::AutoApproved));
    }

    #[tokio::test]
    async fn test_stream_event_collection() {
        struct Collector {
            events: Vec<StreamEvent>,
        }
        impl Collector {
            fn new() -> Self {
                Self { events: Vec::new() }
            }
        }
        #[async_trait::async_trait]
        impl AgentEvents for Collector {
            async fn on_event(&mut self, ev: StreamEvent) {
                self.events.push(ev);
            }
            async fn on_done(&mut self, _: &AgentResult) {}
        }

        let mut collector = Collector::new();
        collector
            .on_event(StreamEvent::TextDelta {
                content: "hi".into(),
            })
            .await;
        collector
            .on_event(StreamEvent::ToolExecStarted {
                id: "1".into(),
                name: "Bash".into(),
            })
            .await;
        collector.on_event(StreamEvent::Done).await;

        assert_eq!(collector.events.len(), 3);
    }

    #[test]
    fn classify_error_streaming_unsupported() {
        assert!(matches!(
            classify_error("streaming not supported for this model"),
            ErrorClass::StreamingUnsupported
        ));
        assert!(matches!(
            classify_error("completion-only mode"),
            ErrorClass::StreamingUnsupported
        ));
    }

    #[test]
    fn classify_error_context_length() {
        assert!(matches!(
            classify_error("context_length_exceeded"),
            ErrorClass::ContextLength
        ));
        assert!(matches!(
            classify_error("too many tokens in input"),
            ErrorClass::ContextLength
        ));
        assert!(matches!(
            classify_error("Please reduce the length of your prompt"),
            ErrorClass::ContextLength
        ));
    }

    #[test]
    fn classify_error_transient() {
        assert!(matches!(
            classify_error("Error 429 rate limited"),
            ErrorClass::Transient
        ));
        assert!(matches!(
            classify_error("503 service unavailable"),
            ErrorClass::Transient
        ));
        assert!(matches!(
            classify_error("connection timed out"),
            ErrorClass::Transient
        ));
    }

    #[test]
    fn classify_error_fatal() {
        assert!(matches!(
            classify_error("invalid API key"),
            ErrorClass::Fatal
        ));
        assert!(matches!(
            classify_error("model not found"),
            ErrorClass::Fatal
        ));
    }

    #[test]
    fn trim_context_skips_short_history() {
        let messages: Vec<ChatMessage> = (0..6)
            .map(|i| ChatMessage {
                role: if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                content: MessageContent::Simple(format!("msg {i}")),
            })
            .collect();
        let mut msgs = messages;
        assert!(!trim_context(&mut msgs));
        assert_eq!(msgs.len(), 6);
    }

    #[test]
    fn trim_context_trims_long_history() {
        let messages: Vec<ChatMessage> = (0..20)
            .map(|i| ChatMessage {
                role: if i % 2 == 0 {
                    MessageRole::User
                } else {
                    MessageRole::Assistant
                },
                content: MessageContent::Simple(format!("msg {i}")),
            })
            .collect();
        let mut msgs = messages;
        assert!(trim_context(&mut msgs));
        // first msg + trim notice + 6 recent = 8
        assert_eq!(msgs.len(), 8);
        assert!(matches!(&msgs[0].content, MessageContent::Simple(s) if s == "msg 0"));
        assert!(matches!(&msgs[1].content, MessageContent::Simple(s) if s.contains("trimmed")));
        assert!(
            matches!(&msgs[1].content, MessageContent::Simple(s) if s.contains("Original task"))
        );
        // Last 6 should be messages 14..20
        assert!(matches!(&msgs[2].content, MessageContent::Simple(s) if s == "msg 14"));
        assert!(matches!(&msgs[7].content, MessageContent::Simple(s) if s == "msg 19"));
    }

    #[tokio::test]
    #[allow(clippy::expect_used)]
    async fn test_event_msg_emission() {
        use crate::AgentConfig;
        use rustycode_llm::mock::MockProvider;
        use rustycode_protocol::EventMsg;
        use std::path::PathBuf;

        let mut session = AgentSession::new(AgentConfig::default(), PathBuf::from("/tmp"));
        let mut rx = session.subscribe();

        let provider = MockProvider::from_text("hello world");
        let tool_registry = rustycode_tools::ToolRegistry::new();
        let mut events = NoOpEvents;

        let _ = session
            .run(
                &provider,
                "model",
                "system",
                vec![],
                &[],
                &tool_registry,
                &mut events,
            )
            .await
            .expect("session run should succeed");

        // Check for expected events
        let mut event_types = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            println!("Test received event: {:?}", msg);
            match msg {
                EventMsg::TurnStarted { .. } => event_types.push("TurnStarted"),
                EventMsg::TextDelta { .. } => event_types.push("TextDelta"),
                EventMsg::TurnCompleted { .. } => event_types.push("TurnCompleted"),
                EventMsg::Done => event_types.push("Done"),
                _ => {}
            }
        }

        assert!(event_types.contains(&"TurnStarted"));
        assert!(event_types.contains(&"TextDelta"));
        assert!(event_types.contains(&"TurnCompleted"));
        assert!(event_types.contains(&"Done"));
    }
}
