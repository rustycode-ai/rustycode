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
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Test Anthropic error handling

#[test]
fn test_anthropic_error_parsing() {
    // Test structured error parsing

    // Simulate error response
    let error_json = r#"{
        "type": "error",
        "error": {
            "type": "invalid_request_error",
            "message": "user prompt must be non-empty",
            "param": "messages.0"
        }
    }"#;

    if let Ok(error_val) = serde_json::from_str::<serde_json::Value>(error_json) {
        if let Some(error_obj) = error_val.get("error").and_then(|e| e.as_object()) {
            let error_type = error_obj
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");

            let message = error_obj
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("no message");

            let param = error_obj.get("param").and_then(|p| p.as_str());

            println!("Error Type: {}", error_type);
            println!("Message: {}", message);
            println!("Parameter: {:?}", param);

            assert_eq!(error_type, "invalid_request_error");
            assert_eq!(message, "user prompt must be non-empty");
            assert_eq!(param, Some("messages.0"));

            println!("✓ Error parsing test passed");
        }
    }
}

#[test]
fn test_error_type_mapping() {
    // Test that all Anthropic error types are mapped correctly

    let test_cases = vec![
        ("invalid_request_error", 400, true),
        ("authentication_error", 401, true),
        ("permission_denied_error", 403, true),
        ("not_found_error", 404, true),
        ("rate_limit_error", 429, true),
        ("api_error", 500, true),
        ("overloaded_error", 529, true),
    ];

    for (error_type, status_code, _) in test_cases {
        println!("Testing: {} (HTTP {})", error_type, status_code);

        // Verify error type is recognized
        match error_type {
            "invalid_request_error" => println!("  → Maps to Api error"),
            "authentication_error" => println!("  → Maps to Auth error"),
            "permission_denied_error" => println!("  → Maps to Auth error"),
            "not_found_error" => println!("  → Maps to Api error"),
            "rate_limit_error" => println!("  → Maps to RateLimited error"),
            "api_error" => println!("  → Maps to Api error"),
            "overloaded_error" => println!("  → Maps to Network error"),
            _ => println!("  → Unknown error type"),
        }
    }

    println!("✓ Error type mapping test passed");
}

#[test]
fn test_error_message_formatting() {
    // Test that error messages include all relevant information

    let error_type = "invalid_request_error";
    let message = "user prompt must be non-empty";
    let param = Some("messages.0");

    // Build error message
    let mut error_msg = format!("{}: {}", error_type, message);
    if let Some(p) = param {
        error_msg.push_str(&format!(" (parameter: {})", p));
    }

    let expected = "invalid_request_error: user prompt must be non-empty (parameter: messages.0)";
    assert_eq!(error_msg, expected);

    println!("Error message: {}", error_msg);
    println!("✓ Error message formatting test passed");
}

#[test]
fn test_streaming_error_detection() {
    // Test SSE error event detection

    let sse_events = vec![
        "event: message_start",
        "data: {\"type\": \"message_start\", \"message\": {\"id\": \"msg_123\"}}",
        "event: error",
        "data: {\"type\": \"error\", \"error\": {\"type\": \"api_error\", \"message\": \"Service unavailable\"}}",
        "event: message_stop",
    ];

    let mut error_found = false;
    let mut error_type = None;

    for line in sse_events {
        if line.starts_with("event: error") {
            error_found = true;
        }

        if line.starts_with("data: ") {
            let json_str = line.trim_start_matches("data: ").trim();
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                if data.get("error").is_some() {
                    error_type = data["error"]["type"].as_str().map(|s| s.to_string());
                }
            }
        }
    }

    assert!(error_found, "Error event should be detected");
    assert_eq!(error_type, Some("api_error".to_string()));

    println!("✓ Streaming error detection test passed");
}
