#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::module_name_repetitions,
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unused_self
)]

//! TUI Benchmark Harness — LiveBench and TerminalBench-style task evaluation
//!
//! This integration test defines 10 benchmark tasks across four categories
//! (Coding, Reasoning, TerminalOps, InstructionFollowing) inspired by the
//! LiveBench and TerminalBench evaluation suites. Each task runs through the
//! headless agent loop (`run_headless_task`) and is scored via filesystem-based
//! verification. Results are emitted as CTRF-compliant JSON reports.
//!
//! # Running
//!
//! ```bash
//! # Unit tests (always run)
//! cargo test --test tui_benchmark_harness -- --test-threads=1
//!
//! # Live API tests (requires ANTHROPIC_API_KEY)
//! cargo test --features live-api-tests --test tui_benchmark_harness -- \
//!     --nocapture --test-threads=1 --ignored
//! ```
//!
//! # Task Categories
//!
//! | Category           | Tasks | Source        |
//! |--------------------|-------|---------------|
//! | Coding             | 1-3   | LiveBench     |
//! | Reasoning          | 4-5   | LiveBench     |
//! | Terminal Ops       | 6-8   | TerminalBench |
//! | Instruction Follow | 9-10  | LiveBench     |

use rustycode_core::headless::runner::run_headless_task;
use rustycode_llm::{anthropic::AnthropicProvider, provider::ProviderConfig};
use rustycode_tools::default_registry;
use secrecy::SecretString;
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;

// Types

/// Task category matching the benchmark domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum TaskCategory {
    Coding,
    Reasoning,
    TerminalOps,
    InstructionFollowing,
}

impl TaskCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Reasoning => "reasoning",
            Self::TerminalOps => "terminal_ops",
            Self::InstructionFollowing => "instruction_following",
        }
    }
}

/// A single benchmark task definition.
#[derive(Clone)]
struct BenchTask {
    name: String,
    category: TaskCategory,
    prompt: String,
    difficulty: String,
    source: String,
    expected_tools: Vec<String>,
    setup_fn: Option<fn(&Path) -> anyhow::Result<()>>,
}

/// Per-task result produced after running a benchmark task.
#[derive(Debug, Clone, Serialize)]
struct TaskBenchmarkResult {
    task_name: String,
    category: String,
    difficulty: String,
    source: String,
    reward: f64,
    duration_ms: u64,
    tool_calls_count: usize,
    expected_tools_used: Vec<String>,
    unexpected_tools_used: Vec<String>,
    error: Option<String>,
    llm_response_preview: String,
}

/// Aggregate benchmark report (CTRF-inspired structure).
#[derive(Debug, Serialize)]
struct BenchmarkReport {
    timestamp: String,
    model: String,
    agent: String,
    total_tasks: usize,
    passed: usize,
    failed: usize,
    average_reward: f64,
    average_duration_ms: f64,
    results: Vec<TaskBenchmarkResult>,
    summary_by_category: std::collections::HashMap<String, CategorySummary>,
}

/// Per-category summary for the report.
#[derive(Debug, Serialize)]
struct CategorySummary {
    total: usize,
    passed: usize,
    average_reward: f64,
}

// Task Definitions

/// Return all 10 benchmark task definitions.
fn get_all_bench_tasks() -> Vec<BenchTask> {
    vec![
        // -- Coding Tasks (LiveBench-inspired) --
        BenchTask {
            name: "create_hello_world".into(),
            category: TaskCategory::Coding,
            prompt: "Create a file called hello.py with a Python function that prints \
                'Hello, World!' and runs it. Verify the output."
                .into(),
            difficulty: "easy".into(),
            source: "livebench".into(),
            expected_tools: vec!["write_file".into(), "bash".into()],
            setup_fn: None,
        },
        BenchTask {
            name: "fix_bug_reverse_string".into(),
            category: TaskCategory::Coding,
            prompt: "The file src/utils.rs contains a reverse_string function with a bug. \
                Read it, identify the bug, fix it, and verify with a test."
                .into(),
            difficulty: "medium".into(),
            source: "livebench".into(),
            expected_tools: vec!["read_file".into(), "write_file".into(), "bash".into()],
            setup_fn: Some(setup_buggy_reverse_string),
        },
        BenchTask {
            name: "implement_fibonacci".into(),
            category: TaskCategory::Coding,
            prompt: "Create a Rust library crate with a fibonacci function that returns \
                the nth fibonacci number. Include unit tests."
                .into(),
            difficulty: "medium".into(),
            source: "livebench".into(),
            expected_tools: vec!["write_file".into(), "bash".into()],
            setup_fn: None,
        },
        // -- Reasoning Tasks (LiveBench-inspired) --
        BenchTask {
            name: "analyze_log_file".into(),
            category: TaskCategory::Reasoning,
            prompt: "Read the file server.log and determine: \
                (1) How many ERROR entries exist, \
                (2) Which service generated the most errors, \
                (3) What is the most common error message. \
                Write the answers to analysis.json."
                .into(),
            difficulty: "medium".into(),
            source: "livebench".into(),
            expected_tools: vec!["read_file".into(), "write_file".into()],
            setup_fn: Some(setup_log_file),
        },
        BenchTask {
            name: "sort_and_filter".into(),
            category: TaskCategory::Reasoning,
            prompt: "Read data.json which contains an array of objects with 'name', 'age', \
                and 'score' fields. Filter to people with score > 80, sort by age descending, \
                and write the top 3 to results.json."
                .into(),
            difficulty: "medium".into(),
            source: "livebench".into(),
            expected_tools: vec!["read_file".into(), "write_file".into()],
            setup_fn: Some(setup_data_json),
        },
        // -- Terminal Operation Tasks (TerminalBench-inspired) --
        BenchTask {
            name: "find_and_count_files".into(),
            category: TaskCategory::TerminalOps,
            prompt: "Find all .rs files in the workspace, count the total number of \
                'fn ' declarations across all of them, and write the count to function_count.txt"
                .into(),
            difficulty: "easy".into(),
            source: "terminalbench".into(),
            expected_tools: vec!["bash".into(), "write_file".into()],
            setup_fn: Some(setup_rust_project_structure),
        },
        BenchTask {
            name: "git_init_and_commit".into(),
            category: TaskCategory::TerminalOps,
            prompt: "Initialize a git repository, add all files, and create an initial commit \
                with message 'Initial commit'. Verify the commit was created."
                .into(),
            difficulty: "easy".into(),
            source: "terminalbench".into(),
            expected_tools: vec!["bash".into()],
            setup_fn: Some(setup_simple_repo),
        },
        BenchTask {
            name: "setup_project_structure".into(),
            category: TaskCategory::TerminalOps,
            prompt: "Create a Python project with this structure: src/__init__.py, \
                src/main.py (with a main function), tests/test_main.py (with a basic test), \
                requirements.txt (with pytest), and run the tests."
                .into(),
            difficulty: "medium".into(),
            source: "terminalbench".into(),
            expected_tools: vec!["write_file".into(), "bash".into()],
            setup_fn: None,
        },
        // -- Instruction Following Tasks (LiveBench-inspired) --
        BenchTask {
            name: "strict_output_format".into(),
            category: TaskCategory::InstructionFollowing,
            prompt: "Read config.toml and extract the 'database' section. Write the extracted \
                values to output.json with EXACTLY this format: \
                {\"host\": string, \"port\": number, \"name\": string}. \
                Do NOT include any other fields."
                .into(),
            difficulty: "easy".into(),
            source: "livebench".into(),
            expected_tools: vec!["read_file".into(), "write_file".into()],
            setup_fn: Some(setup_config_toml),
        },
        BenchTask {
            name: "multi_step_transform".into(),
            category: TaskCategory::InstructionFollowing,
            prompt: "Step 1: Read users.csv. \
                Step 2: Filter to active users only (status=active). \
                Step 3: Convert to JSON format. \
                Step 4: Sort by join_date. \
                Step 5: Write to active_users.json. \
                Complete ALL steps."
                .into(),
            difficulty: "hard".into(),
            source: "livebench".into(),
            expected_tools: vec!["read_file".into(), "write_file".into()],
            setup_fn: Some(setup_users_csv),
        },
    ]
}

// Setup Functions

fn setup_buggy_reverse_string(workspace: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(workspace.join("src"))?;
    std::fs::write(
        workspace.join("src").join("utils.rs"),
        r#"/// Reverses a string. Contains a bug — the characters are not reversed.
pub fn reverse_string(s: &str) -> String {
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reverse() {
        assert_eq!(reverse_string("hello"), "olleh");
        assert_eq!(reverse_string("rust"), "tsur");
        assert_eq!(reverse_string(""), "");
    }
}
"#,
    )?;
    // Minimal Cargo.toml so `cargo test` can run
    std::fs::write(
        workspace.join("Cargo.toml"),
        r#"[package]
name = "bench-fix-bug"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
    )?;
    Ok(())
}

fn setup_log_file(workspace: &Path) -> anyhow::Result<()> {
    std::fs::write(
        workspace.join("server.log"),
        r#"2025-01-15 08:01:12 [INFO] auth-service: user login success
2025-01-15 08:02:30 [ERROR] payment-service: connection timeout to gateway
2025-01-15 08:03:45 [INFO] auth-service: token refreshed
2025-01-15 08:04:01 [ERROR] payment-service: connection timeout to gateway
2025-01-15 08:05:22 [WARN] notification-service: queue length 150
2025-01-15 08:06:10 [ERROR] auth-service: invalid token format
2025-01-15 08:07:33 [INFO] payment-service: payment processed id=9921
2025-01-15 08:08:01 [ERROR] payment-service: connection timeout to gateway
2025-01-15 08:09:44 [INFO] notification-service: email sent
2025-01-15 08:10:02 [ERROR] auth-service: invalid token format
2025-01-15 08:11:55 [INFO] payment-service: refund processed id=8832
2025-01-15 08:12:10 [ERROR] payment-service: connection timeout to gateway
2025-01-15 08:13:01 [INFO] auth-service: user logout
2025-01-15 08:14:22 [WARN] payment-service: slow response 3.2s
2025-01-15 08:15:30 [ERROR] notification-service: email delivery failed
2025-01-15 08:16:11 [INFO] auth-service: user login success
2025-01-15 08:17:45 [ERROR] payment-service: connection timeout to gateway
2025-01-15 08:18:03 [INFO] notification-service: push sent
2025-01-15 08:19:20 [ERROR] auth-service: invalid token format
2025-01-15 08:20:05 [INFO] payment-service: payment processed id=1142
"#,
    )?;
    Ok(())
}

fn setup_data_json(workspace: &Path) -> anyhow::Result<()> {
    let data = serde_json::json!([
        {"name": "Alice",   "age": 30, "score": 95},
        {"name": "Bob",     "age": 25, "score": 72},
        {"name": "Carol",   "age": 35, "score": 88},
        {"name": "Dave",    "age": 28, "score": 65},
        {"name": "Eve",     "age": 40, "score": 91},
        {"name": "Frank",   "age": 22, "score": 55},
        {"name": "Grace",   "age": 33, "score": 83},
        {"name": "Hank",    "age": 27, "score": 78},
        {"name": "Ivy",     "age": 45, "score": 62},
        {"name": "Jack",    "age": 31, "score": 97}
    ]);
    let json = serde_json::to_string_pretty(&data)?;
    std::fs::write(workspace.join("data.json"), json)?;
    Ok(())
}

fn setup_rust_project_structure(workspace: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(workspace.join("src"))?;
    std::fs::write(
        workspace.join("src").join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n\nfn helper() -> i32 { 42 }\n",
    )?;
    std::fs::write(
        workspace.join("src").join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn mul(a: i32, b: i32) -> i32 { a * b }\n",
    )?;
    std::fs::write(
        workspace.join("src").join("utils.rs"),
        "pub fn greet(name: &str) -> String { format!(\"Hello, {}!\", name) }\n",
    )?;
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"bench-rs\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    Ok(())
}

fn setup_simple_repo(workspace: &Path) -> anyhow::Result<()> {
    std::fs::write(workspace.join("README.md"), "# test repo\n")?;
    std::fs::write(workspace.join("hello.txt"), "hello world\n")?;
    Ok(())
}

fn setup_config_toml(workspace: &Path) -> anyhow::Result<()> {
    std::fs::write(
        workspace.join("config.toml"),
        r#"[server]
host = "0.0.0.0"
port = 8080

[database]
host = "db.example.com"
port = 5432
name = "myapp_dev"
pool_size = 10

[logging]
level = "info"
format = "json"
"#,
    )?;
    Ok(())
}

fn setup_users_csv(workspace: &Path) -> anyhow::Result<()> {
    std::fs::write(
        workspace.join("users.csv"),
        "name,email,status,join_date\n\
        Alice,alice@example.com,active,2023-01-15\n\
        Bob,bob@example.com,inactive,2023-03-20\n\
        Carol,carol@example.com,active,2022-11-08\n\
        Dave,dave@example.com,active,2024-01-01\n\
        Eve,eve@example.com,inactive,2023-06-30\n\
        Frank,frank@example.com,active,2022-08-22\n\
        Grace,grace@example.com,active,2023-09-10\n\
        Hank,hank@example.com,inactive,2024-02-14\n",
    )?;
    Ok(())
}

// Verification Logic

/// Verify a completed task by inspecting the workspace filesystem state.
/// Returns a reward score between 0.0 (total failure) and 1.0 (full success).
fn verify_task(task: &BenchTask, workspace: &Path) -> f64 {
    match task.name.as_str() {
        "create_hello_world" => verify_create_hello_world(workspace),
        "fix_bug_reverse_string" => verify_fix_bug_reverse_string(workspace),
        "implement_fibonacci" => verify_implement_fibonacci(workspace),
        "analyze_log_file" => verify_analyze_log_file(workspace),
        "sort_and_filter" => verify_sort_and_filter(workspace),
        "find_and_count_files" => verify_find_and_count_files(workspace),
        "git_init_and_commit" => verify_git_init_and_commit(workspace),
        "setup_project_structure" => verify_setup_project_structure(workspace),
        "strict_output_format" => verify_strict_output_format(workspace),
        "multi_step_transform" => verify_multi_step_transform(workspace),
        _ => 0.0,
    }
}

fn verify_create_hello_world(workspace: &Path) -> f64 {
    let hello = workspace.join("hello.py");
    if !hello.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&hello) else {
        return 0.1;
    };
    let mut score: f64 = 0.3; // file exists
    if content.contains("Hello, World!") || content.contains("hello, world!") {
        score += 0.4; // correct content
    }
    if content.contains("def ") || content.contains("print") {
        score += 0.3; // has function/print
    }
    score.min(1.0)
}

fn verify_fix_bug_reverse_string(workspace: &Path) -> f64 {
    let utils = workspace.join("src").join("utils.rs");
    if !utils.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&utils) else {
        return 0.1;
    };
    let mut score: f64 = 0.2;
    // Check that the function no longer just returns s.to_string()
    if content.contains("chars()") || content.contains("rev()") || content.contains("reverse") {
        score += 0.4; // reversal logic present
    }
    if !content.contains("s.to_string()") || content.contains("chars") {
        score += 0.2; // bug is fixed (no bare s.to_string() or has real logic)
    }
    if content.contains("#[test]") {
        score += 0.2; // test still present
    }
    score.min(1.0)
}

fn verify_implement_fibonacci(workspace: &Path) -> f64 {
    // Check for Cargo.toml (Rust crate) and a lib.rs or src/lib.rs with fibonacci
    let has_cargo = workspace.join("Cargo.toml").exists();
    let lib_paths = [
        workspace.join("src").join("lib.rs"),
        workspace.join("lib.rs"),
    ];
    let lib_content = lib_paths
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok());

    let mut score: f64 = 0.0;
    if has_cargo {
        score += 0.2;
    }
    let Some(content) = lib_content else {
        return score;
    };
    if content.contains("fibonacci") || content.contains("fib") {
        score += 0.4;
    }
    if content.contains("#[test]") {
        score += 0.4;
    }
    score.min(1.0)
}

fn verify_analyze_log_file(workspace: &Path) -> f64 {
    let analysis = workspace.join("analysis.json");
    if !analysis.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&analysis) else {
        return 0.1;
    };
    let mut score: f64 = 0.2; // file exists
                              // Try parsing as JSON
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
        score += 0.3; // valid JSON
        if let Some(arr) = val.as_array() {
            if arr.len() <= 3 {
                score += 0.2; // top 3
            }
            // All should have score > 80
            let all_high_score = arr.iter().all(|item| {
                item.get("score")
                    .and_then(serde_json::Value::as_i64)
                    .is_some_and(|s| s > 80)
            });
            if all_high_score {
                score += 0.3;
            }
        }
    }
    score.min(1.0)
}

fn verify_sort_and_filter(workspace: &Path) -> f64 {
    let output = workspace.join("results.json");
    if !output.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&output) else {
        return 0.1;
    };
    let mut score: f64 = 0.2;
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
        score += 0.2; // valid JSON
        if let Some(arr) = val.as_array() {
            // Should have 3 results (top 3 by age from score > 80)
            if !arr.is_empty() {
                score += 0.1;
            }
            if arr.len() == 3 {
                score += 0.2;
            }
            // Check sorted by age descending: Eve(40), Grace(33), Alice(30)
            let ages: Vec<Option<i64>> = arr
                .iter()
                .map(|item| item.get("age").and_then(serde_json::Value::as_i64))
                .collect();
            if ages.len() >= 2 {
                let descending = ages
                    .windows(2)
                    .all(|w| w[0].unwrap_or(0) >= w[1].unwrap_or(0));
                if descending {
                    score += 0.2;
                }
            }
            // All should have score > 80
            let all_above_80 = arr.iter().all(|item| {
                item.get("score")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0)
                    > 80
            });
            if all_above_80 {
                score += 0.1;
            }
        }
    }
    score.min(1.0)
}

fn verify_find_and_count_files(workspace: &Path) -> f64 {
    let count_file = workspace.join("function_count.txt");
    if !count_file.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&count_file) else {
        return 0.1;
    };
    let mut score: f64 = 0.3;
    // The workspace has: fn main, fn helper, fn add, fn mul, fn greet = 5 fn declarations
    // Accept a reasonable number (3-7)
    if let Ok(num) = content.trim().parse::<u32>() {
        if (3..=10).contains(&num) {
            score += 0.5;
        }
        if num == 5 {
            score += 0.2;
        }
    }
    score.min(1.0)
}

fn verify_git_init_and_commit(workspace: &Path) -> f64 {
    let git_dir = workspace.join(".git");
    if !git_dir.exists() {
        return 0.0;
    }
    let mut score: f64 = 0.3;
    // Check git log for the initial commit
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(workspace)
        .output();
    if let Ok(out) = output {
        let log = String::from_utf8_lossy(&out.stdout);
        if log.contains("Initial commit") || log.contains("initial commit") {
            score += 0.5;
        }
        if !log.trim().is_empty() {
            score += 0.2; // at least some commit exists
        }
    }
    score.min(1.0)
}

fn verify_setup_project_structure(workspace: &Path) -> f64 {
    let mut score: f64 = 0.0;
    let paths = [
        "src/__init__.py",
        "src/main.py",
        "tests/test_main.py",
        "requirements.txt",
    ];
    let mut existing = 0;
    for p in &paths {
        if workspace.join(p).exists() {
            existing += 1;
        }
    }
    score += f64::from(existing) / (paths.len() as f64) * 0.6;
    // Check requirements.txt mentions pytest
    if let Ok(req) = std::fs::read_to_string(workspace.join("requirements.txt")) {
        if req.contains("pytest") {
            score += 0.2;
        }
    }
    // Check main.py has a function
    if let Ok(main) = std::fs::read_to_string(workspace.join("src").join("main.py")) {
        if main.contains("def ") {
            score += 0.2;
        }
    }
    score.min(1.0)
}

fn verify_strict_output_format(workspace: &Path) -> f64 {
    let output = workspace.join("output.json");
    if !output.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&output) else {
        return 0.1;
    };
    let mut score: f64 = 0.2;
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
        score += 0.3; // valid JSON
                      // Must have exactly host, port, name
        let has_host = val.get("host").is_some();
        let has_port = val.get("port").is_some();
        let has_name = val.get("name").is_some();
        let field_count = [has_host, has_port, has_name]
            .iter()
            .filter(|&&b| b)
            .count();
        score += (field_count as f64) / 3.0 * 0.3;
        // Must NOT have extra fields
        if let Some(obj) = val.as_object() {
            if obj.len() == 3 {
                score += 0.2;
            }
        }
    }
    score.min(1.0)
}

fn verify_multi_step_transform(workspace: &Path) -> f64 {
    let output = workspace.join("active_users.json");
    if !output.exists() {
        return 0.0;
    }
    let Ok(content) = std::fs::read_to_string(&output) else {
        return 0.1;
    };
    let mut score: f64 = 0.2;
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
        score += 0.3; // valid JSON
        if let Some(arr) = val.as_array() {
            // Should have only active users (Alice, Carol, Dave, Frank, Grace = 5)
            if !arr.is_empty() {
                score += 0.2;
            }
            // Check no inactive users (Bob, Eve, Hank)
            let no_inactive = arr.iter().all(|item| {
                let email = item.get("email").and_then(|e| e.as_str()).unwrap_or("");
                !email.starts_with("bob") && !email.starts_with("eve") && !email.starts_with("hank")
            });
            if no_inactive {
                score += 0.3;
            }
        }
    }
    score.min(1.0)
}

// Runner Helpers

/// Build the tool schema array from the registry for `run_headless_task`.
fn build_tools_schema(tool_registry: &rustycode_tools::ToolRegistry) -> Vec<serde_json::Value> {
    tool_registry
        .list()
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
        .collect()
}

/// Create the LLM provider from environment variables.
fn create_provider() -> anyhow::Result<(AnthropicProvider, String)> {
    let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let base_url = env::var("ANTHROPIC_BASE_URL").ok();
    let model = env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

    let config = ProviderConfig {
        api_key: Some(SecretString::new(api_key.into())),
        base_url,
        timeout_seconds: Some(300),
        extra_headers: None,
        retry_config: None,
    };
    let provider = AnthropicProvider::new_without_validation(config, model.clone())?;
    Ok((provider, model))
}

/// Create a temporary workspace, optionally running the task's setup function.
fn create_workspace(task: &BenchTask) -> anyhow::Result<PathBuf> {
    let dir = tempfile::tempdir()?;
    let workspace = dir.path().to_path_buf();
    // Keep the tempdir alive by leaking it (fine for tests)
    std::mem::forget(dir);
    if let Some(setup) = task.setup_fn {
        setup(&workspace).with_context(|| format!("setup failed for {}", task.name))?;
    }
    Ok(workspace)
}

/// Run a single benchmark task through the headless agent loop.
async fn run_bench_task(
    provider: &AnthropicProvider,
    model: &str,
    tool_registry: &rustycode_tools::ToolRegistry,
    tools_schema: &[serde_json::Value],
    task: &BenchTask,
) -> TaskBenchmarkResult {
    let workspace = match create_workspace(task) {
        Ok(w) => w,
        Err(e) => {
            return TaskBenchmarkResult {
                task_name: task.name.clone(),
                category: task.category.as_str().to_string(),
                difficulty: task.difficulty.clone(),
                source: task.source.clone(),
                reward: 0.0,
                duration_ms: 0,
                tool_calls_count: 0,
                expected_tools_used: vec![],
                unexpected_tools_used: vec![],
                error: Some(format!("workspace setup: {e:#}")),
                llm_response_preview: String::new(),
            };
        }
    };

    let start = Instant::now();
    let result = run_headless_task(
        provider,
        model,
        tools_schema,
        &task.prompt,
        &workspace,
        tool_registry,
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(response_text) => {
            let reward = verify_task(task, &workspace);
            let preview = response_text.chars().take(200).collect();

            TaskBenchmarkResult {
                task_name: task.name.clone(),
                category: task.category.as_str().to_string(),
                difficulty: task.difficulty.clone(),
                source: task.source.clone(),
                reward,
                duration_ms,
                tool_calls_count: 0, // not tracked by run_headless_task
                expected_tools_used: vec![],
                unexpected_tools_used: vec![],
                error: None,
                llm_response_preview: preview,
            }
        }
        Err(e) => TaskBenchmarkResult {
            task_name: task.name.clone(),
            category: task.category.as_str().to_string(),
            difficulty: task.difficulty.clone(),
            source: task.source.clone(),
            reward: 0.0,
            duration_ms,
            tool_calls_count: 0,
            expected_tools_used: vec![],
            unexpected_tools_used: vec![],
            error: Some(format!("{e:#}")),
            llm_response_preview: String::new(),
        },
    }
}

// Report Generation

/// Build the aggregate report from individual results.
fn build_report(model: &str, results: Vec<TaskBenchmarkResult>) -> BenchmarkReport {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let total_tasks = results.len();
    let passed = results.iter().filter(|r| r.reward >= 0.6).count();
    let failed = total_tasks - passed;
    let average_reward = if total_tasks > 0 {
        results.iter().map(|r| r.reward).sum::<f64>() / (total_tasks as f64)
    } else {
        0.0
    };
    let average_duration_ms = if total_tasks > 0 {
        results.iter().map(|r| r.duration_ms as f64).sum::<f64>() / (total_tasks as f64)
    } else {
        0.0
    };

    let mut summary_by_category = std::collections::HashMap::new();
    for result in &results {
        let entry = summary_by_category
            .entry(result.category.clone())
            .or_insert((0usize, 0usize, 0.0f64));
        entry.0 += 1;
        if result.reward >= 0.6 {
            entry.1 += 1;
        }
        entry.2 += result.reward;
    }

    let summary_by_category: std::collections::HashMap<String, CategorySummary> =
        summary_by_category
            .into_iter()
            .map(|(cat, (total, passed, reward_sum))| {
                (
                    cat,
                    CategorySummary {
                        total,
                        passed,
                        average_reward: if total > 0 {
                            reward_sum / (total as f64)
                        } else {
                            0.0
                        },
                    },
                )
            })
            .collect();

    BenchmarkReport {
        timestamp,
        model: model.to_string(),
        agent: "rustycode-headless".to_string(),
        total_tasks,
        passed,
        failed,
        average_reward,
        average_duration_ms,
        results,
        summary_by_category,
    }
}

/// Write the CTRF-compliant JSON report to /tmp.
fn write_report(report: &BenchmarkReport) -> anyhow::Result<PathBuf> {
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let path = PathBuf::from(format!("/tmp/rustycode_benchmark_{ts}.json"));
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

/// Print a formatted results table to stdout.
fn print_results_table(results: &[TaskBenchmarkResult]) {
    println!("\n{}", "=".repeat(70));
    println!("  BENCHMARK RESULTS");
    println!("{}\n", "=".repeat(70));
    println!(
        "{:<30} {:<12} {:<10} {:<10} {:<10}",
        "TASK", "CATEGORY", "DIFF", "REWARD", "TIME(ms)"
    );
    println!("{}", "-".repeat(72));
    for r in results {
        let status = if r.reward >= 0.6 { "PASS" } else { "FAIL" };
        println!(
            "{:<30} {:<12} {:<10} {:<10.2} {:<10} {}",
            r.task_name, r.category, r.difficulty, r.reward, r.duration_ms, status
        );
        if let Some(ref err) = r.error {
            println!("  ERROR: {err}");
        }
    }
    println!("{}", "-".repeat(72));
}

// Unit Tests (always run)

#[test]
fn test_benchmark_task_definitions() {
    let tasks = get_all_bench_tasks();
    assert_eq!(tasks.len(), 10, "should have exactly 10 tasks");

    for task in &tasks {
        assert!(!task.name.is_empty(), "task name must not be empty");
        assert!(
            !task.prompt.is_empty(),
            "task prompt must not be empty for {}",
            task.name
        );
        assert!(
            matches!(task.difficulty.as_str(), "easy" | "medium" | "hard"),
            "task {} has invalid difficulty: {}",
            task.name,
            task.difficulty
        );
        assert!(
            matches!(
                task.source.as_str(),
                "livebench" | "terminalbench" | "custom"
            ),
            "task {} has invalid source: {}",
            task.name,
            task.source
        );
        assert!(
            !task.expected_tools.is_empty(),
            "task {} must have expected tools",
            task.name
        );
    }

    // Verify category distribution
    let coding = tasks
        .iter()
        .filter(|t| t.category == TaskCategory::Coding)
        .count();
    let reasoning = tasks
        .iter()
        .filter(|t| t.category == TaskCategory::Reasoning)
        .count();
    let terminal = tasks
        .iter()
        .filter(|t| t.category == TaskCategory::TerminalOps)
        .count();
    let instruction = tasks
        .iter()
        .filter(|t| t.category == TaskCategory::InstructionFollowing)
        .count();
    assert_eq!(coding, 3, "3 coding tasks");
    assert_eq!(reasoning, 2, "2 reasoning tasks");
    assert_eq!(terminal, 3, "3 terminal ops tasks");
    assert_eq!(instruction, 2, "2 instruction following tasks");
}

#[test]
fn test_workspace_setup_functions() -> anyhow::Result<()> {
    let tasks_with_setup = get_all_bench_tasks()
        .into_iter()
        .filter(|t| t.setup_fn.is_some())
        .collect::<Vec<_>>();

    assert!(
        tasks_with_setup.len() >= 6,
        "at least 6 tasks should have setup functions"
    );

    for task in &tasks_with_setup {
        let dir = tempfile::tempdir()?;
        let workspace = dir.path();
        if let Some(setup) = task.setup_fn {
            setup(workspace).with_context(|| format!("setup for {}", task.name))?;
        }
        // After setup, at least one file should exist
        let has_files = std::fs::read_dir(workspace)?.next().is_some();
        assert!(
            has_files,
            "workspace for {} should have files after setup",
            task.name
        );
    }
    Ok(())
}

// Integration Tests (live API, feature-gated and ignored by default)

/// Helper: run a subset of tasks and produce the report.
async fn run_benchmark_suite(
    tasks: Vec<BenchTask>,
    suite_name: &str,
    min_pass_rate: f64,
) -> anyhow::Result<()> {
    let (provider, model) = create_provider()?;

    let tool_registry = default_registry();
    let tools_schema = build_tools_schema(&tool_registry);

    println!("\n Running {suite_name} ({} tasks)...", tasks.len());

    let mut results = Vec::new();
    for task in &tasks {
        println!("  > {} ({})...", task.name, task.difficulty);
        let result = run_bench_task(&provider, &model, &tool_registry, &tools_schema, task).await;
        println!(
            "    reward={:.2} duration={}ms",
            result.reward, result.duration_ms
        );
        if let Some(ref err) = result.error {
            println!("    error: {err}");
        }
        results.push(result);
        // Rate-limit between tasks
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    print_results_table(&results);
    let report = build_report(&model, results);
    let report_path = write_report(&report)?;
    println!("\n  Report written to: {}", report_path.display());

    let pass_rate = (report.passed as f64) / (report.total_tasks as f64);
    println!(
        "  Pass rate: {:.1}% (minimum: {:.0}%)",
        pass_rate * 100.0,
        min_pass_rate * 100.0
    );
    assert!(
        pass_rate >= min_pass_rate,
        "{suite_name} pass rate {pass_rate:.1}% below minimum {min_pass_rate:.0}%"
    );

    Ok(())
}

/// Select tasks by category.
#[allow(dead_code)]
fn tasks_by_category(category: TaskCategory) -> Vec<BenchTask> {
    get_all_bench_tasks()
        .into_iter()
        .filter(|t| t.category == category)
        .collect()
}

/// Select tasks by index range (1-based, inclusive).
fn tasks_by_indices(start: usize, end: usize) -> Vec<BenchTask> {
    get_all_bench_tasks()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| (start - 1) <= *i && *i < end)
        .map(|(_, t)| t)
        .collect()
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test tui_benchmark_harness -- --ignored"
)]
async fn benchmark_livebench_coding_tasks() -> anyhow::Result<()> {
    let tasks = tasks_by_indices(1, 3);
    run_benchmark_suite(tasks, "LiveBench Coding Tasks", 0.33).await
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test tui_benchmark_harness -- --ignored"
)]
async fn benchmark_livebench_reasoning_tasks() -> anyhow::Result<()> {
    let tasks = tasks_by_indices(4, 5);
    run_benchmark_suite(tasks, "LiveBench Reasoning Tasks", 0.5).await
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test tui_benchmark_harness -- --ignored"
)]
async fn benchmark_terminalbench_tasks() -> anyhow::Result<()> {
    let tasks = tasks_by_indices(6, 8);
    run_benchmark_suite(tasks, "TerminalBench Tasks", 0.33).await
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test tui_benchmark_harness -- --ignored"
)]
async fn benchmark_instruction_following_tasks() -> anyhow::Result<()> {
    let tasks = tasks_by_indices(9, 10);
    run_benchmark_suite(tasks, "Instruction Following Tasks", 0.5).await
}

#[tokio::test]
#[cfg_attr(
    not(feature = "live-api-tests"),
    ignore = "Requires ANTHROPIC_API_KEY — run with: cargo test --features live-api-tests --test tui_benchmark_harness -- --ignored"
)]
async fn benchmark_full_suite() -> anyhow::Result<()> {
    let tasks = get_all_bench_tasks();
    run_benchmark_suite(tasks, "Full Benchmark Suite", 0.6).await
}
