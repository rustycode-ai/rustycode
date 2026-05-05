//! Tool Executor - Bridge to rustycode-tools
//!
//! This module handles tool execution for ACP requests.

use crate::types::{ContentPart, ToolResult};
use anyhow::Result;
use rustycode_protocol::ToolCall;
use rustycode_tools::ToolContext;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Tool executor
pub struct ToolExecutor {
    tool_registry: Arc<Mutex<Option<rustycode_tools::ToolRegistry>>>,
    cwd: PathBuf,
}

impl ToolExecutor {
    pub fn new(cwd: String) -> Self {
        Self {
            tool_registry: Arc::new(Mutex::new(None)),
            cwd: PathBuf::from(cwd),
        }
    }

    /// Initialize the tool registry
    pub async fn initialize(&mut self) -> Result<()> {
        use rustycode_tools::ToolRegistry;

        let registry = ToolRegistry::new();

        // Register common tools
        // Note: Tools are discovered automatically by the registry

        *self.tool_registry.lock().await = Some(registry);

        info!("Tool executor initialized for cwd: {:?}", self.cwd);
        Ok(())
    }

    /// Execute a tool call
    pub async fn execute_tool(&self, tool_name: &str, tool_input: Value) -> Result<ToolResult> {
        let registry_guard = self.tool_registry.lock().await;

        let registry = if let Some(r) = registry_guard.as_ref() {
            r
        } else {
            // Return error result if no registry available
            warn!("Tool registry not available");
            return Ok(ToolResult {
                content: vec![ContentPart::Text {
                    text: format!(
                        "Tool '{}' not available (registry not initialized)",
                        tool_name
                    ),
                }],
                is_error: true,
                structured_content: None,
            });
        };

        debug!("Executing tool: {} with input: {}", tool_name, tool_input);

        // Map ACP tool names to rustycode tool names
        let mapped_name = tool_name;

        // Create tool call
        let call = ToolCall::with_generated_id(mapped_name, tool_input);

        // Create tool context
        let ctx = ToolContext::new(&self.cwd);

        // Execute the tool
        let result = registry.execute(&call, &ctx);

        if result.success {
            info!("Tool {} executed successfully", tool_name);
            Ok(ToolResult {
                content: vec![ContentPart::Text {
                    text: result.output,
                }],
                is_error: false,
                structured_content: result.data,
            })
        } else {
            error!("Tool {} failed: {:?}", tool_name, result.error);
            Ok(ToolResult {
                content: vec![ContentPart::Text {
                    text: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                }],
                is_error: true,
                structured_content: None,
            })
        }
    }

    /// Check if tools are available
    pub async fn is_available(&self) -> bool {
        self.tool_registry.lock().await.is_some()
    }

    /// Get tool definitions for the LLM
    pub async fn tool_definitions(&self) -> Result<Vec<serde_json::Value>> {
        let registry_guard = self.tool_registry.lock().await;
        let registry = registry_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Tool registry not initialized"))?;

        let mut tools = Vec::new();
        for tool_info in registry.list() {
            let mut tool_json = serde_json::json!({
                "name": tool_info.name,
                "description": tool_info.description,
                "input_schema": tool_info.parameters_schema,
            });

            // Add annotations if present in the tool info
            if let Some(annotations) = tool_info.annotations {
                tool_json["annotations"] = serde_json::to_value(annotations).unwrap_or_default();
            }

            tools.push(tool_json);
        }

        Ok(tools)
    }
}

impl Clone for ToolExecutor {
    fn clone(&self) -> Self {
        Self {
            tool_registry: self.tool_registry.clone(),
            cwd: self.cwd.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_executor_new() {
        let executor = ToolExecutor::new("/tmp/project".to_string());
        assert_eq!(executor.cwd, PathBuf::from("/tmp/project"));
    }

    #[test]
    fn test_tool_executor_new_empty_cwd() {
        let executor = ToolExecutor::new(String::new());
        assert_eq!(executor.cwd, PathBuf::from(""));
    }

    #[test]
    fn test_tool_executor_new_dot_cwd() {
        let executor = ToolExecutor::new(".".to_string());
        assert_eq!(executor.cwd, PathBuf::from("."));
    }

    #[test]
    fn test_tool_executor_clone() {
        let executor = ToolExecutor::new("/test".to_string());
        let cloned = executor.clone();
        assert_eq!(cloned.cwd, executor.cwd);
    }

    #[tokio::test]
    async fn test_tool_executor_not_available_before_init() {
        let executor = ToolExecutor::new("/tmp".to_string());
        assert!(!executor.is_available().await);
    }

    #[tokio::test]
    async fn test_tool_executor_mock_response_without_init() {
        let executor = ToolExecutor::new("/tmp".to_string());
        let result = executor
            .execute_tool("bash", serde_json::json!({"command": "ls"}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.is_error);
        if let ContentPart::Text { text } = &val.content[0] {
            assert!(text.contains("bash"));
        } else {
            panic!("Expected ContentPart::Text");
        }
    }

    #[tokio::test]
    async fn test_tool_executor_mock_response_includes_tool_name() {
        let executor = ToolExecutor::new("/tmp".to_string());
        let result = executor
            .execute_tool("read_file", serde_json::json!({"path": "/etc/hosts"}))
            .await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert!(val.is_error);
        if let ContentPart::Text { text } = &val.content[0] {
            assert!(text.contains("read_file"));
        } else {
            panic!("Expected ContentPart::Text");
        }
    }

    #[tokio::test]
    async fn test_tool_executor_initialize_sets_available() {
        let mut executor = ToolExecutor::new("/tmp".to_string());
        let result = executor.initialize().await;
        // initialize should succeed (tools registry is created)
        assert!(result.is_ok());
        assert!(executor.is_available().await);
    }
}
