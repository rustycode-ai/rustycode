//! Composable input validation trait for tool parameters.
//!
//! Each validator checks a specific input type (commands, paths, URLs, regex).
//! Validators are chained before tool execution to reject invalid input early.

use std::path::Path;

/// Error returned when input validation fails.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("command rejected: {0}")]
    CommandRejected(String),
    #[error("path rejected: {0}")]
    PathRejected(String),
    #[error("URL rejected: {0}")]
    UrlRejected(String),
    #[error("regex rejected: {0}")]
    RegexRejected(String),
    #[error("validation failed: {0}")]
    Other(String),
}

/// Trait for validating tool input before execution.
///
/// Implementations wrap existing validation logic (bash command safety,
/// path sandboxing, URL allowlisting, regex safety) behind a uniform
/// interface that the tool executor can call.
pub trait InputValidator<T: ?Sized>: Send + Sync {
    fn validate(&self, input: &T) -> Result<(), ValidationError>;
}

/// Validates that a path stays within the workspace root.
pub struct PathValidator {
    workspace_root: std::path::PathBuf,
}

impl PathValidator {
    pub fn new(workspace_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }
}

impl InputValidator<Path> for PathValidator {
    fn validate(&self, path: &Path) -> Result<(), ValidationError> {
        let canonical = path
            .canonicalize()
            .map_err(|e| ValidationError::PathRejected(format!("cannot resolve path: {e}")))?;
        let root = self
            .workspace_root
            .canonicalize()
            .map_err(|e| ValidationError::PathRejected(format!("cannot resolve workspace: {e}")))?;
        if !canonical.starts_with(&root) {
            return Err(ValidationError::PathRejected(format!(
                "path {} is outside workspace {}",
                canonical.display(),
                root.display()
            )));
        }
        Ok(())
    }
}

/// Validates that a command string is not obviously dangerous.
/// This is a lightweight check — full validation lives in
/// `rustycode-tools::bash::validate_command_safety`.
pub struct CommandValidator;

impl InputValidator<str> for CommandValidator {
    fn validate(&self, command: &str) -> Result<(), ValidationError> {
        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err(ValidationError::CommandRejected("empty command".into()));
        }
        // Block obvious injection patterns.
        let blocked = ["rm -rf /", "mkfs.", "dd if=", ":(){ :|:& };:"];
        for pattern in &blocked {
            if trimmed.contains(pattern) {
                return Err(ValidationError::CommandRejected(format!(
                    "blocked pattern: {pattern}"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validator_allows_workspace_files() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "x").unwrap();
        let v = PathValidator::new(dir.path());
        assert!(v.validate(file_path.as_path()).is_ok());
    }

    #[test]
    fn path_validator_rejects_outside_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let v = PathValidator::new(dir.path());
        assert!(v.validate(Path::new("/etc/passwd")).is_err());
    }

    #[test]
    fn command_validator_rejects_empty() {
        let v = CommandValidator;
        assert!(v.validate("").is_err());
        assert!(v.validate("  ").is_err());
    }

    #[test]
    fn command_validator_rejects_dangerous_patterns() {
        let v = CommandValidator;
        assert!(v.validate("rm -rf /").is_err());
        assert!(v.validate("dd if=/dev/zero of=/dev/sda").is_err());
    }

    #[test]
    fn command_validator_allows_safe_commands() {
        let v = CommandValidator;
        assert!(v.validate("ls -la").is_ok());
        assert!(v.validate("cargo test").is_ok());
    }
}
