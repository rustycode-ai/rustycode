#![allow(
    clippy::manual_let_else,
    clippy::redundant_clone,
    clippy::single_match_else,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Test tool definitions format for Anthropic
use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, ProviderConfig},
};
use rustycode_tools::default_registry;
use secrecy::SecretString;
use std::env;

#[test]
fn test_tool_definitions_format() {
    // Create a tool registry with default tools registered
    let tool_registry = default_registry();

    // Get tool definitions
    let tools = tool_registry.list();

    println!("Available tools: {}", tools.len());
    for tool in &tools {
        println!("\nTool: {}", tool.name);
        println!("  Description: {}", tool.description);
        println!(
            "  Parameters: {}",
            serde_json::to_string_pretty(&tool.parameters_schema).unwrap()
        );
    }

    // Verify tool definitions can be serialized to JSON
    for tool in &tools {
        let tool_def = serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": {
                "type": "object",
                "properties": tool.parameters_schema,
                "required": []
            }
        });

        // Verify it's valid JSON
        let json_str = serde_json::to_string(&tool_def).unwrap();
        println!("\nFormatted tool definition:\n{}", json_str);

        // Verify we can deserialize it back
        let _value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    }
}

#[test]
fn test_anthropic_request_with_tools() {
    // Skip test if API key not set
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(300),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-haiku".to_string());

    let _provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping test: Failed to create provider: {:?}", e);
            return;
        }
    };

    // Create tool definitions
    let tool_registry = default_registry();
    let tools_list = tool_registry.list();
    let tool_definitions: Vec<serde_json::Value> =
        rustycode_tools_api::build_canonical_tool_schemas(&tools_list);

    println!("Tool definitions count: {}", tool_definitions.len());

    // Create a request with tools
    let messages = vec![ChatMessage::user(
        "Read the Cargo.toml file and tell me the project name.".to_string(),
    )];

    let request = CompletionRequest::new(model.clone(), messages)
        .with_system_prompt(
            "You are RustyCode, an AI coding assistant. Use tools when needed.".to_string(),
        )
        .with_tools(tool_definitions.clone());

    println!("Request created with tools: {:?}", request.tools.is_some());

    // Note: We're not actually sending the request here, just verifying it compiles
    // The actual tool calling test would require a full round-trip
    assert!(request.tools.is_some(), "Tools should be set");
    assert_eq!(
        request.tools.as_ref().unwrap().len(),
        tool_definitions.len()
    );

    println!("✓ Tool definitions properly formatted for Anthropic");
}
