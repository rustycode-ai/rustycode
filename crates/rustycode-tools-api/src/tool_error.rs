//! Structured error type for tool execution failures.
//!
//! Every tool error includes a machine-readable code, a user-facing message,
//! optional technical details, and an actionable suggestion for how to fix it.

/// Well-known error codes for tool failures.
///
/// Codes follow `SCREAMING_SNAKE` convention and are stable across releases.
/// Tool implementations should prefer these codes over ad-hoc strings.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolErrorCode {
    /// Input failed validation (bad path, command, regex, etc.)
    InvalidInput,
    /// Path does not exist or is not accessible
    PathNotFound,
    /// Path exists but access is denied (permissions, sandboxing)
    PermissionDenied,
    /// Path is outside the allowed workspace
    PathOutsideWorkspace,
    /// File is blocked by security policy (.env, credentials, etc.)
    FileBlocked,
    /// Command is blocked by security validation
    CommandBlocked,
    /// Command was not found on the system PATH
    CommandNotFound,
    /// Command execution timed out
    Timeout,
    /// Shell command exited with non-zero status
    CommandFailed,
    /// Tool parameters were missing or malformed
    InvalidParameters,
    /// Required resource (provider, registry, plugin) is unavailable
    ResourceUnavailable,
    /// A tool or resource was not found by name/id
    NotFound,
    /// A resource already exists (duplicate registration, etc.)
    AlreadyExists,
    /// I/O error (disk, network, pipe)
    IoError,
    /// Catch-all for errors that don't fit a specific code
    Internal,
}

impl std::fmt::Display for ToolErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::InvalidInput => "INVALID_INPUT",
            Self::PathNotFound => "PATH_NOT_FOUND",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::PathOutsideWorkspace => "PATH_OUTSIDE_WORKSPACE",
            Self::FileBlocked => "FILE_BLOCKED",
            Self::CommandBlocked => "COMMAND_BLOCKED",
            Self::CommandNotFound => "COMMAND_NOT_FOUND",
            Self::Timeout => "TIMEOUT",
            Self::CommandFailed => "COMMAND_FAILED",
            Self::InvalidParameters => "INVALID_PARAMETERS",
            Self::ResourceUnavailable => "RESOURCE_UNAVAILABLE",
            Self::NotFound => "NOT_FOUND",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::IoError => "IO_ERROR",
            Self::Internal => "INTERNAL_ERROR",
        };
        f.write_str(s)
    }
}

/// Structured error returned by tool implementations.
///
/// Every field is populated with information that helps the caller (LLM or human)
/// understand what went wrong and how to fix it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolError {
    /// Machine-readable error code for programmatic handling.
    pub code: ToolErrorCode,
    /// Short, human-readable summary of the failure.
    pub message: String,
    /// Technical details (original error message, path, command, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Actionable suggestion for how to fix the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

impl ToolError {
    /// Create a new error with code and message.
    pub fn new(code: ToolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            suggestion: None,
        }
    }

    /// Add technical details.
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add an actionable suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    // ── Convenience constructors for common codes ──

    /// Path does not exist.
    pub fn path_not_found(path: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::PathNotFound,
            format!("path not found: {path}"),
        )
        .with_suggestion("Check that the path is correct and the file or directory exists")
    }

    /// Access denied (permissions or sandboxing).
    pub fn permission_denied(path: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::PermissionDenied,
            format!("permission denied: {path}"),
        )
        .with_suggestion("Check file permissions or workspace sandboxing settings")
    }

    /// Path escapes the workspace boundary.
    pub fn path_outside_workspace(
        path: impl std::fmt::Display,
        workspace: impl std::fmt::Display,
    ) -> Self {
        Self::new(
            ToolErrorCode::PathOutsideWorkspace,
            format!("path {path} is outside workspace {workspace}"),
        )
        .with_suggestion("Use a path relative to the workspace root")
    }

    /// File is blocked by security policy.
    pub fn file_blocked(path: impl std::fmt::Display, reason: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::FileBlocked,
            format!("access to {path} blocked: {reason}"),
        )
        .with_suggestion("This file type is restricted for security. Use a different file.")
    }

    /// Command blocked by security validation.
    pub fn command_blocked(
        command: impl std::fmt::Display,
        reason: impl std::fmt::Display,
    ) -> Self {
        Self::new(
            ToolErrorCode::CommandBlocked,
            format!("command blocked: {reason}"),
        )
        .with_details(format!("rejected command: {command}"))
    }

    /// Command not found on system PATH.
    pub fn command_not_found(command: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::CommandNotFound,
            format!("command not found: {command}"),
        )
        .with_suggestion("Install the command or check the spelling")
    }

    /// Operation timed out.
    pub fn timeout(operation: impl std::fmt::Display, duration: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::Timeout,
            format!("{operation} timed out after {duration}"),
        )
        .with_suggestion("Increase the timeout or simplify the operation")
    }

    /// Command exited with non-zero status.
    pub fn command_failed(command: impl std::fmt::Display, exit_code: i32) -> Self {
        Self::new(
            ToolErrorCode::CommandFailed,
            format!("command exited with code {exit_code}"),
        )
        .with_details(format!("command: {command}"))
    }

    /// Tool parameters were invalid.
    pub fn invalid_parameters(
        tool: impl std::fmt::Display,
        reason: impl std::fmt::Display,
    ) -> Self {
        Self::new(
            ToolErrorCode::InvalidParameters,
            format!("invalid parameters for {tool}: {reason}"),
        )
        .with_suggestion("Check the tool schema and provide valid parameters")
    }

    /// Named resource not found.
    pub fn not_found(resource_type: impl std::fmt::Display, name: impl std::fmt::Display) -> Self {
        Self::new(
            ToolErrorCode::NotFound,
            format!("{resource_type} not found: {name}"),
        )
    }

    /// I/O error wrapping an underlying OS error.
    pub fn io(context: impl std::fmt::Display, error: impl std::fmt::Display) -> Self {
        Self::new(ToolErrorCode::IoError, format!("{context}: {error}"))
    }

    /// Internal/catch-all error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ToolErrorCode::Internal, message)
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, ". Suggestion: {suggestion}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ToolError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_code_and_message() {
        let err = ToolError::new(ToolErrorCode::PathNotFound, "file.txt not found");
        assert_eq!(err.to_string(), "[PATH_NOT_FOUND] file.txt not found");
    }

    #[test]
    fn display_includes_suggestion() {
        let err = ToolError::new(ToolErrorCode::Timeout, "sleep 99")
            .with_suggestion("use a shorter timeout");
        let displayed = err.to_string();
        assert!(displayed.contains("[TIMEOUT]"));
        assert!(displayed.contains("Suggestion: use a shorter timeout"));
    }

    #[test]
    fn convenience_path_not_found() {
        let err = ToolError::path_not_found("/tmp/missing.txt");
        assert_eq!(err.code, ToolErrorCode::PathNotFound);
        assert!(err.message.contains("/tmp/missing.txt"));
        assert!(err.suggestion.is_some());
    }

    #[test]
    fn convenience_command_blocked() {
        let err = ToolError::command_blocked("rm -rf /", "destructive pattern");
        assert_eq!(err.code, ToolErrorCode::CommandBlocked);
        assert!(err.details.as_ref().unwrap().contains("rm -rf /"));
    }

    #[test]
    fn convenience_command_failed() {
        let err = ToolError::command_failed("cargo test", 1);
        assert_eq!(err.code, ToolErrorCode::CommandFailed);
        assert!(err.message.contains("code 1"));
    }

    #[test]
    fn convenience_invalid_parameters() {
        let err = ToolError::invalid_parameters("bash", "command is empty");
        assert_eq!(err.code, ToolErrorCode::InvalidParameters);
        assert!(err.suggestion.is_some());
    }

    #[test]
    fn convenience_not_found() {
        let err = ToolError::not_found("skill", "my-skill");
        assert_eq!(err.code, ToolErrorCode::NotFound);
        assert!(err.message.contains("my-skill"));
    }

    #[test]
    fn converts_to_anyhow() {
        let err = ToolError::not_found("tool", "xyz");
        let anyhow_err: anyhow::Error = err.into();
        assert!(anyhow_err.to_string().contains("[NOT_FOUND]"));
    }

    #[test]
    fn code_display_roundtrip() {
        assert_eq!(ToolErrorCode::CommandFailed.to_string(), "COMMAND_FAILED");
        assert_eq!(ToolErrorCode::InvalidInput.to_string(), "INVALID_INPUT");
    }

    #[test]
    fn serialize_deserialize() {
        let err = ToolError::command_failed("make", 2)
            .with_details("build failed")
            .with_suggestion("check Makefile");
        let json = serde_json::to_string(&err).unwrap();
        let parsed: ToolError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, ToolErrorCode::CommandFailed);
        assert_eq!(parsed.message, "command exited with code 2");
        assert_eq!(parsed.details.as_deref(), Some("build failed"));
        assert_eq!(parsed.suggestion.as_deref(), Some("check Makefile"));
    }

    #[test]
    fn serialize_skips_none_fields() {
        let err = ToolError::new(ToolErrorCode::Internal, "oops");
        let json = serde_json::to_string(&err).unwrap();
        assert!(!json.contains("details"));
        assert!(!json.contains("suggestion"));
    }
}
