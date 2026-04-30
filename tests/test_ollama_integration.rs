// Integration test for Ollama provider
// Run with: cargo run --bin test_ollama_integration

use rustycode_llm::ollama::OllamaProvider;
use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Ollama Integration Test ===\n");

    // Create provider with default endpoint
    let provider = OllamaProvider::with_default_endpoint()?;
    println!("✓ Created Ollama provider");
    if let Some(config) = provider.config() {
        println!(
            "  Base URL: {}\n",
            config
                .base_url
                .as_ref()
                .unwrap_or(&"http://localhost:11434".to_string())
        );
    } else {
        println!("  Base URL: http://localhost:11434 (default)\n");
    }

    // Test 1: Check if Ollama is available
    println!("Test 1: Checking availability...");
    let is_available = provider.is_available().await;
    if is_available {
        println!("✓ Ollama server is running\n");
    } else {
        println!("✗ Ollama server is not running");
        println!("  Start it with: ollama serve");
        return Ok(());
    }

    // Test 2: List available models
    println!("Test 2: Listing available models...");
    match provider.list_models().await {
        Ok(models) => {
            println!("✓ Found {} models:", models.len());
            for model in &models {
                println!("  - {}", model);
            }
            println!();
        }
        Err(e) => {
            println!("✗ Failed to list models: {}\n", e);
            return Ok(());
        }
    }

    // Test 3: Test completion with a small model
    println!("Test 3: Testing completion with phi3...");

    // Find a small model to test with
    let models = provider.list_models().await?;
    let test_model = models
        .iter()
        .find(|m| m.contains("phi3") || m.contains("qwen2.5-coder:1.5b"))
        .or_else(|| models.first())
        .cloned()
        .unwrap_or_else(|| "phi3:latest".to_string());

    println!("  Using model: {}\n", test_model);

    let request = CompletionRequest {
        model: test_model.clone(),
        messages: vec![ChatMessage::user(
            "Say 'Hello, Ollama!' in one sentence.".to_string(),
        )],
        temperature: Some(0.7),
        max_tokens: Some(50),
        stream: false,
        system_prompt: None,
        tools: None,
        thinking: None,
        output_config: None,
        container: None,
    };

    match provider.complete(request).await {
        Ok(response) => {
            println!("✓ Completion successful!");
            println!("  Model: {}", response.model);
            println!("  Response: {}", response.content);
            if let Some(usage) = response.usage {
                println!("  Usage:");
                println!("    Input tokens: {}", usage.input_tokens);
                println!("    Output tokens: {}", usage.output_tokens);
                println!("    Total tokens: {}", usage.total_tokens);
            } else {
                println!("  Usage: Not reported by Ollama");
            }
            println!();
        }
        Err(e) => {
            println!("✗ Completion failed: {}\n", e);
            return Ok(());
        }
    }

    // Test 4: Test streaming
    println!("Test 4: Testing streaming completion...");

    let request = CompletionRequest {
        model: test_model.clone(),
        messages: vec![ChatMessage::user("Count from 1 to 5.".to_string())],
        temperature: Some(0.7),
        max_tokens: Some(50),
        stream: true,
        system_prompt: None,
        tools: None,
        thinking: None,
        output_config: None,
        container: None,
    };

    match provider.complete_stream(request).await {
        Ok(mut stream) => {
            println!("  Streaming response: ");
            let mut full_response = String::new();
            while let Some(chunk_result) = futures::StreamExt::next(&mut stream).await {
                match chunk_result {
                    Ok(chunk) => {
                        if let Some(text) = chunk.as_text() {
                            print!("{}", text);
                            full_response.push_str(&text);
                            use std::io::Write;
                            std::io::stdout().flush().ok();
                        }
                    }
                    Err(e) => {
                        println!("\n✗ Stream error: {}\n", e);
                        return Ok(());
                    }
                }
            }
            println!("\n✓ Streaming completed!\n");
        }
        Err(e) => {
            println!("✗ Streaming failed: {}\n", e);
        }
    }

    println!("=== All tests completed successfully! ===");
    Ok(())
}
