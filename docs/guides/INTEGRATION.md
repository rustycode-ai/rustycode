# rustycode-tools Integration Guide

Complete guide for integrating rustycode-tools into your application.

## Table of Contents

- [Quick Start](#quick-start)
- [Adding New Tools](#adding-new-tools)
- [Tool Registration](#tool-registration)
- [Permission System](#permission-system)
- [Caching Configuration](#caching-configuration)
- [Event Bus Integration](#event-bus-integration)
- [Best Practices](#best-practices)
- [Testing Tools](#testing-tools)
- [Common Patterns](#common-patterns)

---

## Quick Start

### Minimal Integration

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use serde_json::json;
use std::path::PathBuf;

// Create executor with default tools
let executor = ToolExecutor::new(PathBuf::from("/workspace"));

// Execute a tool call
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({
        "path": "src/lib.rs"
    }),
};

let result = executor.execute(&call);
assert!(result.success);
println!("{}", result.output);
```

### With Custom Configuration

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

// Create executor with custom cache
let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(300),
    max_entries: 1000,
    ..Default::default()
};

let executor = ToolExecutor::with_cache(
    PathBuf::from("/workspace"),
    cache_config
);
```

---

## Adding New Tools

### Step 1: Define Your Tool

Create a struct implementing the `Tool` trait:

```rust
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};
use anyhow::Result;

pub struct MyCustomTool;

impl Tool for MyCustomTool {
    fn name(&self) -> &str {
        "my_custom_tool"
    }

    fn description(&self) -> &str {
        "Does something custom"
    }

    fn permission(&self) -> ToolPermission {
        // Choose appropriate permission level
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["input"],
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input parameter"
                },
                "optional_param": {
                    "type": "boolean",
                    "description": "Optional parameter"
                }
            }
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        // Extract parameters
        let input = params.get("input")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing 'input' parameter"))?;

        let optional = params.get("optional_param")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Perform tool logic
        let result = format!("Processed: {}", input);
        if optional {
            println!("Optional mode enabled");
        }

        // Return output with metadata
        Ok(ToolOutput::with_structured(
            result.clone(),
            json!({
                "result": result,
                "optional_enabled": optional
            })
        ))
    }
}
```

### Step 2: Register Your Tool

```rust
use rustycode_tools::{ToolRegistry, default_registry};

// Option 1: Create custom registry
let mut registry = ToolRegistry::new();
registry.register(MyCustomTool);

// Option 2: Extend default registry
let mut registry = default_registry();
registry.register(MyCustomTool);
```

### Step 3: Create Custom Executor

```rust
use rustycode_tools::ToolExecutor;

fn create_custom_executor(cwd: PathBuf) -> ToolExecutor {
    // Start with default tools
    let mut registry = default_registry();

    // Add custom tools
    registry.register(MyCustomTool);
    registry.register(AnotherCustomTool);

    // Create executor with custom registry
    ToolExecutor {
        registry,
        context: ToolContext::new(cwd),
        bus: None,
        cache: Arc::new(ToolCache::new_with_defaults()),
    }
}
```

---

## Tool Registration

### Default Registry

```rust
use rustycode_tools::default_registry;

// Get registry with all built-in tools
let registry = default_registry();

// List all tools
let tools = registry.list();
for tool in tools {
    println!("{}: {}", tool.name, tool.description);
}
```

### Custom Registry

```rust
use rustycode_tools::{ToolRegistry, ReadFileTool, WriteFileTool};

// Create empty registry
let mut registry = ToolRegistry::new();

// Register specific tools
registry.register(ReadFileTool);
registry.register(WriteFileTool);
registry.register(MyCustomTool);

// Use registry
let tool = registry.get("read_file").unwrap();
```

### Conditional Registration

```rust
use rustycode_tools::{ToolRegistry, BashTool};

let mut registry = ToolRegistry::new();

// Only register bash tool in executing mode
if session_mode == SessionMode::Executing {
    registry.register(BashTool);
}

// Or filter tools after registration
let allowed_tools = get_allowed_tools(session_mode);
for tool_name in allowed_tools {
    if let Some(tool) = default_registry().get(&tool_name) {
        // Register tool...
    }
}
```

---

## Permission System

### Understanding Permissions

```rust
use rustycode_tools::ToolPermission;

// Permission hierarchy (lowest to highest)
// None < Read < Write < Execute < Network

// Read tools: read_file, list_dir, grep, glob, git_status, etc.
// Write tools: write_file, git_commit
// Execute tools: bash
// Network tools: web_fetch
```

### Declaring Tool Permissions

```rust
impl Tool for MyCustomTool {
    fn permission(&self) -> ToolPermission {
        // Choose based on what your tool does:
        // - ToolPermission::None - No special requirements
        // - ToolPermission::Read - Only reads data
        // - ToolPermission::Write - Modifies files/system
        // - ToolPermission::Execute - Runs commands
        // - ToolPermission::Network - Makes network requests

        ToolPermission::Read
    }
}
```

### Session-based Permission Filtering

```rust
use rustycode_protocol::SessionMode;
use rustycode_tools::{check_tool_permission, get_allowed_tools};

// Check if tool is allowed in current mode
let allowed = check_tool_permission("bash", SessionMode::Planning);
assert!(!allowed); // bash not allowed in planning mode

let allowed = check_tool_permission("bash", SessionMode::Executing);
assert!(allowed); // bash allowed in executing mode

// Get all allowed tools for a mode
let planning_tools = get_allowed_tools(SessionMode::Planning);
println!("Planning tools: {:?}", planning_tools);
// ["read_file", "list_dir", "grep", "glob", ...]
```

### Custom Permission Checks

```rust
use rustycode_tools::{ToolContext, ToolPermission};

fn execute_with_permission_check(
    tool: &dyn Tool,
    params: Value,
    ctx: &ToolContext
) -> Result<ToolOutput> {
    // Check if tool's permission exceeds session limit
    if tool.permission() as u8 > ctx.max_permission as u8 {
        anyhow::bail!(
            "Permission denied: tool requires {:?}, session allows {:?}",
            tool.permission(),
            ctx.max_permission
        );
    }

    // Execute tool
    tool.execute(params, ctx)
}
```

---

## Caching Configuration

### Basic Cache Setup

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

// Default configuration
let executor = ToolExecutor::new(PathBuf::from("/workspace"));

// Custom configuration
let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(600), // 10 minutes
    max_entries: 2000,
    track_file_dependencies: true,
    max_memory_bytes: Some(200 * 1024 * 1024), // 200 MB
    enable_metrics: true,
};

let executor = ToolExecutor::with_cache(
    PathBuf::from("/workspace"),
    cache_config
);
```

### Cache Tuning Guidelines

#### For Read-Heavy Workloads

```rust
let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(1800), // 30 minutes
    max_entries: 5000,
    max_memory_bytes: Some(500 * 1024 * 1024), // 500 MB
    ..Default::default()
};
```

#### For Write-Heavy Workloads

```rust
let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(60), // 1 minute
    max_entries: 500,
    max_memory_bytes: Some(50 * 1024 * 1024), // 50 MB
    ..Default::default()
};
```

#### For Memory-Constrained Environments

```rust
let cache_config = CacheConfig {
    default_ttl: Duration::from_secs(300),
    max_entries: 500,
    max_memory_bytes: Some(25 * 1024 * 1024), // 25 MB
    ..Default::default()
};
```

### Cache Monitoring

```rust
use rustycode_tools::ToolCache;

let cache = ToolCache::new_with_defaults();

// Get statistics
let stats = cache.stats().await;
println!("Total entries: {}", stats.total_entries);
println!("Valid entries: {}", stats.valid_entries);
println!("Hit rate: {:.2}%", stats.metrics.hit_rate() * 100.0);

// Get detailed metrics
let metrics = cache.get_metrics();
println!("Hits: {}", metrics.hits);
println!("Misses: {}", metrics.misses);
println!("Evictions: {}", metrics.evictions);
println!("Memory: {} bytes", metrics.current_memory_bytes);

// Reset metrics
cache.reset_metrics();
```

### Manual Cache Invalidation

```rust
// Prune expired entries
let pruned = cache.prune().await;
println!("Pruned {} entries", pruned);

// Clear all entries
cache.clear().await;
```

---

## Event Bus Integration

### Basic Event Bus Setup

```rust
use rustycode_tools::ToolExecutor;
use rustycode_bus::EventBus;
use std::sync::Arc;

// Create event bus
let bus = Arc::new(EventBus::new());

// Create executor with event bus
let executor = ToolExecutor::with_event_bus(
    PathBuf::from("/workspace"),
    bus.clone()
);
```

### Subscribing to Tool Events

```rust
// Subscribe to all tool events
let (_id, mut rx) = bus.subscribe("tool.*").await?;

// Execute tool (publishes event)
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({
        "path": "src/lib.rs"
    }),
};

executor.execute_with_session(&call, Some(session_id));

// Receive event
use rustycode_bus::ToolExecutedEvent;
let event = rx.recv().await?;

if let Some(tool_event) = event.downcast_ref::<ToolExecutedEvent>() {
    println!("Tool: {}", tool_event.tool_name);
    println!("Success: {}", tool_event.success);
    println!("Output length: {}", tool_event.output.len());
}
```

### Filtering Events

```rust
// Subscribe only to bash events
let (_id, mut rx) = bus.subscribe("tool.bash").await?;

// Subscribe only to successful executions
let (_id, mut rx) = bus.subscribe("tool.*.success").await?;

// Subscribe to specific tool
let (_id, mut rx) = bus.subscribe("tool.read_file").await?;
```

### Custom Event Handling

```rust
use tokio::spawn;

// Spawn event handler task
spawn(async move {
    let (_id, mut rx) = bus.subscribe("tool.*").await.unwrap();

    loop {
        if let Ok(event) = rx.recv().await {
            // Process event
            handle_tool_event(event);
        }
    }
});

fn handle_tool_event(event: DynEvent) {
    // Custom event handling logic
    // - Logging
    // - Metrics collection
    // - Audit trails
    // - Notification systems
}
```

---

## Best Practices

### Tool Design

#### DO: Provide Clear Descriptions

```rust
impl Tool for MyTool {
    fn description(&self) -> &str {
        "Calculate SHA-256 hash of a file"  // Clear and specific
    }
}
```

#### DON'T: Vague Descriptions

```rust
impl Tool for MyTool {
    fn description(&self) -> &str {
        "Process data"  // Too vague
    }
}
```

#### DO: Use Structured Output

```rust
fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let result = process_data();

    Ok(ToolOutput::with_structured(
        result.summary,
        json!({
            "details": result.details,
            "metrics": result.metrics,
            "timestamp":Utc::now()
        })
    ))
}
```

#### DON'T: Text-Only Output

```rust
fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let result = process_data();

    Ok(ToolOutput::text(result))  // No metadata
}
```

### Error Handling

#### DO: Provide Context

```rust
use anyhow::Context;

fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let path = get_path(&params)?;
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    Ok(ToolOutput::text(content))
}
```

#### DON'T: Generic Errors

```rust
fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let content = std::fs::read_to_string("file.txt")?;
    Ok(ToolOutput::text(content))
}
```

### Security

#### DO: Validate Paths

```rust
fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let path = resolve_path(ctx, get_path(&params)?)?;

    // Path is now validated to be within workspace
    let content = std::fs::read_to_string(&path)?;

    Ok(ToolOutput::text(content))
}
```

#### DON'T: Unvalidated Paths

```rust
fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let path = params.get("path").unwrap().as_str().unwrap();

    // No validation - potential security risk!
    let content = std::fs::read_to_string(path)?;

    Ok(ToolOutput::text(content))
}
```

### Performance

#### DO: Use Caching

```rust
// For read-only tools, caching is automatic
let result = executor.execute_cached_with_session(&call, None).await;
```

#### DON'T: Disable Caching Unnecessarily

```rust
// Always execute fresh (slow)
let result = executor.execute(&call);
```

#### DO: Apply Truncation

```rust
use rustycode_tools::truncation::{truncate_items, LIST_MAX_ITEMS};

let items = read_large_list();
let truncated = truncate_items(items, LIST_MAX_ITEMS, "results");
```

### Resource Management

#### DO: Respect Limits

```rust
// Apply reasonable timeouts
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "cargo test",
        "timeout_secs": 60  // Reasonable timeout
    }),
};
```

#### DON'T: Unlimited Resources

```rust
// No timeout - could hang forever
let call = ToolCall {
    name: "bash".to_string(),
    arguments: json!({
        "command": "cargo test"
        // No timeout!
    }),
};
```

---

## Testing Tools

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_execution() {
        let tool = MyCustomTool;
        let ctx = ToolContext::new("/workspace");
        let params = json!({
            "input": "test"
        });

        let result = tool.execute(params, &ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.text.contains("test"));
    }

    #[test]
    fn test_missing_parameter() {
        let tool = MyCustomTool;
        let ctx = ToolContext::new("/workspace");
        let params = json!({});  // Missing 'input'

        let result = tool.execute(params, &ctx);
        assert!(result.is_err());
    }
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_tool_in_registry() {
    let mut registry = ToolRegistry::new();
    registry.register(MyCustomTool);

    let call = ToolCall {
        call_id: "1".to_string(),
        name: "my_custom_tool".to_string(),
        arguments: json!({
            "input": "test"
        }),
    };

    let ctx = ToolContext::new("/workspace");
    let result = registry.execute(&call, &ctx);

    assert!(result.success);
    assert!(!result.output.is_empty());
}
```

### Testing Error Cases

```rust
#[test]
fn test_file_not_found() {
    let tool = ReadFileTool;
    let ctx = ToolContext::new("/workspace");
    let params = json!({
        "path": "nonexistent.txt"
    });

    let result = tool.execute(params, &ctx);

    // Should handle error gracefully
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("No such file"));
}
```

### Testing Permissions

```rust
#[test]
fn test_permission_check() {
    let ctx = ToolContext::new("/workspace")
        .with_max_permission(ToolPermission::Read);

    let tool = WriteFileTool;  // Requires Write permission

    // Should fail due to permission
    let result = tool.execute(
        json!({"path": "test.txt", "content": "test"}),
        &ctx
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("permission denied"));
}
```

---

## Common Patterns

### Tool Composition

```rust
// Tool that uses other tools
pub struct CompositeTool {
    executor: Arc<ToolExecutor>,
}

impl CompositeTool {
    pub fn new(executor: Arc<ToolExecutor>) -> Self {
        Self { executor }
    }
}

impl Tool for CompositeTool {
    fn name(&self) -> &str {
        "composite_tool"
    }

    fn description(&self) -> &str {
        "Combines multiple tool operations"
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["pattern"],
            "properties": {
                "pattern": { "type": "string" }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // Use glob to find files
        let glob_call = ToolCall {
            call_id: "1".to_string(),
            name: "glob".to_string(),
            arguments: json!({
                "pattern": params["pattern"]
            }),
        };

        let glob_result = self.executor.execute(&glob_call);

        // Process glob results...
        Ok(ToolOutput::text("Processed files"))
    }
}
```

### Async Tool Execution

```rust
use tokio::task::spawn_blocking;

// Execute tool in async context
async fn execute_tool_async(
    executor: &ToolExecutor,
    call: &ToolCall
) -> ToolResult {
    spawn_blocking({
        let executor = executor.clone();
        let call = call.clone();
        move || executor.execute(&call)
    })
    .await
    .unwrap()
}
```

### Batch Execution

```rust
use futures::stream::{self, StreamExt};

async fn execute_batch(
    executor: &ToolExecutor,
    calls: Vec<ToolCall>
) -> Vec<ToolResult> {
    stream::iter(calls)
        .map(|call| {
            let executor = executor.clone();
            async move {
                executor.execute_cached_with_session(&call, None).await
            }
        })
        .buffer_unordered(10)  // Max 10 concurrent
        .collect()
        .await
}
```

### Progress Tracking

```rust
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

let progress = Arc::new(AtomicUsize::new(0));
let total = files.len();

for (i, file) in files.iter().enumerate() {
    let call = ToolCall {
        call_id: format!("{}-{}", i, file),
        name: "read_file".to_string(),
        arguments: json!({
            "path": file
        }),
    };

    let result = executor.execute(&call);

    // Update progress
    progress.fetch_add(1, Ordering::Relaxed);
    println!("Progress: {}/{}", progress.load(Ordering::Relaxed), total);
}
```

---

## Migration Guide

### From Legacy Tool System

If you're migrating from an older tool system:

```rust
// Old way
let result = execute_tool("read_file", &["path", "file.txt"]);

// New way
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({
        "path": "file.txt"
    }),
};
let result = executor.execute(&call);
```

### Adding Event Publishing

```rust
// Old: No events
let result = executor.execute(&call);

// New: With events
let result = executor.execute_with_session(&call, Some(session_id));
```

### Enabling Caching

```rust
// Old: Always executes
let result = executor.execute(&call);

// New: With caching
let result = executor.execute_cached_with_session(&call, None).await;
```

---

## Troubleshooting

### Tool Not Found

**Problem:** Tool execution fails with "unknown tool"

**Solution:** Ensure tool is registered:

```rust
let mut registry = ToolRegistry::new();
registry.register(MyTool);  // Don't forget this!
```

### Permission Denied

**Problem:** Tool execution fails with "permission denied"

**Solution:** Check permission levels:

```rust
// Tool requires Write permission
impl Tool for MyTool {
    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }
}

// Session only allows Read permission
let ctx = ToolContext::new("/workspace")
    .with_max_permission(ToolPermission::Read);  // Too restrictive!
```

### Cache Not Working

**Problem:** Tools not being cached

**Solution:** Ensure tool is cacheable:

```rust
// Only read-only tools are cached
// Check is_cacheable_tool() function
// Read tools: read_file, list_dir, grep, glob, etc.
// Write/Execute tools are NOT cached: write_file, bash
```

### Rate Limit Errors

**Problem:** Too many requests

**Solution:** Adjust rate limiter:

```rust
use std::num::NonZeroU32;

let registry = ToolRegistry::with_rate_limiting(
    NonZeroU32::new(20).unwrap(),  // Increase rate
    NonZeroU32::new(40).unwrap()
);
```

---

## See Also

- [API_REFERENCE.md](./API_REFERENCE.md) - Complete API reference
- [EXAMPLES.md](./EXAMPLES.md) - Usage examples
- [TOOL_PERMISSIONS.md](./TOOL_PERMISSIONS.md) - Permission system details
- [TOOL_METADATA_SPEC.md](./TOOL_METADATA_SPEC.md) - Metadata specification
