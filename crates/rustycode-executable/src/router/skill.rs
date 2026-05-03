use crate::{ExecutableError, ExecutableUnit, ExecutionInput, ExecutionOutput};
use async_trait::async_trait;

#[async_trait]
pub trait SkillBundler: Send + Sync {
    async fn bundle(
        &self,
        unit: &ExecutableUnit,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError>;
}
