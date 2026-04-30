//! Streaming responses example for rustycode-llm
//!
//! This example demonstrates how to:
//! - Use streaming responses
//! - Process real-time text generation
//! - Handle streaming errors

use anyhow::Result;
use futures::StreamExt;
use rustycode_llm::{
    AnthropicProvider, ChatMessage, CompletionRequest, LLMProvider, ProviderConfig,
};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<()> {
    streaming_example().await
}

/// Demonstrates streaming responses
async fn streaming_example() -> Result<()> {
    println!("=== Streaming Example ===\n");

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

    let provider = AnthropicProvider::new(config, "claude-opus-4-6".to_string())?;

    // Create a streaming request
    let request = CompletionRequest::new(
        "claude-opus-4-6".to_string(),
        vec![ChatMessage::user(
            "Count from 1 to 10, then tell me a joke.".to_string(),
        )],
    )
    .with_streaming(true);

    // Create the stream
    match provider.complete_stream(request).await {
        Ok(mut stream) => {
            println!("Response (streaming): ");

            // Process each chunk as it arrives
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        if let Some(text) = chunk.as_text() {
                            print!("{}", text);
                            use std::io::Write;
                            std::io::stdout().flush().unwrap();
                        }
                    }
                    Err(e) => {
                        eprintln!("\nStream error: {}", e);
                        break;
                    }
                }
            }

            println!(); // Final newline
        }
        Err(e) => {
            eprintln!("Failed to create stream: {}", e);
        }
    }

    Ok(())
}
