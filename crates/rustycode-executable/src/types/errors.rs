use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutableError {
    #[error("unit not found: {0}")]
    NotFound(String),

    #[error("unsupported context: unit {unit} cannot execute in {context}")]
    UnsupportedContext { unit: String, context: String },

    #[error("capability missing: {0}")]
    CapabilityMissing(String),

    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    #[error("timeout: execution exceeded {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("circular dependency detected: {chain}")]
    CircularDependency { chain: String },

    #[error("validation error: {0}")]
    ValidationError(String),
}
