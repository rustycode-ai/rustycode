use crate::error::{OrchestrationError, Result};
use crate::execution_trace::{ExecutionTrace, TraceEntry};
use crate::guard::{LockManager, LockResult, ResourceGuard};
use crate::hook_points::{HookContext, HookPoint, HookResult};
use crate::isolation::TierIsolation;
use crate::state_machine::TaskContext;
use crate::types::{Step, StepResult};
use rustycode_protocol::agent_protocol::AgentRole;
use std::path::PathBuf;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(
        &self,
        task_id: &str,
        tool_name: &str,
        input: &str,
        allowed_tools: &[&'static str],
        model: &str,
    ) -> Result<StepResult>;
}

use rustycode_tools_security::sandbox::{Sandbox, SandboxConfig, SandboxLevel};

pub struct ShellToolExecutor {
    pub cwd: std::path::PathBuf,
}

#[async_trait::async_trait]
impl ToolExecutor for ShellToolExecutor {
    async fn execute(
        &self,
        _task_id: &str,
        tool_name: &str,
        input: &str,
        allowed_tools: &[&'static str],
        _model: &str,
    ) -> Result<StepResult> {
        if !allowed_tools.contains(&tool_name) {
            return Err(OrchestrationError::ToolExecution(format!(
                "tool '{tool_name}' is not allowed for this role"
            )));
        }

        // Initialize sandbox
        let mut sandbox = Sandbox::new(
            self.cwd.clone(),
            &SandboxConfig::default(),
            SandboxLevel::Strict,
        );
        sandbox
            .enforce()
            .map_err(|e| OrchestrationError::execution(e.to_string()))?;

        match tool_name {
            "Bash" | "sh" => {
                // Validate input for injection attempts before shell interpretation
                if let Err(e) = rustycode_tools::security::validation::validate_command_arg(input) {
                    return Err(OrchestrationError::ToolExecution(format!(
                        "command validation failed: {e}"
                    )));
                }

                let output = rustycode_tools::subprocess::new_tokio_shell_command(input)
                    .output()
                    .await?;
                let mut rendered = String::from_utf8_lossy(&output.stdout).to_string();
                if !output.stderr.is_empty() {
                    rendered.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                Ok(StepResult {
                    output: rendered,
                    exit_code: output.status.code(),
                })
            }
            "noop" => Ok(StepResult {
                output: input.to_string(),
                exit_code: Some(0),
            }),
            _ => Err(OrchestrationError::ToolExecution(format!(
                "no executor registered for tool '{tool_name}'"
            ))),
        }
    }
}

pub struct Musician {
    tool_executor: Arc<dyn ToolExecutor>,
    lock_manager: LockManager,
    bus: crate::bus::BusHandle,
    isolation: Arc<tokio::sync::RwLock<TierIsolation>>,
    hooks: Option<Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>>>,
    autonomy: crate::autonomy::AutonomyConfig,
}

impl Musician {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|e| {
            tracing::warn!("current_dir() failed, falling back to '.': {e}");
            PathBuf::from(".")
        });
        Self {
            tool_executor: Arc::new(ShellToolExecutor { cwd }),
            lock_manager: LockManager::in_memory(),
            bus: crate::bus::BusHandle::new(16),
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
            hooks: None,
            autonomy: crate::autonomy::AutonomyConfig::default(),
        }
    }

    pub fn with_bus(bus: crate::bus::BusHandle) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|e| {
            tracing::warn!("current_dir() failed, falling back to '.': {e}");
            PathBuf::from(".")
        });
        Self {
            tool_executor: Arc::new(ShellToolExecutor { cwd }),
            lock_manager: LockManager::in_memory(),
            bus,
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
            hooks: None,
            autonomy: crate::autonomy::AutonomyConfig::default(),
        }
    }

    pub fn with_tool_executor(tool_executor: Arc<dyn ToolExecutor>) -> Self {
        Self {
            tool_executor,
            lock_manager: LockManager::in_memory(),
            bus: crate::bus::BusHandle::new(16),
            isolation: Arc::new(tokio::sync::RwLock::new(TierIsolation::with_defaults())),
            hooks: None,
            autonomy: crate::autonomy::AutonomyConfig::default(),
        }
    }

    pub fn with_lock_manager(mut self, lm: LockManager) -> Self {
        self.lock_manager = lm;
        self
    }

    /// Allow wiring a shared [`TierIsolation`] between orchestration components.
    pub fn with_isolation(mut self, isolation: Arc<tokio::sync::RwLock<TierIsolation>>) -> Self {
        self.isolation = isolation;
        self
    }

    /// Wire a shared [`HookRegistry`] for lifecycle hook dispatch.
    pub fn with_hooks(
        mut self,
        hooks: Arc<tokio::sync::RwLock<crate::hook_points::HookRegistry>>,
    ) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Configure autonomy level for tool permission gating.
    pub fn with_autonomy(mut self, config: crate::autonomy::AutonomyConfig) -> Self {
        self.autonomy = config;
        self
    }

    pub const fn lock_manager(&self) -> &LockManager {
        &self.lock_manager
    }

    #[tracing::instrument(skip(self, trace), fields(step = %step.description))]
    pub async fn play_step(&self, step: &Step, trace: &mut ExecutionTrace) -> Result<StepResult> {
        let mut ctx = TaskContext::new(trace.task_id.clone(), step.description.clone());
        self.play_step_with_context(step, &mut ctx).await?;
        if let Some(entry) = ctx.execution_trace.steps.last().cloned() {
            trace.append(entry);
        }
        Ok(ctx.execution_trace.steps.last().map_or_else(
            || StepResult {
                output: String::new(),
                exit_code: Some(1),
            },
            |entry| StepResult {
                output: entry.output.clone(),
                exit_code: entry.exit_code,
            },
        ))
    }

    #[allow(clippy::unused_async)]
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self, ctx), fields(step = %step.description))]
    pub async fn play_step_with_context(
        &self,
        step: &Step,
        ctx: &mut TaskContext,
    ) -> Result<StepResult> {
        // Acquire resource locks if the step declares requirements
        let _guard = self.acquire_step_resources(step, &ctx.task_id)?;

        let tool_name = step.suggested_tool.as_deref().unwrap_or("noop");
        let allowed_tools = tools_for_role(ctx.agent_role);

        // Enforce tier isolation policy for tool capability
        {
            let iso = self.isolation.read().await;
            iso.check_tool_allowed(ctx.current_tier, tool_name)
                .map_err(|e| OrchestrationError::isolation(e.to_string()))?;
        }

        // Autonomy enforcement
        {
            let decider = crate::autonomy::AutonomyDecider::new(&self.autonomy);
            let category = crate::autonomy::TaskTypeClassifier::classify(&ctx.original_request);
            let decision = decider.decide(tool_name, category);
            match decision {
                crate::autonomy::AutonomyDecision::Blocked { reason } => {
                    return Err(OrchestrationError::execution(reason));
                }
                crate::autonomy::AutonomyDecision::RequireApproval { reason } => {
                    tracing::info!(%tool_name, %reason, "autonomy requires approval");
                }
                crate::autonomy::AutonomyDecision::AllowWithNotification { message, .. } => {
                    tracing::info!(%message, "autonomy notification");
                }
                crate::autonomy::AutonomyDecision::Allow { .. } => {}
            }
        }

        // Fire PreToolUse hook (with veto support)
        if let Some(hooks) = &self.hooks {
            let guard = hooks.read().await;
            let hook_ctx = HookContext::new(
                HookPoint::PreToolUse,
                tool_name,
                serde_json::json!({
                    "step_id": step.id,
                    "tier": ctx.current_tier,
                }),
            );
            if let Ok(results) = guard.trigger(&hook_ctx) {
                if let Some(HookResult::Abort(reason)) = results
                    .into_iter()
                    .find(|r| matches!(r, HookResult::Abort(_)))
                {
                    return Err(OrchestrationError::HookVeto {
                        reason: reason.unwrap_or_else(|| "hook vetoed".into()),
                    });
                }
            }
        }

        let result = match self
            .tool_executor
            .execute(
                &ctx.task_id,
                tool_name,
                &step.description,
                &allowed_tools,
                "",
            )
            .await
        {
            Ok(r) => {
                // Record tool call to worker registry for visibility
                let target = step.description.split_whitespace().next().unwrap_or("");
                if let Err(e) = crate::worker_registry::global_worker_registry().record_tool_call(
                    &ctx.task_id,
                    tool_name,
                    target,
                ) {
                    tracing::debug!(task_id = %ctx.task_id, tool = %tool_name, "registry record failed: {e}");
                }
                r
            }
            Err(e) => {
                // Fire ToolError hook
                if let Some(hooks) = &self.hooks {
                    let guard = hooks.read().await;
                    let hook_ctx = HookContext::new(
                        HookPoint::ToolError,
                        tool_name,
                        serde_json::json!({
                            "step_id": step.id,
                            "error": e.to_string(),
                        }),
                    );
                    if let Err(e) = guard.trigger(&hook_ctx) {
                        tracing::warn!(error = %e, "Hook trigger failed for ToolError");
                    }
                }
                return Err(e);
            }
        };

        // Fire PostToolUse hook
        if let Some(hooks) = &self.hooks {
            let guard = hooks.read().await;
            let hook_ctx = HookContext::new(
                HookPoint::PostToolUse,
                tool_name,
                serde_json::json!({
                    "step_id": step.id,
                    "exit_code": result.exit_code,
                }),
            );
            if let Err(e) = guard.trigger(&hook_ctx) {
                tracing::warn!(error = %e, "Hook trigger failed for PostToolUse");
            }
        }

        // Estimate token usage and record it against the tier budget.
        let tokens_used = std::cmp::max(
            1,
            rustycode_protocol::estimate_tokens(&result.output) as u64,
        );
        {
            let mut iso = self.isolation.write().await;
            iso.record_usage(ctx.current_tier, tokens_used)
                .map_err(|e| OrchestrationError::isolation(e.to_string()))?;
        }

        // Update task context token count
        ctx.add_tokens(tokens_used);

        let entry = if result.exit_code == Some(0) || result.exit_code.is_none() {
            TraceEntry::new_success(
                step.id.clone(),
                step.index,
                ctx.current_tier,
                tool_name.to_string(),
                serde_json::json!({
                    "description": step.description,
                    "allowed_tools": allowed_tools,
                    "tier": ctx.current_tier,
                    "agent_role": ctx.agent_role,
                }),
                result.output.clone(),
                result.exit_code,
                0.001,
            )
        } else {
            let error_signal = crate::error_signal::ErrorSignal::new(
                crate::error_signal::ErrorCategory::Custom(format!(
                    "ExitCode{}",
                    result.exit_code.unwrap_or(1)
                )),
                result.exit_code,
                result.output.clone(),
                step.id.clone(),
                tool_name.to_string(),
            );
            TraceEntry::new_failure(
                step.id.clone(),
                step.index,
                ctx.current_tier,
                tool_name.to_string(),
                serde_json::json!({
                    "description": step.description,
                    "allowed_tools": allowed_tools,
                    "tier": ctx.current_tier,
                    "agent_role": ctx.agent_role,
                }),
                result.output.clone(),
                result.exit_code,
                error_signal,
                0.001,
            )
        };
        ctx.execution_trace.append(entry);

        self.bus
            .publish(crate::bus::OrchestrationEvent::PartialResult {
                step_id: step.id.clone(),
                content: result.output.clone(),
            });

        Ok(result)
    }

    fn acquire_step_resources(&self, step: &Step, holder: &str) -> Result<Option<ResourceGuard>> {
        if step.required_resources.resources.is_empty() {
            return Ok(None);
        }
        match self
            .lock_manager
            .try_acquire(holder, &step.required_resources)
        {
            LockResult::Acquired(guard) => {
                tracing::debug!(
                    step_id = %step.id,
                    holder,
                    count = step.required_resources.resources.len(),
                    "Step resources acquired"
                );
                Ok(Some(guard))
            }
            LockResult::Conflicted {
                resource,
                holder: conflict_holder,
            } => {
                tracing::warn!(
                    step_id = %step.id,
                    holder,
                    %resource,
                    %conflict_holder,
                    "Step blocked by resource conflict"
                );
                Err(OrchestrationError::ResourceExhausted {
                    resource: format!("{resource} (held by {conflict_holder})"),
                })
            }
        }
    }
}

impl Default for Musician {
    fn default() -> Self {
        Self::new()
    }
}

fn tools_for_role(role: AgentRole) -> Vec<&'static str> {
    match role {
        AgentRole::Planner
        | AgentRole::Architect
        | AgentRole::Researcher
        | AgentRole::Coordinator
        | AgentRole::Reviewer
        | AgentRole::Skeptic
        | AgentRole::Judge
        | AgentRole::Worker
        | AgentRole::Builder
        | AgentRole::Scalpel => vec!["noop", "Bash", "sh"],
        _ => vec!["noop"],
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::redundant_closure,
    clippy::match_wildcard_for_single_variants,
    clippy::items_after_statements,
    clippy::manual_let_else
)]
mod tests {
    use super::*;
    use crate::types::OutputType;

    fn make_step(id: &str, tool: Option<&str>) -> Step {
        Step {
            id: id.into(),
            index: 0,
            description: "test step".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: tool.map(Into::into),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::new(),
        }
    }

    #[tokio::test]
    async fn test_play_step_appends_trace() {
        let musician = Musician::new();
        let step = make_step("s1", Some("Bash"));
        let mut trace = ExecutionTrace::new("t1".into());
        musician.play_step(&step, &mut trace).await.unwrap();
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].tool_name, "Bash");
    }

    #[tokio::test]
    async fn test_play_step_default_tool() {
        let musician = Musician::new();
        let step = make_step("s2", None);
        let mut trace = ExecutionTrace::new("t2".into());
        musician.play_step(&step, &mut trace).await.unwrap();
        assert_eq!(trace.steps[0].tool_name, "noop");
    }

    #[tokio::test]
    async fn test_play_step_returns_success() {
        let musician = Musician::new();
        let step = make_step("s3", Some("Bash"));
        let mut trace = ExecutionTrace::new("t3".into());
        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_play_step_uses_correct_tier() {
        let musician = Musician::new();
        let step = make_step("s4", Some("Bash"));
        let mut trace = ExecutionTrace::new("t4".into());
        musician.play_step(&step, &mut trace).await.unwrap();
        assert_eq!(trace.steps[0].tier, 2);
    }

    #[test]
    fn test_musician_default() {
        let m = Musician::default();
        let _ = &m;
    }

    struct MockExecutor {
        output: String,
        exit_code: Option<i32>,
    }

    #[async_trait::async_trait]
    impl ToolExecutor for MockExecutor {
        async fn execute(
            &self,
            _task_id: &str,
            tool_name: &str,
            _input: &str,
            allowed_tools: &[&'static str],
            _model: &str,
        ) -> Result<StepResult> {
            if !allowed_tools.contains(&tool_name) {
                return Err(OrchestrationError::ToolExecution(format!(
                    "tool '{tool_name}' not allowed"
                )));
            }
            Ok(StepResult {
                output: self.output.clone(),
                exit_code: self.exit_code,
            })
        }
    }

    #[tokio::test]
    async fn test_with_tool_executor_custom() {
        let executor = Arc::new(MockExecutor {
            output: "custom result".into(),
            exit_code: Some(0),
        });
        let musician = Musician::with_tool_executor(executor);
        let step = make_step("s-custom", Some("Bash"));
        let mut trace = ExecutionTrace::new("t-custom".into());
        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert_eq!(result.output, "custom result");
    }

    #[tokio::test]
    async fn test_play_step_with_context() {
        let musician = Musician::new();
        let step = make_step("s-ctx", Some("Bash"));
        let mut ctx = TaskContext::new("t-ctx".into(), "test".into());
        let result = musician
            .play_step_with_context(&step, &mut ctx)
            .await
            .unwrap();
        assert!(result.is_success());
        assert_eq!(ctx.execution_trace.steps.len(), 1);
    }

    #[tokio::test]
    async fn test_play_step_with_context_tracks_tier() {
        let musician = Musician::new();
        let step = make_step("s-tier", Some("Bash"));
        let mut ctx = TaskContext::new("t-tier".into(), "test".into());
        musician
            .play_step_with_context(&step, &mut ctx)
            .await
            .unwrap();
        assert_eq!(ctx.execution_trace.steps[0].tier, 2);
    }

    #[tokio::test]
    async fn test_shell_tool_executor_bash() {
        let executor = ShellToolExecutor {
            cwd: std::env::current_dir().unwrap_or_default(),
        };
        let result = executor
            .execute("t1", "Bash", "echo test_output", &["Bash", "sh"], "model")
            .await
            .unwrap();
        assert!(result.output.contains("test_output"));
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_tool_executor_noop() {
        let executor = ShellToolExecutor {
            cwd: std::env::current_dir().unwrap_or_default(),
        };
        let result = executor
            .execute("t1", "noop", "hello", &["noop"], "model")
            .await
            .unwrap();
        assert_eq!(result.output, "hello");
        assert_eq!(result.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_shell_tool_executor_disallowed_tool() {
        let executor = ShellToolExecutor {
            cwd: std::env::current_dir().unwrap_or_default(),
        };
        let result = executor
            .execute("t1", "python", "code", &["Bash"], "model")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shell_tool_executor_unknown_tool() {
        let executor = ShellToolExecutor {
            cwd: std::env::current_dir().unwrap_or_default(),
        };
        let result = executor
            .execute("t1", "unknown_tool", "input", &["unknown_tool"], "model")
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tools_for_role() {
        let planner_tools = tools_for_role(AgentRole::Planner);
        assert!(planner_tools.contains(&"Bash"));

        let reviewer_tools = tools_for_role(AgentRole::Reviewer);
        assert!(reviewer_tools.contains(&"Bash"));
    }

    #[tokio::test]
    async fn test_step_with_resources_acquires_lock() {
        let musician = Musician::new();
        let lm = musician.lock_manager().clone();
        let step = Step {
            id: "s-res".into(),
            index: 0,
            description: "echo hello".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::new()
                .read("/tmp/musician-test-read.rs"),
        };
        let mut trace = ExecutionTrace::new("t-res".into());
        let result = musician.play_step(&step, &mut trace).await.unwrap();
        assert!(result.is_success());
        // Lock should be released after step completes (guard dropped)
        assert!(lm
            .is_locked(&crate::guard::Resource::path("/tmp/musician-test-read.rs"))
            .is_none());
    }

    #[tokio::test]
    async fn test_step_blocked_by_conflict() {
        let musician = Musician::new();
        let lm = musician.lock_manager().clone();
        // Hold a write lock externally
        let rr = crate::guard::RequiredResources::new().write("/tmp/musician-test-conflict.rs");
        let _guard = match lm.try_acquire("other-agent", &rr) {
            LockResult::Acquired(g) => g,
            _ => panic!("Expected Acquired"),
        };

        let step = Step {
            id: "s-blocked".into(),
            index: 0,
            description: "echo hello".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::new()
                .write("/tmp/musician-test-conflict.rs"),
        };
        let mut trace = ExecutionTrace::new("t-blocked".into());
        let result = musician.play_step(&step, &mut trace).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_with_lock_manager() {
        let lm = LockManager::in_memory();
        let musician = Musician::new().with_lock_manager(lm);
        assert!(musician.lock_manager().held_locks().is_empty());
    }

    #[tokio::test]
    async fn test_failure_trace_entry_for_nonzero_exit() {
        let executor = Arc::new(MockExecutor {
            output: "command failed".into(),
            exit_code: Some(1),
        });
        let musician = Musician::with_tool_executor(executor);
        let step = Step {
            id: "s-fail".into(),
            index: 0,
            description: "failing command".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new("t-fail".into(), "test".into());
        let result = musician
            .play_step_with_context(&step, &mut ctx)
            .await
            .unwrap();
        assert!(!result.is_success());

        let trace = &ctx.execution_trace;
        assert_eq!(trace.steps.len(), 1);
        assert!(
            trace.steps[0].error_signal.is_some(),
            "non-zero exit should produce error_signal"
        );
    }

    #[tokio::test]
    async fn test_success_trace_entry_for_zero_exit() {
        let executor = Arc::new(MockExecutor {
            output: "all good".into(),
            exit_code: Some(0),
        });
        let musician = Musician::with_tool_executor(executor);
        let step = Step {
            id: "s-ok".into(),
            index: 0,
            description: "good command".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: true,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new("t-ok".into(), "test".into());
        musician
            .play_step_with_context(&step, &mut ctx)
            .await
            .unwrap();

        assert!(
            ctx.execution_trace.steps[0].error_signal.is_none(),
            "zero exit should not produce error_signal"
        );
    }

    #[tokio::test]
    async fn test_autonomy_l0_blocks_execution() {
        let musician = Musician::new().with_autonomy(crate::autonomy::AutonomyConfig::new(
            crate::autonomy::AutonomyLevel::L0,
        ));
        let step = Step {
            id: "s-l0".into(),
            index: 0,
            description: "echo blocked".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new("t-l0".into(), "test task".into());
        let result = musician.play_step_with_context(&step, &mut ctx).await;
        assert!(result.is_err(), "L0 should block execution");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("suggest-only") || err.contains("blocked"),
            "Error should mention L0 blocking: {err}"
        );
    }

    #[tokio::test]
    async fn test_autonomy_l4_allows_all() {
        let executor = Arc::new(MockExecutor {
            output: "done".into(),
            exit_code: Some(0),
        });
        let musician = Musician::with_tool_executor(executor).with_autonomy(
            crate::autonomy::AutonomyConfig::new(crate::autonomy::AutonomyLevel::L4),
        );
        let step = Step {
            id: "s-l4".into(),
            index: 0,
            description: "echo allowed".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new("t-l4".into(), "test task".into());
        let result = musician.play_step_with_context(&step, &mut ctx).await;
        assert!(result.is_ok(), "L4 should allow all tools: {result:?}");
    }

    #[tokio::test]
    async fn test_autonomy_default_backward_compat() {
        let executor = Arc::new(MockExecutor {
            output: "default".into(),
            exit_code: Some(0),
        });
        let musician = Musician::with_tool_executor(executor);
        let step = Step {
            id: "s-default".into(),
            index: 0,
            description: "echo default".into(),
            expected_output_type: OutputType::Code,
            suggested_tool: Some("Bash".into()),
            retry_on_failure: false,
            required_resources: crate::guard::RequiredResources::default(),
        };
        let mut ctx = TaskContext::new("t-default".into(), "test task".into());
        // Default is L1 which blocks bash (dangerous tool at L1 requires approval but doesn't hard-block)
        // The AutonomyDecider for L1 returns RequireApproval for exec tools, which logs but continues
        let result = musician.play_step_with_context(&step, &mut ctx).await;
        assert!(
            result.is_ok(),
            "Default L1 should not hard-block: {result:?}"
        );
    }
}
