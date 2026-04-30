use thiserror::Error;

pub type Result<T> = std::result::Result<T, OrchestrationError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestrationErrorCategory {
    Thinking,
    Execution,
    Task,
    Tool,
    Session,
    Verification,
    Recovery,
    IO,
    Config,
    LLM,
    Storage,
    Internal,
    Ast,
}

#[derive(Error, Debug)]
pub enum OrchestrationError {
    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Execution failed: {message}")]
    Execution { message: String },

    #[error("Model communication error: {message}")]
    ModelError { message: String },

    #[error("Pattern recognition error: {message}")]
    PatternError { message: String },

    #[error("Escalation routing error: {message}")]
    EscalationError { message: String },

    #[error("Verification gate failed: {message}")]
    VerificationError { message: String },

    #[error("Task decomposition error: {message}")]
    DecompositionError { message: String },

    #[error("Timeout exceeded: {operation}")]
    Timeout { operation: String },

    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Storage error: {message}")]
    Storage { message: String },

    #[error("LLM provider error: {0}")]
    LLMProvider(#[from] rustycode_llm::ProviderError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Thinking graph error: {message}")]
    ThinkingGraph { message: String },

    #[error("Thought not found: {id}")]
    ThoughtNotFound { id: String },

    #[error("Thinking strategy error: {message}")]
    ThinkingStrategy { message: String },

    #[error("Thinking scoring error: {message}")]
    ThinkingScoring { message: String },

    #[error("Thinking convergence error: {message}")]
    ThinkingConvergence { message: String },

    #[error("Thinking serialization error: {message}")]
    ThinkingSerialization { message: String },

    #[error("Thinking metacognitive error: {message}")]
    ThinkingMetacognitive { message: String },

    #[error("Task not found: {0}")]
    TaskNotFound(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Worktree error: {0}")]
    Worktree(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Model routing error: {0}")]
    ModelRouting(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Recovery error: {0}")]
    Recovery(String),

    #[error("Isolation error: {message}")]
    Isolation { message: String },

    #[error("Handoff error: {message}")]
    Handoff { message: String },

    #[error("Fork-join error: {message}")]
    ForkJoin { message: String },

    #[error("Schema error: {message}")]
    Schema { message: String },

    #[error("Hook vetoed: {reason}")]
    HookVeto { reason: String },

    #[error("Prompt load error for '{template}': missing variables {missing:?}. {hint}")]
    PromptLoad {
        template: String,
        missing: Vec<String>,
        hint: String,
    },

    #[error("AST phase violation: expected {expected}, got {actual}")]
    AstPhaseViolation { expected: String, actual: String },

    #[error("AST step failed: milestone {milestone}, exit code {exit_code}")]
    AstStepFailed { milestone: usize, exit_code: i32 },

    #[error("AST verification failed: {status}")]
    AstVerification { status: String },

    #[error("AST config error: {message}")]
    AstConfig { message: String },

    #[error("AST recovery exhausted: strategy {strategy}, {attempts} attempts")]
    AstRecovery { strategy: String, attempts: u32 },

    #[error("AST ledger error: {message}")]
    AstLedger { message: String },
}

impl From<tokio::sync::broadcast::error::RecvError> for OrchestrationError {
    fn from(err: tokio::sync::broadcast::error::RecvError) -> Self {
        Self::Internal {
            message: format!("broadcast receive error: {err}"),
        }
    }
}

impl From<anyhow::Error> for OrchestrationError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal {
            message: err.to_string(),
        }
    }
}

impl OrchestrationError {
    pub const fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::Execution { .. }
                | Self::ModelError { .. }
                | Self::VerificationError { .. }
                | Self::Timeout { .. }
                | Self::ResourceExhausted { .. }
                | Self::ThinkingStrategy { .. }
                | Self::ThinkingScoring { .. }
                | Self::ThinkingConvergence { .. }
                | Self::AstPhaseViolation { .. }
                | Self::AstStepFailed { .. }
                | Self::AstVerification { .. }
                | Self::AstRecovery { .. }
        )
    }

    pub const fn category(&self) -> OrchestrationErrorCategory {
        match self {
            Self::ThinkingGraph { .. }
            | Self::ThoughtNotFound { .. }
            | Self::ThinkingStrategy { .. }
            | Self::ThinkingScoring { .. }
            | Self::ThinkingConvergence { .. }
            | Self::ThinkingSerialization { .. }
            | Self::ThinkingMetacognitive { .. } => OrchestrationErrorCategory::Thinking,

            Self::Execution { .. } => OrchestrationErrorCategory::Execution,
            Self::DecompositionError { .. } | Self::TaskNotFound(_) => {
                OrchestrationErrorCategory::Task
            }
            Self::ToolExecution(_) | Self::HookVeto { .. } => OrchestrationErrorCategory::Tool,
            Self::Session(_) => OrchestrationErrorCategory::Session,
            Self::VerificationError { .. } | Self::Schema { .. } => {
                OrchestrationErrorCategory::Verification
            }
            Self::Recovery(_) => OrchestrationErrorCategory::Recovery,
            Self::Io(_) => OrchestrationErrorCategory::IO,
            Self::Configuration { .. } | Self::PromptLoad { .. } => {
                OrchestrationErrorCategory::Config
            }
            Self::ModelError { .. } | Self::LLMProvider(_) | Self::ModelRouting(_) => {
                OrchestrationErrorCategory::LLM
            }
            Self::Storage { .. }
            | Self::Serialization(_)
            | Self::Parse(_)
            | Self::PatternError { .. }
            | Self::EscalationError { .. }
            | Self::Git(_)
            | Self::Worktree(_)
            | Self::InvalidState(_)
            | Self::Timeout { .. }
            | Self::ResourceExhausted { .. }
            | Self::Internal { .. }
            | Self::Isolation { .. }
            | Self::Handoff { .. }
            | Self::ForkJoin { .. } => OrchestrationErrorCategory::Internal,
            Self::AstPhaseViolation { .. }
            | Self::AstStepFailed { .. }
            | Self::AstVerification { .. }
            | Self::AstConfig { .. }
            | Self::AstRecovery { .. }
            | Self::AstLedger { .. } => OrchestrationErrorCategory::Ast,
        }
    }

    pub fn thinking(msg: impl Into<String>) -> Self {
        Self::ThinkingGraph {
            message: msg.into(),
        }
    }

    pub fn execution(msg: impl Into<String>) -> Self {
        Self::Execution {
            message: msg.into(),
        }
    }

    pub fn task(msg: impl Into<String>) -> Self {
        Self::DecompositionError {
            message: msg.into(),
        }
    }

    pub fn tool(msg: impl Into<String>) -> Self {
        Self::ToolExecution(msg.into())
    }

    pub fn session(msg: impl Into<String>) -> Self {
        Self::Session(msg.into())
    }

    pub fn verification(msg: impl Into<String>) -> Self {
        Self::VerificationError {
            message: msg.into(),
        }
    }

    pub fn recovery(msg: impl Into<String>) -> Self {
        Self::Recovery(msg.into())
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Configuration {
            message: msg.into(),
        }
    }

    pub fn llm(msg: impl Into<String>) -> Self {
        Self::ModelError {
            message: msg.into(),
        }
    }

    pub fn isolation(msg: impl Into<String>) -> Self {
        Self::Isolation {
            message: msg.into(),
        }
    }

    pub fn handoff(msg: impl Into<String>) -> Self {
        Self::Handoff {
            message: msg.into(),
        }
    }

    pub fn fork_join(msg: impl Into<String>) -> Self {
        Self::ForkJoin {
            message: msg.into(),
        }
    }

    pub fn schema(msg: impl Into<String>) -> Self {
        Self::Schema {
            message: msg.into(),
        }
    }

    pub fn ast_phase(expected: impl Into<String>, actual: impl Into<String>) -> Self {
        Self::AstPhaseViolation {
            expected: expected.into(),
            actual: actual.into(),
        }
    }

    pub const fn ast_step(milestone: usize, exit_code: i32) -> Self {
        Self::AstStepFailed {
            milestone,
            exit_code,
        }
    }

    pub fn ast_verification(msg: impl Into<String>) -> Self {
        Self::AstVerification { status: msg.into() }
    }

    pub fn ast_recovery(strategy: impl Into<String>, attempts: u32) -> Self {
        Self::AstRecovery {
            strategy: strategy.into(),
            attempts,
        }
    }

    pub fn ast_ledger(msg: impl Into<String>) -> Self {
        Self::AstLedger {
            message: msg.into(),
        }
    }

    pub fn ast_config(msg: impl Into<String>) -> Self {
        Self::AstConfig {
            message: msg.into(),
        }
    }

    pub fn from_thinking_error(err: crate::thinking::core::error::Error) -> Self {
        use crate::thinking::core::error::Error;
        match err {
            Error::ThoughtNotFound(id) => Self::ThoughtNotFound { id },
            Error::GraphError(msg) | Error::InvalidOperation(msg) => {
                Self::ThinkingGraph { message: msg }
            }
            Error::StrategyError(msg) => Self::ThinkingStrategy { message: msg },
            Error::ScoringError(msg) => Self::ThinkingScoring { message: msg },
            Error::PruningError(msg) => Self::ThinkingGraph {
                message: format!("pruning: {msg}"),
            },
            Error::SerializationError(msg) => Self::ThinkingSerialization { message: msg },
            Error::MetacognitiveError(msg) => Self::ThinkingMetacognitive { message: msg },
            Error::ConfigError(msg) => Self::Configuration { message: msg },
            Error::ProviderError(msg) => Self::ModelError { message: msg },
            Error::InvalidThoughtId(msg) => Self::ThoughtNotFound { id: msg },
            Error::Unknown => Self::Internal {
                message: "unknown thinking error".into(),
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category_execution() {
        assert_eq!(
            OrchestrationError::execution("x").category(),
            OrchestrationErrorCategory::Execution
        );
    }

    #[test]
    fn test_error_category_thinking() {
        assert_eq!(
            OrchestrationError::thinking("x").category(),
            OrchestrationErrorCategory::Thinking
        );
        assert_eq!(
            OrchestrationError::ThinkingStrategy {
                message: "x".into()
            }
            .category(),
            OrchestrationErrorCategory::Thinking
        );
        assert_eq!(
            OrchestrationError::ThoughtNotFound { id: "t1".into() }.category(),
            OrchestrationErrorCategory::Thinking
        );
    }

    #[test]
    fn test_error_category_task() {
        assert_eq!(
            OrchestrationError::task("x").category(),
            OrchestrationErrorCategory::Task
        );
        assert_eq!(
            OrchestrationError::TaskNotFound("t1".into()).category(),
            OrchestrationErrorCategory::Task
        );
    }

    #[test]
    fn test_error_category_tool() {
        assert_eq!(
            OrchestrationError::tool("x").category(),
            OrchestrationErrorCategory::Tool
        );
    }

    #[test]
    fn test_error_category_session() {
        assert_eq!(
            OrchestrationError::session("x").category(),
            OrchestrationErrorCategory::Session
        );
    }

    #[test]
    fn test_error_category_verification() {
        assert_eq!(
            OrchestrationError::verification("x").category(),
            OrchestrationErrorCategory::Verification
        );
    }

    #[test]
    fn test_error_category_recovery() {
        assert_eq!(
            OrchestrationError::recovery("x").category(),
            OrchestrationErrorCategory::Recovery
        );
    }

    #[test]
    fn test_error_category_config() {
        assert_eq!(
            OrchestrationError::config("x").category(),
            OrchestrationErrorCategory::Config
        );
    }

    #[test]
    fn test_error_category_llm() {
        assert_eq!(
            OrchestrationError::llm("x").category(),
            OrchestrationErrorCategory::LLM
        );
    }

    #[test]
    fn test_is_recoverable() {
        assert!(OrchestrationError::execution("x").is_recoverable());
        assert!(OrchestrationError::ModelError {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::VerificationError {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::Timeout {
            operation: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::ResourceExhausted {
            resource: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::ThinkingStrategy {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::ThinkingScoring {
            message: "x".into()
        }
        .is_recoverable());
        assert!(OrchestrationError::ThinkingConvergence {
            message: "x".into()
        }
        .is_recoverable());
    }

    #[test]
    fn test_is_not_recoverable() {
        assert!(!OrchestrationError::Configuration {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::Internal {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::Storage {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::ThinkingGraph {
            message: "x".into()
        }
        .is_recoverable());
        assert!(!OrchestrationError::ThoughtNotFound { id: "x".into() }.is_recoverable());
    }

    #[test]
    fn test_error_display_messages() {
        assert!(OrchestrationError::execution("boom")
            .to_string()
            .contains("boom"));
        assert!(OrchestrationError::Timeout {
            operation: "step-1".into()
        }
        .to_string()
        .contains("step-1"));
        assert!(OrchestrationError::ResourceExhausted {
            resource: "memory".into()
        }
        .to_string()
        .contains("memory"));
        assert!(OrchestrationError::ThoughtNotFound { id: "t1".into() }
            .to_string()
            .contains("t1"));
    }

    #[test]
    fn test_convenience_constructors() {
        assert!(matches!(
            OrchestrationError::thinking("x"),
            OrchestrationError::ThinkingGraph { .. }
        ));
        assert!(matches!(
            OrchestrationError::execution("x"),
            OrchestrationError::Execution { .. }
        ));
        assert!(matches!(
            OrchestrationError::task("x"),
            OrchestrationError::DecompositionError { .. }
        ));
        assert!(matches!(
            OrchestrationError::tool("x"),
            OrchestrationError::ToolExecution(_)
        ));
        assert!(matches!(
            OrchestrationError::session("x"),
            OrchestrationError::Session(_)
        ));
        assert!(matches!(
            OrchestrationError::verification("x"),
            OrchestrationError::VerificationError { .. }
        ));
        assert!(matches!(
            OrchestrationError::recovery("x"),
            OrchestrationError::Recovery(_)
        ));
        assert!(matches!(
            OrchestrationError::config("x"),
            OrchestrationError::Configuration { .. }
        ));
        assert!(matches!(
            OrchestrationError::llm("x"),
            OrchestrationError::ModelError { .. }
        ));
    }

    #[test]
    fn test_from_thinking_error_conversions() {
        use crate::thinking::core::error::Error as ThinkError;

        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::ThoughtNotFound("t1".into())),
            OrchestrationError::ThoughtNotFound { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::GraphError("g".into())),
            OrchestrationError::ThinkingGraph { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::StrategyError("s".into())),
            OrchestrationError::ThinkingStrategy { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::ScoringError("sc".into())),
            OrchestrationError::ThinkingScoring { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::MetacognitiveError("m".into())),
            OrchestrationError::ThinkingMetacognitive { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::ConfigError("c".into())),
            OrchestrationError::Configuration { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::ProviderError("p".into())),
            OrchestrationError::ModelError { .. }
        ));
        assert!(matches!(
            OrchestrationError::from_thinking_error(ThinkError::Unknown),
            OrchestrationError::Internal { .. }
        ));
    }

    #[test]
    fn test_all_error_categories_covered() {
        // Verify every variant maps to a non-Internal category where appropriate
        assert_eq!(
            OrchestrationError::task("x").category(),
            OrchestrationErrorCategory::Task
        );
        assert_eq!(
            OrchestrationError::tool("x").category(),
            OrchestrationErrorCategory::Tool
        );
        assert_eq!(
            OrchestrationError::ThinkingSerialization {
                message: "x".into()
            }
            .category(),
            OrchestrationErrorCategory::Thinking
        );
        assert_eq!(
            OrchestrationError::ThinkingMetacognitive {
                message: "x".into()
            }
            .category(),
            OrchestrationErrorCategory::Thinking
        );
    }

    #[test]
    fn test_isolation_error_category() {
        assert_eq!(
            OrchestrationError::isolation("blocked").category(),
            OrchestrationErrorCategory::Internal
        );
    }

    #[test]
    fn test_handoff_error_category() {
        assert_eq!(
            OrchestrationError::handoff("missing context").category(),
            OrchestrationErrorCategory::Internal
        );
    }

    #[test]
    fn test_fork_join_error_category() {
        assert_eq!(
            OrchestrationError::fork_join("timeout").category(),
            OrchestrationErrorCategory::Internal
        );
    }

    #[test]
    fn test_new_error_variants_display() {
        assert!(OrchestrationError::isolation("tool blocked")
            .to_string()
            .contains("tool blocked"));
        assert!(OrchestrationError::handoff("missing ctx")
            .to_string()
            .contains("missing ctx"));
        assert!(OrchestrationError::fork_join("spawn failed")
            .to_string()
            .contains("spawn failed"));
    }

    #[test]
    fn test_ast_error_variants_category() {
        assert_eq!(
            OrchestrationError::AstPhaseViolation {
                expected: "RESEARCH".into(),
                actual: "EXECUTE".into(),
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
        assert_eq!(
            OrchestrationError::AstStepFailed {
                milestone: 3,
                exit_code: 1,
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
        assert_eq!(
            OrchestrationError::AstVerification {
                status: "FAIL".into(),
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
        assert_eq!(
            OrchestrationError::AstConfig {
                message: "missing ledger".into(),
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
        assert_eq!(
            OrchestrationError::AstRecovery {
                strategy: "retry".into(),
                attempts: 5,
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
        assert_eq!(
            OrchestrationError::AstLedger {
                message: "io error".into(),
            }
            .category(),
            OrchestrationErrorCategory::Ast
        );
    }

    #[test]
    fn test_ast_error_is_recoverable() {
        assert!(OrchestrationError::AstPhaseViolation {
            expected: "RESEARCH".into(),
            actual: "EXECUTE".into(),
        }
        .is_recoverable());
        assert!(OrchestrationError::AstStepFailed {
            milestone: 1,
            exit_code: 1,
        }
        .is_recoverable());
        assert!(OrchestrationError::AstVerification {
            status: "FAIL".into(),
        }
        .is_recoverable());
        assert!(OrchestrationError::AstRecovery {
            strategy: "retry".into(),
            attempts: 3,
        }
        .is_recoverable());
    }

    #[test]
    fn test_ast_error_is_not_recoverable() {
        assert!(!OrchestrationError::AstConfig {
            message: "bad".into(),
        }
        .is_recoverable());
        assert!(!OrchestrationError::AstLedger {
            message: "io".into(),
        }
        .is_recoverable());
    }

    #[test]
    fn test_ast_error_convenience_constructors() {
        assert!(matches!(
            OrchestrationError::ast_phase("RESEARCH", "EXECUTE"),
            OrchestrationError::AstPhaseViolation { .. }
        ));
        assert!(matches!(
            OrchestrationError::ast_step(3, 1),
            OrchestrationError::AstStepFailed { .. }
        ));
        assert!(matches!(
            OrchestrationError::ast_verification("FAIL"),
            OrchestrationError::AstVerification { .. }
        ));
        assert!(matches!(
            OrchestrationError::ast_recovery("retry", 5),
            OrchestrationError::AstRecovery { .. }
        ));
        assert!(matches!(
            OrchestrationError::ast_ledger("io error"),
            OrchestrationError::AstLedger { .. }
        ));
        assert!(matches!(
            OrchestrationError::ast_config("missing"),
            OrchestrationError::AstConfig { .. }
        ));
    }

    #[test]
    fn test_ast_error_display_messages() {
        assert!(OrchestrationError::AstPhaseViolation {
            expected: "RESEARCH".into(),
            actual: "EXECUTE".into(),
        }
        .to_string()
        .contains("RESEARCH"));
        assert!(OrchestrationError::AstStepFailed {
            milestone: 3,
            exit_code: 1,
        }
        .to_string()
        .contains('3'));
        assert!(OrchestrationError::AstVerification {
            status: "FAIL".into(),
        }
        .to_string()
        .contains("FAIL"));
        assert!(OrchestrationError::AstConfig {
            message: "missing ledger".into(),
        }
        .to_string()
        .contains("missing ledger"));
        assert!(OrchestrationError::AstRecovery {
            strategy: "retry".into(),
            attempts: 5,
        }
        .to_string()
        .contains('5'));
        assert!(OrchestrationError::AstLedger {
            message: "io error".into(),
        }
        .to_string()
        .contains("io error"));
    }
}
