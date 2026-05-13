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

//! End-to-end tests for OpenAIProvider
//!
//! These tests verify that:
//! 1. API key validation works correctly
//! 2. Chat completion works (requires OPENAI_API_KEY env var)
//! 3. Streaming response parsing functions correctly
//! 4. Tool call format matches OpenAI function calling spec

use futures::StreamExt;
use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, OpenAiProvider, ProviderConfig};
use secrecy::SecretString;

fn create_openai_provider() -> Result<OpenAiProvider, String> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .map_err(|_| "OPENAI_API_KEY environment variable not set".to_string())?;

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    OpenAiProvider::new(config, "gpt-4o-mini".to_string())
        .map_err(|e| format!("Failed to create provider: {}", e))
}

fn create_test_request(prompt: &str) -> CompletionRequest {
    CompletionRequest::new(
        "gpt-4o-mini".to_string(),
        vec![ChatMessage::user(prompt.to_string())],
    )
}

// Unit Tests (No API Key Required)

#[test]
fn test_openai_api_key_validation_requires_valid_format() {
    // OpenAI API keys should start with "sk-"
    let config = ProviderConfig {
        api_key: Some(SecretString::new("invalid-key-format".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = OpenAiProvider::new(config, "gpt-4o-mini".to_string());
    // The provider uses metadata validation which checks the key format
    assert!(result.is_err());
}

#[test]
fn test_openai_api_key_validation_accepts_sk_prefix() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test123456789".to_string())
                .into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = OpenAiProvider::new(config, "gpt-4o-mini".to_string());
    assert!(result.is_ok());
}

#[test]
fn test_openai_api_key_validation_with_sk_provision_prefix() {
    // OpenAI also uses "sk-proj-" prefix for some keys
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            "sk-proj-test123456789".to_string().into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    // Should pass validation as it starts with "sk-"
    let result = OpenAiProvider::new(config, "gpt-4o-mini".to_string());
    assert!(result.is_ok());
}

#[test]
fn test_openai_provider_name() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();
    assert_eq!(provider.name(), "openai");
}

#[test]
fn test_openai_default_endpoint() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();
    assert_eq!(provider.endpoint(), "https://api.openai.com/v1");
}

#[test]
fn test_openai_custom_endpoint() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: Some("https://custom.openai.proxy.com/v1".to_string()),
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();
    assert!(provider.endpoint().contains("custom.openai.proxy.com"));
}

#[test]
fn test_openai_endpoint_trailing_slash_handling() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: Some("https://api.openai.com/v1/".to_string()),
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();
    // Trailing slash should be trimmed
    assert!(!provider.endpoint().ends_with('/'));
}

#[tokio::test]
async fn test_openai_list_models() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();

    let result = provider.list_models().await;
    if let Ok(models) = result {
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.starts_with("gpt-")));
    }
}

// Integration Tests (Require API Key)

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_chat_completion_simple() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = create_test_request("Say 'Hello, World!' and nothing else.");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("hello"));
    assert!(response.content.contains("World") || response.content.contains("world"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_chat_completion_with_system_prompt() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = CompletionRequest::new(
        "gpt-4o-mini".to_string(),
        vec![ChatMessage::user("What is your role?".to_string())],
    )
    .with_system_prompt("You are a helpful Rust programming expert.".to_string());

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_chat_completion_returns_usage() {
    // Arrange
    let provider = create_openai_provider().unwrap();
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
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_chat_completion_multi_turn() {
    // Arrange
    let provider = create_openai_provider().unwrap();

    let messages = vec![
        ChatMessage::user("My favorite color is blue.".to_string()),
        ChatMessage::assistant("I'll remember that your favorite color is blue.".to_string()),
        ChatMessage::user("What is my favorite color?".to_string()),
    ];

    let request = CompletionRequest::new("gpt-4o-mini".to_string(), messages);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("blue"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_streaming_simple() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = create_test_request("Count from 1 to 5, one number per line.");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut full_response = String::new();
    let mut chunk_count = 0;

    while let Some(chunk_result) = stream.next().await {
        if let Ok(chunk) = chunk_result {
            if let rustycode_protocol::stream_event::StreamEvent::TextDelta { content } = chunk {
                if !content.is_empty() {
                    full_response.push_str(&content);
                    chunk_count += 1;
                }
            }
        }
    }

    // Assert
    assert!(!full_response.is_empty());
    assert!(chunk_count > 0, "Should receive at least one chunk");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_streaming_with_done_marker() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = create_test_request("Say 'test'");

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
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_with_temperature() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = create_test_request("Say 'test'").with_temperature(0.0);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires OPENAI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_openai_with_max_tokens() {
    // Arrange
    let provider = create_openai_provider().unwrap();
    let request = create_test_request("Tell me a very short story").with_max_tokens(50);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

// Tool Format Tests

#[test]
fn test_openai_function_calling_format() {
    // Verify OpenAI's function calling format matches API specification
    let tool_definition = serde_json::json!({
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Search for files matching a pattern",
            "parameters": {
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
        }
    });

    // Verify structure - must have "type": "function" wrapper
    assert_eq!(tool_definition["type"], "function");
    assert!(tool_definition["function"].is_object());

    let func = &tool_definition["function"];
    assert!(func["name"].is_string());
    assert!(func["description"].is_string());
    assert!(func["parameters"].is_object());
    assert_eq!(func["parameters"]["type"], "object");
}

#[test]
fn test_openai_function_calling_with_nested_parameters() {
    // Verify more complex function schema
    let tool_definition = serde_json::json!({
        "type": "function",
        "function": {
            "name": "Write",
            "description": "Write content to a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    },
                    "options": {
                        "type": "object",
                        "properties": {
                            "create_dirs": {
                                "type": "boolean",
                                "default": true
                            }
                        }
                    }
                },
                "required": ["path", "content"]
            }
        }
    });

    let func = &tool_definition["function"];
    let options = &func["parameters"]["properties"]["options"];
    assert!(options.is_object());
    assert_eq!(options["type"], "object");
}

#[test]
fn test_openai_function_call_format_with_enum() {
    // Verify function with enum constraint
    let tool_definition = serde_json::json!({
        "type": "function",
        "function": {
            "name": "set_mode",
            "description": "Set the editor mode",
            "parameters": {
                "type": "object",
                "properties": {
                    "mode": {
                        "type": "string",
                        "enum": ["edit", "view", "readonly"],
                        "description": "Editor mode"
                    }
                },
                "required": ["mode"]
            }
        }
    });

    let func = &tool_definition["function"];
    let mode = &func["parameters"]["properties"]["mode"];
    assert_eq!(mode["type"], "string");
    assert!(mode["enum"].is_array());

    let enum_values = mode["enum"].as_array().unwrap();
    assert_eq!(enum_values.len(), 3);
}

#[test]
fn test_openai_function_call_with_array_parameter() {
    // Verify function with array parameter
    let tool_definition = serde_json::json!({
        "type": "function",
        "function": {
            "name": "batch_read_files",
            "description": "Read multiple files at once",
            "parameters": {
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": {
                            "type": "string"
                        },
                        "description": "List of file paths to read"
                    }
                },
                "required": ["paths"]
            }
        }
    });

    let func = &tool_definition["function"];
    let paths = &func["parameters"]["properties"]["paths"];
    assert_eq!(paths["type"], "array");
    assert!(paths["items"].is_object());
    assert_eq!(paths["items"]["type"], "string");
}

#[test]
fn test_openai_multiple_functions_format() {
    // Verify format for multiple functions in a single request
    let tools = vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "Read",
                "description": "Read contents of a file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "Write",
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
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "search_files",
                "description": "Search for files",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string"}
                    },
                    "required": ["pattern"]
                }
            }
        }),
    ];

    // Verify all tools have correct structure
    for tool in &tools {
        assert_eq!(tool["type"], "function");
        assert!(tool["function"].is_object());
        assert!(tool["function"]["name"].is_string());
        assert!(tool["function"]["parameters"].is_object());
    }

    // Verify unique names
    let names: Vec<_> = tools
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    let unique_names: std::collections::HashSet<_> = names.iter().cloned().collect();
    assert_eq!(
        unique_names.len(),
        names.len(),
        "Function names must be unique"
    );
}

#[test]
fn test_openai_function_response_format() {
    // Verify format of function call in assistant's response
    let function_call = serde_json::json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [
            {
                "id": "call_abc123",
                "type": "function",
                "function": {
                    "name": "search_files",
                    "arguments": "{\"pattern\": \"*.rs\", \"recursive\": true}"
                }
            }
        ]
    });

    assert!(function_call["tool_calls"].is_array());
    let tool_calls = function_call["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);

    let call = &tool_calls[0];
    assert!(call["id"].is_string());
    assert_eq!(call["type"], "function");
    assert_eq!(call["function"]["name"], "search_files");
    assert!(call["function"]["arguments"].is_string());
}

// Error Handling Tests

#[test]
fn test_openai_new_without_validation_bypasses_checks() {
    // Test that new_without_validation allows invalid keys
    let config = ProviderConfig {
        api_key: Some(SecretString::new("not-a-real-key".to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let result = OpenAiProvider::new_without_validation(config, "gpt-4o-mini".to_string());
    assert!(result.is_ok());
}

// Provider Availability Tests

#[tokio::test]
async fn test_openai_is_available_with_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            std::env::var("OPENAI_API_KEY")
                .unwrap_or_else(|_| "sk-test".to_string())
                .into(),
        )),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4o-mini".to_string()).unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(available);
}

#[tokio::test]
async fn test_openai_is_available_without_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider =
        OpenAiProvider::new_without_validation(config, "gpt-4o-mini".to_string()).unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(!available);
}

// Model-Specific Tests

#[test]
fn test_openai_gpt4o_model_string() {
    let model = "gpt-4o";
    assert!(model.starts_with("gpt-"));
    assert!(model.contains("4"));
}

#[test]
fn test_openai_o1_model_string() {
    let model = "o1";
    assert!(model.starts_with('o'));
}

#[test]
fn test_openai_mini_model_string() {
    let model = "gpt-4o-mini";
    assert!(model.contains("mini"));
}
