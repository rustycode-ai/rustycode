#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::future_not_send,
    clippy::ignored_unit_patterns,
    clippy::implicit_hasher,
    clippy::items_after_statements,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_fields_in_debug,
    clippy::needless_continue,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::struct_excessive_bools,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_literal_bound,
    clippy::unnecessary_wraps,
    clippy::unused_async,
    clippy::unused_self,
    clippy::unwrap_used
)]
//! # `RustyCode` MCP (Model Context Protocol)
//!
//! This crate provides a full implementation of the Model Context Protocol (MCP),
//! enabling communication between AI assistants and external tools/resources via stdio.
//!
//! ## Architecture
//!
//! The MCP implementation consists of:
//!
//! - **Client**: Connects to MCP servers and exposes their tools/resources
//! - **Server**: Hosts tools and resources for AI assistants
//! - **Transport**: JSON-RPC over stdio with async support
//! - **Protocol**: Full MCP protocol implementation with extensions
//!
//! ## Features
//!
//! - Async/await support throughout
//! - Tool discovery and calling
//! - Resource access (files, prompts, templates)
//! - Prompt template management
//! - Integration with rustycode-tools
//! - Tool proxying and delegation
//!
//! ## Example: Client Usage
//!
//! ```ignore
//! use rustycode_mcp::{McpClient, McpClientConfig};
//! use rustycode_tools::ToolExecutor;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create MCP client
//!     let config = McpClientConfig::default();
//!     let mut client = McpClient::new(config);
//!
//!     // Connect to a server
//!     client.connect_stdio(
//!         "my-server",
//!         "/usr/local/bin/my-mcp-server",
//!         &[]
//!     ).await?;
//!
//!     // List available tools
//!     let tools = client.list_tools("my-server").await?;
//!     for tool in tools {
//!         println!("Tool: {} - {}", tool.name, tool.description);
//!     }
//!
//!     // Call a tool
//!     let result = client.call_tool(
//!         "my-server",
//!         "my_tool",
//!         serde_json::json!({"param": "value"})
//!     ).await?;
//!
//!     println!("Result: {:?}", result);
//!     Ok(())
//! }
//! ```
//!
//! ## Example: Server Usage
//!
//! ```no_run
//! use rustycode_mcp::{McpServer, McpServerConfig};
//! use rustycode_tools::ToolExecutor;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Create tool executor
//!     let executor = ToolExecutor::from_cwd(PathBuf::from("."));
//!
//!     // Create MCP server
//!     let config = McpServerConfig::default();
//!     let mut server = McpServer::new("my-server", config);
//!
//!     // Register tools from executor
//!     server.register_tool_executor(executor);
//!
//!     // Run server (stdio)
//!     server.run_stdio().await?;
//!     Ok(())
//! }
//! ```

pub mod allowlist;
pub mod client;
pub mod enterprise;
pub mod headers_helper;
pub mod http_transport;
pub mod lsp_tools;
pub mod manager;
pub mod oauth;
pub mod protocol;
pub mod proxy;
pub mod resources;
pub mod server;
pub mod server_enablement;
pub mod sse;
pub mod stdio_client;
pub mod system_prompt;
pub mod testing;
pub mod tmux_control;
pub mod tools;
pub mod transport;
pub mod types;

// Re-export key types
pub use allowlist::{AllowlistEntry, AllowlistManager, AllowlistStatus, SessionAllowlist};
pub use client::{McpClient, McpClientConfig};
pub use enterprise::{
    retry_with_backoff, ConnectionPool, Metrics, MetricsCollector, PoolConfig, PoolStats,
    RateLimiter, RateLimiterConfig, RetryConfig,
};
pub use http_transport::HttpTransport;
pub use lsp_tools::build_lsp_tool_executor;
pub use manager::{
    HealthStatus, ManagerConfig, McpConfigFile, McpOAuthConfig, McpServer as ManagedMcpServer,
    McpServerManager, McpTransportType, ServerConfig,
};
pub use oauth::{
    AuthorizationUrl, OAuthClientCredentials, OAuthManager, OAuthMetadata, OAuthToken,
};
pub use protocol::{McpNotification, McpRequest, McpResponse};
pub use proxy::{ProxyConfig, ToolProxy};
pub use resources::template_matching;
pub use resources::{Resource, ResourceBatch, ResourceContent, ResourceManager};
pub use server::{McpServer, McpServerConfig};
pub use server_enablement::{
    BlockType, ServerDisplayState, ServerEnablementConfig, ServerEnablementManager,
    ServerEnablementState, ServerLoadResult,
};
pub use stdio_client::{
    McpClientError as StdioClientError, McpClientManager, McpClientResult as StdioClientResult,
    McpServerConfig as StdioServerConfig, McpStdioClient,
};
pub use system_prompt::{
    combine_mcp_prompts, McpPromptGenerator, McpSystemPrompt, ResourceDescription, ToolDescription,
};
pub use tmux_control::{TerminalBackendKind, TmuxMcpConfig, TmuxMcpServer};
pub use tools::{
    CacheStats, ToolCache, ToolCall, ToolExecutionEngine, ToolExecutionResult, ToolRegistry,
};
pub use transport::{StdioTransport, Transport};
pub use types::*;

use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

/// MCP protocol version (2025-11-25 spec)
pub const MCP_VERSION: &str = "2025-11-25";

/// Core error type for MCP operations
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum McpError {
    #[error("JSON-RPC error: {0}")]
    JsonRpcError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Method not found: {0}")]
    MethodNotFound(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Timeout waiting for response")]
    Timeout,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Server not found: {0}")]
    ServerNotFound(String),

    #[error("Call failed: {0}")]
    CallFailed(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Rate limited: try again in {0:?}")]
    RateLimited(Duration),

    #[error("Session expired: {0}")]
    SessionExpired(String),
}

impl Serialize for McpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for McpError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::InternalError(s))
    }
}

/// Result type for MCP operations
pub type McpResult<T> = Result<T, McpError>;

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_mcp_error_display() {
        assert_eq!(
            McpError::JsonRpcError("bad request".into()).to_string(),
            "JSON-RPC error: bad request"
        );
        assert_eq!(
            McpError::TransportError("connection refused".into()).to_string(),
            "Transport error: connection refused"
        );
        assert_eq!(
            McpError::Timeout.to_string(),
            "Timeout waiting for response"
        );
        assert_eq!(McpError::ConnectionClosed.to_string(), "Connection closed");
        assert_eq!(
            McpError::RateLimited(Duration::from_secs(30)).to_string(),
            "Rate limited: try again in 30s"
        );
    }

    #[test]
    fn test_mcp_error_serialize() {
        let err = McpError::ToolNotFound("bash".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json, serde_json::json!("Tool not found: bash"));
    }

    #[test]
    fn test_mcp_error_deserialize() {
        let json = serde_json::json!("some error string");
        let err: McpError = serde_json::from_value(json).unwrap();
        assert!(matches!(err, McpError::InternalError(s) if s == "some error string"));
    }

    #[test]
    fn test_mcp_version_constant() {
        assert!(!MCP_VERSION.is_empty());
        assert!(MCP_VERSION.contains('-'));
    }

    #[test]
    fn test_mcp_error_all_variants_display() {
        assert_eq!(
            McpError::ProtocolError("version mismatch".into()).to_string(),
            "Protocol error: version mismatch"
        );
        assert_eq!(
            McpError::ToolNotFound("bash".into()).to_string(),
            "Tool not found: bash"
        );
        assert_eq!(
            McpError::ResourceNotFound("file://x".into()).to_string(),
            "Resource not found: file://x"
        );
        assert_eq!(
            McpError::InvalidRequest("missing field".into()).to_string(),
            "Invalid request: missing field"
        );
        assert_eq!(
            McpError::MethodNotFound("foo/bar".into()).to_string(),
            "Method not found: foo/bar"
        );
        assert_eq!(
            McpError::InternalError("overflow".into()).to_string(),
            "Internal error: overflow"
        );
        assert_eq!(
            McpError::ServerNotFound("my-server".into()).to_string(),
            "Server not found: my-server"
        );
        assert_eq!(
            McpError::CallFailed("timeout".into()).to_string(),
            "Call failed: timeout"
        );
        assert_eq!(
            McpError::ConnectionError("refused".into()).to_string(),
            "Connection error: refused"
        );
    }

    #[test]
    fn test_mcp_error_serialize_roundtrip() {
        // Serialize various errors and verify the string representation
        let cases = vec![
            McpError::JsonRpcError("bad".into()),
            McpError::TransportError("io".into()),
            McpError::Timeout,
            McpError::ConnectionClosed,
            McpError::RateLimited(Duration::from_secs(5)),
        ];
        for err in cases {
            let json = serde_json::to_value(&err).unwrap();
            // Serialization produces a string
            assert!(json.is_string());
            // Deserialization always produces InternalError variant
            let roundtrip: McpError = serde_json::from_value(json).unwrap();
            assert!(matches!(roundtrip, McpError::InternalError(_)));
        }
    }

    #[test]
    fn test_mcp_result_ok() {
        let result: McpResult<String> = Ok("hello".to_string());
        assert_eq!(result.as_ref().unwrap(), "hello");
    }

    #[test]
    fn test_mcp_result_err() {
        let result: McpResult<String> = Err(McpError::Timeout);
        assert!(result.is_err());
    }
}
