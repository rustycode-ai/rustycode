use crate::app::async_::{QuestionOption, StreamChunk};
use async_trait::async_trait;
use rustycode_agent::{AgentEvents, AgentResult, ApprovalDecision};
use rustycode_core::streaming::ToolCall;
use rustycode_orchestration::bus::OrchestrationEvent;
use rustycode_protocol::stream_event::StreamEvent;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, SyncSender};
use std::time::Duration;

pub struct StreamEventAdapter {
    stream_tx: SyncSender<StreamChunk>,
    approval_rx: Option<Receiver<bool>>,
    question_rx: Option<Receiver<String>>,
    active_tools: HashMap<String, ToolCall>,
    pending_tool_id: Option<String>,
}

impl StreamEventAdapter {
    pub fn new(stream_tx: SyncSender<StreamChunk>) -> Self {
        Self {
            stream_tx,
            approval_rx: None,
            question_rx: None,
            active_tools: HashMap::new(),
            pending_tool_id: None,
        }
    }

    pub fn with_approval_rx(mut self, rx: Receiver<bool>) -> Self {
        self.approval_rx = Some(rx);
        self
    }

    pub fn with_question_rx(mut self, rx: Receiver<String>) -> Self {
        self.question_rx = Some(rx);
        self
    }

    pub fn emit(&self, chunk: StreamChunk) {
        // Blocking send: never drop chunks (fixes missing final text after tool calls)
        if let Err(_e) = self.stream_tx.send(chunk) {
            tracing::debug!("Stream channel closed during send");
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
            OrchestrationEvent::ToolExecutionStarted { tool, args, .. } => {
                let tool_id = format!("exec-{}", tool);
                let call = ToolCall::new(tool_id.clone(), tool.clone(), args);
                self.emit(StreamChunk::ToolStart {
                    tool_name: tool.clone(),
                    input_json: call.partial_json.clone(),
                    tool_id: tool_id.clone(),
                });
                self.active_tools.insert(tool_id, call);
            }
            OrchestrationEvent::ToolExecutionFinished { tool, result, .. } => {
                let tool_id = format!("exec-{}", tool);
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
                self.emit(StreamChunk::Error(
                    "Context limit reached — response may be incomplete".to_string(),
                ));
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
                self.emit(StreamChunk::Error(format!(
                    "Step failed: {}",
                    signal.message
                )));
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

        let tool_id = self
            .pending_tool_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        self.emit(StreamChunk::ApprovalRequest {
            tool_name: tool_name.to_string(),
            tool_id,
            description: format!("Execute tool: {}", tool_name),
            diff: Some(diff),
        });

        // Wait for user approval from the TUI side
        match self
            .approval_rx
            .as_ref()
            .map(|rx| rx.recv_timeout(Duration::from_mins(5)))
        {
            Some(Ok(true)) => ApprovalDecision::Approve,
            Some(Ok(false)) => ApprovalDecision::Reject("rejected by user".to_string()),
            Some(Err(_)) => {
                // Timeout: only auto-approve safe (read-only) tools.
                // Dangerous tools must be rejected to prevent silent
                // approval of destructive operations like `rm -rf`.
                let tool_type = crate::tool_approval::risk::classify_tool_type(tool_name);
                let command_str = input.to_string();
                let risk = crate::tool_approval::risk::classify_tool_risk(&tool_type, &command_str);
                let is_safe = matches!(risk, crate::tool_approval::risk::RiskLevel::Safe);
                if is_safe {
                    tracing::warn!(
                        "Tool approval timed out for {}, auto-approving safe tool",
                        tool_name
                    );
                    self.emit(StreamChunk::ApprovalApproved {
                        tool_id: self
                            .pending_tool_id
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    });
                    ApprovalDecision::AutoApproved
                } else {
                    tracing::warn!(
                        "Tool approval timed out for {}, auto-rejecting ({:?} risk)",
                        tool_name,
                        risk
                    );
                    self.emit(StreamChunk::ApprovalRejected {
                        tool_id: self
                            .pending_tool_id
                            .clone()
                            .unwrap_or_else(|| "unknown".to_string()),
                    });
                    self.emit(StreamChunk::Text(
                        "[Tool execution rejected: approval timed out]\n".to_string(),
                    ));
                    ApprovalDecision::Reject(format!("approval timed out ({:?} risk)", risk))
                }
            }
            None => ApprovalDecision::AutoApproved,
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
                name: "bash".into(),
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
                name: "bash".into(),
            })
            .await;
        assert_eq!(
            rx.recv().unwrap(),
            StreamChunk::ToolStart {
                tool_name: "bash".into(),
                tool_id: "t1".into(),
                input_json: r#"{"command":"ls -la"}"#.into()
            }
        );

        adapter
            .on_event(StreamEvent::ToolExecCompleted {
                id: "t1".into(),
                name: "bash".into(),
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
                assert_eq!(tool_name, "bash");
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
