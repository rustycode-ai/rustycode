# Getting Started with rustycode-tools

**Version:** 0.1.0
**Last Updated:** 2025-03-14

A quick start guide to get you up and running with rustycode-tools in minutes.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rustycode-tools = "0.1.0"
rustycode-protocol = "0.1.0"
```

## Basic Setup

### 1. Create a Tool Executor

```rust
use rustycode_tools::ToolExecutor;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create executor with your project directory
    let executor = ToolExecutor::new(PathBuf::from("/path/to/project"));

    Ok(())
}
```

### 2. Execute Your First Tool

```rust
use rustycode_protocol::ToolCall;
use serde_json::json;

// Read a file
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({"path": "Cargo.toml"}),
};

let result = executor.execute(&call);

if result.success {
    println!("{}", result.output);
}
```

## Common Operations

### Reading Files

```rust
// Read entire file
let call = ToolCall {
    name: "read_file".to_string(),
    arguments: json!({"path": "src/main.rs"}),
    ..Default::default()
};

// Read specific lines (10-20)
let call = ToolCall {
    name: "read_file".to_string(),
    arguments: json!({
        "path": "src/main.rs",
        "start_line": 10,
        "end_line": 20
    }),
    ..Default::default()
};
```

### Writing Files

```rust
let call = ToolCall {
    name: "write_file".to_string(),
    arguments: json!({
        "path": "src/new_file.rs",
        "content": "fn main() {\n    println!(\"Hello!\");\n}\n"
    }),
    ..Default::default()
};

let result = executor.execute(&call);

// Check metadata
if let Some(metadata) = result.structured {
    println!("Wrote {} lines", metadata["lines"]);
}
```

### Searching Files

```rust
// Grep search
let call = ToolCall {
    name: "grep".to_string(),
    arguments: json!({
        "pattern": r"async\s+fn",
        "path": "src"
    }),
    ..Default::default()
};

let result = executor.execute(&call);

// Check match statistics
if let Some(metadata) = result.structured {
    println!("Found {} matches", metadata["total_matches"]);
}

// Glob pattern matching
let call = ToolCall {
    name: "glob".to_string(),
    arguments: json!({"pattern": "**/*.rs"}),
    ..Default::default()
};
```

### Running Commands

```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "cargo test",
        "timeout_secs": 60
    }),
    ..Default::default()
};

let result = executor.execute(&call);

// Check execution details
if let Some(metadata) = result.structured {
    println!("Exit code: {}", metadata["exit_code"]);
    println!("Duration: {}ms", metadata["execution_time_ms"]);
}
```

### Listing Directories

```rust
// List all files recursively
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": "src",
        "recursive": true
    }),
    ..Default::default()
};

// List only Rust files
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": "src",
        "filter": ".rs"
    }),
    ..Default::default()
};

// List only directories
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": ".",
        "filter": "dir"
    }),
    ..Default::default()
};
```

## Working with Metadata

All tools return structured metadata:

```rust
let result = executor.execute(&call);

// Access metadata
if let Some(metadata) = result.structured {
    // Common fields
    if let Some(truncated) = metadata.get("truncated") {
        println!("Output truncated: {}", truncated);
    }

    // Tool-specific fields
    if let Some(lines) = metadata.get("total_lines") {
        println!("Total lines: {}", lines);
    }
}
```

## Error Handling

```rust
let result = executor.execute(&call);

match (result.success, result.error) {
    (true, None) => {
        println!("Success: {}", result.output);
    }
    (false, Some(error)) => {
        eprintln!("Error: {}", error);

        // Handle specific errors
        if error.contains("No such file") {
            eprintln!("File not found. Check the path.");
        } else if error.contains("Permission denied") {
            eprintln!("Permission denied. Check file permissions.");
        }
    }
    _ => {
        eprintln!("Unexpected state");
    }
}
```

## Caching

Enable caching for improved performance:

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(300),  // 5 minutes
    max_entries: 1000,
    track_file_dependencies: true,
    max_memory_bytes: Some(100 * 1024 * 1024),  // 100MB
    enable_metrics: true,
};

let executor = ToolExecutor::with_cache(
    PathBuf::from("."),
    cache_config,
);

// Use cached execution
let result = executor.execute_cached_with_session(&call, None).await;
```

## Tool Reference

### Available Tools

| Tool | Permission | Description |
|------|------------|-------------|
| `read_file` | Read | Read text files |
| `write_file` | Write | Write files |
| `list_dir` | Read | List directories |
| `grep` | Read | Search with regex |
| `glob` | Read | Pattern matching |
| `bash` | Execute | Run commands |
| `web_fetch` | Network | Fetch URLs |
| `git_status` | Read | Git status |
| `git_diff` | Read | Git diff |
| `git_log` | Read | Git history |
| `git_commit` | Write | Create commits |

### Quick Reference

See [QUICK_REFERENCE.md](QUICK_REFERENCE.md) for complete parameter and metadata reference.

## Next Steps

1. **Learn More**: Read [API_REFERENCE.md](API_REFERENCE.md) for complete API documentation
2. **See Examples**: Check [EXAMPLES.md](EXAMPLES.md) for real-world usage patterns
3. **Integrate**: Follow [INTEGRATION.md](INTEGRATION.md) for LLM integration
4. **Understand Metadata**: Read [LLM_METADATA_SPEC.md](LLM_METADATA_SPEC.md) for metadata format

## Common Patterns

### Batch File Processing

```rust
// Find all Rust files
let glob_call = ToolCall {
    name: "glob".to_string(),
    arguments: json!({"pattern": "**/*.rs"}),
    ..Default::default()
};

let glob_result = executor.execute(&glob_call);

// Process each file
for file_path in glob_result.output.lines() {
    let read_call = ToolCall {
        name: "read_file".to_string(),
        arguments: json!({"path": file_path}),
        ..Default::default()
    };

    let result = executor.execute(&read_call);
    // Process file...
}
```

### Search and Replace

```rust
// Find all occurrences
let grep_call = ToolCall {
    name: "grep".to_string(),
    arguments: json!({
        "pattern": "old_function",
        "path": "src"
    }),
    ..Default::default()
};

let grep_result = executor.execute(&grep_call);

// Replace in found files
// (requires reading, modifying, and writing back)
```

### Git Workflow

```rust
// Check status
let status_call = ToolCall {
    name: "git_status".to_string(),
    arguments: json!({}),
    ..Default::default()
};

let status_result = executor.execute(&status_call);

// Commit changes if needed
if let Some(metadata) = status_result.structured {
    if metadata["staged"].as_u64().unwrap_or(0) > 0 {
        let commit_call = ToolCall {
            name: "git_commit".to_string(),
            arguments: json!({
                "message": "feat: Add new feature"
            }),
            ..Default::default()
        };

        executor.execute(&commit_call);
    }
}
```

## Tips

1. **Always check results**: Verify `result.success` before using output
2. **Use metadata**: Leverage structured metadata for decision making
3. **Enable caching**: Use cached execution for read operations
4. **Handle errors**: Provide helpful error messages to users
5. **Filter results**: Use `filter` parameter in `list_dir` to reduce noise

## Troubleshooting

See [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for common issues and solutions.

## Support

For more help, refer to:
- [API_REFERENCE.md](API_REFERENCE.md) - Complete API documentation
- [EXAMPLES.md](EXAMPLES.md) - Detailed examples
- [INTEGRATION.md](INTEGRATION.md) - Integration guide
- Main [README.md](../README.md) - Crate overview
