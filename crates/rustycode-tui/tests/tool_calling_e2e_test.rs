#![allow(
    clippy::bool_to_int_with_if,
    clippy::branches_sharing_code,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::collapsible_else_if,
    clippy::collection_is_never_read,
    clippy::default_trait_access,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::explicit_iter_loop,
    clippy::float_cmp,
    clippy::if_not_else,
    clippy::ignore_without_reason,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::items_after_statements,
    clippy::iter_on_single_items,
    clippy::literal_string_with_formatting_args,
    clippy::manual_assert,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::needless_collect,
    clippy::needless_continue,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::no_effect_underscore_binding,
    clippy::option_if_let_else,
    clippy::range_plus_one,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::ref_option,
    clippy::search_is_some,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::struct_field_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::trivially_copy_pass_by_ref,
    clippy::unchecked_time_subtraction,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unreadable_literal,
    clippy::unused_async,
    clippy::unused_peekable,
    clippy::unused_self,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::used_underscore_binding,
    clippy::useless_let_if_seq
)]

//! End-to-end test for tool calling with Anthropic API
use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::{default_registry, ToolContext};
use secrecy::SecretString;
use std::env;

#[test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
fn test_tool_calling_e2e() {
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

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("Skipping test: Failed to create provider: {:?}", e);
            return;
        }
    };

    // Create tool registry and generate tool definitions
    let tool_registry = default_registry();
    let tools = tool_registry.list();
    println!("Available tools: {}", tools.len());

    let tool_definitions: Vec<serde_json::Value> =
        rustycode_tools_api::build_canonical_tool_schemas(&tools);

    println!("Tool definitions: {}", tool_definitions.len());

    // Create a simple system prompt
    let system_prompt = "You are RustyCode, an AI coding assistant. Use tools when needed.";

    // Test 1: Simple tool use request
    println!("\n=== Test 1: Read file request ===");
    let messages = vec![ChatMessage::user(
        "Read the Cargo.toml file and tell me the project name.".to_string(),
    )];

    let request = CompletionRequest::new(model.clone(), messages)
        .with_system_prompt(system_prompt.to_string())
        .with_tools(tool_definitions.clone());

    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt.block_on(async { provider.complete(request).await }) {
        Ok(response) => {
            println!("Response: {}", response.content);

            // Check if response contains tool calls
            if response.content.contains("```tool") {
                println!("✓ Tool call detected in response");

                // Parse the tool call
                let tool_start = response.content.find("```tool").unwrap();
                let tool_body = &response.content[tool_start + 8..];
                let tool_end_rel = tool_body.find("```").unwrap_or(tool_body.len());
                let tool_json = &tool_body[..tool_end_rel].trim();

                println!("Tool call JSON: {}", tool_json);

                // Verify it's a read_file call
                if tool_json.contains("Read") {
                    println!("✓ Correct tool (read_file) detected");
                } else {
                    println!("✗ Unexpected tool in response");
                }
            } else {
                println!("✗ No tool call detected in response");
                println!("Response may have answered without using tools");
            }
        }
        Err(e) => {
            println!("✗ Request failed: {:?}", e);
        }
    }

    // Test 2: Request that doesn't need tools
    println!("\n=== Test 2: Non-tool request ===");
    let messages2 = vec![ChatMessage::user("What is 2+2?".to_string())];

    let request2 = CompletionRequest::new(model.clone(), messages2)
        .with_system_prompt(system_prompt.to_string())
        .with_tools(tool_definitions.clone());

    match rt.block_on(async { provider.complete(request2).await }) {
        Ok(response2) => {
            println!("Response: {}", response2.content);

            // Should NOT contain tool calls
            if !response2.content.contains("```tool") {
                println!("✓ No tool call for simple math question (correct)");
            } else {
                println!("✗ Unexpected tool call for simple question");
            }
        }
        Err(e) => {
            println!("✗ Request failed: {:?}", e);
        }
    }

    println!("\n=== Tests Complete ===");
}

#[test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests -- --ignored"
)]
fn test_tool_execution() {
    let _api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    // Test that we can execute tools directly
    let tool_registry = default_registry();
    let ctx = ToolContext::new(env::current_dir().unwrap());

    // Test read_file tool
    if let Some(tool) = tool_registry.get("Read") {
        println!("Testing read_file tool...");

        let params = serde_json::json!({
            "path": "Cargo.toml"
        });

        match tool.execute(params, &ctx) {
            Ok(result) => {
                println!("✓ Tool executed successfully");
                println!("Output length: {} chars", result.text.len());

                // Verify output contains expected content
                if result.text.contains("[package]") || result.text.contains("name =") {
                    println!("✓ Output appears to be valid TOML");
                } else {
                    println!("✗ Unexpected output format");
                }
            }
            Err(e) => {
                println!("✗ Tool execution failed: {:?}", e);
            }
        }
    } else {
        println!("✗ read_file tool not found in registry");
    }
}
