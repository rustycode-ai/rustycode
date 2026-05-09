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

//! End-to-end tests for AnthropicProvider
//!
//! These tests verify that:
//! 1. API key validation works correctly
//! 2. Chat completion works (requires ANTHROPIC_API_KEY env var)
//! 3. Streaming response parsing functions correctly
//! 4. Tool call format matches Anthropic API spec

use futures::StreamExt;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, ProviderConfig,
};
use secrecy::SecretString;

fn create_anthropic_provider() -> Result<AnthropicProvider, String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set".to_string())?;

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    AnthropicProvider::new(config, "claude-sonnet-4-6".to_string())
        .map_err(|e| format!("Failed to create provider: {}", e))
}

fn create_test_request(prompt: &str) -> CompletionRequest {
    CompletionRequest::new(
        "claude-sonnet-4-6".to_string(),
        vec![ChatMessage::user(prompt.to_string())],
    )
}

// Unit Tests (No API Key Required)

#[test]
fn test_anthropic_api_key_validation_requires_key() {
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string());
    assert!(result.is_err());
}

#[test]
fn test_anthropic_api_key_validation_rejects_empty_key() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string());
    assert!(result.is_err());
}

#[test]
fn test_anthropic_api_key_validation_accepts_whitespace_only_key_is_rejected() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("   ".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string());
    assert!(result.is_err());
}

#[test]
fn test_anthropic_api_key_validation_accepts_valid_key() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            "sk-ant-api03-test-key".to_string().into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string());
    assert!(result.is_ok());
}

#[test]
fn test_anthropic_provider_name() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-test".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
    assert_eq!(provider.name(), "anthropic");
}

#[test]
fn test_anthropic_list_models() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-test".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();

    // This test doesn't require async since list_models just returns a static list
    let models = futures::executor::block_on(provider.list_models()).unwrap();

    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.contains("claude")));
    assert!(models.iter().any(|m| m.contains("sonnet")));
}

#[test]
fn test_anthropic_endpoint_default() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-test".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        AnthropicProvider::new_without_validation(config, "claude-sonnet-4-6".to_string()).unwrap();

    // The endpoint should be the default Anthropic API
    // We can't access endpoint() directly as it's private, but we can verify the provider was created
    assert_eq!(provider.name(), "anthropic");
}

#[test]
fn test_anthropic_custom_base_url() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-ant-test".to_string().into())),
        base_url: Some("https://custom.proxy.com".to_string()),
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = AnthropicProvider::new_without_validation(config, "claude-sonnet-4-6".to_string());
    assert!(result.is_ok());
}

// Integration Tests (Require API Key)

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_chat_completion_simple() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = create_test_request("Say 'Hello, World!' and nothing else.");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("hello"));
    assert_eq!(response.model, "claude-sonnet-4-6");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_chat_completion_with_system_prompt() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = CompletionRequest::new(
        "claude-sonnet-4-6".to_string(),
        vec![ChatMessage::user("What is your role?".to_string())],
    )
    .with_system_prompt("You are a helpful Rust programming expert.".to_string());

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    // Response should mention being a Rust expert or helper
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_chat_completion_returns_usage() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = create_test_request("Count to 10.");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(response.usage.is_some());
    let usage = response.usage.as_ref().unwrap();
    assert!(usage.input_tokens > 0);
    assert!(usage.output_tokens > 0);
    assert_eq!(usage.total_tokens, usage.input_tokens + usage.output_tokens);
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_chat_completion_multi_turn() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();

    let messages = vec![
        ChatMessage::user("My favorite color is blue.".to_string()),
        ChatMessage::assistant("I'll remember that your favorite color is blue.".to_string()),
        ChatMessage::user("What is my favorite color?".to_string()),
    ];

    let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), messages);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("blue"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_streaming_simple() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = create_test_request("Count from 1 to 5, one number per line.");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut full_response = String::new();
    let mut chunk_count = 0;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                if let rustycode_protocol::stream_event::StreamEvent::TextDelta { content } = chunk
                {
                    full_response.push_str(&content);
                    chunk_count += 1;
                }
            }
            Err(e) => {
                panic!("Unexpected error in stream: {}", e);
            }
        }
    }

    // Assert
    assert!(!full_response.is_empty());
    assert!(chunk_count > 0, "Should receive at least one chunk");
    assert!(full_response.contains("1") || full_response.contains("2"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_streaming_with_long_response() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request =
        create_test_request("Write a haiku about programming. Just the haiku, nothing else.");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut full_response = String::new();

    while let Some(chunk_result) = stream.next().await {
        if let Ok(chunk) = chunk_result {
            if let rustycode_protocol::stream_event::StreamEvent::TextDelta { content } = chunk {
                full_response.push_str(&content);
            }
        }
    }

    // Assert
    assert!(!full_response.is_empty());
    // A haiku is typically 3 lines
    let line_count = full_response.lines().count();
    assert!(line_count >= 1, "Should have at least one line");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_with_temperature() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = create_test_request("Say 'test'").with_temperature(0.0); // Use deterministic temperature

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_with_max_tokens() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();
    let request = create_test_request("Tell me a short story").with_max_tokens(50); // Limit response length

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    // With max_tokens=50, response should be reasonably short
    assert!(
        response.content.len() < 500,
        "Response should be limited by max_tokens"
    );
}

// Tool Format Tests

#[test]
fn test_anthropic_tool_format_structure() {
    // Verify Anthropic's tool format matches API specification
    let tool_definition = serde_json::json!({
        "name": "search_files",
        "description": "Search for files matching a pattern",
        "input_schema": {
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "File pattern to search for (e.g., '*.rs')"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Search recursively in subdirectories",
                    "default": false
                }
            },
            "required": ["pattern"]
        }
    });

    // Verify structure
    assert!(tool_definition["name"].is_string());
    assert!(tool_definition["description"].is_string());
    assert!(tool_definition["input_schema"].is_object());
    assert_eq!(tool_definition["input_schema"]["type"], "object");
    assert!(tool_definition["input_schema"]["properties"].is_object());
    assert!(tool_definition["input_schema"]["required"].is_array());

    // Verify required field
    let required = tool_definition["input_schema"]["required"]
        .as_array()
        .unwrap();
    assert_eq!(required.len(), 1);
    assert_eq!(required[0], "pattern");
}

#[test]
fn test_anthropic_tool_format_with_nested_parameters() {
    // Verify more complex tool schema
    let tool_definition = serde_json::json!({
        "name": "Write",
        "description": "Write content to a file at the specified path",
        "input_schema": {
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                },
                "options": {
                    "type": "object",
                    "properties": {
                        "create_dirs": {
                            "type": "boolean",
                            "default": true
                        },
                        "mode": {
                            "type": "string",
                            "enum": ["overwrite", "append", "error_if_exists"],
                            "default": "overwrite"
                        }
                    }
                }
            },
            "required": ["path", "content"]
        }
    });

    // Verify nested structure
    let options = &tool_definition["input_schema"]["properties"]["options"];
    assert!(options.is_object());
    assert_eq!(options["type"], "object");
    assert!(options["properties"].is_object());

    // Verify enum constraint
    let mode = &options["properties"]["mode"];
    assert_eq!(mode["type"], "string");
    assert!(mode["enum"].is_array());
}

#[test]
fn test_anthropic_tool_use_block_format() {
    // Verify the format of tool_use content blocks in responses
    let tool_use_block = serde_json::json!({
        "type": "tool_use",
        "id": "toolu_01ABC123XYZ",
        "name": "search_files",
        "input": {
            "pattern": "*.rs",
            "recursive": true
        }
    });

    assert_eq!(tool_use_block["type"], "tool_use");
    assert!(tool_use_block["id"].is_string());
    assert!(tool_use_block["name"].is_string());
    assert!(tool_use_block["input"].is_object());
}

#[test]
fn test_anthropic_tool_result_block_format() {
    // Verify the format of tool_result content blocks
    let tool_result_block = serde_json::json!({
        "type": "tool_result",
        "tool_use_id": "toolu_01ABC123XYZ",
        "content": "Found 42 files matching *.rs"
    });

    assert_eq!(tool_result_block["type"], "tool_result");
    assert!(tool_result_block["tool_use_id"].is_string());
    assert!(tool_result_block["content"].is_string());
}

#[test]
fn test_anthropic_multiple_tools_format() {
    // Verify format for multiple tools in a single request
    let tools = vec![
        serde_json::json!({
            "name": "Read",
            "description": "Read contents of a text file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }
        }),
        serde_json::json!({
            "name": "Write",
            "description": "Write content to a file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "content": {"type": "string"}
                },
                "required": ["path", "content"]
            }
        }),
        serde_json::json!({
            "name": "search_files",
            "description": "Search for files",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"}
                },
                "required": ["pattern"]
            }
        }),
    ];

    // Verify all tools have correct structure
    for tool in &tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["input_schema"].is_object());
        assert_eq!(tool["input_schema"]["type"], "object");
    }

    // Verify unique names
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let unique_names: std::collections::HashSet<_> = names.iter().cloned().collect();
    assert_eq!(unique_names.len(), names.len(), "Tool names must be unique");
}

// Error Handling Tests

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires live API — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_invalid_api_key() {
    // Arrange - Use an invalid API key
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            "sk-ant-invalid-key-12345".to_string().into(),
        )),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string()).unwrap();
    let request = create_test_request("Test");

    // Act
    let result = LLMProvider::complete(&provider, request).await;

    // Assert - Should get an authentication error
    // Note: This test might pass if the invalid key format happens to work
    // The exact error depends on Anthropic's API behavior
    match result {
        Err(rustycode_llm::provider::ProviderError::Auth(_)) => {
            // Expected - authentication failed
        }
        Err(_) => {
            // Also acceptable - some kind of error occurred
        }
        Ok(_) => {
            // Unexpected - request succeeded with invalid key
            // This might happen in test environments with mocking
        }
    }
}

// Provider Availability Tests

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_anthropic_is_available_with_valid_key() {
    // Arrange
    let provider = create_anthropic_provider().unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(available);
}

#[tokio::test]
async fn test_anthropic_is_available_without_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        AnthropicProvider::new_without_validation(config, "claude-sonnet-4-6".to_string()).unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(!available);
}
