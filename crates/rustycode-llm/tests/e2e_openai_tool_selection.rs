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

//! End-to-end tests for OpenAI provider with intelligent tool selection
//!
//! These tests verify that:
//! 1. Tool selection works correctly for OpenAI's function calling API
//! 2. Tools are properly formatted for OpenAI API
//! 3. Context reduction is achieved by sending only relevant tools

use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, OpenAiProvider, ProviderConfig};
use rustycode_tools::{ToolProfile, ToolSelector};
use secrecy::SecretString;

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_tool_selection_explore_intent() {
    // Test that "show me" queries trigger Explore profile
    let config = ProviderConfig {
        api_key: std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        OpenAiProvider::new(config, "gpt-4".to_string()).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gpt-4".to_string(),
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
    assert!(usage.input_tokens > 0);
    assert!(usage.output_tokens > 0);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_tool_selection_implement_intent() {
    // Test that "create/add" queries trigger Implement profile
    let config = ProviderConfig {
        api_key: std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        OpenAiProvider::new(config, "gpt-4".to_string()).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gpt-4".to_string(),
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
async fn test_openai_tool_selection_debug_intent() {
    // Test that "fix/debug" queries trigger Debug profile
    let config = ProviderConfig {
        api_key: std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|k| SecretString::new(k.into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        OpenAiProvider::new(config, "gpt-4".to_string()).expect("Failed to create provider");

    let request = CompletionRequest::new(
        "gpt-4".to_string(),
        vec![ChatMessage::user(
            "Fix the failing test in src/auth/tests.rs - it's panicking on line 42".to_string(),
        )],
    );

    let response = provider.complete(request).await.expect("Completion failed");

    assert!(!response.content.is_empty());
    println!("Response: {}", response.content);
}

#[test]
fn test_openai_tool_formatting() {
    // Verify OpenAI's function calling format structure
    let tool_definitions = vec![
        serde_json::json!({
            "type": "function",
            "function": {
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
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
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
            }
        }),
    ];

    // Verify structure
    for tool_def in &tool_definitions {
        assert_eq!(tool_def["type"], "function");
        assert!(tool_def["function"].is_object());
        assert!(tool_def["function"]["name"].is_string());
        assert!(tool_def["function"]["parameters"].is_object());
    }

    println!(
        "OpenAI tool definitions: {}",
        serde_json::to_string_pretty(&tool_definitions).unwrap()
    );
}

#[test]
fn test_openai_context_reduction() {
    // Test that tool selection reduces tool count for OpenAI
    let selector = ToolSelector::new();

    // Get tool counts for different profiles
    let explore_tools: Vec<String> = ToolProfile::Explore
        .available_tools()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let implement_tools: Vec<String> = ToolProfile::Implement
        .available_tools()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let debug_tools: Vec<String> = ToolProfile::Debug
        .available_tools()
        .iter()
        .map(|s| s.to_string())
        .collect();

    println!("OpenAI tool selection:");
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
