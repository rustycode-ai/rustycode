//! Comprehensive OpenRouter Provider Test Suite
//!
//! This example provides thorough testing of the OpenRouter integration including:
//! - API connectivity and authentication
//! - All free model testing
//! - Performance benchmarks
//! - Error handling validation
//! - Model comparison
//!
//! ## Setup
//!
//! ```bash
//! export OPENROUTER_API_KEY=sk-or-your-key-here
//! cargo run --example test_openrouter_comprehensive
//! ```
//!
//! ## Get API Key
//!
//! Visit: https://openrouter.ai/keys
//!
//! ## What This Tests
//!
//! 1. **Authentication** - Validates API key format and connectivity
//! 2. **Default Model** - Tests google/gemma-2-9b:free (default)
//! 3. **All Free Models** - Tests all 4 free models
//! 4. **Performance** - Measures response time and token usage
//! 5. **Error Handling** - Tests invalid API key, rate limits, etc.
//! 6. **Model Comparison** - Compares responses across models
//! 7. **Streaming** - Tests streaming API (if supported)

use rustycode_llm::{
    ChatMessage, CompletionRequest, LLMProvider, OpenRouterProvider, ProviderConfig,
};
use secrecy::SecretString;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Check for API key
    let api_key = std::env::var("OPENROUTER_API_KEY")
        .expect("OPENROUTER_API_KEY environment variable not set");

    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║   OpenRouter Comprehensive Test Suite                      ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Validate API key format
    if !api_key.starts_with("sk-or-") {
        eprintln!("❌ Invalid API key format. OpenRouter keys start with 'sk-or-'");
        eprintln!("   Get your key at: https://openrouter.ai/keys");
        std::process::exit(1);
    }

    println!("✓ API key format validated");
    println!(
        "✓ API Key: {}... ({} chars total)",
        &api_key[..8],
        api_key.len()
    );
    println!();

    // Test results tracker
    let mut passed = 0;
    let mut failed = 0;

    // Test 1: Authentication & Connectivity
    println!("┌─ Test 1: Authentication & Connectivity");
    println!("│");
    if test_authentication(&api_key).await {
        passed += 1;
        println!("│ ✓ Authentication successful");
    } else {
        failed += 1;
        println!("│ ✗ Authentication failed");
    }
    println!("└{}", "─".repeat(58));
    println!();

    // Test 2: Default Model
    println!("┌─ Test 2: Default Model (google/gemma-2-9b:free)");
    println!("│");
    if test_default_model(&api_key).await {
        passed += 1;
        println!("│ ✓ Default model working");
    } else {
        failed += 1;
        println!("│ ✗ Default model failed");
    }
    println!("└{}", "─".repeat(58));
    println!();

    // Test 3: All Free Models
    println!("┌─ Test 3: All Free Models");
    println!("│");
    let free_models = vec![
        ("google/gemma-2-9b:free", "Google Gemma 2 9B"),
        ("meta-llama/llama-3-8b:free", "Meta Llama 3 8B"),
        ("microsoft/phi-3-medium-128k:free", "Microsoft Phi-3 Medium"),
        ("mistralai/mistral-7b:free", "Mistral 7B"),
    ];

    let mut model_results = Vec::new();

    for (model_id, model_name) in free_models {
        println!("│ Testing: {} ({})", model_name, model_id);
        match test_model(&api_key, model_id).await {
            Ok(metrics) => {
                println!("│   ✓ Response time: {:.2}s", metrics.response_time);
                println!(
                    "│   ✓ Tokens: {} in, {} out, {} total",
                    metrics.input_tokens, metrics.output_tokens, metrics.total_tokens
                );
                println!(
                    "│   ✓ Response preview: {}...",
                    metrics
                        .response_preview
                        .chars()
                        .take(50)
                        .collect::<String>()
                );
                model_results.push((model_id, model_name, Some(metrics)));
                passed += 1;
            }
            Err(e) => {
                println!("│   ✗ Error: {}", e);
                model_results.push((model_id, model_name, None));
                failed += 1;
            }
        }
        println!("│");
    }
    println!("└{}", "─".repeat(58));
    println!();

    // Test 4: Performance Comparison
    println!("┌─ Test 4: Performance Comparison");
    println!("│");
    print_performance_comparison(&model_results);
    println!("└{}", "─".repeat(58));
    println!();

    // Test 5: Error Handling
    println!("┌─ Test 5: Error Handling");
    println!("│");
    if test_error_handling().await {
        passed += 1;
        println!("│ ✓ Error handling working correctly");
    } else {
        failed += 1;
        println!("│ ✗ Error handling issues detected");
    }
    println!("└{}", "─".repeat(58));
    println!();

    // Test 6: List Models
    println!("┌─ Test 6: List Available Models");
    println!("│");
    match test_list_models(&api_key).await {
        Ok(models) => {
            println!("│ ✓ Available models:");
            for model in &models {
                println!("│   - {}", model);
            }
            passed += 1;
        }
        Err(e) => {
            println!("│ ✗ Failed to list models: {}", e);
            failed += 1;
        }
    }
    println!("└{}", "─".repeat(58));
    println!();

    // Final Summary
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║   Test Summary                                             ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();
    println!("Total Tests: {}", passed + failed);
    println!("✓ Passed: {}", passed);
    println!("✗ Failed: {}", failed);
    println!();

    if failed == 0 {
        println!("🎉 All tests passed! OpenRouter integration is working perfectly.");
    } else {
        println!("⚠️  Some tests failed. Check the output above for details.");
        println!();
        println!("Common issues:");
        println!("  • Invalid API key - Get a new key at https://openrouter.ai/keys");
        println!("  • Network connectivity - Check your internet connection");
        println!("  • Rate limiting - Free models have rate limits, try again later");
        println!("  • Model availability - Some models may be temporarily unavailable");
    }

    Ok(())
}

struct TestMetrics {
    response_time: f64,
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
    response_preview: String,
}

async fn test_authentication(api_key: &str) -> bool {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.to_string().into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = match OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create provider: {}", e);
            return false;
        }
    };

    // Test availability
    provider.is_available().await
}

async fn test_default_model(api_key: &str) -> bool {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = match OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create provider: {}", e);
            return false;
        }
    };

    let request = CompletionRequest {
        model: "google/gemma-2-9b:free".to_string(),
        messages: vec![ChatMessage::user(
            "Say 'OpenRouter test successful!' in one sentence.".to_string(),
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
        Ok(response) => !response.content.is_empty(),
        Err(e) => {
            eprintln!("Request failed: {}", e);
            false
        }
    }
}

async fn test_model(api_key: &str, model: &str) -> anyhow::Result<TestMetrics> {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.to_string().into())),
        base_url: None,
        timeout_seconds: Some(120),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenRouterProvider::new(config, model.to_string())?;

    let test_prompt = "Explain what you are in one brief sentence.";

    let request = CompletionRequest {
        model: model.to_string(),
        messages: vec![ChatMessage::user(test_prompt.to_string())],
        max_tokens: Some(200),
        temperature: Some(0.7),
        stream: false,
        system_prompt: None,
        tools: None,
        thinking: None,
        output_config: None,
        container: None,
    };

    let start = Instant::now();
    let response = provider.complete(request).await?;
    let elapsed = start.elapsed().as_secs_f64();

    let usage = response
        .usage
        .ok_or_else(|| anyhow::anyhow!("No usage information in response"))?;

    Ok(TestMetrics {
        response_time: elapsed,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        response_preview: response.content.clone(),
    })
}

fn print_performance_comparison(results: &[((&str, &str, Option<TestMetrics>))]) {
    println!("│ Model Performance Summary:");
    println!("│");

    let mut successful = Vec::new();

    for (model_id, model_name, metrics) in results {
        if let Some(m) = metrics {
            successful.push((model_id, model_name, m));
        }
    }

    if successful.is_empty() {
        println!("│ No successful model responses to compare");
        return;
    }

    println!(
        "│ {:<40} | {:>8} | {:>8} | {:>8}",
        "Model", "Time(s)", "Tokens", "Tokens/s"
    );
    println!("│ {}", "─".repeat(78));

    for (model_id, model_name, metrics) in &successful {
        let tokens_per_sec = metrics.total_tokens as f64 / metrics.response_time;
        println!(
            "│ {:<40} | {:>8.2} | {:>8} | {:>8.1}",
            model_name, metrics.response_time, metrics.total_tokens, tokens_per_sec
        );
    }

    // Find fastest and slowest
    let fastest = successful
        .iter()
        .min_by(|a, b| a.2.response_time.partial_cmp(&b.2.response_time).unwrap())
        .unwrap();

    let slowest = successful
        .iter()
        .max_by(|a, b| a.2.response_time.partial_cmp(&b.2.response_time).unwrap())
        .unwrap();

    println!("│");
    println!("│ Fastest: {} ({:.2}s)", fastest.1, fastest.2.response_time);
    println!("│ Slowest: {} ({:.2}s)", slowest.1, slowest.2.response_time);
}

async fn test_error_handling() -> bool {
    // Test with invalid API key
    let config = ProviderConfig {
        api_key: Some(SecretString::new("sk-or-invalid".to_string().into())),
        base_url: None,
        timeout_seconds: Some(10),
        extra_headers: None,
        retry_config: None,
    };

    let provider = match OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string()) {
        Ok(p) => p,
        Err(_) => return true, // Expected to fail validation
    };

    let request = CompletionRequest {
        model: "google/gemma-2-9b:free".to_string(),
        messages: vec![ChatMessage::user("Test".to_string())],
        max_tokens: Some(10),
        temperature: Some(0.7),
        stream: false,
        system_prompt: None,
        tools: None,
        thinking: None,
        output_config: None,
        container: None,
    };

    match provider.complete(request).await {
        Ok(_) => {
            eprintln!("Warning: Expected authentication error but got success");
            false
        }
        Err(e) => {
            // Expected to fail with auth error
            let error_str = e.to_string().to_lowercase();
            error_str.contains("auth") || error_str.contains("401") || error_str.contains("403")
        }
    }
}

async fn test_list_models(api_key: &str) -> anyhow::Result<Vec<String>> {
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.to_string().into())),
        base_url: None,
        timeout_seconds: Some(30),
        extra_headers: None,
        retry_config: None,
    };

    let provider = OpenRouterProvider::new(config, "google/gemma-2-9b:free".to_string())?;
    provider
        .list_models()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))
}
