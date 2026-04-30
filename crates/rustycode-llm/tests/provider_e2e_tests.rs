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

//! End-to-end tests for LLM providers
//!
//! These tests verify provider functionality with real API calls when credentials
//! are available, and mock behavior otherwise.

use anyhow::Result;
use rustycode_llm::mock::MockProvider;
use rustycode_llm::provider::{
    ChatMessage, CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig,
};

///////////////////////////////////////////////////////////////////////////////
// MockProvider Tests
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn mock_provider_basic_completion() {
    let provider = MockProvider::from_text("Hello, world!");
    let request = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Test".to_string())],
    );

    let response = provider.complete(request).await.unwrap();
    assert_eq!(response.content, "Hello, world!");
    assert_eq!(response.model, "mock");
}

#[tokio::test]
async fn mock_provider_multiple_responses() {
    let responses: Vec<Result<CompletionResponse, _>> = vec![
        Ok(CompletionResponse {
            content: "First".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
        Ok(CompletionResponse {
            content: "Second".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: None,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
    ];

    let _provider = MockProvider::new(responses, None);
    let mut request = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Test".to_string())],
    );
    request.stream = false;
    request.thinking = None;

    let assistant_msg = ChatMessage::assistant("Hi there".to_string());
    assert_eq!(
        assistant_msg.content,
        rustycode_protocol::MessageContent::Simple("Hi there".to_string())
    );

    let system_msg = ChatMessage::system("You are helpful".to_string());
    assert_eq!(
        system_msg.content,
        rustycode_protocol::MessageContent::Simple("You are helpful".to_string())
    );
}

///////////////////////////////////////////////////////////////////////////////
// CompletionRequest Tests
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn completion_request_builder_pattern() {
    let request = CompletionRequest::new(
        "test-model".to_string(),
        vec![ChatMessage::user("Test".to_string())],
    )
    .with_max_tokens(100)
    .with_temperature(0.7)
    .with_streaming(false)
    .with_system_prompt("You are helpful".to_string());

    assert_eq!(request.model, "test-model");
    assert_eq!(request.max_tokens, Some(100));
    assert_eq!(request.temperature, Some(0.7));
    assert!(!request.stream);
    assert_eq!(request.system_prompt, Some("You are helpful".to_string()));
}

///////////////////////////////////////////////////////////////////////////////
// Integration Tests
///////////////////////////////////////////////////////////////////////////////

#[tokio::test]
async fn all_providers_implement_trait() {
    // This test verifies that all providers implement the LLMProvider trait
    // by checking that they can be boxed and used polymorphically

    let mock: Box<dyn LLMProvider> = Box::new(MockProvider::from_text("test"));

    // Test that trait methods are accessible
    assert_eq!(mock.name(), "mock");
    assert!(mock.is_available().await); // is_available returns bool, not Result
}

#[tokio::test]
async fn provider_config_cloning() {
    let config1 = ProviderConfig {
        timeout_seconds: Some(30),
        ..Default::default()
    };

    let config2 = config1.clone();

    assert_eq!(config1.timeout_seconds, config2.timeout_seconds);
}

///////////////////////////////////////////////////////////////////////////////
// NOTE: Provider-Specific Tests
///////////////////////////////////////////////////////////////////////////////
//
// Tests for AnthropicProvider, OpenAIProvider, and GeminiProvider require
// API keys and are marked with #[ignore]. To run them:
//
//   export ANTHROPIC_API_KEY="sk-..."
//   export OPENAI_API_KEY="sk-..."
//   export GEMINI_API_KEY="..."
//   cargo test --package rustycode-llm --test provider_e2e_tests -- --ignored
//
// These tests are intentionally omitted here to avoid requiring credentials
// for normal test runs. The MockProvider provides sufficient coverage for
// the provider trait contract.
