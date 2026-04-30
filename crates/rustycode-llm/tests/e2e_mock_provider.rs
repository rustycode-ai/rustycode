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
    clippy::needless_collect,
    clippy::needless_raw_string_hashes,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! End-to-end tests for MockProvider
//!
//! These tests verify that:
//! 1. Mock provider responds with configured responses
//! 2. Streaming mode works correctly
//! 3. Error handling functions as expected
//! 4. Tool call generation works (when configured)

use futures::StreamExt;
use rustycode_llm::{
    mock::MockProvider,
    provider::{ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, ProviderError},
};

fn clear_mock_env() {
    std::env::remove_var("RUSTYCODE_MOCK_RESPONSE");
    std::env::remove_var("RUSTYCODE_MOCK_MODEL");
    std::env::remove_var("RUSTYCODE_MOCK_ERROR_MESSAGE");
}

fn create_test_request(prompt: &str) -> CompletionRequest {
    CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user(prompt.to_string())],
    )
}

#[tokio::test]
async fn test_mock_chat_completion_with_single_response() {
    // Arrange
    let expected_response = "Hello, this is a mock response!";
    let provider = MockProvider::from_text(expected_response);
    let request = create_test_request("Hello");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert_eq!(response.content, expected_response);
    assert_eq!(response.model, "mock");
    assert!(response.usage.is_none());
    assert!(response.stop_reason.is_none());
}

#[tokio::test]
async fn test_mock_chat_completion_with_multiple_responses() {
    // Arrange
    let responses = vec![
        Ok(CompletionResponse {
            content: "First response".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
        Ok(CompletionResponse {
            content: "Second response".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
        Ok(CompletionResponse {
            content: "Third response".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
    ];
    let provider = MockProvider::new(responses, None);

    // Act - First call
    let request = create_test_request("Test 1");
    let response1 = LLMProvider::complete(&provider, request).await.unwrap();
    assert_eq!(response1.content, "First response");

    // Act - Second call
    let request = create_test_request("Test 2");
    let response2 = LLMProvider::complete(&provider, request).await.unwrap();
    assert_eq!(response2.content, "Second response");

    // Act - Third call
    let request = create_test_request("Test 3");
    let response3 = LLMProvider::complete(&provider, request).await.unwrap();
    assert_eq!(response3.content, "Third response");
}

#[tokio::test]
async fn test_mock_streaming_mode() {
    // Arrange
    let expected_chunks = ["Hello", " ", "world", "!"];
    let chunk_results: Vec<Result<String, String>> =
        expected_chunks.iter().map(|&s| Ok(s.to_string())).collect();

    let responses = chunk_results
        .into_iter()
        .map(|r| match r {
            Ok(text) => Ok(CompletionResponse {
                content: text,
                model: "mock".to_string(),
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            }),
            Err(e) => Err(ProviderError::Unknown(e)),
        })
        .collect();

    let provider = MockProvider::new(responses, None);
    let request = create_test_request("Stream test");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut collected_chunks = Vec::new();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.unwrap();
        if let rustycode_protocol::stream_event::StreamEvent::TextDelta { content } = chunk {
            if !content.is_empty() {
                collected_chunks.push(content);
            }
        }
    }

    // Assert
    assert_eq!(collected_chunks.len(), 4);
    assert_eq!(collected_chunks.join(""), "Hello world!");
}

#[tokio::test]
async fn test_mock_error_handling() {
    // Arrange
    let error_message = "Simulated API error";
    let responses = vec![Err(ProviderError::Unknown(error_message.to_string()))];
    let provider = MockProvider::new(responses, None);
    let request = create_test_request("Error test");

    // Act
    let result = LLMProvider::complete(&provider, request).await;

    // Assert
    assert!(result.is_err());
    match result {
        Err(ProviderError::Unknown(msg)) => {
            assert_eq!(msg, error_message);
        }
        _ => panic!("Expected Unknown error"),
    }
}

#[tokio::test]
async fn test_mock_streaming_with_error() {
    // Arrange
    let error_message = "Stream error";
    let chunk_results: Vec<Result<String, String>> = vec![
        Ok("Before error".to_string()),
        Err(error_message.to_string()),
        Ok("After error".to_string()),
    ];

    let responses = chunk_results
        .into_iter()
        .map(|r| match r {
            Ok(text) => Ok(CompletionResponse {
                content: text,
                model: "mock".to_string(),
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            }),
            Err(e) => Err(ProviderError::Unknown(e)),
        })
        .collect();

    let provider = MockProvider::new(responses, None);
    let request = create_test_request("Stream error test");

    // Act
    let mut stream = LLMProvider::complete_stream(&provider, request)
        .await
        .unwrap();

    let mut results = Vec::new();
    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    // Assert
    assert_eq!(results.len(), 3);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_ok());
}

#[tokio::test]
async fn test_mock_provider_is_available() {
    // Arrange
    let provider = MockProvider::from_text("test");

    // Act
    let available = LLMProvider::is_available(&provider).await;

    // Assert
    assert!(available);
}

#[tokio::test]
async fn test_mock_provider_list_models() {
    // Arrange
    let provider = MockProvider::from_text("test");

    // Act
    let models = LLMProvider::list_models(&provider).await.unwrap();

    // Assert
    assert_eq!(models.len(), 1);
    assert_eq!(models[0], "mock");
}

#[tokio::test]
async fn test_mock_provider_name() {
    // Arrange
    let provider = MockProvider::from_text("test");

    // Act
    let name = LLMProvider::name(&provider);

    // Assert
    assert_eq!(name, "mock");
}

#[tokio::test]
async fn test_mock_repeats_last_response_on_exhaustion() {
    // Arrange
    let response = CompletionResponse {
        content: "Repeated response".to_string(),
        model: "mock".to_string(),
        usage: None,
        stop_reason: Some("end_turn".to_string()),
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    };
    let provider = MockProvider::new(vec![Ok(response.clone())], None);

    // Act - Call multiple times
    let request1 = create_test_request("Call 1");
    let result1 = LLMProvider::complete(&provider, request1).await.unwrap();

    let request2 = create_test_request("Call 2");
    let result2 = LLMProvider::complete(&provider, request2).await.unwrap();

    // Assert - Both should return the same response
    assert_eq!(result1.content, "Repeated response");
    assert_eq!(result2.content, "Repeated response");
}

#[tokio::test]
async fn test_mock_empty_response_queue() {
    // Arrange - Provider with no responses
    let provider = MockProvider::new(vec![], None);
    let request = create_test_request("Empty test");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert - Should return empty response
    assert_eq!(response.content, "");
    assert_eq!(response.model, "mock");
}

#[tokio::test]
async fn test_mock_with_environment_variables() {
    clear_mock_env();
    std::env::set_var("RUSTYCODE_MOCK_RESPONSE", "Response from env");
    std::env::set_var("RUSTYCODE_MOCK_MODEL", "env-model");

    let config = rustycode_llm::provider::ProviderConfig::default();
    let provider = MockProvider::from_env(config);
    let request = create_test_request("Env test");

    let response = LLMProvider::complete(&provider, request).await.unwrap();

    assert_eq!(response.content, "Response from env");
    assert_eq!(response.model, "env-model");

    std::env::remove_var("RUSTYCODE_MOCK_RESPONSE");
    std::env::remove_var("RUSTYCODE_MOCK_MODEL");
}

#[tokio::test]
async fn test_mock_with_error_before_stream() {
    // Use MockProvider::new() directly instead of env vars to avoid
    // race conditions with parallel tests sharing environment variables.
    let provider = MockProvider::new(
        vec![Err(ProviderError::Unknown(
            "Error before stream".to_string(),
        ))],
        None,
    );
    let request = create_test_request("Error test");

    // Act
    let result = LLMProvider::complete(&provider, request).await;

    // Assert
    assert!(result.is_err());
    match result {
        Err(ProviderError::Unknown(msg)) => {
            assert_eq!(msg, "Error before stream");
        }
        _ => panic!("Expected Unknown error"),
    }
}

#[tokio::test]
async fn test_mock_tool_call_response() {
    // Arrange - Simulate a tool call response
    let tool_call_json = r#"[
        {
            "name": "search_files",
            "arguments": {"pattern": "*.rs"}
        }
    ]"#;

    let response_content = format!("```tool\n{}\n```", tool_call_json);
    let provider = MockProvider::from_text(response_content);
    let request = create_test_request("Search for Rust files");

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert!(response.content.contains("```tool"));
    assert!(response.content.contains("search_files"));
    assert!(response.content.contains("pattern"));
}

#[tokio::test]
async fn test_mock_multi_turn_conversation() {
    // Arrange
    let provider = MockProvider::from_text("Mock response");

    let messages = vec![
        ChatMessage::user("Hello".to_string()),
        ChatMessage::assistant("Hi there!".to_string()),
        ChatMessage::user("How are you?".to_string()),
    ];

    let request = CompletionRequest::new("mock".to_string(), messages);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert_eq!(response.content, "Mock response");
}

#[tokio::test]
async fn test_mock_with_system_prompt() {
    // Arrange
    let provider = MockProvider::from_text("Response with system prompt");

    let request = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Test".to_string())],
    )
    .with_system_prompt("You are a helpful assistant.".to_string());

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert_eq!(response.content, "Response with system prompt");
}

#[tokio::test]
async fn test_mock_with_temperature_and_max_tokens() {
    // Arrange
    let provider = MockProvider::from_text("Response with parameters");

    let request = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Test".to_string())],
    )
    .with_temperature(0.7)
    .with_max_tokens(1000);

    // Act
    let response = LLMProvider::complete(&provider, request).await.unwrap();

    // Assert
    assert_eq!(response.content, "Response with parameters");
}
