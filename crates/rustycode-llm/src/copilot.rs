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

use crate::openai_compatible::{
    build_completion_response, map_http_error, parse_openai_sse_lines, OpenAiCompatibleMessage,
    OpenAiCompatibleResponse, SseParseConfig, SseParseState,
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
use crate::tools::normalize_tools_for_openai;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

/// GitHub Copilot LLM provider
///
/// Uses GitHub Copilot's OpenAI-compatible API endpoint with GitHub token authentication.
pub struct CopilotProvider {
    config: ProviderConfig,
    client: reqwest::Client,
    endpoint: String,
}

impl CopilotProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
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
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", token).parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid token format: {}", e))
            })?,
        );
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

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.githubcopilot.com".to_string());

        Ok(Self {
            config,
            client,
            endpoint,
        })
    }

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

    fn convert_messages(
        messages: Vec<crate::provider::ChatMessage>,
    ) -> Vec<OpenAiCompatibleMessage> {
        messages
            .into_iter()
            .map(|msg| OpenAiCompatibleMessage {
                role: msg.role.as_ref().to_string(),
                content: Some(msg.content.to_text()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect()
    }

    fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

#[async_trait]
impl LLMProvider for CopilotProvider {
    fn name(&self) -> &'static str {
        "copilot"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .map_or(false, |k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
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
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .map(|t| normalize_tools_for_openai(t));

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
        });
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(rf) = build_openai_response_format(&request.output_config) {
            body["response_format"] = rf;
        }
        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

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
                "GitHub Copilot",
                "GITHUB_TOKEN",
            ));
        }

        let copilot_response: OpenAiCompatibleResponse = response.json().await.map_err(|e| {
            ProviderError::Serialization(format!("failed to parse Copilot response: {}", e))
        })?;

        build_completion_response(&copilot_response)
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let url = format!("{}/chat/completions", self.endpoint());
        let messages = Self::convert_messages(request.messages.clone());
        let tools = request
            .tools
            .as_ref()
            .map(|t| normalize_tools_for_openai(t));

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "stream": true
        });
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

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
                "GitHub Copilot",
                "GITHUB_TOKEN",
            ));
        }

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
        assert!(
            msg.contains("GITHUB_TOKEN"),
            "Error should mention GITHUB_TOKEN, got: {}",
            msg
        );
    }

    #[test]
    fn test_config_returns_some() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert!(p.config().is_some());
    }
}
