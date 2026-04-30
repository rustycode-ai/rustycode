//! Error handling example for rustycode-llm
//!
//! This example demonstrates how to:
//! - Handle different error types
//! - Implement retry logic
//! - Classify errors for recovery

use anyhow::Result;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, ProviderConfig, ProviderError,
};
use secrecy::SecretString;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    error_handling_example().await
}

/// Demonstrates comprehensive error handling
async fn error_handling_example() -> Result<()> {
    println!("=== Error Handling Example ===\n");

    let config = ProviderConfig {
        // Try with invalid API key to see error handling
        api_key: Some(SecretString::from("invalid-key")),
        base_url: Some("https://api.anthropic.com".to_string()),
        timeout_seconds: Some(10), // Short timeout for demo
        extra_headers: None,
        retry_config: None,
    };

    let provider = AnthropicProvider::new(config, "claude-sonnet-4-6".to_string())?;
    let request = CompletionRequest::new(
        "claude-opus-4-6".to_string(),
        vec![ChatMessage::user("Hello!".to_string())],
    );

    // Attempt the request with error handling
    match provider.complete(request).await {
        Ok(response) => {
            println!("Success: {}", response.content);
        }
        Err(e) => {
            handle_error(e);
        }
    }

    Ok(())
}

/// Handle different error types with specific strategies
fn handle_error(error: ProviderError) {
    match &error {
        ProviderError::Auth(msg) => {
            eprintln!("❌ Authentication Error: {}", msg);
            eprintln!("   → Check your API key");
        }
        ProviderError::RateLimited { retry_delay: None } => {
            eprintln!("⏱️  Rate Limited");
            eprintln!("   → Wait before retrying");
            eprintln!("   → Consider implementing exponential backoff");
        }
        ProviderError::Network(msg) => {
            eprintln!("🌐 Network Error: {}", msg);
            eprintln!("   → Check your internet connection");
            eprintln!("   → The API might be temporarily unavailable");
        }
        ProviderError::Timeout(msg) => {
            eprintln!("⏰ Timeout: {}", msg);
            eprintln!("   → The request took too long");
            eprintln!("   → Try with a shorter prompt or different model");
        }
        ProviderError::InvalidModel(model) => {
            eprintln!("🤖 Invalid Model: {}", model);
            eprintln!("   → Check the model name");
            eprintln!("   → Ensure you have access to this model");
        }
        ProviderError::Api(msg) => {
            eprintln!("📡 API Error: {}", msg);
            eprintln!("   → The API returned an error");
            eprintln!("   → Check API status page");
        }
        ProviderError::Configuration(msg) => {
            eprintln!("⚙️  Configuration Error: {}", msg);
            eprintln!("   → Check your provider configuration");
        }
        ProviderError::Serialization(msg) => {
            eprintln!("🔧 Serialization Error: {}", msg);
            eprintln!("   → Failed to parse API response");
            eprintln!("   → API may have changed its format");
        }
        ProviderError::Unknown(msg) => {
            eprintln!("❓ Unknown Error: {}", msg);
            eprintln!("   → An unexpected error occurred");
        }
    }

    // Demonstrate retry logic for recoverable errors
    if is_recoverable(&error) {
        eprintln!("\n💡 This error is recoverable - you could retry with:");
        eprintln!("   - Exponential backoff (start at 1s, max 32s)");
        eprintln!("   - Jitter to avoid thundering herd");
        eprintln!("   - Max 3-5 retries");
    }
}

/// Check if an error is recoverable (worth retrying)
fn is_recoverable(error: &ProviderError) -> bool {
    matches!(
        error,
        ProviderError::RateLimited { retry_delay: None }
            | ProviderError::Network(_)
            | ProviderError::Timeout(_)
            | ProviderError::Api(_)
    )
}

/// Example: Retry with exponential backoff
async fn retry_with_backoff<T, F, Fut>(mut operation: F) -> Result<T, ProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ProviderError>>,
{
    let max_retries = 3;
    let mut delay = Duration::from_secs(1);

    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if is_recoverable(&e) && attempt < max_retries - 1 => {
                eprintln!("Attempt {} failed, retrying in {:?}...", attempt + 1, delay);
                sleep(delay).await;
                delay = std::cmp::min(delay * 2, Duration::from_secs(32));
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}
