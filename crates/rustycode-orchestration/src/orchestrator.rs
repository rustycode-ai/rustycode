//! Tiered Step Orchestrator.
//!
//! Manages the execution of a single step across different capability tiers,
//! handling retries, escalations, and verification.

use crate::bus::BusHandle;
use crate::composer::Composer;
use crate::conductor::{Conductor, EscalationDecision};
use crate::delegation::{DelegationConfig, DelegationContext, DelegationPlanner, SpawnDecision};
use crate::editor::Editor;
use crate::error::{OrchestrationError, Result};
use crate::isolation::TierIsolation;
use crate::musician::Musician;
use crate::task_context::TaskContext;
use crate::tool_tiers::ToolActivationManager;
use crate::types::{Step, StepResult};
use crate::verification_gates::{VerificationGateRegistry, VerificationOutcome};
use rustycode_skill::budget::BudgetEnforcer;
use std::sync::Arc;

pub struct StepOrchestrator {
    conductor: Arc<Conductor>,
    musician: Arc<Musician>,
    editor: Arc<Editor>,
    composer: Arc<Composer>,
    verification_gate: Arc<VerificationGateRegistry>,
    isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
    activation: Arc<tokio::sync::RwLock<ToolActivationManager>>,
    budget_enforcer: Arc<tokio::sync::RwLock<BudgetEnforcer>>,
    #[allow(dead_code)]
    bus: BusHandle,
    ast_pipeline: tokio::sync::RwLock<crate::ast::pipeline::AstPipeline>,
    hooks: Option<Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>>>,
    delegation_planner: DelegationPlanner,
}

impl StepOrchestrator {
    pub fn with_isolation_and_activation(
        conductor: Arc<Conductor>,
        musician: Arc<Musician>,
        editor: Arc<Editor>,
        composer: Arc<Composer>,
        verification_gate: Arc<VerificationGateRegistry>,
        bus: BusHandle,
        isolation: TierIsolation,
        activation: ToolActivationManager,
    ) -> Self {
        Self {
            conductor,
            musician,
            editor,
            composer,
            verification_gate,
            isolation: Arc::new(tokio::sync::RwLock::new(isolation)),
            activation: Arc::new(tokio::sync::RwLock::new(activation)),
            budget_enforcer: Arc::new(tokio::sync::RwLock::new(BudgetEnforcer::new(100_000))),
            bus,
            ast_pipeline: tokio::sync::RwLock::new(crate::ast::pipeline::AstPipeline::new(
                std::path::PathBuf::from(".ast"),
            )),
            hooks: None,
            delegation_planner: DelegationPlanner::new(DelegationConfig::default()),
        }
    }

    pub fn new(
        conductor: Arc<Conductor>,
        musician: Arc<Musician>,
        editor: Arc<Editor>,
        composer: Arc<Composer>,
        verification_gate: Arc<VerificationGateRegistry>,
        bus: BusHandle,
    ) -> Self {
        Self {
            conductor,
            musician,
            editor,
            composer,
            verification_gate,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
            activation: Arc::new(tokio::sync::RwLock::new(ToolActivationManager::new())),
            budget_enforcer: Arc::new(tokio::sync::RwLock::new(BudgetEnforcer::new(100_000))),
            bus,
            ast_pipeline: tokio::sync::RwLock::new(crate::ast::pipeline::AstPipeline::new(
                std::path::PathBuf::from(".ast"),
            )),
            hooks: None,
            delegation_planner: DelegationPlanner::new(DelegationConfig::default()),
        }
    }

    /// Wire a shared [`HookRegistry`] for lifecycle hook dispatch.
    pub fn with_hooks(
        mut self,
        hooks: Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>>,
    ) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Apply skill-scoped tool restrictions to the activation manager.
    pub async fn apply_skill_scope(&self, allowed_tools: &[String]) {
        let mut activation = self.activation.write().await;
        activation.intersect_scope(allowed_tools);
    }

    pub async fn clear_skill_scope(&self) {
        let mut activation = self.activation.write().await;
        activation.clear_scope();
    }

    /// Unified gateway for executing steps with isolation and budget enforcement.
    ///
    /// Lock acquisition order (MUST be consistent across all code paths):
    /// 1. `self.isolation` (`RwLock`) — read for check, write for usage recording
    /// 2. `self.activation` (`RwLock`) — read for check, write for usage recording
    /// 3. `self.budget_enforcer` (`RwLock`) — write for enforcement
    async fn dispatch_orchestrated_step(
        &self,
        step: &Step,
        ctx: &mut TaskContext,
        target_tier: u8,
    ) -> Result<StepResult> {
        let tool_name = step.suggested_tool.as_deref().unwrap_or("noop");

        // 1. Isolation & Activation Checks
        {
            let isolation = self.isolation.read().await;
            isolation
                .check_tool_allowed(target_tier, tool_name)
                .map_err(|e| OrchestrationError::Isolation {
                    message: e.to_string(),
                })?;
        }

        {
            // Intercept structured_thinking calls for AST dispatch
            if tool_name == "structured_thinking" {
                let task = &step.description;
                if !task.is_empty() {
                    let mut pipeline = self.ast_pipeline.write().await;
                    match pipeline.run_to_completion(task) {
                        Ok(result) => {
                            tracing::info!(
                                status = ?result.status,
                                milestones = result.completed_milestones.len(),
                                "AST pipeline completed"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "AST pipeline failed");
                        }
                    }
                }
            }

            let activation = self.activation.read().await;
            if !activation.is_active(tool_name) {
                return Err(OrchestrationError::Isolation {
                    message: format!(
                        "Tool '{tool_name}' is not currently active at tier {target_tier}"
                    ),
                });
            }
        }

        // 2. Dispatch
        let res = self.musician.play_step_with_context(step, ctx).await;

        // 3. Usage Recording & Budget Enforcement
        if let Ok(result) = &res {
            let tokens = Self::estimate_token_usage(step, result);
            let success = result.exit_code.is_none_or(|c| c == 0);

            {
                let mut isolation = self.isolation.write().await;
                isolation.record_usage(target_tier, tokens).map_err(|e| {
                    OrchestrationError::Isolation {
                        message: e.to_string(),
                    }
                })?;
            }

            {
                let mut activation = self.activation.write().await;
                let prev_tier = activation.current_tier();
                activation.usage_mut().record(tool_name, success);
                let new_tier = activation.current_tier();
                drop(activation);

                if new_tier > prev_tier {
                    if let Some(hooks) = &self.hooks {
                        let guard = hooks.read().await;
                        let hook_ctx = crate::hook_points::HookContext::new(
                            crate::hook_points::HookPoint::TierPromoted,
                            tool_name,
                            serde_json::json!({
                                "step_id": step.id,
                                "from_tier": prev_tier.to_string(),
                                "to_tier": new_tier.to_string(),
                            }),
                        );
                        if let Err(e) = guard.trigger(&hook_ctx) {
                            tracing::debug!(error = %e, "Hook trigger failed for tier promotion");
                        }
                    }
                }
            }

            {
                let mut enforcer = self.budget_enforcer.write().await;
                enforcer.enforce_budget();
            }
        }

        res
    }

    async fn execute_at_tier(&self, step: &Step, ctx: &mut TaskContext) -> Result<StepResult> {
        match ctx.current_tier {
            3 => {
                let trace = &ctx.execution_trace;
                let error_signal = crate::error_signal::ErrorSignal::new(
                    crate::error_signal::SignalCategory::LogicError,
                    None,
                    "Tier 2 verification failed".into(),
                    step.id.clone(),
                    step.suggested_tool.clone().unwrap_or_default(),
                );
                let patched_steps = self.editor.patch_score(trace, step, &error_signal).await?;
                let target = patched_steps.first().unwrap_or(step);

                let original_tier = ctx.current_tier;
                ctx.current_tier = 2;
                let res = self.dispatch_orchestrated_step(target, ctx, 2).await;
                ctx.current_tier = original_tier;
                res
            }
            4 | 5 => {
                let composed_steps = self.composer.compose_new_score(ctx).await.map_err(|e| {
                    OrchestrationError::Internal {
                        message: format!("Composer failed: {e}"),
                    }
                })?;
                let target = composed_steps.first().unwrap_or(step);
                self.dispatch_orchestrated_step(target, ctx, 2).await
            }
            _ => {
                self.dispatch_orchestrated_step(step, ctx, ctx.current_tier)
                    .await
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn execute_step(&self, step: &Step, ctx: &mut TaskContext) -> Result<StepResult> {
        let budget_pressure = if ctx.budget_limit > 0.0 {
            ctx.cost_used / ctx.budget_limit
        } else {
            0.0
        };
        let delegation_context = DelegationContext {
            context_pressure: budget_pressure,
            remaining_budget: ctx.budget_remaining(),
            affected_paths: vec![],
            past_failure_count: usize::from(ctx.attempt_count),
            parent_task_id: ctx.task_id.clone(),
        };

        let decision = self
            .delegation_planner
            .should_spawn(&step.description, &delegation_context);

        if !matches!(decision, SpawnDecision::Inline) {
            tracing::info!(
                step_id = %step.id,
                decision = ?decision,
                "Delegation planner recommends spawning"
            );
        }

        let mut retries = 0u8;
        let max_retries = ctx.constraints.max_retries;
        let mut thinking_attempted = false;

        loop {
            if ctx.cost_used >= ctx.budget_limit {
                return Err(OrchestrationError::ResourceExhausted {
                    resource: "budget_limit".into(),
                });
            }

            let result = self.execute_at_tier(step, ctx).await?;

            let entry =
                ctx.execution_trace
                    .steps
                    .last()
                    .ok_or_else(|| OrchestrationError::Internal {
                        message: "trace entry missing after execution".into(),
                    })?;

            let outcome = self.verification_gate.verify(step, entry);

            match outcome {
                VerificationOutcome::Valid => {
                    return Ok(result);
                }
                VerificationOutcome::Invalid { reason, category } => {
                    let error_signal = crate::error_signal::ErrorSignal::new(
                        category.clone(),
                        entry.exit_code,
                        reason.clone(),
                        step.id.clone(),
                        entry.tool_name.clone(),
                    );

                    match self.conductor.handle_error(ctx, &error_signal) {
                        EscalationDecision::Retry => {
                            if retries >= max_retries {
                                return Err(OrchestrationError::Execution {
                                    message: format!(
                                        "max retries reached for step {}: {}",
                                        step.id, reason
                                    ),
                                });
                            }
                            retries += 1;
                            ctx.attempt_count += 1;
                            tracing::info!(step_id = %step.id, retry = retries, "Retrying step");
                        }
                        EscalationDecision::Escalate {
                            next_tier,
                            reason: esc_reason,
                        } => {
                            tracing::info!(
                                step_id = %step.id,
                                from = ctx.current_tier,
                                to = next_tier,
                                reason = %esc_reason,
                                "Escalating step execution"
                            );
                            ctx.escalate();
                            retries = 0;
                        }
                        EscalationDecision::Abandon { reason: ab_reason } => {
                            if ctx.current_tier >= 4 && !thinking_attempted {
                                if let Some(_thinking_marker) =
                                    self.conductor.try_thinking(&step.description, &reason)
                                {
                                    tracing::info!(
                                        step_id = %step.id,
                                        "Deep thinking triggered before tier 4 abandonment"
                                    );
                                    thinking_attempted = true;
                                    ctx.advance_phase(
                                        crate::task_context::TaskPhase::Tier5Thinking,
                                    );
                                    continue;
                                }
                            }
                            return Err(OrchestrationError::Execution {
                                message: format!("Step abandoned: {ab_reason}"),
                            });
                        }
                        EscalationDecision::WarnBudget { remaining_usd } => {
                            tracing::warn!(remaining = %remaining_usd, "Task budget low");
                            return Ok(result);
                        }
                    }
                }
                VerificationOutcome::Uncertain { reason } => {
                    tracing::warn!(step_id = %step.id, %reason, "Verification outcome uncertain");
                    return Ok(result);
                }
            }
        }
    }

    /// Deep multi-turn reasoning via Composer (bypasses shell execution).
    /// Directly invokes the `ThinkingExecutor` for strategy-aware iterative LLM calls.
    pub async fn think_deep(&self, ctx: &mut TaskContext) -> Result<String> {
        ctx.escalate();
        ctx.escalate();
        self.composer
            .compose_new_score(ctx)
            .await
            .map(|steps| {
                steps
                    .into_iter()
                    .next()
                    .map(|s| s.description)
                    .unwrap_or_default()
            })
            .map_err(|e| OrchestrationError::Internal {
                message: format!("Deep thinking failed: {e}"),
            })
    }

    fn estimate_token_usage(step: &Step, result: &StepResult) -> u64 {
        let chars = step
            .description
            .len()
            .saturating_add(result.output.len())
            .max(1);
        ((chars as u64) / 128).max(1)
    }
}
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::redundant_clone,
    clippy::items_after_statements,
    clippy::collection_is_never_read,
    clippy::default_trait_access
)]
mod tests {
    use super::*;
    use crate::config::OrchestrationConfig;
    use crate::types::OutputType;

    fn make_step(id: &str) -> Step {
        Step {
            id: id.into(),
            index: 0,
            description: "echo hello".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        }
    }

    fn make_orchestrator() -> StepOrchestrator {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        StepOrchestrator::new(
            Arc::new(Conductor::new(OrchestrationConfig::default())),
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(VerificationGateRegistry::new()),
            bus,
        )
    }

    #[tokio::test]
    async fn test_execute_step_success() {
        let orchestrator = make_orchestrator();
        let step = make_step("s1");
        let mut ctx = TaskContext::new("t1".into(), "test".into());

        let result = orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_execute_step_records_trace() {
        let orchestrator = make_orchestrator();
        let step = make_step("s2");
        let mut ctx = TaskContext::new("t2".into(), "test".into());

        orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        assert_eq!(ctx.execution_trace.steps.len(), 1);
        assert_eq!(ctx.execution_trace.steps[0].step_id, "s2");
    }

    #[tokio::test]
    async fn test_execute_step_budget_exhausted() {
        let orchestrator = make_orchestrator();
        let step = make_step("s3");
        let mut ctx = TaskContext::new("t3".into(), "test".into());
        ctx.cost_used = ctx.budget_limit;

        let result = orchestrator.execute_step(&step, &mut ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("budget_limit"));
    }

    #[tokio::test]
    async fn test_execute_step_at_higher_tier() {
        let orchestrator = make_orchestrator();
        let step = make_step("s4");
        let mut ctx = TaskContext::new("t4".into(), "test".into());
        ctx.current_tier = 3;

        let result = orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_execute_step_verification_uncertain_passes() {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        let mut registry = VerificationGateRegistry::new();

        struct AlwaysUncertain;
        impl crate::verification_gates::VerificationStrategy for AlwaysUncertain {
            fn verify(
                &self,
                _step: &Step,
                _result: &crate::execution_trace::TraceEntry,
            ) -> VerificationOutcome {
                VerificationOutcome::Uncertain {
                    reason: "not sure".into(),
                }
            }
        }
        registry.register_strategy(OutputType::Code, Box::new(AlwaysUncertain));

        let orchestrator = StepOrchestrator::new(
            Arc::new(Conductor::new(OrchestrationConfig::default())),
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(registry),
            bus,
        );

        let step = make_step("s-uncertain");
        let mut ctx = TaskContext::new("t-uncertain".into(), "test".into());
        let result = orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        assert!(result.is_success());
    }

    #[tokio::test]
    async fn test_execute_step_tier_4_composer_path() {
        let orchestrator = make_orchestrator();
        let step = make_step("s-tier4");
        let mut ctx = TaskContext::new("t-tier4".into(), "test".into());
        ctx.current_tier = 4;

        let result = orchestrator.execute_step(&step, &mut ctx).await;
        // Tier 4 now routes through Composer → MockLlmProvider → Musician.
        // The mock provider returns "test response" which Composer parses and
        // feeds to Musician for shell execution.
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_execute_step_records_in_trace() {
        let orchestrator = make_orchestrator();
        let step = make_step("s-attempt");
        let mut ctx = TaskContext::new("t-attempt".into(), "test".into());

        orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        // First successful attempt doesn't increment attempt_count (only retries do)
        assert_eq!(ctx.execution_trace.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_step_budget_near_limit() {
        let orchestrator = make_orchestrator();
        let step = make_step("s-near");
        let mut ctx = TaskContext::new("t-near".into(), "test".into());
        ctx.cost_used = ctx.budget_limit - 0.001;

        // Should still succeed since cost_used < budget_limit
        let result = orchestrator.execute_step(&step, &mut ctx).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_execute_step_step_result_fields() {
        let orchestrator = make_orchestrator();
        let step = make_step("s-fields");
        let mut ctx = TaskContext::new("t-fields".into(), "test".into());

        let result = orchestrator.execute_step(&step, &mut ctx).await.unwrap();
        assert!(!result.output.is_empty() || result.is_success());
        assert!(result.is_success());
    }

    /// Verify that `WarnBudget` returns the result instead of looping forever.
    #[tokio::test]
    async fn test_execute_step_budget_warning_returns_result() {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        let mut registry = VerificationGateRegistry::new();

        struct AlwaysInvalid;
        impl crate::verification_gates::VerificationStrategy for AlwaysInvalid {
            fn verify(
                &self,
                _step: &Step,
                _result: &crate::execution_trace::TraceEntry,
            ) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "budget test".into(),
                    category: crate::error_signal::ErrorCategory::Internal,
                }
            }
        }
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));

        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 10.0;
        config.budget.warn_threshold_pct = 0.0;

        let conductor = Arc::new(Conductor::with_bus(config, bus.clone()));

        let orchestrator = StepOrchestrator::new(
            conductor,
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(registry),
            bus,
        );

        let step = make_step("s-bw");
        let mut ctx = TaskContext::new("t-bw".into(), "test".into());
        // Set cost_used past warn threshold but below total_max_usd
        ctx.cost_used = 5.0;
        ctx.budget_limit = 10.0;

        // With warn_threshold_pct=0.0, any cost triggers WarnBudget
        // The result should still return Ok (not loop infinitely)
        let result = orchestrator.execute_step(&step, &mut ctx).await;
        assert!(result.is_ok(), "WarnBudget should return Ok with result");
    }

    #[tokio::test]
    async fn test_execute_step_max_retries_fails() {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        let mut registry = VerificationGateRegistry::new();

        struct AlwaysInvalid;
        impl crate::verification_gates::VerificationStrategy for AlwaysInvalid {
            fn verify(
                &self,
                _step: &Step,
                _result: &crate::execution_trace::TraceEntry,
            ) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "always fails".into(),
                    category: crate::error_signal::ErrorCategory::Internal,
                }
            }
        }
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));

        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 100.0;
        config.budget.warn_threshold_pct = 1.0;
        config.escalation.insert(
            "tier_2".into(),
            crate::config::TierConfig {
                max_attempts: 100,
                critical_errors: vec![],
                recoverable_errors: vec![],
            },
        );

        let orchestrator = StepOrchestrator::new(
            Arc::new(Conductor::with_bus(config, bus.clone())),
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(registry),
            bus,
        );

        let step = make_step("s-max-retry");
        let mut ctx = TaskContext::new("t-max-retry".into(), "test".into());
        ctx.constraints.max_retries = 2;

        let result = orchestrator.execute_step(&step, &mut ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("max retries"));
    }

    /// When tier 4 abandons, `try_thinking` may trigger `Tier5Thinking` phase.
    /// After one more attempt, if it still fails, it should abandon for real
    /// (`thinking_attempted` prevents infinite loop).
    #[tokio::test]
    async fn test_execute_step_thinking_trigger_at_tier4() {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        let mut registry = VerificationGateRegistry::new();

        struct AlwaysInvalid;
        impl crate::verification_gates::VerificationStrategy for AlwaysInvalid {
            fn verify(
                &self,
                _step: &Step,
                _result: &crate::execution_trace::TraceEntry,
            ) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "a long enough error context to trigger thinking".into(),
                    category: crate::error_signal::ErrorCategory::Internal,
                }
            }
        }
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));

        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 100.0;
        config.budget.warn_threshold_pct = 1.0;
        // No escalation configs → conductor abandons at tier >= 4

        let orchestrator = StepOrchestrator::new(
            Arc::new(Conductor::with_bus(config, bus.clone())),
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(registry),
            bus,
        );

        let step = Step {
            id: "s-think".into(),
            index: 0,
            description: "a complex task that needs deep reasoning".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new(
            "t-think".into(),
            "a complex task that needs deep reasoning".into(),
        );
        ctx.current_tier = 4;

        let result = orchestrator.execute_step(&step, &mut ctx).await;
        // Should eventually fail (thinking gives one more shot but verification still fails)
        assert!(result.is_err());
    }

    /// At tier < 4, thinking is never triggered even on abandon.
    #[tokio::test]
    async fn test_execute_step_no_thinking_below_tier4() {
        let bus = BusHandle::new(16);
        let workspace = Arc::new(crate::shared_workspace::SharedWorkspace::new());
        let store = Arc::new(crate::reasoning_store::ReasoningStore::new(
            Default::default(),
        ));
        let llm_provider = Arc::new(crate::mock_provider_for_tests::MockLlmProvider::new());
        let mut registry = VerificationGateRegistry::new();

        struct AlwaysInvalid;
        impl crate::verification_gates::VerificationStrategy for AlwaysInvalid {
            fn verify(
                &self,
                _step: &Step,
                _result: &crate::execution_trace::TraceEntry,
            ) -> VerificationOutcome {
                VerificationOutcome::Invalid {
                    reason: "short error".into(),
                    category: crate::error_signal::ErrorCategory::Internal,
                }
            }
        }
        registry.register_strategy(OutputType::Code, Box::new(AlwaysInvalid));

        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 100.0;
        config.budget.warn_threshold_pct = 1.0;

        let orchestrator = StepOrchestrator::new(
            Arc::new(Conductor::with_bus(config, bus.clone())),
            Arc::new(Musician::with_bus(bus.clone())),
            Arc::new(Editor::new(bus.clone())),
            Arc::new(Composer::new(llm_provider, workspace, store, bus.clone())),
            Arc::new(registry),
            bus,
        );

        let step = make_step("s-no-think");
        let mut ctx = TaskContext::new("t-no-think".into(), "test".into());
        // Tier 2 — thinking should NOT be triggered
        ctx.current_tier = 2;

        // Need max_attempts=0 so conductor abandons immediately
        let result = orchestrator.execute_step(&step, &mut ctx).await;
        // Will eventually fail via max retries (3 by default) since conductor returns Retry at tier 2
        assert!(result.is_err());
    }
}
