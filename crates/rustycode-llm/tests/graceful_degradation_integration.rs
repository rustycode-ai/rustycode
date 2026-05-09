#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_raw_string_hashes,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for graceful degradation workflows
//!
//! Tests for error classification, fallback behavior, and degraded workflows.

use rustycode_llm::{
    DegradationErrorKind, DegradationHandler, DegradationRetryConfig, ErrorClassifier,
    ErrorSeverity, LocalCodeAnalyzer, LocalSearchEngine, OfflineMode, OfflineModeConfig,
    PartialResult, StaticToolDescriptions,
};

// ─── Error Classification Tests ──────────────────────────────────────────

#[test]
fn test_classify_various_errors() {
    let test_cases = vec![
        ("HTTP 429: rate limited", DegradationErrorKind::RateLimit),
        ("HTTP 401: unauthorized", DegradationErrorKind::AuthError),
        ("Connection timeout", DegradationErrorKind::Timeout),
        ("Network unreachable", DegradationErrorKind::NetworkError),
        (
            "Context limit exceeded",
            DegradationErrorKind::ContextTooLong,
        ),
        ("Model not found", DegradationErrorKind::ModelUnavailable),
        (
            "HTTP 400: bad request",
            DegradationErrorKind::InvalidRequest,
        ),
    ];

    for (error_msg, expected) in test_cases {
        let classified = ErrorClassifier::classify(error_msg);
        assert_eq!(classified, expected, "Failed for: {}", error_msg);
    }
}

#[test]
fn test_error_severity_classification() {
    // Transient errors
    assert_eq!(
        DegradationErrorKind::RateLimit.severity(),
        ErrorSeverity::Transient
    );
    assert_eq!(
        DegradationErrorKind::Timeout.severity(),
        ErrorSeverity::Transient
    );

    // Permanent errors
    assert_eq!(
        DegradationErrorKind::AuthError.severity(),
        ErrorSeverity::Permanent
    );
    assert_eq!(
        DegradationErrorKind::InvalidRequest.severity(),
        ErrorSeverity::Permanent
    );

    // Network errors
    assert_eq!(
        DegradationErrorKind::NetworkError.severity(),
        ErrorSeverity::Network
    );
}

#[test]
fn test_retry_eligibility() {
    // Retryable errors
    assert!(DegradationErrorKind::RateLimit.is_retryable());
    assert!(DegradationErrorKind::Timeout.is_retryable());
    assert!(DegradationErrorKind::NetworkError.is_retryable());

    // Non-retryable errors
    assert!(!DegradationErrorKind::AuthError.is_retryable());
    assert!(!DegradationErrorKind::InvalidRequest.is_retryable());
    assert!(!DegradationErrorKind::ContextTooLong.is_retryable());
}

// ─── Degradation Handler Tests ──────────────────────────────────────────────

#[test]
fn test_handler_retry_strategy() {
    let handler = DegradationHandler::new();

    // Test exponential backoff
    let config = handler.retry_config();
    assert_eq!(config.max_attempts, 3);
    assert_eq!(config.base_delay_ms, 2_000);

    // Verify backoff progression
    let delay0 = config.delay_for_attempt(0);
    let delay1 = config.delay_for_attempt(1);
    let delay2 = config.delay_for_attempt(2);

    assert!(delay1 > delay0);
    assert!(delay2 > delay1);
}

#[test]
fn test_handler_creates_degradation_metadata() {
    let handler = DegradationHandler::new();
    let metadata = handler.create_degradation(DegradationErrorKind::NetworkError);

    assert!(metadata.is_degraded);
    assert_eq!(
        metadata.error_kind,
        Some(DegradationErrorKind::NetworkError)
    );
    assert!(metadata.error_message.is_some());
    assert!(metadata.recovery_suggestion.is_some());
}

// ─── Partial Results Tests ──────────────────────────────────────────────────

#[test]
fn test_partial_result_with_data() {
    let result = PartialResult::success("cached_value".to_string());

    assert!(result.has_data());
    assert!(!result.is_degraded());
    assert_eq!(result.available, Some("cached_value".to_string()));
}

#[test]
fn test_partial_result_degraded() {
    let result = PartialResult::<String>::degraded(
        Some("partial_data".to_string()),
        DegradationErrorKind::NetworkError,
    );

    assert!(result.has_data());
    assert!(result.is_degraded());
    assert_eq!(result.available, Some("partial_data".to_string()));
}

#[test]
fn test_partial_result_with_unavailable_features() {
    let result = PartialResult::success("base_data".to_string())
        .unavailable("llm_summary", "API unavailable")
        .unavailable("SemanticSearch", "No semantic model");

    assert!(result.has_data());
    assert_eq!(result.unavailable.len(), 2);
    assert_eq!(
        result.unavailable.get("llm_summary"),
        Some(&"API unavailable".to_string())
    );
}

// ─── Offline Mode Tests ──────────────────────────────────────────────────────

#[test]
fn test_offline_mode_basic() {
    let config = OfflineModeConfig::default();
    let mut mode = OfflineMode::new(config);

    assert!(!mode.is_offline());
    mode.enable();
    assert!(mode.is_offline());
    mode.disable();
    assert!(!mode.is_offline());
}

#[test]
fn test_offline_mode_service_management() {
    let config = OfflineModeConfig::all_services();
    let mut mode = OfflineMode::new(config);
    mode.enable();

    // Initially all services should be available
    assert!(mode.service_available("code_analysis"));
    assert!(mode.service_available("message_search"));

    // Mark as unavailable
    mode.mark_unavailable("code_analysis");
    assert!(!mode.service_available("code_analysis"));
    assert!(mode.service_available("message_search"));

    // Mark as available again
    mode.mark_available("code_analysis");
    assert!(mode.service_available("code_analysis"));
}

#[test]
fn test_offline_mode_online_services_always_available() {
    let config = OfflineModeConfig::default();
    let mode = OfflineMode::new(config);

    // Even if offline mode is disabled, services should be available
    assert!(mode.service_available("code_analysis"));
    assert!(mode.service_available("message_search"));
}

// ─── Local Code Analysis Tests ──────────────────────────────────────────────

#[test]
fn test_local_code_analysis() {
    let code = r#"
        fn main() {
            println!("Hello, world!");
        }

        struct MyStruct {
            field: String,
        }
    "#;

    let result = LocalCodeAnalyzer::analyze_structure(code);

    assert!(result.line_count > 0);
    assert_eq!(result.function_count, 1);
    assert_eq!(result.struct_count, 1);
    assert!(!result.has_error_markers);
}

#[test]
fn test_syntax_validation_with_errors() {
    let code = "{ incomplete";
    let result = LocalCodeAnalyzer::validate_syntax(code);

    assert!(!result.issues.is_empty());
    assert!(result.issues[0].contains("Unmatched"));
}

#[test]
fn test_code_metadata_extraction() {
    let code = "pub fn my_function() {}\npub struct MyStruct {}";
    let meta = LocalCodeAnalyzer::extract_metadata(code, "test.rs");

    assert_eq!(meta.file_name, "test.rs");
    assert_eq!(meta.language, "Rust");
    assert!(!meta.exports.is_empty());
}

// ─── Local Search Tests ──────────────────────────────────────────────────────

#[test]
fn test_local_search_workflow() {
    let text = r#"
        This is a test document.
        It contains multiple lines.
        Some lines have the search term.
        And some lines do not.
    "#;

    // Single match search
    let results = LocalSearchEngine::search(text, "test", 10);
    assert!(!results.is_empty());
    assert!(results[0].content.contains("test"));

    // Stats check
    let stats = LocalSearchEngine::search_stats(text, "lines");
    assert!(stats.matching_lines > 0);
    assert!(stats.match_percentage > 0.0);
}

#[test]
fn test_local_search_relevance_scoring() {
    let text = "The quick brown fox\nA brown dog\nBrown bear";

    let results = LocalSearchEngine::search(text, "brown", 10);
    assert!(!results.is_empty());

    // Results should be sorted by relevance
    for i in 0..results.len() - 1 {
        assert!(results[i].relevance_score >= results[i + 1].relevance_score);
    }
}

// ─── Static Tools Tests ──────────────────────────────────────────────────────

#[test]
fn test_static_tools_available() {
    let tools = StaticToolDescriptions::all();

    assert!(tools.contains_key("code_analysis"));
    assert!(tools.contains_key("text_search"));
    assert!(tools.contains_key("file_metadata"));
    assert!(tools.contains_key("syntax_check"));

    for (_, tool) in tools {
        assert!(tool.offline);
        assert!(!tool.capabilities.is_empty());
    }
}

#[test]
fn test_static_tool_lookup() {
    let code_analysis = StaticToolDescriptions::get("code_analysis");
    assert!(code_analysis.is_some());

    let tool = code_analysis.unwrap();
    assert_eq!(tool.name, "Code Analysis");
    assert!(tool.offline);
    assert!(!tool.capabilities.is_empty());
}

// ─── Degraded Workflow Tests ────────────────────────────────────────────────

#[test]
fn test_complete_degradation_workflow() {
    // 1. Detect error
    let error_msg = "Connection timeout: failed to reach api.example.com";
    let handler = DegradationHandler::new();
    let error_kind = handler.classify_error(error_msg);

    assert_eq!(error_kind, DegradationErrorKind::Timeout);
    assert!(handler.is_retryable(&error_kind));

    // 2. Create partial result
    let partial_data = "cached_response".to_string();
    let result = PartialResult::degraded(Some(partial_data), error_kind.clone());

    assert!(result.has_data());
    assert!(result.is_degraded());

    // 3. Add unavailable features
    let result = result.unavailable("realtime_updates", "API temporarily down");

    assert_eq!(result.unavailable.len(), 1);

    // 4. Create degradation metadata
    let metadata = handler.create_degradation(error_kind);
    assert!(metadata.is_degraded);
    assert!(metadata.error_message.is_some());
}

#[test]
fn test_offline_workflow_with_local_analysis() {
    // Setup offline mode
    let config = OfflineModeConfig::all_services();
    let mut mode = OfflineMode::new(config);
    mode.enable();

    // Verify offline
    assert!(mode.is_offline());
    assert!(mode.service_available("code_analysis"));

    // Perform local analysis
    let code = "fn example() { let x = 42; }";
    let analysis = LocalCodeAnalyzer::analyze_structure(code);

    assert_eq!(analysis.function_count, 1);

    // Create result marked as offline
    let result =
        PartialResult::success(analysis).unavailable("llm_enhancement", "Offline mode active");

    assert!(!result.metadata.offline_mode); // Not marked in result itself
    assert_eq!(result.unavailable.len(), 1);
}

#[test]
fn test_error_recovery_suggestions() {
    let error_types = vec![
        DegradationErrorKind::RateLimit,
        DegradationErrorKind::AuthError,
        DegradationErrorKind::NetworkError,
        DegradationErrorKind::Timeout,
    ];

    for error_kind in error_types {
        let suggestion = error_kind.recovery_suggestion();
        assert!(!suggestion.is_empty());
        assert!(suggestion.len() < 200); // Reasonable message length
    }
}

#[test]
fn test_retry_backoff_progression() {
    let config = DegradationRetryConfig::default();

    let delays: Vec<_> = (0..5)
        .map(|attempt| config.delay_for_attempt(attempt))
        .collect();

    // Verify exponential backoff
    assert!(delays[0] < delays[1]);
    assert!(delays[1] < delays[2]);
    assert!(delays[2] < delays[3]);

    // Verify capping at max delay
    assert_eq!(delays[4], delays[3]);
}
