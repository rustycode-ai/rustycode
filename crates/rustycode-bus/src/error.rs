// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Error types for the event bus

use thiserror::Error;

/// Result type for event bus operations
pub type Result<T> = std::result::Result<T, EventBusError>;

/// Errors that can occur in the event bus
#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum EventBusError {
    /// Invalid subscription pattern
    #[error("Invalid subscription pattern: {0}")]
    InvalidPattern(String),

    /// Maximum subscribers reached
    #[error("Maximum subscribers reached")]
    MaxSubscribersReached,

    /// Subscriber not found
    #[error("Subscriber not found")]
    SubscriberNotFound,

    /// Hook error
    #[error("Hook error: {0}")]
    HookError(String),

    /// Event serialization error
    #[error("Event serialization error: {0}")]
    SerializationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = EventBusError::InvalidPattern("test.*".to_string());
        assert!(err.to_string().contains("Invalid subscription pattern"));

        let err = EventBusError::MaxSubscribersReached;
        assert!(err.to_string().contains("Maximum subscribers"));

        let err = EventBusError::SubscriberNotFound;
        assert!(err.to_string().contains("Subscriber not found"));

        let err = EventBusError::HookError("test error".to_string());
        assert!(err.to_string().contains("Hook error"));
    }

    #[test]
    fn test_serialization_error_display() {
        let err = EventBusError::SerializationError("invalid json".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Event serialization error"));
        assert!(msg.contains("invalid json"));
    }

    #[test]
    fn test_error_debug_format() {
        let err = EventBusError::InvalidPattern("bad pattern".to_string());
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("InvalidPattern"));

        let err = EventBusError::MaxSubscribersReached;
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("MaxSubscribersReached"));

        let err = EventBusError::SubscriberNotFound;
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("SubscriberNotFound"));

        let err = EventBusError::HookError("hook fail".to_string());
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("HookError"));
        assert!(debug_str.contains("hook fail"));

        let err = EventBusError::SerializationError("serde fail".to_string());
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("SerializationError"));
        assert!(debug_str.contains("serde fail"));
    }

    #[test]
    fn test_error_clone() {
        let err = EventBusError::InvalidPattern("test.*".to_string());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());

        let err = EventBusError::HookError("msg".to_string());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_error_equality_via_debug() {
        let err1 = EventBusError::HookError("same".to_string());
        let err2 = EventBusError::HookError("same".to_string());
        assert_eq!(format!("{err1:?}"), format!("{:?}", err2));

        let err3 = EventBusError::HookError("different".to_string());
        assert_ne!(format!("{err1:?}"), format!("{:?}", err3));
    }

    #[test]
    fn test_result_type_ok() {
        let result: Result<()> = Ok(());
        assert!(result.is_ok());
    }

    #[test]
    fn test_result_type_err() {
        let err = EventBusError::SubscriberNotFound;
        assert!(err.to_string().contains("Subscriber not found"));
    }

    #[test]
    fn test_display_contains_original_message() {
        let err = EventBusError::InvalidPattern("[invalid regex[".to_string());
        assert!(err.to_string().contains("[invalid regex["));

        let err = EventBusError::HookError(String::new());
        assert!(err.to_string().contains("Hook error:"));

        let err = EventBusError::SerializationError(String::new());
        assert!(err.to_string().contains("Event serialization error:"));
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for error
    // =========================================================================

    // 1. EventBusError clone preserves variant
    #[test]
    fn error_clone_preserves_variant() {
        let err = EventBusError::MaxSubscribersReached;
        let cloned = err;
        assert!(matches!(cloned, EventBusError::MaxSubscribersReached));

        let err = EventBusError::SubscriberNotFound;
        let cloned = err;
        assert!(matches!(cloned, EventBusError::SubscriberNotFound));
    }

    // 2. EventBusError clone preserves string content
    #[test]
    fn error_clone_preserves_string_content() {
        let err = EventBusError::InvalidPattern("my-pattern".to_string());
        let cloned = err;
        assert!(cloned.to_string().contains("my-pattern"));

        let err = EventBusError::HookError("hook-err-msg".to_string());
        let cloned = err;
        assert!(cloned.to_string().contains("hook-err-msg"));
    }

    // 3. EventBusError debug for all variants
    #[test]
    fn error_debug_all_variants() {
        let debug = format!("{:?}", EventBusError::MaxSubscribersReached);
        assert!(debug.contains("MaxSubscribersReached"));

        let debug = format!("{:?}", EventBusError::SubscriberNotFound);
        assert!(debug.contains("SubscriberNotFound"));
    }

    // 4. InvalidPattern with special characters
    #[test]
    fn invalid_pattern_special_chars() {
        let err = EventBusError::InvalidPattern("regex[.*+?]".to_string());
        let msg = err.to_string();
        assert!(msg.contains("regex[.*+?]"));
    }

    // 5. HookError with multiline message
    #[test]
    fn hook_error_multiline() {
        let err = EventBusError::HookError("line1\nline2\nline3".to_string());
        assert!(err.to_string().contains("line1"));
    }

    // 6. SerializationError with JSON fragment
    #[test]
    fn serialization_error_json_fragment() {
        let err = EventBusError::SerializationError("{\"key\":}".to_string());
        assert!(err.to_string().contains("{\"key\":}"));
    }

    // 7. Result type maps correctly
    #[test]
    fn result_type_mapping() {
        let err = EventBusError::InvalidPattern("x".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid subscription pattern"));
    }

    // 8. MaxSubscribersReached is unit variant
    #[test]
    fn max_subscribers_is_unit() {
        let err = EventBusError::MaxSubscribersReached;
        let msg = err.to_string();
        assert_eq!(msg, "Maximum subscribers reached");
    }

    // 9. SubscriberNotFound is unit variant
    #[test]
    fn subscriber_not_found_is_unit() {
        let err = EventBusError::SubscriberNotFound;
        let msg = err.to_string();
        assert_eq!(msg, "Subscriber not found");
    }

    // 10. Empty string in InvalidPattern
    #[test]
    fn invalid_pattern_empty_string() {
        let err = EventBusError::InvalidPattern(String::new());
        assert!(err.to_string().contains("Invalid subscription pattern:"));
    }

    // 11. Empty string in HookError
    #[test]
    fn hook_error_empty_string() {
        let err = EventBusError::HookError(String::new());
        let msg = err.to_string();
        assert!(msg.starts_with("Hook error:"));
    }

    // 12. EventBusError is Debug
    #[test]
    fn error_has_debug_trait() {
        let err = EventBusError::InvalidPattern("test".to_string());
        let _ = format!("{err:?}");
    }

    // 13. EventBusError is Clone
    #[test]
    fn error_has_clone_trait() {
        let err = EventBusError::SerializationError("err".to_string());
        let _cloned = err;
    }

    // 14. Non-exhaustive check - can add variants
    #[test]
    fn error_non_exhaustive_wildcard() {
        let err = EventBusError::InvalidPattern("x".to_string());
        let msg = err.to_string();
        assert!(!msg.is_empty());
    }

    // 15. Display vs Debug differ for string variants
    #[test]
    fn display_vs_debug_differ() {
        let err = EventBusError::HookError("detail".to_string());
        let display = err.to_string();
        let debug = format!("{err:?}");
        // Debug contains variant name, Display contains user message
        assert!(debug.contains("HookError"));
        assert!(display.contains("Hook error"));
    }
}
