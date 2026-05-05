//! LLM Integration - Bridge to rustycode-llm providers
//!
//! This module provides the integration between ACP and the actual LLM providers.

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// LLM integration manager
pub struct LLMIntegration {
    provider: Arc<Mutex<Option<Box<dyn rustycode_llm::provider::LLMProvider>>>>,
    default_model: String,
}

impl LLMIntegration {
    pub fn new(default_model: String) -> Self {
        Self {
            provider: Arc::new(Mutex::new(None)),
            default_model,
        }
    }

    /// Initialize the LLM provider from config
    pub async fn initialize(&mut self) -> Result<()> {
        use rustycode_config::Config;
        use rustycode_llm::provider::{LLMProvider, ProviderConfig};
        use secrecy::SecretString;

        // Load config from current directory
        let current_dir = std::env::current_dir()
            .map_err(|e| anyhow::anyhow!("Failed to get current directory: {}", e))?;

        let config = Config::load(&current_dir)
            .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

        // Get provider from model name (e.g., "claude-3-5-sonnet-20241022" -> anthropic)
        let (provider_name, _) = self.default_model.split_once('-').unwrap_or_else(|| {
            warn!(
                "Unable to parse model name '{}', defaulting to gpt-4",
                self.default_model
            );
            ("gpt", "4")
        });

        let provider: Option<Box<dyn LLMProvider>> = match provider_name {
            "claude" | "anthropic" => {
                // Get API key from config
                let api_key = config.providers.anthropic.and_then(|p| p.api_key);

                // Check if API key exists and is not empty
                let has_key = api_key.as_ref().is_some_and(|k| !k.is_empty());

                if !has_key {
                    info!("No Anthropic API key found, skipping LLM initialization");
                    return Ok(());
                }

                let provider_config = ProviderConfig {
                    api_key: api_key.map(|k| SecretString::new(k.into())),
                    base_url: None,
                    timeout_seconds: Some(120),
                    extra_headers: None,
                    retry_config: None,
                };

                match rustycode_llm::AnthropicProvider::new(
                    provider_config,
                    self.default_model.clone(),
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        error!("Failed to create Anthropic provider: {}", e);
                        None
                    }
                }
            }
            "gpt" | "openai" => {
                // Get API key from config
                let api_key = config.providers.openai.and_then(|p| p.api_key);

                // Check if API key exists and is not empty
                let has_key = api_key.as_ref().is_some_and(|k| !k.is_empty());

                if !has_key {
                    info!("No OpenAI API key found, skipping LLM initialization");
                    return Ok(());
                }

                let provider_config = ProviderConfig {
                    api_key: api_key.map(|k| SecretString::new(k.into())),
                    base_url: None,
                    timeout_seconds: Some(120),
                    extra_headers: None,
                    retry_config: None,
                };

                match rustycode_llm::OpenAiProvider::new(
                    provider_config,
                    self.default_model.clone(),
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        error!("Failed to create OpenAI provider: {}", e);
                        None
                    }
                }
            }
            _ => {
                info!("Unknown provider: {}, using mock responses", provider_name);
                None
            }
        };

        let has_provider = provider.is_some();
        *self.provider.lock().await = provider;

        if has_provider {
            info!("LLM provider initialized: {}", self.default_model);
        } else {
            info!("LLM provider not available, will use mock responses");
        }

        Ok(())
    }

    /// Process messages with the LLM
    pub async fn process_messages(
        &self,
        messages: &[crate::types::PromptMessage],
        tools: Option<Vec<serde_json::Value>>,
        _system_prompt: Option<&str>,
    ) -> Result<rustycode_llm::provider::CompletionResponse> {
        use rustycode_llm::provider::{ChatMessage, CompletionRequest, MessageRole};
        use rustycode_protocol::MessageContent;

        let provider_guard = self.provider.lock().await;

        let provider = match provider_guard.as_ref() {
            Some(p) => p,
            None => {
                return Err(anyhow::anyhow!("LLM provider not initialized"));
            }
        };

        // Convert ACP messages to LLM messages
        let llm_messages: Vec<ChatMessage> = messages
            .iter()
            .filter_map(|m| match m {
                crate::types::PromptMessage::User { parts } => {
                    let mut contents = Vec::new();
                    for part in parts {
                        match part {
                            crate::types::ContentPart::Text { text } => {
                                contents.push(serde_json::json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }
                            crate::types::ContentPart::ToolResult {
                                tool_use_id,
                                content,
                                is_error,
                            } => {
                                let mut res = serde_json::json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_use_id,
                                    "content": content
                                });
                                if let Some(err) = is_error {
                                    if *err {
                                        res["is_error"] = serde_json::json!(true);
                                    }
                                }
                                contents.push(res);
                            }
                            _ => {}
                        }
                    }
                    if contents.is_empty() {
                        return None;
                    }
                    if contents.len() == 1 && contents[0]["type"] == "text" {
                        Some(ChatMessage::user(
                            contents[0]["text"].as_str().unwrap_or_default(),
                        ))
                    } else {
                        Some(ChatMessage {
                            role: MessageRole::User,
                            content: MessageContent::Simple(
                                serde_json::to_string(&contents).unwrap_or_default(),
                            ),
                        })
                    }
                }
                crate::types::PromptMessage::Assistant { parts } => {
                    let mut contents = Vec::new();
                    for part in parts {
                        match part {
                            crate::types::ContentPart::Text { text } => {
                                contents.push(serde_json::json!({
                                    "type": "text",
                                    "text": text
                                }));
                            }
                            crate::types::ContentPart::Tool { name, input, id } => {
                                contents.push(serde_json::json!({
                                    "type": "tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": input
                                }));
                            }
                            _ => {}
                        }
                    }
                    if contents.is_empty() {
                        return None;
                    }
                    Some(ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(
                            serde_json::to_string(&contents).unwrap_or_default(),
                        ),
                    })
                }
            })
            .collect();

        if llm_messages.is_empty() {
            return Ok(rustycode_llm::provider::CompletionResponse {
                content: "I couldn't find any user messages to process.".to_string(),
                model: self.default_model.clone(),
                usage: None,
                stop_reason: None,
                citations: None,
                thinking_blocks: None,
                structured_output: None,
            });
        }

        debug!("Processing {} messages with LLM", llm_messages.len());

        // Create completion request
        let mut request = CompletionRequest::new(self.default_model.clone(), llm_messages);
        request.tools = tools;

        // Get completion from provider
        let response = provider.complete(request).await?;

        Ok(response)
    }

    /// Check if LLM is available
    pub async fn is_available(&self) -> bool {
        self.provider.lock().await.is_some()
    }
}

impl Clone for LLMIntegration {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            default_model: self.default_model.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_integration_new() {
        let llm = LLMIntegration::new("claude-sonnet-4-6".to_string());
        assert_eq!(llm.default_model, "claude-sonnet-4-6");
    }

    #[test]
    fn test_llm_integration_new_openai_model() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        assert_eq!(llm.default_model, "gpt-4");
    }

    #[test]
    fn test_llm_integration_new_custom_model() {
        let llm = LLMIntegration::new("my-custom-model".to_string());
        assert_eq!(llm.default_model, "my-custom-model");
    }

    #[test]
    fn test_llm_integration_clone() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let cloned = llm.clone();
        assert_eq!(cloned.default_model, "claude-3");
    }

    #[tokio::test]
    async fn test_llm_not_available_before_init() {
        let llm = LLMIntegration::new("claude-3".to_string());
        assert!(!llm.is_available().await);
    }

    #[tokio::test]
    async fn test_process_messages_returns_error_without_provider() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "hello".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, None, None).await;
        // Should return error since no provider is initialized
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "LLM provider not initialized"
        );
    }

    #[tokio::test]
    async fn test_process_messages_empty_messages_returns_error_without_provider() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        let messages = vec![];
        let result = llm.process_messages(&messages, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_messages_with_assistant_only_returns_error_without_provider() {
        let llm = LLMIntegration::new("gpt-4".to_string());
        let messages = vec![crate::types::PromptMessage::Assistant {
            parts: vec![crate::types::ContentPart::Text {
                text: "I said this".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_messages_with_system_prompt_none_returns_error() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "hello".to_string(),
            }],
        }];
        let result = llm.process_messages(&messages, None, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_messages_with_system_prompt_some_returns_error() {
        let llm = LLMIntegration::new("claude-3".to_string());
        let messages = vec![crate::types::PromptMessage::User {
            parts: vec![crate::types::ContentPart::Text {
                text: "hello".to_string(),
            }],
        }];
        let result = llm
            .process_messages(&messages, None, Some("be concise"))
            .await;
        assert!(result.is_err());
    }
}
