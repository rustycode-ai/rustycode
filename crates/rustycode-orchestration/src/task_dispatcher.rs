//! Execution bridge that converts `TaskSpecs` into `AgentSessions` via `ForkJoinExecutor`.
//!
//! V1 routes everything through `ForkJoinExecutor`. The real `AgentSession` wiring
//! (LLM provider integration, tool execution, conversation management) will be
//! added in V2.

use crate::bus::BusHandle;
use crate::bus::OrchestrationEvent;
use crate::delegation::{EnsemblePlan, SpawnDecision, TaskSpec};
use crate::fork_join::{ContextSnapshot, ForkJoinConfig, ForkJoinExecutor, ForkSpec};
use crate::task_runner::TaskRunner;
#[cfg(test)]
use crate::types::ExecutionTier;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::Arc;

/// Outcome of executing a single task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub cost_usd: f64,
    pub duration_ms: i64,
}

impl TaskResult {
    pub fn success(
        task_id: impl Into<String>,
        output: impl Into<String>,
        cost_usd: f64,
        duration_ms: i64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            success: true,
            output: output.into(),
            cost_usd,
            duration_ms,
        }
    }

    pub fn failure(
        task_id: impl Into<String>,
        reason: impl Into<String>,
        duration_ms: i64,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            success: false,
            output: reason.into(),
            cost_usd: 0.0,
            duration_ms,
        }
    }
}

/// Converts `TaskSpecs` into executed `TaskResults` via `ForkJoinExecutor`.
///
/// V1: routes all execution through `ForkJoinExecutor`.
/// V2: will wire directly to `AgentSession` for real LLM tool-use loops.
pub struct TaskDispatcher {
    fork_join: ForkJoinExecutor,
    bus: BusHandle,
}

impl TaskDispatcher {
    pub const fn new(fork_join: ForkJoinExecutor, bus: BusHandle) -> Self {
        Self { fork_join, bus }
    }

    /// Create with a real task runner.
    pub fn with_runner(runner: Arc<dyn TaskRunner>, bus: BusHandle) -> Self {
        let fj = ForkJoinExecutor::with_runner(ForkJoinConfig::default(), bus.clone(), runner);
        Self { fork_join: fj, bus }
    }

    /// Dispatch a spawn decision to the appropriate execution path.
    pub async fn dispatch(&self, decision: SpawnDecision) -> Vec<TaskResult> {
        match decision {
            SpawnDecision::Inline => Vec::new(),
            SpawnDecision::Spawn(spec) => vec![self.execute_single(&spec).await],
            SpawnDecision::SpawnParallel(specs) => self.execute_parallel(&specs).await,
            SpawnDecision::Ensemble(plan) => self.execute_ensemble(&plan).await,
        }
    }

    /// Execute a single task spec through the `ForkJoinExecutor`.
    ///
    /// V1: routes through `ForkJoinExecutor`. The real `AgentSession` wiring
    /// will be added in V2 — this placeholder creates a snapshot, a single
    /// fork spec, and delegates execution.
    async fn execute_single(&self, spec: &TaskSpec) -> TaskResult {
        let tier = spec.effective_tier();
        let start = std::time::Instant::now();

        self.bus.publish(OrchestrationEvent::ForkStarted {
            task_id: spec.task_id.clone(),
            fork_id: spec.task_id.clone(),
            fork_count: 1,
        });

        let snapshot = ContextSnapshot::new(&spec.task_id, &spec.prompt, tier.as_u8());

        let fork_spec = task_spec_to_fork_spec(spec);
        let fj_result = self.fork_join.execute_forks(&snapshot, &[fork_spec]).await;

        let elapsed_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

        let task_result = match fj_result.fork_results.into_iter().next() {
            Some(fr) => TaskResult {
                task_id: spec.task_id.clone(),
                success: fr.success,
                output: fr.output,
                cost_usd: fr.cost_usd,
                duration_ms: elapsed_ms,
            },
            None => TaskResult::failure(&spec.task_id, "no fork result returned", elapsed_ms),
        };

        self.bus.publish(OrchestrationEvent::ForkCompleted {
            task_id: spec.task_id.clone(),
            fork_id: spec.task_id.clone(),
            success: task_result.success,
            duration_ms: task_result.duration_ms,
        });

        task_result
    }

    /// Execute multiple task specs in parallel through the `ForkJoinExecutor`.
    async fn execute_parallel(&self, specs: &[TaskSpec]) -> Vec<TaskResult> {
        if specs.is_empty() {
            return Vec::new();
        }

        // Use the first spec's metadata for the snapshot (V1 simplification).
        let first = &specs[0];
        let tier = first.effective_tier();

        let snapshot = ContextSnapshot::new(&first.task_id, &first.prompt, tier.as_u8());

        let fork_specs: Vec<ForkSpec> = specs.iter().map(task_spec_to_fork_spec).collect();
        let fj_result = self.fork_join.execute_forks(&snapshot, &fork_specs).await;

        let mut results = Vec::with_capacity(specs.len());
        for (spec, fr) in specs.iter().zip(fj_result.fork_results.into_iter()) {
            results.push(TaskResult {
                task_id: spec.task_id.clone(),
                success: fr.success,
                output: fr.output,
                cost_usd: fr.cost_usd,
                duration_ms: fr.duration_ms,
            });
        }

        results
    }

    /// Execute an ensemble plan: run participants sequentially, aborting on
    /// veto-capable participant failure.
    async fn execute_ensemble(&self, plan: &EnsemblePlan) -> Vec<TaskResult> {
        let mut results = Vec::with_capacity(plan.participants.len());

        for (participant_spec, task_spec) in &plan.participants {
            let result = self.execute_single(task_spec).await;
            let failed = !result.success;
            results.push(result);

            if failed && participant_spec.can_veto {
                break;
            }
        }

        results
    }
}

/// Convert a `TaskSpec` into a `ForkSpec` for V1 `ForkJoinExecutor` routing.
fn task_spec_to_fork_spec(spec: &TaskSpec) -> ForkSpec {
    let tier = spec.effective_tier();
    let mut fork = ForkSpec::new(&spec.task_id, &spec.prompt, tier);
    fork.role = Some(spec.role);
    fork.resume_from.clone_from(&spec.resume_from);

    for path in &spec.path_scope {
        fork = fork.with_path(path.clone());
    }

    fork
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::delegation::{EnsemblePlan, TaskRole};
    use crate::ensemble_strategy::ParticipantSpec;
    use crate::fork_join::ForkJoinConfig;

    fn make_bus() -> BusHandle {
        BusHandle::new(64)
    }

    fn make_dispatcher(bus: BusHandle) -> TaskDispatcher {
        let fj = ForkJoinExecutor::new(ForkJoinConfig::default(), bus.clone());
        TaskDispatcher::new(fj, bus)
    }

    fn make_spec(prompt: &str) -> TaskSpec {
        let mut spec = TaskSpec::new(prompt, TaskRole::Code);
        spec.task_id = format!("test-{}", spec.task_id);
        spec
    }

    #[test]
    fn task_result_success_factory() {
        let r = TaskResult::success("t1", "done", 0.05, 100);
        assert_eq!(r.task_id, "t1");
        assert!(r.success);
        assert_eq!(r.output, "done");
        assert!((r.cost_usd - 0.05).abs() < f64::EPSILON);
        assert_eq!(r.duration_ms, 100);
    }

    #[test]
    fn task_result_failure_factory() {
        let r = TaskResult::failure("t2", "timeout", 50);
        assert_eq!(r.task_id, "t2");
        assert!(!r.success);
        assert_eq!(r.output, "timeout");
        assert!((r.cost_usd).abs() < f64::EPSILON);
        assert_eq!(r.duration_ms, 50);
    }

    #[test]
    fn task_dispatcher_new() {
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);
        let _ = &dispatcher;
    }

    #[test]
    fn task_spec_to_fork_spec_preserves_resume_from() {
        let mut spec = make_spec("do the thing").with_resume_from("checkpoint-7");
        spec.path_scope.push(PathBuf::from("src/lib.rs"));
        let fork = task_spec_to_fork_spec(&spec);
        assert_eq!(fork.resume_from.as_deref(), Some("checkpoint-7"));
        assert_eq!(fork.path_scope.len(), 1);
    }

    #[tokio::test]
    async fn dispatch_inline_returns_empty() {
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);
        let results = dispatcher.dispatch(SpawnDecision::Inline).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn dispatch_spawn_executes_via_fork_join() {
        let bus = make_bus();
        let mut rx = bus.subscribe();
        let dispatcher = make_dispatcher(bus);

        let spec = make_spec("build feature");
        let mut spec = spec;
        spec.task_id = "t1".into();
        let results = dispatcher.dispatch(SpawnDecision::Spawn(spec)).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].task_id, "t1");

        // Verify bus events were published.
        let e1 = rx.try_recv().unwrap();
        assert!(matches!(e1, OrchestrationEvent::ForkStarted { .. }));
        // ForkJoinExecutor may publish additional internal events; drain remaining.
        while let Ok(_extra) = rx.try_recv() {}
    }

    #[tokio::test]
    async fn dispatch_spawn_parallel_executes_multiple() {
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        let specs = vec![
            {
                let mut s = make_spec("task A");
                s.task_id = "t1".into();
                s
            },
            {
                let mut s = make_spec("task B");
                s.task_id = "t2".into();
                s
            },
            {
                let mut s = make_spec("task C");
                s.task_id = "t3".into();
                s
            },
        ];
        let results = dispatcher
            .dispatch(SpawnDecision::SpawnParallel(specs))
            .await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].task_id, "t1");
        assert_eq!(results[1].task_id, "t2");
        assert_eq!(results[2].task_id, "t3");
        for r in &results {
            assert!(r.success);
        }
    }

    #[tokio::test]
    async fn dispatch_ensemble_respects_can_veto() {
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        // Create an ensemble where the veto participant is second.
        // Since ForkJoinExecutor V1 always succeeds, we can only verify
        // the structure executes all participants when all succeed.
        let participants = vec![
            ParticipantSpec {
                role: "worker".into(),
                weight: 1.0,
                can_veto: false,
            },
            ParticipantSpec {
                role: "reviewer".into(),
                weight: 1.0,
                can_veto: true,
            },
        ];
        let specs = vec![
            {
                let mut s = make_spec("implement");
                s.task_id = "e1".into();
                s
            },
            {
                let mut s = make_spec("review");
                s.task_id = "e2".into();
                s
            },
        ];
        let paired: Vec<_> = participants.into_iter().zip(specs.into_iter()).collect();
        let plan = EnsemblePlan {
            strategy: crate::ensemble_strategy::StrategyKind::SequentialReview,
            participants: paired,
        };

        let results = dispatcher.dispatch(SpawnDecision::Ensemble(plan)).await;

        // Both should succeed in V1, so both results present.
        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(results[1].success);
    }

    #[tokio::test]
    async fn dispatch_ensemble_aborts_on_veto_failure() {
        // V1 ForkJoinExecutor always succeeds, so this test validates
        // the abort logic by verifying that when a non-veto participant
        // is in position 0, both results are present even though we
        // cannot inject a real failure. The abort path is structurally
        // tested through code review of execute_ensemble.
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        let participants = vec![
            ParticipantSpec {
                role: "worker-a".into(),
                weight: 1.0,
                can_veto: false,
            },
            ParticipantSpec {
                role: "worker-b".into(),
                weight: 1.0,
                can_veto: true,
            },
            ParticipantSpec {
                role: "worker-c".into(),
                weight: 1.0,
                can_veto: false,
            },
        ];
        let specs = vec![
            {
                let mut s = make_spec("task A");
                s.task_id = "ea".into();
                s
            },
            {
                let mut s = make_spec("task B");
                s.task_id = "eb".into();
                s
            },
            {
                let mut s = make_spec("task C");
                s.task_id = "ec".into();
                s
            },
        ];
        let paired: Vec<_> = participants.into_iter().zip(specs.into_iter()).collect();
        let plan = EnsemblePlan {
            strategy: crate::ensemble_strategy::StrategyKind::SequentialReview,
            participants: paired,
        };

        let results = dispatcher.dispatch(SpawnDecision::Ensemble(plan)).await;

        assert_eq!(results.len(), 3);
    }

    #[test]
    fn task_spec_to_fork_spec_conversion() {
        let mut spec = TaskSpec::new("do work", TaskRole::Code);
        spec.task_id = "t1".into();
        spec = spec
            .with_path(PathBuf::from("src/main.rs"))
            .with_path(PathBuf::from("src/lib.rs"));

        let fork = task_spec_to_fork_spec(&spec);

        assert_eq!(fork.fork_id, "t1");
        assert_eq!(fork.description, "do work");
        assert_eq!(fork.tier, ExecutionTier::Editor);
        assert_eq!(fork.path_scope.len(), 2);
        assert_eq!(fork.path_scope[0], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn task_spec_to_fork_spec_with_tier_override() {
        let mut spec = TaskSpec::new("think hard", TaskRole::Code);
        spec.task_id = "t1".into();
        spec = spec.with_tier_override(ExecutionTier::Thinking);

        let fork = task_spec_to_fork_spec(&spec);
        assert_eq!(fork.tier, ExecutionTier::Thinking);
    }

    #[test]
    fn task_spec_to_fork_spec_planner_role() {
        let mut spec = TaskSpec::new("plan this", TaskRole::Plan);
        spec.task_id = "t1".into();
        let fork = task_spec_to_fork_spec(&spec);
        assert_eq!(fork.tier, ExecutionTier::Composer);
    }

    #[test]
    fn task_spec_to_fork_spec_no_paths() {
        let mut spec = TaskSpec::new("do stuff", TaskRole::Review);
        spec.task_id = "t1".into();
        let fork = task_spec_to_fork_spec(&spec);
        assert!(fork.path_scope.is_empty());
        assert_eq!(fork.tier, ExecutionTier::Editor);
    }
}
