# Tool Metadata Specification for LLM Consumption

## Overview

Tool metadata provides structured information about tool execution results, enabling LLMs to make informed decisions about subsequent actions. This specification defines the standard format and available fields.

## Why Metadata Matters

1. **Context Efficiency**: LLMs can understand results without processing full output
2. **Error Detection**: Structured failure indicators enable quick recovery
3. **Performance Tracking**: Execution time helps optimize workflow
4. **Content Validation**: Hashes and counts verify data integrity
5. **Statistical Insights**: Aggregated data aids decision-making

## Standard Fields (All Tools)

Every tool output includes these base fields:

```json
{
  "tool_name": "string",           // Tool identifier (e.g., "bash", "read_file")
  "success": boolean,               // true if operation succeeded
  "execution_time_ms": number,      // Execution duration in milliseconds
  "error": string | null,           // Error message if failed
  "truncated": boolean | null       // true if output was truncated
}
```

### Field Descriptions

- **tool_name**: Identifier used for tool selection and logging
- **success**: Quick check for pass/fail (derived from error field)
- **execution_time_ms**: Performance metric for timeout detection and optimization
- **error**: Human-readable error description when success=false
- **truncated**: Indicates whether output exceeds truncation limits

## Tool-Specific Fields

### ReadFileTool

File reading operations include content validation metadata:

```json
{
  "path": "string",                  // Full file path
  "bytes": number,                   // File size in bytes
  "lines": number,                   // Line count
  "content_hash": "string",          // SHA-256 hash of content
  "read_time_ms": number,            // Time spent reading
  "encoding": "utf-8" | string       // Detected encoding
}
```

**Usage Examples**:
```rust
// Detect large files before processing
if metadata["bytes"] > 1_000_000 {
    // File is >1MB, consider different approach
}

// Verify content hasn't changed
if metadata["content_hash"] != previous_hash {
    // File modified, re-parse
}
```

### WriteFileTool

File writing operations confirm what was written:

```json
{
  "path": "string",                  // Full file path
  "bytes_written": number,           // Bytes written to disk
  "lines_written": number,           // Lines written
  "write_time_ms": number,           // Time spent writing
  "existing_overwritten": boolean    // true if file existed before
}
```

**Usage Examples**:
```rust
// Verify write succeeded
if metadata["lines_written"] != expected_lines {
    // Mismatch - write may have failed
}

// Track file changes
if metadata["existing_overwritten"] {
    log_backup(path);
}
```

### BashTool

Command execution includes detailed status information:

```json
{
  "exit_code": number,               // Process exit code (0 = success)
  "command": "string",               // Executed command (for logging)
  "timeout_secs": number,            // Timeout limit used
  "timeout_hit": boolean,            // true if killed for timeout
  "signal": number | null,           // Signal if terminated by signal
  "stdout_truncated": boolean,       // true if stdout was truncated
  "stderr_truncated": boolean        // true if stderr was truncated
}
```

**Usage Examples**:
```rust
// Detect test failures
if metadata["exit_code"] != 0 {
    // Command failed, check stderr
}

// Detect timeouts
if metadata["timeout_hit"] {
    // Command took too long, optimize or increase timeout
}
```

### GrepTool

Pattern search results include statistical metadata:

```json
{
  "pattern": "string",               // Search pattern used
  "total_matches": number,           // Total matches found
  "files_with_matches": number,      // Number of files containing pattern
  "max_matches_per_file": number,    // Limit applied
  "top_files": [                     // Files with most matches
    {
      "path": "string",
      "matches": number
    }
  ],
  "before_context": number,          // Context lines before match
  "after_context": number            // Context lines after match
}
```

**Usage Examples**:
```rust
// Estimate result size
if metadata["total_matches"] > 1000 {
    // Too many results, refine pattern
}

// Focus on relevant files
let top_file = &metadata["top_files"][0];
if top_file["matches"] > metadata["total_matches"] * 0.5 {
    // Half of all matches in one file
}
```

### GlobTool

File pattern matching results:

```json
{
  "pattern": "string",               // Search pattern used
  "total_matches": number,           // Total files found
  "extensions": [                    // File type breakdown
    {
      "extension": "string",
      "count": number
    }
  ],
  "max_depth": number,               // Directory depth searched
  "search_time_ms": number           // Time spent searching
}
```

**Usage Examples**:
```rust
// Analyze codebase composition
for ext in &metadata["extensions"] {
    println!("{}: {} files", ext["extension"], ext["count"]);
}

// Detect project type
if has_extension(&metadata["extensions"], "go") {
    // Go project, use Go-specific tools
}
```

## Performance Considerations

### Expensive Operations

Some metadata fields are expensive to compute:

| Field | Cost | When to Include |
|-------|------|-----------------|
| content_hash | High | Large files, integrity verification |
| line_count | Medium | Text files, code analysis |
| extensions | Low | All glob operations |

### Best Practices

1. **Omit expensive fields** for quick operations
2. **Cache hashes** for repeated reads
3. **Use structured data** over text parsing when possible
4. **Prefer metadata queries** over re-processing output

## Error Handling

All tools follow consistent error reporting:

```json
{
  "success": false,
  "error": "descriptive message",
  "error_type": "FileNotFound | PermissionDenied | Timeout | InvalidInput",
  "recovery_hint": "suggested action"
}
```

### Error Types

- **FileNotFound**: File doesn't exist, check path
- **PermissionDenied**: Insufficient permissions, check ownership
- **Timeout**: Operation took too long, increase timeout or optimize
- **InvalidInput**: Malformed request, check parameters

## Integration Examples

### Example 1: Batch File Processing

```rust
let mut results = Vec::new();

for file in files {
    let output = read_file_tool.execute(params, ctx)?;
    let metadata = output.metadata;

    // Use metadata to decide processing strategy
    if metadata["bytes"] > LARGE_FILE_THRESHOLD {
        results.push(process_large_file(file, &metadata));
    } else {
        results.push(process_small_file(file, &metadata));
    }
}
```

### Example 2: Adaptive Command Timeout

```rust
let base_timeout = 30;

// Execute with initial timeout
let output = bash_tool.execute(params, ctx)?;
let metadata = output.metadata;

// Adjust timeout based on execution time
if metadata["success"] == true {
    let execution_time = metadata["execution_time_ms"].as_u64().unwrap_or(0);
    if execution_time > base_timeout * 1000 * 0.8 {
        // Command took 80% of timeout, increase for next run
        increased_timeout = base_timeout * 2;
    }
}
```

### Example 3: Search Result Analysis

```rust
let output = grep_tool.execute(params, ctx)?;
let metadata = output.metadata;

// Analyze match distribution
let total = metadata["total_matches"].as_u64().unwrap_or(0);
let files = metadata["files_with_matches"].as_u64().unwrap_or(0);

if total > 0 {
    let avg_per_file = total / files;

    if avg_per_file > 100 {
        // Highly repetitive pattern, consider refactoring
        println!("Pattern appears {} times per file on average", avg_per_file);
    }
}
```

## Version History

- **v1.0** (2025-03-14): Initial specification with core tool coverage
- **v1.1** (2025-03-14): Added SHA-256 hashing, execution time tracking
- **v1.2** (2025-03-14): Added statistical metadata for search tools

## Future Enhancements

Planned additions:

1. **Memory usage tracking**: RAM consumption per tool
2. **Cache hit rates**: Tool cache effectiveness
3. **Parallel execution stats**: Concurrent tool usage
4. **Cost estimation**: API costs for external tools
5. **Carbon footprint**: Energy consumption estimates

## See Also

- [TOOL_ENHANCEMENTS.md](./TOOL_ENHANCEMENTS.md) - Implementation details
- [QUICK_REFERENCE.md](./QUICK_REFERENCE.md) - Quick lookup guide
- [INDEX.md](./INDEX.md) - Main documentation index
