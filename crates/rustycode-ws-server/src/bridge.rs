use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rustycode_orchestration::bus::{BusHandle, OrchestrationEvent};
use rustycode_protocol::StreamEvent;
use tokio::sync::{broadcast, oneshot, RwLock};
use tracing::{info, warn};

type EventSender = tokio::sync::mpsc::Sender<StreamEvent>;

pub struct EventBridge {
    handle: Arc<RwLock<BusHandle>>,
    sessions: Arc<RwLock<HashMap<String, EventSender>>>,
    cancel: Arc<Mutex<tokio_util::sync::CancellationToken>>,
}

impl EventBridge {
    pub fn new(handle: BusHandle) -> Self {
        info!("EventBridge created");
        Self {
            handle: Arc::new(RwLock::new(handle)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            cancel: Arc::new(Mutex::new(tokio_util::sync::CancellationToken::new())),
        }
    }

    pub async fn bus_handle(&self) -> BusHandle {
        self.handle.read().await.clone()
    }

    pub async fn register(&self, session_token: &str, sender: EventSender) {
        self.sessions
            .write()
            .await
            .insert(session_token.to_string(), sender);
    }

    pub async fn unregister(&self, session_token: &str) {
        self.sessions.write().await.remove(session_token);
    }

    pub async fn start(&self) {
        let ready = self.spawn_forwarder();
        let _ = ready.await;
    }

    /// Swap the bus handle (e.g. after pipeline rebuild on provider switch)
    /// and restart the forwarding loop on the new bus.
    pub async fn resubscribe(&self, new_handle: BusHandle) {
        let old_cancel = {
            let mut guard = self
                .cancel
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::mem::replace(&mut *guard, tokio_util::sync::CancellationToken::new())
        };
        old_cancel.cancel();
        {
            let mut guard = self.handle.write().await;
            *guard = new_handle;
        }
        let ready = self.spawn_forwarder();
        let _ = ready.await;
        info!("EventBridge resubscribed to new bus");
    }

    fn spawn_forwarder(&self) -> oneshot::Receiver<()> {
        let (ready_tx, ready_rx) = oneshot::channel();
        let cancel = {
            self.cancel
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        };
        let handle = Arc::clone(&self.handle);
        let sessions = self.sessions.clone();

        tokio::spawn(async move {
            let bus = handle.read().await.clone();
            let mut rx = bus.subscribe();
            drop(bus);
            let _ = ready_tx.send(());

            loop {
                tokio::select! {
                    biased;
                    () = cancel.cancelled() => {
                        info!("EventBridge forwarder cancelled");
                        break;
                    }
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                let Some(stream_event) = convert_event(&event) else {
                                    continue;
                                };
                                // Clone senders out of the read lock before awaiting
                                // to avoid holding the lock across potentially slow sends.
                                let senders: Vec<_> = {
                                    let read_guard = sessions.read().await;
                                    read_guard
                                        .iter()
                                        .map(|(t, s)| (t.clone(), s.clone()))
                                        .collect()
                                };
                                let mut dead_tokens = Vec::new();
                                for (token, sender) in senders {
                                    if sender.is_closed() {
                                        dead_tokens.push(token);
                                    } else if let Err(e) = sender.send(stream_event.clone()).await {
                                        warn!(session = %token, "failed to forward event: {e}");
                                        dead_tokens.push(token);
                                    }
                                }
                                if !dead_tokens.is_empty() {
                                    let mut write_guard = sessions.write().await;
                                    for token in dead_tokens {
                                        write_guard.remove(&token);
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                warn!("EventBridge lagged by {n} events");
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("EventBridge: bus channel closed");
                                break;
                            }
                        }
                    }
                }
            }
        });
        ready_rx
    }
}

impl Drop for EventBridge {
    fn drop(&mut self) {
        info!("EventBridge dropped");
    }
}

fn convert_event(event: &OrchestrationEvent) -> Option<StreamEvent> {
    match event {
        OrchestrationEvent::TextDelta { content, .. }
        | OrchestrationEvent::StreamDelta { content, .. } => Some(StreamEvent::TextDelta {
            content: content.clone(),
        }),
        OrchestrationEvent::ThinkingDelta { content, .. } => Some(StreamEvent::ThinkingDelta {
            content: content.clone(),
        }),
        OrchestrationEvent::ToolCallStarted {
            tool_id, tool_name, ..
        } => Some(StreamEvent::ToolCallStarted {
            id: tool_id.clone(),
            name: tool_name.clone(),
        }),
        OrchestrationEvent::ToolInputDelta { tool_id, chunk, .. } => {
            Some(StreamEvent::ToolInputDelta {
                id: tool_id.clone(),
                chunk: chunk.clone(),
            })
        }
        OrchestrationEvent::ToolCallCompleted {
            tool_id,
            tool_name,
            success,
            output_preview,
            ..
        } => Some(StreamEvent::ToolExecCompleted {
            id: tool_id.clone(),
            name: tool_name.clone(),
            output: output_preview.clone(),
            is_error: !success,
        }),
        OrchestrationEvent::ToolExecutionStarted { task_id, tool, .. } => {
            Some(StreamEvent::ToolExecStarted {
                id: task_id.clone(),
                name: tool.clone(),
            })
        }
        OrchestrationEvent::ToolExecutionFinished {
            task_id,
            tool,
            result,
        } => Some(StreamEvent::ToolExecCompleted {
            id: task_id.clone(),
            name: tool.clone(),
            output: result.clone(),
            is_error: false,
        }),
        OrchestrationEvent::StepFailed { step_id, signal } => {
            Some(StreamEvent::ToolExecCompleted {
                id: step_id.clone(),
                name: String::new(),
                output: signal.message.clone(),
                is_error: true,
            })
        }
        OrchestrationEvent::TaskCompleted { .. } => Some(StreamEvent::Done),
        // PhaseTransition, EscalationSignal, and TierHandoff are internal
        // orchestration status — the frontend tracks progress via the
        // dedicated plan_lifecycle events (plan_created, plan_step_started,
        // etc.) and tool events.  Emitting them as TextDelta pollutes the
        // chat with "[Plan] context gathered" noise.
        OrchestrationEvent::PhaseTransition { .. }
        | OrchestrationEvent::EscalationSignal { .. }
        | OrchestrationEvent::TierHandoff { .. } => None,
        OrchestrationEvent::TokenUsage {
            input_tokens,
            output_tokens,
            ..
        } => Some(StreamEvent::TokenUsage {
            input_tokens: *input_tokens,
            output_tokens: *output_tokens,
        }),
        OrchestrationEvent::CacheUsage {
            cache_read_tokens,
            cache_creation_tokens,
            ..
        } => Some(StreamEvent::CacheUsage {
            cache_read_tokens: *cache_read_tokens,
            cache_creation_tokens: *cache_creation_tokens,
        }),
        other => convert_plan_event(other),
    }
}

fn to_stream_steps(
    steps: &[(String, String)],
) -> Vec<rustycode_protocol::stream_event::StreamPlanStep> {
    steps
        .iter()
        .map(
            |(name, desc)| rustycode_protocol::stream_event::StreamPlanStep {
                name: name.clone(),
                description: desc.clone(),
            },
        )
        .collect()
}

fn convert_plan_event(event: &OrchestrationEvent) -> Option<StreamEvent> {
    match event {
        OrchestrationEvent::PlanCreated {
            plan_id,
            title,
            steps,
            ..
        } => Some(StreamEvent::PlanCreated {
            id: plan_id.clone(),
            title: title.clone(),
            steps: to_stream_steps(steps),
        }),
        OrchestrationEvent::PlanStepStarted {
            plan_id,
            step_index,
            ..
        } => Some(StreamEvent::PlanStepStarted {
            plan_id: plan_id.clone(),
            step_index: *step_index,
        }),
        OrchestrationEvent::PlanStepCompleted {
            plan_id,
            step_index,
            success,
            message,
            ..
        } => Some(StreamEvent::PlanStepCompleted {
            plan_id: plan_id.clone(),
            step_index: *step_index,
            success: *success,
            message: message.clone(),
        }),
        OrchestrationEvent::PlanCompleted {
            plan_id,
            success,
            summary,
            ..
        } => Some(StreamEvent::PlanCompleted {
            plan_id: plan_id.clone(),
            success: *success,
            summary: summary.clone(),
        }),
        OrchestrationEvent::PlanApprovalRequested {
            plan_id,
            title,
            steps,
            ..
        } => Some(StreamEvent::PlanApprovalRequested {
            plan_id: plan_id.clone(),
            title: title.clone(),
            steps: to_stream_steps(steps),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn convert_text_delta() {
        let event = OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "hello".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::TextDelta {
                content: "hello".into()
            }
        );
    }

    #[test]
    fn convert_thinking_delta() {
        let event = OrchestrationEvent::ThinkingDelta {
            task_id: "t1".into(),
            content: "reasoning".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ThinkingDelta {
                content: "reasoning".into()
            }
        );
    }

    #[test]
    fn convert_tool_call_started() {
        let event = OrchestrationEvent::ToolCallStarted {
            task_id: "t1".into(),
            step_id: "s1".into(),
            tool_id: "tc-1".into(),
            tool_name: "Bash".into(),
            input_preview: "echo hi".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ToolCallStarted {
                id: "tc-1".into(),
                name: "Bash".into()
            }
        );
    }

    #[test]
    fn convert_tool_input_delta() {
        let event = OrchestrationEvent::ToolInputDelta {
            task_id: "t1".into(),
            tool_id: "tc-1".into(),
            chunk: r#"{"cmd":"echo"#.into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ToolInputDelta {
                id: "tc-1".into(),
                chunk: r#"{"cmd":"echo"#.into(),
            }
        );
    }

    #[test]
    fn convert_tool_call_completed() {
        let event = OrchestrationEvent::ToolCallCompleted {
            task_id: "t1".into(),
            step_id: "s1".into(),
            tool_id: "tc-1".into(),
            tool_name: "Bash".into(),
            success: true,
            output_preview: "ok".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ToolExecCompleted {
                id: "tc-1".into(),
                name: "Bash".into(),
                output: "ok".into(),
                is_error: false
            }
        );
    }

    #[test]
    fn convert_task_completed() {
        let event = OrchestrationEvent::TaskCompleted {
            task_id: "t1".into(),
            tier_used: 2,
            cost_usd: 0.01,
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(result, StreamEvent::Done);
    }

    #[test]
    fn convert_stream_delta() {
        let event = OrchestrationEvent::StreamDelta {
            task_id: "t1".into(),
            content: "stream text".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::TextDelta {
                content: "stream text".into()
            }
        );
    }

    #[test]
    fn convert_tool_execution_started() {
        let event = OrchestrationEvent::ToolExecutionStarted {
            task_id: "t1".into(),
            tool: "Bash".into(),
            args: "echo hi".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ToolExecStarted {
                id: "t1".into(),
                name: "Bash".into()
            }
        );
    }

    #[test]
    fn convert_tool_execution_finished() {
        let event = OrchestrationEvent::ToolExecutionFinished {
            task_id: "t1".into(),
            tool: "Read".into(),
            result: "file contents".into(),
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::ToolExecCompleted {
                id: "t1".into(),
                name: "Read".into(),
                output: "file contents".into(),
                is_error: false
            }
        );
    }

    #[test]
    fn convert_step_failed() {
        let event = OrchestrationEvent::StepFailed {
            step_id: "s1".into(),
            signal: rustycode_orchestration::error_signal::ErrorSignal::new(
                rustycode_orchestration::error_signal::ErrorCategory::LogicError,
                Some(1),
                "test failed".into(),
                "s1".into(),
                "Bash".into(),
            ),
        };
        let result = convert_event(&event).unwrap();
        assert!(matches!(
            result,
            StreamEvent::ToolExecCompleted { is_error: true, .. }
        ));
    }

    #[test]
    fn convert_phase_transition_is_suppressed() {
        let event = OrchestrationEvent::PhaseTransition {
            task_id: "t1".into(),
            from: rustycode_protocol::ExecutionPhase::Explore,
            to: rustycode_protocol::ExecutionPhase::Plan,
            reason: "context gathered".into(),
        };
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn convert_escalation_signal_is_suppressed() {
        let event = OrchestrationEvent::EscalationSignal {
            task_id: "t1".into(),
            from_tier: 2,
            to_tier: 3,
            reason: "stuck".into(),
        };
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn convert_tier_handoff_is_suppressed() {
        let event = OrchestrationEvent::TierHandoff {
            task_id: "t1".into(),
            from_tier: 1,
            to_tier: 2,
            package_size_bytes: 4096,
        };
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn convert_plan_created() {
        let event = OrchestrationEvent::PlanCreated {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            title: "Refactor".into(),
            steps: vec![("Step 1".into(), "Read".into())],
        };
        let result = convert_event(&event).unwrap();
        assert!(matches!(result, StreamEvent::PlanCreated { .. }));
        if let StreamEvent::PlanCreated { id, steps, .. } = result {
            assert_eq!(id, "plan-1");
            assert_eq!(steps.len(), 1);
        }
    }

    #[test]
    fn convert_plan_step_started() {
        let event = OrchestrationEvent::PlanStepStarted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            step_index: 2,
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::PlanStepStarted {
                plan_id: "plan-1".into(),
                step_index: 2,
            }
        );
    }

    #[test]
    fn convert_plan_step_completed() {
        let event = OrchestrationEvent::PlanStepCompleted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            step_index: 0,
            success: true,
            message: "done".into(),
        };
        let result = convert_event(&event).unwrap();
        assert!(matches!(
            result,
            StreamEvent::PlanStepCompleted { success: true, .. }
        ));
    }

    #[test]
    fn convert_plan_completed() {
        let event = OrchestrationEvent::PlanCompleted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            success: true,
            summary: "All done".into(),
        };
        let result = convert_event(&event).unwrap();
        assert!(matches!(
            result,
            StreamEvent::PlanCompleted { success: true, .. }
        ));
    }

    #[test]
    fn convert_plan_approval_requested() {
        let event = OrchestrationEvent::PlanApprovalRequested {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            title: "Big refactor".into(),
            steps: vec![("Analyze".into(), "Read".into())],
        };
        let result = convert_event(&event).unwrap();
        assert!(matches!(result, StreamEvent::PlanApprovalRequested { .. }));
        if let StreamEvent::PlanApprovalRequested { steps, .. } = result {
            assert_eq!(steps.len(), 1);
        }
    }

    #[test]
    fn convert_ignores_unmapped_events() {
        let event = OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "partial".into(),
        };
        assert!(convert_event(&event).is_none());
    }

    #[test]
    fn convert_token_usage() {
        let event = OrchestrationEvent::TokenUsage {
            task_id: "t1".into(),
            input_tokens: 1500,
            output_tokens: 800,
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::TokenUsage {
                input_tokens: 1500,
                output_tokens: 800
            }
        );
    }

    #[test]
    fn convert_cache_usage() {
        let event = OrchestrationEvent::CacheUsage {
            task_id: "t1".into(),
            cache_read_tokens: 5000,
            cache_creation_tokens: 1200,
        };
        let result = convert_event(&event).unwrap();
        assert_eq!(
            result,
            StreamEvent::CacheUsage {
                cache_read_tokens: 5000,
                cache_creation_tokens: 1200
            }
        );
    }

    #[tokio::test]
    async fn bridge_registers_and_forwards() {
        let bus = BusHandle::new(16);
        let bridge = EventBridge::new(bus.clone());
        bridge.start().await;

        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        bridge.register("sess-1", tx).await;

        bus.publish(OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "hello".into(),
        });

        let event = rx.recv().await.unwrap();
        assert_eq!(
            event,
            StreamEvent::TextDelta {
                content: "hello".into()
            }
        );
    }

    #[tokio::test]
    async fn bridge_unregister_stops_delivery() {
        let bus = BusHandle::new(16);
        let bridge = EventBridge::new(bus.clone());
        bridge.start().await;

        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        bridge.register("sess-1", tx).await;
        bridge.unregister("sess-1").await;

        bus.publish(OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "orphaned".into(),
        });

        // Channel should not receive anything (sender dropped)
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn bridge_resubscribe_switches_bus() {
        let bus1 = BusHandle::new(16);
        let bridge = EventBridge::new(bus1.clone());
        bridge.start().await;

        let (tx, mut rx) = tokio::sync::mpsc::channel(128);
        bridge.register("sess-1", tx).await;

        // Publish on old bus — should arrive
        bus1.publish(OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "old-bus".into(),
        });
        let event = rx.recv().await.unwrap();
        assert_eq!(
            event,
            StreamEvent::TextDelta {
                content: "old-bus".into()
            }
        );

        // Resubscribe to a new bus
        let bus2 = BusHandle::new(16);
        bridge.resubscribe(bus2.clone()).await;

        // Give the new forwarder time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Publish on new bus — should arrive
        bus2.publish(OrchestrationEvent::TextDelta {
            task_id: "t2".into(),
            content: "new-bus".into(),
        });
        let event = rx.recv().await.unwrap();
        assert_eq!(
            event,
            StreamEvent::TextDelta {
                content: "new-bus".into()
            }
        );
    }
}
