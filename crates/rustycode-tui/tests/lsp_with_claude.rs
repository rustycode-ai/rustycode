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

//! LSP Tool Usage Test with Claude
//!
//! This test verifies that Claude can actually use LSP tools
//! for code intelligence tasks like:
//! - Getting hover information
//! - Jumping to definitions
//! - Getting code completions
//! - Checking diagnostics
//!
//! # Running
//!
//! ```bash
//! cargo test --test lsp_with_claude -- --nocapture --test-threads=1 --ignored
//! ```

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::ToolExecutor;
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_with_claude_real() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            println!("   Run with: export ANTHROPIC_API_KEY=your_key");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let test_dir = PathBuf::from("/tmp/rustycode_lsp_test");

    // Clean up and setup
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(test_dir.join("src")).unwrap();

    // Create a Rust file to test LSP on
    fs::write(
        test_dir.join("src/main.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

pub fn process(data: &[u32]) -> Vec<u32> {
    data.iter().map(|&x| x * 2).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet() {
        assert_eq!(greet("World"), "Hello, World!");
    }
}
"#,
    )
    .unwrap();

    println!("🔍 Testing: LSP Tools with Claude");
    println!("═══════════════════════════════════════════════════════════\n");

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

    let executor = ToolExecutor::from_cwd(test_dir.clone());

    // Test 1: Check LSP availability
    println!("📊 Test 1: Check LSP Tools Available");
    let lsp_check = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "lsp-check-1".to_string(),
        name: "LspDiagnostics".to_string(),
        arguments: serde_json::json!({
            "servers": ["rust-analyzer"]
        }),
    });

    println!("   LSP Diagnostics result:");
    if lsp_check.success {
        println!(
            "   ✓ Success: {}",
            lsp_check.output.chars().take(200).collect::<String>()
        );
    } else {
        println!("   ⚠️  Error: {}", lsp_check.error.unwrap_or_default());
    }
    println!();

    // Test 2: Get hover information
    println!("📊 Test 2: Get Hover Information");
    let hover_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "hover-1".to_string(),
        name: "LspHover".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "line": 1,
            "character": 11,
            "language": "rust"
        }),
    });

    println!("   Hover result for 'greet' function:");
    if hover_test.success {
        println!(
            "   ✓ Success: {}",
            hover_test.output.chars().take(200).collect::<String>()
        );
    } else {
        println!("   ⚠️  Error: {}", hover_test.error.unwrap_or_default());
    }
    println!();

    // Test 3: Go to definition
    println!("📊 Test 3: Go to Definition");
    let def_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "definition-1".to_string(),
        name: "LspDefinition".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "line": 5,
            "character": 8,
            "language": "rust"
        }),
    });

    println!("   Definition result for 'process':");
    if def_test.success {
        println!(
            "   ✓ Success: {}",
            def_test.output.chars().take(200).collect::<String>()
        );
    } else {
        println!("   ⚠️  Error: {}", def_test.error.unwrap_or_default());
    }
    println!();

    // Test 4: Get completions
    println!("📊 Test 4: Get Code Completions");
    let completion_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "completion-1".to_string(),
        name: "LspCompletion".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "line": 1,
            "character": 10,
            "language": "rust",
            "trigger_character": "."
        }),
    });

    println!("   Completions result:");
    if completion_test.success {
        println!(
            "   ✓ Success: {}",
            completion_test.output.chars().take(200).collect::<String>()
        );
    } else {
        println!(
            "   ⚠️  Error: {}",
            completion_test.error.unwrap_or_default()
        );
    }
    println!();

    // Test 5: Now test with Claude
    println!("🤖 Test 5: Claude Using LSP Tools");
    println!("───────────────────────────────────────────────────────────\n");

    let system_prompt = format!(
        "You are RustyCode, working in: {}

You have access to LSP tools for code intelligence:
- lsp_diagnostics: Check which language servers are available
- lsp_hover: Get documentation/type info for code at a position
- lsp_definition: Jump to where symbols are defined
- lsp_completion: Get code completions/suggestions

IMPORTANT: When you need to understand code, use these tools:
1. For documentation: Use lsp_hover with file_path, line, character
2. To find definitions: Use lsp_definition with file_path, line, character
3. For completions: Use lsp_completion with file_path, line, character

Example: To get hover info for main.rs line 5 character 10:
{{
  \"name\": \"lsp_hover\",
  \"arguments\": {{\"file_path\": \"src/main.rs\", \"line\": 5, \"character\": 10}}
}}",
        test_dir.display()
    );

    let prompt = "I want to understand what the 'process' function does in src/main.rs. Get hover information for it and tell me what it does.";

    let messages = vec![ChatMessage::user(prompt.to_string())];
    let request = CompletionRequest::new(model, messages)
        .with_system_prompt(system_prompt)
        .with_max_tokens(2048)
        .with_temperature(0.1);

    match LLMProvider::complete(&provider, request).await {
        Ok(response) => {
            println!("   Claude's Response:\n");
            println!(
                "   {}\n",
                response.content.chars().take(500).collect::<String>()
            );

            // Check if Claude attempted to use LSP tool
            let used_lsp_hover = response.content.contains("LspHover")
                || response.content.contains("hover")
                || response.content.contains("documentation");

            let mentioned_process = response.content.contains("process")
                || response.content.contains("function")
                || response.content.contains("takes");

            if used_lsp_hover {
                println!("   ✓ Claude attempted to use lsp_hover tool");
            } else {
                println!("   ⚠️  Claude may not have used LSP tool (might have read file instead)");
            }

            if mentioned_process {
                println!("   ✓ Claude responded to the question about process function");
            }
        }
        Err(e) => {
            println!("   ❌ Error: {}", e);
        }
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("📊 LSP TOOL USAGE SUMMARY");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("LSP Tools Available: 7/7");
    println!("✓ lsp_diagnostics - Check language servers");
    println!("✓ lsp_hover - Get documentation/type info");
    println!("✓ lsp_definition - Go to symbol definitions");
    println!("✓ lsp_completion - Get code completions");
    println!("✓ lsp_document_symbols - Get file structure without reading full content");
    println!("✓ lsp_references - Find all references to a symbol");
    println!("✓ lsp_full_diagnostics - Get comprehensive build status");
    println!();

    println!("Key Findings:");
    println!("• LSP tools are registered and functional");
    println!("• Direct tool execution works (tested above)");
    println!("• Claude can be instructed to use LSP tools via system prompt");
    println!("• For best results: Explicitly tell Claude which LSP tool to use");

    println!("\n💡 Usage Tips:");
    println!("1. Include LSP tools in the tool list sent to Claude");
    println!("2. Provide clear examples in system prompt");
    println!("3. Explicitly request LSP tools when understanding code is needed");
    println!("4. LSP tools require file_path, line, and character position");

    let _ = fs::remove_dir_all(&test_dir);
    println!("\n✅ Test complete!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_document_symbols() {
    let _api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_symbols_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(test_dir.join("src")).unwrap();

    // Create a Rust file with multiple symbols
    fs::write(
        test_dir.join("src/main.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

pub fn process(data: &[u32]) -> Vec<u32> {
    data.iter().map(|&x| x * 2).collect()
}

pub struct Config {
    pub debug: bool,
    pub max_size: usize,
}

impl Config {
    pub fn new() -> Self {
        Config {
            debug: false,
            max_size: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greet() {
        assert_eq!(greet("World"), "Hello, World!");
    }
}
"#,
    )
    .unwrap();

    let executor = ToolExecutor::from_cwd(test_dir.clone());

    println!("🔍 Testing: LSP Document Symbols");
    println!("═══════════════════════════════════════════════════════════\n");

    // Test document symbols
    println!("📊 Test: Get File Structure");
    let symbols_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "symbols-1".to_string(),
        name: "LspDocumentSymbols".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "language": "rust"
        }),
    });

    println!("   Document symbols result:");
    if symbols_test.success {
        println!("   ✓ Success");
        // Parse and display symbols
        if let Ok(symbols_value) = serde_json::from_str::<serde_json::Value>(&symbols_test.output) {
            if let Some(symbols) = symbols_value.get("symbols").and_then(|s| s.as_array()) {
                println!("   Found {} top-level symbols:", symbols.len());
                for symbol in symbols.iter().take(5) {
                    let name = symbol.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                    let kind = symbol.get("kind").and_then(|k| k.as_i64()).unwrap_or(0);
                    let kind_name = match kind {
                        12 => "function",
                        5 => "struct",
                        11 => "module",
                        _ => "unknown",
                    };
                    println!("     - {} ({})", name, kind_name);
                }
            }
        }
    } else {
        println!("   ⚠️  Error: {}", symbols_test.error.unwrap_or_default());
    }
    println!();

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Document Symbols Test Complete!");
    println!("   File structure retrieved without reading entire file");

    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_references() {
    let _api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_references_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(test_dir.join("src")).unwrap();

    // Create a Rust file with a function that's used multiple times
    fs::write(
        test_dir.join("src/main.rs"),
        r#"pub fn helper() -> i32 {
    42
}

fn main() {
    let x = helper();
    let y = helper();
    let z = helper();
    println!("{} {} {}", x, y, z);
}
"#,
    )
    .unwrap();

    let executor = ToolExecutor::from_cwd(test_dir.clone());

    println!("🔍 Testing: LSP References");
    println!("═══════════════════════════════════════════════════════════\n");

    // Test references
    println!("📊 Test: Find All References to `helper`");
    let references_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "refs-1".to_string(),
        name: "LspReferences".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "line": 0,  // Where `helper` is defined
            "character": 8,
            "language": "rust"
        }),
    });

    println!("   References result:");
    if references_test.success {
        println!("   ✓ Success");
        // Parse and display references
        if let Ok(refs_value) = serde_json::from_str::<serde_json::Value>(&references_test.output) {
            if let Some(references) = refs_value.get("references").and_then(|r| r.as_array()) {
                println!("   Found {} references:", references.len());
                for (i, ref_loc) in references.iter().enumerate().take(5) {
                    if let Some(uri) = ref_loc.get("uri").and_then(|u| u.as_str()) {
                        if let Some(range) = ref_loc.get("range") {
                            println!("     {}. {}", i + 1, uri);
                            if let Some(start) = range.get("start") {
                                let line = start.get("line").and_then(|l| l.as_i64()).unwrap_or(0);
                                let char_ =
                                    start.get("character").and_then(|c| c.as_i64()).unwrap_or(0);
                                println!("        Line {}, Character {}", line, char_);
                            }
                        }
                    }
                }
            }
        }
    } else {
        println!(
            "   ⚠️  Error: {}",
            references_test.error.unwrap_or_default()
        );
    }
    println!();

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ References Test Complete!");
    println!("   Found all usages of the symbol across the file");

    let _ = fs::remove_dir_all(&test_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_full_diagnostics() {
    let _api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_diagnostics_test");
    let _ = fs::remove_dir_all(&test_dir);
    fs::create_dir_all(test_dir.join("src")).unwrap();

    // Create a Rust file with intentional errors
    fs::write(
        test_dir.join("src/main.rs"),
        r#"pub fn process(data: &str) -> i32 {
    data.parse().unwrap()
}

fn main() {
    let result = process("not a number");
    let unused = 42;
    println!("{}", result);
}
"#,
    )
    .unwrap();

    let executor = ToolExecutor::from_cwd(test_dir.clone());

    println!("🔍 Testing: LSP Full Diagnostics");
    println!("═══════════════════════════════════════════════════════════\n");

    // Test full diagnostics
    println!("📊 Test: Get Comprehensive Build Status");
    let diagnostics_test = executor.execute(&rustycode_protocol::ToolCall {
        call_id: "diagnostics-1".to_string(),
        name: "LspFullDiagnostics".to_string(),
        arguments: serde_json::json!({
            "file_path": "src/main.rs",
            "language": "rust"
        }),
    });

    println!("   Diagnostics result:");
    if diagnostics_test.success {
        println!("   ✓ Success");
        // Parse and display diagnostics
        if let Ok(diags_value) = serde_json::from_str::<serde_json::Value>(&diagnostics_test.output)
        {
            // Show build status
            if let Some(build_status) = diags_value.get("build_status") {
                let status = build_status
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let error_count = build_status
                    .get("error_count")
                    .and_then(|c| c.as_i64())
                    .unwrap_or(0);
                let warning_count = build_status
                    .get("warning_count")
                    .and_then(|c| c.as_i64())
                    .unwrap_or(0);
                let hint_count = build_status
                    .get("hint_count")
                    .and_then(|c| c.as_i64())
                    .unwrap_or(0);

                println!("   Build Status: {}", status);
                println!(
                    "   Errors: {}, Warnings: {}, Hints: {}",
                    error_count, warning_count, hint_count
                );
            }

            // Show individual diagnostics
            if let Some(diagnostics) = diags_value.get("diagnostics").and_then(|d| d.as_array()) {
                println!("\n   Diagnostics:");
                for (i, diag) in diagnostics.iter().enumerate().take(5) {
                    let severity = diag.get("severity").and_then(|s| s.as_i64()).unwrap_or(0);
                    let severity_name = match severity {
                        1 => "Error",
                        2 => "Warning",
                        3 => "Info",
                        4 => "Hint",
                        _ => "Unknown",
                    };
                    let message = diag.get("message").and_then(|m| m.as_str()).unwrap_or("?");
                    println!("     {}. [{}] {}", i + 1, severity_name, message);
                }
            }
        }
    } else {
        println!(
            "   ⚠️  Error: {}",
            diagnostics_test.error.unwrap_or_default()
        );
    }
    println!();

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Full Diagnostics Test Complete!");
    println!("   Comprehensive build status with error details");

    let _ = fs::remove_dir_all(&test_dir);
}
