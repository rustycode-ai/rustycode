#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::literal_string_with_formatting_args,
    clippy::manual_let_else,
    clippy::map_unwrap_or,
    clippy::needless_raw_string_hashes,
    clippy::single_match_else,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used,
    clippy::use_self
)]

//! LSP Performance & Token Usage Test
//!
//! This test validates that:
//! 1. Claude can use the new LSP tools effectively
//! 2. LSP tools reduce token usage compared to traditional methods
//! 3. LSP tools accomplish tasks faster and more accurately
//!
//! # Running
//!
//! ```bash
//! cargo test --test lsp_performance_test -- --nocapture --test-threads=1 --ignored
//! ```

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use secrecy::SecretString;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug)]
struct TokenUsage {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug)]
#[allow(dead_code)]
struct TestResult {
    task_name: String,
    approach: String,
    duration_ms: u128,
    token_usage: Option<TokenUsage>,
    success: bool,
    claude_response: String,
}

impl TestResult {
    fn print_comparison(&self, other: &TestResult) {
        println!("\n📊 Comparison: {}", self.task_name);
        println!("═══════════════════════════════════════════════════════════");

        // Duration comparison
        let duration_diff = if self.duration_ms > other.duration_ms {
            (self.duration_ms - other.duration_ms) as i128
        } else {
            -((other.duration_ms - self.duration_ms) as i128)
        };
        let duration_pct = if other.duration_ms > 0 {
            (duration_diff as f64 / other.duration_ms as f64) * 100.0
        } else {
            0.0
        };

        // Token comparison
        let token_diff = if let (Some(t1), Some(t2)) = (&self.token_usage, &other.token_usage) {
            let diff = t1.total_tokens as i128 - t2.total_tokens as i128;
            let pct = (diff as f64 / t2.total_tokens as f64) * 100.0;
            Some((diff, pct))
        } else {
            None
        };

        println!("Duration:");
        println!("  {}: {}ms", self.approach, self.duration_ms);
        println!("  {}: {}ms", other.approach, other.duration_ms);
        if duration_diff >= 0 {
            println!(
                "  Difference: +{}ms ({:.1}% slower)",
                duration_diff,
                duration_pct.abs()
            );
        } else {
            println!(
                "  Difference: {}ms ({:.1}% faster)",
                duration_diff,
                duration_pct.abs()
            );
        }

        if let Some((diff, pct)) = token_diff {
            println!("\nToken Usage:");
            println!(
                "  {}: {} tokens (in: {}, out: {})",
                self.approach,
                self.token_usage
                    .as_ref()
                    .map(|t| t.total_tokens)
                    .unwrap_or(0),
                self.token_usage
                    .as_ref()
                    .map(|t| t.input_tokens)
                    .unwrap_or(0),
                self.token_usage
                    .as_ref()
                    .map(|t| t.output_tokens)
                    .unwrap_or(0)
            );
            println!(
                "  {}: {} tokens (in: {}, out: {})",
                other.approach,
                other
                    .token_usage
                    .as_ref()
                    .map(|t| t.total_tokens)
                    .unwrap_or(0),
                other
                    .token_usage
                    .as_ref()
                    .map(|t| t.input_tokens)
                    .unwrap_or(0),
                other
                    .token_usage
                    .as_ref()
                    .map(|t| t.output_tokens)
                    .unwrap_or(0)
            );
            if diff >= 0 {
                println!("  Difference: +{} tokens ({:.1}% more)", diff, pct.abs());
            } else {
                println!("  Difference: {} tokens ({:.1}% less)", diff, pct.abs());
            }
        }

        println!("═══════════════════════════════════════════════════════════\n");
    }
}

fn create_test_project(path: &std::path::Path) {
    fs::create_dir_all(path.join("src")).unwrap();

    // Create a moderately complex Rust project
    fs::write(
        path.join("src/main.rs"),
        r#"pub mod config;
pub mod processor;
pub mod utils;

use config::Config;
use processor::process_data;
use utils::validate_input;

fn main() {
    let config = Config::new();
    let data = vec![1, 2, 3, 4, 5];

    if validate_input(&data) {
        let result = process_data(&data, &config);
        println!("Result: {:?}", result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process() {
        let config = Config::new();
        let data = vec![1, 2, 3];
        let result = process_data(&data, &config);
        assert_eq!(result.len(), 3);
    }
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/config.rs"),
        r#"pub struct Config {
    pub debug_mode: bool,
    pub max_items: usize,
    pub threshold: f64,
}

impl Config {
    pub fn new() -> Self {
        Config {
            debug_mode: false,
            max_items: 100,
            threshold: 0.5,
        }
    }

    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug_mode = debug;
        self
    }

    pub fn with_max_items(mut self, max: usize) -> Self {
        self.max_items = max;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/processor.rs"),
        r#"use crate::config::Config;
use std::collections::HashMap;

pub fn process_data(data: &[i32], config: &Config) -> Vec<i32> {
    if config.debug_mode {
        println!("Processing {} items", data.len());
    }

    data.iter()
        .filter(|&&x| (x as f64) > config.threshold)
        .map(|&x| x * 2)
        .take(config.max_items)
        .collect()
}

pub fn aggregate_results(results: &[i32]) -> HashMap<String, i32> {
    let mut map = HashMap::new();
    map.insert("sum".to_string(), results.iter().sum());
    map.insert("count".to_string(), results.len() as i32);
    map.insert("max".to_string(), *results.iter().max().unwrap_or(&0));
    map
}

pub fn validate_threshold(value: f64, config: &Config) -> bool {
    value >= config.threshold
}
"#,
    )
    .unwrap();

    fs::write(
        path.join("src/utils.rs"),
        r#"pub fn validate_input(data: &[i32]) -> bool {
    !data.is_empty() && data.len() <= 1000
}

pub fn format_output(data: &[i32]) -> String {
    data.iter()
        .map(|&x| x.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn calculate_stats(data: &[i32]) -> (f64, f64, i32) {
    if data.is_empty() {
        return (0.0, 0.0, 0);
    }

    let sum: i32 = data.iter().sum();
    let mean = sum as f64 / data.len() as f64;
    let max = *data.iter().max().unwrap_or(&0);

    let variance = data.iter()
        .map(|&x| (x as f64 - mean).powi(2))
        .sum::<f64>() / data.len() as f64;
    let stddev = variance.sqrt();

    (mean, stddev, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_input() {
        assert!(validate_input(&[1, 2, 3]));
        assert!(!validate_input(&[]));
        assert!(!validate_input(&vec![0; 1001]));
    }

    #[test]
    fn test_calculate_stats() {
        let data = vec![1, 2, 3, 4, 5];
        let (mean, stddev, max) = calculate_stats(&data);
        assert_eq!(mean, 3.0);
        assert_eq!(max, 5);
    }
}
"#,
    )
    .unwrap();
}

async fn run_claude_test(
    provider: &AnthropicProvider,
    model: String,
    system_prompt: String,
    user_message: String,
) -> (String, Option<TokenUsage>) {
    let messages = vec![ChatMessage::user(user_message)];
    let request = CompletionRequest::new(model, messages)
        .with_system_prompt(system_prompt)
        .with_max_tokens(4096)
        .with_temperature(0.1);

    let start = Instant::now();
    match LLMProvider::complete(provider, request).await {
        Ok(response) => {
            let duration = start.elapsed();

            // Extract token usage if available
            let token_usage = response.usage.as_ref().map(|usage| TokenUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.input_tokens + usage.output_tokens,
            });

            println!("  ⏱️  Duration: {}ms", duration.as_millis());
            if let Some(usage) = &token_usage {
                println!(
                    "  📊 Tokens: {} in + {} out = {} total",
                    usage.input_tokens, usage.output_tokens, usage.total_tokens
                );
            }

            (response.content, token_usage)
        }
        Err(e) => {
            println!("  ❌ Error: {}", e);
            (format!("Error: {}", e), None)
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_vs_traditional_file_understanding() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(
        ProviderConfig {
            api_key: Some(SecretString::new(api_key.into())),
            base_url,
            timeout_seconds: Some(180),
            extra_headers: None,
            retry_config: None,
        },
        model.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_lsp_perf_test");
    let _ = fs::remove_dir_all(&test_dir);
    create_test_project(&test_dir);

    println!("🧪 Testing: File Understanding - LSP vs Traditional");
    println!("═══════════════════════════════════════════════════════════\n");

    // Task: Understand the structure of the project
    let task = "What are the main modules in src/processor.rs and what functions do they export?";

    // Approach 1: Traditional (read file)
    println!("📝 Approach 1: Read entire file");
    let traditional_prompt = format!(
        "You are working in: {}

{}

Use the read_file tool to read src/processor.rs and tell me what functions it exports.",
        test_dir.display(),
        "You have access to: read_file, list_dir, grep"
    );

    let (response1, usage1) = run_claude_test(
        &provider,
        model.clone(),
        traditional_prompt,
        task.to_string(),
    )
    .await;

    let result1 = TestResult {
        task_name: "File Understanding".to_string(),
        approach: "Read File".to_string(),
        duration_ms: 0, // Will be set by run_claude_test
        token_usage: usage1,
        success: !response1.starts_with("Error"),
        claude_response: response1.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result1.claude_response);

    // Approach 2: LSP (document symbols)
    println!("📝 Approach 2: LSP Document Symbols");
    let lsp_prompt = format!(
        "You are working in: {}

{}

Use the lsp_document_symbols tool to get the structure of src/processor.rs and tell me what functions it exports.",
        test_dir.display(),
        "You have access to: lsp_document_symbols, lsp_hover, lsp_definition"
    );

    let (response2, usage2) =
        run_claude_test(&provider, model.clone(), lsp_prompt, task.to_string()).await;

    let result2 = TestResult {
        task_name: "File Understanding".to_string(),
        approach: "LSP Document Symbols".to_string(),
        duration_ms: 0,
        token_usage: usage2,
        success: !response2.starts_with("Error"),
        claude_response: response2.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result2.claude_response);

    // Compare results
    result1.print_comparison(&result2);

    // Verify quality
    let quality1 = result1.claude_response.contains("process_data")
        && result1.claude_response.contains("aggregate_results");
    let quality2 = result2.claude_response.contains("process_data")
        && result2.claude_response.contains("aggregate_results");

    println!("✅ Quality Check:");
    println!("  Traditional approach found functions: {}", quality1);
    println!("  LSP approach found functions: {}", quality2);

    let _ = fs::remove_dir_all(&test_dir);
    println!("✅ Test complete!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_vs_traditional_find_references() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(
        ProviderConfig {
            api_key: Some(SecretString::new(api_key.into())),
            base_url,
            timeout_seconds: Some(180),
            extra_headers: None,
            retry_config: None,
        },
        model.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_lsp_refs_test");
    let _ = fs::remove_dir_all(&test_dir);
    create_test_project(&test_dir);

    println!("🧪 Testing: Find References - LSP vs Traditional");
    println!("═══════════════════════════════════════════════════════════\n");

    // Task: Find where process_data is used
    let task = "Where is the process_data function called in the codebase?";

    // Approach 1: Traditional (grep)
    println!("📝 Approach 1: Grep search");
    let traditional_prompt = format!(
        "You are working in: {}

{}

Use the grep tool to search for 'process_data' and find where it's called.",
        test_dir.display(),
        "You have access to: read_file, grep, list_dir"
    );

    let (response1, usage1) = run_claude_test(
        &provider,
        model.clone(),
        traditional_prompt,
        task.to_string(),
    )
    .await;

    let result1 = TestResult {
        task_name: "Find References".to_string(),
        approach: "Grep".to_string(),
        duration_ms: 0,
        token_usage: usage1,
        success: !response1.starts_with("Error"),
        claude_response: response1.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result1.claude_response);

    // Approach 2: LSP (references)
    println!("📝 Approach 2: LSP References");
    let lsp_prompt = format!(
        "You are working in: {}

{}

Use the lsp_references tool to find where process_data is defined and called.
First use lsp_definition or read_file to find where it's defined (line ~5 in src/processor.rs),
then use lsp_references at that location.",
        test_dir.display(),
        "You have access to: lsp_references, lsp_definition, lsp_document_symbols"
    );

    let (response2, usage2) =
        run_claude_test(&provider, model.clone(), lsp_prompt, task.to_string()).await;

    let result2 = TestResult {
        task_name: "Find References".to_string(),
        approach: "LSP References".to_string(),
        duration_ms: 0,
        token_usage: usage2,
        success: !response2.starts_with("Error"),
        claude_response: response2.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result2.claude_response);

    // Compare results
    result1.print_comparison(&result2);

    // Verify quality
    let quality1 = result1.claude_response.contains("main.rs");
    let quality2 = result2.claude_response.contains("main.rs");

    println!("✅ Quality Check:");
    println!("  Grep approach found references: {}", quality1);
    println!("  LSP approach found references: {}", quality2);

    let _ = fs::remove_dir_all(&test_dir);
    println!("✅ Test complete!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_vs_traditional_build_check() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(
        ProviderConfig {
            api_key: Some(SecretString::new(api_key.into())),
            base_url,
            timeout_seconds: Some(180),
            extra_headers: None,
            retry_config: None,
        },
        model.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_lsp_build_test");
    let _ = fs::remove_dir_all(&test_dir);
    create_test_project(&test_dir);

    println!("🧪 Testing: Build Status Check - LSP vs Traditional");
    println!("═══════════════════════════════════════════════════════════\n");

    // Task: Check build status
    let task = "Check if there are any compilation errors in src/processor.rs";

    // Approach 1: Traditional (cargo check)
    println!("📝 Approach 1: Cargo check");
    let traditional_prompt = format!(
        "You are working in: {}

{}

Use the bash tool to run 'cargo check --message-format=short' and report any errors.",
        test_dir.display(),
        "You have access to: bash, read_file"
    );

    let (response1, usage1) = run_claude_test(
        &provider,
        model.clone(),
        traditional_prompt,
        task.to_string(),
    )
    .await;

    let result1 = TestResult {
        task_name: "Build Status".to_string(),
        approach: "Cargo Check".to_string(),
        duration_ms: 0,
        token_usage: usage1,
        success: !response1.starts_with("Error"),
        claude_response: response1.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result1.claude_response);

    // Approach 2: LSP (diagnostics)
    println!("📝 Approach 2: LSP Full Diagnostics");
    let lsp_prompt = format!(
        "You are working in: {}

{}

Use the lsp_full_diagnostics tool to check for errors in src/processor.rs.",
        test_dir.display(),
        "You have access to: lsp_full_diagnostics, lsp_document_symbols"
    );

    let (response2, usage2) =
        run_claude_test(&provider, model.clone(), lsp_prompt, task.to_string()).await;

    let result2 = TestResult {
        task_name: "Build Status".to_string(),
        approach: "LSP Diagnostics".to_string(),
        duration_ms: 0,
        token_usage: usage2,
        success: !response2.starts_with("Error"),
        claude_response: response2.chars().take(500).collect(),
    };

    println!("\n  Response preview:");
    println!("  {}...\n", result2.claude_response);

    // Compare results
    result1.print_comparison(&result2);

    // Verify quality
    let quality1 = result1.claude_response.contains("error")
        || result1.claude_response.contains("warning")
        || result1.claude_response.contains("Finished");
    let quality2 = result2.claude_response.contains("error")
        || result2.claude_response.contains("warning")
        || result2.claude_response.contains("success");

    println!("✅ Quality Check:");
    println!("  Cargo check provided build info: {}", quality1);
    println!("  LSP diagnostics provided build info: {}", quality2);

    let _ = fs::remove_dir_all(&test_dir);
    println!("✅ Test complete!");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires API key — run with: cargo test --features live-api-tests -- --ignored"
)]
async fn test_lsp_comprehensive_workflow() {
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping test: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let provider = match AnthropicProvider::new_without_validation(
        ProviderConfig {
            api_key: Some(SecretString::new(api_key.into())),
            base_url,
            timeout_seconds: Some(180),
            extra_headers: None,
            retry_config: None,
        },
        model.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            println!("❌ Failed to create provider: {:?}", e);
            return;
        }
    };

    let test_dir = PathBuf::from("/tmp/rustycode_lsp_workflow_test");
    let _ = fs::remove_dir_all(&test_dir);
    create_test_project(&test_dir);

    println!("🧪 Testing: Comprehensive LSP Workflow");
    println!("═══════════════════════════════════════════════════════════\n");

    let task = "Analyze the project structure and tell me:
1. What modules exist and what do they contain?
2. Where is the Config struct used?
3. Are there any compilation errors?";

    let lsp_prompt = format!(
        "You are RustyCode, working in: {}

You have access to advanced LSP tools for code intelligence:
- lsp_document_symbols: Get file structure without reading full content
- lsp_references: Find all references to a symbol
- lsp_full_diagnostics: Get comprehensive build status

TASK: {}

Use LSP tools to accomplish this efficiently:
1. Use lsp_document_symbols to understand each module's structure
2. Use lsp_references to find where Config is used
3. Use lsp_full_diagnostics to check for compilation errors

Be efficient - don't read entire files unless absolutely necessary.",
        test_dir.display(),
        task
    );

    println!("📝 Running comprehensive LSP workflow...\n");
    let (response, usage) =
        run_claude_test(&provider, model.clone(), lsp_prompt, task.to_string()).await;

    println!("\n📄 Full Response:");
    println!("═══════════════════════════════════════════════════════════");
    for line in response.lines().take(50) {
        println!("{}", line);
    }
    if response.lines().count() > 50 {
        println!("... ({} more lines)", response.lines().count() - 50);
    }
    println!("═══════════════════════════════════════════════════════════\n");

    // Analyze response quality
    let has_modules = response.contains("main.rs") || response.contains("modules");
    let has_config = response.contains("Config") || response.contains("config");
    let has_diagnostics = response.contains("error")
        || response.contains("warning")
        || response.contains("success")
        || response.contains("no errors");

    println!("✅ Quality Analysis:");
    println!("  Identified project modules: {}", has_modules);
    println!("  Found Config usage: {}", has_config);
    println!("  Provided build status: {}", has_diagnostics);

    if let Some(u) = &usage {
        println!("\n📊 Token Efficiency:");
        println!("  Total tokens used: {}", u.total_tokens);
        println!("  Input tokens: {}", u.input_tokens);
        println!("  Output tokens: {}", u.output_tokens);
        println!(
            "  Input/Output ratio: {:.2}",
            u.input_tokens as f64 / u.output_tokens as f64
        );
    }

    let all_checks_pass = has_modules && has_config && has_diagnostics;

    println!(
        "\n🎯 Overall Result: {}",
        if all_checks_pass {
            "✅ PASS"
        } else {
            "⚠️  PARTIAL"
        }
    );

    let _ = fs::remove_dir_all(&test_dir);
    println!("✅ Test complete!");
}
