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

use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk, Usage,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};

// Import macros exported at crate root
use crate::retry::extract_retry_after_ms;
use crate::{build_request, get_api_key, shared_client};
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// Default Together AI API endpoint
const TOGETHER_API_ENDPOINT: &str = "https://api.together.xyz/v1/chat/completions";

#[derive(Serialize)]
struct TogetherRequest {
    model: String,
    messages: Vec<TogetherMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct TogetherMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct TogetherResponse {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    object: String,
    #[allow(dead_code)] // Kept for future use
    created: u64,
    model: String,
    choices: Vec<TogetherChoice>,
    usage: TogetherUsage,
}

#[derive(Deserialize)]
struct TogetherChoice {
    #[allow(dead_code)] // Kept for future use
    index: usize,
    message: TogetherResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct TogetherResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<TogetherToolCall>>,
}

/// Tool call from Together AI response (OpenAI-compatible format)
#[derive(Deserialize)]
struct TogetherToolCall {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    r#type: String,
    function: TogetherFunction,
}

/// Function call within a tool call
#[derive(Deserialize)]
struct TogetherFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct TogetherUsage {
    #[allow(dead_code)] // Kept for future use
    prompt_tokens: usize,
    #[allow(dead_code)] // Kept for future use
    completion_tokens: usize,
    total_tokens: usize,
}

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
        }
    }

    fn get_api_key(&self) -> Result<String, ProviderError> {
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
        if self.get_api_key().is_err() {
            return false;
        }

        // Try to make a simple request to verify connectivity
        let api_key = match self.get_api_key() {
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
        let api_key = self.get_api_key()?;

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

        #[derive(Deserialize)]
        struct ModelsResponse {
            data: Vec<ModelData>,
        }

        #[derive(Deserialize)]
        struct ModelData {
            id: String,
        }

        let models_response: ModelsResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse models response: {}", e))
        })?;

        Ok(models_response.data.iter().map(|m| m.id.clone()).collect())
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let api_key = self.get_api_key()?;

        // Build messages vector
        let mut messages = Vec::new();

        // Add system prompt if provided
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(TogetherMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        // Add messages from request
        for msg in request.messages {
            messages.push(TogetherMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            });
        }

        let body = TogetherRequest {
            model: request.model,
            messages,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature.unwrap_or(0.7),
            response_format: build_openai_response_format(&request.output_config),
        };

        // Build request with provider-specific headers
        let req = build_request!(
            self.client.post(&self.endpoint),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(format!("Failed to send request: {}", e)))?;

        if !response.status().is_success() {
            // Capture headers early so we can parse Retry-After for rate limits
            let headers = response.headers().clone();
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());

            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your TOGETHER_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Together AI service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        let together_response: TogetherResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("Failed to parse response: {}", e))
        })?;

        let choice = together_response
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
            model: together_response.model,
            usage: Some(Usage {
                input_tokens: together_response.usage.prompt_tokens as u32,
                output_tokens: together_response.usage.completion_tokens as u32,
                total_tokens: together_response.usage.total_tokens as u32,
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
        let api_key = self.get_api_key()?;

        // Build messages vector
        let mut messages = Vec::new();

        // Add system prompt if provided
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(TogetherMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        // Add messages from request
        for msg in request.messages {
            messages.push(TogetherMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            });
        }

        let request_body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });

        // Build request with provider-specific headers
        let req = build_request!(
            self.client.post(&self.endpoint),
            headers = [
                ("Authorization", format!("Bearer {}", api_key)),
                ("Content-Type", "application/json"),
            ],
            extra_headers = &self.config.extra_headers
        );

        let response = req.json(&request_body).send().await.map_err(|e| {
            ProviderError::Network(format!("Failed to connect to Together AI: {}", e))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read error".to_string());
            return Err(match status.as_u16() {
                401 | 403 => ProviderError::Auth(format!(
                    "Authentication failed. Check your TOGETHER_API_KEY env var. {}",
                    error_text
                )),
                404 => ProviderError::InvalidModel(format!("model not found: {}", error_text)),
                429 => ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                },
                502..=504 => ProviderError::Network(format!(
                    "Together AI service temporarily unavailable ({}). Please retry in a few seconds.",
                    error_text
                )),
                _ => ProviderError::Api(format!("{}: {}", status, error_text)),
            });
        }

        // Convert bytes stream to SSE stream (Together uses OpenAI-compatible format)
        let bytes_stream = response.bytes_stream();
        let line_buffer = crate::sse::SseLineBuffer::new();

        let sse_stream = bytes_stream.map(move |chunk_result| {
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
