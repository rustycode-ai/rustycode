//! Mistral AI LLM provider implementation.
//!
//! This provider supports Mistral AI's API which provides access to
//! language models like Mistral 7B, Mixtral 8x7B, Mistral Large, etc.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Mistral AI dashboard)
//! - Model name (e.g., "mistral-large-latest", "mixtral-8x7b-2707")
//!
//! ## Environment Variables
//!
//! - `MISTRAL_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "mistral"
//! model = "mistral-large-latest"
//! api_key = "your-api-key"
//! ```

use crate::openai_compatible::{
    build_completion_response, build_request_with_auth, map_http_error,
    parse_openai_sse_lines, OpenAiCompatibleMessage, OpenAiCompatibleResponse, OpenAiFunction,
    OpenAiToolCall, SseParseConfig, SseParseState,
};
use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, PromptOptimizations, PromptTemplate,
    ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::shared_client;
use crate::sse::SseByteBuffer;
use crate::tools::normalize_tools_for_openai;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;

/// Default Mistral AI API endpoint
const MISTRAL_API_ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";

/// Mistral AI LLM provider
pub struct MistralProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl MistralProvider {
    pub fn new(config: ProviderConfig, _model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| MISTRAL_API_ENDPOINT.to_string());

        let client = shared_client!();

        Ok(Self {
            config,
            client,
            endpoint,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "MISTRAL_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "mistral".to_string(),
            display_name: "Mistral AI".to_string(),
            description: "Advanced AI models including Mistral Large, Mixtral, and Codestral".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Mistral API key from console.mistral.ai".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("Your API key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        default: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant provided by Mistral AI.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate, and well-structured responses\n- When writing code, include explanations and best practices\n- Ask clarifying questions when requirements are ambiguous\n- Be thorough but concise in your answers".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Mistral models excel at reasoning and code generation.".to_string(),
                        "Use structured formatting for complex responses.".to_string(),
                    ],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
            ],
        }
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
        let config_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string());
        let env_key = std::env::var("MISTRAL_API_KEY").ok();
        config_key.or(env_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Mistral API key is required. Set api_key in config or MISTRAL_API_KEY env var"
                    .to_string(),
            )
        })
    }

    fn convert_messages(messages: Vec<crate::provider::ChatMessage>) -> Vec<OpenAiCompatibleMessage> {
        use crate::provider::MessageRole;
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let mut result = Vec::new();
        for msg in messages {
            let role = match msg.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool(_) => "tool",
            };

            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();
            let mut tool_call_id = None;

            match &msg.content {
                MessageContent::Simple(t) => text_parts.push(t.clone()),
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text, .. } => text_parts.push(text.clone()),
                            ContentBlock::ToolUse {
                                id, name, input, ..
                            } => {
                                tool_calls.push(OpenAiToolCall {
                                    id: id.clone(),
                                    tool_type: "function".to_string(),
                                    function: OpenAiFunction {
                                        name: name.clone(),
                                        arguments: serde_json::to_string(input).unwrap_or_default(),
                                    },
                                });
                            }
                            ContentBlock::ToolResult {
                                tool_use_id,
                                content: result_text,
                                ..
                            } => {
                                tool_call_id = Some(tool_use_id.clone());
                                text_parts.push(result_text.clone());
                            }
                            ContentBlock::Thinking { thinking, .. } => {
                                text_parts.push(thinking.clone());
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }

            if let MessageRole::Tool(ref id) = msg.role {
                tool_call_id = Some(id.clone());
            }

            result.push(OpenAiCompatibleMessage {
                role: role.to_string(),
                content: Some(text_parts.join("\n")),
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                tool_call_id,
                name: None,
            });
        }
        result
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl LLMProvider for MistralProvider {
    fn name(&self) -> &'static str {
        "mistral"
    }

    async fn is_available(&self) -> bool {
        self.get_api_key().is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "mistral-large-2407".to_string(),
            "mixtral-8x22b-2407".to_string(),
            "mixtral-8x7b-2407".to_string(),
            "mistral-medium-2312".to_string(),
            "mistral-small-2409".to_string(),
            "codestral-2405".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.get_api_key()?;
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request.tools.as_ref().map(|t| normalize_tools_for_openai(t));

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
        });
        if let Some(rf) = build_openai_response_format(&request.output_config) {
            body["response_format"] = rf;
        }
        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }
        if let Some(tc) = &request.tool_choice {
            body["tool_choice"] = tc.clone();
        }

        let req = build_request_with_auth(
            self.client.post(self.endpoint()),
            &api_key,
            self.config.extra_headers.as_ref(),
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let headers = response.headers().clone();
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(map_http_error(
                status,
                error_text,
                &headers,
                "Mistral",
                "MISTRAL_API_KEY",
            ));
        }

        let mistral_response: OpenAiCompatibleResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        build_completion_response(&mistral_response)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.get_api_key()?;
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request.tools.as_ref().map(|t| normalize_tools_for_openai(t));

        let mut request_body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });
        if let Some(tools) = tools {
            request_body["tools"] = serde_json::json!(tools);
        }
        if let Some(tc) = &request.tool_choice {
            request_body["tool_choice"] = tc.clone();
        }

        let req = build_request_with_auth(
            self.client.post(self.endpoint()),
            &api_key,
            self.config.extra_headers.as_ref(),
        );

        let response = req.json(&request_body).send().await.map_err(|e| {
            ProviderError::Network(format!("Failed to connect to Mistral: {}", e))
        })?;

        if !response.status().is_success() {
            let headers = response.headers().clone();
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(map_http_error(
                status,
                error_text,
                &headers,
                "Mistral",
                "MISTRAL_API_KEY",
            ));
        }

        // Convert bytes stream to SSE stream using shared parser
        let bytes_stream = response.bytes_stream();
        let line_buffer = SseByteBuffer::new();
        let sse_state = SseParseState::default();
        let config = SseParseConfig::all();

        let sse_stream = bytes_stream.flat_map(move |chunk_result| {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    return futures::stream::iter(vec![Err(ProviderError::Network(format!(
                        "Failed to read chunk: {}",
                        e
                    )))]);
                }
            };
            let lines = line_buffer.feed_chunk(&chunk);
            let complete_lines = lines.join("\n");
            let events = parse_openai_sse_lines(&complete_lines, config, &sse_state);
            futures::stream::iter(events)
        });

        Ok(Box::pin(sse_stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn make_config(api_key: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: api_key.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_provider_name() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.name(), "mistral");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_fields() {
        let meta = MistralProvider::metadata();
        assert_eq!(meta.provider_id, "mistral");
        assert_eq!(meta.display_name, "Mistral AI");
        assert!(!meta.config_schema.required_fields.is_empty());
        assert!(
            meta.config_schema
                .required_fields
                .iter()
                .any(|f| f.name == "api_key"),
            "api_key should be required"
        );
    }

    #[test]
    fn test_metadata_env_mapping() {
        let meta = MistralProvider::metadata();
        assert_eq!(
            meta.config_schema.env_mappings.get("api_key"),
            Some(&"MISTRAL_API_KEY".to_string())
        );
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.endpoint(), MISTRAL_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://custom.mistral.example.com/v1".to_string());
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.endpoint(), "https://custom.mistral.example.com/v1");
    }

    #[tokio::test]
    async fn test_is_available_with_key() {
        let config = make_config(Some("valid-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("mistral-large")));
    }

    #[test]
    fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
