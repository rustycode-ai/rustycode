use thiserror::Error;

/// Main error type for the learning system
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LearningError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Extraction error: {0}")]
    Extraction(#[from] ExtractionError),

    #[error("Pattern not found: {0}")]
    PatternNotFound(String),

    #[error("Invalid pattern configuration: {0}")]
    InvalidPattern(String),

    #[error("Failed to apply instinct: {0}")]
    ApplicationFailed(String),
}

/// Errors related to pattern storage
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum StorageError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Storage directory not found: {0}")]
    DirectoryNotFound(String),

    #[error("Failed to save pattern: {0}")]
    SaveFailed(String),

    #[error("Failed to load pattern: {0}")]
    LoadFailed(String),
}

// Implement From<serde_json::Error> for LearningError
impl From<serde_json::Error> for LearningError {
    fn from(err: serde_json::Error) -> Self {
        Self::Storage(StorageError::Serialization(err))
    }
}

/// Errors related to pattern extraction
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ExtractionError {
    #[error("No patterns found in session")]
    NoPatternsFound,

    #[error("Session data incomplete: {0}")]
    IncompleteSession(String),

    #[error("Pattern validation failed: {0}")]
    ValidationFailed(String),

    #[error("Confidence too low: {0}")]
    LowConfidence(f32),
}

/// Result type for learning operations
pub type Result<T> = std::result::Result<T, LearningError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learning_error_display() {
        assert_eq!(
            LearningError::PatternNotFound("test".into()).to_string(),
            "Pattern not found: test"
        );
        assert_eq!(
            LearningError::InvalidPattern("bad config".into()).to_string(),
            "Invalid pattern configuration: bad config"
        );
        assert_eq!(
            LearningError::ApplicationFailed("timeout".into()).to_string(),
            "Failed to apply instinct: timeout"
        );
    }

    #[test]
    fn test_storage_error_display() {
        assert_eq!(
            StorageError::DirectoryNotFound("/tmp/patterns".into()).to_string(),
            "Storage directory not found: /tmp/patterns"
        );
        assert_eq!(
            StorageError::SaveFailed("disk full".into()).to_string(),
            "Failed to save pattern: disk full"
        );
        assert_eq!(
            StorageError::LoadFailed("corrupt file".into()).to_string(),
            "Failed to load pattern: corrupt file"
        );
    }

    #[test]
    fn test_extraction_error_display() {
        assert_eq!(
            ExtractionError::NoPatternsFound.to_string(),
            "No patterns found in session"
        );
        assert_eq!(
            ExtractionError::IncompleteSession("missing tool_calls".into()).to_string(),
            "Session data incomplete: missing tool_calls"
        );
        assert_eq!(
            ExtractionError::LowConfidence(0.12).to_string(),
            "Confidence too low: 0.12"
        );
    }

    #[test]
    fn test_errors_are_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(LearningError::Storage(
            StorageError::DirectoryNotFound("x".into()),
        ));
        assert!(err.to_string().contains("x"));
    }
}
