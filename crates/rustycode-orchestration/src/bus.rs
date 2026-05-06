use crate::error_signal::ErrorSignal;
use crate::guard::{Resource, ResourceAccess};
use rustycode_protocol::{ExecutionPhase, MilestoneId, MilestoneStatus, PlanId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum OrchestrationEvent {
    PartialResult {
        step_id: String,
        content: String,
    },
    StreamDelta {
        task_id: String,
        content: String,
    },
    ToolExecutionStarted {
        task_id: String,
        tool: String,
        args: String,
    },
    ToolExecutionFinished {
        task_id: String,
        tool: String,
        result: String,
    },
    Objection {
        step_id: String,
        reason: String,
    },
    StepFailed {
        step_id: String,
        signal: ErrorSignal,
    },
    TaskCompleted {
        task_id: String,
        tier_used: u8,
        cost_usd: f64,
    },
    PhaseTransition {
        task_id: String,
        from: ExecutionPhase,
        to: ExecutionPhase,
        reason: String,
    },
    EscalationSignal {
        task_id: String,
        from_tier: u8,
        to_tier: u8,
        reason: String,
    },
    WorkspaceUpdated {
        task_id: String,
        key: String,
        written_by: String,
    },
    EnsembleStarted {
        task_id: String,
        strategy: String,
        participant_count: usize,
    },
    EnsembleCompleted {
        task_id: String,
        confidence: f64,
        steps_produced: usize,
    },
    ResourceIntent {
        holder: String,
        resources: Vec<(Resource, ResourceAccess)>,
    },
    ResourceConflict {
        holder: String,
        resource: Resource,
        conflict_with: String,
    },
    /// Real-time text delta from the LLM.
    TextDelta {
        task_id: String,
        content: String,
    },
    /// Thinking/reasoning delta from the LLM.
    ThinkingDelta {
        task_id: String,
        content: String,
    },
    /// A tool call started during step execution.
    ToolCallStarted {
        task_id: String,
        step_id: String,
        tool_id: String,
        tool_name: String,
        input_preview: String,
    },
    /// Incremental tool input JSON chunk.
    ToolInputDelta {
        task_id: String,
        tool_id: String,
        chunk: String,
    },
    /// A tool call completed during step execution.
    ToolCallCompleted {
        task_id: String,
        step_id: String,
        tool_id: String,
        tool_name: String,
        success: bool,
        output_preview: String,
    },
    /// A tier handoff occurred.
    TierHandoff {
        task_id: String,
        from_tier: u8,
        to_tier: u8,
        package_size_bytes: usize,
    },
    /// A parallel fork was started.
    ForkStarted {
        task_id: String,
        fork_id: String,
        fork_count: usize,
    },
    /// A parallel fork completed.
    ForkCompleted {
        task_id: String,
        fork_id: String,
        success: bool,
        duration_ms: i64,
    },
    /// A tier's context budget was exceeded.
    ContextBudgetExceeded {
        task_id: String,
        tier: u8,
        used: u64,
        limit: u64,
    },
    /// Token usage report from the LLM.
    TokenUsage {
        task_id: String,
        input_tokens: u64,
        output_tokens: u64,
    },
    /// Cache token accounting.
    CacheUsage {
        task_id: String,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
    },
    /// A plan has been created with steps.
    PlanCreated {
        task_id: String,
        plan_id: String,
        title: String,
        steps: Vec<(String, String)>, // (name, description)
    },
    /// A plan step has started executing.
    PlanStepStarted {
        task_id: String,
        plan_id: String,
        step_index: usize,
    },
    /// A plan step has finished.
    PlanStepCompleted {
        task_id: String,
        plan_id: String,
        step_index: usize,
        success: bool,
        message: String,
    },
    /// The entire plan has finished.
    PlanCompleted {
        task_id: String,
        plan_id: String,
        success: bool,
        summary: String,
    },
    /// Plan is awaiting user approval.
    PlanApprovalRequested {
        task_id: String,
        plan_id: String,
        title: String,
        steps: Vec<(String, String)>, // (name, description)
    },
    /// Milestone execution progress changed.
    MilestoneProgress {
        task_id: String,
        milestone_id: MilestoneId,
        milestone_title: String,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
        plan_rows: Vec<MilestonePlanProgress>,
    },
    /// A delegated task has been spawned into its own context.
    TaskSpawned {
        task_id: String,
        role: String,
        tier: u8,
        parent_task_id: String,
    },
    /// A delegated task completed successfully.
    TaskDelegationCompleted {
        task_id: String,
        role: String,
        output_preview: String,
        cost_usd: f64,
        duration_ms: i64,
    },
    /// A delegated task failed.
    TaskDelegationFailed {
        task_id: String,
        role: String,
        error: String,
        cost_usd: f64,
        duration_ms: i64,
    },
    /// A parallel delegation batch started.
    DelegationBatchStarted {
        parent_task_id: String,
        task_count: usize,
        roles: Vec<String>,
    },
    /// A parallel delegation batch completed.
    DelegationBatchCompleted {
        parent_task_id: String,
        succeeded: usize,
        failed: usize,
        total_cost_usd: f64,
        total_duration_ms: i64,
    },
}

/// Compact plan snapshot used for milestone progress rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestonePlanProgress {
    pub plan_id: PlanId,
    pub title: String,
    pub state: MilestonePlanState,
    pub blocked_by: Vec<String>,
}

/// Rendered milestone plan state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MilestonePlanState {
    Draft,
    Ready,
    Running,
    Completed,
    Blocked,
    Failed,
}

#[derive(Clone)]
pub struct BusHandle {
    tx: Arc<broadcast::Sender<OrchestrationEvent>>,
}

impl std::fmt::Debug for BusHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusHandle")
            .field("receiver_count", &self.tx.receiver_count())
            .finish()
    }
}

impl BusHandle {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx: Arc::new(tx) }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestrationEvent> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: OrchestrationEvent) {
        if let Err(e) = self.tx.send(event) {
            tracing::debug!(error = %e, "No subscribers for orchestration event");
        }
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

pub struct MessageBus {
    handle: BusHandle,
}

impl MessageBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            handle: BusHandle::new(capacity),
        }
    }

    pub fn handle(&self) -> BusHandle {
        self.handle.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<OrchestrationEvent> {
        self.handle.subscribe()
    }

    pub fn publish(&self, event: OrchestrationEvent) {
        self.handle.publish(event);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_bus_handle_publish_subscribe() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "test".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PartialResult { step_id, .. } if step_id == "s1"
        ));
    }

    #[test]
    fn test_bus_handle_clone_shares_channel() {
        let handle = BusHandle::new(16);
        let handle2 = handle.clone();
        let mut rx = handle.subscribe();

        handle2.publish(OrchestrationEvent::TaskCompleted {
            task_id: "t1".into(),
            tier_used: 2,
            cost_usd: 0.01,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::TaskCompleted { .. }));
    }

    #[test]
    fn test_bus_handle_debug() {
        let handle = BusHandle::new(16);
        let debug = format!("{handle:?}");
        assert!(debug.contains("BusHandle"));
    }

    #[test]
    fn test_bus_handle_receiver_count() {
        let handle = BusHandle::new(16);
        assert_eq!(handle.receiver_count(), 0);
        let _rx1 = handle.subscribe();
        assert_eq!(handle.receiver_count(), 1);
    }

    #[test]
    fn test_resource_intent_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ResourceIntent {
            holder: "agent-1".into(),
            resources: vec![(Resource::path("/test.rs"), ResourceAccess::Read)],
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::ResourceIntent { .. }));
    }

    #[test]
    fn test_phase_transition_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PhaseTransition {
            task_id: "t1".into(),
            from: ExecutionPhase::Explore,
            to: ExecutionPhase::Plan,
            reason: "gathered context".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PhaseTransition { task_id, from: ExecutionPhase::Explore, to: ExecutionPhase::Plan, .. }
                if task_id == "t1"
        ));
    }

    #[test]
    fn test_resource_conflict_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ResourceConflict {
            holder: "agent-2".into(),
            resource: Resource::path("/test.rs"),
            conflict_with: "agent-1".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::ResourceConflict { .. }));
    }

    #[test]
    fn test_step_failed_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        let signal = crate::error_signal::ErrorSignal::new(
            crate::error_signal::SignalCategory::LogicError,
            Some(1),
            "test error".into(),
            "s1".into(),
            "bash".into(),
        );

        handle.publish(OrchestrationEvent::StepFailed {
            step_id: "s1".into(),
            signal,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::StepFailed { step_id, .. } if step_id == "s1"));
    }

    #[test]
    fn test_escalation_signal_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::EscalationSignal {
            task_id: "t1".into(),
            from_tier: 2,
            to_tier: 3,
            reason: "budget_exceeded".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, OrchestrationEvent::EscalationSignal { task_id, from_tier: 2, to_tier: 3, .. } if task_id == "t1")
        );
    }

    #[test]
    fn test_workspace_updated_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::WorkspaceUpdated {
            task_id: "t1".into(),
            key: "result".into(),
            written_by: "musician".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(
            matches!(event, OrchestrationEvent::WorkspaceUpdated { key, .. } if key == "result")
        );
    }

    #[test]
    fn test_ensemble_events() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::EnsembleStarted {
            task_id: "t1".into(),
            strategy: "parallel".into(),
            participant_count: 3,
        });

        handle.publish(OrchestrationEvent::EnsembleCompleted {
            task_id: "t1".into(),
            confidence: 0.85,
            steps_produced: 5,
        });

        let event1 = rx.try_recv().unwrap();
        assert!(matches!(
            event1,
            OrchestrationEvent::EnsembleStarted {
                participant_count: 3,
                ..
            }
        ));

        let event2 = rx.try_recv().unwrap();
        assert!(matches!(
            event2,
            OrchestrationEvent::EnsembleCompleted {
                steps_produced: 5,
                ..
            }
        ));
    }

    #[test]
    fn test_message_bus_new_and_handle() {
        let bus = MessageBus::new(16);
        let handle = bus.handle();
        let mut rx = bus.subscribe();

        handle.publish(OrchestrationEvent::TaskCompleted {
            task_id: "t1".into(),
            tier_used: 2,
            cost_usd: 0.01,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(event, OrchestrationEvent::TaskCompleted { .. }));
    }

    #[test]
    fn test_bus_no_receiver_drops_message() {
        let handle = BusHandle::new(16);
        // Publish without any receiver -- should not panic
        handle.publish(OrchestrationEvent::PartialResult {
            step_id: "s1".into(),
            content: "orphaned".into(),
        });
    }

    #[test]
    fn test_tier_handoff_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::TierHandoff {
            task_id: "t1".into(),
            from_tier: 2,
            to_tier: 3,
            package_size_bytes: 1024,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::TierHandoff { task_id, from_tier: 2, to_tier: 3, package_size_bytes: 1024 }
                if task_id == "t1"
        ));
    }

    #[test]
    fn test_fork_started_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ForkStarted {
            task_id: "t1".into(),
            fork_id: "fork-0".into(),
            fork_count: 3,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ForkStarted { fork_count: 3, .. }
        ));
    }

    #[test]
    fn test_fork_completed_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ForkCompleted {
            task_id: "t1".into(),
            fork_id: "fork-0".into(),
            success: true,
            duration_ms: 500,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ForkCompleted {
                success: true,
                duration_ms: 500,
                ..
            }
        ));
    }

    #[test]
    fn test_context_budget_exceeded_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ContextBudgetExceeded {
            task_id: "t1".into(),
            tier: 2,
            used: 100_000,
            limit: 100_000,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ContextBudgetExceeded {
                tier: 2,
                used: 100_000,
                ..
            }
        ));
    }

    #[test]
    fn test_text_delta_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::TextDelta {
            task_id: "t1".into(),
            content: "hello".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::TextDelta { task_id, content }
                if task_id == "t1" && content == "hello"
        ));
    }

    #[test]
    fn test_thinking_delta_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ThinkingDelta {
            task_id: "t1".into(),
            content: "reasoning...".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ThinkingDelta { task_id, content }
                if task_id == "t1" && content == "reasoning..."
        ));
    }

    #[test]
    fn test_tool_call_started_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ToolCallStarted {
            task_id: "t1".into(),
            step_id: "s1".into(),
            tool_id: "tc-1".into(),
            tool_name: "bash".into(),
            input_preview: "echo hi".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ToolCallStarted { task_id, tool_name, .. }
                if task_id == "t1" && tool_name == "bash"
        ));
    }

    #[test]
    fn test_tool_input_delta_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ToolInputDelta {
            task_id: "t1".into(),
            tool_id: "tc-1".into(),
            chunk: r#"{"cmd":"echo"#.into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ToolInputDelta { task_id, tool_id, chunk }
                if task_id == "t1" && tool_id == "tc-1" && chunk == r#"{"cmd":"echo"#
        ));
    }

    #[test]
    fn test_plan_created_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PlanCreated {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            title: "Refactor auth".into(),
            steps: vec![("Step 1".into(), "Read files".into())],
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PlanCreated { plan_id, steps, .. }
                if plan_id == "plan-1" && steps.len() == 1
        ));
    }

    #[test]
    fn test_plan_step_started_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PlanStepStarted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            step_index: 2,
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PlanStepStarted { step_index: 2, .. }
        ));
    }

    #[test]
    fn test_plan_step_completed_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PlanStepCompleted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            step_index: 0,
            success: true,
            message: "done".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PlanStepCompleted { success: true, .. }
        ));
    }

    #[test]
    fn test_plan_completed_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PlanCompleted {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            success: true,
            summary: "All steps done".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PlanCompleted { success: true, summary, .. }
                if summary == "All steps done"
        ));
    }

    #[test]
    fn test_plan_approval_requested_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::PlanApprovalRequested {
            task_id: "t1".into(),
            plan_id: "plan-1".into(),
            title: "Big refactor".into(),
            steps: vec![
                ("Analyze".into(), "Read codebase".into()),
                ("Implement".into(), "Write code".into()),
            ],
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::PlanApprovalRequested { steps, .. }
                if steps.len() == 2
        ));
    }

    #[test]
    fn test_tool_call_completed_event() {
        let handle = BusHandle::new(16);
        let mut rx = handle.subscribe();

        handle.publish(OrchestrationEvent::ToolCallCompleted {
            task_id: "t1".into(),
            step_id: "s1".into(),
            tool_id: "tc-1".into(),
            tool_name: "bash".into(),
            success: true,
            output_preview: "hi".into(),
        });

        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            OrchestrationEvent::ToolCallCompleted { task_id, success: true, .. }
                if task_id == "t1"
        ));
    }
}
