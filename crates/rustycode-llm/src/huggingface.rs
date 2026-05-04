//! Hugging Face Inference API LLM provider implementation.
//!
//! This provider supports Hugging Face's Inference API which provides access to
//! thousands of models hosted on the Hugging Face Hub.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Hugging Face settings)
//! - Model name (e.g., "meta-llama/Meta-Llama-3-8B-Instruct")
//!
//! ## Environment Variables
//!
//! - `HF_TOKEN` or `HUGGINGFACE_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "huggingface"
//! model = "meta-llama/Meta-Llama-3-8B-Instruct"
//! api_key = "your-api-key"
//! ```

use crate::openai_compatible::{
    build_completion_response, build_request_with_auth, map_http_error, parse_openai_sse_lines,
    OpenAiCompatibleMessage, OpenAiCompatibleResponse, OpenAiFunction, OpenAiToolCall,
    SseParseConfig, SseParseState,
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

/// Default Hugging Face Inference API endpoint
const HF_API_ENDPOINT: &str = "https://api-inference.huggingface.co/v1/chat/completions";

/// Hugging Face Inference API LLM provider
pub struct HuggingFaceProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    default_model: String,
    endpoint: String,
}

impl HuggingFaceProvider {
    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| HF_API_ENDPOINT.to_string());

        let client = shared_client!();

        Ok(Self {
            config,
            client,
            default_model,
            endpoint,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "HF_TOKEN".to_string());
        env_mappings.insert("api_key".to_string(), "HUGGINGFACE_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "huggingface".to_string(),
            display_name: "Hugging Face".to_string(),
            description: "Access thousands of models on the Hugging Face Hub via Inference API".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Hugging Face API token (from hf.co/settings/tokens)".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("hf_...".to_string()),
                        default: None,
                        validation_pattern: Some("^hf_.*".to_string()),
                        validation_error: Some("API key must start with 'hf_'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        default: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant accessed via the Hugging Face Inference API.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate responses\n- When writing code, include explanations\n- Ask clarifying questions when needed\n- Be thorough but concise".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "You can access various models from the Hugging Face Hub.".to_string(),
                        "Model capabilities may vary depending on the selected model.".to_string(),
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
        let hf_token = std::env::var("HF_TOKEN").ok();
        let hf_api_key = std::env::var("HUGGINGFACE_API_KEY").ok();
        config_key.or(hf_token).or(hf_api_key).ok_or_else(|| {
            ProviderError::Configuration(
                "Hugging Face API key required. Set api_key in config, HF_TOKEN, or HUGGINGFACE_API_KEY env var".to_string(),
            )
        })
    }

    fn convert_messages(
        messages: Vec<crate::provider::ChatMessage>,
    ) -> Vec<OpenAiCompatibleMessage> {
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

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl LLMProvider for HuggingFaceProvider {
    fn name(&self) -> &'static str {
        "huggingface"
    }

    async fn is_available(&self) -> bool {
        self.get_api_key().is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "meta-llama/Llama-4-8B-Instruct".to_string(),
            "meta-llama/Llama-3.3-70B-Instruct".to_string(),
            "meta-llama/Llama-3.1-405B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-8B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-70B-Instruct".to_string(),
            "mistralai/Mistral-7B-Instruct-v0.2".to_string(),
            "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string(),
            "mistralai/Mixtral-8x22B-Instruct-v0.1".to_string(),
            "google/gemma-7b".to_string(),
            "tiiuae/falcon-7b-instruct".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.get_api_key()?;
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .map(|t| normalize_tools_for_openai(t));

        let mut body = serde_json::json!({
            "model": model,
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
            .map_err(|e| ProviderError::Network(format!("failed to send request: {}", e)))?;

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
                "Hugging Face",
                "HF_TOKEN",
            ));
        }

        let hf_response: OpenAiCompatibleResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse response: {}", e))
        })?;

        build_completion_response(&hf_response)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.get_api_key()?;
        let model = if request.model.is_empty() {
            &self.default_model
        } else {
            &request.model
        };

        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .map(|t| normalize_tools_for_openai(t));

        let mut request_body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
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

        let response = req
            .json(&request_body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("failed to connect: {}", e)))?;

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
                "Hugging Face",
                "HF_TOKEN",
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
                        "failed to read chunk: {}",
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
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(provider.name(), "huggingface");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("hf_test-key"));
        let provider = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = HuggingFaceProvider::metadata();
        assert_eq!(metadata.display_name, "Hugging Face");
        assert_eq!(metadata.provider_id, "huggingface");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = HuggingFaceProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = HuggingFaceProvider::metadata();
        assert!(metadata.config_schema.env_mappings.contains_key("api_key"));
    }

    #[test]
    fn test_metadata_no_recommended_models() {
        let metadata = HuggingFaceProvider::metadata();
        assert!(metadata.recommended_models.is_empty());
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(provider.endpoint(), HF_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("hf_test-key"));
        config.base_url = Some("https://my-hf-proxy.example.com/v1/chat/completions".to_string());
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://my-hf-proxy.example.com/v1/chat/completions"
        );
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models
            .iter()
            .any(|m| m.contains("llama") || m.contains("Llama")));
    }

    #[tokio::test]
    async fn test_is_available_with_key() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
