//! GitHub Copilot LLM provider implementation.
//!
//! GitHub Copilot uses an OpenAI-compatible API with GitHub-specific authentication.
//! The provider supports GitHub tokens and Copilot-specific models.
//!
//! ## Configuration
//!
//! The provider requires:
//! - GitHub token (from GitHub settings)
//! - Model name (e.g., "gpt-4o-copilot", "gpt-4-copilot")
//!
//! ## Environment Variables
//!
//! - `GITHUB_TOKEN` - GitHub personal access token for authentication
//!
//! ## Example Configuration
//!
//! ```rust
//! use rustycode_llm::{CopilotProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: Some(SecretString::new("ghp_your-token".to_string().into())),
//!     base_url: None, // Uses default https://api.githubcopilot.com
//!     timeout_seconds: Some(120),
//!     extra_headers: None,
//!     retry_config: None,
//! };
//! let provider = CopilotProvider::new(config).unwrap();
//! ```

use crate::provider::{
    build_openai_response_format, CompletionRequest, CompletionResponse, LLMProvider,
    ProviderConfig, ProviderError, StreamChunk, Usage,
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

/// GitHub Copilot request (OpenAI-compatible format)
#[derive(Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
    usage: Option<CopilotUsage>,
    model: String,
}

#[derive(Deserialize)]
struct CopilotChoice {
    message: CopilotResponseMessage,
    finish_reason: Option<String>,
}

/// Full response message that can include tool calls
#[derive(Deserialize)]
struct CopilotResponseMessage {
    #[allow(dead_code)] // Kept for future use
    role: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<CopilotToolCall>>,
}

/// Tool call from Copilot API response (OpenAI-compatible format)
#[derive(Deserialize)]
struct CopilotToolCall {
    #[allow(dead_code)] // Kept for future use
    id: String,
    #[allow(dead_code)] // Kept for future use
    r#type: String,
    function: CopilotFunction,
}

/// Function call within a tool call
#[derive(Deserialize)]
struct CopilotFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct CopilotUsage {
    prompt_tokens: usize,
    completion_tokens: usize,
    total_tokens: usize,
}

/// GitHub Copilot LLM provider
///
/// Uses GitHub Copilot's OpenAI-compatible API endpoint with GitHub token authentication.
/// Supports Copilot-specific models like `gpt-4-copilot` and `gpt-4o-copilot`.
pub struct CopilotProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl CopilotProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let token = config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                ProviderError::Configuration(
                    "GitHub token is required. Set api_key in config or GITHUB_TOKEN env var"
                        .to_string(),
                )
            })?
            .expose_secret();

        let mut headers = reqwest::header::HeaderMap::new();

        // GitHub Copilot uses GitHub token for authentication
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", token).parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid token format: {}", e))
            })?,
        );

        // GitHub Copilot requires specific headers
        headers.insert(
            reqwest::header::HeaderName::from_static("copilot-integration-id"),
            reqwest::header::HeaderValue::from_static("vscode-chat"),
        );

        headers.insert(
            reqwest::header::HeaderName::from_static("editor-version"),
            reqwest::header::HeaderValue::from_static("vscode/1.0.0"),
        );

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

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(timeout)
            .build()
            .map_err(|e| {
                ProviderError::Configuration(format!("failed to build HTTP client: {}", e))
            })?;

        Ok(Self { config, client })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "copilot".to_string(),
            display_name: "GitHub Copilot".to_string(),
            description: "GitHub Copilot's AI models with OpenAI-compatible API".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "GitHub Token".to_string(),
                    description:
                        "Your GitHub personal access token from github.com/settings/tokens"
                            .to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("ghp_...".to_string()),
                    default: None,
                    validation_pattern: Some("^ghp_.*".to_string()),
                    validation_error: Some("GitHub token must start with 'ghp-'".to_string()),
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (optional)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some("https://api.githubcopilot.com".to_string()),
                    default: Some("https://api.githubcopilot.com".to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "GITHUB_TOKEN".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template:
                    "You are a helpful AI assistant powered by GitHub Copilot. {context}"
                        .to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Copilot models are optimized for coding tasks.".to_string(),
                        "Provide clear, concise code examples when helpful.".to_string(),
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
                    model_id: "gpt-4.1-copilot".to_string(),
                    display_name: "GPT-4.1 Copilot".to_string(),
                    description:
                        "Latest GPT-4 model with improved reasoning and coding capabilities"
                            .to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex coding tasks".to_string(),
                        "Architecture design".to_string(),
                        "Code refactoring".to_string(),
                    ],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "gpt-4o-copilot".to_string(),
                    display_name: "GPT-4o Copilot".to_string(),
                    description: "Multimodal model with strong performance across tasks"
                        .to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "General coding".to_string(),
                        "Code explanation".to_string(),
                        "Debugging".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "gpt-4o-mini-copilot".to_string(),
                    display_name: "GPT-4o Mini Copilot".to_string(),
                    description: "Fast and cost-effective for simple tasks".to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec!["Quick code fixes".to_string(), "Simple queries".to_string()],
                    cost_tier: 2,
                },
                ModelInfo {
                    model_id: "o3-mini-copilot".to_string(),
                    display_name: "o3 Mini Copilot".to_string(),
                    description: "Reasoning-optimized model for complex problem-solving"
                        .to_string(),
                    context_window: 200000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex algorithms".to_string(),
                        "Math-heavy tasks".to_string(),
                    ],
                    cost_tier: 4,
                },
            ],
        }
    }

    pub fn endpoint(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://api.githubcopilot.com")
    }
}

#[async_trait]
impl LLMProvider for CopilotProvider {
    fn name(&self) -> &'static str {
        "copilot"
    }

    async fn is_available(&self) -> bool {
        // Simple check: if we have a token and can make a basic request
        // For a more thorough check, we could call a health endpoint
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // GitHub Copilot doesn't have a public models list endpoint
        // Return known Copilot models (as of March 2026)
        Ok(vec![
            "gpt-4.1-copilot".to_string(),
            "gpt-4o-copilot".to_string(),
            "gpt-4o-mini-copilot".to_string(),
            "o1-copilot".to_string(),
            "o1-mini-copilot".to_string(),
            "o3-mini-copilot".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        let url = format!("{}/chat/completions", self.endpoint());

        // Convert messages to Copilot format
        let copilot_messages: Vec<CopilotMessage> = request
            .messages
            .into_iter()
            .map(|msg| CopilotMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = CopilotRequest {
            model: request.model,
            messages: copilot_messages,
            temperature: request.temperature.unwrap_or(0.7),
            max_tokens: request.max_tokens,
            response_format: build_openai_response_format(&request.output_config),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to send request to GitHub Copilot: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response.text().await.unwrap_or_default();

            match status.as_u16() {
                401 | 403 => return Err(ProviderError::Auth(format!(
                    "Authentication failed. Check your GITHUB_TOKEN env var. {}: {}",
                    status, text
                ))),
                404 => return Err(ProviderError::InvalidModel(text.clone())),
                429 => return Err(ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                }),
                502..=504 => return Err(ProviderError::Network(format!(
                    "GitHub Copilot service temporarily unavailable ({}). Please retry in a few seconds.",
                    text
                ))),
                _ => return Err(ProviderError::Api(format!("{}: {}", status, text))),
            }
        }

        let resp: CopilotResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse Copilot response: {}", e))
        })?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::Api("no choices in Copilot response".to_string()))?;

        let usage = resp.usage.map(|u| Usage {
            input_tokens: u.prompt_tokens as u32, // usize→u32 safe: token counts never exceed 2^32
            output_tokens: u.completion_tokens as u32,
            total_tokens: u.total_tokens as u32,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: None,
        });

        // Build content string, appending tool calls if present
        let mut content = choice.message.content.unwrap_or_default();

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
            model: resp.model,
            usage,
            stop_reason: choice.finish_reason,
            citations: None,
            thinking_blocks: None,
            structured_output: None,
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = format!("{}/chat/completions", self.endpoint());

        // Convert messages to Copilot format
        let copilot_messages: Vec<CopilotMessage> = request
            .messages
            .into_iter()
            .map(|msg| CopilotMessage {
                role: msg.role.as_ref().to_string(),
                content: msg.content.to_text(),
            })
            .collect();

        let body = CopilotRequest {
            model: request.model.clone(),
            messages: copilot_messages,
            temperature: request.temperature.unwrap_or(0.7),
            max_tokens: request.max_tokens,
            response_format: build_openai_response_format(&request.output_config),
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                ProviderError::Network(format!("failed to send request to GitHub Copilot: {}", e))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let headers = response.headers().clone();
            let text = response.text().await.unwrap_or_default();

            return match status.as_u16() {
                401 | 403 => Err(ProviderError::Auth(format!(
                    "Authentication failed. Check your GITHUB_TOKEN env var. {}: {}",
                    status, text
                ))),
                404 => Err(ProviderError::InvalidModel(text.clone())),
                429 => Err(ProviderError::RateLimited {
                    retry_delay: extract_retry_after_ms(&headers).map(Duration::from_millis),
                }),
                502..=504 => Err(ProviderError::Network(format!(
                    "GitHub Copilot service temporarily unavailable ({}). Please retry in a few seconds.",
                    text
                ))),
                _ => Err(ProviderError::Api(format!("{}: {}", status, text))),
            };
        }

        // Convert bytes stream to SSE stream
        let bytes_stream = response.bytes_stream();

        // Parse SSE events from byte stream (OpenAI-compatible format)
        let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
            let chunk = chunk_result
                .map_err(|e| ProviderError::Network(format!("failed to read chunk: {}", e)))?;
            let text = String::from_utf8_lossy(&chunk);
            let mut chunks = Vec::new();

            for line in text.lines() {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("data: ") {
                    let json_str = line.trim_start_matches("data: ").trim();

                    // GitHub Copilot sends "[DONE]" when complete (OpenAI-compatible)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::SecretString;

    fn make_config(token: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: token.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_requires_token() {
        assert!(CopilotProvider::new(make_config(None)).is_err());
    }

    #[test]
    fn test_creates_with_token() {
        assert!(CopilotProvider::new(make_config(Some("ghp_test123"))).is_ok());
    }

    #[test]
    fn test_default_endpoint() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert_eq!(p.endpoint(), "https://api.githubcopilot.com");
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("ghp_test"));
        config.base_url = Some("https://proxy.example.com".to_string());
        let p = CopilotProvider::new(config).unwrap();
        assert_eq!(p.endpoint(), "https://proxy.example.com");
    }

    #[test]
    fn test_provider_name() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert_eq!(p.name(), "copilot");
    }

    #[tokio::test]
    async fn test_is_available() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert!(p.is_available().await);

        // Creating without a token should fail
        let p_no_token = CopilotProvider::new(make_config(None));
        assert!(p_no_token.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = CopilotProvider::metadata();
        assert_eq!(metadata.display_name, "GitHub Copilot");
        assert_eq!(metadata.provider_id, "copilot");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = CopilotProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = CopilotProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"GITHUB_TOKEN".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = CopilotProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("copilot")));
    }

    #[test]
    fn test_copilot_request_serialization() {
        let request = CopilotRequest {
            model: "gpt-4o-copilot".to_string(),
            messages: vec![CopilotMessage {
                role: "user".to_string(),
                content: "Write a function".to_string(),
            }],
            temperature: 0.7,
            max_tokens: Some(2048),
            response_format: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"model\":\"gpt-4o-copilot\""));
        assert!(json.contains("\"max_tokens\":2048"));
    }

    #[test]
    fn test_copilot_request_serialization_no_max_tokens() {
        let request = CopilotRequest {
            model: "gpt-4o-copilot".to_string(),
            messages: vec![],
            temperature: 0.5,
            max_tokens: None,
            response_format: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // max_tokens should be absent when None
        assert!(!json.contains("\"max_tokens\""));
    }

    #[test]
    fn test_copilot_response_deserialization() {
        let json = r#"{
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Here is a function"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {"prompt_tokens": 20, "completion_tokens": 10, "total_tokens": 30},
            "model": "gpt-4o-copilot"
        }"#;
        let response: CopilotResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "gpt-4o-copilot");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content.as_deref(),
            Some("Here is a function")
        );
        assert!(response.usage.is_some());
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 30);
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        let models = p.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("copilot")));
    }

    #[test]
    fn test_token_required_error_message() {
        let result = CopilotProvider::new(make_config(None));
        let msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("expected error for missing token"),
        };
        // Validation should mention the env var
        assert!(
            msg.contains("GITHUB_TOKEN"),
            "Error should mention GITHUB_TOKEN, got: {}",
            msg
        );
    }
}
