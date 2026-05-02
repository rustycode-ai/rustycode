pub mod direct;
pub mod skill;
pub mod agent;

use crate::{
    ExecutableRegistry, ExecutableUnit, ExecutionContext, ExecutionCapability,
    ExecutionInput, ExecutionOutput, ExecutionMode, ExecutableError,
};
use std::sync::Arc;
use async_trait::async_trait;

pub use direct::DirectExecutor;
pub use skill::SkillBundler;
pub use agent::AgentExecutor;

/// Routes `ExecutableUnit` invocations to context-specific handlers
pub struct ExecutionRouter {
    registry: Arc<ExecutableRegistry>,
    direct_executor: Arc<dyn DirectExecutor>,
    skill_bundler: Arc<dyn SkillBundler>,
    agent_executor: Arc<dyn AgentExecutor>,
}

impl ExecutionRouter {
    pub fn new(
        registry: Arc<ExecutableRegistry>,
        direct: Arc<dyn DirectExecutor>,
        skill: Arc<dyn SkillBundler>,
        agent: Arc<dyn AgentExecutor>,
    ) -> Self {
        Self {
            registry,
            direct_executor: direct,
            skill_bundler: skill,
            agent_executor: agent,
        }
    }

    pub fn new_with_defaults(registry: Arc<ExecutableRegistry>) -> Self {
        Self {
            registry,
            direct_executor: Arc::new(DefaultDirectExecutor),
            skill_bundler: Arc::new(DefaultSkillBundler),
            agent_executor: Arc::new(DefaultAgentExecutor),
        }
    }

    /// Route a unit invocation to the appropriate handler
    pub async fn execute(
        &self,
        unit_id: &str,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        let unit = self
            .registry
            .get(unit_id)
            .await
            .ok_or_else(|| ExecutableError::NotFound(unit_id.to_string()))?;

        if !context_unit_supports(&context, &unit) {
            return Err(ExecutableError::UnsupportedContext {
                unit: unit_id.to_string(),
                context: format!("{context:?}"),
            });
        }

        match context {
            ExecutionContext::DirectTool { .. } | ExecutionContext::ProgrammaticCall { .. } => {
                self.direct_executor.execute(&unit, input).await
            }
            ExecutionContext::SkillReference { .. } => {
                self.skill_bundler.bundle(&unit, input).await
            }
            ExecutionContext::AgentReasoning { .. } => {
                self.agent_executor.execute(&unit, input).await
            }
        }
    }

    /// Execute with automatic context selection (Hybrid mode)
    pub async fn execute_hybrid(
        &self,
        unit_id: &str,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        let unit = self
            .registry
            .get(unit_id)
            .await
            .ok_or_else(|| ExecutableError::NotFound(unit_id.to_string()))?;

        let context = Self::select_context(&unit, &input);
        self.execute(unit_id, input, context).await
    }

    fn select_context(unit: &ExecutableUnit, _input: &ExecutionInput) -> ExecutionContext {
        if unit.advanced_metadata.execution_strategy == ExecutionMode::Autonomous
            && unit.capabilities.can_reason_autonomously
        {
            ExecutionContext::AgentReasoning {
                autonomous: true,
                max_steps: Some(10),
                can_delegate: true,
            }
        } else if unit.capabilities.can_execute_directly {
            ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: Some(30_000),
            }
        } else {
            ExecutionContext::SkillReference {
                discoverable: true,
                cacheable: true,
            }
        }
    }
}

/// Check whether a unit supports the given execution context
const fn context_unit_supports(context: &ExecutionContext, unit: &ExecutableUnit) -> bool {
    match context.requires_capability() {
        ExecutionCapability::DirectExecution => unit.capabilities.can_execute_directly,
        ExecutionCapability::Knowledge => unit.capabilities.can_bundle_knowledge,
        ExecutionCapability::Reasoning => unit.capabilities.can_reason_autonomously,
    }
}

// Stub implementations for testing
struct DefaultDirectExecutor;
struct DefaultSkillBundler;
struct DefaultAgentExecutor;

#[async_trait]
impl DirectExecutor for DefaultDirectExecutor {
    async fn execute(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}

#[async_trait]
impl SkillBundler for DefaultSkillBundler {
    async fn bundle(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}

#[async_trait]
impl AgentExecutor for DefaultAgentExecutor {
    async fn execute(
        &self,
        _unit: &ExecutableUnit,
        _input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}
