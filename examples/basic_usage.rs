//! Basic usage example for rustycode-llm
//!
//! This example demonstrates the standard way to configure and use
//! LLM providers with the RustyCode framework.

use anyhow::Result;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, OpenAiProvider, ProviderConfig,
};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Anthropic Example
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").ok().map(SecretString::from);
    let anthropic_config = ProviderConfig {
        api_key: anthropic_key,
        ..Default::default()
    };
    
    let anthropic = AnthropicProvider::new(anthropic_config, "claude-3-5-sonnet-20240620".to_string())?;
    run_example("Anthropic", &anthropic, "Explain quantum computing simply.").await?;

    // 2. OpenAI Example
    let openai_key = std::env::var("OPENAI_API_KEY").ok().map(SecretString::from);
    let openai_config = ProviderConfig {
        api_key: openai_key,
        ..Default::default()
    };
    
    let openai = OpenAiProvider::new(openai_config, "gpt-4o".to_string())?;
    run_example("OpenAI", &openai, "Explain async/await in Rust.").await?;

    Ok(())
}

/// Helper to run a completion and display results
async fn run_example<P: LLMProvider>(name: &str, provider: &P, prompt: &str) -> Result<()> {
    println!("=== {} Example ===\n", name);

    if !provider.is_available().await {
        println!("Provider {} is not available (missing API key).\n", name);
        return Ok(());
    }

    let request = CompletionRequest::new(
        provider.name().to_string(), // Simplified usage of model id
        vec![ChatMessage::user(prompt.to_string())],
    );

    match provider.complete(request).await {
        Ok(response) => {
            println!("Response: {}\n", response.content);
            println!("Model: {}", response.model);
        }
        Err(e) => eprintln!("Error calling {}: {}", name, e),
    }
    println!();
    Ok(())
}
