//! Main orchestration pipeline.
//!
//! Coordinates the overall task lifecycle: decomposition, refined planning,
//! tiered step execution, and final validation.

use crate::bus::{BusHandle, OrchestrationEvent};
use crate::composer::Composer;
use crate::conductor::Conductor;
use crate::config::OrchestrationConfig;
use crate::editor::Editor;
use crate::error::Result;
use crate::isolation::TierIsolation;
use crate::musician::Musician;
use crate::orchestrator::StepOrchestrator;
use crate::phase_lifecycle::PhaseLifecycleManager;
use crate::reasoning_store::ReasoningStore;
use crate::shared_workspace::SharedWorkspace;
use crate::skeptic::Skeptic;
use crate::state_machine::{TaskContext, TaskPhase};
use crate::supervisor::{
    translate_event, RuleBasedSupervisor, SupervisionDirective, Supervisor, TaskSnapshot,
};
use crate::verification_gates::VerificationGateRegistry;
use chrono::Utc;
use rustycode_protocol::{
    CommandPlan, ConvoyPlan, ConvoyRisk, ExecutionPhase, PhaseSkipConfig, PlanApproval,
};
use std::sync::{Arc, LazyLock};

// Regex patterns are compile-time constants — panicking here means a bug.
#[allow(clippy::unwrap_used)]
static TOOL_CALL_RESPONSE_BLOCK: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?s)<tool_call[^>]*>.*?</tool_call<tool_response[^>]*>.*?</tool_response\s*>",
    )
    .unwrap()
});
#[allow(clippy::unwrap_used)]
static TOOL_CALL_PAIR: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?s)<tool_call[^>]*>.*?</tool_call\s*>(?:\s*<tool_response[^>]*>.*?</tool_response\s*>)?",
    )
    .unwrap()
});
#[allow(clippy::unwrap_used)]
static TOOL_RESPONSE_ONLY: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<tool_response[^>]*>.*?</tool_response\s*>").unwrap());

fn clean_tool_xml(text: &str) -> String {
    // Handle malformed closing tags first (e.g. `</tool_call<tool_response`)
    let cleaned = TOOL_CALL_RESPONSE_BLOCK.replace_all(text, "");
    // Handle well-formed pairs (tool_call + optional tool_response)
    let cleaned = TOOL_CALL_PAIR.replace_all(&cleaned, "");
    // Handle any orphaned tool_response blocks
    let cleaned = TOOL_RESPONSE_ONLY.replace_all(&cleaned, "");
    cleaned.trim().to_string()
}

/// Callback trait for interactive operations during pipeline execution.
///
/// Implementations handle approval prompts and cancellation — things that
/// require user interaction and can't go through the broadcast bus.
#[async_trait::async_trait]
pub trait PipelineInteraction: Send + Sync {
    async fn request_approval(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> rustycode_agent::ApprovalDecision;

    fn is_cancelled(&self) -> bool;
}

/// A no-op interaction that auto-approves everything and never cancels.
pub struct SilentInteraction;

#[async_trait::async_trait]
impl PipelineInteraction for SilentInteraction {
    async fn request_approval(
        &self,
        _tool_name: &str,
        _input: &serde_json::Value,
    ) -> rustycode_agent::ApprovalDecision {
        rustycode_agent::ApprovalDecision::AutoApproved
    }

    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TaskResult {
    Success {
        output: String,
        total_cost: f64,
        tier_used: u8,
        steps_completed: usize,
        execution_trace: crate::execution_trace::ExecutionTrace,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        structured_output: Option<serde_json::Value>,
    },
    Failed {
        reason: String,
        total_cost: f64,
        steps_completed: usize,
    },
}

pub struct OrchestrationPipeline {
    orchestrator: Arc<StepOrchestrator>,
    bus: BusHandle,
    workspace: Arc<SharedWorkspace>,
    supervisor: Arc<tokio::sync::Mutex<dyn Supervisor>>,
    hooks: Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>>,
    interaction: Arc<tokio::sync::Mutex<Option<Arc<dyn PipelineInteraction>>>>,
    output_schema: Option<serde_json::Value>,
    tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
    #[allow(dead_code)]
    system_prompt: Option<String>,
}

impl OrchestrationPipeline {
    pub fn new(config: OrchestrationConfig) -> Self {
        let llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider> =
            Arc::new(rustycode_llm::mock::MockProvider::from_text("mock result"));
        Self::build(config, llm_provider, None, None, None)
    }

    pub fn with_provider(
        config: OrchestrationConfig,
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
    ) -> Self {
        Self::build(config, llm_provider, None, None, None)
    }

    pub fn with_provider_and_model(
        config: OrchestrationConfig,
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: &str,
    ) -> Self {
        Self::build(config, llm_provider, Some(model), None, None)
    }

    pub fn with_provider_model_and_tools(
        config: OrchestrationConfig,
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: &str,
        tool_registry: Arc<rustycode_tools::ToolRegistry>,
    ) -> Self {
        Self::build(config, llm_provider, Some(model), Some(tool_registry), None)
    }

    pub fn with_provider_model_and_prompt(
        config: OrchestrationConfig,
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: &str,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self::build(
            config,
            llm_provider,
            Some(model),
            None,
            Some(system_prompt.into()),
        )
    }

    pub fn tool_count(&self) -> usize {
        self.tool_registry.as_ref().map_or(0, |r| r.list().len())
    }

    fn build(
        config: OrchestrationConfig,
        llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
        model: Option<&str>,
        tool_registry: Option<Arc<rustycode_tools::ToolRegistry>>,
        system_prompt: Option<String>,
    ) -> Self {
        let bus = BusHandle::new(64);
        let workspace = Arc::new(SharedWorkspace::new());
        let store = Arc::new(ReasoningStore::new(std::path::PathBuf::from(
            "/tmp/rustycode/reasoning",
        )));
        let interaction: Arc<tokio::sync::Mutex<Option<Arc<dyn PipelineInteraction>>>> =
            Arc::new(tokio::sync::Mutex::new(None));

        let isolation = Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults()));
        let hooks = Arc::new(tokio::sync::RwLock::new(
            crate::hook_points::HookRegistry::new(),
        ));

        let default_prompt =
            "You are an expert software engineer. Complete the task described by the user.";
        let prompt = system_prompt.as_deref().unwrap_or(default_prompt);

        let stored_registry = tool_registry.clone();

        let musician: Arc<Musician> = match model {
            Some(m) => {
                let registry =
                    tool_registry.unwrap_or_else(|| Arc::new(rustycode_tools::ToolRegistry::new()));
                let executor = crate::agent_executor::AgentSessionExecutor::new(
                    llm_provider.clone(),
                    registry,
                    prompt,
                    m,
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp")),
                    bus.clone(),
                )
                .with_interaction(interaction.clone());
                Arc::new(
                    Musician::with_tool_executor(Arc::new(executor))
                        .with_isolation(isolation.clone())
                        .with_hooks(hooks.clone())
                        .with_autonomy(config.autonomy.clone()),
                )
            }
            None => Arc::new(
                Musician::with_bus(bus.clone())
                    .with_isolation(isolation.clone())
                    .with_hooks(hooks.clone())
                    .with_autonomy(config.autonomy.clone()),
            ),
        };
        let editor = Arc::new(Editor::new(bus.clone()).with_isolation(isolation.clone()));
        let composer = match model {
            Some(m) => Arc::new(
                Composer::with_model(llm_provider, workspace.clone(), store, bus.clone(), m)
                    .with_isolation(isolation.clone()),
            ),
            None => Arc::new(
                Composer::new(llm_provider, workspace.clone(), store, bus.clone())
                    .with_isolation(isolation.clone()),
            ),
        };
        let conductor = Arc::new(Conductor::with_bus_and_isolation(
            config,
            bus.clone(),
            isolation,
        ));
        let verification_gate = Arc::new(VerificationGateRegistry::new());

        let skeptic = Skeptic::new(bus.clone());
        skeptic.start_monitoring();

        let orchestrator = Arc::new(
            StepOrchestrator::new(
                conductor,
                musician,
                editor,
                composer,
                verification_gate,
                bus.clone(),
            )
            .with_hooks(hooks.clone()),
        );

        Self {
            orchestrator,
            bus,
            workspace,
            supervisor: Arc::new(tokio::sync::Mutex::new(RuleBasedSupervisor::new())),
            hooks,
            interaction,
            output_schema: None,
            tool_registry: stored_registry,
            system_prompt,
        }
    }

    /// Set a JSON schema for structured output.
    ///
    /// When set, the `StructuredOutputTool` is injected into the tool registry
    /// and the final response is validated against this schema.
    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    pub fn workspace(&self) -> Arc<SharedWorkspace> {
        self.workspace.clone()
    }

    pub fn bus_handle(&self) -> BusHandle {
        self.bus.clone()
    }

    pub const fn orchestrator(&self) -> &Arc<StepOrchestrator> {
        &self.orchestrator
    }

    pub fn hooks(&self) -> Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>> {
        self.hooks.clone()
    }

    pub async fn conduct(
        &self,
        task_id: String,
        task: String,
    ) -> Result<crate::pipeline::TaskResult> {
        let mut ctx = TaskContext::new(task_id, task);
        ctx.workspace = Some(self.workspace.clone());
        ctx.phase_skip = PhaseSkipConfig::new();
        ctx.reset_execution_phase();

        let mut lifecycle = PhaseLifecycleManager::new(ctx.phase_skip);
        let starting_phase = lifecycle.current_phase();

        if starting_phase == ExecutionPhase::Explore {
            if let Err(e) = lifecycle.enter_plan() {
                tracing::warn!("Failed to enter plan phase: {e}");
            }
            if let Err(e) = ctx.transition_execution_phase(ExecutionPhase::Plan) {
                tracing::warn!("Failed to transition to Plan phase: {e}");
            }
            self.bus.publish(OrchestrationEvent::PhaseTransition {
                task_id: ctx.task_id.clone(),
                from: ExecutionPhase::Explore,
                to: ExecutionPhase::Plan,
                reason: "context gathered".to_string(),
            });
        }

        let plan = ConvoyPlan {
            id: format!("plan-{}", ctx.task_id),
            summary: ctx.original_request.clone(),
            approach: "Gather context, validate a plan, then execute the approved step."
                .to_string(),
            files_to_modify: Vec::new(),
            commands_to_run: vec![CommandPlan {
                command: "cargo test".to_string(),
                description: "Verify changes".to_string(),
            }],
            risks: vec![ConvoyRisk {
                level: rustycode_protocol::team::RiskLevel::Moderate,
                description: "Implementation may need refinement".to_string(),
                mitigation: "Validate with tests before reporting success".to_string(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec!["Implementation verified by tests".to_string()],
            approval: PlanApproval::default(),
            created_at: Utc::now(),
        };

        if let Err(e) = lifecycle.submit_plan(plan) {
            tracing::warn!("Failed to submit plan: {e}");
        }
        if let Err(e) = lifecycle.approve_plan() {
            tracing::warn!("Failed to approve plan: {e}");
        }
        if let Err(e) = ctx.transition_execution_phase(ExecutionPhase::Act) {
            tracing::warn!("Failed to transition to Act phase: {e}");
        }
        self.bus.publish(OrchestrationEvent::PhaseTransition {
            task_id: ctx.task_id.clone(),
            from: ExecutionPhase::Plan,
            to: ExecutionPhase::Act,
            reason: "plan approved".to_string(),
        });

        // Execute steps
        if let Err(e) = self.execute_steps(&mut ctx).await {
            ctx.complete(TaskPhase::Failed);
            return Ok(TaskResult::Failed {
                reason: e.to_string(),
                total_cost: ctx.cost_used,
                steps_completed: ctx.execution_trace.steps.len(),
            });
        }

        ctx.complete(TaskPhase::Completed);

        self.bus.publish(OrchestrationEvent::TaskCompleted {
            task_id: ctx.task_id.clone(),
            tier_used: ctx.current_tier,
            cost_usd: ctx.cost_used,
        });

        let output_text = ctx
            .execution_trace
            .steps
            .last()
            .map(|s| s.output.clone())
            .unwrap_or_default();

        let output_text = clean_tool_xml(&output_text);
        let structured_output = self.extract_structured_output(&output_text);

        Ok(TaskResult::Success {
            output: output_text,
            total_cost: ctx.cost_used,
            tier_used: ctx.current_tier,
            steps_completed: ctx.execution_trace.steps.len(),
            execution_trace: ctx.execution_trace.clone(),
            structured_output,
        })
    }

    pub async fn conduct_with_history(
        &self,
        task_id: String,
        task: String,
        history: Vec<(String, String)>,
        _system_prompt: &str,
    ) -> Result<crate::pipeline::TaskResult> {
        let mut ctx = TaskContext::new(task_id, task);
        ctx.workspace = Some(self.workspace.clone());
        ctx.phase_skip = PhaseSkipConfig::new();
        ctx.reset_execution_phase();

        ctx.conversation_history = history;

        let mut lifecycle = PhaseLifecycleManager::new(ctx.phase_skip);
        let starting_phase = lifecycle.current_phase();

        if starting_phase == ExecutionPhase::Explore {
            if let Err(e) = lifecycle.enter_plan() {
                tracing::warn!("Failed to enter plan phase: {e}");
            }
            if let Err(e) = ctx.transition_execution_phase(ExecutionPhase::Plan) {
                tracing::warn!("Failed to transition to Plan phase: {e}");
            }
            self.bus.publish(OrchestrationEvent::PhaseTransition {
                task_id: ctx.task_id.clone(),
                from: ExecutionPhase::Explore,
                to: ExecutionPhase::Plan,
                reason: "context gathered".to_string(),
            });
        }

        let plan = ConvoyPlan {
            id: format!("plan-{}", ctx.task_id),
            summary: ctx.original_request.clone(),
            approach: "Continue conversation with prior context.".to_string(),
            files_to_modify: Vec::new(),
            commands_to_run: vec![CommandPlan {
                command: "cargo test".to_string(),
                description: "Verify changes".to_string(),
            }],
            risks: vec![ConvoyRisk {
                level: rustycode_protocol::team::RiskLevel::Moderate,
                description: "Implementation may need refinement".to_string(),
                mitigation: "Validate with tests before reporting success".to_string(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec!["Implementation verified by tests".to_string()],
            approval: PlanApproval::default(),
            created_at: Utc::now(),
        };

        if let Err(e) = lifecycle.submit_plan(plan) {
            tracing::warn!("Failed to submit plan: {e}");
        }
        if let Err(e) = lifecycle.approve_plan() {
            tracing::warn!("Failed to approve plan: {e}");
        }
        if let Err(e) = ctx.transition_execution_phase(ExecutionPhase::Act) {
            tracing::warn!("Failed to transition to Act phase: {e}");
        }
        self.bus.publish(OrchestrationEvent::PhaseTransition {
            task_id: ctx.task_id.clone(),
            from: ExecutionPhase::Plan,
            to: ExecutionPhase::Act,
            reason: "plan approved".to_string(),
        });

        if let Err(e) = self.execute_steps(&mut ctx).await {
            ctx.complete(TaskPhase::Failed);
            return Ok(TaskResult::Failed {
                reason: e.to_string(),
                total_cost: ctx.cost_used,
                steps_completed: ctx.execution_trace.steps.len(),
            });
        }

        ctx.complete(TaskPhase::Completed);

        self.bus.publish(OrchestrationEvent::TaskCompleted {
            task_id: ctx.task_id.clone(),
            tier_used: ctx.current_tier,
            cost_usd: ctx.cost_used,
        });

        let output_text = ctx
            .execution_trace
            .steps
            .last()
            .map(|s| s.output.clone())
            .unwrap_or_default();

        let output_text = clean_tool_xml(&output_text);
        let structured_output = self.extract_structured_output(&output_text);

        Ok(TaskResult::Success {
            output: output_text,
            total_cost: ctx.cost_used,
            tier_used: ctx.current_tier,
            steps_completed: ctx.execution_trace.steps.len(),
            execution_trace: ctx.execution_trace.clone(),
            structured_output,
        })
    }

    async fn execute_steps(&self, ctx: &mut TaskContext) -> Result<()> {
        // In a full implementation, we would call the TaskDecomposer here.
        // For V1, we assume a simple single-step plan.
        let steps = vec![crate::types::Step {
            id: "step-1".into(),
            index: 0,
            description: ctx.original_request.clone(),
            expected_output_type: crate::types::OutputType::Verification,
            suggested_tool: Some("bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        }];

        for step in steps {
            match self.orchestrator.execute_step(&step, ctx).await {
                Ok(_) => {
                    tracing::info!(step_id = %step.id, "Step completed successfully");
                    if let Some(entry) = ctx.execution_trace.steps.last() {
                        ctx.add_cost(entry.cost_usd);
                    }
                }
                Err(e) => {
                    tracing::error!(step_id = %step.id, error = %e, "Step failed permanently");
                    return Err(e);
                }
            }

            // Advisory supervision — reconcile after each step and log directives.
            self.log_supervisor_directive(ctx).await;
        }
        Ok(())
    }

    /// Parse structured output from text when an output schema is configured.
    ///
    /// Gracefully falls back: if the provider already returned valid JSON
    /// via `output_config`, it passes through; otherwise text is parsed.
    fn extract_structured_output(&self, text: &str) -> Option<serde_json::Value> {
        let schema = self.output_schema.as_ref()?;
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(v) if v.is_object() => {
                if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                    for req in required {
                        if let Some(key) = req.as_str() {
                            if v.get(key).is_none() {
                                tracing::warn!(key, "Structured output missing required field");
                                return None;
                            }
                        }
                    }
                }
                Some(v)
            }
            Ok(v) => {
                tracing::warn!(
                    kind = v.to_string().chars().take(20).collect::<String>(),
                    "Structured output is not a JSON object"
                );
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "Structured output JSON parse failed");
                None
            }
        }
    }

    pub async fn conduct_streaming(
        &self,
        task_id: String,
        task: String,
        interaction: Arc<dyn PipelineInteraction>,
    ) -> Result<TaskResult> {
        {
            let mut guard = self.interaction.lock().await;
            *guard = Some(interaction);
        }
        self.conduct(task_id, task).await
    }

    /// Build a supervision snapshot from the current task context and supervisor state.
    fn build_snapshot(ctx: &TaskContext, consecutive_failures: u8) -> TaskSnapshot {
        TaskSnapshot {
            task_id: ctx.task_id.clone(),
            current_phase: ctx.current_phase.to_string(),
            current_tier: ctx.current_tier,
            cost_used: ctx.cost_used,
            budget_remaining: ctx.budget_remaining(),
            attempt_count: ctx.attempt_count,
            consecutive_failures,
            steps_completed: ctx.execution_trace.steps.len(),
            active_tools: Vec::new(),
        }
    }

    /// Run supervisor reconciliation on the current task state and log the directive.
    ///
    /// Phase 1: advisory only — directives are logged, not auto-applied.
    async fn log_supervisor_directive(&self, ctx: &TaskContext) {
        let directive = {
            let mut guard = self.supervisor.lock().await;
            let snapshot = Self::build_snapshot(ctx, guard.consecutive_failure_count());
            guard.reconcile(&snapshot)
        };
        if !matches!(directive, SupervisionDirective::Continue) {
            tracing::info!(
                task_id = %ctx.task_id,
                directive = ?directive,
                "Supervisor directive (advisory)"
            );
        }
    }

    /// Feed an orchestration bus event into the supervisor for observation.
    ///
    /// Returns the directive (if any) for callers to optionally act on.
    ///
    /// TODO: Wire bus events into this method during the step loop so that
    /// `observe()` runs alongside `reconcile()` for full supervisor coverage.
    #[allow(dead_code)]
    pub async fn observe_event(&self, event: &OrchestrationEvent) -> Option<SupervisionDirective> {
        let sup_event = translate_event(event)?;
        let mut guard = self.supervisor.lock().await;
        guard.observe(&sup_event)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    fn default_config() -> OrchestrationConfig {
        OrchestrationConfig::default()
    }

    fn default_trace() -> crate::execution_trace::ExecutionTrace {
        crate::execution_trace::ExecutionTrace::new("test".into())
    }

    #[test]
    fn test_task_result_success_serialization() {
        let result = TaskResult::Success {
            output: "done".into(),
            total_cost: 0.05,
            tier_used: 2,
            steps_completed: 3,
            execution_trace: default_trace(),
            structured_output: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }

    #[test]
    fn test_task_result_failed_serialization() {
        let result = TaskResult::Failed {
            reason: "exhausted".into(),
            total_cost: 1.0,
            steps_completed: 10,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }

    #[test]
    fn test_task_result_equality() {
        let a = TaskResult::Success {
            output: "x".into(),
            total_cost: 0.1,
            tier_used: 2,
            steps_completed: 1,
            execution_trace: default_trace(),
            structured_output: None,
        };
        let b = TaskResult::Success {
            output: "x".into(),
            total_cost: 0.1,
            tier_used: 2,
            steps_completed: 1,
            execution_trace: default_trace(),
            structured_output: None,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_task_result_inequality() {
        let a = TaskResult::Success {
            output: "x".into(),
            total_cost: 0.1,
            tier_used: 2,
            steps_completed: 1,
            execution_trace: default_trace(),
            structured_output: None,
        };
        let b = TaskResult::Failed {
            reason: "x".into(),
            total_cost: 0.1,
            steps_completed: 1,
        };
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn test_pipeline_conduct_simple_task() {
        let pipeline = OrchestrationPipeline::new(default_config());
        let result = pipeline
            .conduct("task-1".into(), "hello world".into())
            .await
            .unwrap();
        match result {
            TaskResult::Success {
                output,
                tier_used,
                steps_completed,
                ..
            } => {
                assert!(!output.is_empty() || steps_completed > 0);
                assert!(tier_used >= 2);
            }
            TaskResult::Failed { .. } => {}
        }
    }

    #[tokio::test]
    async fn test_pipeline_conduct_publishes_event() {
        let pipeline = OrchestrationPipeline::new(default_config());
        let mut rx = pipeline.bus_handle().subscribe();
        let _ = pipeline.conduct("task-2".into(), "test event".into()).await;

        let mut found = false;
        while let Ok(event) = rx.try_recv() {
            if let OrchestrationEvent::TaskCompleted { task_id, .. } = event {
                if task_id == "task-2" {
                    found = true;
                }
            }
        }
        assert!(found, "Expected TaskCompleted event");
    }

    #[tokio::test]
    async fn test_pipeline_workspace_accessible() {
        let pipeline = OrchestrationPipeline::new(default_config());
        let ws = pipeline.workspace();
        let _ = ws.read("nonexistent").await;
    }

    #[test]
    fn test_pipeline_bus_handle_clonable() {
        let pipeline = OrchestrationPipeline::new(default_config());
        let bus1 = pipeline.bus_handle();
        let bus2 = pipeline.bus_handle();
        let _ = bus1.subscribe();
        let _ = bus2.subscribe();
    }

    #[test]
    fn test_task_result_debug() {
        let result = TaskResult::Success {
            output: "done".into(),
            total_cost: 0.05,
            tier_used: 2,
            steps_completed: 3,
            execution_trace: default_trace(),
            structured_output: None,
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("Success"));
    }

    #[tokio::test]
    async fn test_pipeline_conduct_multiple_tasks() {
        let pipeline = OrchestrationPipeline::new(default_config());

        let r1 = pipeline
            .conduct("t-a".into(), "task A".into())
            .await
            .unwrap();
        let r2 = pipeline
            .conduct("t-b".into(), "task B".into())
            .await
            .unwrap();

        // Both should complete (success or failure)
        assert!(matches!(
            r1,
            TaskResult::Success { .. } | TaskResult::Failed { .. }
        ));
        assert!(matches!(
            r2,
            TaskResult::Success { .. } | TaskResult::Failed { .. }
        ));
    }

    #[test]
    fn test_task_result_failed_zero_cost() {
        let result = TaskResult::Failed {
            reason: "immediate failure".into(),
            total_cost: 0.0,
            steps_completed: 0,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: TaskResult = serde_json::from_str(&json).unwrap();
        assert_eq!(result, back);
    }

    #[test]
    fn test_clean_tool_xml_strips_malformed_tool_call_response() {
        let raw = "before<tool_call request_id=\"tool_call_1\">\n{\"name\": \"search_files\", \"arguments\": {}}\n</tool_call<tool_response request_id=\"tool_call_1\">\nsearch_files is not available.\n</tool_response>after";
        let cleaned = clean_tool_xml(raw);
        assert_eq!(cleaned, "beforeafter");
    }

    #[test]
    fn test_clean_tool_xml_strips_well_formed_tool_call() {
        let raw = "Hello<tool_call request_id=\"t1\">\n{\"name\": \"bash\"}\n</tool_call\n><tool_response request_id=\"t1\">\nok\n</tool_response\n>World";
        let cleaned = clean_tool_xml(raw);
        assert_eq!(cleaned, "HelloWorld");
    }

    #[test]
    fn test_clean_tool_xml_preserves_normal_text() {
        let text = "This is normal output with no tool calls.";
        assert_eq!(clean_tool_xml(text), text);
    }

    #[test]
    fn test_clean_tool_xml_strips_standalone_tool_response() {
        let raw = "start<tool_response request_id=\"r1\">\nsome output\n</tool_response>end";
        let cleaned = clean_tool_xml(raw);
        assert_eq!(cleaned, "startend");
    }

    #[test]
    fn test_clean_tool_xml_handles_empty_string() {
        assert_eq!(clean_tool_xml(""), "");
    }
}
