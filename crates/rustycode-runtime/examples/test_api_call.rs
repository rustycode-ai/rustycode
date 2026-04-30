#![allow(clippy::uninlined_format_args)]
// Test API call to verify endpoint is correct
use rustycode_llm::provider::MessageContent;
use rustycode_llm::{ChatMessage, CompletionRequest, MessageRole};
use rustycode_runtime::agent::{create_provider_from_config, load_provider_config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("=== Testing API Call with Loaded Config ===\n");

    // Load config
    let (provider_type, _model, config) = load_provider_config()?;
    println!("✓ Config loaded for provider: {}", provider_type);
    println!("  Base URL: {:?}", config.base_url);

    // Create provider
    let provider = create_provider_from_config()?;
    println!("✓ Provider created successfully");

    // Test with a simple completion request
    let request = CompletionRequest {
        model: "claude-sonnet-4-6".to_string(),
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(
                "What is 2+2? Answer with just the number.".to_string(),
            ),
        }],
        max_tokens: Some(10),
        temperature: Some(0.7),
        system_prompt: None,
        tools: None,
        stream: false,
        thinking: None,
        output_config: None,
        container: None,
        tool_choice: None,
        parallel_tool_calls: None,
        session_id: None,
    };

    println!("\nSending test request...");

    match provider.complete(request).await {
        Ok(response) => {
            println!("✓ API call successful!");
            println!("  Model: {}", response.model);
            println!("  Content: {}", response.content);
            if let Some(usage) = response.usage {
                println!(
                    "  Usage: {} input tokens, {} output tokens",
                    usage.input_tokens, usage.output_tokens
                );
            }
        }
        Err(e) => {
            println!("✗ API call failed: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
