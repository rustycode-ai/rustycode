use crate::{ExecutableError, ExecutableUnit, ExecutionInput, ExecutionOutput};
use async_trait::async_trait;

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(
        &self,
        unit: &ExecutableUnit,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError>;
}
