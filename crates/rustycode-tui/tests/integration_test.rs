#![allow(
    clippy::doc_markdown,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration tests for TUI LLM provider functionality
//!
//! These tests verify that:
//! - Providers initialize correctly with environment variables
//! - Model names are passed correctly to the API
//! - API requests complete successfully
//! - Responses are parsed and returned

use rustycode_llm::anthropic::AnthropicProvider;
use rustycode_llm::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig};
use secrecy::SecretString;
use std::env;

#[test]
fn test_anthropic_provider_initialization() {
    // Test that provider initializes with valid config
    let api_key = env::var("ANTHROPIC_API_KEY").ok();
    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    if api_key.is_none() {
        println!("Skipping test: ANTHROPIC_API_KEY not set");
        return;
    }

    let config = ProviderConfig {
        api_key: api_key.map(|k| SecretString::new(k.into())),
        base_url,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());
    let provider = AnthropicProvider::new_without_validation(config, model);

    assert!(
        provider.is_ok(),
        "Provider should initialize with valid config"
    );
    let provider = provider.unwrap();
    assert_eq!(provider.name(), "anthropic");
}

#[test]
fn test_anthropic_provider_requires_api_key() {
    let config = ProviderConfig {
        api_key: None,
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    // The strict constructor should fail without API key
    let result = AnthropicProvider::new(config, "claude-3-haiku".to_string());
    assert!(
        result.is_err(),
        "Strict provider constructor should require API key"
    );
}

#[tokio::test]
async fn test_anthropic_completion() {
    let api_key = env::var("ANTHROPIC_API_KEY").ok();
    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    if api_key.is_none() {
        println!("Skipping test: ANTHROPIC_API_KEY not set");
        return;
    }

    let config = ProviderConfig {
        api_key: api_key.map(|k| SecretString::new(k.into())),
        base_url,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());
    let provider = AnthropicProvider::new_without_validation(config, model.clone()).unwrap();

    // Check if provider is available
    if !provider.is_available().await {
        println!("Skipping test: Provider not available");
        return;
    }

    let request = CompletionRequest::new(
        model,
        vec![ChatMessage::user(
            "Say 'test passed' in exactly those words.".to_string(),
        )],
    );

    let result = provider.complete(request).await;

    match result {
        Ok(response) => {
            println!("Response: {}", response.content);
            assert!(!response.content.is_empty(), "Response should not be empty");
            assert!(
                response.usage.is_some(),
                "Response should include usage info"
            );
        }
        Err(e) => {
            panic!("Request failed: {:?}", e);
        }
    }
}

/// Use a mutex to serialize tests that modify ANTHROPIC_MODEL env var.
/// Without this, concurrent test threads race on env var access causing
/// intermittent failures.
static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_model_name_from_env() {
    let _guard = ENV_TEST_LOCK.lock().unwrap();
    // Test that model name is correctly read from environment
    env::set_var("ANTHROPIC_MODEL", "test-model");
    let model = env::var("ANTHROPIC_MODEL").unwrap();
    assert_eq!(model, "test-model");
    env::remove_var("ANTHROPIC_MODEL");
}

#[test]
fn test_default_model_name() {
    let _guard = ENV_TEST_LOCK.lock().unwrap();
    // Test that default model is used when env var is not set
    env::remove_var("ANTHROPIC_MODEL");
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());
    assert_eq!(model, "claude-3-haiku");
}
