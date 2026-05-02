use async_trait::async_trait;
use crate::types::{UnitCapabilities, ExecutionContext};
use crate::ExecutableError;

/// Unified interface for executable units
#[async_trait]
pub trait Callable: Send + Sync {
    async fn execute(
        &self,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError>;

    fn get_runtime_capabilities(&self) -> UnitCapabilities;

    async fn validate_input(&self, _input: &ExecutionInput) -> Result<(), String> {
        Ok(())
    }

    async fn process_output(&self, output: ExecutionOutput) -> Result<ExecutionOutput, ExecutableError> {
        Ok(output)
    }
}

#[derive(Clone, Debug)]
pub struct ExecutionInput {
    pub data: serde_json::Value,
    pub caller_info: Option<CallerInfo>,
    pub session_context: Option<SessionContext>,
}

#[derive(Clone, Debug)]
pub struct ExecutionOutput {
    pub data: serde_json::Value,
    pub metadata: ExecutionMetadata,
}

#[derive(Clone, Debug)]
pub struct ExecutionMetadata {
    pub duration_ms: u64,
    pub tokens_used: Option<TokenUsage>,
    pub was_cached: bool,
    pub trace: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct CallerInfo {
    pub role: String,
}

#[derive(Clone, Debug)]
pub struct SessionContext {
    pub session_id: String,
}

#[derive(Clone, Debug)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// No-op callable used as placeholder by loaders
pub struct NoOpCallable;

#[async_trait]
impl Callable for NoOpCallable {
    async fn execute(
        &self,
        _input: ExecutionInput,
        _context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Ok(ExecutionOutput {
            data: serde_json::json!({"status": "noop"}),
            metadata: ExecutionMetadata {
                duration_ms: 0,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }

    fn get_runtime_capabilities(&self) -> UnitCapabilities {
        UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        }
    }
}
