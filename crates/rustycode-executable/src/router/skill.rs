use async_trait::async_trait;
use crate::{ExecutableUnit, ExecutionInput, ExecutionOutput, ExecutableError};

#[async_trait]
pub trait SkillBundler: Send + Sync {
    async fn bundle(&self, unit: &ExecutableUnit, input: ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
