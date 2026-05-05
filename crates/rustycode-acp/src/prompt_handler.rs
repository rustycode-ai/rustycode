//! Prompt Handler - Process user prompts with LLM
//!
//! This module handles the actual processing of user messages,
//! integration with the LLM, and tool execution.

use crate::llm_integration::LLMIntegration;
use crate::tool_executor::ToolExecutor;
use crate::types::*;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Streaming callback type
pub type StreamCallback = Box<dyn Fn(SessionUpdate) + Send + Sync>;

/// Result from processing a prompt
#[derive(Debug)]
pub struct PromptResult {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCallResult>>,
    pub stop_reason: String,
}

/// Tool call result
#[derive(Debug)]
pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub output: Value,
}

/// Prompt Handler
pub struct PromptHandler {
    llm: Arc<Mutex<LLMIntegration>>,
    tool_executor: Arc<Mutex<ToolExecutor>>,
}

impl PromptHandler {
    pub fn new(cwd: String, default_model: String) -> Self {
        Self {
            llm: Arc::new(Mutex::new(LLMIntegration::new(default_model))),
            tool_executor: Arc::new(Mutex::new(ToolExecutor::new(cwd))),
        }
    }

    /// Initialize the prompt handler
    pub async fn initialize(&self) -> Result<()> {
        // Initialize LLM
        {
            let mut llm = self.llm.lock().await;
            llm.initialize().await?;
        }

        // Initialize tools
        {
            let mut tools = self.tool_executor.lock().await;
            tools.initialize().await?;
        }

        info!("Prompt handler initialized");
        Ok(())
    }

    /// Process a user prompt with full LLM integration and tool loop
    pub async fn process_prompt(
        &self,
        session_id: &str,
        messages: &[PromptMessage],
        cwd: &str,
        stream_callback: Option<StreamCallback>,
    ) -> Result<PromptResult> {
        info!("Processing prompt for session {}", session_id);

        if let Some(ref cb) = stream_callback {
            cb(SessionUpdate::Plan {
                steps: vec!["Processing prompt".to_string()],
            });
        }

        let mut conversation = messages.to_vec();
        let mut turn_count = 0;
        const MAX_TURNS: usize = 10;

        loop {
            turn_count += 1;
            if turn_count > MAX_TURNS {
                warn!(
                    "Max turns ({}) reached for session {}",
                    MAX_TURNS, session_id
                );
                return Ok(PromptResult {
                    content: "I reached my maximum turn limit. Please refine your request."
                        .to_string(),
                    tool_calls: None,
                    stop_reason: "max_turns".to_string(),
                });
            }

            // Get LLM response
            let llm_available = {
                let llm_guard = self.llm.lock().await;
                llm_guard.is_available().await
            };

            let tool_definitions = {
                let executor = self.tool_executor.lock().await;
                if executor.is_available().await {
                    Some(executor.tool_definitions().await?)
                } else {
                    None
                }
            };

            let response = if llm_available {
                // Use real LLM
                let llm_guard = self.llm.lock().await;
                llm_guard
                    .process_messages(&conversation, tool_definitions, None)
                    .await?
            } else {
                // Fallback to basic pattern matching
                let content = self.process_fallback(&conversation, cwd).await?;
                rustycode_llm::provider::CompletionResponse {
                    content,
                    model: "fallback".to_string(),
                    usage: None,
                    stop_reason: Some("end_turn".to_string()),
                    citations: None,
                    thinking_blocks: None,
                    structured_output: None,
                }
            };

            let content = response.content.clone();
            let stop_reason = response
                .stop_reason
                .clone()
                .unwrap_or_else(|| "end_turn".to_string());

            debug!(
                "LLM response (turn {}): stop_reason={}",
                turn_count, stop_reason
            );

            if stop_reason == "tool_use" {
                // Parse tool calls
                let tool_calls = self.parse_tool_calls(&content)?;
                if tool_calls.is_empty() {
                    warn!("LLM stopped for tool_use but no tool calls found in content");
                    return Ok(PromptResult {
                        content,
                        tool_calls: None,
                        stop_reason: "tool_use".to_string(),
                    });
                }

                // Add assistant's tool call to conversation
                conversation.push(PromptMessage::Assistant {
                    parts: tool_calls
                        .iter()
                        .map(|tc| ContentPart::Tool {
                            name: tc.name.clone(),
                            input: Some(tc.arguments.clone()),
                            id: tc.id.clone(),
                        })
                        .collect(),
                });

                // Execute tools
                let mut tool_results = Vec::new();
                for tc in tool_calls {
                    if let Some(ref cb) = stream_callback {
                        cb(SessionUpdate::ToolCallStart {
                            tool_call_id: tc.id.clone().unwrap_or_default(),
                            name: tc.name.clone(),
                            input: tc.arguments.clone(),
                        });
                    }

                    let result = {
                        let executor = self.tool_executor.lock().await;
                        executor.execute_tool(&tc.name, tc.arguments).await?
                    };

                    if let Some(ref cb) = stream_callback {
                        cb(SessionUpdate::ToolCallFinished {
                            tool_call_id: tc.id.clone().unwrap_or_default(),
                            result: serde_json::to_value(&result)?,
                        });
                    }

                    tool_results.push(ContentPart::ToolResult {
                        tool_use_id: tc.id.unwrap_or_default(),
                        content: result
                            .content
                            .iter()
                            .map(|part| match part {
                                ContentPart::Text { text } => text.clone(),
                                _ => String::new(),
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                        is_error: Some(result.is_error),
                    });
                }

                // Add tool results to conversation
                conversation.push(PromptMessage::User {
                    parts: tool_results,
                });

                // Continue loop to get next LLM response
                continue;
            }

            // Not a tool call, return final response
            return Ok(PromptResult {
                content,
                tool_calls: None,
                stop_reason,
            });
        }
    }

    /// Parse tool calls from LLM response content (Anthropic format)
    fn parse_tool_calls(
        &self,
        content: &str,
    ) -> Result<Vec<rustycode_llm::tool_executor::ParsedToolCall>> {
        use rustycode_llm::tool_executor::ParsedToolCall;

        // Try to parse as JSON array (Anthropic structured format)
        if let Ok(json_value) = serde_json::from_str::<Value>(content) {
            if let Some(blocks) = json_value.as_array() {
                let mut tool_calls = Vec::new();
                for block in blocks {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        let name = block["name"].as_str().unwrap_or_default().to_string();
                        let arguments = block["input"].clone();
                        let id = block
                            .get("id")
                            .and_then(|i| i.as_str())
                            .map(|s| s.to_string());
                        tool_calls.push(ParsedToolCall {
                            name,
                            arguments,
                            id,
                        });
                    }
                }
                if !tool_calls.is_empty() {
                    return Ok(tool_calls);
                }
            }
        }

        Ok(Vec::new())
    }

    /// Fallback processing when LLM is not available
    async fn process_fallback(&self, messages: &[PromptMessage], _cwd: &str) -> Result<String> {
        // Extract user message
        let user_message = messages
            .iter()
            .filter_map(|m| {
                if let PromptMessage::User { parts } = m {
                    parts.iter().find_map(|p| {
                        if let ContentPart::Text { text } = p {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let user_msg_lower = user_message.to_lowercase();

        debug!("Using fallback processing for: {}", user_message);

        // Pattern-based responses
        if user_msg_lower.contains("hello") || user_msg_lower.contains("hi ") {
            Ok("Hello! I'm RustyCode, your AI coding assistant. I can help you with:\n\n\
                  • Reading and writing files\n\
                  • Running commands\n\
                  • Searching code\n\
                  • Explaining code\n\
                  • Refactoring\n\n\
                  Note: LLM integration is not yet configured. Add an API key to enable full functionality.".to_string())
        } else if user_msg_lower.contains("help") {
            Ok("RustyCode Commands:\n\n\
                  • \"list files\" - List files in current directory\n\
                  • \"read <file>\" - Read a file\n\
                  • \"write <file> <content>\" - Write to a file\n\
                  • \"search <query>\" - Search for text\n\n\
                  Note: LLM not configured. Add API key for intelligent responses."
                .to_string())
        } else if user_msg_lower.contains("list files") || user_msg_lower.contains("ls") {
            Ok(
                "I would list files here if I had access to the tools registry.\n\n\
                  Note: LLM not configured. Add API key for intelligent responses."
                    .to_string(),
            )
        } else if user_msg_lower.contains("read") {
            Ok("I would read the file here.\n\n\
                  Note: LLM not configured. Add API key for intelligent responses."
                .to_string())
        } else if user_msg_lower.contains("write") {
            Ok("I would write to the file here.\n\n\
                  Note: LLM not configured. Add API key for intelligent responses."
                .to_string())
        } else {
            Ok(format!("I received: {}\n\nNote: LLM not configured. Add API key for intelligent responses.", user_message))
        }
    }

    /// Check if handler is ready (LLM and tools available)
    pub async fn is_ready(&self) -> bool {
        let llm_guard = self.llm.lock().await;
        let tools_guard = self.tool_executor.lock().await;

        llm_guard.is_available().await && tools_guard.is_available().await
    }
}

impl Default for PromptHandler {
    fn default() -> Self {
        Self::new(".".to_string(), "claude-sonnet-4-6".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_handler_new() {
        let _handler = PromptHandler::new("/tmp/project".to_string(), "claude-3".to_string());
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_fallback() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![ContentPart::Text {
                text: "hello".to_string(),
            }],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp", None)
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("RustyCode"));
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_mixed_parts() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![PromptMessage::User {
            parts: vec![
                ContentPart::Text {
                    text: "hello".to_string(),
                },
                ContentPart::Tool {
                    name: "bash".to_string(),
                    input: Some(serde_json::json!({"command": "ls"})),
                    id: None,
                },
            ],
        }];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp", None)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prompt_handler_process_prompt_multiple_user_messages() {
        let handler = PromptHandler::new(".".to_string(), "claude-3".to_string());
        let messages = vec![
            PromptMessage::User {
                parts: vec![ContentPart::Text {
                    text: "first".to_string(),
                }],
            },
            PromptMessage::Assistant {
                parts: vec![ContentPart::Text {
                    text: "response".to_string(),
                }],
            },
            PromptMessage::User {
                parts: vec![ContentPart::Text {
                    text: "hello".to_string(),
                }],
            },
        ];
        let result = handler
            .process_prompt("test-session", &messages, "/tmp", None)
            .await;
        assert!(result.is_ok());
        let content = result.unwrap().content;
        assert!(content.contains("RustyCode"));
    }

    #[test]
    fn test_prompt_result_debug() {
        let result = PromptResult {
            content: "test".to_string(),
            tool_calls: None,
            stop_reason: "end_turn".to_string(),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("test"));
        assert!(debug_str.contains("end_turn"));
    }

    #[test]
    fn test_tool_call_result_debug() {
        let result = ToolCallResult {
            id: "call-1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"cmd": "ls"}),
            output: serde_json::json!({"stdout": "file.txt"}),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("call-1"));
        assert!(debug_str.contains("bash"));
    }
}
