//! Perplexity AI LLM provider implementation.
//!
//! This provider supports Perplexity AI's API which provides access to
//! various LLM models including their own pplx models and others.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Perplexity AI)
//! - Model name (e.g., "llama-3.1-sonar-small-128k-online", "mixtral-8x7b-instruct")
//!
//! ## Environment Variables
//!
//! - `PERPLEXITY_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```toml
//! [ai]
//! provider = "perplexity"
//! model = "llama-3.1-sonar-small-128k-online"
//! api_key = "your-api-key"
//! ```

use crate::openai_compatible::{
    build_completion_response, build_request_with_auth, convert_messages_simple, map_http_error,
    parse_openai_sse_lines, OpenAiCompatibleResponse, SseParseConfig, SseParseState,
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
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;

/// Default Perplexity API endpoint
const PERPLEXITY_API_ENDPOINT: &str = "https://api.perplexity.ai/chat/completions";

/// Perplexity AI LLM provider
pub struct PerplexityProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl PerplexityProvider {
    pub fn new(config: ProviderConfig, _default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| PERPLEXITY_API_ENDPOINT.to_string());

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
            provider_id: "perplexity".to_string(),
            display_name: "Perplexity AI".to_string(),
            description: "AI-powered search and reasoning with real-time web access".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Perplexity API key from perplexity.ai".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("pplx-...".to_string()),
                        default: None,
                        validation_pattern: Some("^pplx-.*".to_string()),
                        validation_error: Some("API key must start with 'pplx-'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.perplexity.ai".to_string()),
                        default: Some("https://api.perplexity.ai".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "PERPLEXITY_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant with access to real-time information.\n\n{context}".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Use your web search capability to provide current, accurate information.".to_string(),
                        "Cite sources when referencing specific facts or current events.".to_string(),
                        "Be direct and factual in your responses.".to_string(),
                    ],
                },
                tool_format: ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: Some(128),
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "llama-3.1-sonar-small-128k-online".to_string(),
                    display_name: "Sonar Small (Online)".to_string(),
                    description: "Fast model with real-time web search".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["Quick queries".to_string(), "Web search".to_string()],
                    cost_tier: 1,
                },
                ModelInfo {
                    model_id: "llama-3.1-sonar-large-128k-online".to_string(),
                    display_name: "Sonar Large (Online)".to_string(),
                    description: "Balanced model with web search".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General tasks".to_string(), "Research".to_string()],
                    cost_tier: 2,
                },
            ],
        }
    }

    fn api_key(&self) -> Result<String, ProviderError> {
        get_api_key!(self, "PERPLEXITY_API_KEY")
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl LLMProvider for PerplexityProvider {
    fn name(&self) -> &'static str {
        "perplexity"
    }

    async fn is_available(&self) -> bool {
        self.api_key().is_ok()
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "llama-3.1-sonar-huge-128k-online".to_string(),
            "llama-3.1-sonar-large-128k-online".to_string(),
            "llama-3.1-sonar-small-128k-online".to_string(),
            "mixtral-8x7b-instruct".to_string(),
        ])
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
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
        });
        if let Some(rf) = build_openai_response_format(&request.output_config) {
            body["response_format"] = rf;
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
                "Perplexity",
                "PERPLEXITY_API_KEY",
            ));
        }

        let perplexity_response: OpenAiCompatibleResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        build_completion_response(&perplexity_response)
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
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });

        let req = build_request_with_auth(
            self.client.post(self.endpoint()),
            &api_key,
            self.config.extra_headers.as_ref(),
        );

        let response = req.json(&request_body).send().await.map_err(|e| {
            ProviderError::Network(format!("Failed to connect to Perplexity: {}", e))
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
                "Perplexity",
                "PERPLEXITY_API_KEY",
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
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert_eq!(provider.name(), "perplexity");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = PerplexityProvider::new(config, "sonar".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = PerplexityProvider::metadata();
        assert_eq!(metadata.display_name, "Perplexity AI");
        assert_eq!(metadata.provider_id, "perplexity");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = PerplexityProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
        assert_eq!(metadata.tool_calling.max_tools_per_call, Some(128));
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = PerplexityProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"PERPLEXITY_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = PerplexityProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("sonar")));
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert_eq!(provider.endpoint(), PERPLEXITY_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("pplx-test-key"));
        config.base_url =
            Some("https://custom-perplexity.example.com/chat/completions".to_string());
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://custom-perplexity.example.com/chat/completions"
        );
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("sonar")));
        assert!(models.iter().any(|m| m.contains("mixtral")));
    }

    #[tokio::test]
    async fn test_is_available_with_key() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert!(provider.is_available().await);
    }

    #[tokio::test]
    async fn test_is_available_without_key() {
        let config = ProviderConfig {
            api_key: Some(SecretString::new(String::new().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };
        let result = PerplexityProvider::new(config, "sonar".to_string());
        if let Ok(provider) = result {
            assert!(!provider.is_available().await);
        }
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
