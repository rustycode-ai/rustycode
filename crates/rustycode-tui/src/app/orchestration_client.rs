//! Orchestration Client Trait
//!
//! Abstracts TUI interactions with background services, enabling decoupling
//! and independent testing of UI components.

use crate::services::agent_mode::AiMode;
use anyhow::Result;
use rustycode_protocol::{ToolCall, ToolResult};

pub trait OrchestrationClient: Send {
    /// Request the LLM stream to stop
    fn request_stop_stream(&self);

    /// Get current AI mode
    fn ai_mode(&self) -> AiMode;

    /// Execute a tool call
    fn execute_tool(&self, call: ToolCall) -> Result<ToolResult>;

    /// Check if streaming is active
    fn is_streaming(&self) -> bool;

    /// Set current AI mode
    fn set_ai_mode(&self, mode: AiMode);

    /// Get current working directory
    fn cwd(&self) -> std::path::PathBuf;

    /// Send a message to the backend service
    fn send_message(&self, message: String) -> Result<()>;
}
