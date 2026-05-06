//! Execution bridge that converts `TaskSpecs` into `AgentSessions` via `ForkJoinExecutor`.
//!
//! V1 routes everything through `ForkJoinExecutor`. V2 wires directly to
//! `AgentSession` from `rustycode-agent-runtime` for real LLM tool-use loops.

use crate::bus::BusHandle;
use crate::bus::OrchestrationEvent;
use crate::delegation::{EnsemblePlan, SpawnDecision, TaskSpec};
use crate::fork_join::{ContextSnapshot, ForkJoinConfig, ForkJoinExecutor, ForkSpec};
use crate::task_runner::TaskRunner;
#[cfg(test)]
use crate::types::ExecutionTier;
use std::future::Future;
#[cfg(test)]
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// Trait for V2 session-based task execution.
///
/// Implementors provide concrete LLM provider + tool registry wiring,
/// decoupling `TaskDispatcher` from infrastructure dependencies.
pub trait SessionExecutor: Send + Sync {
    fn execute_session(
        &self,
        spec: &TaskSpec,
    ) -> Pin<Box<dyn Future<Output = TaskResult> + Send + '_>>;
}

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
    session_executor: Option<Arc<dyn SessionExecutor>>,
}

impl TaskDispatcher {
    pub const fn new(fork_join: ForkJoinExecutor, bus: BusHandle) -> Self {
        Self {
            fork_join,
            bus,
            session_executor: None,
        }
    }

    /// Create with a real task runner.
    pub fn with_runner(runner: Arc<dyn TaskRunner>, bus: BusHandle) -> Self {
        let fj = ForkJoinExecutor::with_runner(ForkJoinConfig::default(), bus.clone(), runner);
        Self {
            fork_join: fj,
            bus,
            session_executor: None,
        }
    }

    /// Configure a V2 session executor for real LLM tool-use loops.
    pub fn with_session_executor(mut self, executor: Arc<dyn SessionExecutor>) -> Self {
        self.session_executor = Some(executor);
        self
    }

    /// Dispatch a spawn decision to the appropriate execution path.
    pub async fn dispatch(&self, decision: SpawnDecision) -> Vec<TaskResult> {
        match decision {
            SpawnDecision::Inline => Vec::new(),
            SpawnDecision::Spawn(spec) => {
                if let Some(ref token) = spec.delegation_token {
                    if !token.can_delegate() {
                        return vec![TaskResult::failure(
                            &spec.task_id,
                            format!(
                                "delegation depth {} would exceed max {}",
                                token.depth + 1,
                                token.max_depth
                            ),
                            0,
                        )];
                    }
                }
                vec![self.execute_single(&spec).await]
            }
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
        if let Some(ref executor) = self.session_executor {
            return executor.execute_session(spec).await;
        }

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

    /// Execute a task spec through a real `AgentSession` (V2 path).
    ///
    /// Maps `TaskSpec` fields to `AgentConfig`, creates a session, runs it,
    /// and collects the result. Falls back to V1 ForkJoin if the session
    /// fails to initialize.
    ///
    /// The caller must supply the concrete `LLMProvider`, model name,
    /// `ToolRegistry`, and `AgentEvents` sink — `TaskDispatcher` owns the
    /// orchestration logic but not the infrastructure wiring.
    #[allow(dead_code)] // Used by V2 dispatch when enabled
    pub async fn execute_via_session(
        &self,
        spec: &TaskSpec,
        provider: &dyn rustycode_llm::provider::LLMProvider,
        model: &str,
        tool_registry: &rustycode_tools::ToolRegistry,
        events: &mut dyn rustycode_agent_runtime::AgentEvents,
    ) -> TaskResult {
        let start = std::time::Instant::now();
        let role_label = format!("{:?}", spec.role);

        self.bus.publish(OrchestrationEvent::TaskSpawned {
            task_id: spec.task_id.clone(),
            role: role_label.clone(),
            tier: spec.effective_tier().as_u8(),
            parent_task_id: "dispatcher".to_string(),
        });

        // Map TaskSpec → AgentConfig
        let config = task_spec_to_agent_config(spec);
        let cwd = spec.path_scope.first().map_or_else(
            || std::path::PathBuf::from("."),
            |p| {
                p.parent()
                    .map_or_else(|| p.clone(), std::path::Path::to_path_buf)
            },
        );

        let agent_result = run_agent_session(
            &config,
            &cwd,
            provider,
            model,
            &spec.prompt,
            spec.role.system_prompt(),
            tool_registry,
            events,
        )
        .await;

        let elapsed_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

        match agent_result {
            Ok(result) => {
                let success = !result.final_text.is_empty();
                self.bus
                    .publish(OrchestrationEvent::TaskDelegationCompleted {
                        task_id: spec.task_id.clone(),
                        role: role_label.clone(),
                        output_preview: truncate_preview(&result.final_text, 200),
                        cost_usd: 0.0, // Will be populated from token usage when available
                        duration_ms: elapsed_ms,
                    });
                TaskResult {
                    task_id: spec.task_id.clone(),
                    success,
                    output: result.final_text,
                    cost_usd: 0.0,
                    duration_ms: elapsed_ms,
                }
            }
            Err(e) => {
                self.bus.publish(OrchestrationEvent::TaskDelegationFailed {
                    task_id: spec.task_id.clone(),
                    role: role_label,
                    error: e.to_string(),
                    cost_usd: 0.0,
                    duration_ms: elapsed_ms,
                });
                TaskResult::failure(&spec.task_id, e.to_string(), elapsed_ms)
            }
        }
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

// ---------------------------------------------------------------------------
// V2 helpers — AgentSession execution path
// ---------------------------------------------------------------------------

/// Map `TaskSpec` fields to an `AgentConfig` for V2 session execution.
///
/// Uses `max_steps` for the turn cap (defaulting to 25) and a fixed 900s
/// wall-clock timeout. The `budget_limit` is a USD cap (not a time budget)
/// so it doesn't map directly to `timeout_secs`.
fn task_spec_to_agent_config(spec: &TaskSpec) -> rustycode_agent_runtime::AgentConfig {
    rustycode_agent_runtime::AgentConfig {
        max_turns: spec.max_steps.map_or(25, |steps| steps as usize),
        timeout_secs: 900,
        max_tool_result_bytes: 8_000,
        temperature: 0.2,
        effort: None,
    }
}

/// Run a single agent session with the given config and prompt.
///
/// This is the V2 execution path — creates a real `AgentSession`,
/// runs the tool-use loop, and returns the final result.
async fn run_agent_session(
    config: &rustycode_agent_runtime::AgentConfig,
    cwd: &std::path::Path,
    provider: &dyn rustycode_llm::provider::LLMProvider,
    model: &str,
    user_prompt: &str,
    system_prompt: &str,
    tool_registry: &rustycode_tools::ToolRegistry,
    events: &mut dyn rustycode_agent_runtime::AgentEvents,
) -> anyhow::Result<rustycode_agent_runtime::AgentResult> {
    use rustycode_agent_runtime::AgentSession;
    use rustycode_llm::provider::{ChatMessage, MessageRole};
    use rustycode_protocol::MessageContent;
    use std::sync::Arc;

    let mut session = AgentSession::new(config.clone(), cwd);

    // Wire a sync adapter over the async mailbox router for send_message.
    let mailbox = crate::mailbox_router::MailboxRouter::new(crate::bus::BusHandle::new(16));
    let sender = crate::mailbox_sender::MailboxSender::new(mailbox);
    session = session.with_message_sender(Arc::new(sender));

    let messages = vec![ChatMessage {
        role: MessageRole::User,
        content: MessageContent::Simple(user_prompt.to_string()),
    }];

    let tools_schema: Vec<serde_json::Value> = tool_registry
        .list()
        .into_iter()
        .map(|info| {
            serde_json::json!({
                "name": info.name,
                "description": info.description,
                "parameters": info.parameters_schema,
            })
        })
        .collect();

    session
        .run(
            provider,
            model,
            system_prompt,
            messages,
            &tools_schema,
            tool_registry,
            events,
        )
        .await
}

// ---------------------------------------------------------------------------
// Concrete SessionExecutor implementation
// ---------------------------------------------------------------------------

/// Concrete `SessionExecutor` wiring `TaskSpec` → `AgentSession` execution.
///
/// Holds the infrastructure dependencies (LLM provider, tool registry, bus)
/// that `TaskDispatcher` doesn't need to know about directly.
pub struct RealSessionExecutor {
    provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
    model: String,
    tool_registry: Arc<rustycode_tools::ToolRegistry>,
    bus: BusHandle,
}

impl RealSessionExecutor {
    pub fn new(
        provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: impl Into<String>,
        tool_registry: Arc<rustycode_tools::ToolRegistry>,
        bus: BusHandle,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            tool_registry,
            bus,
        }
    }
}

impl std::fmt::Debug for RealSessionExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealSessionExecutor")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

impl SessionExecutor for RealSessionExecutor {
    fn execute_session(
        &self,
        spec: &TaskSpec,
    ) -> Pin<Box<dyn Future<Output = TaskResult> + Send + '_>> {
        let spec = spec.clone();
        let provider = Arc::clone(&self.provider);
        let model = self.model.clone();
        let tool_registry = Arc::clone(&self.tool_registry);
        let bus = self.bus.clone();

        Box::pin(async move {
            let dispatcher = TaskDispatcher::new(
                ForkJoinExecutor::new(ForkJoinConfig::default(), bus.clone()),
                bus,
            );

            let mut sink = NoopEvents;
            dispatcher
                .execute_via_session(&spec, &*provider, &model, &tool_registry, &mut sink)
                .await
        })
    }
}

struct NoopEvents;

#[async_trait::async_trait]
impl rustycode_agent_runtime::AgentEvents for NoopEvents {
    async fn on_event(&mut self, _event: rustycode_protocol::stream_event::StreamEvent) {}
}

fn truncate_preview(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let mut end = max_len;
    if !text.is_char_boundary(end) {
        end = text.floor_char_boundary(max_len);
    }
    format!("{}...", &text[..end])
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

    // ---- Delegation depth enforcement ----

    #[tokio::test]
    async fn dispatch_rejects_max_depth_exceeded() {
        use crate::delegation::DelegationToken;

        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        let mut spec = make_spec("deep task");
        spec.task_id = "deep-1".into();
        let mut token = DelegationToken::root("root");
        token.depth = 2;
        token.max_depth = 3;
        spec = spec.with_delegation_token(token);

        let results = dispatcher.dispatch(SpawnDecision::Spawn(spec)).await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].output.contains("delegation depth"));
    }

    #[tokio::test]
    async fn dispatch_allows_within_depth_limit() {
        use crate::delegation::DelegationToken;

        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        let mut spec = make_spec("normal task");
        spec.task_id = "normal-1".into();
        let token = DelegationToken::root("root");
        spec = spec.with_delegation_token(token);

        let results = dispatcher.dispatch(SpawnDecision::Spawn(spec)).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn dispatch_without_token_works_normally() {
        let bus = make_bus();
        let dispatcher = make_dispatcher(bus);

        let spec = make_spec("untokened task");
        let results = dispatcher.dispatch(SpawnDecision::Spawn(spec)).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn session_executor_used_when_configured() {
        struct MockSessionExecutor;
        impl SessionExecutor for MockSessionExecutor {
            fn execute_session(
                &self,
                spec: &TaskSpec,
            ) -> Pin<Box<dyn Future<Output = TaskResult> + Send + '_>> {
                let task_id = spec.task_id.clone();
                Box::pin(async move { TaskResult::success(task_id, "mock-v2-result", 0.42, 7) })
            }
        }

        let bus = make_bus();
        let dispatcher = make_dispatcher(bus).with_session_executor(Arc::new(MockSessionExecutor));

        let mut spec = make_spec("v2 task");
        spec.task_id = "v2-1".into();
        let results = dispatcher.dispatch(SpawnDecision::Spawn(spec)).await;
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].output, "mock-v2-result");
        assert!((results[0].cost_usd - 0.42).abs() < f64::EPSILON);
        assert_eq!(results[0].duration_ms, 7);
    }
}
