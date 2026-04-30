use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("LLM provider error: {0}")]
    ProviderError(String),

    #[error("invalid thought ID: {0}")]
    InvalidThoughtId(String),

    #[error("thought not found: {0}")]
    ThoughtNotFound(String),

    #[error("graph error: {0}")]
    GraphError(String),

    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    #[error("strategy error: {0}")]
    StrategyError(String),

    #[error("scoring error: {0}")]
    ScoringError(String),

    #[error("pruning error: {0}")]
    PruningError(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("configuration error: {0}")]
    ConfigError(String),

    #[error("metacognitive error: {0}")]
    MetacognitiveError(String),

    #[error("unknown error")]
    Unknown,
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

impl From<Box<bincode::ErrorKind>> for Error {
    fn from(err: Box<bincode::ErrorKind>) -> Self {
        Self::SerializationError(err.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_error_display() {
        let e = Error::ProviderError("timeout".into());
        assert!(e.to_string().contains("timeout"));
    }

    #[test]
    fn test_invalid_thought_id_display() {
        let e = Error::InvalidThoughtId("bad-id".into());
        assert!(e.to_string().contains("bad-id"));
    }

    #[test]
    fn test_thought_not_found_display() {
        let e = Error::ThoughtNotFound("t-42".into());
        assert!(e.to_string().contains("t-42"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err: serde_json::Error = serde_json::from_str::<i32>("not json").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::SerializationError(_)));
    }

    #[test]
    fn test_unknown_variant() {
        let e = Error::Unknown;
        assert_eq!(e.to_string(), "unknown error");
    }

    #[test]
    fn test_result_type_alias() {
        let _: Result<String> = Ok("test".into());
    }
}
