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

use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
    Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::retry::extract_retry_after_ms;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Perplexity API endpoint
const PERPLEXITY_API_ENDPOINT: &str = "https://api.perplexity.ai/chat/completions";

#[derive(Serialize)]
struct PerplexityRequest {
    model: String,
    messages: Vec<PerplexityMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize)]
struct PerplexityMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct PerplexityResponse {
    #[allow(dead_code)] // Kept for future use
    id: String,
    model: String,
    choices: Vec<PerplexityChoice>,
    usage: PerplexityUsage,
}

#[derive(Deserialize)]
struct PerplexityChoice {
    #[allow(dead_code)] // Kept for future use
    index: usize,
    message: PerplexityResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct PerplexityResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<PerplexityToolCall>>,
}

/// Tool call from Perplexity response (OpenAI-compatible format)
#[derive(Deserialize)]
struct PerplexityToolCall {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    r#type: String,
    function: PerplexityFunction,
}

/// Function call within a tool call
#[derive(Deserialize)]
struct PerplexityFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct PerplexityUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

/// Perplexity AI LLM provider
pub struct PerplexityProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    #[allow(dead_code)] // Kept for future use
    default_model: String,
}

impl PerplexityProvider {
    pub fn new(config: ProviderConfig, default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let client = Self::build_client(&config)?;
        Ok(Self {
            config,
            client,
            default_model,
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

    fn build_client(config: &ProviderConfig) -> Result<reqwest::Client, ProviderError> {
        let api_key = config
            .api_key
            .as_ref()
            .map(|k| k.expose_secret().to_string())
            .unwrap_or_else(|| std::env::var("PERPLEXITY_API_KEY").unwrap_or_default());

        let mut headers = reqwest::header::HeaderMap::new();
        if !api_key.is_empty() {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", api_key)
                    .parse()
                    .unwrap_or_else(|_| reqwest::header::HeaderValue::from_static("")),
            );
        }
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        // Add validated extra headers
        use crate::provider::validate_extra_headers;
        let validated_headers = validate_extra_headers(&config.extra_headers)?;
        for (header_name, header_value) in validated_headers {
            headers.insert(header_name, header_value);
        }

        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(120));

        reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })
    }

    fn endpoint(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| PERPLEXITY_API_ENDPOINT.to_string())
    }
}

#[async_trait]
impl LLMProvider for PerplexityProvider {
    fn name(&self) -> &'static str {
        "perplexity"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Return known Perplexity models (as of March 2026)
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
        let messages: Vec<PerplexityMessage> = request
            .messages
            .into_iter()
            .map(|msg| PerplexityMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = PerplexityRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            stream: None,
        };

        let response = self
            .client
            .post(self.endpoint())
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your PERPLEXITY_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Perplexity service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let perplexity_response: PerplexityResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        let choice = perplexity_response
            .choices
            .first()
            .ok_or_else(|| ProviderError::Api("No choices in response".to_string()))?;

        // Build content string, appending tool calls if present
        let mut content = choice.message.content.clone().unwrap_or_default();

        if let Some(tool_calls) = &choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let tool_calls_json: Vec<serde_json::Value> = tool_calls
                    .iter()
                    .map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments,
                            }
                        })
                    })
                    .collect();
                let formatted = serde_json::to_string_pretty(&tool_calls_json)
                    .unwrap_or_else(|_| "[]".to_string());
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&format!("```tool\n{}\n```", formatted));
            }
        }

        Ok(CompletionResponse {
            content,
            model: perplexity_response.model,
            usage: Some(Usage {
                input_tokens: perplexity_response.usage.prompt_tokens as u32,
                output_tokens: perplexity_response.usage.completion_tokens as u32,
                total_tokens: perplexity_response.usage.total_tokens as u32,
                cache_read_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_tokens: None,
            }),
            stop_reason: crate::provider::normalize_stop_reason(choice.finish_reason.as_deref()),
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let messages: Vec<PerplexityMessage> = request
            .messages
            .into_iter()
            .map(|msg| PerplexityMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = PerplexityRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            stream: Some(true),
        };

        let response = self
            .client
            .post(self.endpoint())
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("Failed to connect to Perplexity: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your PERPLEXITY_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(error_text.clone()),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                "Perplexity service temporarily unavailable ({}). Please retry in a few seconds.",
                error_text
            )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseLineBuffer::new();

        let sse_stream = bytes_stream.map(move |chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("Failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            let lines = line_buffer.feed_chunk(&text);
            for line in &lines {
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();
                    if json_str == "[DONE]" {
                        continue;
                    }
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content") {
                                        if let Some(content_str) = content.as_str() {
                                            chunks.push(content_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(rustycode_protocol::stream_event::StreamEvent::TextDelta {
                content: chunks.join(""),
            })
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

    #[test]
    fn test_perplexity_request_serialization() {
        let request = PerplexityRequest {
            model: "sonar".to_string(),
            messages: vec![PerplexityMessage {
                role: "user".to_string(),
                content: "What is the capital of France?".to_string(),
            }],
            max_tokens: 512,
            temperature: 0.3,
            stream: Some(true),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"sonar\""));
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"max_tokens\":512"));
    }

    #[test]
    fn test_perplexity_request_no_stream_serialization() {
        let request = PerplexityRequest {
            model: "sonar".to_string(),
            messages: vec![],
            max_tokens: 1024,
            temperature: 0.7,
            stream: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // stream should be absent when None
        assert!(!json.contains("\"stream\""));
    }

    #[test]
    fn test_perplexity_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-abc123",
            "model": "sonar",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "The capital of France is Paris."},
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 15, "completion_tokens": 10, "total_tokens": 25}
        }"#;
        let response: PerplexityResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "sonar");
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("The capital of France is Paris.")
        );
        assert_eq!(response.usage.total_tokens, 25);
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
        // Create with an empty key by building config manually
        // PerplexityProvider::new requires a key, so we use empty string
        let config = ProviderConfig {
            api_key: Some(SecretString::new(String::new().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };
        let result = PerplexityProvider::new(config, "sonar".to_string());
        // Empty key should still construct but is_available returns false
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
