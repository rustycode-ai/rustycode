# rustycode-tools Integration Guide

**Version:** 0.1.0
**Last Updated:** 2025-03-14

Comprehensive guide for integrating rustycode-tools with LLM providers, building custom tool systems, and optimizing performance.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [LLM Provider Integration](#llm-provider-integration)
- [Tool Selection Workflow](#tool-selection-workflow)
- [Metadata Propagation](#metadata-propagation)
- [Caching Strategy](#caching-strategy)
- [Permission Management](#permission-management)
- [Event Bus Integration](#event-bus-integration)
- [Performance Optimization](#performance-optimization)
- [Security Considerations](#security-considerations)
- [Testing Strategy](#testing-strategy)
- [Deployment](#deployment)

---

## Overview

rustycode-tools provides a comprehensive tool execution framework designed for AI code assistance. This guide covers integration patterns for:

- **LLM Providers** - OpenAI, Anthropic, Google, etc.
- **Agent Systems** - Multi-agent orchestration
- **CLI Tools** - Command-line interfaces
- **Web Services** - HTTP APIs
- **IDE Extensions** - Editor integrations

### Key Features

- **Type Safety** - Compile-time and runtime tool interfaces
- **Security** - Sandbox, rate limiting, path validation
- **Performance** - Caching, zero-cost abstractions
- **Extensibility** - Plugin system, custom tools
- **Observability** - Metrics, audit logs, event bus

---

## Architecture

### Core Components

```
┌─────────────────────────────────────────────────────────────┐
│                     Application Layer                        │
│  (LLM Provider, Agent System, CLI, Web Service, etc.)        │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│                   ToolExecutor                               │
│  - Tool Registry                                            │
│  - Caching Layer                                             │
│  - Rate Limiting                                             │
│  - Event Bus Integration                                     │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│                   Tool Implementation                        │
│  - Built-in Tools (File, Search, Bash, Git, LSP, Web)       │
│  - Custom Tools                                             │
│  - Plugins                                                  │
│  - Compile-Time Tools                                       │
└────────────────────────┬────────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────────┐
│                   Security Layer                             │
│  - Path Validation                                          │
│  - Permission Checking                                       │
│  - Rate Limiting                                             │
│  - Sandbox Constraints                                       │
└─────────────────────────────────────────────────────────────┘
```

### Data Flow

```
Request → ToolCall → Permission Check → Rate Limit → Cache Check
                                           ↓
                                    Tool Execution
                                           ↓
                            Security Validation
                                           ↓
                              Result Processing
                                           ↓
                            Metadata Generation
                                           ↓
                            Cache Update (if applicable)
                                           ↓
                            Event Publishing (if configured)
                                           ↓
                                  ToolResult
```

---

## LLM Provider Integration

### OpenAI Integration

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::{ToolCall, ToolResult};
use serde_json::json;

pub struct OpenAIToolIntegration {
    executor: ToolExecutor,
}

impl OpenAIToolIntegration {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self {
            executor: ToolExecutor::new(workspace),
        }
    }

    /// Convert rustycode tools to OpenAI function calling format
    pub fn to_openai_functions(&self) -> Vec<serde_json::Value> {
        self.executor
            .list()
            .into_iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters_schema
                    }
                })
            })
            .collect()
    }

    /// Execute tool call from OpenAI response
    pub fn execute_openai_call(&self, call: &ToolCall) -> ToolResult {
        self.executor.execute(call)
    }

    /// Format tool result for OpenAI
    pub fn format_for_openai(&self, result: &ToolResult) -> String {
        if result.success {
            result.output.clone()
        } else {
            format!("Error: {}", result.error.as_ref().unwrap_or(&"Unknown error".to_string()))
        }
    }
}

// Usage example
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let integration = OpenAIToolIntegration::new(std::path::PathBuf::from("."));

    // Get tools for OpenAI API
    let functions = integration.to_openai_functions();

    // Make OpenAI API call with functions
    let openai_response = openai_client
        .chat()
        .add_functions(functions)
        .messages(messages)
        .await?;

    // Execute tool calls from response
    if let Some(tool_calls) = openai_response.tool_calls {
        for tool_call in tool_calls {
            let call = ToolCall {
                call_id: tool_call.id,
                name: tool_call.function.name,
                arguments: serde_json::from_str(&tool_call.function.arguments)?,
            };

            let result = integration.execute_openai_call(&call);
            let formatted = integration.format_for_openai(&result);

            // Send result back to OpenAI
            openai_client.send_tool_result(tool_call.id, formatted).await?;
        }
    }

    Ok(())
}
```

### Anthropic Integration

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;

pub struct AnthropicToolIntegration {
    executor: ToolExecutor,
}

impl AnthropicToolIntegration {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self {
            executor: ToolExecutor::new(workspace),
        }
    }

    /// Convert rustycode tools to Anthropic tool format
    pub fn to_anthropic_tools(&self) -> Vec<serde_json::Value> {
        self.executor
            .list()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters_schema
                })
            })
            .collect()
    }

    /// Execute tool call from Anthropic response
    pub fn execute_anthropic_call(&self, call: &ToolCall) -> ToolResult {
        self.executor.execute(call)
    }

    /// Format tool result for Anthropic with metadata
    pub fn format_for_anthropic(&self, result: &ToolResult) -> String {
        let mut output = String::new();

        if result.success {
            output.push_str(&result.output);

            // Append metadata for LLM context
            if let Some(metadata) = &result.structured {
                output.push_str("\n\n<metadata>\n");
                output.push_str(&serde_json::to_string_pretty(metadata).unwrap());
                output.push_str("\n</metadata>");
            }
        } else {
            output.push_str(&format!(
                "Tool execution failed: {}",
                result.error.as_ref().unwrap_or(&"Unknown error".to_string())
            ));
        }

        output
    }
}
```

### Generic LLM Adapter

```rust
use rustycode_tools::ToolExecutor;
use serde_json::Value;

pub struct LLMToolAdapter {
    executor: ToolExecutor,
}

impl LLMToolAdapter {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self {
            executor: ToolExecutor::new(workspace),
        }
    }

    /// Get tool definitions for any LLM provider
    pub fn get_tool_definitions(&self) -> Vec<Value> {
        self.executor
            .list()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters_schema,
                    "permission": format!("{:?}", tool.permission)
                })
            })
            .collect()
    }

    /// Execute tool call and return standardized result
    pub fn execute_tool(&self, call: &ToolCall) -> anyhow::Result<ToolExecutionResult> {
        let result = self.executor.execute(call);

        Ok(ToolExecutionResult {
            tool_name: result.name.clone(),
            success: result.success,
            output: result.output,
            error: result.error,
            metadata: result.structured,
            execution_id: uuid::Uuid::new_v4().to_string(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolExecutionResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub metadata: Option<Value>,
    pub execution_id: String,
}
```

---

## Tool Selection Workflow

### Intelligent Tool Selection

```rust
use rustycode_tools::{ToolExecutor, ToolInfo};
use rustycode_protocol::ToolCall;

pub struct ToolSelector {
    executor: ToolExecutor,
    usage_history: std::collections::HashMap<String, usize>,
}

impl ToolSelector {
    pub fn new(executor: ToolExecutor) -> Self {
        Self {
            executor,
            usage_history: std::collections::HashMap::new(),
        }
    }

    /// Select appropriate tools based on user query
    pub fn select_tools_for_query(&self, query: &str) -> Vec<String> {
        let all_tools = self.executor.list();
        let mut selected = Vec::new();
        let query_lower = query.to_lowercase();

        // Keyword-based selection
        for tool in all_tools {
            if self.should_include_tool(&tool, &query_lower) {
                selected.push(tool.name.clone());
            }
        }

        // Sort by usage frequency (most used first)
        selected.sort_by(|a, b| {
            let count_a = self.usage_history.get(a).unwrap_or(&0);
            let count_b = self.usage_history.get(b).unwrap_or(&0);
            count_b.cmp(count_a)
        });

        selected
    }

    fn should_include_tool(&self, tool: &ToolInfo, query: &str) -> bool {
        let tool_desc = tool.description.to_lowercase();
        let tool_name = tool.name.to_lowercase();

        // File operations
        if query.contains("file") || query.contains("read") || query.contains("write") {
            return tool_name.contains("file") || tool_name.contains("dir");
        }

        // Search operations
        if query.contains("search") || query.contains("find") || query.contains("grep") {
            return tool_name.contains("grep") || tool_name.contains("glob");
        }

        // Command execution
        if query.contains("run") || query.contains("execute") || query.contains("command") {
            return tool_name.contains("bash");
        }

        // Git operations
        if query.contains("git") || query.contains("commit") || query.contains("diff") {
            return tool_name.starts_with("git_");
        }

        // Web operations
        if query.contains("http") || query.contains("fetch") || query.contains("url") {
            return tool_name.contains("web") || tool_name.contains("fetch");
        }

        false
    }

    /// Record tool usage for learning
    pub fn record_usage(&mut self, tool_name: String) {
        *self.usage_history.entry(tool_name).or_insert(0) += 1;
    }
}
```

### Context-Aware Tool Filtering

```rust
use rustycode_tools::{ToolExecutor, ToolPermission};
use rustycode_protocol::SessionMode;

pub struct ContextAwareToolFilter {
    executor: ToolExecutor,
}

impl ContextAwareToolFilter {
    pub fn new(executor: ToolExecutor) -> Self {
        Self { executor }
    }

    /// Get tools allowed in current context
    pub fn get_allowed_tools(&self, mode: SessionMode) -> Vec<String> {
        rustycode_tools::get_allowed_tools(mode)
    }

    /// Filter tools based on project context
    pub fn filter_for_context(&self, context: &ProjectContext) -> Vec<String> {
        let all_tools = self.executor.list();
        let mut filtered = Vec::new();

        for tool in all_tools {
            if self.is_relevant_for_context(&tool, context) {
                filtered.push(tool.name);
            }
        }

        filtered
    }

    fn is_relevant_for_context(&self, tool: &ToolInfo, context: &ProjectContext) -> bool {
        match context.project_type {
            ProjectType::Rust => {
                // Rust projects: include cargo/build tools
                tool.name.contains("file") || tool.name.contains("grep")
            }
            ProjectType::JavaScript => {
                // JS projects: include npm/node tools
                tool.name.contains("file") || tool.name.contains("bash")
            }
            ProjectType::Generic => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub project_type: ProjectType,
    pub has_git: bool,
    pub has_tests: bool,
}

#[derive(Debug, Clone)]
pub enum ProjectType {
    Rust,
    JavaScript,
    Python,
    Generic,
}
```

---

## Metadata Propagation

### Metadata Collection Pipeline

```rust
use rustycode_tools::{ToolExecutor, AuditLogEntry};
use rustycode_protocol::ToolCall;
use std::sync::Arc;
use std::time::SystemTime;

pub struct MetadataCollector {
    executor: ToolExecutor,
    audit_log: Arc<std::sync::Mutex<Vec<AuditLogEntry>>>,
}

impl MetadataCollector {
    pub fn new(executor: ToolExecutor) -> Self {
        Self {
            executor,
            audit_log: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    /// Execute tool and collect comprehensive metadata
    pub fn execute_with_metadata(&self, call: &ToolCall) -> ToolExecutionWithMetadata {
        let start_time = SystemTime::now();

        // Execute tool
        let result = self.executor.execute(call);

        let duration = start_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();

        // Create audit log entry
        let audit_entry = AuditLogEntry::new(
            result.name.clone(),
            Some(duration as u128),
            result.success,
            result.error.clone(),
            result.output.len(),
        );

        // Store in audit log
        self.audit_log.lock().unwrap().push(audit_entry.clone());

        ToolExecutionWithMetadata {
            result,
            metadata: ExecutionMetadata {
                duration_ms: duration as u64,
                timestamp: SystemTime::now(),
                output_size: result.output.len(),
                cached: false, // Could be detected from cache
            },
            audit_entry,
        }
    }

    pub fn get_audit_log(&self) -> Vec<AuditLogEntry> {
        self.audit_log.lock().unwrap().clone()
    }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionWithMetadata {
    pub result: rustycode_protocol::ToolResult,
    pub metadata: ExecutionMetadata,
    pub audit_entry: AuditLogEntry,
}

#[derive(Debug, Clone)]
pub struct ExecutionMetadata {
    pub duration_ms: u64,
    pub timestamp: SystemTime,
    pub output_size: usize,
    pub cached: bool,
}
```

### Metadata for LLM Context

```rust
use serde_json::{json, Value};

pub struct LLMMetadataFormatter;

impl LLMMetadataFormatter {
    /// Format tool result with metadata for LLM consumption
    pub fn format_for_llm(
        result: &rustycode_protocol::ToolResult,
        metadata: &ExecutionMetadata,
    ) -> String {
        let mut formatted = String::new();

        // Add tool result
        formatted.push_str(&result.output);

        // Add metadata block
        formatted.push_str("\n\n<tool_metadata>\n");
        formatted.push_str(&format!("tool: {}\n", result.name));
        formatted.push_str(&format!("success: {}\n", result.success));
        formatted.push_str(&format!("duration_ms: {}\n", metadata.duration_ms));
        formatted.push_str(&format!("output_size: {}\n", metadata.output_size));
        formatted.push_str(&format!("cached: {}\n", metadata.cached));

        if let Some(structured) = &result.structured {
            formatted.push_str(&format!("structured_data: {}\n",
                serde_json::to_string_pretty(structured).unwrap()));
        }

        formatted.push_str("</tool_metadata>");

        formatted
    }

    /// Extract key insights from metadata
    pub fn extract_insights(metadata: &Value) -> Vec<String> {
        let mut insights = Vec::new();

        // Check for truncation
        if metadata.get("truncated").and_then(|v| v.as_bool()).unwrap_or(false) {
            insights.push("Output was truncated due to size limits".to_string());
        }

        // Check for binary files
        if metadata.get("binary").and_then(|v| v.as_bool()).unwrap_or(false) {
            insights.push("File is binary and cannot be displayed".to_string());
        }

        // Check for errors
        if let Some(error) = metadata.get("error") {
            insights.push(format!("Error occurred: {}", error.as_str().unwrap_or("Unknown")));
        }

        // Check for high match counts
        if let Some(total_matches) = metadata.get("total_matches") {
            if let Some(count) = total_matches.as_u64() {
                if count > 1000 {
                    insights.push(format!("High number of matches: {}", count));
                }
            }
        }

        insights
    }
}
```

---

## Caching Strategy

### Cache Configuration for LLM Workloads

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};
use std::time::Duration;

pub struct LLMCacheStrategy;

impl LLMCacheStrategy {
    /// Configure cache for optimal LLM performance
    pub fn configure_for_llm() -> CacheConfig {
        CacheConfig {
            // Longer TTL for LLM workloads (10 minutes)
            default_ttl: Duration::from_secs(600),

            // Larger cache for LLM context
            max_entries: 5000,

            // Enable file dependency tracking
            track_file_dependencies: true,

            // 500MB memory limit for large contexts
            max_memory_bytes: Some(500 * 1024 * 1024),

            // Enable metrics for monitoring
            enable_metrics: true,
        }
    }

    /// Create executor optimized for LLM workloads
    pub fn create_llm_executor(workspace: std::path::PathBuf) -> ToolExecutor {
        ToolExecutor::with_cache(
            workspace,
            Self::configure_for_llm(),
        )
    }
}
```

### Cache-Aware Tool Selection

```rust
use rustycode_tools::{ToolExecutor, is_cacheable_tool};

pub struct CacheAwareExecutor {
    executor: ToolExecutor,
}

impl CacheAwareExecutor {
    pub fn new(executor: ToolExecutor) -> Self {
        Self { executor }
    }

    /// Prioritize cacheable tools for performance
    pub async fn execute_optimized(&self, call: &rustycode_protocol::ToolCall) -> rustycode_protocol::ToolResult {
        if is_cacheable_tool(&call.name) {
            // Use cached execution
            self.executor.execute_cached_with_session(call, None).await
        } else {
            // Direct execution for non-cacheable tools
            self.executor.execute(call)
        }
    }

    /// Batch execute cacheable tools
    pub async fn batch_execute_cached(&self, calls: Vec<rustycode_protocol::ToolCall>) -> Vec<rustycode_protocol::ToolResult> {
        let futures: Vec<_> = calls
            .into_iter()
            .filter(|call| is_cacheable_tool(&call.name))
            .map(|call| {
                self.executor.execute_cached_with_session(&call, None)
            })
            .collect();

        futures::future::join_all(futures).await
    }
}
```

---

## Permission Management

### Permission-Based Tool Filtering

```rust
use rustycode_tools::{ToolExecutor, ToolPermission};
use rustycode_protocol::{SessionMode, SessionConfig};

pub struct PermissionManager {
    executor: ToolExecutor,
}

impl PermissionManager {
    pub fn new(executor: ToolExecutor) -> Self {
        Self { executor }
    }

    /// Get tools allowed for given session mode
    pub fn get_session_tools(&self, mode: SessionMode) -> Vec<String> {
        rustycode_tools::get_allowed_tools(mode)
    }

    /// Check if tool execution is allowed
    pub fn check_permission(&self, tool_name: &str, mode: SessionMode) -> bool {
        rustycode_tools::check_tool_permission(tool_name, mode)
    }

    /// Create permission-aware context
    pub fn create_context(&self, mode: SessionMode, workspace: std::path::PathBuf) -> rustycode_tools::ToolContext {
        let max_permission = match mode {
            SessionMode::Planning => ToolPermission::Read,
            SessionMode::Executing => ToolPermission::Network,
        };

        rustycode_tools::ToolContext::new(workspace)
            .with_max_permission(max_permission)
    }
}
```

### Dynamic Permission Adjustment

```rust
pub struct AdaptivePermissionManager {
    executor: ToolExecutor,
    trust_level: f64, // 0.0 to 1.0
}

impl AdaptivePermissionManager {
    pub fn new(executor: ToolExecutor) -> Self {
        Self {
            executor,
            trust_level: 0.5, // Start with medium trust
        }
    }

    /// Adjust trust based on behavior
    pub fn adjust_trust(&mut self, successful_executions: usize, total_executions: usize) {
        let success_rate = successful_executions as f64 / total_executions as f64;

        // Increase trust if success rate > 90%
        if success_rate > 0.9 {
            self.trust_level = (self.trust_level + 0.1).min(1.0);
        }
        // Decrease trust if success rate < 50%
        else if success_rate < 0.5 {
            self.trust_level = (self.trust_level - 0.2).max(0.0);
        }
    }

    /// Get max permission based on trust level
    pub fn get_max_permission(&self) -> ToolPermission {
        match self.trust_level {
            x if x >= 0.8 => ToolPermission::Network,
            x if x >= 0.5 => ToolPermission::Execute,
            x if x >= 0.2 => ToolPermission::Write,
            _ => ToolPermission::Read,
        }
    }
}
```

---

## Event Bus Integration

### Tool Execution Events

```rust
use rustycode_bus::{EventBus, ToolExecutedEvent};
use rustycode_tools::ToolExecutor;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create event bus
    let bus = Arc::new(EventBus::new());

    // Create executor with event bus
    let executor = ToolExecutor::with_event_bus(
        std::path::PathBuf::from("."),
        bus.clone(),
    );

    // Subscribe to tool execution events
    let (_id, mut rx) = bus.subscribe("tool.*").await?;

    // Spawn event handler
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match event {
                rustycode_bus::BusEvent::ToolExecuted(e) => {
                    println!("Tool {} executed with success={}",
                        e.tool_name, e.success);
                }
                _ => {}
            }
        }
    });

    // Execute tools - events will be published automatically
    let call = rustycode_protocol::ToolCall {
        call_id: "1".to_string(),
        name: "read_file".to_string(),
        arguments: serde_json::json!({"path": "Cargo.toml"}),
    };

    executor.execute_with_session(&call, Some("session-123".to_string()));

    Ok(())
}
```

### Custom Event Handlers

```rust
use rustycode_bus::{BusEvent, ToolExecutedEvent};
use std::sync::Arc;

pub struct MetricsCollector {
    tool_usage: std::collections::HashMap<String, usize>,
    total_executions: usize,
    successful_executions: usize,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            tool_usage: std::collections::HashMap::new(),
            total_executions: 0,
            successful_executions: 0,
        }
    }

    pub async fn handle_event(&mut self, event: BusEvent) {
        if let BusEvent::ToolExecuted(e) = event {
            self.total_executions += 1;
            if e.success {
                self.successful_executions += 1;
            }

            *self.tool_usage.entry(e.tool_name).or_insert(0) += 1;
        }
    }

    pub fn get_success_rate(&self) -> f64 {
        if self.total_executions == 0 {
            1.0
        } else {
            self.successful_executions as f64 / self.total_executions as f64
        }
    }
}
```

---

## Performance Optimization

### Parallel Tool Execution

```rust
use rustycode_tools::ToolExecutor;
use rustycode_protocol::ToolCall;
use std::sync::Arc;

pub struct ParallelExecutor {
    executor: Arc<ToolExecutor>,
    max_parallel: usize,
}

impl ParallelExecutor {
    pub fn new(executor: ToolExecutor, max_parallel: usize) -> Self {
        Self {
            executor: Arc::new(executor),
            max_parallel,
        }
    }

    /// Execute multiple tools in parallel
    pub async fn execute_parallel(&self, calls: Vec<ToolCall>) -> Vec<rustycode_protocol::ToolResult> {
        use futures::stream::{self, StreamExt};

        stream::iter(calls)
            .map(|call| {
                let executor = self.executor.clone();
                async move {
                    executor.execute_with_session(&call, None)
                }
            })
            .buffer_unordered(self.max_parallel)
            .collect()
            .await
    }
}
```

### Compile-Time Tool Optimization

```rust
use rustycode_tools::compile_time::*;

/// Use compile-time tools for maximum performance
pub fn fast_file_processing() -> anyhow::Result<()> {
    // Zero-cost abstraction - no runtime overhead
    let result = ToolDispatcher::<CompileTimeReadFile>::dispatch(ReadFileInput {
        path: std::path::PathBuf::from("Cargo.toml"),
        start_line: None,
        end_line: None,
    })?;

    println!("Read {} lines in {} bytes",
        result.line_count, result.byte_count);

    Ok(())
}
```

---

## Security Considerations

### Sandbox Configuration

```rust
use rustycode_tools::{ToolExecutor, SandboxConfig};

pub fn create_secure_executor(workspace: std::path::PathBuf) -> ToolExecutor {
    let sandbox = SandboxConfig::new()
        .allow_path(workspace.clone())
        .deny_path("/etc")
        .deny_path("/sys")
        .deny_path("/proc")
        .timeout(30)
        .max_output(10 * 1024 * 1024); // 10MB

    // Apply sandbox to context
    let ctx = rustycode_tools::ToolContext::new(workspace)
        .with_sandbox(sandbox);

    // Create executor with secure context
    ToolExecutor::new(ctx.cwd)
}
```

### Input Validation

```rust
use rustycode_tools::ToolExecutor;

pub struct SecureExecutor {
    executor: ToolExecutor,
}

impl SecureExecutor {
    pub fn new(executor: ToolExecutor) -> Self {
        Self { executor }
    }

    /// Validate and execute tool call
    pub fn execute_validated(&self, call: &rustycode_protocol::ToolCall) -> anyhow::Result<rustycode_protocol::ToolResult> {
        // Validate tool name
        if !self.is_safe_tool(&call.name) {
            return Err(anyhow::anyhow!("Tool not allowed: {}", call.name));
        }

        // Validate arguments
        self.validate_arguments(&call.name, &call.arguments)?;

        // Execute
        Ok(self.executor.execute(call))
    }

    fn is_safe_tool(&self, tool_name: &str) -> bool {
        // Whitelist of allowed tools
        matches!(tool_name,
            "read_file" | "list_dir" | "grep" | "glob" | "git_status"
        )
    }

    fn validate_arguments(&self, tool_name: &str, args: &serde_json::Value) -> anyhow::Result<()> {
        match tool_name {
            "read_file" => {
                if let Some(path) = args.get("path") {
                    let path_str = path.as_str().ok_or_else(|| anyhow::anyhow!("Invalid path"))?;
                    if path_str.contains("..") {
                        return Err(anyhow::anyhow!("Path traversal detected"));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
```

---

## Testing Strategy

### Integration Testing

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use rustycode_tools::ToolExecutor;
    use rustycode_protocol::ToolCall;
    use serde_json::json;

    #[tokio::test]
    async fn test_tool_execution_flow() {
        let executor = ToolExecutor::new(std::path::PathBuf::from("."));

        let call = ToolCall {
            call_id: "1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "Cargo.toml"}),
        };

        let result = executor.execute(&call);

        assert!(result.success);
        assert!(result.output.contains("[package]"));
    }

    #[tokio::test]
    async fn test_cache_hit() {
        let cache_config = rustycode_tools::CacheConfig {
            default_ttl: std::time::Duration::from_secs(60),
            ..Default::default()
        };

        let executor = rustycode_tools::ToolExecutor::with_cache(
            std::path::PathBuf::from("."),
            cache_config,
        );

        let call = ToolCall {
            call_id: "1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "Cargo.toml"}),
        };

        // First call - cache miss
        let result1 = executor.execute_cached_with_session(&call, None).await;

        // Second call - cache hit
        let result2 = executor.execute_cached_with_session(&call, None).await;

        assert_eq!(result1.output, result2.output);
    }
}
```

---

## Deployment

### Production Configuration

```rust
use rustycode_tools::{ToolExecutor, CacheConfig};

pub fn create_production_executor(workspace: std::path::PathBuf) -> ToolExecutor {
    let cache_config = CacheConfig {
        default_ttl: std::time::Duration::from_secs(300), // 5 minutes
        max_entries: 10000,
        track_file_dependencies: true,
        max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
        enable_metrics: true,
    };

    ToolExecutor::with_cache(workspace, cache_config)
}
```

### Monitoring Setup

```rust
use rustycode_tools::ToolExecutor;

pub struct MonitoredExecutor {
    executor: ToolExecutor,
    metrics: Arc<std::sync::Mutex<ExecutionMetrics>>,
}

#[derive(Debug, Default)]
pub struct ExecutionMetrics {
    pub total_executions: usize,
    pub successful_executions: usize,
    pub failed_executions: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub average_duration_ms: f64,
}

impl MonitoredExecutor {
    pub fn new(executor: ToolExecutor) -> Self {
        Self {
            executor,
            metrics: Arc::new(std::sync::Mutex::new(ExecutionMetrics::default())),
        }
    }

    pub fn get_metrics(&self) -> ExecutionMetrics {
        self.metrics.lock().unwrap().clone()
    }
}
```

---

## See Also

- [API_REFERENCE.md](./API_REFERENCE.md) - Complete API reference
- [EXAMPLES.md](./EXAMPLES.md) - Usage examples
- [TOOL_METADATA.md](./TOOL_METADATA.md) - Metadata reference
- [LLM_METADATA_SPEC.md](./LLM_METADATA_SPEC.md) - LLM metadata format
