//! Basic usage example for rustycode-llm
//!
//! This example demonstrates how to:
//! - Configure a provider
//! - Create a completion request
//! - Handle responses and errors

use anyhow::Result;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, OpenAiProvider, ProviderConfig,
};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<()> {
    // Example 1: Simple completion with Anthropic
    anthropic_example().await?;

    // Example 2: Simple completion with OpenAI
    openai_example().await?;

    Ok(())
}

/// Demonstrates basic usage with Anthropic Claude
async fn anthropic_example() -> Result<()> {
    println!("=== Anthropic Claude Example ===\n");

    // Configure the provider
    let config = ProviderConfig {
        api_key: std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .map(SecretString::from),
        base_url: Some("https://api.anthropic.com".to_string()),
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let provider = AnthropicProvider::new(config, "claude-3-opus-20240229".to_string())?;

    // Create a completion request
    let request = CompletionRequest::new(
        "claude-3-opus-20240229".to_string(),
        vec![ChatMessage::user(
            "What is Rust programming language?".to_string(),
        )],
    );

    // Send the request
    match provider.complete(request).await {
        Ok(response) => {
            println!("Response: {}\n", response.content);
            println!("Model: {}", response.model);
            if let Some(usage) = response.usage {
                println!(
                    "Tokens: {} input + {} output = {} total",
                    usage.input_tokens, usage.output_tokens, usage.total_tokens
                );
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

/// Demonstrates basic usage with OpenAI GPT
async fn openai_example() -> Result<()> {
    println!("=== OpenAI GPT Example ===\n");

    // Configure the provider
    let config = ProviderConfig {
        api_key: std::env::var("OPENAI_API_KEY").ok().map(SecretString::from),
        base_url: Some("https://api.openai.com/v1".to_string()),
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenAiProvider::new(config, "gpt-4".to_string())?;

    // Create a completion request
    let request = CompletionRequest::new(
        "gpt-4".to_string(),
        vec![ChatMessage::user(
            "Explain async/await in Rust.".to_string(),
        )],
    )
    .with_temperature(0.7)
    .with_max_tokens(500);

    // Send the request
    match provider.complete(request).await {
        Ok(response) => {
            println!("Response: {}\n", response.content);
            println!("Model: {}", response.model);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}
