use thiserror::Error;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("sandbox not available: {0}")]
    NotAvailable(String),
    #[error("sandbox execution failed: {0}")]
    ExecutionFailed(String),
    #[error("policy error: {0}")]
    PolicyError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
