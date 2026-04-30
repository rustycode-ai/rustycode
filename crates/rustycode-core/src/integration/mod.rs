//! Integration layer for external systems.
//!
//! This module provides integration with external systems and extensibility points:
//!
//! ## Hooks System
//!
//! The hooks subsystem provides a comprehensive hooks system based on Claude SDK patterns,
//! enabling control over tool execution, session events, and subagent lifecycle.
//!
//! ### Hook Types
//!
//! - **PreToolUseHook**: Before tool execution (can allow/deny/modify)
//! - **PostToolUseHook**: After tool execution (observability)
//! - **UserPromptSubmitHook**: Before sending user message (can sanitize/validate)
//! - **StopHook**: When session stops (cleanup, logging)
//! - **SubagentStartHook**: When subagent starts (tracking)
//! - **SubagentStopHook**: When subagent stops (cleanup)
//! - **FileChangedHook**: When a watched file changes (reactive workflows)
//!
//! ### Usage
//!
//! ```ignore
//! use rustycode_core::integration::{HookRegistry, HookContext, PreToolUseHook, HookAction};
//! use rustycode_protocol::SessionId;
//! use std::sync::Arc;
//!
//! // Create a hook registry
//! let mut registry = HookRegistry::new();
//!
//! // Register a pre-tool-use hook
//! registry.register_pre_tool_use(Box::new(MyPermissionHook));
//! ```
//!
//! ## MCP Integration
//!
//! The MCP (Model Context Protocol) subsystem provides integration with MCP servers,
//! enabling automatic tool discovery and execution from external MCP servers.
//!
//! ### Features
//!
//! - Automatic MCP server lifecycle management
//! - Tool discovery and namespacing (`mcp_<server>_<tool>`)
//! - Retry logic for tool execution
//! - Server health monitoring
//!
//! ### Usage
//!
//! ```ignore
//! use rustycode_core::integration::McpIntegration;
//! use rustycode_config::Config;
//!
//! # async fn example(config: &Config) -> anyhow::Result<()> {
//! let mut mcp = McpIntegration::new(config).await?;
//! mcp.start_servers().await?;
//!
//! // Execute an MCP tool
//! let result = mcp.execute_tool("mcp_server_read_file", serde_json::json!({})).await?;
//! # Ok(())
//! # }
//! ```

pub mod hooks_integration;
pub mod mcp_integration;

// Re-export hooks types for backward compatibility
pub use hooks_integration::{
    FileChangeEvent, FileChangeKind, FileChangedHook, HookAction, HookContext, HookRegistry,
    PostToolUseHook, PreToolUseHook, StopHook, SubagentStartHook, SubagentStopHook,
    UserPromptSubmitHook,
};

// Re-export MCP integration types
pub use mcp_integration::{McpIntegration, McpToolInfo};
