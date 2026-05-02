use async_trait::async_trait;
use crate::{ExecutableUnit, ExecutionInput, ExecutionOutput, ExecutableError};

#[async_trait]
pub trait DirectExecutor: Send + Sync {
    async fn execute(&self, unit: &ExecutableUnit, input: ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
