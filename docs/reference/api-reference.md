# RustyCode API Reference

This document provides comprehensive API documentation for RustyCode's core crates, with examples and usage patterns.

## Table of Contents

- [rustycode-id API](#rustycode-id-api)
- [rustycode-bus API](#rustycode-bus-api)
- [rustycode-runtime API](#rustycode-runtime-api)
- [rustycode-tools API](#rustycode-tools-api)

---

## rustycode-id API

The `rustycode-id` crate provides time-sortable, compact identifiers.

### Overview

```rust
use rustycode_id::{SessionId, EventId, MemoryId};

// Create new IDs
let session_id = SessionId::new();
let event_id = EventId::new();
let memory_id = MemoryId::new();

// IDs are time-sortable
assert!(session_id < event_id, "earlier ID sorts first");

// Compact representation (58% smaller than UUIDs)
println!("Session ID: {}", session_id); // sess_000VDe7lKm28qQj4zH1c1
```

### Type-Safe ID Types

#### SessionId

```rust
use rustycode_id::SessionId;

/// Create a new session ID
let id = SessionId::new();

/// Parse from string
let id = SessionId::parse("sess_000VDe7lKm28qQj4zH1c1")?;

/// Get timestamp
let timestamp = id.timestamp();
println!("Created at: {}", timestamp);

/// Convert to string
let s = id.to_string();
assert!(s.starts_with("sess_"));

/// Default implementation
let id = SessionId::default();
```

#### EventId

```rust
use rustycode_id::EventId;

let id = EventId::new();
assert!(id.to_string().starts_with("evt_"));
```

#### MemoryId

```rust
use rustycode_id::MemoryId;

let id = MemoryId::new();
assert!(id.to_string().starts_with("mem_"));
```

#### SkillId

```rust
use rustycode_id::SkillId;

let id = SkillId::new();
assert!(id.to_string().starts_with("skl_"));
```

#### ToolId

```rust
use rustycode_id::ToolId;

let id = ToolId::new();
assert!(id.to_string().starts_with("tool_"));
```

#### FileId

```rust
use rustycode_id::FileId;

let id = FileId::new();
assert!(id.to_string().starts_with("file_"));
```

### SortableId (Low-Level API)

For advanced use cases, you can work with `SortableId` directly:

```rust
use rustycode_id::SortableId;
use chrono::{DateTime, Utc};

/// Create with custom prefix
let id = SortableId::new("custom_");

/// Create from components
let timestamp_ms = 1234567890000u64;
let random = 9876543210u64;
let id = SortableId::from_components("test_", timestamp_ms, random);

/// Parse from string
let id = SortableId::parse("test_000VDe7lKm28qQj4zH1c1")?;

/// Access components
let prefix = id.prefix();           // "test_"
let ts_ms = id.timestamp_ms();      // 1234567890000
let ts: DateTime<Utc> = id.timestamp();
let random = id.random();           // 9876543210

/// Convert to string
let s = id.to_string();
```

### Error Handling

```rust
use rustycode_id::{SessionId, IdError};

// Parse invalid format
let result = SessionId::parse("invalid_id");
match result {
    Err(IdError::InvalidFormat(msg)) => {
        eprintln!("Invalid format: {}", msg);
    }
    Err(IdError::InvalidPrefix { expected, found }) => {
        eprintln!("Wrong prefix: expected {}, got {}", expected, found);
    }
    _ => {}
}

// Wrong prefix
let result = SessionId::parse("evt_000VDe7lKm28qQj4zH1c1");
assert!(matches!(result, Err(IdError::InvalidPrefix { .. })));
```

### Serialization

All ID types support serde serialization:

```rust
use rustycode_id::SessionId;
use serde_json;

let id = SessionId::new();

// Serialize to JSON
let json = serde_json::to_string(&id)?;
println!("JSON: {}", json); // "sess_000VDe7lKm28qQj4zH1c1"

// Deserialize from JSON
let id2: SessionId = serde_json::from_str(&json)?;
assert_eq!(id.to_string(), id2.to_string());
```

---

## rustycode-bus API

The `rustycode-bus` crate provides type-safe, asynchronous event bus functionality.

### EventBus

#### Creating an Event Bus

```rust
use rustycode_bus::EventBus;

// Create with default configuration
let bus = EventBus::new();

// Create with custom configuration
let config = rustycode_bus::EventBusConfig {
    channel_capacity: 1000,
    max_subscribers: 500,
    debug_logging: true,
};
let bus = EventBus::with_config(config);
```

#### Publishing Events

```rust
use rustycode_bus::{EventBus, SessionStartedEvent};
use rustycode_id::SessionId;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = EventBus::new();

    // Create and publish an event
    let event = SessionStartedEvent::new(
        SessionId::new(),
        "Analyze codebase".to_string(),
        "Initial session".to_string(),
    );

    bus.publish(event).await?;
    Ok(())
}
```

#### Subscribing to Events

```rust
use rustycode_bus::EventBus;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = EventBus::new();

    // Subscribe to exact event type
    let (_id, mut rx) = bus.subscribe("session.started").await?;

    // Subscribe to wildcard pattern
    let (_id, mut rx) = bus.subscribe("session.*").await?;

    // Subscribe to all events
    let (_id, mut rx) = bus.subscribe("*").await?;

    // Spawn task to handle events
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("Received: {}", event.event_type());
        }
    });

    Ok(())
}
```

#### Event Types

##### SessionStartedEvent

```rust
use rustycode_bus::SessionStartedEvent;
use rustycode_id::SessionId;

let event = SessionStartedEvent::new(
    SessionId::new(),
    "Analyze codebase".to_string(),
    "Initial session".to_string(),
);

// Access event data
let session_id = event.session_id.clone();
let task = event.task.clone();
let metadata = event.metadata.clone();
```

##### ContextAssembledEvent

```rust
use rustycode_bus::ContextAssembledEvent;
use rustycode_id::SessionId;

let event = ContextAssembledEvent::new(
    SessionId::new(),
    "Read 50 files".to_string(),
    "Context assembled".to_string(),
);
```

##### PlanCreatedEvent

```rust
use rustycode_bus::PlanCreatedEvent;
use rustycode_id::SessionId;
use uuid::Uuid;

let plan_id = Uuid::new_v4();
let event = PlanCreatedEvent::new(
    SessionId::new(),
    plan_data.clone(),
    plan_id.to_string(),
);
```

##### PlanApprovedEvent

```rust
use rustycode_bus::PlanApprovedEvent;
use rustycode_id::SessionId;

let event = PlanApprovedEvent::new(
    SessionId::new(),
    "Plan approved".to_string(),
);
```

##### PlanRejectedEvent

```rust
use rustycode_bus::PlanRejectedEvent;
use rustycode_id::SessionId;

let event = PlanRejectedEvent::new(
    SessionId::new(),
    "Plan rejected".to_string(),
);
```

##### ToolExecutedEvent

```rust
use rustycode_bus::ToolExecutedEvent;
use rustycode_id::SessionId;
use serde_json::json;

let event = ToolExecutedEvent::new(
    SessionId::new(),
    "read_file".to_string(),
    json!({"path": "Cargo.toml"}),
    true,
    "Content read successfully".to_string(),
    None,
);
```

#### Hooks

```rust
use rustycode_bus::{EventBus, HookPhase};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = EventBus::new();

    // Register pre-publish hook
    bus.register_hook(HookPhase::PrePublish, |event| {
        tracing::info!("Before publishing: {}", event.event_type());
        Ok(())
    }).await;

    // Register post-publish hook
    bus.register_hook(HookPhase::PostPublish, |event| {
        tracing::info!("After publishing: {}", event.event_type());
        Ok(())
    }).await;

    // Register error hook
    bus.register_hook(HookPhase::OnError, |event| {
        tracing::error!("Error in event: {}", event.event_type());
        Ok(())
    }).await;

    Ok(())
}
```

#### Metrics

```rust
use rustycode_bus::EventBus;

let bus = EventBus::new();

// Get metrics snapshot
let metrics = bus.metrics();
println!("Events published: {}", metrics.events_published);
println!("Events delivered: {}", metrics.events_delivered);
println!("Events failed: {}", metrics.events_failed);
println!("Subscribers: {}", metrics.subscriber_count);
```

#### Subscription Handle

```rust
use rustycode_bus::{EventBus, SubscriptionHandle};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bus = Arc::new(EventBus::new());

    let (id, rx) = bus.subscribe("session.*").await?;
    let handle = SubscriptionHandle::new(id, bus.clone());

    // Handle auto-unsubscribes when dropped
    drop(handle);

    // Or manually unsubscribe
    handle.unsubscribe().await?;

    Ok(())
}
```

---

## rustycode-runtime API

The `rustycode-runtime` crate provides an async facade over the synchronous core runtime.

### AsyncRuntime

#### Creating a Runtime

```rust
use rustycode_runtime::AsyncRuntime;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load runtime from current directory
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    Ok(())
}
```

#### Running Tasks

```rust
use rustycode_runtime::AsyncRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    // Run a task
    let report = runtime.run(
        Path::new("."),
        "Inspect the codebase"
    ).await?;

    println!("Task: {}", report.session.task);
    println!("Files: {}", report.context_plan);

    Ok(())
}
```

#### Executing Tools

```rust
use rustycode_runtime::AsyncRuntime;
use rustycode_protocol::{ToolCall, SessionId};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    let session_id = SessionId::new();
    let call = ToolCall {
        call_id: "call-1".to_string(),
        name: "read_file".to_string(),
        arguments: json!({"path": "Cargo.toml"}),
    };

    let result = runtime.execute_tool(
        &session_id,
        call,
        Path::new("."),
    ).await?;

    if result.success {
        println!("Output: {}", result.output);
    } else {
        eprintln!("Error: {:?}", result.error);
    }

    Ok(())
}
```

#### Event Subscriptions

```rust
use rustycode_runtime::AsyncRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    // Subscribe to tool events
    let (_id, mut rx) = runtime.subscribe_events("tool.*").await?;

    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            println!("Tool event: {}", event.event_type());
        }
    });

    // Run task (will publish events)
    runtime.run(Path::new("."), "test task").await?;

    Ok(())
}
```

#### Planning

```rust
use rustycode_runtime::AsyncRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    // Start planning
    let report = runtime.start_planning(
        Path::new("."),
        "Refactor the codebase"
    ).await?;

    let session_id = &report.session.id;

    // Approve plan
    runtime.approve_plan(session_id).await?;

    // Or reject plan
    // runtime.reject_plan(session_id).await?;

    Ok(())
}
```

#### Configuration

```rust
use rustycode_runtime::AsyncRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    // Access configuration
    let config = runtime.config();
    println!("Data dir: {:?}", config.data_dir);
    println!("Skills dir: {:?}", config.skills_dir);

    Ok(())
}
```

#### Tool List

```rust
use rustycode_runtime::AsyncRuntime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = AsyncRuntime::load(Path::new(".")).await?;

    // List available tools
    let tools = runtime.tool_list();
    for tool in tools {
        println!("{}: {}", tool.name, tool.description);
    }

    Ok(())
}
```

---

## rustycode-tools API

The `rustycode-tools` crate provides both runtime and compile-time tool systems.

### Runtime Tool System

#### Tool Trait

```rust
use rustycode_tools::{Tool, ToolContext, ToolOutput};
use serde_json::Value;

struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "My custom tool"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            },
            "required": ["input"]
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        let input = params["input"].as_str().unwrap();
        Ok(ToolOutput::text(format!("Processed: {}", input)))
    }
}
```

#### ToolRegistry

```rust
use rustycode_tools::{ToolRegistry, Tool, ToolContext};
use rustycode_protocol::ToolCall;

// Create registry
let mut registry = ToolRegistry::new();

// Register custom tool
registry.register(MyTool);

// List tools
let tools = registry.list();
for tool in tools {
    println!("{}: {}", tool.name, tool.description);
}

// Execute tool
let call = ToolCall {
    call_id: "call-1".to_string(),
    name: "my_tool".to_string(),
    arguments: serde_json::json!({"input": "test"}),
};

let ctx = ToolContext::new(std::env::current_dir()?);
let result = registry.execute(&call, &ctx);
```

#### ToolExecutor

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use std::path::PathBuf;

let executor = ToolExecutor::new(PathBuf::from("."));

// List tools
let tools = executor.list();

// Execute tool
let call = ToolCall {
    call_id: "call-1".to_string(),
    name: "read_file".to_string(),
    arguments: serde_json::json!({"path": "Cargo.toml"}),
};

let result = executor.execute(&call);
```

### Compile-Time Tool System

#### ToolDispatcher

The compile-time system provides zero-cost abstraction with type safety:

```rust
use rustycode_tools::compile_time::*;
use std::path::PathBuf;

// Read file with compile-time type safety
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("Cargo.toml"),
    start_line: None,
    end_line: None,
})?;

println!("Read {} bytes from {}", result.bytes, result.path.display());

// This would NOT compile - wrong type!
// let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(
//     WriteFileInput { ... }
// );
```

#### CompileTimeReadFile

```rust
use rustycode_tools::compile_time::*;
use std::path::PathBuf;

// Basic usage
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("src/lib.rs"),
    start_line: None,
    end_line: None,
})?;

println!("Content:\n{}", result.content);

// Read specific line range
let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("src/lib.rs"),
    start_line: Some(10),
    end_line: Some(20),
})?;

println!("Lines 10-20:\n{}", result.content);
```

#### CompileTimeWriteFile

```rust
use rustycode_tools::compile_time::*;
use std::path::PathBuf;

// Write file
let result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(WriteFileInput {
    path: PathBuf::from("output.txt"),
    content: "Hello, World!".to_string(),
    create_parents: Some(false),
})?;

println!("Wrote {} bytes", result.bytes_written);

// Write with parent directory creation
let result = ToolDispatcher::<CompileTimeWriteFile>::dispatch(WriteFileInput {
    path: PathBuf::from("nested/dir/output.txt"),
    content: "Hello, World!".to_string(),
    create_parents: Some(true),
})?;
```

#### CompileTimeBash

```rust
use rustycode_tools::compile_time::*;

// Execute command
let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
    command: "echo".to_string(),
    args: Some(vec!["Hello".to_string(), "World".to_string()]),
    working_dir: None,
    timeout_secs: Some(5),
})?;

println!("Output: {}", result.stdout);
println!("Exit code: {}", result.exit_code);

// Execute with timeout
let result = ToolDispatcher::<CompileTimeBash>::dispatch(BashInput {
    command: "sleep".to_string(),
    args: Some(vec!["10".to_string()]),
    working_dir: None,
    timeout_secs: Some(1), // Timeout after 1 second
});

// This will return Err(BashError::Timeout(1))
```

#### Tool Metadata

```rust
use rustycode_tools::compile_time::*;

// Access metadata (const-evaluable)
assert_eq!(CompileTimeReadFile::METADATA.name, "read_file");
assert_eq!(CompileTimeReadFile::METADATA.permission, ToolPermission::Read);

assert_eq!(CompileTimeWriteFile::METADATA.name, "write_file");
assert_eq!(CompileTimeWriteFile::METADATA.permission, ToolPermission::Write);

assert_eq!(CompileTimeBash::METADATA.name, "bash");
assert_eq!(CompileTimeBash::METADATA.permission, ToolPermission::Execute);
```

### Performance Comparison

The compile-time system is significantly faster:

```rust
// Compile-time: ~5-10ns per call (monomorphized, inlined)
// Runtime: ~50-100ns per call (vtable lookup, JSON parsing)
// Speedup: 5-10x faster

// Benchmark compile-time dispatch
let start = std::time::Instant::now();
for _ in 0..100_000 {
    let _ = ToolDispatcher::<CompileTimeReadFile>::dispatch(input.clone());
}
let duration = start.elapsed();
println!("Compile-time: {:.2} ns/call", duration.as_nanos() / 100_000);
```

### Built-in Tools

#### ReadFileTool

```rust
use rustycode_tools::ReadFileTool;
use serde_json::json;

let tool = ReadFileTool;
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(
    json!({"path": "Cargo.toml"}),
    &ctx
)?;

println!("{}", result.text);
```

#### WriteFileTool

```rust
use rustycode_tools::WriteFileTool;

let tool = WriteFileTool;
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(
    json!({"path": "test.txt", "content": "Hello"}),
    &ctx
)?;
```

#### BashTool

```rust
use rustycode_tools::BashTool;

let tool = BashTool::default();
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(
    json!({"command": "echo", "args": ["Hello"]}),
    &ctx
)?;
```

#### GitStatusTool, GitDiffTool, GitLogTool, GitCommitTool

```rust
use rustycode_tools::{GitStatusTool, GitDiffTool, GitLogTool, GitCommitTool};

let tool = GitStatusTool;
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(json!({}), &ctx)?;
```

#### GrepTool, GlobTool

```rust
use rustycode_tools::{GrepTool, GlobTool};

// Search for pattern
let tool = GrepTool;
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(
    json!({"pattern": "TODO", "path": "."}),
    &ctx
)?;

// Glob files
let tool = GlobTool;
let result = tool.execute(
    json!({"pattern": "**/*.rs"}),
    &ctx
)?;
```

#### LspDiagnosticsTool

```rust
use rustycode_tools::LspDiagnosticsTool;

let tool = LspDiagnosticsTool;
let ctx = ToolContext::new(std::env::current_dir()?);

let result = tool.execute(
    json!({"path": "src/lib.rs"}),
    &ctx
)?;
```

---

## Common Patterns

### Error Handling

```rust
use anyhow::{Result, Context};

fn example() -> Result<()> {
    let id = rustycode_id::SessionId::new();
    let bus = rustycode_bus::EventBus::new();

    // Provide context for errors
    bus.publish(event)
        .context("failed to publish session started event")?;

    Ok(())
}
```

### Async Tasks

```rust
use tokio::task::JoinHandle;

fn spawn_task(bus: Arc<EventBus>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let (_id, mut rx) = bus.subscribe("session.*").await.unwrap();
        while let Ok(event) = rx.recv().await {
            println!("Event: {}", event.event_type());
        }
    })
}
```

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();
        // Test code here
    }

    #[test]
    fn test_session_id() {
        let id = SessionId::new();
        assert!(id.to_string().starts_with("sess_"));
    }
}
```

---

## Best Practices

1. **Use compile-time tools** when performance matters
2. **Subscribe to wildcard patterns** for flexibility
3. **Always handle errors** with proper context
4. **Use async/await** for I/O operations
5. **Document public APIs** with examples
6. **Write tests** for all functionality
7. **Run clippy** to catch common mistakes
8. **Format code** with cargo fmt before committing

For more information, see:
- [Developer Guide](developer-guide.md)
- [Architecture Overview](architecture.md)
- [Troubleshooting](troubleshooting.md)
# rustycode-tools API Reference

Complete API documentation for the rustycode-tools crate, providing tool interfaces, execution contexts, and system integration.

## Table of Contents

- [Core Types](#core-types)
- [Tool Trait](#tool-trait)
- [Built-in Tools](#built-in-tools)
- [Tool Execution](#tool-execution)
- [Caching System](#caching-system)
- [Permission System](#permission-system)
- [Error Handling](#error-handling)

---

## Core Types

### ToolContext

Runtime context passed to every tool invocation.

```rust
pub struct ToolContext {
    pub cwd: PathBuf,              // Current working directory
    pub sandbox: SandboxConfig,    // Sandbox configuration
    pub max_permission: ToolPermission, // Maximum allowed permission
}
```

**Methods:**

- `new(cwd: impl AsRef<Path>) -> Self` - Create context with default settings
- `with_sandbox(self, sandbox: SandboxConfig) -> Self` - Set sandbox configuration
- `with_max_permission(self, perm: ToolPermission) -> Self` - Set permission level

**Example:**

```rust
use rustycode_tools::ToolContext;
use std::path::PathBuf;

let ctx = ToolContext::new(PathBuf::from("/workspace"))
    .with_max_permission(ToolPermission::Write);
```

### ToolOutput

Output produced by a tool execution.

```rust
pub struct ToolOutput {
    pub text: String,                    // Plain-text result for model consumption
    pub structured: Option<serde_json::Value>, // Optional structured JSON
}
```

**Constructors:**

- `text(text: impl Into<String>) -> Self` - Create text-only output
- `with_structured(text: impl Into<String>, structured: Value) -> Self` - Create output with metadata

**Example:**

```rust
use rustycode_tools::ToolOutput;
use serde_json::json;

// Text-only output
let output = ToolOutput::text("File read successfully");

// With structured metadata
let output = ToolOutput::with_structured(
    "File read successfully",
    json!({
        "path": "/workspace/file.txt",
        "lines": 42,
        "bytes": 1024
    })
);
```

### ToolPermission

Permission level for tools (runtime version).

```rust
pub enum ToolPermission {
    None,      // No restrictions
    Read,      // Read-only filesystem access
    Write,     // Write filesystem access
    Execute,   // Execute commands
    Network,   // Network access
}
```

**Permission Hierarchy:** `None < Read < Write < Execute < Network`

---

## Tool Trait

The core trait that all tools must implement.

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn permission(&self) -> ToolPermission { ToolPermission::None }
    fn parameters_schema(&self) -> Value;
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput>;
}
```

### Required Methods

#### `name() -> &str`

Unique identifier for the tool (e.g., `"read_file"`, `"bash"`).

#### `description() -> &str`

Human-readable description of what the tool does.

#### `parameters_schema() -> Value`

JSON Schema describing the parameters the tool accepts.

**Example:**

```rust
fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "required": ["path"],
        "properties": {
            "path": {
                "type": "string",
                "description": "File path to read"
            },
            "start_line": {
                "type": "integer",
                "minimum": 1,
                "description": "First line to return (1-indexed)"
            }
        }
    })
}
```

#### `execute() -> Result<ToolOutput>`

Execute the tool with given parameters.

**Parameters:**
- `params: Value` - Tool arguments as JSON
- `ctx: &ToolContext` - Runtime context

**Returns:**
- `Ok(ToolOutput)` - Execution result
- `Err(anyhow::Error)` - Execution error

### Optional Methods

#### `permission() -> ToolPermission`

Default returns `ToolPermission::None`. Override to specify required permission level.

---

## Built-in Tools

### Filesystem Tools

#### ReadFileTool

**Tool Name:** `"read_file"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "required": ["path"],
  "properties": {
    "path": { "type": "string" },
    "start_line": { "type": "integer", "minimum": 1 },
    "end_line": { "type": "integer", "minimum": 1 }
  }
}
```

**Features:**
- Binary file detection with helpful error messages
- Automatic language detection
- Content hashing for caching (SHA-256)
- Smart truncation for large files
- Line range support

**Structured Output:**
```json
{
  "path": "/workspace/src/lib.rs",
  "total_bytes": 1024,
  "total_lines": 42,
  "shown_bytes": 1024,
  "binary": false,
  "language": "rust",
  "content_hash": "a1b2c3d4..."
}
```

#### WriteFileTool

**Tool Name:** `"write_file"`

**Permission:** `ToolPermission::Write`

**Parameters:**
```json
{
  "type": "object",
  "required": ["path", "content"],
  "properties": {
    "path": { "type": "string" },
    "content": { "type": "string" }
  }
}
```

**Structured Output:**
```json
{
  "path": "/workspace/src/lib.rs",
  "bytes": 1024,
  "lines": 42
}
```

#### ListDirTool

**Tool Name:** `"list_dir"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "path": { "type": "string" },
    "recursive": { "type": "boolean" },
    "max_depth": { "type": "integer" },
    "filter": { "type": "string" }
  }
}
```

**Filter Options:**
- `"file"` - List only files
- `"dir"` - List only directories
- `"all"` - List all entries
- `".ext"` - Filter by extension (e.g., `".rs"`)

### Search Tools

#### GrepTool

**Tool Name:** `"grep"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string" },
    "path": { "type": "string" },
    "before_context": { "type": "integer" },
    "after_context": { "type": "integer" },
    "max_matches_per_file": { "type": "integer" }
  }
}
```

**Features:**
- Regex pattern matching (compiled once)
- Context lines (before/after)
- Per-file match limits
- File-level statistics
- Smart truncation (GREP_MAX_MATCHES)

**Structured Output:**
```json
{
  "pattern": "TODO",
  "total_matches": 42,
  "files_with_matches": 5,
  "top_files": [
    { "path": "/workspace/src/lib.rs", "matches": 15 }
  ],
  "before_context": 2,
  "after_context": 2
}
```

#### GlobTool

**Tool Name:** `"glob"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string" }
  }
}
```

**Features:**
- Case-insensitive pattern matching
- Extension statistics
- Automatic skipping of `.git`, `target`, `node_modules`

**Structured Output:**
```json
{
  "pattern": "lib",
  "total_matches": 10,
  "extensions": [
    { "extension": "rs", "count": 5 },
    { "extension": "toml", "count": 2 }
  ]
}
```

### Shell Tool

#### BashTool

**Tool Name:** `"bash"`

**Permission:** `ToolPermission::Execute`

**Parameters:**
```json
{
  "type": "object",
  "required": ["command"],
  "properties": {
    "command": { "type": "string" },
    "cwd": { "type": "string" },
    "timeout_secs": { "type": "integer" },
    "transform": { "type": "string" }
  }
}
```

**Transform Options:**
- `"compact_git_status"` - Compact git status output
- `"test_summary"` - Summarize test results
- `"cargo_build"` - Summarize cargo build output
- `"lint_summary"` - Summarize lint output
- `"git_log"` - Compact git log
- `"docker_build"` - Summarize Docker builds
- `"npm_install"` - Summarize npm installs
- `"auto"` - Auto-detect appropriate transform

**Security Features:**
- Shell command parsing validation
- Blocked destructive binaries (rm, dd, mkfs, etc.)
- Dangerous flag detection (find -delete, find -exec)
- Fork bomb prevention
- Shell expansion blocking
- Workspace path enforcement

**Structured Output:**
```json
{
  "exit_code": 0,
  "command": "cargo test",
  "execution_time_ms": 1234,
  "timeout_secs": 30,
  "stdout": "...",
  "stderr": "..."
}
```

### Git Tools

#### GitStatusTool

**Tool Name:** `"git_status"`

**Permission:** `ToolPermission::Read`

**Structured Output:**
```json
{
  "branch": "main",
  "staged": ["src/lib.rs"],
  "modified": ["README.md"],
  "untracked": ["new_file.rs"],
  "has_changes": true
}
```

#### GitDiffTool

**Tool Name:** `"git_diff"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "staged": { "type": "boolean" },
    "path": { "type": "string" }
  }
}
```

**Structured Output:**
```json
{
  "staged": false,
  "files_changed": 3,
  "total_additions": 42,
  "total_deletions": 15,
  "changes": [
    {
      "path": "src/lib.rs",
      "additions": 30,
      "deletions": 5
    }
  ]
}
```

#### GitCommitTool

**Tool Name:** `"git_commit"`

**Permission:** `ToolPermission::Write`

**Parameters:**
```json
{
  "type": "object",
  "required": ["message"],
  "properties": {
    "message": { "type": "string" },
    "files": {
      "type": "array",
      "items": { "type": "string" }
    }
  }
}
```

**Structured Output:**
```json
{
  "commit_sha": "a1b2c3d4...",
  "staged_files": ["src/lib.rs"]
}
```

#### GitLogTool

**Tool Name:** `"git_log"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "limit": { "type": "integer" }
  }
}
```

**Structured Output:**
```json
{
  "commits": [
    {
      "sha": "a1b2c3d4",
      "message": "Add new feature"
    }
  ]
}
```

### LSP Tools

#### LspDiagnosticsTool

**Tool Name:** `"lsp_diagnostics"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "servers": {
      "type": "array",
      "items": { "type": "string" }
    }
  }
}
```

#### LspHoverTool

**Tool Name:** `"lsp_hover"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "required": ["file_path", "line", "character"],
  "properties": {
    "file_path": { "type": "string" },
    "line": { "type": "integer", "minimum": 0 },
    "character": { "type": "integer", "minimum": 0 },
    "language": { "type": "string" }
  }
}
```

#### LspDefinitionTool

**Tool Name:** `"lsp_definition"`

**Permission:** `ToolPermission::Read`

**Parameters:** Same as `lsp_hover`

#### LspCompletionTool

**Tool Name:** `"lsp_completion"`

**Permission:** `ToolPermission::Read`

**Parameters:**
```json
{
  "type": "object",
  "required": ["file_path", "line", "character"],
  "properties": {
    "file_path": { "type": "string" },
    "line": { "type": "integer", "minimum": 0 },
    "character": { "type": "integer", "minimum": 0 },
    "language": { "type": "string" },
    "trigger_character": { "type": "string" }
  }
}
```

### Web Tool

#### WebFetchTool

**Tool Name:** `"web_fetch"`

**Permission:** `ToolPermission::Network`

**Parameters:**
```json
{
  "type": "object",
  "required": ["url"],
  "properties": {
    "url": { "type": "string" },
    "convert_markdown": { "type": "boolean" }
  }
}
```

**Structured Output:**
```json
{
  "url": "https://example.com",
  "chars": 5000,
  "truncated": false,
  "converted": true,
  "status_code": 200,
  "time_to_first_byte_ms": 123,
  "total_time_ms": 456,
  "headers": {
    "content-type": "text/html"
  }
}
```

---

## Tool Execution

### ToolRegistry

Holds all registered tools and dispatches calls by name.

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    rate_limiter: Arc<RateLimiter>,
}
```

**Methods:**

#### `new() -> Self`

Create a new registry with default rate limiting.

#### `with_rate_limiting(max_per_second: NonZeroU32, max_burst: NonZeroU32) -> Self`

Create a registry with custom rate limiting.

#### `register(&mut self, tool: impl Tool + 'static)`

Register a new tool.

#### `list(&self) -> Vec<ToolInfo>`

List all registered tools.

#### `get(&self, name: &str) -> Option<&dyn Tool>`

Get a tool by name.

#### `execute(&self, call: &ToolCall, ctx: &ToolContext) -> ToolResult`

Execute a tool call.

**Example:**

```rust
use rustycode_tools::{ToolRegistry, ReadFileTool, ToolContext};
use rustycode_protocol::ToolCall;
use serde_json::json;
use std::path::PathBuf;

let mut registry = ToolRegistry::new();
registry.register(ReadFileTool);

let ctx = ToolContext::new(PathBuf::from("/workspace"));
let call = ToolCall {
    call_id: "1".to_string(),
    name: "read_file".to_string(),
    arguments: json!({ "path": "src/lib.rs" }),
};

let result = registry.execute(&call, &ctx);
assert!(result.success);
```

### ToolExecutor

High-level executor with caching and event bus integration.

```rust
pub struct ToolExecutor {
    registry: ToolRegistry,
    context: ToolContext,
    bus: Option<Arc<EventBus>>,
    cache: Arc<ToolCache>,
}
```

**Constructors:**

- `new(cwd: PathBuf) -> Self` - Basic executor
- `with_cache(cwd: PathBuf, cache_config: CacheConfig) -> Self` - With custom cache
- `with_event_bus(cwd: PathBuf, bus: Arc<EventBus>) -> Self` - With event bus
- `with_todo_state(cwd: PathBuf, todo_state: TodoState) -> Self` - With todo management

**Methods:**

- `list(&self) -> Vec<ToolInfo>` - List available tools
- `execute(&self, call: &ToolCall) -> ToolResult` - Execute without caching
- `execute_with_session(&self, call: &ToolCall, session_id: Option<SessionId>) -> ToolResult` - Execute with events
- `execute_cached_with_session(&self, call: &ToolCall, session_id: Option<SessionId>) -> ToolResult` - Execute with caching

**Example:**

```rust
use rustycode_tools::ToolExecutor;
use rustycode_bus::EventBus;
use std::path::PathBuf;
use std::sync::Arc;

// Basic executor
let executor = ToolExecutor::new(PathBuf::from("/workspace"));

// With event bus
let bus = Arc::new(EventBus::new());
let executor = ToolExecutor::with_event_bus(PathBuf::from("/workspace"), bus);
```

---

## Caching System

### ToolCache

Thread-safe LRU cache with file dependency tracking.

```rust
pub struct ToolCache {
    entries: Arc<RwLock<LruCache<CacheKey, CacheEntry>>>,
    config: CacheConfig,
    metrics: Arc<RwLock<CacheMetrics>>,
}
```

### CacheConfig

Cache configuration options.

```rust
pub struct CacheConfig {
    pub default_ttl: Duration,           // Default: 5 minutes
    pub max_entries: usize,              // Default: 1000
    pub track_file_dependencies: bool,   // Default: true
    pub max_memory_bytes: Option<usize>, // Default: 100 MB
    pub enable_metrics: bool,            // Default: true
}
```

### CacheKey

Fast cache key using u64 hashing.

```rust
pub struct CacheKey {
    pub tool_name: String,
    pub arguments_hash: u64,
}
```

### CacheMetrics

Performance metrics for monitoring.

```rust
pub struct CacheMetrics {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub total_puts: usize,
    pub current_memory_bytes: usize,
    pub current_entries: usize,
}
```

**Methods:**
- `hit_rate(&self) -> f64` - Calculate cache hit rate
- `avg_entry_size(&self) -> usize` - Calculate average entry size

### Cacheable Tools

Only read-only, idempotent tools are cached:
- `read_file`
- `list_dir`
- `grep`
- `glob`
- `git_status`
- `git_diff`
- `git_log`
- `lsp_diagnostics`
- `lsp_hover`
- `lsp_definition`
- `lsp_completion`
- `web_fetch`

---

## Permission System

### Permission Levels

Tools declare their required permission level:

```rust
fn permission(&self) -> ToolPermission {
    ToolPermission::Read
}
```

### Session-based Filtering

Tools are filtered by session mode:

- **Planning mode:** Only read tools allowed
- **Executing mode:** All tools allowed

### Permission Checking

```rust
// Check if tool is allowed in session mode
pub fn check_tool_permission(tool_name: &str, mode: SessionMode) -> bool;

// Get all tools allowed in a mode
pub fn get_allowed_tools(mode: SessionMode) -> Vec<String>;
```

**Example:**

```rust
use rustycode_protocol::SessionMode;
use rustycode_tools::check_tool_permission;

// In planning mode
assert!(check_tool_permission("read_file", SessionMode::Planning));
assert!(!check_tool_permission("bash", SessionMode::Planning));

// In executing mode
assert!(check_tool_permission("bash", SessionMode::Executing));
```

---

## Rate Limiting

### RateLimiter

Global rate limiter to prevent DoS attacks.

```rust
pub struct RateLimiter {
    global: GovernorRateLimiter,
    max_per_second: NonZeroU32,
    max_burst: NonZeroU32,
}
```

**Configuration:**
- Default: 10 requests/second, burst of 20
- Applied per directory (using cwd as key)

**Methods:**
- `new(max_per_second: NonZeroU32, max_burst: NonZeroU32) -> Self`
- `check_limit(&self, key: &str) -> Result<()>`
- `quota(&self) -> (NonZeroU32, NonZeroU32)`

**Example:**

```rust
use rustycode_tools::RateLimiter;
use std::num::NonZeroU32;

let limiter = RateLimiter::new(
    NonZeroU32::new(20).unwrap(),  // 20/sec
    NonZeroU32::new(40).unwrap(),  // burst of 40
);

limiter.check_limit("/workspace")?;
```

---

## Error Handling

### Error Types

Tools use `anyhow::Error` for error reporting:

```rust
use anyhow::{anyhow, Result};

fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
    let path = params.get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string parameter 'path'"))?;

    // ... tool logic ...

    Ok(ToolOutput::text("Success"))
}
```

### ToolResult

Execution result format for protocol integration.

```rust
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub structured: Option<Value>,
}
```

### Common Error Patterns

#### Missing Parameter

```rust
let path = required_string(&params, "path")?;
```

#### Validation Error

```rust
anyhow::ensure!(
    path.exists(),
    "Path '{}' does not exist",
    path.display()
);
```

#### Security Error

```rust
anyhow::bail!(
    "Path traversal blocked: '{}' is outside workspace",
    path.display()
);
```

---

## Security Features

### Path Validation

- Symlink detection and blocking
- Parent directory traversal prevention
- Workspace boundary enforcement
- Canonical path verification

### Command Safety

- Shell parsing validation
- Destructive binary blocking
- Dangerous flag detection
- Fork bomb prevention
- Shell expansion blocking

### File Type Detection

- Binary file detection
- Extension-based blocking
- Size limits
- Content validation

---

## Best Practices

### Tool Implementation

1. **Always validate input parameters**
2. **Use structured output for metadata**
3. **Provide helpful error messages**
4. **Declare appropriate permission levels**
5. **Handle edge cases gracefully**

### Performance

1. **Use caching for read-only operations**
2. **Apply truncation to large outputs**
3. **Pre-compile regex patterns**
4. **Minimize file I/O**
5. **Use async operations when possible**

### Security

1. **Validate all user input**
2. **Check path boundaries**
3. **Block dangerous operations**
4. **Use principle of least privilege**
5. **Audit tool permissions regularly**

---

## See Also

- [EXAMPLES.md](./EXAMPLES.md) - Usage examples
- [INTEGRATION.md](./INTEGRATION.md) - Integration guide
- [TOOL_PERMISSIONS.md](./TOOL_PERMISSIONS.md) - Permission system details
- [TOOL_METADATA_SPEC.md](./TOOL_METADATA_SPEC.md) - Metadata specification
