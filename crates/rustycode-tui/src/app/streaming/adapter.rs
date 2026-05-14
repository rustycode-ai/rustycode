use crate::app::async_::{QuestionOption, StreamChunk, StreamError};
use async_trait::async_trait;
use rustycode_agent_runtime::{AgentEvents, AgentResult, ApprovalDecision};
use rustycode_core::streaming::ToolCall;
use rustycode_orchestration::bus::OrchestrationEvent;
use rustycode_protocol::permission_modes::PermissionMode;
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_protocol::tool_names as tn;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

pub struct StreamEventAdapter {
    stream_tx: SyncSender<StreamChunk>,
    approval_rx: Option<Receiver<(String, bool)>>,
    question_rx: Option<Receiver<String>>,
    active_tools: HashMap<String, ToolCall>,
    pending_tool_id: Option<String>,
    permission_mode: PermissionMode,
}

impl StreamEventAdapter {
    pub fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            stream_tx,
            approval_rx: None,
            question_rx: None,
            active_tools: HashMap::new(),
            pending_tool_id: None,
            permission_mode: PermissionMode::Default,
        }
    }

    pub fn with_approval_rx(mut self, rx: Receiver<(String, bool)>) -> Self {
        self.approval_rx = Some(rx);
        self
    }

    pub fn with_question_rx(mut self, rx: Receiver<String>) -> Self {
        self.question_rx = Some(rx);
        self
    }

    pub fn with_permission_mode(mut self, mode: PermissionMode) -> Self {
        self.permission_mode = mode;
        self
    }

    pub fn emit(&self, chunk: StreamChunk) {
        // Blocking send: never drop chunks (fixes missing final text after tool calls)
        if let Err(_e) = self.stream_tx.send(chunk) {
            tracing::debug!("Stream channel closed during send");
        }
    }

    pub fn on_event_msg(&mut self, event: rustycode_protocol::EventMsg) {
        match event {
            rustycode_protocol::EventMsg::TextDelta { delta } => {
                self.emit(StreamChunk::Text(delta));
            }
            rustycode_protocol::EventMsg::ThinkingDelta { delta } => {
                self.emit(StreamChunk::Thinking(delta));
            }
            rustycode_protocol::EventMsg::TurnStarted { turn, .. } => {
                self.emit(StreamChunk::SystemMessage(format!("Turn {turn} started")));
            }
            rustycode_protocol::EventMsg::ToolCallStarted {
                tool_id, tool_name, ..
            } => {
                self.pending_tool_id = Some(tool_id.clone());
                self.active_tools.insert(
                    tool_id.clone(),
                    ToolCall::new(tool_id, tool_name, String::new()),
                );
            }
            rustycode_protocol::EventMsg::ToolInputDelta { tool_id, delta } => {
                if let Some(tool) = self.active_tools.get_mut(&tool_id) {
                    tool.push_json(&delta);
                }
            }
            rustycode_protocol::EventMsg::ToolExecStarted {
                tool_id,
                tool_name: _,
            } => {
                if let Some(tool) = self.active_tools.get(&tool_id) {
                    self.emit(StreamChunk::ToolStart {
                        tool_name: tool.name.clone(),
                        tool_id,
                        input_json: tool.partial_json.clone(),
                    });
                }
            }
            rustycode_protocol::EventMsg::ToolExecCompleted {
                tool_id,
                tool_name,
                success,
                output,
                ..
            } => {
                let duration_ms = self
                    .active_tools
                    .remove(&tool_id)
                    .map(|t| t.elapsed_ms())
                    .unwrap_or(0);
                self.emit(StreamChunk::ToolComplete {
                    tool_name,
                    tool_id,
                    duration_ms,
                    success,
                    output_size: output.len(),
                    output: Some(output),
                });
            }
            rustycode_protocol::EventMsg::TokenUsage {
                input_tokens,
                output_tokens,
                ..
            } => {
                self.emit(StreamChunk::TokenUsage {
                    input_tokens: input_tokens as usize,
                    output_tokens: output_tokens as usize,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                });
            }
            rustycode_protocol::EventMsg::Done => {
                self.active_tools.clear();
                self.emit(StreamChunk::Done);
            }
            rustycode_protocol::EventMsg::ApprovalRequired {
                tool_name,
                tool_id,
                description,
                ..
            } => {
                self.emit(StreamChunk::ApprovalRequest {
                    tool_name,
                    tool_id,
                    description,
                    diff: None, // could be added to EventMsg
                });
            }
            rustycode_protocol::EventMsg::ApprovalRejected { tool_id } => {
                self.emit(StreamChunk::ApprovalRejected { tool_id });
            }
            _ => {
                // Ignore other events for now
            }
        }
    }

    pub fn on_orchestration_event(&mut self, event: OrchestrationEvent) {
        match event {
            OrchestrationEvent::TextDelta { content, .. }
            | OrchestrationEvent::StreamDelta { content, .. } => {
                self.emit(StreamChunk::Text(content));
            }
            OrchestrationEvent::ThinkingDelta { content, .. } => {
                self.emit(StreamChunk::Thinking(content));
            }
            OrchestrationEvent::ToolCallStarted {
                tool_name,
                tool_id,
                input_preview,
                ..
            } => {
                let call = ToolCall::new(tool_id.clone(), tool_name.clone(), input_preview);
                self.emit(StreamChunk::ToolStart {
                    tool_name,
                    tool_id: tool_id.clone(),
                    input_json: call.partial_json.clone(),
                });
                self.active_tools.insert(tool_id, call);
            }
            OrchestrationEvent::ToolCallCompleted {
                tool_name,
                tool_id,
                success,
                output_preview,
                ..
            } => {
                let duration_ms = self
                    .active_tools
                    .remove(&tool_id)
                    .map(|t| t.elapsed_ms())
                    .unwrap_or(0);
                self.emit(StreamChunk::ToolComplete {
                    tool_name,
                    tool_id,
                    duration_ms,
                    success,
                    output_size: output_preview.len(),
                    output: Some(output_preview),
                });
            }
            OrchestrationEvent::ToolExecutionStarted {
                task_id,
                tool,
                args,
            } => {
                let tool_id = format!("exec-{}-{}", task_id, tool);
                let call = ToolCall::new(tool_id.clone(), tool.clone(), args);
                self.emit(StreamChunk::ToolStart {
                    tool_name: tool.clone(),
                    input_json: call.partial_json.clone(),
                    tool_id: tool_id.clone(),
                });
                self.active_tools.insert(tool_id, call);
            }
            OrchestrationEvent::ToolExecutionFinished {
                task_id,
                tool,
                result,
            } => {
                let tool_id = format!("exec-{}-{}", task_id, tool);
                let duration_ms = self
                    .active_tools
                    .remove(&tool_id)
                    .map(|t| t.elapsed_ms())
                    .unwrap_or(0);
                self.emit(StreamChunk::ToolComplete {
                    tool_name: tool,
                    tool_id,
                    duration_ms,
                    success: true,
                    output_size: result.len(),
                    output: Some(result),
                });
            }
            OrchestrationEvent::MilestoneProgress {
                milestone_id,
                milestone_title,
                status,
                plans_total,
                plans_completed,
                current_plan_summary,
                action_hint,
                plan_rows,
                ..
            } => {
                self.emit(StreamChunk::MilestoneProgress {
                    milestone_id: milestone_id.to_string(),
                    milestone_title,
                    status,
                    plans_total,
                    plans_completed,
                    current_plan_summary,
                    action_hint,
                    plan_rows,
                });
            }
            // Intentionally silenced: internal orchestration events, not user-facing.
            // Logged at debug level; only errors surface to UI.
            OrchestrationEvent::PhaseTransition { to, reason, .. } => {
                tracing::debug!(phase = ?to, reason = %reason, "orchestration phase transition");
            }
            OrchestrationEvent::TaskCompleted {
                tier_used,
                cost_usd,
                ..
            } => {
                tracing::debug!(
                    tier = tier_used,
                    cost_usd = cost_usd,
                    "orchestration task completed"
                );
            }
            OrchestrationEvent::EscalationSignal {
                from_tier,
                to_tier,
                reason,
                ..
            } => {
                tracing::debug!(from = from_tier, to = to_tier, reason = %reason, "tier escalation");
            }
            OrchestrationEvent::TierHandoff {
                from_tier,
                to_tier,
                package_size_bytes,
                ..
            } => {
                tracing::debug!(
                    from = from_tier,
                    to = to_tier,
                    bytes = package_size_bytes,
                    "tier handoff"
                );
            }
            OrchestrationEvent::ForkStarted {
                fork_id,
                fork_count,
                ..
            } => {
                tracing::debug!(fork_id = %fork_id, branches = fork_count, "fork started");
            }
            OrchestrationEvent::ForkCompleted {
                fork_id,
                success,
                duration_ms,
                ..
            } => {
                tracing::debug!(fork_id = %fork_id, success, duration_ms, "fork completed");
            }
            OrchestrationEvent::ContextBudgetExceeded {
                tier, used, limit, ..
            } => {
                tracing::warn!(tier, used, limit, "context budget exceeded");
                self.emit(StreamChunk::Error(StreamError::ContextBudgetExceeded));
            }
            OrchestrationEvent::EnsembleStarted {
                strategy,
                participant_count,
                ..
            } => {
                tracing::debug!(strategy = %strategy, participants = participant_count, "ensemble started");
            }
            OrchestrationEvent::EnsembleCompleted {
                confidence,
                steps_produced,
                ..
            } => {
                tracing::debug!(
                    confidence = confidence,
                    steps = steps_produced,
                    "ensemble completed"
                );
            }
            OrchestrationEvent::PartialResult { step_id, content } => {
                tracing::debug!(step_id = %step_id, len = content.len(), "partial result");
            }
            OrchestrationEvent::StepFailed { signal, .. } => {
                tracing::warn!(message = %signal.message, "orchestration step failed");
                self.emit(StreamChunk::Error(StreamError::OrchestrationStepFailed {
                    message: signal.message.clone(),
                }));
            }
            _ => {}
        }
    }
}

#[async_trait]
impl AgentEvents for StreamEventAdapter {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::TextDelta { content } => {
                self.emit(StreamChunk::Text(content));
            }
            StreamEvent::ThinkingDelta { content } => {
                self.emit(StreamChunk::Thinking(content));
            }
            StreamEvent::ToolCallStarted { id, name } => {
                self.pending_tool_id = Some(id.clone());
                self.active_tools
                    .insert(id.clone(), ToolCall::new(id, name, String::new()));
            }
            StreamEvent::ToolInputDelta { id, chunk } => {
                if let Some(tool) = self.active_tools.get_mut(&id) {
                    tool.push_json(&chunk);
                }
            }
            StreamEvent::ToolExecStarted { id, name: _ } => {
                if let Some(tool) = self.active_tools.get(&id) {
                    self.emit(StreamChunk::ToolStart {
                        tool_name: tool.name.clone(),
                        tool_id: id,
                        input_json: tool.partial_json.clone(),
                    });
                }
            }
            StreamEvent::ToolExecCompleted {
                id,
                name,
                output,
                is_error,
            } => {
                let duration_ms = self
                    .active_tools
                    .remove(&id)
                    .map(|t| t.elapsed_ms())
                    .unwrap_or(0);
                self.emit(StreamChunk::ToolComplete {
                    tool_name: name,
                    tool_id: id,
                    duration_ms,
                    success: !is_error,
                    output_size: output.len(),
                    output: Some(output),
                });
            }
            StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
            } => {
                self.emit(StreamChunk::TokenUsage {
                    input_tokens: input_tokens as usize,
                    output_tokens: output_tokens as usize,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                });
            }
            StreamEvent::CacheUsage {
                cache_read_tokens,
                cache_creation_tokens,
            } => {
                self.emit(StreamChunk::CacheUsage {
                    cache_read_tokens: cache_read_tokens as usize,
                    cache_creation_tokens: cache_creation_tokens as usize,
                });
            }
            StreamEvent::TurnStarted { turn } => {
                self.emit(StreamChunk::SystemMessage(format!("Turn {turn} started")));
            }
            StreamEvent::Done => {
                self.emit(StreamChunk::Done);
            }
            _ => {}
        }
    }

    async fn on_approval_needed(
        &mut self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> ApprovalDecision {
        let diff = input
            .as_object()
            .map(|obj| {
                obj.iter()
                    .take(2)
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        // Extract the actual command for bash tools (not the key=value diff)
        let bash_command = if tool_name == tn::BASH {
            input
                .as_object()
                .and_then(|obj| obj.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
        } else {
            ""
        };

        let tool_id = self
            .pending_tool_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        // Classify risk for logging
        let tool_type = crate::tool_approval::risk::classify_tool_type(tool_name);
        let risk_command = if tool_name == tn::BASH {
            bash_command.to_string()
        } else {
            input.to_string()
        };
        let risk = crate::tool_approval::risk::classify_tool_risk(&tool_type, &risk_command);
        tracing::info!(
            "Tool approval request: {} (risk={:?}, type={:?}, cmd={})",
            tool_name,
            risk,
            tool_type,
            if tool_name == tn::BASH {
                bash_command
            } else {
                &diff
            }
        );

        // Auto-decide based on permission mode
        match &self.permission_mode {
            PermissionMode::Bypass => {
                tracing::info!("Tool approval: {} auto-approved (Bypass mode)", tool_name);
                return ApprovalDecision::Approve;
            }
            PermissionMode::Auto => {
                // Auto-approve safe tools, reject dangerous ones
                if matches!(risk, crate::tool_approval::risk::RiskLevel::Safe) {
                    tracing::info!(
                        "Tool approval: {} auto-approved (Auto mode, safe tool)",
                        tool_name
                    );
                    return ApprovalDecision::Approve;
                } else {
                    tracing::warn!(
                        "Tool approval: {} auto-rejected (Auto mode, {:?} risk)",
                        tool_name,
                        risk
                    );
                    self.emit(StreamChunk::Text(format!(
                        "[Tool '{}' rejected: Auto mode only allows safe tools]\n",
                        tool_name
                    )));
                    return ApprovalDecision::Reject(format!(
                        "Auto mode rejected (risk={:?})",
                        risk
                    ));
                }
            }
            // Other modes (Default, Plan, AcceptEdits, DontAsk, Bubble) fall through to ask user
            _ => {}
        }

        // For bash tools, send the raw command (not key=value) so the TUI's
        // SmartApprove can properly classify read-only vs dangerous commands.
        let display_diff = if tool_name == tn::BASH && !bash_command.is_empty() {
            Some(bash_command.to_string())
        } else {
            Some(diff)
        };

        let approval_tool_id = tool_id.clone();

        self.emit(StreamChunk::ApprovalRequest {
            tool_name: tool_name.to_string(),
            tool_id: approval_tool_id.clone(),
            description: format!("Execute tool: {}", tool_name),
            diff: display_diff,
        });

        // Wait for user approval from the TUI side
        match self.approval_rx.as_ref() {
            Some(rx) => {
                let deadline = std::time::Instant::now() + Duration::from_mins(5);
                loop {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        tracing::warn!(
                            "Tool approval timed out for {}, rejecting for safety ({:?} risk)",
                            tool_name,
                            risk
                        );
                        self.emit(StreamChunk::ApprovalRejected {
                            tool_id: tool_id.clone(),
                        });
                        self.emit(StreamChunk::Text(format!(
                            "[Tool '{}' rejected: approval timed out after 5 minutes — rejected for safety]\n",
                            tool_name
                        )));
                        break ApprovalDecision::Reject(
                            "approval timed out after 5 minutes — rejected for safety".to_string(),
                        );
                    }

                    let wait = deadline.saturating_duration_since(now);
                    match rx.recv_timeout(wait.min(Duration::from_secs(1))) {
                        Ok((response_tool_id, approved))
                            if response_tool_id == approval_tool_id =>
                        {
                            if approved {
                                tracing::info!("Tool approval: {} approved by user", tool_name);
                                break ApprovalDecision::Approve;
                            }
                            tracing::info!("Tool approval: {} rejected by user", tool_name);
                            break ApprovalDecision::Reject("rejected by user".to_string());
                        }
                        Ok((_response_tool_id, _approved)) => {
                            tracing::debug!(
                                tool_id = %tool_id,
                                "Ignoring approval response for a different tool"
                            );
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(_) => {
                            tracing::warn!(
                                "Approval channel closed while waiting for {}, rejecting",
                                tool_name
                            );
                            break ApprovalDecision::Reject(
                                "approval channel closed while waiting for response".to_string(),
                            );
                        }
                    }
                }
            }
            None => {
                // No approval channel available (e.g., orchestration forwarding thread).
                // Reuse risk already classified above for consistency.
                tracing::warn!(
                    "Tool approval: {} has no approval channel (risk={:?})",
                    tool_name,
                    risk
                );
                let is_safe = matches!(risk, crate::tool_approval::risk::RiskLevel::Safe);
                if is_safe {
                    tracing::debug!(
                        "No approval channel for {}, auto-approving safe tool",
                        tool_name
                    );
                    ApprovalDecision::AutoApproved
                } else {
                    tracing::warn!(
                        "No approval channel for {}, rejecting ({:?} risk)",
                        tool_name,
                        risk
                    );
                    self.emit(StreamChunk::Text(
                        "[Tool execution rejected: no approval channel available]\n".to_string(),
                    ));
                    ApprovalDecision::Reject(format!(
                        "no approval channel available ({:?} risk)",
                        risk
                    ))
                }
            }
        }
    }

    async fn on_question(&mut self, question: &str, options: &[String]) -> Option<String> {
        let question_options = options
            .iter()
            .map(|opt| QuestionOption {
                label: opt.clone(),
                description: String::new(),
            })
            .collect();

        let question_id = self
            .pending_tool_id
            .clone()
            .unwrap_or_else(|| "anon".to_string());

        self.emit(StreamChunk::QuestionRequest {
            question_id,
            question_text: question.to_string(),
            header: "Question".to_string(),
            options: question_options,
            multi_select: false,
        });

        // Wait for user answer
        self.question_rx
            .as_ref()
            .and_then(|rx| rx.recv_timeout(Duration::from_mins(2)).ok())
    }

    async fn on_done(&mut self, _result: &AgentResult) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[tokio::test]
    async fn test_adapter_text_delta() {
        let (tx, rx) = sync_channel(1);
        let mut adapter = StreamEventAdapter::new(tx);
        adapter
            .on_event(StreamEvent::TextDelta {
                content: "Hello".to_string(),
            })
            .await;

        let chunk = rx.recv().unwrap();
        assert_eq!(chunk, StreamChunk::Text("Hello".to_string()));
    }

    #[tokio::test]
    async fn test_adapter_tool_lifecycle() {
        let (tx, rx) = sync_channel(10);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter
            .on_event(StreamEvent::ToolCallStarted {
                id: "t1".into(),
                name: "Bash".into(),
            })
            .await;

        adapter
            .on_event(StreamEvent::ToolInputDelta {
                id: "t1".into(),
                chunk: r#"{"command":"ls -la"}"#.into(),
            })
            .await;

        adapter
            .on_event(StreamEvent::ToolExecStarted {
                id: "t1".into(),
                name: "Bash".into(),
            })
            .await;
        assert_eq!(
            rx.recv().unwrap(),
            StreamChunk::ToolStart {
                tool_name: "Bash".into(),
                tool_id: "t1".into(),
                input_json: r#"{"command":"ls -la"}"#.into()
            }
        );

        adapter
            .on_event(StreamEvent::ToolExecCompleted {
                id: "t1".into(),
                name: "Bash".into(),
                output: "ok".into(),
                is_error: false,
            })
            .await;
        let chunk = rx.recv().unwrap();
        match chunk {
            StreamChunk::ToolComplete {
                tool_name,
                tool_id,
                duration_ms,
                success,
                output_size,
                output,
            } => {
                assert_eq!(tool_name, "Bash");
                assert_eq!(tool_id, "t1");
                assert!(
                    duration_ms < 100,
                    "duration_ms should be near zero, got {duration_ms}"
                );
                assert!(success);
                assert_eq!(output_size, 2);
                assert_eq!(output, Some("ok".into()));
            }
            other => panic!("expected ToolComplete, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_adapter_orchestration_text() {
        let (tx, rx) = sync_channel(1);
        let mut adapter = StreamEventAdapter::new(tx);
        adapter.on_orchestration_event(OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "Orch".into(),
        });

        assert_eq!(rx.recv().unwrap(), StreamChunk::Text("Orch".to_string()));
    }

    #[tokio::test]
    async fn layer2_text_delta_sequence_preserves_order_and_content() {
        let (tx, rx) = sync_channel(10);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter
            .on_event(StreamEvent::TextDelta {
                content: "Hello".to_string(),
            })
            .await;
        adapter
            .on_event(StreamEvent::TextDelta {
                content: ",".to_string(),
            })
            .await;
        adapter
            .on_event(StreamEvent::TextDelta {
                content: " world".to_string(),
            })
            .await;

        let chunk1 = rx.recv().unwrap();
        let chunk2 = rx.recv().unwrap();
        let chunk3 = rx.recv().unwrap();

        assert_eq!(chunk1, StreamChunk::Text("Hello".to_string()));
        assert_eq!(chunk2, StreamChunk::Text(",".to_string()));
        assert_eq!(chunk3, StreamChunk::Text(" world".to_string()));
    }

    #[tokio::test]
    async fn layer2_identical_consecutive_text_deltas_not_deduplicated() {
        let (tx, rx) = sync_channel(10);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter
            .on_event(StreamEvent::TextDelta {
                content: ".".to_string(),
            })
            .await;
        adapter
            .on_event(StreamEvent::TextDelta {
                content: ".".to_string(),
            })
            .await;

        let chunk1 = rx.recv().unwrap();
        let chunk2 = rx.recv().unwrap();

        // Both periods must appear — adapter must NOT deduplicate
        assert_eq!(chunk1, StreamChunk::Text(".".to_string()));
        assert_eq!(chunk2, StreamChunk::Text(".".to_string()));
        assert!(rx.try_recv().is_err()); // No additional chunks
    }

    #[tokio::test]
    async fn layer2_thinking_delta_maps_to_thinking_chunk() {
        let (tx, rx) = sync_channel(1);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter
            .on_event(StreamEvent::ThinkingDelta {
                content: "thinking...".to_string(),
            })
            .await;

        assert_eq!(
            rx.recv().unwrap(),
            StreamChunk::Thinking("thinking...".to_string())
        );
    }

    #[tokio::test]
    async fn layer2_done_event_maps_to_done_chunk() {
        let (tx, rx) = sync_channel(1);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter.on_event(StreamEvent::Done).await;

        assert_eq!(rx.recv().unwrap(), StreamChunk::Done);
    }

    #[tokio::test]
    async fn layer2_token_usage_maps_correctly() {
        let (tx, rx) = sync_channel(1);
        let mut adapter = StreamEventAdapter::new(tx);

        adapter
            .on_event(StreamEvent::TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            })
            .await;

        match rx.recv().unwrap() {
            StreamChunk::TokenUsage {
                input_tokens,
                output_tokens,
                ..
            } => {
                assert_eq!(input_tokens, 100);
                assert_eq!(output_tokens, 50);
            }
            _ => panic!("Expected TokenUsage chunk"),
        }
    }
}
