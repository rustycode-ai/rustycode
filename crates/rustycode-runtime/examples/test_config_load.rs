#![allow(clippy::uninlined_format_args)]
// Test config loading diagnostics
use rustycode_runtime::agent::load_provider_config;

fn main() {
    env_logger::init();

    println!("=== Testing Provider Configuration Loading ===\n");

    match load_provider_config() {
        Ok((provider_type, model, config)) => {
            println!("✓ Config loaded successfully!");
            println!("  Provider: {}", provider_type);
            println!("  Model: {}", model);
            println!("  Base URL: {:?}", config.base_url);
            println!(
                "  API Key: {}",
                if config.api_key.is_some() {
                    "***SET***"
                } else {
                    "NOT SET"
                }
            );
            println!("  Timeout: {:?}", config.timeout_seconds);

            if let Some(base_url) = config.base_url {
                println!("\n✓ Base URL configured: {}", base_url);

                // Test endpoint construction
                let endpoint = if base_url.ends_with('/') {
                    format!("{}v1/messages", base_url)
                } else {
                    format!("{}/v1/messages", base_url)
                };
                println!("  Full endpoint would be: {}", endpoint);
            } else {
                println!("\n✗ No base_url configured - will use default Anthropic endpoint");
            }
        }
        Err(e) => {
            println!("✗ Failed to load config: {}", e);
        }
    }
}
