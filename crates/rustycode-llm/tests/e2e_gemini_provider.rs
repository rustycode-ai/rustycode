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

//! End-to-end tests for GeminiProvider
//!
//! These tests verify that:
//! 1. API key validation works correctly
//! 2. Chat completion works (requires GEMINI_API_KEY env var)
//! 3. Streaming response parsing functions correctly
//! 4. Tool call format matches Gemini function declaration spec

use futures::StreamExt;
use rustycode_llm::{ChatMessage, CompletionRequest, GeminiProvider, LLMProvider, ProviderConfig};
use secrecy::SecretString;

fn create_gemini_provider() -> Result<GeminiProvider, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .or_else(|_| std::env::var("GOOGLE_API_KEY"))
        .map_err(|_| "GEMINI_API_KEY or GOOGLE_API_KEY environment variable not set".to_string())?;

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    GeminiProvider::new(config).map_err(|e| format!("Failed to create provider: {}", e))
}

fn create_test_request(prompt: &str) -> CompletionRequest {
    CompletionRequest::new(
        "gemini-2.5-flash".to_string(),
        vec![ChatMessage::user(prompt.to_string())],
    )
}

// Unit Tests (No API Key Required)

#[test]
fn test_gemini_api_key_validation_requires_key() {
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let result = GeminiProvider::new(config);
    assert!(result.is_err());
}

#[test]
fn test_gemini_api_key_validation_rejects_empty_key() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let result = GeminiProvider::new(config);
    assert!(result.is_err());
}

#[test]
fn test_gemini_api_key_validation_accepts_valid_key() {
    // Google API keys typically start with "AIza"
    let config = ProviderConfig {
        api_key: Some(SecretString::new(
            "AIzaSyTestKey123456789".to_string().into(),
        )),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let result = GeminiProvider::new(config);
    assert!(result.is_ok());
}

#[test]
fn test_gemini_provider_name() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();
    assert_eq!(provider.name(), "gemini");
}

#[tokio::test]
async fn test_gemini_list_models() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();
    let models = provider.list_models().await.unwrap();

    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.contains("gemini")));
}

#[test]
fn test_gemini_default_endpoint() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();

    // Verify the endpoint is constructed correctly
    let endpoint = provider.endpoint("gemini-2.5-flash");
    assert!(endpoint.contains("generativelanguage.googleapis.com"));
    assert!(endpoint.contains("generateContent"));
}

#[test]
fn test_gemini_stream_endpoint() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();

    let stream_endpoint = provider.stream_endpoint("gemini-2.5-flash");
    assert!(stream_endpoint.contains("streamGenerateContent"));
}

#[test]
fn test_gemini_custom_endpoint() {
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: Some("https://custom.gemini.proxy.com".to_string()),
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();
    let endpoint = provider.endpoint("gemini-2.5-flash");
    assert!(endpoint.contains("custom.gemini.proxy.com"));
}

#[test]
fn test_gemini_new_without_validation() {
    // Test that new_without_validation bypasses some checks
    let config = ProviderConfig {
        api_key: Some(SecretString::new("test-key".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let result = GeminiProvider::new_without_validation(config);
    assert!(result.is_ok());
}

// Integration Tests (Require API Key)

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_chat_completion_simple() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = create_test_request("Say 'Hello, World!' and nothing else.");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("hello"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_chat_completion_with_system_prompt() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = CompletionRequest::new(
        "gemini-2.5-flash".to_string(),
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
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_chat_completion_returns_usage() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = create_test_request("Count to 10.");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    // Gemini returns usage_metadata in some responses
    // The provider parses it into the usage field
    if let Some(usage) = &response.usage {
        assert!(usage.total_tokens > 0);
    }
    // If usage is None, that's also acceptable for some Gemini models
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_chat_completion_multi_turn() {
    // Arrange
    let provider = create_gemini_provider().unwrap();

    let messages = vec![
        ChatMessage::user("My favorite color is blue.".to_string()),
        ChatMessage::assistant("I'll remember that your favorite color is blue.".to_string()),
        ChatMessage::user("What is my favorite color?".to_string()),
    ];

    let request = CompletionRequest::new("gemini-2.5-flash".to_string(), messages);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
    assert!(response.content.to_lowercase().contains("blue"));
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_streaming_simple() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = create_test_request("Count from 1 to 5, one number per line.");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut full_response = String::new();
    let mut chunk_count = 0;

    while let Some(chunk_result) = stream.next().await {
        if let Ok(chunk) = chunk_result {
            // Extract text from StreamEvent::TextDelta variant
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
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_streaming_with_longer_response() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request =
        create_test_request("Write a haiku about programming. Just the haiku, nothing else.");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut full_response = String::new();

    while let Some(chunk_result) = stream.next().await {
        if let Ok(chunk) = chunk_result {
            // Extract text from StreamEvent::TextDelta variant
            if let rustycode_protocol::stream_event::StreamEvent::TextDelta { content } = chunk {
                full_response.push_str(&content);
            }
        }
    }

    // Assert
    assert!(!full_response.is_empty());
    let line_count = full_response.lines().count();
    assert!(line_count >= 1, "Should have at least one line");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_with_temperature() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = create_test_request("Say 'test'").with_temperature(0.0);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires GEMINI_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_gemini_with_max_tokens() {
    // Arrange
    let provider = create_gemini_provider().unwrap();
    let request = create_test_request("Tell me a very short story").with_max_tokens(50);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(!response.content.is_empty());
}

// Tool Format Tests

#[test]
fn test_gemini_function_declaration_format() {
    // Verify Gemini's function declaration format (simpler than OpenAI)
    let tool_definition = serde_json::json!({
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
    });

    // Verify structure - Gemini uses direct format (no "type" wrapper)
    assert!(tool_definition["name"].is_string());
    assert!(tool_definition["description"].is_string());
    assert!(tool_definition["parameters"].is_object());
    assert_eq!(tool_definition["parameters"]["type"], "object");
    assert!(tool_definition["parameters"]["properties"].is_object());
    assert!(tool_definition["parameters"]["required"].is_array());
}

#[test]
fn test_gemini_function_with_nested_parameters() {
    // Verify more complex function schema
    let tool_definition = serde_json::json!({
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

    let options = &tool_definition["parameters"]["properties"]["options"];
    assert!(options.is_object());
    assert_eq!(options["type"], "object");
    assert!(options["properties"].is_object());

    let mode = &options["properties"]["mode"];
    assert_eq!(mode["type"], "string");
    assert!(mode["enum"].is_array());
}

#[test]
fn test_gemini_function_with_array_parameter() {
    // Verify function with array parameter
    let tool_definition = serde_json::json!({
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
    });

    let paths = &tool_definition["parameters"]["properties"]["paths"];
    assert_eq!(paths["type"], "array");
    assert!(paths["items"].is_object());
    assert_eq!(paths["items"]["type"], "string");
}

#[test]
fn test_gemini_function_response_format() {
    // Verify format of function call in response (functionCall)
    let function_call = serde_json::json!({
        "name": "search_files",
        "args": {
            "pattern": "*.rs",
            "recursive": true
        }
    });

    assert!(function_call["name"].is_string());
    assert!(function_call["args"].is_object());
    assert_eq!(function_call["args"]["pattern"], "*.rs");
    assert_eq!(function_call["args"]["recursive"], true);
}

#[test]
fn test_gemini_function_response_parts_format() {
    // Verify format in content parts (what Gemini actually returns)
    let content = serde_json::json!({
        "parts": [
            {
                "text": "I'll search for Rust files for you."
            },
            {
                "functionCall": {
                    "name": "search_files",
                    "args": {
                        "pattern": "*.rs"
                    }
                }
            }
        ]
    });

    assert!(content["parts"].is_array());
    let parts = content["parts"].as_array().unwrap();

    // First part is text
    assert!(parts[0]["text"].is_string());

    // Second part is functionCall
    assert!(parts[1]["functionCall"].is_object());
    assert!(parts[1]["functionCall"]["name"].is_string());
    assert!(parts[1]["functionCall"]["args"].is_object());
}

#[test]
fn test_gemini_function_response_parts_format_multiple() {
    // Verify format with multiple function calls in one response
    let content = serde_json::json!({
        "parts": [
            {
                "functionCall": {
                    "name": "Read",
                    "args": {
                        "path": "Cargo.toml"
                    }
                }
            },
            {
                "functionCall": {
                    "name": "Read",
                    "args": {
                        "path": "src/main.rs"
                    }
                }
            }
        ]
    });

    let parts = content["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);

    for part in parts {
        assert!(part["functionCall"].is_object());
        assert_eq!(part["functionCall"]["name"], "Read");
    }
}

#[test]
fn test_gemini_multiple_functions_format() {
    // Verify format for multiple functions in a single request
    let tools = vec![
        serde_json::json!({
            "name": "Read",
            "description": "Read contents of a file",
            "parameters": {
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
            "parameters": {
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
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"}
                },
                "required": ["pattern"]
            }
        }),
    ];

    // Verify all tools have correct structure (no wrapper)
    for tool in &tools {
        assert!(tool["name"].is_string());
        assert!(tool["description"].is_string());
        assert!(tool["parameters"].is_object());
        assert_eq!(tool["parameters"]["type"], "object");
    }

    // Verify unique names
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    let unique_names: std::collections::HashSet<_> = names.iter().cloned().collect();
    assert_eq!(
        unique_names.len(),
        names.len(),
        "Function names must be unique"
    );
}

#[test]
fn test_gemini_vs_openai_format_difference() {
    // Demonstrate the key difference in tool format

    // Gemini format: direct name/description/parameters
    let gemini_format = serde_json::json!({
        "name": "search_files",
        "description": "Search for files",
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
            "description": "Search for files",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string"}
                }
            }
        }
    });

    // Verify Gemini format is simpler
    assert!(gemini_format.get("type").is_none());
    assert!(gemini_format.get("function").is_none());
    assert!(gemini_format["name"].is_string());

    // Verify OpenAI format has wrapper
    assert_eq!(openai_format["type"], "function");
    assert!(openai_format["function"].is_object());
}

// Error Handling Tests

#[test]
fn test_gemini_metadata_validation() {
    // Test that provider metadata validation works
    let metadata = GeminiProvider::metadata();

    assert_eq!(metadata.provider_id, "gemini");
    assert_eq!(metadata.display_name, "Google Gemini");
    assert!(!metadata.description.is_empty());
}

#[test]
fn test_gemini_config_validation_via_metadata() {
    // Test that metadata validates API key format
    let metadata = GeminiProvider::metadata();

    // Check required fields
    assert!(!metadata.config_schema.required_fields.is_empty());

    // Find api_key field
    let api_key_field = metadata
        .config_schema
        .required_fields
        .iter()
        .find(|f| f.name == "api_key")
        .expect("API key should be a required field");

    assert_eq!(
        api_key_field.field_type,
        rustycode_llm::provider_metadata::ConfigFieldType::APIKey
    );
    assert!(api_key_field.validation_pattern.is_some());
}

// Provider Availability Tests

#[tokio::test]
async fn test_gemini_is_available_with_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: Some(SecretString::new("AIzaTestKey".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new(config).unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(available);
}

#[tokio::test]
async fn test_gemini_is_available_without_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    // Act & Assert
    // Even new_without_validation requires an API key for Gemini
    // because the x-goog-api-key header is mandatory
    match GeminiProvider::new_without_validation(config) {
        Ok(_) => panic!("Expected error when creating Gemini provider without API key"),
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Google API key is required") || error_msg.contains("required"),
                "Expected 'required' in error message, got: {}",
                error_msg
            );
        }
    }
}

#[tokio::test]
async fn test_gemini_is_available_with_empty_key() {
    // Arrange
    let config = ProviderConfig {
        api_key: Some(SecretString::new("".to_string().into())),
        base_url: None,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = GeminiProvider::new_without_validation(config).unwrap();

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(!available);
}

// Model-Specific Tests

#[test]
fn test_gemini_model_names() {
    let models = vec![
        "gemini-2.5-pro",
        "gemini-2.5-flash",
        "gemini-2.0-flash",
        "gemini-1.5-pro",
        "gemini-1.5-flash",
    ];

    for model in models {
        assert!(
            model.starts_with("gemini-"),
            "Model should start with 'gemini-'"
        );
    }
}

#[test]
fn test_gemini_model_versions() {
    // Verify model version patterns
    let models = vec![
        ("gemini-2.5-pro", Some("2.5"), Some("pro")),
        ("gemini-2.5-flash", Some("2.5"), Some("flash")),
        ("gemini-1.5-flash-8b", Some("1.5"), Some("flash")),
    ];

    for (model, expected_version, expected_variant) in models {
        assert!(model.contains(expected_version.unwrap()));
        assert!(model.contains(expected_variant.unwrap()));
    }
}
