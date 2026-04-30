//! Error classification for the TUI.
//!
//! Categorizes errors for display by `show_error()` in message_ops.rs.

use anyhow::Error as AnyhowError;

/// Types of errors that can occur in the TUI
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// Network-related errors (timeout, connection refused, DNS failure)
    Network,
    /// Authentication errors (invalid API key, expired token)
    Authentication,
    /// Rate limiting errors (429, quota exceeded)
    RateLimit,
    /// Streaming errors (connection interrupted, incomplete response)
    Streaming,
    /// Tool execution errors
    ToolExecution,
    /// File operation errors
    FileOperation,
    /// Configuration errors
    Configuration,
    /// Unknown/unclassified errors
    Other,
}

/// Classify an error into a category for user-friendly display.
pub fn classify_error(error: &AnyhowError) -> ErrorCategory {
    let error_msg = error.to_string().to_lowercase();

    // Network errors
    if error_msg.contains("connection refused")
        || error_msg.contains("timeout")
        || error_msg.contains("timed out")
        || error_msg.contains("network")
        || error_msg.contains("dns")
        || error_msg.contains("host not found")
        || error_msg.contains("connection reset")
        || error_msg.contains("unreachable")
    {
        return ErrorCategory::Network;
    }

    // Authentication errors
    if error_msg.contains("401")
        || error_msg.contains("403")
        || error_msg.contains("unauthorized")
        || error_msg.contains("forbidden")
        || error_msg.contains("invalid api key")
        || error_msg.contains("authentication")
        || error_msg.contains("auth failed")
    {
        return ErrorCategory::Authentication;
    }

    // Rate limiting errors
    if error_msg.contains("429")
        || error_msg.contains("rate limit")
        || error_msg.contains("quota")
        || error_msg.contains("too many requests")
        || error_msg.contains("throttle")
    {
        return ErrorCategory::RateLimit;
    }

    // Streaming errors
    if error_msg.contains("stream")
        || error_msg.contains("incomplete")
        || error_msg.contains("interrupted")
        || error_msg.contains("connection closed")
    {
        return ErrorCategory::Streaming;
    }

    // Tool execution errors
    if error_msg.contains("tool")
        || error_msg.contains("execution")
        || error_msg.contains("command failed")
        || error_msg.contains("exit code")
    {
        return ErrorCategory::ToolExecution;
    }

    // File operation errors
    if error_msg.contains("no such file")
        || error_msg.contains("permission denied")
        || error_msg.contains("not found")
        || error_msg.contains("could not read")
        || error_msg.contains("could not write")
    {
        return ErrorCategory::FileOperation;
    }

    // Configuration errors
    if error_msg.contains("config")
        || error_msg.contains("missing")
        || error_msg.contains("required")
        || error_msg.contains("invalid")
    {
        return ErrorCategory::Configuration;
    }

    ErrorCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_network_error() {
        let error = anyhow::anyhow!("connection refused");
        assert_eq!(classify_error(&error), ErrorCategory::Network);

        let error = anyhow::anyhow!("request timed out");
        assert_eq!(classify_error(&error), ErrorCategory::Network);
    }

    #[test]
    fn test_classify_auth_error() {
        let error = anyhow::anyhow!("401 unauthorized");
        assert_eq!(classify_error(&error), ErrorCategory::Authentication);

        let error = anyhow::anyhow!("invalid api key");
        assert_eq!(classify_error(&error), ErrorCategory::Authentication);
    }

    #[test]
    fn test_classify_rate_limit_error() {
        let error = anyhow::anyhow!("429 too many requests");
        assert_eq!(classify_error(&error), ErrorCategory::RateLimit);

        let error = anyhow::anyhow!("rate limit exceeded");
        assert_eq!(classify_error(&error), ErrorCategory::RateLimit);
    }

    #[test]
    fn test_classify_streaming_error() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("stream interrupted")),
            ErrorCategory::Streaming
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("incomplete response")),
            ErrorCategory::Streaming
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("connection closed prematurely")),
            ErrorCategory::Streaming
        );
    }

    #[test]
    fn test_classify_tool_execution_error() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("tool execution failed")),
            ErrorCategory::ToolExecution
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("command failed with exit code 1")),
            ErrorCategory::ToolExecution
        );
    }

    #[test]
    fn test_classify_file_operation_error() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("no such file or directory")),
            ErrorCategory::FileOperation
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("permission denied")),
            ErrorCategory::FileOperation
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("could not read file")),
            ErrorCategory::FileOperation
        );
    }

    #[test]
    fn test_classify_configuration_error() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("config file missing")),
            ErrorCategory::Configuration
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("invalid configuration")),
            ErrorCategory::Configuration
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("missing required field")),
            ErrorCategory::Configuration
        );
    }

    #[test]
    fn test_classify_unknown_error() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("something completely unexpected")),
            ErrorCategory::Other
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("oops")),
            ErrorCategory::Other
        );
    }

    #[test]
    fn test_error_category_equality() {
        assert_eq!(ErrorCategory::Network, ErrorCategory::Network);
        assert_ne!(ErrorCategory::Network, ErrorCategory::Other);
    }

    #[test]
    fn test_classify_network_dns() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("dns resolution failed")),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("host not found")),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("network unreachable")),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("connection reset by peer")),
            ErrorCategory::Network
        );
    }

    #[test]
    fn test_classify_auth_variants() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("403 forbidden")),
            ErrorCategory::Authentication
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("authentication failed")),
            ErrorCategory::Authentication
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("auth failed for user")),
            ErrorCategory::Authentication
        );
    }

    #[test]
    fn test_classify_rate_limit_variants() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("quota exceeded")),
            ErrorCategory::RateLimit
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("throttle limit reached")),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn test_classify_is_case_insensitive() {
        assert_eq!(
            classify_error(&anyhow::anyhow!("CONNECTION REFUSED")),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error(&anyhow::anyhow!("Rate Limit Exceeded")),
            ErrorCategory::RateLimit
        );
    }
}
