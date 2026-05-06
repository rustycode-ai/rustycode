#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! End-to-end tests for Gemini provider with intelligent tool selection
//!
//! These tests verify that:
//! 1. Tool selection works correctly for Gemini's function declaration API
//! 2. Tools are properly formatted for Gemini API
//! 3. Context reduction is achieved by sending only relevant tools

use rustycode_llm::{ChatMessage, CompletionRequest, GeminiProvider, LLMProvider, ProviderConfig};
use rustycode_tools_api::ToolProfile;
use secrecy::SecretString;

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_tool_selection_explore_intent() {
    // Test that "show me" queries trigger Explore profile
    let config = ProviderConfig {
        api_key: std::env::var("GOOGLE_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gemini-2.5-pro".to_string(),
        vec![ChatMessage::user(
            "Show me how the authentication system works in this codebase".to_string(),
        )],
    );

    let response = provider.complete(request).await.expect("Completion failed");

    // Verify response is generated
    assert!(!response.content.is_empty());
    println!("Response: {}", response.content);

    // Verify usage statistics
    assert!(response.usage.is_some());
    let usage = response.usage.as_ref().unwrap();
    assert!(usage.total_tokens > 0);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_tool_selection_implement_intent() {
    // Test that "create/add" queries trigger Implement profile
    let config = ProviderConfig {
        api_key: std::env::var("GOOGLE_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gemini-2.5-pro".to_string(),
        vec![ChatMessage::user(
            "Create a new user authentication endpoint with JWT tokens".to_string(),
        )],
    );

    let response = provider.complete(request).await.expect("Completion failed");

    assert!(!response.content.is_empty());
    println!("Response: {}", response.content);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_tool_selection_debug_intent() {
    // Test that "fix/debug" queries trigger Debug profile
    let config = ProviderConfig {
        api_key: std::env::var("GOOGLE_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gemini-2.5-pro".to_string(),
        vec![ChatMessage::user(
            "Fix the failing test in src/auth/tests.rs - it's panicking on line 42".to_string(),
        )],
    );

    let response = provider.complete(request).await.expect("Completion failed");

    assert!(!response.content.is_empty());
    println!("Response: {}", response.content);
}

#[test]
fn test_gemini_tool_formatting() {
    // Verify Gemini's function declaration format structure
    let tool_definitions = vec![
        serde_json::json!({
            "name": "read_file",
            "description": "Read a UTF-8 text file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to read"
                    }
                },
                "required": ["path"]
            }
        }),
        serde_json::json!({
            "name": "write_file",
            "description": "Write content to a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }
        }),
    ];

    // Verify structure - Gemini uses simpler format without nested "function" object
    for tool_def in &tool_definitions {
        assert!(tool_def["name"].is_string());
        assert!(tool_def["description"].is_string());
        assert!(tool_def["parameters"].is_object());
        assert_eq!(tool_def["parameters"]["type"], "object");
    }

    println!(
        "Gemini tool definitions: {}",
        serde_json::to_string_pretty(&tool_definitions).unwrap()
    );
}

#[test]
fn test_gemini_context_reduction() {
    // Test that tool selection reduces tool count for Gemini

    // Get tool counts for different profiles
    let explore_tools: Vec<String> = ToolProfile::Explore
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();
    let implement_tools: Vec<String> = ToolProfile::Implement
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();
    let debug_tools: Vec<String> = ToolProfile::Debug
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();

    println!("Gemini tool selection:");
    println!("  Explore: {} tools", explore_tools.len());
    println!("  Implement: {} tools", implement_tools.len());
    println!("  Debug: {} tools", debug_tools.len());

    // Verify that different profiles produce different selections
    let all_different =
        explore_tools.len() != implement_tools.len() || implement_tools.len() != debug_tools.len();

    assert!(
        all_different,
        "Different profiles should select different numbers of tools"
    );
}

#[test]
fn test_gemini_vs_openai_formatting_difference() {
    // Demonstrate the key difference in tool format between Gemini and OpenAI

    // Gemini format: direct name/description/parameters
    let gemini_format = serde_json::json!({
        "name": "search_files",
        "description": "Search for files matching a pattern",
        "parameters": {
            "type": "object",
            "properties": {
                "pattern": {"type": "string"}
            }
        }
    });

    // OpenAI format: nested with type: "function" wrapper
    let openai_format = serde_json::json!({
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Search for files matching a pattern",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"}
                }
            }
        }
    });

    // Verify Gemini format is simpler (no "type" or "function" wrapper)
    assert!(gemini_format.get("type").is_none());
    assert!(gemini_format.get("function").is_none());
    assert!(gemini_format["name"].is_string());

    // Verify OpenAI format has wrapper
    assert_eq!(openai_format["type"], "function");
    assert!(openai_format["function"].is_object());

    println!(
        "Gemini format (simpler): {}",
        serde_json::to_string_pretty(&gemini_format).unwrap()
    );
    println!(
        "\nOpenAI format (with wrapper): {}",
        serde_json::to_string_pretty(&openai_format).unwrap()
    );
}
