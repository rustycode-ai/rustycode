# rustycode-tools-api

Tool trait definitions and abstractions for RustyCode.

## Purpose

Provides trait definitions for tools without implementation details. This separation allows tool trait definitions to be used without pulling in the full tool execution engine, preventing circular dependencies between tool consumers and tool implementations.

## Key Types

- `Tool` — Core trait that all tools implement
- `ToolExecutor` — Trait for executing tools
- `ToolProfile` — Tool metadata (name, description, signature)
- `ToolRegistry` — Registry interface for discovering tools
- `ToolSelector` — Logic for selecting appropriate tools
- `ToolError`, `ToolResult` — Error and result types
- `ToolParameter` — Parameter definition with type and constraints

## Tool Trait

```rust
pub trait Tool: Send + Sync {
    /// Tool name (e.g., "Bash", "git", "file_read")
    fn name(&self) -> &str;
    
    /// Human-readable description
    fn description(&self) -> &str;
    
    /// Parameter definitions
    fn parameters(&self) -> Vec<ToolParameter>;
    
    /// Execute the tool
    async fn execute(&self, args: ToolArgs) -> ToolResult;
    
    /// Required permissions (file, network, etc.)
    fn required_permissions(&self) -> Vec<Permission>;
}
```

## Public API

```rust
use rustycode_tools_api::{Tool, ToolProfile, ToolParameter};

// Define a tool
pub struct MyTool;

impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    
    fn description(&self) -> &str { "Does something useful" }
    
    fn parameters(&self) -> Vec<ToolParameter> {
        vec![
            ToolParameter {
                name: "input".to_string(),
                param_type: "string".to_string(),
                description: "Input data".to_string(),
                required: true,
            }
        ]
    }
    
    async fn execute(&self, args: ToolArgs) -> ToolResult {
        // Implementation here
        Ok("output".to_string())
    }
    
    fn required_permissions(&self) -> Vec<Permission> {
        vec![Permission::FileRead]
    }
}
```

## Traits

- `Tool` — Individual tool implementation
- `ToolExecutor` — Trait for tool execution engines
- `ToolRegistry` — Trait for tool discovery
- `ToolSelector` — Trait for tool selection logic
- `PermissionValidator` — Trait for checking tool permissions

## Error Handling

```rust
pub enum ToolError {
    ExecutionFailed(String),
    PermissionDenied(String),
    InvalidArguments(String),
    NotFound(String),
    Timeout,
}

pub type ToolResult<T> = Result<T, ToolError>;
```

## Dependencies

Minimal by design:
- `serde` — Serialization of parameters
- `anyhow` — Error handling
- (No async runtime — let implementers choose)

## Architecture Notes

This crate defines only the contract. Implementation is in:
- `rustycode-tools` — Actual tool implementations
- `rustycode-tools-registry` — Discovery and registration
- `rustycode-guard` — Security validation

Separation allows:
- Consumers to depend on `-api` without pulling in all implementations
- Multiple implementations of tool traits
- Custom tool implementations outside the workspace
- Avoiding circular dependencies

## Testing

Tests verify trait implementation patterns and error handling. Implementations are tested in their respective crates.

## See Also

- `rustycode-tools` — Tool implementations
- `rustycode-tools-registry` — Tool discovery
- `rustycode-guard` — Security checks
