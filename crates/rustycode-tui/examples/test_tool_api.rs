#![allow(clippy::uninlined_format_args)]

// Test to verify tool use API configuration
// Run with: cargo run --bin test_tool_api --release

use rustycode_llm::{create_provider, ChatMessage, CompletionRequest};
use rustycode_tools::default_registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("=== Tool Use API Test ===\n");

    // Load provider config
    let (provider_type, model, _) = rustycode_llm::load_provider_config_from_env()?;
    println!("Provider: {}", provider_type);
    println!("Model: {}\n", model);

    // List available tools
    let registry = default_registry();
    let tools = registry.list();
    println!("Registered tools: {} tools", tools.len());
    for tool in &tools {
        println!("  - {}: {}", tool.name, tool.description);
    }
    println!();

    // Create a simple request without tools for now
    let messages = vec![ChatMessage::user("List files in current directory")];

    let _request = CompletionRequest::new(model.clone(), messages)
        .with_streaming(false)
        .with_max_tokens(1000);

    println!("Request prepared (without tools for API key verification)");

    // Create provider and send request
    let _provider = create_provider(&provider_type, &model)?;

    println!("\nProvider created successfully!");
    println!("To enable tool use, implement generate_tool_schema_for_provider on ToolRegistry");

    Ok(())
}
