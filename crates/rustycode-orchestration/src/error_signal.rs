use serde::{Deserialize, Serialize};

pub type SignalCategory = ErrorCategory;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorContext {
    pub component: String,
    pub operation: String,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ErrorCategory {
    SyntaxError,
    CompileError,
    TypeError,
    LogicError,
    PermissionDenied,
    DiskFull,
    ToolTimeout,
    ContextLengthExceeded,
    Fatal,
    Internal,
    Custom(String),
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SyntaxError => write!(f, "syntax_error"),
            Self::CompileError => write!(f, "compile_error"),
            Self::TypeError => write!(f, "type_error"),
            Self::LogicError => write!(f, "logic_error"),
            Self::PermissionDenied => write!(f, "permission_denied"),
            Self::DiskFull => write!(f, "disk_full"),
            Self::ToolTimeout => write!(f, "tool_timeout"),
            Self::ContextLengthExceeded => write!(f, "context_length_exceeded"),
            Self::Fatal => write!(f, "fatal"),
            Self::Internal => write!(f, "internal"),
            Self::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}

impl ErrorCategory {
    pub const fn is_recoverable(&self) -> bool {
        !matches!(self, Self::Fatal)
    }

    pub const fn escalate_to_tier(&self) -> u8 {
        match self {
            Self::PermissionDenied | Self::DiskFull | Self::ToolTimeout => 2,
            Self::SyntaxError
            | Self::CompileError
            | Self::TypeError
            | Self::LogicError
            | Self::ContextLengthExceeded
            | Self::Internal
            | Self::Custom(_) => 3,
            Self::Fatal => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorSignal {
    pub category: ErrorCategory,
    pub exit_code: Option<i32>,
    pub message: String,
    pub step_id: String,
    pub tool_name: String,
    pub captured_at: chrono::DateTime<chrono::Utc>,
}

impl ErrorSignal {
    pub fn new(
        category: ErrorCategory,
        exit_code: Option<i32>,
        message: String,
        step_id: String,
        tool_name: String,
    ) -> Self {
        let truncated = if message.len() > 2048 {
            let mut end = 2048;
            while end > 0 && !message.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}... [truncated]", &message[..end])
        } else {
            message
        };
        Self {
            category,
            exit_code,
            message: truncated,
            step_id,
            tool_name,
            captured_at: chrono::Utc::now(),
        }
    }
}

#[derive(Default)]
pub struct ErrorClassifier {
    patterns: Vec<(regex::Regex, ErrorCategory)>,
}

impl ErrorClassifier {
    #[allow(clippy::unwrap_used)]
    pub fn new() -> Self {
        let patterns = vec![
            // Syntax / parse errors
            (
                regex::Regex::new(r"(?i)syntax error|unexpected token|parse error|unexpected end of (file|input)|unterminated string").unwrap(),
                ErrorCategory::SyntaxError,
            ),
            // Compile / build errors (Rust, Go, C/C++, Java, TypeScript, Python)
            (
                regex::Regex::new(r"(?i)error\[E\d+\]|compilation failed|compile error|error: aborting due to|cannot find (?:value|function|module|type|trait)|use of undeclared|unresolved import|package [\w.-]+ is not in GOROOT|ld: (?:undefined|duplicate) symbol|linker command failed|Cannot find name|TS\d+:|no matching package named").unwrap(),
                ErrorCategory::CompileError,
            ),
            // Type errors
            (
                regex::Regex::new(r"(?i)TypeError|type mismatch|undefined (symbol|reference)|expected .+, found .+|mismatched types|cast to .+ failed").unwrap(),
                ErrorCategory::TypeError,
            ),
            // Runtime logic errors (assert, panic, segfault, OOM)
            (
                regex::Regex::new(r"(?i)assertion failed|panic!|thread.*panicked|segmentation fault|segfault|SIGSEGV|out of memory|OOM|killed|fatal error|abort").unwrap(),
                ErrorCategory::LogicError,
            ),
            // Module / dependency / import errors (must come before network patterns —
            // ENOTFOUND would false-positive on "ModuleNotFoundError")
            (
                regex::Regex::new(r"(?i)ModuleNotFoundError|import error|cannot resolve module|package not found|dependency .* not found|module not found").unwrap(),
                ErrorCategory::CompileError,
            ),
            // File not found errors
            (
                regex::Regex::new(r"(?i)no such file or directory|ENOENT").unwrap(),
                ErrorCategory::CompileError,
            ),
            // Network / connectivity errors
            (
                regex::Regex::new(r"(?i)connection refused|connection reset|network (is unreachable|error)|ETIMEDOUT|ECONNREFUSED|ECONNRESET|ENOTFOUND|DNS lookup failed|curl: \(\d+\)").unwrap(),
                ErrorCategory::LogicError,
            ),
            (
                regex::Regex::new(r"(?i)permission denied|EACCES|operation not permitted|EPERM|access denied").unwrap(),
                ErrorCategory::PermissionDenied,
            ),
            (
                regex::Regex::new(r"(?i)no space left|disk full|ENOSPC|write error: No space left").unwrap(),
                ErrorCategory::DiskFull,
            ),
            (
                regex::Regex::new(r"(?i)context length exceeded|too many tokens|max tokens|context window|token limit|maximum context length").unwrap(),
                ErrorCategory::ContextLengthExceeded,
            ),
            (
                regex::Regex::new(r"(?i)(tool |command )?timed? ?out|timeout|deadline exceeded|ETIMEDOUT|watchdog").unwrap(),
                ErrorCategory::ToolTimeout,
            ),
        ];
        Self { patterns }
    }

    pub fn classify(&self, output: &str, exit_code: i32) -> ErrorCategory {
        for (pattern, category) in &self.patterns {
            if pattern.is_match(output) {
                return category.clone();
            }
        }
        match exit_code {
            13 => ErrorCategory::PermissionDenied,
            28 => ErrorCategory::DiskFull,
            124 => ErrorCategory::ToolTimeout,
            _ => ErrorCategory::Custom(format!("ExitCode{exit_code}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_error_category_is_recoverable() {
        assert!(ErrorCategory::SyntaxError.is_recoverable());
        assert!(ErrorCategory::LogicError.is_recoverable());
        assert!(ErrorCategory::ToolTimeout.is_recoverable());
    }

    #[test]
    fn test_error_category_fatal_not_recoverable() {
        assert!(!ErrorCategory::Fatal.is_recoverable());
    }

    #[test]
    fn test_error_category_escalate_tier() {
        assert_eq!(ErrorCategory::PermissionDenied.escalate_to_tier(), 2);
        assert_eq!(ErrorCategory::SyntaxError.escalate_to_tier(), 3);
        assert_eq!(ErrorCategory::Fatal.escalate_to_tier(), 4);
        assert_eq!(ErrorCategory::Custom("x".into()).escalate_to_tier(), 3);
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::SyntaxError.to_string(), "syntax_error");
        assert_eq!(ErrorCategory::Fatal.to_string(), "fatal");
        assert_eq!(
            ErrorCategory::Custom("foo".into()).to_string(),
            "custom:foo"
        );
    }

    #[test]
    fn test_error_signal_new_basic() {
        let signal = ErrorSignal::new(
            ErrorCategory::LogicError,
            Some(1),
            "test error".into(),
            "step-1".into(),
            "bash".into(),
        );
        assert_eq!(signal.category, ErrorCategory::LogicError);
        assert_eq!(signal.exit_code, Some(1));
        assert_eq!(signal.message, "test error");
        assert_eq!(signal.step_id, "step-1");
        assert_eq!(signal.tool_name, "bash");
    }

    #[test]
    fn test_error_signal_truncates_long_message() {
        let long_msg = "x".repeat(3000);
        let signal = ErrorSignal::new(
            ErrorCategory::CompileError,
            None,
            long_msg,
            "s1".into(),
            "bash".into(),
        );
        assert!(signal.message.len() <= 2100);
        assert!(signal.message.contains("truncated"));
    }

    #[test]
    fn test_error_signal_serialization() {
        let signal = ErrorSignal::new(
            ErrorCategory::TypeError,
            Some(42),
            "type mismatch".into(),
            "s1".into(),
            "cargo".into(),
        );
        let json = serde_json::to_string(&signal).unwrap();
        let back: ErrorSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category, ErrorCategory::TypeError);
        assert_eq!(back.exit_code, Some(42));
        assert_eq!(back.message, "type mismatch");
    }

    #[test]
    fn test_error_classifier_syntax_error() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("syntax error at line 5", 1),
            ErrorCategory::SyntaxError
        );
        assert_eq!(
            classifier.classify("unexpected token '}'", 1),
            ErrorCategory::SyntaxError
        );
    }

    #[test]
    fn test_error_classifier_compile_error() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("error[E0308]: mismatched types", 1),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_error_classifier_type_error() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("TypeError: undefined is not a function", 1),
            ErrorCategory::TypeError
        );
    }

    #[test]
    fn test_error_classifier_permission_denied() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("permission denied", 1),
            ErrorCategory::PermissionDenied
        );
    }

    #[test]
    fn test_error_classifier_falls_back_to_exit_code() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("unknown error", 13),
            ErrorCategory::PermissionDenied
        );
        assert_eq!(
            classifier.classify("unknown error", 28),
            ErrorCategory::DiskFull
        );
        assert_eq!(
            classifier.classify("unknown error", 124),
            ErrorCategory::ToolTimeout
        );
    }

    #[test]
    fn test_error_classifier_custom_fallback() {
        let classifier = ErrorClassifier::new();
        let result = classifier.classify("mysterious error", 99);
        assert!(matches!(result, ErrorCategory::Custom(s) if s == "ExitCode99"));
    }

    #[test]
    fn test_signal_category_is_type_alias() {
        let _: SignalCategory = ErrorCategory::SyntaxError;
    }

    #[test]
    fn test_error_context_fields() {
        let ctx = ErrorContext {
            component: "editor".into(),
            operation: "patch".into(),
            task_id: Some("t1".into()),
        };
        assert_eq!(ctx.component, "editor");
        assert_eq!(ctx.operation, "patch");
        assert_eq!(ctx.task_id, Some("t1".into()));
    }

    #[test]
    fn test_truncation_with_multibyte_utf8() {
        // Create a message with multibyte characters that exceeds 2048 bytes
        let base = "日本語"; // 3 bytes per char
        let repeat_count = 2048 / 9 + 10; // guaranteed to exceed 2048 bytes
        let long_msg = base.repeat(repeat_count);
        assert!(long_msg.len() > 2048, "Message should exceed 2048 bytes");

        let signal = ErrorSignal::new(
            ErrorCategory::Internal,
            None,
            long_msg,
            "s1".into(),
            "bash".into(),
        );
        assert!(signal.message.contains("[truncated]"));
        // Verify the message doesn't panic and is valid UTF-8
        assert!(signal.message.len() <= 2100);
    }

    #[test]
    fn test_error_classifier_disk_full() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("no space left on device", 1),
            ErrorCategory::DiskFull
        );
        assert_eq!(classifier.classify("disk full", 1), ErrorCategory::DiskFull);
    }

    #[test]
    fn test_error_classifier_timeout() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("tool timed out", 1),
            ErrorCategory::ToolTimeout
        );
        assert_eq!(
            classifier.classify("timeout after 30s", 1),
            ErrorCategory::ToolTimeout
        );
    }

    #[test]
    fn test_error_classifier_context_length() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("context length exceeded", 1),
            ErrorCategory::ContextLengthExceeded
        );
    }

    #[test]
    fn test_error_category_custom_display() {
        assert_eq!(
            ErrorCategory::Custom("MyError".into()).to_string(),
            "custom:MyError"
        );
    }

    #[test]
    fn test_error_category_all_recoverable_except_fatal() {
        let categories = [
            ErrorCategory::SyntaxError,
            ErrorCategory::CompileError,
            ErrorCategory::TypeError,
            ErrorCategory::LogicError,
            ErrorCategory::PermissionDenied,
            ErrorCategory::DiskFull,
            ErrorCategory::ToolTimeout,
            ErrorCategory::ContextLengthExceeded,
            ErrorCategory::Internal,
            ErrorCategory::Custom("x".into()),
        ];
        for cat in &categories {
            assert!(cat.is_recoverable(), "{cat:?} should be recoverable");
        }
        assert!(!ErrorCategory::Fatal.is_recoverable());
    }

    #[test]
    fn test_error_category_all_display() {
        let cases = [
            (ErrorCategory::SyntaxError, "syntax_error"),
            (ErrorCategory::CompileError, "compile_error"),
            (ErrorCategory::TypeError, "type_error"),
            (ErrorCategory::LogicError, "logic_error"),
            (ErrorCategory::PermissionDenied, "permission_denied"),
            (ErrorCategory::DiskFull, "disk_full"),
            (ErrorCategory::ToolTimeout, "tool_timeout"),
            (
                ErrorCategory::ContextLengthExceeded,
                "context_length_exceeded",
            ),
            (ErrorCategory::Internal, "internal"),
        ];
        for (cat, expected) in &cases {
            assert_eq!(cat.to_string(), *expected);
        }
    }

    #[test]
    fn test_error_signal_no_truncation_short_message() {
        let signal = ErrorSignal::new(
            ErrorCategory::LogicError,
            Some(1),
            "short".into(),
            "s1".into(),
            "bash".into(),
        );
        assert_eq!(signal.message, "short");
        assert!(!signal.message.contains("[truncated]"));
    }

    #[test]
    fn test_classify_rust_panic() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("thread 'main' panicked at 'assertion failed: x > 0'", 101),
            ErrorCategory::LogicError
        );
    }

    #[test]
    fn test_classify_segv() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("Segmentation fault (core dumped)", 139),
            ErrorCategory::LogicError
        );
    }

    #[test]
    fn test_classify_connection_refused() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("connection refused (os error 111)", 1),
            ErrorCategory::LogicError
        );
    }

    #[test]
    fn test_classify_module_not_found() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("ModuleNotFoundError: No module named 'numpy'", 1),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_rust_cannot_find() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify(
                "error[E0433]: failed to resolve: use of undeclared crate `serde`",
                1
            ),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_ts_error() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("TS2307: Cannot find module 'express'", 1),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_linker_error() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("ld: undefined symbol: _ZN3foo3barEv", 1),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_out_of_memory() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("memory allocation of 8192 bytes failed: out of memory", 137),
            ErrorCategory::LogicError
        );
    }

    #[test]
    fn test_classify_context_window() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("This model's maximum context length is 8192 tokens", 400),
            ErrorCategory::ContextLengthExceeded
        );
    }

    #[test]
    fn test_classify_cargo_no_package() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("error: no matching package named `fake-crate` found", 101),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_enoent() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("No such file or directory (os error 2)", 1),
            ErrorCategory::CompileError
        );
    }

    #[test]
    fn test_classify_access_denied() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("Access denied for user 'root'@'localhost'", 1),
            ErrorCategory::PermissionDenied
        );
    }

    #[test]
    fn test_classify_deadline_exceeded() {
        let classifier = ErrorClassifier::new();
        assert_eq!(
            classifier.classify("deadline exceeded after 30s", 1),
            ErrorCategory::ToolTimeout
        );
    }
}
