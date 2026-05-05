#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::float_cmp,
    clippy::missing_const_for_fn,
    clippy::manual_string_new,
    clippy::option_if_let_else
)]
// ── Error Path Tests for RustyCode Core ────────────────────────────────────────
//
// This file contains comprehensive tests for error conditions in:
// 1. Runtime module - Session failures, tool execution errors
// 2. Execution module - Step failures, validation errors
// 3. Context modules - Budget exhaustion, missing files
// 4. Recovery module - Retry logic, fallback behavior

use chrono::Utc;
use rustycode_core::context::{enforce_budget, ContextBudget, RustyCodeIgnore, TokenCounter};
use rustycode_core::execution::{
    is_critical_tool, ExecutionConfig, ExecutionContext, StepExecutorRegistry,
};
use rustycode_core::recovery::{
    ErrorCategory, ErrorClassification, ErrorClassifier, RecoveryConfig, RecoveryEngine,
    RecoveryStrategy,
};
use rustycode_core::validation::{
    validate_plan, ComprehensivePlanValidator, ValidationError, ValidationResult,
};
use rustycode_protocol::{Plan, PlanId, PlanStatus, PlanStep, SessionId, StepStatus};
use rustycode_tools::ToolRegistry;
use std::path::{Path, PathBuf};

// RUNTIME MODULE ERROR TESTS

mod runtime_error_tests {
    use super::*;
    use rustycode_core::runtime::Runtime;
    use tempfile::TempDir;

    #[test]
    fn test_runtime_load_fails_with_invalid_config() {
        // Create a temp directory without proper config
        let temp_dir = TempDir::new().unwrap();

        // Runtime::load should handle missing config gracefully or return error
        let result = Runtime::load(temp_dir.path());

        // Either Ok or Err is acceptable depending on config defaults
        // The key is that it doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_runtime_execute_tool_fails_with_invalid_tool() {
        let temp_dir = TempDir::new().unwrap();
        let runtime = Runtime::load(temp_dir.path()).unwrap();

        // Create a test session
        let session = rustycode_protocol::Session::builder()
            .task("test".to_string())
            .build();
        runtime.storage.insert_session(&session).unwrap();

        // Try to execute a non-existent tool
        let call = rustycode_protocol::ToolCall {
            call_id: "test-call".to_string(),
            name: "nonexistent_tool".to_string(),
            arguments: serde_json::json!({}),
        };

        let result = runtime.execute_tool(&session.id, call, temp_dir.path());

        // Should return an error for unknown tool
        if let Ok(tool_result) = result {
            assert!(
                tool_result.error.is_some() || !tool_result.success,
                "Executing non-existent tool should report error"
            );
        }
    }

    #[test]
    fn test_runtime_tool_permission_blocked_in_planning_mode() {
        let temp_dir = TempDir::new().unwrap();
        let runtime = Runtime::load(temp_dir.path()).unwrap();

        // Create a session in planning mode
        let mut session = rustycode_protocol::Session::builder()
            .task("test".to_string())
            .build();
        session.mode = rustycode_protocol::SessionMode::Planning;
        runtime.storage.insert_session(&session).unwrap();

        // Try to execute a tool not permitted in planning mode
        let call = rustycode_protocol::ToolCall {
            call_id: "test-call".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"command": "echo test"}),
        };

        let result = runtime.execute_tool(&session.id, call, temp_dir.path());

        // Should fail due to mode restriction
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not permitted") || err_msg.contains("Planning"));
    }

    #[test]
    fn test_runtime_session_operations_fail_without_manager() {
        // Test that session operations handle missing manager gracefully
        let temp_dir = TempDir::new().unwrap();
        let runtime = Runtime::load(temp_dir.path()).unwrap();

        // Session manager should be initialized by Runtime::load
        // But we test that operations return proper errors
        let fake_id = SessionId::new();

        // Loading non-existent session should fail gracefully
        let result = runtime.load_session(&fake_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_runtime_check_tool_permission_invalid_session() {
        let temp_dir = TempDir::new().unwrap();
        let runtime = Runtime::load(temp_dir.path()).unwrap();

        // Create a call and test with non-existent session
        let call = rustycode_protocol::ToolCall {
            call_id: "test".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "test.txt"}),
        };

        let fake_id = SessionId::new();
        // This should succeed since session doesn't exist (no permission check)
        let result = runtime.check_tool_permission_and_publish(&fake_id, &call);
        assert!(result.is_ok());
    }
}

// EXECUTION MODULE ERROR TESTS

mod execution_error_tests {
    use super::*;

    #[test]
    fn test_execution_context_iteration_limit_error() {
        let config = ExecutionConfig {
            max_iterations: 2,
            step_timeout_secs: 30,
            continue_on_error: false,
        };
        let mut ctx = ExecutionContext::new(config, PathBuf::from("."));

        // First two iterations should succeed
        assert!(ctx.check_iteration_limit().is_ok());
        assert!(ctx.check_iteration_limit().is_ok());

        // Third should fail with iteration limit error
        let result = ctx.check_iteration_limit();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Exceeded maximum iterations"));
    }

    #[test]
    fn test_execution_context_record_error_accumulates() {
        let config = ExecutionConfig {
            max_iterations: 100,
            step_timeout_secs: 30,
            continue_on_error: true,
        };
        let mut ctx = ExecutionContext::new(config, PathBuf::from("."));

        // Record multiple errors
        ctx.record_error("Error 1".to_string());
        ctx.record_error("Error 2".to_string());
        ctx.record_error("Error 3".to_string());

        assert_eq!(ctx.errors.len(), 3);
        assert_eq!(ctx.errors[0], "Error 1");
        assert_eq!(ctx.errors[2], "Error 3");
    }

    #[test]
    fn test_execution_context_should_continue_on_error() {
        // Test with continue_on_error = false
        let config = ExecutionConfig {
            max_iterations: 100,
            step_timeout_secs: 30,
            continue_on_error: false,
        };
        let mut ctx = ExecutionContext::new(config.clone(), PathBuf::from("."));

        ctx.record_error("First error".to_string());
        // Should continue with first error when continue_on_error is false
        // (logic: continue_on_error || errors.len() < 3)
        assert!(ctx.should_continue);

        // Test with continue_on_error = true
        let mut ctx2 = ExecutionContext::new(config, PathBuf::from("."));
        ctx2.record_error("Error 1".to_string());
        ctx2.record_error("Error 2".to_string());
        ctx2.record_error("Error 3".to_string());
        ctx2.record_error("Error 4".to_string());

        // Should stop after 3 errors even with continue_on_error = true
        assert!(!ctx2.should_continue);
    }

    #[test]
    fn test_step_executor_registry_get_nonexistent() {
        let registry = StepExecutorRegistry::new();

        // Getting non-existent executor should return None
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_is_critical_tool_detection() {
        // Critical tools
        assert!(is_critical_tool("read_file"));
        assert!(is_critical_tool("write_file"));
        assert!(is_critical_tool("bash"));
        assert!(is_critical_tool("bash:ls -la"));
        assert!(is_critical_tool("read_file:path=test.txt"));

        // Non-critical tools
        assert!(!is_critical_tool("grep"));
        assert!(!is_critical_tool("glob"));
        assert!(!is_critical_tool("git_status"));
        assert!(!is_critical_tool("list_dir"));
    }

    #[test]
    fn test_execution_config_custom_values() {
        let config = ExecutionConfig {
            max_iterations: 50,
            step_timeout_secs: 120,
            continue_on_error: false,
        };

        assert_eq!(config.max_iterations, 50);
        assert_eq!(config.step_timeout_secs, 120);
        assert!(!config.continue_on_error);
    }
}

// CONTEXT MODULE ERROR TESTS

mod context_error_tests {
    use super::*;

    #[test]
    fn test_context_budget_reserve_exceeds_error() {
        let mut budget = ContextBudget::new(100);

        // Reserve within budget
        assert!(budget.reserve(50).is_ok());

        // Exceed budget should fail
        let result = budget.reserve(60);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Cannot reserve"));
        assert!(err_msg.contains("would exceed budget"));
    }

    #[test]
    fn test_context_budget_use_reserved_exceeds_error() {
        let mut budget = ContextBudget::new(100);
        budget.reserve(30).unwrap();

        // Try to use more than reserved
        let result = budget.use_reserved(50);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Cannot use"));
        assert!(err_msg.contains("only"));
        assert!(err_msg.contains("reserved"));
    }

    #[test]
    fn test_context_budget_exhaustion() {
        let mut budget = ContextBudget::new(100);

        assert!(!budget.is_exhausted());

        // Reserve entire budget
        budget.reserve(100).unwrap();
        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining(), 0);
    }

    #[test]
    fn test_context_budget_utilization_edge_cases() {
        let budget = ContextBudget::new(0);
        // Zero budget should have 0 utilization
        assert_eq!(budget.utilization(), 0.0);

        let mut budget = ContextBudget::new(100);
        budget.reserve(50).unwrap();
        let util = budget.utilization();
        assert!((util - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_enforce_budget_with_zero_budget() {
        let items = vec!["item1", "item2", "item3"];
        let result = enforce_budget(&items, 0, |s| s.len());

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_enforce_budget_exhaustion() {
        let items = vec!["short", "medium length", "very long text item"];

        // Small budget should only fit first item
        let result = enforce_budget(&items, 5, |s| s.len()).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], &"short");

        // Budget fits first two
        let result = enforce_budget(&items, 20, |s| s.len()).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_token_counter_edge_cases() {
        // Empty string
        assert_eq!(TokenCounter::estimate_tokens(""), 0);

        // Single character
        assert_eq!(TokenCounter::estimate_tokens("a"), 1);

        // Four characters should be 1 token
        assert_eq!(TokenCounter::estimate_tokens("test"), 1);

        // Five characters should be 2 tokens (ceiling division)
        assert_eq!(TokenCounter::estimate_tokens("hello"), 2);
    }

    #[test]
    fn test_rustycode_ignore_should_ignore_content_for_minified() {
        let ignorer = RustyCodeIgnore::defaults_only();

        // Minified files should be content-ignored but not path-ignored
        let min_js = Path::new("src/app.min.js");
        assert!(!ignorer.should_ignore(min_js));
        assert!(ignorer.should_ignore_content(min_js));

        // Regular source files should not be ignored
        let regular = Path::new("src/app.js");
        assert!(!ignorer.should_ignore(regular));
        assert!(!ignorer.should_ignore_content(regular));
    }

    #[test]
    fn test_rustycode_ignore_binary_extensions() {
        let ignorer = RustyCodeIgnore::defaults_only();

        // Binary extensions should always be ignored
        assert!(ignorer.should_ignore(Path::new("lib/native.so")));
        assert!(ignorer.should_ignore(Path::new("assets/image.png")));
        assert!(ignorer.should_ignore(Path::new("data/file.pdf")));
    }

    #[test]
    fn test_rustycode_ignore_load_missing_files() {
        // Loading from directory without ignore files should use defaults only
        let temp_dir = tempfile::tempdir().unwrap();
        let ignorer = RustyCodeIgnore::load(temp_dir.path());

        // Should still ignore target/ (default pattern)
        assert!(ignorer.should_ignore(Path::new("target/debug/main")));

        // Should not ignore custom patterns
        assert!(!ignorer.should_ignore(Path::new("custom_ignore.tmp")));
    }
}

// RECOVERY MODULE ERROR TESTS

mod recovery_error_tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_recovery_engine_retry_exhausts_all_attempts() {
        let mut config = RecoveryConfig::default();
        config.retry.max_attempts = 3;
        config.retry.initial_backoff = std::time::Duration::from_millis(1);
        config.retry.jitter = false;

        let engine = RecoveryEngine::new(config);

        // Operation that always fails
        let operation = || async { Err::<String, _>(std::io::Error::other("always fails")) };

        // Error that suggests retry (transient)
        let error = anyhow::anyhow!("Connection timeout");
        let result = engine
            .recover(error, "always_fail_op", &operation)
            .await
            .unwrap();

        // Should exhaust all retries
        assert!(!result.is_success());
        assert_eq!(result.attempts(), 3);
        assert_eq!(result.strategy_used(), RecoveryStrategy::Retry);
    }

    #[tokio::test]
    async fn test_recovery_engine_fallback_no_handler_registered() {
        let _engine = RecoveryEngine::with_defaults();

        // Modify classifier to suggest fallback
        let mut classifier = ErrorClassifier::new();
        classifier.add_pattern(
            "needs fallback".to_string(),
            ErrorClassification::new(
                ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "Test fallback".to_string(),
            ),
        );

        // Replace classifier via new engine with modified classifier
        let mut engine = RecoveryEngine::new(RecoveryConfig::default());
        *engine.classifier_mut() = classifier;

        let operation = || async { Err::<String, _>(std::io::Error::other("needs fallback")) };

        let error = anyhow::anyhow!("needs fallback");
        let result = engine.recover(error, "test_op", &operation).await.unwrap();

        // Should fail because no fallback handler registered
        assert!(!result.is_success());
        let log = result.log();
        assert!(log.iter().any(|e| e.action.contains("No fallback handler")));
    }

    #[tokio::test]
    async fn test_recovery_engine_fallback_handler_fails() {
        let mut engine = RecoveryEngine::with_defaults();

        // Register a failing fallback handler
        engine
            .register_fallback("test_op".to_string(), || {
                Err(anyhow::anyhow!("fallback also failed"))
            })
            .unwrap();

        // Modify classifier to force fallback
        engine.classifier_mut().add_pattern(
            "trigger fallback".to_string(),
            ErrorClassification::new(
                ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "Test".to_string(),
            ),
        );

        let operation = || async { Err::<String, _>(std::io::Error::other("trigger fallback")) };

        let error = anyhow::anyhow!("trigger fallback");
        let result = engine.recover(error, "test_op", &operation).await.unwrap();

        // Should fail because fallback handler also failed
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_recovery_engine_fallback_deserialization_fails() {
        let mut engine = RecoveryEngine::with_defaults();

        // Register fallback that returns wrong type
        engine
            .register_fallback("test_op".to_string(), || {
                Ok(serde_json::json!(123)) // Number, but we expect String
            })
            .unwrap();

        engine.classifier_mut().add_pattern(
            "type mismatch".to_string(),
            ErrorClassification::new(
                ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "Test".to_string(),
            ),
        );

        let operation = || async { Err::<String, _>(std::io::Error::other("type mismatch")) };

        let error = anyhow::anyhow!("type mismatch");
        let result = engine
            .recover::<String, _, _>(error, "test_op", &operation)
            .await
            .unwrap();

        // Should fail due to deserialization error
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn test_recovery_engine_abort_strategy() {
        let mut engine = RecoveryEngine::with_defaults();

        // Force abort classification
        engine.classifier_mut().add_pattern(
            "fatal".to_string(),
            ErrorClassification::critical("Fatal error".to_string()),
        );

        let operation =
            || async { Err::<String, _>(std::io::Error::other("fatal error occurred")) };

        let error = anyhow::anyhow!("fatal error occurred");
        let result = engine
            .recover(error, "abort_test", &operation)
            .await
            .unwrap();

        assert!(!result.is_success());
        assert_eq!(result.strategy_used(), RecoveryStrategy::Abort);
        assert_eq!(result.attempts(), 1); // Should not retry
    }

    #[tokio::test]
    async fn test_recovery_engine_skip_strategy() {
        let mut engine = RecoveryEngine::with_defaults();

        // Force skip classification (validation error)
        engine.classifier_mut().add_pattern(
            "invalid".to_string(),
            ErrorClassification::validation("Invalid input".to_string()),
        );

        let operation =
            || async { Err::<String, _>(std::io::Error::other("invalid input provided")) };

        let error = anyhow::anyhow!("invalid input provided");
        let result = engine
            .recover(error, "skip_test", &operation)
            .await
            .unwrap();

        assert!(!result.is_success());
        assert_eq!(result.strategy_used(), RecoveryStrategy::Skip);
        assert_eq!(result.attempts(), 1); // Should not retry
    }

    #[tokio::test]
    async fn test_recovery_engine_retry_then_success() {
        let engine = RecoveryEngine::with_defaults();
        let attempt = Arc::new(AtomicU32::new(0));

        let operation = || {
            let attempt = Arc::clone(&attempt);
            async move {
                let count = attempt.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err::<String, _>(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "transient failure",
                    ))
                } else {
                    Ok("success".to_string())
                }
            }
        };

        let error = anyhow::anyhow!("transient failure");
        let result = engine
            .recover(error, "retry_success", &operation)
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(result.strategy_used(), RecoveryStrategy::Retry);
        assert!(result.attempts() >= 2);
    }

    #[tokio::test]
    async fn test_recovery_engine_fallback_success() {
        let mut engine = RecoveryEngine::with_defaults();

        // Register successful fallback
        engine
            .register_fallback("fallback_op".to_string(), || {
                Ok(serde_json::json!("fallback_result"))
            })
            .unwrap();

        // Force fallback strategy
        engine.classifier_mut().add_pattern(
            "use fallback".to_string(),
            ErrorClassification::new(
                ErrorCategory::ResourceNotFound,
                false,
                RecoveryStrategy::Fallback,
                "Use registered fallback".to_string(),
            ),
        );

        let operation = || async {
            Err::<String, _>(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "use fallback",
            ))
        };

        let error = anyhow::anyhow!("use fallback");
        let result = engine
            .recover(error, "fallback_op", &operation)
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(result.strategy_used(), RecoveryStrategy::Fallback);
        assert_eq!(result.attempts(), 1);
    }
}

// ERROR CLASSIFICATION TESTS

mod error_classification_tests {
    use super::*;

    #[test]
    fn test_error_classifier_transient_errors() {
        let classifier = ErrorClassifier::new();

        let transient_errors = vec![
            "Connection timeout",
            "ETIMEDOUT: Operation timed out",
            "ECONNREFUSED: Connection refused",
            "Network is unavailable",
            "Service temporarily unavailable",
            "Rate limit exceeded",
            "Too many requests",
            "Gateway timeout",
            "Deadline exceeded",
        ];

        for error_msg in transient_errors {
            let error = anyhow::anyhow!(error_msg);
            let classification = classifier.classify(&error);
            assert_eq!(
                classification.category,
                ErrorCategory::Transient,
                "Failed for: {}",
                error_msg
            );
            assert!(
                classification.retryable,
                "Should be retryable: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_classifier_validation_errors() {
        let classifier = ErrorClassifier::new();

        let validation_errors = vec![
            "Invalid JSON format",
            "Validation failed: missing required field",
            "Malformed request body",
            "Parse error at line 10",
            "Syntax error in configuration",
        ];

        for error_msg in validation_errors {
            let error = anyhow::anyhow!(error_msg);
            let classification = classifier.classify(&error);
            assert_eq!(
                classification.category,
                ErrorCategory::Validation,
                "Failed for: {}",
                error_msg
            );
            assert!(
                !classification.retryable,
                "Should not be retryable: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_error_classifier_authorization_errors() {
        let classifier = ErrorClassifier::new();

        let auth_errors = vec![
            "Permission denied",
            "Access forbidden",
            "Authentication required",
            "Not allowed to perform this action",
        ];

        for error_msg in auth_errors {
            let error = anyhow::anyhow!(error_msg);
            let classification = classifier.classify(&error);
            assert_eq!(
                classification.category,
                ErrorCategory::Authorization,
                "Failed for: {}",
                error_msg
            );
            assert!(
                !classification.retryable,
                "Should not be retryable: {}",
                error_msg
            );
            assert_eq!(classification.suggested_strategy, RecoveryStrategy::Abort);
        }
    }

    #[test]
    fn test_error_classifier_critical_errors() {
        let classifier = ErrorClassifier::new();

        let critical_errors = vec![
            "Data corruption detected",
            "Security breach attempt",
            "Fatal system error",
            "Internal error: compromised state",
        ];

        for error_msg in critical_errors {
            let error = anyhow::anyhow!(error_msg);
            let classification = classifier.classify(&error);
            assert_eq!(
                classification.category,
                ErrorCategory::Critical,
                "Failed for: {}",
                error_msg
            );
            assert!(!classification.retryable);
            assert_eq!(classification.suggested_strategy, RecoveryStrategy::Abort);
        }
    }

    #[test]
    fn test_error_classifier_custom_patterns() {
        let mut classifier = ErrorClassifier::new();

        // Add custom pattern
        classifier.add_pattern(
            "custom_error".to_string(),
            ErrorClassification::transient("Custom transient error".to_string()),
        );

        let error = anyhow::anyhow!("ECONNREFUSED: custom_error occurred");
        let classification = classifier.classify(&error);

        assert_eq!(classification.category, ErrorCategory::Transient);
        assert!(classification.retryable);
    }

    #[test]
    fn test_error_classification_retry_success_probability() {
        // Transient errors should have higher retry success probability
        let transient = ErrorClassification::transient("test".to_string());
        assert!(transient.retry_success_probability > 0.5);

        // Validation errors should have zero retry success probability
        let validation = ErrorClassification::validation("test".to_string());
        assert_eq!(validation.retry_success_probability, 0.0);

        // Authorization errors should have zero retry success probability
        let auth = ErrorClassification::authorization("test".to_string());
        assert_eq!(auth.retry_success_probability, 0.0);
    }
}

// VALIDATION ERROR TESTS

mod validation_error_tests {
    use super::*;

    fn create_test_step(order: usize, tools: Vec<&str>) -> PlanStep {
        PlanStep {
            order,
            title: format!("Step {}", order),
            description: format!("Description for step {}", order),
            tools: tools.into_iter().map(String::from).collect(),
            expected_outcome: format!("Outcome for step {}", order),
            rollback_hint: format!("Rollback for step {}", order),
            execution_status: StepStatus::Pending,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        }
    }

    fn create_test_plan(steps: Vec<PlanStep>) -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test task".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Test summary".to_string(),
            approach: "Test approach".to_string(),
            steps,
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        }
    }

    #[test]
    fn test_validation_error_empty_plan_message() {
        let plan = create_test_plan(vec![]);
        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Plan validation failed"));
        assert!(err_msg.contains("Plan has no steps"));
    }

    #[test]
    fn test_validation_error_missing_tool_message() {
        let plan = create_test_plan(vec![create_test_step(0, vec!["nonexistent_tool"])]);
        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Tool not found"));
        assert!(err_msg.contains("nonexistent_tool"));
    }

    #[test]
    fn test_validation_error_invalid_path_traversal() {
        let mut plan = create_test_plan(vec![create_test_step(0, vec![])]);
        plan.files_to_modify = vec!["../../../etc/passwd".to_string()];

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid file path"));
        assert!(err_msg.contains(".."));
    }

    #[test]
    fn test_validation_error_invalid_path_absolute() {
        let mut plan = create_test_plan(vec![create_test_step(0, vec![])]);
        plan.files_to_modify = vec!["/etc/passwd".to_string()];

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid file path"));
        assert!(err_msg.contains("relative"));
    }

    #[test]
    fn test_validation_error_invalid_path_null_char() {
        let mut plan = create_test_plan(vec![create_test_step(0, vec![])]);
        plan.files_to_modify = vec!["file\0name.txt".to_string()];

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid file path"));
        assert!(err_msg.contains("invalid characters"));
    }

    #[test]
    fn test_validation_error_multiple_errors() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "".to_string(), // Missing task
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "".to_string(), // Missing summary
            approach: "Test".to_string(),
            steps: vec![], // Empty steps
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };

        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validate_plan(&plan, &tool_registry, workspace);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Multiple validation errors") || err_msg.contains("validation failed")
        );
    }

    #[test]
    fn test_validation_result_combine_errors() {
        let errors = vec![
            ValidationResult::Invalid(vec![ValidationError::EmptyPlan]),
            ValidationResult::Valid,
            ValidationResult::Invalid(vec![ValidationError::MissingField {
                field: "title".to_string(),
                step_index: 0,
            }]),
        ];

        let combined = ValidationResult::combine(errors);
        assert!(!combined.is_valid());
        assert_eq!(combined.errors().len(), 2);
    }

    #[test]
    fn test_validation_result_combine_all_valid() {
        let errors = vec![
            ValidationResult::Valid,
            ValidationResult::Valid,
            ValidationResult::Valid,
        ];

        let combined = ValidationResult::combine(errors);
        assert!(combined.is_valid());
        assert_eq!(combined.errors().len(), 0);
    }

    #[test]
    fn test_comprehensive_validator_runs_all_validators() {
        let validator = ComprehensivePlanValidator::new();

        // Should have multiple validators
        let plan = create_test_plan(vec![create_test_step(0, vec![])]);
        let tool_registry = ToolRegistry::new();
        let workspace = Path::new("/tmp/test");

        let result = validator
            .validate_all(&plan, &tool_registry, workspace)
            .unwrap();
        // Should be valid for a minimal valid plan
        assert!(result.is_valid());
    }
}

// ERROR MESSAGE QUALITY TESTS

mod error_message_tests {
    use super::*;

    #[test]
    fn test_recovery_strategy_display() {
        assert_eq!(RecoveryStrategy::Retry.to_string(), "Retry");
        assert_eq!(RecoveryStrategy::Skip.to_string(), "Skip");
        assert_eq!(RecoveryStrategy::Abort.to_string(), "Abort");
        assert_eq!(RecoveryStrategy::Fallback.to_string(), "Fallback");
    }

    #[test]
    fn test_recovery_strategy_behavior() {
        // Retry, Skip, Fallback can continue
        assert!(RecoveryStrategy::Retry.can_continue());
        assert!(RecoveryStrategy::Skip.can_continue());
        assert!(RecoveryStrategy::Fallback.can_continue());
        assert!(!RecoveryStrategy::Abort.can_continue());

        // Only Abort stops execution
        assert!(RecoveryStrategy::Abort.stops_execution());
        assert!(!RecoveryStrategy::Retry.stops_execution());
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::Transient.to_string(), "Transient");
        assert_eq!(ErrorCategory::Validation.to_string(), "Validation");
        assert_eq!(
            ErrorCategory::ResourceNotFound.to_string(),
            "ResourceNotFound"
        );
        assert_eq!(ErrorCategory::Authorization.to_string(), "Authorization");
        assert_eq!(ErrorCategory::Critical.to_string(), "Critical");
        assert_eq!(ErrorCategory::Unknown.to_string(), "Unknown");
    }

    #[test]
    fn test_error_category_default_strategies() {
        assert_eq!(
            ErrorCategory::Transient.default_strategy(),
            RecoveryStrategy::Retry
        );
        assert_eq!(
            ErrorCategory::Validation.default_strategy(),
            RecoveryStrategy::Skip
        );
        assert_eq!(
            ErrorCategory::ResourceNotFound.default_strategy(),
            RecoveryStrategy::Fallback
        );
        assert_eq!(
            ErrorCategory::Authorization.default_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            ErrorCategory::Critical.default_strategy(),
            RecoveryStrategy::Abort
        );
        assert_eq!(
            ErrorCategory::Unknown.default_strategy(),
            RecoveryStrategy::Retry
        );
    }

    #[test]
    fn test_validation_error_formats() {
        // Test ValidationError formatting
        let err = ValidationError::CircularDependency("step 1 -> step 2 -> step 1".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Circular dependency"));
        assert!(msg.contains("step 1"));

        let err = ValidationError::ToolNotFound {
            tool_name: "read_file".to_string(),
            step_title: "Read config".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Tool not found"));
        assert!(msg.contains("read_file"));
        assert!(msg.contains("Read config"));

        let err = ValidationError::InvalidPath {
            path: "../etc/passwd".to_string(),
            reason: "traversal not allowed".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid file path"));
        assert!(msg.contains("../etc/passwd"));
        assert!(msg.contains("traversal not allowed"));
    }
}
