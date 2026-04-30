# rustycode-tools Usage Examples

**Version:** 0.1.0
**Last Updated:** 2025-03-14

Real-world usage examples for rustycode-tools, including common patterns, integration scenarios, and best practices.

## Table of Contents

- [Basic Usage](#basic-usage)
- [File Operations](#file-operations)
- [Search Operations](#search-operations)
- [Command Execution](#command-execution)
- [Web Fetching](#web-fetching)
- [Git Operations](#git-operations)
- [LSP Integration](#lsp-integration)
- [Caching](#caching)
- [Rate Limiting](#rate-limiting)
- [Custom Tools](#custom-tools)
- [Plugin System](#plugin-system)
- [Compile-Time Tools](#compile-time-tools)
- [Error Handling](#error-handling)
- [Performance Patterns](#performance-patterns)

---

## Basic Usage

### Minimal Setup

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create executor with current directory
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Execute a tool call
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "Cargo.toml"}),
    };

    let result = executor.execute(&call);

    if result.success {
        println!("{}", result.output);
    } else {
        eprintln!("Error: {}", result.error.unwrap_or_default());
    }

    Ok(())
}
```

### With Custom Configuration

```rust
use rustycode_tools::{ToolExecutor, CacheConfig, SandboxConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Configure cache
    let cache_config = CacheConfig {
        default_ttl: Duration::from_secs(600), // 10 minutes
        max_entries: 2000,
        track_file_dependencies: true,
        max_memory_bytes: Some(200 * 1024 * 1024), // 200 MB
        enable_metrics: true,
    };

    // Create executor with cache
    let executor = ToolExecutor::with_cache(
        PathBuf::from("."),
        cache_config,
    );

    Ok(())
}
```

### With Event Bus Integration

```rust
use rustycode_tools::ToolExecutor;
use rustycode_bus::EventBus;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create event bus
    let bus = Arc::new(EventBus::new());

    // Create executor with event bus
    let executor = ToolExecutor::with_event_bus(
        PathBuf::from("."),
        bus.clone(),
    );

    // Subscribe to tool execution events
    let (_id, mut rx) = bus.subscribe("tool.*").await?;

    // Execute tools in background
    tokio::spawn(async move {
        // Tool execution will publish events
    });

    // Listen for events
    while let Ok(event) = rx.recv().await {
        println!("Tool executed: {:?}", event);
    }

    Ok(())
}
```

---

## File Operations

### Reading Files

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn read_file_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Read entire file
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "src/main.rs"}),
    };
    let result = executor.execute(&call);

    // Read specific line range (lines 10-20)
    let call = ToolCall {
        call_id: "2".to_string(),
        name: "read_file".to_string(),
        arguments: json!({
            "path": "src/main.rs",
            "start_line": 10,
            "end_line": 20
        }),
    };
    let result = executor.execute(&call);

    Ok(())
}
```

### Writing Files

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn write_file_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "write_file".to_string(),
        arguments: json!({
            "path": "src/new_file.rs",
            "content": "fn main() {\n    println!(\"Hello, world!\");\n}\n"
        }),
    };

    let result = executor.execute(&call);

    if result.success {
        println!("File written successfully");
        if let Some(metadata) = result.structured {
            println!("Lines: {}", metadata["lines"]);
            println!("Bytes: {}", metadata["bytes"]);
        }
    }

    Ok(())
}
```

### Listing Directories

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn list_dir_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // List all files recursively
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "list_dir".to_string(),
        arguments: json!({
            "path": "src",
            "recursive": true,
            "max_depth": 3
        }),
    };

    // List only Rust files
    let call = ToolCall {
        call_id: "2".to_string(),
        name: "list_dir".to_string(),
        arguments: json!({
            "path": "src",
            "recursive": true,
            "filter": ".rs"
        }),
    };

    // List only directories
    let call = ToolCall {
        call_id: "3".to_string(),
        name: "list_dir".to_string(),
        arguments: json!({
            "path": ".",
            "recursive": false,
            "filter": "dir"
        }),
    };

    Ok(())
}
```

### Batch File Processing

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn batch_process_files() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // First, list all Rust files
    let list_call = ToolCall {
        call_id: "1".to_string(),
        name: "glob".to_string(),
        arguments: json!({"pattern": "**/*.rs"}),
    };

    let list_result = executor.execute(&list_call);

    if !list_result.success {
        return Err(anyhow::anyhow!("Failed to list files"));
    }

    // Parse file paths from output
    let files: Vec<&str> = list_result
        .output
        .lines()
        .collect();

    // Process each file
    for (i, file) in files.iter().enumerate() {
        let call = ToolCall {
            call_id: format!("read-{}", i),
            name: "read_file".to_string(),
            arguments: json!({"path": file}),
        };

        let result = executor.execute(&call);

        if result.success {
            // Process file content
            process_content(&result.output)?;
        }
    }

    Ok(())
}

fn process_content(content: &str) -> anyhow::Result<()> {
    // Custom processing logic
    Ok(())
}
```

---

## Search Operations

### Grep Search

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn grep_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Simple pattern search
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "grep".to_string(),
        arguments: json!({
            "pattern": r"async\s+fn",
            "path": "src"
        }),
    };

    let result = executor.execute(&call);

    // Check metadata for statistics
    if let Some(metadata) = result.structured {
        println!("Total matches: {}", metadata["total_matches"]);
        println!("Files with matches: {}", metadata["files_with_matches"]);
    }

    Ok(())
}
```

### Context-Aware Search

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn grep_with_context() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "grep".to_string(),
        arguments: json!({
            "pattern": r"TODO|FIXME",
            "path": "src",
            "before_context": 2,
            "after_context": 2
        }),
    };

    let result = executor.execute(&call);
    // Output will show 2 lines before and after each match

    Ok(())
}
```

### Limited Results

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn grep_limited() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "grep".to_string(),
        arguments: json!({
            "pattern": r"println!",
            "path": "src",
            "max_matches_per_file": 10
        }),
    };

    let result = executor.execute(&call);
    // Each file will show max 10 matches

    Ok(())
}
```

### Glob Pattern Matching

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn glob_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Find all Rust files
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "glob".to_string(),
        arguments: json!({"pattern": "**/*.rs"}),
    };

    let result = executor.execute(&call);

    // Analyze file types
    if let Some(metadata) = result.structured {
        if let Some(extensions) = metadata.get("extensions") {
            println!("File types found:");
            for ext in extensions.as_array().unwrap() {
                let ext_name = ext["extension"].as_str().unwrap();
                let count = ext["count"].as_u64().unwrap();
                println!("  {}: {} files", ext_name, count);
            }
        }
    }

    Ok(())
}
```

---

## Command Execution

### Basic Command Execution

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn bash_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "bash".to_string(),
        arguments: json!({
            "command": "cargo test",
            "timeout_secs": 60
        }),
    };

    let result = executor.execute(&call);

    if result.success {
        println!("Tests passed");
    } else {
        println!("Tests failed");
        // Check exit code from metadata
        if let Some(metadata) = result.structured {
            println!("Exit code: {}", metadata["exit_code"]);
        }
    }

    Ok(())
}
```

### Command with Custom Working Directory

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn bash_with_cwd() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "bash".to_string(),
        arguments: json!({
            "command": "npm install",
            "cwd": "frontend",
            "timeout_secs": 120
        }),
    };

    let result = executor.execute(&call);

    Ok(())
}
```

### Adaptive Timeout Based on Execution Time

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn adaptive_timeout() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));
    let mut timeout = 30;

    loop {
        let call = ToolCall {
            call_id: "1".to_string(),
            name: "bash".to_string(),
            arguments: json!({
                "command": "cargo build",
                "timeout_secs": timeout
            }),
        };

        let result = executor.execute(&call);

        if let Some(metadata) = result.structured {
            let execution_time = metadata["execution_time_ms"].as_u64().unwrap_or(0);
            let timeout_ms = timeout as u64 * 1000;

            // If command took 80%+ of timeout, double it for next run
            if execution_time > timeout_ms * 8 / 10 {
                timeout *= 2;
                println!("Increasing timeout to {}s", timeout);
            }
        }

        if result.success {
            break;
        }

        // Check if it timed out
        if let Some(error) = &result.error {
            if error.contains("timed out") {
                println!("Command timed out, retrying with longer timeout");
                continue;
            }
        }

        break;
    }

    Ok(())
}
```

---

## Web Fetching

### Fetch URL

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn web_fetch_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "web_fetch".to_string(),
        arguments: json!({
            "url": "https://example.com/docs",
            "convert_markdown": true
        }),
    };

    let result = executor.execute(&call);

    if result.success {
        println!("{}", result.output);

        if let Some(metadata) = result.structured {
            println!("Status: {}", metadata["status_code"]);
            println!("Time: {}ms", metadata["total_time_ms"]);

            // Access response headers
            if let Some(headers) = metadata.get("headers") {
                println!("Content-Type: {}", headers["content-type"]);
            }
        }
    }

    Ok(())
}
```

### Fetch with Error Handling

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn web_fetch_with_retry() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let urls = vec![
        "https://api.example.com/data1.json",
        "https://api.example.com/data2.json",
    ];

    for url in urls {
        let call = ToolCall {
            call_id: "fetch".to_string(),
            name: "web_fetch".to_string(),
            arguments: json!({"url": url}),
        };

        match executor.execute(&call) {
            result if result.success => {
                println!("Fetched: {}", url);

                // Parse JSON response
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&result.output) {
                    // Process data
                }
            }
            result => {
                eprintln!("Failed to fetch {}: {}", url, result.error.unwrap_or_default());
            }
        }
    }

    Ok(())
}
```

---

## Git Operations

### Git Status

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;

fn git_status_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "git_status".to_string(),
        arguments: serde_json::json!({}),
    };

    let result = executor.execute(&call);

    if let Some(metadata) = result.structured {
        println!("Branch: {}", metadata["branch"]);
        println!("Staged: {}", metadata["staged"]);
        println!("Unstaged: {}", metadata["unstaged"]);
        println!("Untracked: {}", metadata["untracked"]);
    }

    Ok(())
}
```

### Git Diff

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn git_diff_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // Diff specific file
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "git_diff".to_string(),
        arguments: json!({
            "path_spec": "src/main.rs",
            "color_words": true
        }),
    };

    let result = executor.execute(&call);

    // Check diff statistics
    if let Some(metadata) = result.structured {
        println!("Files changed: {}", metadata["files_changed"]);
        println!("Additions: {}", metadata["additions"]);
        println!("Deletions: {}", metadata["deletions"]);
    }

    Ok(())
}
```

### Git Commit

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn git_commit_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    // First, check status
    let status_call = ToolCall {
        call_id: "1".to_string(),
        name: "git_status".to_string(),
        arguments: json!({}),
    };

    let status_result = executor.execute(&status_call);

    // If there are changes, commit them
    if let Some(metadata) = status_result.structured {
        let staged = metadata["staged"].as_u64().unwrap_or(0);

        if staged > 0 {
            let commit_call = ToolCall {
                call_id: "2".to_string(),
                name: "git_commit".to_string(),
                arguments: json!({
                    "message": "feat: Add new feature"
                }),
            };

            let commit_result = executor.execute(&commit_call);

            if commit_result.success {
                println!("Committed successfully");

                if let Some(commit_meta) = commit_result.structured {
                    println!("Commit: {}", commit_meta["commit_hash"]);
                }
            }
        }
    }

    Ok(())
}
```

### Git Log Analysis

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn git_log_analysis() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "git_log".to_string(),
        arguments: json!({
            "limit": 20
        }),
    };

    let result = executor.execute(&call);

    // Parse commit history
    for line in result.output.lines() {
        if line.starts_with("commit ") {
            let commit_hash = &line[7..47];
            println!("Commit: {}", commit_hash);
        }
    }

    Ok(())
}
```

---

## LSP Integration

### Get Diagnostics

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;

fn lsp_diagnostics_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "lsp_diagnostics".to_string(),
        arguments: serde_json::json!({}),
    };

    let result = executor.execute(&call);

    if let Some(metadata) = result.structured {
        println!("Errors: {}", metadata["error_count"]);
        println!("Warnings: {}", metadata["warning_count"]);
        println!("Info: {}", metadata["info_count"]);
    }

    Ok(())
}
```

### Hover Information

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn lsp_hover_example() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "lsp_hover".to_string(),
        arguments: json!({
            "file": "src/main.rs",
            "line": 42,
            "column": 10
        }),
    };

    let result = executor.execute(&call);

    if result.success {
        println!("Hover info: {}", result.output);
    }

    Ok(())
}
```

---

## Caching

### Basic Caching

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cache_config = CacheConfig {
        default_ttl: Duration::from_secs(300),
        max_entries: 1000,
        track_file_dependencies: true,
        max_memory_bytes: None,
        enable_metrics: true,
    };

    let executor = ToolExecutor::with_cache(
        PathBuf::from("."),
        cache_config,
    );

    // First call - executes and caches result
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "Cargo.toml"}),
    };

    let result1 = executor.execute_cached_with_session(&call, None).await;

    // Second call - returns cached result
    let result2 = executor.execute_cached_with_session(&call, None).await;

    Ok(())
}
```

### Cache Invalidation

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cache_config = CacheConfig {
        default_ttl: Duration::from_secs(300),
        track_file_dependencies: true,
        ..Default::default()
    };

    let executor = ToolExecutor::with_cache(
        PathBuf::from("."),
        cache_config,
    );

    // Read file
    let call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "src/main.rs"}),
    };

    let result1 = executor.execute_cached_with_session(&call, None).await;

    // Modify file externally...
    // Next read will invalidate cache due to file modification time change

    let result2 = executor.execute_cached_with_session(&call, None).await;

    Ok(())
}
```

---

## Rate Limiting

### Custom Rate Limit

```rust
use rustycode_tools::{ToolExecutor, ToolRegistry};
use std::num::NonZeroU32;

fn custom_rate_limit() -> anyhow::Result<()> {
    // Create registry with custom rate limit
    let registry = ToolRegistry::with_rate_limiting(
        NonZeroU32::new(20).unwrap(),  // 20 requests per second
        NonZeroU32::new(40).unwrap(),  // burst of 40
    );

    // Use registry in executor...
    // (You'll need to implement ToolExecutor::with_registry)

    Ok(())
}
```

---

## Custom Tools

### Implementing a Custom Tool

```rust
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};
use anyhow::Result;

struct UppercaseTool;

impl Tool for UppercaseTool {
    fn name(&self) -> &str {
        "uppercase"
    }

    fn description(&self) -> &str {
        "Convert text to uppercase"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {"type": "string"}
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'text' parameter"))?;

        let uppercase = text.to_uppercase();

        Ok(ToolOutput::text(uppercase))
    }
}

// Register and use
fn use_custom_tool() -> anyhow::Result<()> {
    let mut registry = rustycode_tools::ToolRegistry::new();
    registry.register(UppercaseTool);

    let call = rustycode_protocol::ToolCall {
        call_id: "1".to_string(),
        name: "uppercase".to_string(),
        arguments: json!({"text": "hello world"}),
    };

    let ctx = rustycode_tools::ToolContext::new(std::env::current_dir()?);
    let result = registry.execute(&call, &ctx);

    if result.success {
        assert_eq!(result.output, "HELLO WORLD");
    }

    Ok(())
}
```

---

## Plugin System

### Creating a Plugin

```rust
use rustycode_tools::{
    ToolPlugin, ToolContext, ToolOutput, PluginCapabilities,
    PluginManager, PluginInfo
};
use serde_json::{json, Value};
use anyhow::Result;

struct DatabasePlugin {
    connection: Option<String>,
}

impl ToolPlugin for DatabasePlugin {
    fn name(&self) -> &str {
        "database_query"
    }

    fn description(&self) -> &str {
        "Execute database queries"
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities {
            max_permission: rustycode_tools::ToolPermission::Read,
            filesystem: false,
            network: true,
            execute: false,
            max_memory_mb: Some(50),
            max_cpu_secs: Some(10),
        }
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string"}
            }
        })
    }

    fn initialize(&mut self) -> Result<()> {
        // Setup database connection
        self.connection = Some("postgresql://...".to_string());
        Ok(())
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing 'query' parameter"))?;

        // Execute query...
        let result = format!("Query result for: {}", query);

        Ok(ToolOutput::text(result))
    }

    fn cleanup(&mut self) -> Result<()> {
        // Close connection
        self.connection = None;
        Ok(())
    }
}

// Use plugin
fn use_plugin() -> anyhow::Result<()> {
    let mut manager = PluginManager::new();

    let plugin = Box::new(DatabasePlugin { connection: None });
    manager.register(plugin)?;

    // List plugins
    let plugins = manager.list();
    for plugin in plugins {
        println!("{}: {}", plugin.name, plugin.description);
    }

    // Execute plugin
    let ctx = rustycode_tools::ToolContext::new(std::env::current_dir()?);
    let result = manager.execute(
        "database_query",
        json!({"query": "SELECT * FROM users"}),
        &ctx,
    )?;

    Ok(())
}
```

---

## Compile-Time Tools

### Using Compile-Time Tools

```rust
use rustycode_tools::compile_time::*;
use std::path::PathBuf;

fn compile_time_example() -> anyhow::Result<()> {
    // Type-safe, zero-cost tool execution
    let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: PathBuf::from("Cargo.toml"),
        start_line: None,
        end_line: None,
    })?;

    println!("Content:\n{}", result.content);
    println!("Lines: {}", result.line_count);
    println!("Bytes: {}", result.byte_count);

    Ok(())
}
```

### Custom Compile-Time Tool

```rust
use rustycode_tools::compile_time::*;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct WordCountInput {
    text: String,
}

#[derive(Debug, Serialize)]
struct WordCountOutput {
    word_count: usize,
    char_count: usize,
}

#[derive(Debug, thiserror::Error)]
enum WordCountError {
    #[error("Empty input")]
    EmptyInput,
}

struct WordCountTool;

impl Tool for WordCountTool {
    type Input = WordCountInput;
    type Output = WordCountOutput;
    type Error = WordCountError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "word_count",
        description: "Count words and characters",
        permission: ToolPermission::None,
        category: ToolCategory::ReadOnly,
    };

    fn execute(input: Self::Input) -> Result<Self::Output, Self::Error> {
        if input.text.is_empty() {
            return Err(WordCountError::EmptyInput);
        }

        let word_count = input.text.split_whitespace().count();
        let char_count = input.text.chars().count();

        Ok(WordCountOutput {
            word_count,
            char_count,
        })
    }
}

fn use_custom_compile_time_tool() -> anyhow::Result<()> {
    let result = ToolDispatcher::<WordCountTool>::dispatch(WordCountInput {
        text: "Hello world".to_string(),
    })?;

    println!("Words: {}", result.word_count);
    println!("Chars: {}", result.char_count);

    Ok(())
}
```

---

## Error Handling

### Comprehensive Error Handling

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn comprehensive_error_handling() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "nonexistent.txt"}),
    };

    let result = executor.execute(&call);

    match (result.success, result.error) {
        (true, None) => {
            println!("Success: {}", result.output);
        }
        (false, Some(error)) => {
            // Handle specific error types
            if error.contains("No such file") {
                eprintln!("File not found. Please check the path.");
            } else if error.contains("Permission denied") {
                eprintln!("Permission denied. Check file permissions.");
            } else if error.contains("Binary file") {
                eprintln!("Cannot read binary file.");
            } else {
                eprintln!("Error: {}", error);
            }
        }
        (false, None) => {
            eprintln!("Unknown error occurred");
        }
        _ => {
            eprintln!("Unexpected state");
        }
    }

    Ok(())
}
```

### Retry Logic

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;

fn retry_with_backoff() -> anyhow::Result<()> {
    let executor = ToolExecutor::new(PathBuf::from("."));

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "bash".to_string(),
        arguments: json!({
            "command": "curl https://api.example.com/data",
            "timeout_secs": 10
        }),
    };

    let max_retries = 3;
    let mut delay_ms = 1000;

    for attempt in 0..max_retries {
        let result = executor.execute(&call);

        if result.success {
            println!("Success on attempt {}", attempt + 1);
            return Ok(());
        }

        // Check if error is retryable
        let should_retry = result
            .error
            .as_ref()
            .map(|e| e.contains("timeout") || e.contains("connection"))
            .unwrap_or(false);

        if !should_retry || attempt == max_retries - 1 {
            return Err(anyhow::anyhow!("Failed after {} attempts: {}",
                attempt + 1,
                result.error.unwrap_or_default()));
        }

        // Exponential backoff
        println!("Retry {} in {}ms", attempt + 2, delay_ms);
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        delay_ms *= 2;
    }

    Ok(())
}
```

---

## Performance Patterns

### Parallel File Processing

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;
use std::sync::Arc;
use std::thread;

fn parallel_file_processing() -> anyhow::Result<()> {
    let executor = Arc::new(ToolExecutor::new(PathBuf::from(".")));

    let files = vec![
        "src/main.rs",
        "src/lib.rs",
        "src/config.rs",
    ];

    let handles: Vec<_> = files
        .into_iter()
        .map(|file| {
            let executor = executor.clone();
            thread::spawn(move || {
                let call = ToolCall {
                    call_id: "1".to_string(),
                    name: "read_file".to_string(),
                    arguments: json!({"path": file}),
                };

                executor.execute(&call)
            })
        })
        .collect();

    // Collect results
    for handle in handles {
        let result = handle.join().unwrap();
        if result.success {
            // Process result
        }
    }

    Ok(())
}
```

### Batch Processing with Caching

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

#[tokio::main]
async fn batch_with_cache() -> anyhow::Result<()> {
    let cache_config = CacheConfig {
        default_ttl: Duration::from_secs(600),
        ..Default::default()
    };

    let executor = Arc::new(ToolExecutor::with_cache(
        PathBuf::from("."),
        cache_config,
    ));

    let files = vec!["Cargo.toml", "Cargo.lock", "README.md"];

    // Process concurrently with caching
    let futures: Vec<_> = files
        .into_iter()
        .map(|file| {
            let executor = executor.clone();
            async move {
                let call = rustycode_protocol::ToolCall {
                    call_id: "1".to_string(),
                    name: "read_file".to_string(),
                    arguments: serde_json::json!({"path": file}),
                };

                executor.execute_cached_with_session(&call, None).await
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for result in results {
        if result.success {
            // Process result
        }
    }

    Ok(())
}
```

---

## Best Practices

### 1. Always Check Results

```rust
let result = executor.execute(&call);

if !result.success {
    // Handle error
    if let Some(error) = result.error {
        eprintln!("Tool execution failed: {}", error);
        return Err(anyhow::anyhow!("Tool error: {}", error));
    }
}

// Use result.output
```

### 2. Leverage Metadata

```rust
if let Some(metadata) = result.structured {
    // Use metadata for decision making
    if metadata["total_matches"].as_u64().unwrap_or(0) > 1000 {
        // Too many results, refine search
    }
}
```

### 3. Use Caching for Read Operations

```rust
// For read-only tools, use cached execution
let result = executor.execute_cached_with_session(&call, None).await;
```

### 4. Validate Inputs

```rust
// Always validate user input before tool execution
if !path.is_empty() && !path.contains("..") {
    let call = ToolCall { /* ... */ };
    let result = executor.execute(&call);
}
```

### 5. Handle Rate Limits

```rust
match executor.execute(&call) {
    result if result.error.as_ref().map(|e| e.contains("rate limit")).unwrap_or(false) => {
        // Back off and retry
        std::thread::sleep(Duration::from_millis(1000));
    }
    result => { /* ... */ }
}
```

---

## See Also

- [API_REFERENCE.md](./API_REFERENCE.md) - Complete API reference
- [INTEGRATION.md](./INTEGRATION.md) - Integration guide
- [TOOL_METADATA.md](./TOOL_METADATA.md) - Metadata reference
- [LLM_METADATA_SPEC.md](./LLM_METADATA_SPEC.md) - LLM metadata format
