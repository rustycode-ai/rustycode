//! Tool-specific error messages and recovery suggestions
//!
//! Provides helpful error messages and recovery strategies for common
//! tool failures. Inspired by cline's responses.ts which offers
//! context-specific guidance when tools fail.
//!
//! # Error Categories
//!
//! - **File not found**: Suggest using glob/grep to find the file
//! - **Permission denied**: Suggest checking file permissions or using sudo
//! - **Syntax errors**: Suggest reading the full file to identify issues
//! - **Build failures**: Suggest breaking changes into smaller steps
//! - **Repeated failures**: Suggest alternative approaches

use std::collections::HashMap;

/// Types of tool errors with specific recovery suggestions
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ToolErrorType {
    /// File or directory not found at the specified path
    FileNotFound,
    /// Permission denied when accessing a resource
    PermissionDenied,
    /// Compilation or build failed
    BuildFailed,
    /// Tests failed after changes
    TestsFailed,
    /// Syntax or parse error in code
    SyntaxError,
    /// Type checking error
    TypeError,
    /// Network or API error
    NetworkError,
    /// Timeout occurred
    Timeout,
    /// Command execution failed (non-zero exit)
    CommandFailed,
    /// Tool input was invalid or malformed
    InvalidInput,
    /// Generic error without specific categorization
    Generic,
}

impl ToolErrorType {
    /// Get a display name for this error type
    pub fn display_name(&self) -> &'static str {
        match self {
            ToolErrorType::FileNotFound => "File not found",
            ToolErrorType::PermissionDenied => "Permission denied",
            ToolErrorType::BuildFailed => "Build failed",
            ToolErrorType::TestsFailed => "Tests failed",
            ToolErrorType::SyntaxError => "Syntax error",
            ToolErrorType::TypeError => "Type error",
            ToolErrorType::NetworkError => "Network error",
            ToolErrorType::Timeout => "Timeout",
            ToolErrorType::CommandFailed => "Command failed",
            ToolErrorType::InvalidInput => "Invalid input",
            ToolErrorType::Generic => "Error",
        }
    }

    /// Get recovery suggestions for this error type
    pub fn recovery_suggestions(&self) -> Vec<String> {
        match self {
            ToolErrorType::FileNotFound => vec![
                "Use 'glob' or 'grep' to search for the correct file path".to_string(),
                "Use 'list_dir' to see available files in the directory".to_string(),
                "Check for typos in the file path".to_string(),
            ],
            ToolErrorType::PermissionDenied => vec![
                "Check file permissions with 'ls -la'".to_string(),
                "Consider using a different file location if accessible".to_string(),
                "For system operations, the command may require elevated privileges".to_string(),
            ],
            ToolErrorType::BuildFailed => vec![
                "Review the full error output to identify the specific issue".to_string(),
                "Check for missing dependencies or imports".to_string(),
                "Try building with verbose output for more details".to_string(),
                "Break down the changes into smaller, testable increments".to_string(),
            ],
            ToolErrorType::TestsFailed => vec![
                "Run the specific failing test for more details".to_string(),
                "Check if the test expectations match the implementation".to_string(),
                "Review recent changes that may have affected the test".to_string(),
                "Consider if the test needs to be updated for the new behavior".to_string(),
            ],
            ToolErrorType::SyntaxError => vec![
                "Read the full file to identify the exact error location".to_string(),
                "Check for missing brackets, parentheses, or semicolons".to_string(),
                "Use a language server or linter for real-time error detection".to_string(),
            ],
            ToolErrorType::TypeError => vec![
                "Review the type annotations in the code".to_string(),
                "Check for implicit type conversions that may fail".to_string(),
                "Use a language server for type checking".to_string(),
            ],
            ToolErrorType::NetworkError => vec![
                "Check your internet connection".to_string(),
                "Verify the API endpoint is correct and accessible".to_string(),
                "Consider retrying the operation after a brief delay".to_string(),
            ],
            ToolErrorType::Timeout => vec![
                "The operation took longer than expected".to_string(),
                "Consider breaking the operation into smaller steps".to_string(),
                "Increase the timeout if the operation is expected to take longer".to_string(),
            ],
            ToolErrorType::CommandFailed => vec![
                "Review the command output for specific error messages".to_string(),
                "Check if all required dependencies are installed".to_string(),
                "Verify the command syntax and arguments".to_string(),
                "Try running the command in a shell directly to debug".to_string(),
            ],
            ToolErrorType::InvalidInput => vec![
                "Review the tool's expected input format".to_string(),
                "Check for required parameters that may be missing".to_string(),
                "Ensure data types match the tool's expectations".to_string(),
            ],
            ToolErrorType::Generic => vec![
                "Review the error message for specific details".to_string(),
                "Try a different approach to accomplish the task".to_string(),
            ],
        }
    }
}

/// Error count tracker for repeated failures
#[derive(Debug, Clone, Default)]
pub struct ErrorTracker {
    /// Track error counts by tool name
    error_counts: HashMap<String, usize>,
    /// Track the last error for each tool
    last_errors: HashMap<String, String>,
    /// Threshold for suggesting alternative approaches
    suggest_alternative_threshold: usize,
}

impl ErrorTracker {
    /// Create a new error tracker
    pub fn new() -> Self {
        Self {
            error_counts: HashMap::new(),
            last_errors: HashMap::new(),
            suggest_alternative_threshold: 3,
        }
    }

    /// Set the threshold for suggesting alternatives
    pub fn with_threshold(mut self, threshold: usize) -> Self {
        self.suggest_alternative_threshold = threshold;
        self
    }

    /// Record an error for a tool
    pub fn record_error(&mut self, tool_name: &str, error_message: &str) {
        *self.error_counts.entry(tool_name.to_string()).or_insert(0) += 1;
        self.last_errors
            .insert(tool_name.to_string(), error_message.to_string());
    }

    /// Get the error count for a tool
    pub fn error_count(&self, tool_name: &str) -> usize {
        *self.error_counts.get(tool_name).unwrap_or(&0)
    }

    /// Get the last error message for a tool
    pub fn last_error(&self, tool_name: &str) -> Option<&String> {
        self.last_errors.get(tool_name)
    }

    /// Check if we should suggest an alternative approach
    pub fn should_suggest_alternative(&self, tool_name: &str) -> bool {
        self.error_count(tool_name) >= self.suggest_alternative_threshold
    }

    /// Get an alternative tool suggestion
    pub fn suggest_alternative_tool(&self, tool_name: &str) -> Option<String> {
        if !self.should_suggest_alternative(tool_name) {
            return None;
        }

        match tool_name {
            "bash" => Some(
                "Instead of using bash, try using a specific tool designed for this operation".to_string(),
            ),
            "edit_file" | "search_replace" => Some(
                "For repeated edit failures, try using write_file to replace the entire file content".to_string(),
            ),
            "read_file" => Some(
                "If read_file is failing, the file may not exist. Use glob to find it first".to_string(),
            ),
            _ => Some(format!(
                "The '{}' tool has failed {} times. Consider a different approach",
                tool_name,
                self.error_count(tool_name)
            )),
        }
    }

    /// Clear errors for a tool (after successful operation)
    pub fn clear_errors(&mut self, tool_name: &str) {
        self.error_counts.remove(tool_name);
        self.last_errors.remove(tool_name);
    }

    /// Clear all errors
    pub fn clear_all(&mut self) {
        self.error_counts.clear();
        self.last_errors.clear();
    }
}

/// Format a tool error with recovery suggestions
pub fn format_tool_error(
    tool_name: &str,
    error_type: ToolErrorType,
    error_message: &str,
    tracker: &ErrorTracker,
) -> String {
    let mut output = format!("{}: {}\n", error_type.display_name(), error_message);

    // Add recovery suggestions
    let suggestions = error_type.recovery_suggestions();
    if !suggestions.is_empty() {
        output.push_str("\nSuggestions:\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, suggestion));
        }
    }

    // Add alternative suggestion if repeatedly failing
    if let Some(alt) = tracker.suggest_alternative_tool(tool_name) {
        output.push_str(&format!("\n⚠️ {}\n", alt));
    }

    output
}

/// Format a file not found error with search suggestions
pub fn format_file_not_found_error(path: &str) -> String {
    format!(
        "File not found: '{}'\n\n\
         Suggestions:\n\
         1. Use 'glob' to search for files: glob '**/*{}*'\n\
         2. Use 'grep' to search content: grep '<pattern>' **/*\n\
         3. Use 'list_dir' to see files in the current directory\n\
         4. Check the file path for typos",
        path,
        if let Some(file_name) = path.rsplit('/').next() {
            file_name
        } else {
            path
        }
    )
}

/// Format a command failure with next steps
pub fn format_command_failure(command: &str, exit_code: i32, stderr: Option<&str>) -> String {
    let mut output = format!("Command failed with exit code {}: {}\n", exit_code, command);

    if let Some(err) = stderr {
        if !err.is_empty() {
            output.push_str(&format!("\nError output:\n{}\n", err));
        }
    }

    output.push_str("\nSuggestions:\n");
    output.push_str("1. Review the error output for specific issues\n");
    output.push_str("2. Check if required dependencies are installed\n");
    output.push_str("3. Verify the command syntax and arguments\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_type_display_names() {
        assert_eq!(ToolErrorType::FileNotFound.display_name(), "File not found");
        assert_eq!(ToolErrorType::BuildFailed.display_name(), "Build failed");
    }

    #[test]
    fn test_recovery_suggestions() {
        let suggestions = ToolErrorType::FileNotFound.recovery_suggestions();
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.contains("glob")));
    }

    #[test]
    fn test_error_tracker() {
        let mut tracker = ErrorTracker::new();
        assert_eq!(tracker.error_count("bash"), 0);

        tracker.record_error("bash", "command not found");
        assert_eq!(tracker.error_count("bash"), 1);

        tracker.record_error("bash", "permission denied");
        assert_eq!(tracker.error_count("bash"), 2);
        assert_eq!(
            tracker.last_error("bash"),
            Some(&"permission denied".to_string())
        );
    }

    #[test]
    fn test_suggest_alternative_threshold() {
        let mut tracker = ErrorTracker::new().with_threshold(2);

        tracker.record_error("bash", "error 1");
        assert!(!tracker.should_suggest_alternative("bash"));

        tracker.record_error("bash", "error 2");
        assert!(tracker.should_suggest_alternative("bash"));
        assert!(tracker.suggest_alternative_tool("bash").is_some());
    }

    #[test]
    fn test_format_file_not_found() {
        let error = format_file_not_found_error("src/main.rs");
        assert!(error.contains("File not found"));
        assert!(error.contains("glob"));
        assert!(error.contains("list_dir"));
    }

    #[test]
    fn test_clear_errors() {
        let mut tracker = ErrorTracker::new();
        tracker.record_error("bash", "error");
        assert_eq!(tracker.error_count("bash"), 1);

        tracker.clear_errors("bash");
        assert_eq!(tracker.error_count("bash"), 0);
    }
}
