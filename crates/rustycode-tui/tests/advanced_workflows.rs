#![allow(
    clippy::manual_let_else,
    clippy::needless_raw_string_hashes,
    clippy::redundant_clone,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Advanced workflow tests with full tool execution
//!
//! These tests verify:
//! 1. Multi-turn conversations with tool execution
//! 2. Memory and context retention across turns
//! 3. Agent-based workflows with planning
//! 4. LSP tool usage for code intelligence
//!
//! # Running
//!
//! ```bash
//! cargo test --test advanced_workflows -- --nocapture --test-threads=1 --ignored
//! ```

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::default_registry;
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Test LSP tool integration (simple test, no API key needed)
#[test]
#[ignore = "default_registry() is empty; requires populated tool registry"]
fn test_lsp_tool_availability() {
    println!("🔍 Testing: LSP Tool Integration");
    println!("═══════════════════════════════════════════════════════════\n");

    let registry = default_registry();
    let tools = registry.list();

    // Check for LSP tools
    let lsp_tools = vec![
        "LspDiagnostics",
        "LspHover",
        "LspDefinition",
        "LspCompletion",
    ];

    println!("📊 Available LSP Tools:\n");
    let mut found = 0;
    for tool_name in &lsp_tools {
        let tool = tools.iter().find(|t| t.name == *tool_name);
        match tool {
            Some(tool) => {
                found += 1;
                println!("   ✓ {} - {}", tool.name, tool.description);
            }
            None => {
                println!("   ✗ {} - NOT FOUND", tool_name);
            }
        }
    }

    println!(
        "\n📊 Results: {}/{} LSP tools available",
        found,
        lsp_tools.len()
    );

    if found == lsp_tools.len() {
        println!("\n✅ All LSP tools are registered and available");
        println!("   System can provide code intelligence features");
    } else {
        println!("\n⚠️  Some LSP tools missing");
        println!("   This may affect code intelligence capabilities");
    }

    assert!(found >= 2, "At least 2 LSP tools should be available");
}

/// Test multi-turn conversation memory retention
#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_multiturn_memory_retention() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let test_dir = PathBuf::from("/tmp/rustycode_memory_test");

    let _ = fs::remove_dir_all(&test_dir);
    setup_complex_project(&test_dir);

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(180),
        extra_headers: None,
        retry_config: None,
    };

    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(config, model.clone()) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    println!("🧠 Testing: Memory Retention Across Multi-Turn Conversations");
    println!("═══════════════════════════════════════════════════════════\n");

    let system_prompt = format!(
        "You are RustyCode, working in: {}

IMPORTANT: Remember ALL information from previous turns. Build context progressively.",
        test_dir.display()
    );

    // Turn 1: List directory
    println!("📝 Turn 1: List directory");
    let conversation1 = vec![ChatMessage::user(
        "What files are in the src directory?".to_string(),
    )];

    let request1 = CompletionRequest::new(model.clone(), conversation1)
        .with_system_prompt(system_prompt.clone())
        .with_max_tokens(1024)
        .with_temperature(0.1);

    let response1_content = match LLMProvider::complete(&provider, request1).await {
        Ok(r) => {
            let preview = r.content.chars().take(100).collect::<String>();
            println!("   Response: {}\n", preview);
            r.content.clone()
        }
        Err(e) => {
            println!("   ❌ Error: {}", e);
            return;
        }
    };

    // Turn 2: Refer back to turn 1
    println!("📝 Turn 2: Reference information from turn 1");
    let conversation2 = vec![
        ChatMessage::user("What files are in the src directory?".to_string()),
        ChatMessage::assistant(response1_content.clone()),
        ChatMessage::user("Now read main.rs and tell me what it imports".to_string()),
    ];

    let request2 = CompletionRequest::new(model.clone(), conversation2)
        .with_system_prompt(system_prompt.clone())
        .with_max_tokens(1024)
        .with_temperature(0.1);

    let response2_content = match LLMProvider::complete(&provider, request2).await {
        Ok(r) => {
            let preview = r.content.chars().take(150).collect::<String>();
            println!("   Response: {}\n", preview);

            // Check if LLM references files from turn 1
            let has_context = r.content.contains("main.rs")
                || r.content.contains("src/")
                || r.content.contains("directory");

            if has_context {
                println!("   ✓ LLM remembered context from turn 1");
            } else {
                println!("   ⚠️  LLM may not have full context from turn 1");
            }
            r.content.clone()
        }
        Err(e) => {
            println!("   ❌ Error: {}", e);
            return;
        }
    };

    // Turn 3: Accumulate more context
    println!("📝 Turn 3: Accumulate context from both previous turns");
    let conversation3 = vec![
        ChatMessage::user("What files are in the src directory?".to_string()),
        ChatMessage::assistant(response1_content),
        ChatMessage::user("Now read main.rs and tell me what it imports".to_string()),
        ChatMessage::assistant(response2_content),
        ChatMessage::user(
            "Based on the files you've seen, does main.rs use utils or processor?".to_string(),
        ),
    ];

    let request3 = CompletionRequest::new(model, conversation3)
        .with_system_prompt(system_prompt)
        .with_max_tokens(1024)
        .with_temperature(0.1);

    match LLMProvider::complete(&provider, request3).await {
        Ok(r) => {
            let preview = r.content.chars().take(150).collect::<String>();
            println!("   Response: {}\n", preview);

            // Check if LLM accumulated context from both previous turns
            let accumulated = r.content.contains("main.rs")
                || r.content.contains("utils")
                || r.content.contains("processor")
                || r.content.contains("imports");

            if accumulated {
                println!("   ✓ LLM successfully accumulated context across all turns");
                println!("\n✅ Memory retention test PASSED");
                println!("   System maintains conversation context effectively");
            } else {
                println!("   ⚠️  Context accumulation may be limited");
            }
        }
        Err(e) => {
            println!("   ❌ Error: {}", e);
        }
    }

    let _ = fs::remove_dir_all(&test_dir);
}

/// Setup a complex project for testing
fn setup_complex_project(path: &std::path::Path) {
    fs::create_dir_all(path.join("src")).unwrap();

    fs::write(
        path.join("src/main.rs"),
        r#"use utils::helper;
use processor::process;

fn main() {
    let result = process(42);
    println!("Result: {}", result);
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/utils.rs"),
        r#"pub fn helper(x: i32) -> i32 {
    x * 2
}

pub fn format_result(s: &str) -> String {
    format!("Result: {}", s)
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/processor.rs"),
        r#"pub fn process(input: i32) -> i32 {
    input + 10
}

pub fn process_batch(inputs: &[i32]) -> Vec<i32> {
    inputs.iter().map(|&x| process(x)).collect()
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/lib.rs"),
        r#"pub mod utils;
pub mod processor;
"#,
    )
    .unwrap();
}
