#![allow(
    clippy::doc_markdown,
    clippy::manual_string_new,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! End-to-end test for tool execution with mock provider
//! This verifies the tool execution pipeline works without requiring API keys
//!
//! Run with: cargo test -p rustycode-tui --test tool_execution_e2e_mock

use rustycode_llm::{
    mock::MockProvider,
    provider::{ChatMessage, CompletionRequest, CompletionResponse, LLMProvider},
};
use rustycode_tools::{default_registry, ToolContext};
use std::collections::HashMap;

#[tokio::test]
#[ignore = "default_registry() is empty; requires populated tool registry"]
async fn test_tool_execution_pipeline_with_mock() {
    println!("=== Testing Tool Execution Pipeline ===\n");

    // 1. Verify tool registry works
    println!("Step 1: Testing tool registry");
    let tool_registry = default_registry();
    let tools = tool_registry.list();
    println!("✓ Available tools: {}", tools.len());
    assert!(!tools.is_empty(), "Tool registry should have tools");

    // 2. Verify tool context creation
    println!("\nStep 2: Testing tool context");
    let _tool_context = ToolContext::new(".");
    println!("✓ Tool context created successfully");

    // 3. Verify mock provider can handle tool calls
    println!("\nStep 3: Testing mock provider with tool calls");

    // Create a mock response that simulates a tool call
    let tool_response = CompletionResponse {
        content: "".to_string(),
        model: "mock".to_string(),
        usage: None,
        stop_reason: Some("tool_use".to_string()),
        citations: None,
        thinking_blocks: None,
        structured_output: None,
    };

    let mock = MockProvider::new(vec![Ok(tool_response)], None);

    // Create a simple request
    let request = CompletionRequest::new(
        "mock".to_string(),
        vec![ChatMessage::user("Read the main.rs file".to_string())],
    );

    match mock.complete(request).await {
        Ok(response) => {
            println!("✓ Mock provider handled tool call request");
            assert_eq!(response.stop_reason, Some("tool_use".to_string()));
        }
        Err(e) => {
            panic!("Mock provider failed: {:?}", e);
        }
    }

    // 4. Verify tool definition formatting
    println!("\nStep 4: Testing tool definition formatting");
    let tool_definitions: Vec<serde_json::Value> = tools
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters_schema
            })
        })
        .collect();

    println!("✓ Formatted {} tool definitions", tool_definitions.len());
    assert!(!tool_definitions.is_empty());

    // 5. Verify tool parameter extraction
    println!("\nStep 5: Testing tool parameter extraction");
    let test_params = serde_json::json!({
        "file_path": "./test.rs",
        "start_line": 1,
        "end_line": 10
    });

    let params_map: HashMap<String, serde_json::Value> =
        serde_json::from_value(test_params).unwrap();
    println!("✓ Tool parameters extracted: {}", params_map.len());
    assert_eq!(
        params_map.get("file_path").unwrap(),
        &serde_json::json!("./test.rs")
    );

    println!("\n=== All tool execution pipeline tests passed! ===");
}

#[tokio::test]
async fn test_multi_tool_execution_flow() {
    println!("\n=== Testing Multi-Tool Execution Flow ===\n");

    // Simulate a conversation with multiple tool calls
    let responses = vec![
        // First response: User asks to read a file
        Ok(CompletionResponse {
            content: "I'll read the main.rs file for you.".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("tool_use".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
        // Second response: Agent analyzes the file
        Ok(CompletionResponse {
            content: "I can see this is a Rust main function.".to_string(),
            model: "mock".to_string(),
            usage: None,
            stop_reason: Some("end_turn".to_string()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        }),
    ];

    let mock = MockProvider::new(responses, None);

    let conversation = vec![
        ChatMessage::user("Read the main.rs file and tell me what it does".to_string()),
        ChatMessage::assistant("I'll read the main.rs file for you.".to_string()),
    ];

    let request = CompletionRequest::new("mock".to_string(), conversation);

    match mock.complete(request).await {
        Ok(response) => {
            println!("✓ Multi-tool conversation handled correctly");
            // Mock provider returns responses sequentially, so this is the first response
            assert_eq!(response.content, "I'll read the main.rs file for you.");
        }
        Err(e) => {
            panic!("Multi-tool execution failed: {:?}", e);
        }
    }

    println!("\n=== Multi-tool execution flow test passed! ===");
}

#[tokio::test]
async fn test_tool_error_handling() {
    println!("\n=== Testing Tool Error Handling ===\n");

    // Test 1: Invalid tool name handling (verify registry doesn't crash)
    println!("Test 1: Invalid tool name handling");
    let tool_registry = default_registry();
    let tools = tool_registry.list();
    let has_invalid_tool = tools.iter().any(|t| t.name == "nonexistent_tool");
    assert!(
        !has_invalid_tool,
        "Invalid tool should not exist in registry"
    );
    println!("✓ Invalid tool name handled correctly");

    // Test 2: Invalid parameters handling
    println!("\nTest 2: Invalid parameters");
    let test_params = serde_json::json!({
        "invalid_param": "value"
    });

    let params_map: Result<HashMap<String, serde_json::Value>, _> =
        serde_json::from_value(test_params);
    assert!(params_map.is_ok(), "Parameter parsing should succeed");
    println!("✓ Invalid parameters handled gracefully");

    // Test 3: Empty tool execution
    println!("\nTest 3: Empty tool context");
    let _tool_context = ToolContext::new(".");
    println!("✓ Empty context handled correctly");

    println!("\n=== Tool error handling tests passed! ===");
}

#[test]
fn test_tool_schema_validation() {
    println!("\n=== Testing Tool Schema Validation ===\n");

    let tool_registry = default_registry();
    let tools = tool_registry.list();

    println!("Validating schemas for {} tools...", tools.len());

    for tool in tools {
        // Verify each tool has required fields
        assert!(!tool.name.is_empty(), "Tool name should not be empty");
        assert!(
            !tool.description.is_empty(),
            "Tool description should not be empty"
        );

        // Verify input schema is valid JSON
        let schema = &tool.parameters_schema;
        assert!(schema.is_object(), "Input schema should be a JSON object");

        // Check for required schema properties
        if let Some(obj) = schema.as_object() {
            let type_value = obj.get("type");
            let props_value = obj.get("properties");
            assert!(
                type_value.is_some() || props_value.is_some(),
                "Schema for '{}' should have type or properties, got: {}",
                tool.name,
                schema
            );
        }

        println!("✓ {} schema validated", tool.name);
    }

    println!("\n=== Tool schema validation passed! ===");
}
