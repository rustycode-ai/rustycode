//! Test OpenRouter free model integration
//!
//! This test demonstrates the OpenRouter provider working with free models.
//!
//! Setup:
//! ```bash
//! export OPENROUTER_API_KEY=sk-or-...
//! cargo run --example test_openrouter
//! ```

use rustycode_llm::{
    ChatMessage, CompletionRequest, LLMProvider, OpenRouterProvider, ProviderConfig,
};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load API key from environment
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY environment variable not set");

    println!("=== OpenRouter Free Model Test ===");
    println!();

    // Test free models
    let free_models = vec![
        "google/gemma-2-9b:free",
        "meta-llama/llama-3-8b:free",
        "microsoft/phi-3-medium-128k:free",
        "mistralai/mistral-7b:free",
    ];

    for model in free_models {
        println!("Testing model: {}", model);

        let config = ProviderConfig {
            api_key: Some(SecretString::new(api_key.clone().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let provider = OpenRouterProvider::new(config, model.to_string())?;

        let request = CompletionRequest {
            model: model.to_string(),
            messages: vec![ChatMessage::user(
                "Say 'Hello from OpenRouter!' in one sentence.".to_string(),
            )],
            max_tokens: Some(100),
            temperature: Some(0.7),
            stream: false,
            system_prompt: None,
            tools: None,
            thinking: None,
            output_config: None,
            container: None,
        };

        match provider.complete(request).await {
            Ok(response) => {
                println!("  ✓ Success!");
                println!("  Response: {}", response.content);
                if let Some(usage) = response.usage {
                    println!(
                        "  Tokens: {} in, {} out, {} total",
                        usage.input_tokens, usage.output_tokens, usage.total_tokens
                    );
                }
                println!();
            }
            Err(e) => {
                println!("  ✗ Error: {}", e);
                println!();
            }
        }
    }

    println!("=== Test Complete ===");

    Ok(())
}
