//! Multi-provider example for rustycode-llm
//!
//! This example demonstrates how to:
//! - Use multiple providers
//! - Compare responses across providers
//! - Implement provider fallback logic

use anyhow::Result;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, OpenAiProvider, ProviderConfig,
    ProviderError,
};
use secrecy::SecretString;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    // Example 1: Query multiple providers
    multi_provider_query().await?;

    // Example 2: Provider fallback
    provider_fallback().await?;

    Ok(())
}

/// Query multiple providers and compare responses
async fn multi_provider_query() -> Result<()> {
    println!("=== Multi-Provider Query ===\n");

        let prompt = "What is the capital of France?";

    // Configure multiple providers
    let mut providers: HashMap<&str, Box<dyn LLMProvider>> = HashMap::new();

    // Anthropic
    if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        let config = ProviderConfig {
            api_key: Some(SecretString::from(api_key)),
            base_url: Some("https://api.anthropic.com".to_string()),
            timeout_seconds: Some(180),
            extra_headers: None,
            retry_config: None,
        };
        providers.insert(
            "anthropic",
            Box::new(AnthropicProvider::new(
                config,
                "claude-sonnet-4-6".to_string(),
            )?),
        );
    }

    // OpenAI
    if let Ok(api_key) = std::env::var("OPENAI_API_KEY") {
        let config = ProviderConfig {
            api_key: Some(SecretString::from(api_key)),
            base_url: Some("https://api.openai.com/v1".to_string()),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };
        providers.insert(
            "openai",
            Box::new(OpenAiProvider::new(config, "gpt-4".to_string())?),
        );
    }

    println!("Querying {} providers...\n", providers.len());

    // Query all providers
    let mut responses = HashMap::new();
    for (name, provider) in &providers {
        let model = match *name {
            "anthropic" => "claude-opus-4-6",
            "openai" => "gpt-4",
            _ => continue,
        };

        let request = CompletionRequest::new(
            model.to_string(),
            vec![ChatMessage::user(prompt.to_string())],
        );

        match provider.complete(request).await {
            Ok(response) => {
                println!("✅ {} ({})", name, response.model);
                println!("   {}\n", response.content);
                responses.insert(name, response);
            }
            Err(e) => {
                println!("❌ {}: {}\n", name, e);
            }
        }
    }

    // Compare responses
    if responses.len() > 1 {
        println!("=== Comparison ===");
        for (name, response) in &responses {
            if let Some(usage) = &response.usage {
                println!("{}: {} tokens", name, usage.total_tokens);
            }
        }
    }

    Ok(())
}

/// Demonstrate provider fallback logic
async fn provider_fallback() -> Result<()> {
    println!("=== Provider Fallback ===\n");

    let prompt = "Tell me a short joke.";

    // Try providers in order of preference
    let providers = vec![("anthropic", "claude-opus-4-6"), ("openai", "gpt-4")];

    for (provider_name, model) in providers {
        println!("Trying {}...", provider_name);

        let config = match provider_name {
            "anthropic" => ProviderConfig {
                api_key: std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .map(SecretString::from),
                base_url: Some("https://api.anthropic.com".to_string()),
                timeout_seconds: Some(180),
                extra_headers: None,
                retry_config: None,
            },
            "openai" => ProviderConfig {
                api_key: std::env::var("OPENAI_API_KEY").ok().map(SecretString::from),
                base_url: Some("https://api.openai.com/v1".to_string()),
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            },
            _ => continue,
        };

        let provider: Result<Box<dyn LLMProvider>, ProviderError> = match provider_name {
            "anthropic" => Ok(Box::new(AnthropicProvider::new(config, model.to_string())?)),
            "openai" => Ok(Box::new(OpenAiProvider::new(config, model.to_string())?)),
            _ => continue,
        };

        if let Ok(provider) = provider {
            let request = CompletionRequest::new(
                model.to_string(),
                vec![ChatMessage::user(prompt.to_string())],
            );

            match provider.complete(request).await {
                Ok(response) => {
                    println!("✅ Success with {}!\n", provider_name);
                    println!("{}", response.content);
                    return Ok(());
                }
                Err(e) => {
                    println!("❌ Failed: {}\n", e);
                    // Try next provider
                    continue;
                }
            }
        }
    }

    eprintln!("All providers failed");
    Ok(())
}
