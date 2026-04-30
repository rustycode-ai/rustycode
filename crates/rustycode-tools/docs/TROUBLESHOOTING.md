# Troubleshooting Guide

**Version:** 0.1.0
**Last Updated:** 2025-03-14

Common issues and solutions when working with rustycode-tools.

## Table of Contents

- [File Operations](#file-operations)
- [Search Operations](#search-operations)
- [Command Execution](#command-execution)
- [Web Fetching](#web-fetching)
- [Git Operations](#git-operations)
- [Performance Issues](#performance-issues)
- [Permission Errors](#permission-errors)
- [Caching Problems](#caching-problems)
- [LSP Issues](#lsp-issues)

---

## File Operations

### Issue: "No such file or directory"

**Symptoms:**
```
Error: No such file or directory (os error 2)
```

**Causes:**
- File path is incorrect
- File doesn't exist
- Path is outside workspace

**Solutions:**

1. **Verify the file exists:**
```rust
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": ".",
        "recursive": false
    }),
    ..Default::default()
};
let result = executor.execute(&call);
println!("Available files: {}", result.output);
```

2. **Check if path is relative:**
```rust
// Use relative paths from workspace root
let call = ToolCall {
    name: "read_file".to_string(),
    arguments: json!({"path": "src/main.rs"}),  // Good
    // NOT: "/absolute/path/to/src/main.rs" or "../parent/file.rs"
    ..Default::default()
};
```

3. **Validate path format:**
```rust
fn validate_path(path: &str) -> bool {
    !path.contains("..") && !path.starts_with("/")
}
```

### Issue: "Binary file detected"

**Symptoms:**
```
Error: Cannot read binary file
Hint: This file appears to be binary (extension: .png).
Binary files are not supported for text operations.
```

**Causes:**
- File has binary extension (png, jpg, pdf, etc.)
- File contains non-UTF-8 content

**Solutions:**

1. **Use appropriate tools for binary files:**
```rust
// For images: Use image processing tools
// For PDFs: Use PDF extraction tools
// For archives: Use archive extraction tools
```

2. **Check file type first:**
```rust
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": ".",
        "recursive": false
    }),
    ..Default::default()
};

// Filter to text files only
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": ".",
        "filter": ".rs"  // Only Rust files
    }),
    ..Default::default()
};
```

### Issue: File output truncated

**Symptoms:**
```json
{
  "truncated": true,
  "shown_lines": 80,
  "total_lines": 542
}
```

**Causes:**
- File exceeds size limits
- Default limit: 80 lines / 10KB

**Solutions:**

1. **Read specific line ranges:**
```rust
let call = ToolCall {
    name: "read_file".to_string(),
    arguments: json!({
        "path": "large_file.rs",
        "start_line": 1,
        "end_line": 100
    }),
    ..Default::default()
};
```

2. **Process file in chunks:**
```rust
fn read_in_chunks(executor: &ToolExecutor, path: &str, chunk_size: usize) {
    let mut start = 1;
    loop {
        let end = start + chunk_size - 1;

        let call = ToolCall {
            name: "read_file".to_string(),
            arguments: json!({
                "path": path,
                "start_line": start,
                "end_line": end
            }),
            ..Default::default()
        };

        let result = executor.execute(&call);

        if let Some(metadata) = result.structured {
            let shown = metadata["shown_lines"].as_u64().unwrap_or(0);
            if shown == 0 {
                break;
            }
        }

        // Process chunk...

        start += chunk_size;
    }
}
```

### Issue: Write permission denied

**Symptoms:**
```
Error: Permission denied
```

**Causes:**
- Insufficient filesystem permissions
- Read-only filesystem
- File is locked by another process

**Solutions:**

1. **Check file permissions:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "ls -la /path/to/file"
    }),
    ..Default::default()
};
```

2. **Check workspace permissions:**
```rust
// Ensure workspace directory is writable
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "test -w /path/to/workspace && echo 'Writable' || echo 'Not writable'"
    }),
    ..Default::default()
};
```

---

## Search Operations

### Issue: Grep pattern invalid

**Symptoms:**
```
Error: Invalid regex pattern
```

**Causes:**
- Malformed regex pattern
- Invalid escape sequences

**Solutions:**

1. **Validate regex syntax:**
```rust
fn validate_regex(pattern: &str) -> Result<(), regex::Error> {
    regex::Regex::new(pattern)?;
    Ok(())
}
```

2. **Use raw strings for patterns:**
```rust
let call = ToolCall {
    name: "grep".to_string(),
    arguments: json!({
        "pattern": r"async\s+fn",  // Raw string - good
        // NOT: "async\\s+fn"      // Escaped - error-prone
        "path": "src"
    }),
    ..Default::default()
};
```

3. **Test pattern first:**
```rust
use regex::Regex;

let re = Regex::new(r"pattern").unwrap();
assert!(re.is_match("test string"));
```

### Issue: Too many grep results

**Symptoms:**
```json
{
  "total_matches": 15420,
  "files_with_matches": 342,
  "truncated": true
}
```

**Causes:**
- Pattern is too generic
- Large codebase

**Solutions:**

1. **Refine pattern:**
```rust
// Too broad
"fn"

// Better
"async\s+fn\s+\w+"

// Best - more specific
"async\s+fn\s+process_request"
```

2. **Limit matches per file:**
```rust
let call = ToolCall {
    name: "grep".to_string(),
    arguments: json!({
        "pattern": r"TODO|FIXME",
        "path": "src",
        "max_matches_per_file": 10
    }),
    ..Default::default()
};
```

3. **Narrow search path:**
```rust
// Search specific directory
"path": "src/core"

// Use glob first to narrow scope
```

### Issue: Glob returns no results

**Symptoms:**
```
// Empty output
```

**Causes:**
- Pattern doesn't match any files
- Pattern syntax error

**Solutions:**

1. **Verify pattern syntax:**
```rust
// Test pattern manually
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "find . -name '*.rs' | head -20"
    }),
    ..Default::default()
};
```

2. **Start with simple pattern:**
```rust
// Start simple
"*.rs"

// Then add complexity
"src/**/*.rs"

// Most specific
"src/tools/**/*.rs"
```

3. **List directory contents first:**
```rust
let call = ToolCall {
    name: "list_dir".to_string(),
    arguments: json!({
        "path": "src",
        "recursive": true
    }),
    ..Default::default()
};
```

---

## Command Execution

### Issue: Command timeout

**Symptoms:**
```
Error: Command timed out after 30 seconds
```

**Causes:**
- Command takes longer than timeout
- Command is hung

**Solutions:**

1. **Increase timeout:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "cargo build --release",
        "timeout_secs": 300  // 5 minutes
    }),
    ..Default::default()
};
```

2. **Use adaptive timeout:**
```rust
let mut timeout = 30;

loop {
    let call = ToolCall {
        name: "bash".to_string(),
        arguments: json!({
            "command": "cargo build",
            "timeout_secs": timeout
        }),
        ..Default::default()
    };

    let result = executor.execute(&call);

    if let Some(metadata) = result.structured {
        let exec_time = metadata["execution_time_ms"].as_u64().unwrap_or(0);
        let timeout_ms = timeout as u64 * 1000;

        // If command took 80%+ of timeout, double it
        if exec_time > timeout_ms * 8 / 10 {
            timeout *= 2;
        }
    }

    if result.success {
        break;
    }

    if !result.error.unwrap_or_default().contains("timed out") {
        break;
    }
}
```

3. **Check if command is hung:**
```rust
// Test with simpler command first
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "echo 'test'",
        "timeout_secs": 5
    }),
    ..Default::default()
};
```

### Issue: Command not found

**Symptoms:**
```
Error: command not found: cargo
```

**Causes:**
- Command not in PATH
- Tool not installed
- Wrong environment

**Solutions:**

1. **Check PATH:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "echo $PATH"
    }),
    ..Default::default()
};
```

2. **Use absolute paths:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "/usr/bin/cargo test"  // Absolute path
    }),
    ..Default::default()
};
```

3. **Verify tool installation:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "which cargo && cargo --version"
    }),
    ..Default::default()
};
```

### Issue: High memory usage

**Symptoms:**
- Commands consuming excessive memory
- System slowdown

**Solutions:**

1. **Monitor memory usage:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "/usr/bin/time -v cargo build 2>&1 | grep 'Maximum resident'"
    }),
    ..Default::default()
};
```

2. **Limit memory with ulimit:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "ulimit -v 4194304 && cargo build"  // 4GB limit
    }),
    ..Default::default()
};
```

---

## Web Fetching

### Issue: HTTP errors

**Symptoms:**
```json
{
  "status_code": 404,
  "error": "Not Found"
}
```

**Causes:**
- URL is incorrect
- Resource doesn't exist
- Network issues

**Solutions:**

1. **Verify URL:**
```rust
let call = ToolCall {
    name: "web_fetch".to_string(),
    arguments: json!({
        "url": "https://example.com/api/data"
    }),
    ..Default::default()
};

// Check status code in metadata
if let Some(metadata) = result.structured {
    let status = metadata["status_code"].as_u64().unwrap_or(0);
    if status >= 400 {
        eprintln!("HTTP Error: {}", status);
    }
}
```

2. **Test URL manually:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "curl -I https://example.com"
    }),
    ..Default::default()
};
```

3. **Handle redirects:**
```rust
// web_fetch follows redirects automatically
// Check final URL in metadata if needed
```

### Issue: Content truncated

**Symptoms:**
```json
{
  "chars": 50000,
  "truncated": true
}
```

**Causes:**
- Content exceeds 50,000 character limit

**Solutions:**

1. **Check if truncation matters:**
```rust
if let Some(metadata) = result.structured {
    if metadata["truncated"].as_bool().unwrap_or(false) {
        // Decide if truncation is acceptable
        // For some content (HTML), truncation may be fine
        // For JSON/API responses, you may need full content
    }
}
```

2. **Use range requests (if supported):**
```rust
// Fetch first part
let call1 = ToolCall {
    name: "web_fetch".to_string(),
    arguments: json!({
        "url": "https://example.com/large-data.json"
    }),
    ..Default::default()
};

// For full content, consider using curl with output to file
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "curl -o /tmp/data.json https://example.com/large-data.json"
    }),
    ..Default::default()
};
```

---

## Git Operations

### Issue: Not a git repository

**Symptoms:**
```
Error: fatal: not a git repository
```

**Causes:**
- Directory is not initialized as git repo
- Wrong path

**Solutions:**

1. **Initialize git repository:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "git init"
    }),
    ..Default::default()
};
```

2. **Check if directory is git repo:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "git status"
    }),
    ..Default::default()
};
```

### Issue: Nothing to commit

**Symptoms:**
```
Error: nothing to commit, working tree clean
```

**Causes:**
- No staged changes
- All changes committed

**Solutions:**

1. **Check git status first:**
```rust
let call = ToolCall {
    name: "git_status".to_string(),
    arguments: json!({}),
    ..Default::default()
};

let result = executor.execute(&call);

if let Some(metadata) = result.structured {
    let staged = metadata["staged"].as_u64().unwrap_or(0);

    if staged == 0 {
        // Stage files first
        let add_call = ToolCall {
            name: "bash".to_string(),
            arguments: json!({
                "command": "git add ."
            }),
            ..Default::default()
        };
        executor.execute(&add_call);
    }
}
```

---

## Performance Issues

### Issue: Slow tool execution

**Symptoms:**
- Tools taking longer than expected

**Solutions:**

1. **Check execution time in metadata:**
```rust
let result = executor.execute(&call);

if let Some(metadata) = result.structured {
    if let Some(time) = metadata.get("execution_time_ms") {
        println!("Tool took {}ms", time);
    }
}
```

2. **Enable caching:**
```rust
use rustycode_tools::CacheConfig;
use std::time::Duration;

let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(300),
    track_file_dependencies: true,
    ..Default::default()
};

let executor = ToolExecutor::with_cache(
    PathBuf::from("."),
    cache_config,
);
```

3. **Use compile-time tools for hot paths:**
```rust
use rustycode_tools::compile_time::*;

// Faster than runtime tools
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("Cargo.toml"),
    start_line: None,
    end_line: None,
})?;
```

### Issue: High memory consumption

**Symptoms:**
- Process using excessive memory
- System slowdown

**Solutions:**

1. **Limit cache size:**
```rust
let cache_config = CacheConfig {
    max_entries: 500,  // Reduce from 1000
    max_memory_bytes: Some(50 * 1024 * 1024),  // 50MB
    ..Default::default()
};
```

2. **Clear cache periodically:**
```rust
executor.cache().clear();
```

3. **Monitor memory usage:**
```rust
let metrics = executor.cache().get_metrics();
println!("Cache memory: {} bytes", metrics.memory_usage_bytes);
```

---

## Permission Errors

### Issue: Permission denied

**Symptoms:**
```
Error: Permission denied
```

**Causes:**
- Insufficient permissions for operation
- Tool requires higher permission level

**Solutions:**

1. **Check tool permissions:**
```rust
use rustycode_tools::get_tool_permission;

let perm = get_tool_permission("bash");
// Returns: Some(ToolPermission::Execute)
```

2. **Adjust session mode:**
```rust
use rustycode_protocol::SessionMode;

// Planning mode: Read-only tools
// Executing mode: All tools available
```

3. **Use appropriate context:**
```rust
use rustycode_tools::{ToolContext, ToolPermission};

let ctx = ToolContext::new(PathBuf::from("."))
    .with_max_permission(ToolPermission::Execute);
```

---

## Caching Problems

### Issue: Cache not working

**Symptoms:**
- Same tool executed multiple times
- No cache hits

**Solutions:**

1. **Check cache is enabled:**
```rust
// Ensure using cached execution
let result = executor.execute_cached_with_session(&call, None).await;
```

2. **Verify cacheable tool:**
```rust
use rustycode_tools::is_cacheable_tool;

if is_cacheable_tool("read_file") {
    // Tool supports caching
}
```

3. **Check cache metrics:**
```rust
let metrics = executor.cache().get_metrics();
println!("Hit rate: {:.2}%", metrics.hit_rate() * 100.0);
```

### Issue: Stale cache data

**Symptoms:**
- Returns outdated file contents
- File changes not reflected

**Solutions:**

1. **File dependencies are tracked automatically:**
```rust
let cache_config = CacheConfig {
    track_file_dependencies: true,  // Enabled by default
    ..Default::default()
};
```

2. **Manually invalidate cache:**
```rust
use std::path::PathBuf;

let file_path = PathBuf::from("src/main.rs");
executor.cache().invalidate_path(&file_path);
```

3. **Clear entire cache:**
```rust
executor.cache().clear();
```

---

## LSP Issues

### Issue: LSP not available

**Symptoms:**
```
Error: LSP client not available
```

**Causes:**
- Language server not running
- Project not configured for LSP

**Solutions:**

1. **Check if LSP is running:**
```rust
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "ps aux | grep rust-analyzer"
    }),
    ..Default::default()
};
```

2. **Start language server:**
```rust
// Start rust-analyzer or appropriate language server
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "rust-analyzer"
    }),
    ..Default::default()
};
```

---

## Getting Help

If you're still experiencing issues:

1. **Check logs:** Enable debug logging for detailed error information
2. **Review examples:** See [EXAMPLES.md](EXAMPLES.md) for working code
3. **API Reference:** Consult [API_REFERENCE.md](API_REFERENCE.md) for detailed API docs
4. **Integration Guide:** Read [INTEGRATION.md](INTEGRATION.md) for integration patterns

## Common Error Messages

| Error | Cause | Solution |
|-------|-------|----------|
| "No such file" | File doesn't exist | Check path, verify file exists |
| "Permission denied" | Insufficient permissions | Check file permissions, use appropriate permission level |
| "Invalid regex" | Malformed regex pattern | Validate pattern syntax, use raw strings |
| "Command timed out" | Command too slow | Increase timeout, optimize command |
| "Binary file detected" | Attempted to read binary file | Use appropriate tools for binary files |
| "Not a git repository" | Directory not a git repo | Initialize with `git init` |
| "Cache miss" | Result not in cache | Normal for first execution, check cache config |

## Best Practices to Avoid Issues

1. **Validate inputs:** Check file paths, URLs, and patterns before use
2. **Check results:** Always verify `result.success` before using output
3. **Handle errors:** Provide helpful error messages to users
4. **Use metadata:** Leverage structured metadata for decision making
5. **Enable caching:** Use cached execution for read operations
6. **Monitor performance:** Track execution times and cache hit rates
7. **Test patterns:** Validate regex patterns before use
8. **Use timeouts:** Set appropriate timeouts for long-running commands
9. **Check permissions:** Ensure sufficient permissions for operations
10. **Handle truncation:** Check if output was truncated and adjust accordingly
