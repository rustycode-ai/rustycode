//! Together AI LLM provider implementation.
//!
//! This provider supports Together AI's API which provides access to
//! many open-source models like Llama, Mixtral, and more.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Together AI dashboard)
//! - Model name (e.g., "mistralai/Mixtral-8x7B-Instruct-v0.1")
//!
//! ## Environment Variables
//!
//! - `TOGETHER_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```rust
//! use rustycode_llm::{TogetherProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: Some(SecretString::new("your-api-key".to_string().into())),
//!     base_url: None, // Uses default https://api.together.xyz/v1/chat/completions
//!     timeout_seconds: Some(120),
//!     extra_headers: None,
//!     retry_config: None,
//! };
//! let provider = TogetherProvider::new(config);
//! ```
//!
//! ## Streaming
//!
//! Together AI uses an OpenAI-compatible streaming format (SSE) that
//! returns text chunks in real-time as they're generated.

use crate::openai_compatible::{
    build_completion_response, build_request_with_auth, convert_messages_simple, map_http_error,
    parse_openai_sse_lines, OpenAiCompatibleResponse, OpenAiModelListResponse, SseParseConfig,
    SseParseState,
};
use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::sse::SseByteBuffer;
use crate::{get_api_key, shared_client};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Together AI API endpoint
const TOGETHER_API_ENDPOINT: &str = "https://api.together.xyz/v1/chat/completions";

/// Together AI LLM provider
pub struct TogetherProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl TogetherProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| TOGETHER_API_ENDPOINT.to_string());

        // Use shared global client pool
        let client = shared_client!();

        Ok(Self {
            config,
            client,
            endpoint,
        })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "together".to_string(),
            display_name: "Together AI".to_string(),
            description: "Open-source models hosted on Together AI platform".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Together AI API key from api.together.xyz".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("your-api-key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                ],
                optional_fields: vec![],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "TOGETHER_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by open-source models.\n\n=== YOUR ROLE ===\n{context}\n\n=== RESPONSE GUIDELINES ===\n- Be direct and to the point\n- Provide clear, actionable responses\n- Focus on practical solutions\n- Avoid unnecessary verbosity\n- Get straight to the answer".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Concise,
                    special_instructions: vec![
                        "Be direct and to the point.".to_string(),
                        "Provide clear, actionable responses.".to_string(),
                        "Focus on practical, implementable solutions.".to_string(),
                        "Avoid unnecessary elaboration.".to_string(),
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
                ModelInfo {
                    model_id: "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string(),
                    display_name: "Mixtral 8x7B".to_string(),
                    description: "Open-source mixture-of-experts model".to_string(),
                    context_window: 32_768,
                    supports_tools: true,
                    use_cases: vec!["General assistance".to_string(), "Coding".to_string()],
                    cost_tier: 2,
                },
            ],
                    model_behavior_profiles: HashMap::new(),
        }
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        get_api_key!(self, "TOGETHER_API_KEY")
    }
}

#[async_trait]
impl LLMProvider for TogetherProvider {
    fn name(&self) -> &'static str {
        "together"
    }

    async fn is_available(&self) -> bool {
        // Check if API key is available
        if self.api_key().is_err() {
            return false;
        }

        // Try to make a simple request to verify connectivity
        let api_key = match self.api_key() {
            Ok(key) => key,
            Err(_) => return false,
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build();

        let client = match client {
            Ok(c) => c,
            Err(_) => return false,
        };

        let response = client
            .get("https://api.together.xyz/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        response.map(|r| r.status().is_success()).unwrap_or(false)
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let api_key = self.api_key()?;

        let response = self
            .client
            .get("https://api.together.xyz/v1/models")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to fetch models: {}", e)))?;

        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "Failed to list models: HTTP {}",
                response.status()
            )));
        }

        let models_response: OpenAiModelListResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse models response: {}", e))
        })?;

        Ok(models_response.data.iter().map(|m| m.id.clone()).collect())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.api_key()?;
        let messages = convert_messages_simple(&request);

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });
        if let Some(rf) = build_openai_response_format(&request.output_config) {
            body["response_format"] = rf;
        }

        let req = build_request_with_auth(
            self.client.post(&self.endpoint),
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
                "Together AI",
                "TOGETHER_API_KEY",
            ));
        }

        let together_response: OpenAiCompatibleResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        build_completion_response(&together_response)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let api_key = self.api_key()?;
        let messages = convert_messages_simple(&request);

        let request_body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });

        let req = build_request_with_auth(
            self.client.post(&self.endpoint),
            &api_key,
            self.config.extra_headers.as_ref(),
        );

        let response = req.json(&request_body).send().await.map_err(|e| {
            ProviderError::Network(format!("Failed to connect to Together AI: {}", e))
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
                "Together AI",
                "TOGETHER_API_KEY",
            ));
        }

        // Convert bytes stream to SSE stream using shared parser
        let bytes_stream = response.bytes_stream();
        let line_buffer = SseByteBuffer::new();
        let sse_state = SseParseState::default();
        let config = SseParseConfig::minimal();

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
        let provider = TogetherProvider::new(config).unwrap();
        assert_eq!(provider.name(), "together");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = TogetherProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = TogetherProvider::new(config);
        assert!(result.is_err());
    }
}
