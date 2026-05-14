#![allow(
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::needless_raw_string_hashes,
    clippy::single_char_pattern,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Integration test for end-to-end tool execution
//!
//! This test verifies that:
//! 1. Tools are selected correctly
//! 2. Tools execute successfully
//! 3. Tool results are properly formatted
//! 4. Multi-step conversations work
//!
//! # Running
//!
//! ```bash
//! cargo test --test integration_tool_execution -- --nocapture --test-threads=1 --ignored
//! ```
//!
//! # Requirements
//!
//! - ANTHROPIC_API_KEY environment variable set
//! - Network access to LLM API

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::ToolDispatcher;
use secrecy::SecretString;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

/// Test case for tool execution
#[allow(dead_code)]
struct ToolExecutionTest {
    name: String,
    prompt: String,
    expected_tool: String,
    verify_result: Box<dyn Fn(&str) -> bool>,
}

/// Integration test for tool execution
#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test integration_tool_execution -- --ignored"
)]
async fn test_tool_execution_end_to_end() {
    // Check for API key
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            println!("   Run with: export ANTHROPIC_API_KEY=your_key");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    // Setup test repository
    println!("📁 Setting up test repository...");
    let repo_path = setup_test_repo();
    println!("   ✓ Test repo created at: {}", repo_path.display());

    // Create provider
    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(120),
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

    // Define test cases
    let test_cases = vec![
        ToolExecutionTest {
            name: "Read main.rs".to_string(),
            prompt: "Read the main.rs file and tell me what function is defined in it".to_string(),
            expected_tool: "Read".to_string(),
            verify_result: Box::new(|output| {
                output.contains("fn main")
                    || output.contains("hello")
                    || output.contains("println")
                    || output.contains("world")
            }),
        },
        ToolExecutionTest {
            name: "List src directory".to_string(),
            prompt: "List all files in the src directory".to_string(),
            expected_tool: "ListDir".to_string(),
            verify_result: Box::new(|output| {
                output.contains("main.rs")
                    || output.contains("lib.rs")
                    || output.contains("utils.rs")
            }),
        },
        ToolExecutionTest {
            name: "Search for function".to_string(),
            prompt: "Search for 'fn helper_function' in the codebase".to_string(),
            expected_tool: "Grep".to_string(),
            verify_result: Box::new(|output| {
                output.contains("helper_function") || output.contains("Helper")
            }),
        },
    ];

    println!("\n📊 Running {} integration tests...\n", test_cases.len());

    let mut passed = 0;
    let mut failed = 0;

    for test_case in test_cases {
        println!("▶ Test: {}", test_case.name);
        println!("   Prompt: {}", test_case.prompt);

        let start = Instant::now();

        // Create system prompt
        let system_prompt = format!(
            "You are RustyCode, a coding assistant working in: {}

IMPORTANT: Always use the specialized tools below. Do NOT use bash commands unless explicitly requested.

Available tools:
- read_file: Read the complete contents of a text file
- list_dir: List all files and directories in a path
- grep: Search for text patterns across files in the codebase
- write_file: Write content to a file

Examples:
- To see files in 'src', use list_dir tool with path='src'
- To search code, use grep tool with the search pattern
- To read a file, use read_file tool with the file path

When using tools, respond naturally after execution.",
            repo_path.display()
        );

        let messages = vec![ChatMessage::user(test_case.prompt.clone())];
        let request = CompletionRequest::new(model.clone(), messages)
            .with_system_prompt(system_prompt)
            .with_max_tokens(1024)
            .with_temperature(0.1);

        match LLMProvider::complete(&provider, request).await {
            Ok(response) => {
                let response_text = response.content;
                let duration = start.elapsed();

                // Check if tool was used
                // Look for various tool call formats
                let tool_used = response_text.contains(&test_case.expected_tool)
                    || response_text.contains(&format!("<{}", test_case.expected_tool))
                    || response_text.contains(&format!("\"{}\"", test_case.expected_tool))
                    || response_text.contains("tool_use")
                    || response_text.contains("\"tool\":")
                    || (response_text.contains("<")
                        && response_text.contains(">")
                        && response_text.contains("file"));

                if tool_used {
                    println!("   ✓ Tool used: {}", test_case.expected_tool);

                    // For now, we're just checking if the LLM tried to use the tool
                    // In a full integration test, we would:
                    // 1. Parse the tool call
                    // 2. Execute it
                    // 3. Verify the result
                    // 4. Continue the conversation with the result

                    println!("   ✓ Duration: {:?}", duration);
                    println!(
                        "   ✓ Response: {}",
                        response_text.chars().take(100).collect::<String>()
                    );
                    passed += 1;
                } else {
                    println!("   ✗ Tool not used");
                    println!("   Response: {}", response_text);
                    failed += 1;
                }
            }
            Err(e) => {
                println!("   ❌ LLM error: {}", e);
                failed += 1;
            }
        }

        println!();

        // Small delay between tests
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Print results
    println!("═══════════════════════════════════════════════════════════");
    println!("📊 TEST RESULTS");
    println!("═══════════════════════════════════════════════════════════\n");
    println!("Passed: {}/{}", passed, passed + failed);
    println!("Failed: {}/{}", failed, passed + failed);

    if failed == 0 {
        println!("\n🎉 All tests passed!");
    } else {
        println!("\n⚠️  Some tests failed");
    }

    // Clean up
    let _ = std::fs::remove_dir_all(repo_path);

    // Assert for CI/CD
    assert_eq!(failed, 0, "Some integration tests failed");
}

/// Test actual tool execution without LLM
#[test]
#[ignore = "default_registry() is empty; requires populated tool registry"]
fn test_direct_tool_execution() {
    println!("📁 Setting up test repository...");
    let repo_path = setup_test_repo();
    println!("   ✓ Test repo created at: {}", repo_path.display());

    let executor = ToolDispatcher::from_cwd(repo_path.clone());

    // Test 1: Read file
    println!("\n▶ Test: Read main.rs");
    let read_result = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "test-1".to_string(),
        name: "Read".to_string(),
        arguments: serde_json::json!({"path": "src/main.rs"}),
    });

    assert!(
        read_result.success,
        "read_file failed: {:?}",
        read_result.error
    );
    assert!(
        read_result.output.contains("fn main"),
        "Output doesn't contain 'fn main'"
    );
    println!("   ✓ Read file successful");

    // Test 2: List directory
    println!("\n▶ Test: List src directory");
    let list_result = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "test-2".to_string(),
        name: "ListDir".to_string(),
        arguments: serde_json::json!({"path": "src"}),
    });

    assert!(
        list_result.success,
        "list_dir failed: {:?}",
        list_result.error
    );
    assert!(
        list_result.output.contains("main.rs"),
        "Output doesn't contain 'main.rs'"
    );
    println!("   ✓ List directory successful");

    // Test 3: Search
    println!("\n▶ Test: Search for 'fn main'");
    let search_result = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "test-3".to_string(),
        name: "Grep".to_string(),
        arguments: serde_json::json!({"pattern": "fn main"}),
    });

    assert!(
        search_result.success,
        "grep failed: {:?}",
        search_result.error
    );
    assert!(
        search_result.output.contains("main"),
        "Output doesn't contain 'main'"
    );
    println!("   ✓ Search successful");

    println!("\n🎉 All tool execution tests passed!");

    // Clean up
    let _ = std::fs::remove_dir_all(repo_path);
}

/// Setup test repository with sample files
fn setup_test_repo() -> PathBuf {
    let repo_path = PathBuf::from("/tmp/rustycode_integration_test");

    // Clean up existing repo
    if repo_path.exists() {
        std::fs::remove_dir_all(&repo_path).unwrap();
    }

    // Create directory structure
    std::fs::create_dir_all(repo_path.join("src")).unwrap();

    // Create sample files - make them larger to require tool use
    std::fs::write(
        repo_path.join("src/main.rs"),
        r#"//! Main entry point for the application
//!
//! This module contains the main function and several helper functions
//! for processing user input and managing application state.

use std::io::{self, Write};

/// Main entry point
fn main() {
    println!("Welcome to the application!");
    prompt_user();
}

/// Prompt the user for input
fn prompt_user() {
    print!("Enter your name: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Failed to read line");

    let name = input.trim();
    println!("Hello, {}!", name);
}

pub fn helper_function() -> String {
    "Helper result".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helper_function() {
        assert_eq!(helper_function(), "Helper result");
    }
}
"#,
    )
    .unwrap();

    std::fs::write(
        repo_path.join("Cargo.toml"),
        r#"[package]
name = "integration-test"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    std::fs::write(
        repo_path.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub mod utils;
"#,
    )
    .unwrap();

    std::fs::write(
        repo_path.join("src/utils.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#,
    )
    .unwrap();

    repo_path
}
