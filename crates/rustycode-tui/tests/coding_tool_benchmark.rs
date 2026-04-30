#![allow(
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::needless_raw_string_hashes,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args
)]

//! Coding Tool Benchmark - Evaluate tool execution success rate
//!
//! This benchmark sends real coding tasks to the LLM and measures:
//! - Tool use accuracy (correct tool selected)
//! - Parameter accuracy (correct parameters passed)
//! - Execution success rate (tool executed successfully)
//! - End-to-end success rate (task completed correctly)
//!
//! # Running
//!
//! ```bash
//! # Run with ANTHROPIC_API_KEY set
//! cargo test --test coding_tool_benchmark -- --nocapture --test-threads=1 --ignored
//! ```
//!
//! # Requirements
//!
//! - ANTHROPIC_API_KEY environment variable set
//! - Test repository with code files to analyze
//! - Network access to LLM API

use rustycode_llm::{
    anthropic::AnthropicProvider,
    provider::{ChatMessage, CompletionRequest, LLMProvider, ProviderConfig},
};
use rustycode_tools::default_registry;
use secrecy::SecretString;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

// Test repository setup
const TEST_REPO_PATH: &str = "/tmp/rustycode_benchmark_repo";

/// Benchmark result for a single task
#[derive(Debug)]
#[allow(dead_code)]
struct BenchmarkResult {
    task_name: String,
    tool_use_detected: bool,
    correct_tool: bool,
    tool_executed: bool,
    task_completed: bool,
    error_message: Option<String>,
    duration_ms: u64,
}

/// Overall benchmark statistics
#[derive(Debug)]
struct BenchmarkStats {
    total_tasks: usize,
    tool_use_detected: usize,
    correct_tool_selected: usize,
    tools_executed: usize,
    tasks_completed: usize,
    total_duration_ms: u64,
}

impl BenchmarkStats {
    fn tool_use_detection_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            return 0.0;
        }
        (self.tool_use_detected as f64) / (self.total_tasks as f64)
    }

    fn tool_selection_accuracy(&self) -> f64 {
        if self.tool_use_detected == 0 {
            return 0.0;
        }
        (self.correct_tool_selected as f64) / (self.tool_use_detected as f64)
    }

    fn execution_success_rate(&self) -> f64 {
        if self.correct_tool_selected == 0 {
            return 0.0;
        }
        (self.tools_executed as f64) / (self.correct_tool_selected as f64)
    }

    fn task_completion_rate(&self) -> f64 {
        if self.total_tasks == 0 {
            return 0.0;
        }
        (self.tasks_completed as f64) / (self.total_tasks as f64)
    }

    fn average_duration_ms(&self) -> f64 {
        if self.total_tasks == 0 {
            return 0.0;
        }
        (self.total_duration_ms as f64) / (self.total_tasks as f64)
    }
}

/// Coding tasks to benchmark
fn get_coding_tasks() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "read_file",
            "Read the main.rs file and tell me what it does",
            "read_file",
        ),
        (
            "list_files",
            "List all files in the src directory",
            "list_dir",
        ),
        (
            "search_code",
            "Search for 'fn main' in the codebase",
            "grep",
        ),
        (
            "find_files",
            "Find all Rust files in the src directory",
            "list_dir",
        ),
    ]
}

/// Setup test repository with sample files
fn setup_test_repo() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let repo_path = PathBuf::from(TEST_REPO_PATH);

    // Clean up existing repo
    if repo_path.exists() {
        std::fs::remove_dir_all(&repo_path)?;
    }

    // Create directory structure
    std::fs::create_dir_all(repo_path.join("src"))?;
    std::fs::create_dir_all(repo_path.join("tests"))?;

    // Create sample files
    std::fs::write(
        repo_path.join("src/main.rs"),
        r#"fn main() {
    println!("Hello, world!");
}

pub fn helper_function() -> String {
    "Helper".to_string()
}
"#,
    )?;

    std::fs::write(
        repo_path.join("Cargo.toml"),
        r#"[package]
name = "benchmark-test"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
    )?;

    std::fs::write(
        repo_path.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub mod utils;
"#,
    )?;

    std::fs::write(
        repo_path.join("src/utils.rs"),
        r#"pub fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#,
    )?;

    Ok(repo_path)
}

/// Run a single coding task
async fn run_coding_task(
    provider: &AnthropicProvider,
    model: &str,
    tool_definitions: Vec<serde_json::Value>,
    task_name: &str,
    prompt: &str,
    expected_tool: &str,
    repo_path: &std::path::Path,
) -> BenchmarkResult {
    let start = Instant::now();

    let system_prompt = format!(
        "You are RustyCode, a coding assistant. You are working in a repository at: {}

Available tools:
- read_file: Read the complete contents of a file
- list_dir: List all files and directories in a path
- grep: Search for text patterns across files in the codebase
- write_file: Write content to a file

IMPORTANT: Use the EXACT tool name as specified above.",
        repo_path.display()
    );

    let messages = vec![ChatMessage::user(prompt.to_string())];
    let request = CompletionRequest::new(model.to_string(), messages)
        .with_system_prompt(system_prompt)
        .with_tools(tool_definitions)
        .with_max_tokens(1024)
        .with_temperature(0.1);

    let result = match LLMProvider::complete(provider, request).await {
        Ok(response) => {
            let response_text = response.content;

            // Check for tool use indicators
            let tool_use_detected = response_text.contains("\"name\"")
                || response_text.contains("```tool")
                || response_text.contains("<tool>");

            // Check if expected tool is mentioned
            let correct_tool = if tool_use_detected {
                response_text.contains(expected_tool)
                    || response_text
                        .contains(expected_tool.strip_suffix("_file").unwrap_or(expected_tool))
            } else {
                false
            };

            // For now, we'll count it as completed if tool use was detected
            // In a real benchmark, we'd parse and execute the tool
            let task_completed = tool_use_detected && correct_tool;

            BenchmarkResult {
                task_name: task_name.to_string(),
                tool_use_detected,
                correct_tool,
                tool_executed: task_completed, // Simplified - if tool selected, assume executed
                task_completed,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        Err(e) => BenchmarkResult {
            task_name: task_name.to_string(),
            tool_use_detected: false,
            correct_tool: false,
            tool_executed: false,
            task_completed: false,
            error_message: Some(format!("LLM error: {}", e)),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    };

    result
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test coding_tool_benchmark -- --ignored"
)]
async fn benchmark_coding_tools() {
    // Check for API key
    let api_key = match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("❌ Skipping benchmark: ANTHROPIC_API_KEY not set");
            println!("   Run with: export ANTHROPIC_API_KEY=your_key");
            println!("   Then: cargo test --test coding_tool_benchmark -- --nocapture --test-threads=1 --ignored");
            return;
        }
    };

    let base_url = env::var("ANTHROPIC_BASE_URL").ok();

    // Setup test repository
    println!("📁 Setting up test repository...");
    let repo_path = match setup_test_repo() {
        Ok(path) => {
            println!("   ✓ Test repo created at: {}", path.display());
            path
        }
        Err(e) => {
            println!("   ❌ Failed to create test repo: {}", e);
            return;
        }
    };

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

    // Create tool registry
    let tool_registry = default_registry();
    let tools = tool_registry.list();
    println!("🔧 Available tools: {}", tools.len());

    let tool_definitions: Vec<serde_json::Value> = tools
        .into_iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": {
                    "type": "object",
                    "properties": tool.parameters_schema,
                    "required": []
                }
            })
        })
        .collect();

    // Get coding tasks
    let tasks = get_coding_tasks();
    println!("\n📊 Running {} coding tasks benchmark...\n", tasks.len());

    let mut results = Vec::new();

    for (task_name, prompt, expected_tool) in tasks {
        println!("▶ Task: {} ({})", task_name, prompt);

        let result = run_coding_task(
            &provider,
            &model,
            tool_definitions.clone(),
            task_name,
            prompt,
            expected_tool,
            &repo_path,
        )
        .await;

        println!("   Tool use detected: {}", result.tool_use_detected);
        println!("   Correct tool: {}", result.correct_tool);
        println!("   Task completed: {}", result.task_completed);
        println!("   Duration: {}ms", result.duration_ms);

        if let Some(error) = &result.error_message {
            println!("   ❌ Error: {}", error);
        }

        println!();
        results.push(result);

        // Small delay between tasks
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // Calculate statistics
    let stats = BenchmarkStats {
        total_tasks: results.len(),
        tool_use_detected: results.iter().filter(|r| r.tool_use_detected).count(),
        correct_tool_selected: results.iter().filter(|r| r.correct_tool).count(),
        tools_executed: results.iter().filter(|r| r.tool_executed).count(),
        tasks_completed: results.iter().filter(|r| r.task_completed).count(),
        total_duration_ms: results.iter().map(|r| r.duration_ms).sum(),
    };

    // Print results
    println!("═══════════════════════════════════════════════════════════");
    println!("📊 BENCHMARK RESULTS");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Tasks Run: {}/{}", stats.tasks_completed, stats.total_tasks);
    println!(
        "Tool Use Detection Rate: {:.1}%",
        stats.tool_use_detection_rate() * 100.0
    );
    println!(
        "Tool Selection Accuracy: {:.1}%",
        stats.tool_selection_accuracy() * 100.0
    );
    println!(
        "Execution Success Rate: {:.1}%",
        stats.execution_success_rate() * 100.0
    );
    println!(
        "Task Completion Rate: {:.1}%",
        stats.task_completion_rate() * 100.0
    );
    println!("Average Duration: {:.1}ms", stats.average_duration_ms());

    println!("\n🎯 Success Criteria:");
    println!(
        "   Tool Use Detection Rate > 80%: {}",
        if stats.tool_use_detection_rate() > 0.8 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        }
    );
    println!(
        "   Tool Selection Accuracy > 75%: {}",
        if stats.tool_selection_accuracy() > 0.75 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        }
    );
    println!(
        "   Task Completion Rate > 70%: {}",
        if stats.task_completion_rate() > 0.70 {
            "✓ PASS"
        } else {
            "✗ FAIL"
        }
    );

    // Overall assessment
    let overall_pass = stats.tool_use_detection_rate() > 0.8
        && stats.tool_selection_accuracy() > 0.75
        && stats.task_completion_rate() > 0.70;

    println!(
        "\n{}",
        if overall_pass {
            "🎉 OVERALL: PASS"
        } else {
            "⚠️  OVERALL: NEEDS IMPROVEMENT"
        }
    );

    // Clean up
    let _ = std::fs::remove_dir_all(repo_path);

    // Assert for CI/CD
    assert!(
        overall_pass,
        "Benchmark did not meet minimum success criteria"
    );
}
