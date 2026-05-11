//! Google Gemini LLM provider implementation.
//!
//! This provider supports Google's Gemini API which provides access to
//! language models like Gemini 2.5 Pro, Gemini 2.0 Flash, and more.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Google AI Studio)
//! - Model name (e.g., "gemini-2.5-pro", "gemini-2.0-flash")
//!
//! ## Environment Variables
//!
//! - `GOOGLE_API_KEY` - API key for authentication
//!
//! ## Example Configuration
//!
//! ```rust
//! use rustycode_llm::{GeminiProvider, ProviderConfig};
//! use secrecy::SecretString;
//!
//! let config = ProviderConfig {
//!     api_key: Some(SecretString::new("AIza-your-api-key".to_string().into())),
//!     base_url: Some("https://generativelanguage.googleapis.com".to_string()),
//!     timeout_seconds: Some(180),
//!     extra_headers: None,
//!     retry_config: None,
//! };
//! let provider = GeminiProvider::new(config).unwrap();
//! ```
//!
//! ## Streaming
//!
//! Gemini uses a streaming-specific endpoint (`streamGenerateContent`) that
//! returns Server-Sent Events (SSE) with real-time text generation.

use crate::auth::{ApiKeyHeaderAuth, AuthMethod};
use crate::model_cache::ModelCache;
use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::transport::{HttpSseTransport, HttpTransport};
use crate::wire::gemini::GeminiProtocol;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;

/// Default timeout in seconds for Gemini requests.
const DEFAULT_TIMEOUT_SECS: u64 = 180;

/// Extract retry delay from a Gemini 429 error response.
///
/// Gemini 429 responses contain JSON like:
/// ```json
/// {"error":{"code":429,"message":"...retry after 60s...","status":"RESOURCE_EXHAUSTED"}}
/// ```
///
/// This function parses the error text looking for patterns like "retry after Xs",
/// "retry in X seconds", or "retry in X sec" and returns the extracted duration.
fn extract_gemini_retry_delay(error_text: &str) -> Option<std::time::Duration> {
    let message = if let Ok(val) = serde_json::from_str::<serde_json::Value>(error_text) {
        val.get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    let search_text = message.as_deref().unwrap_or(error_text);
    let lower = search_text.to_lowercase();

    for prefix in &["retry after ", "retry in "] {
        if let Some(pos) = lower.find(prefix) {
            let after = &lower[pos + prefix.len()..];
            if let Some(secs) = extract_leading_number(after) {
                return Some(std::time::Duration::from_secs(secs));
            }
        }
    }

    None
}

/// Extract a leading integer from a string slice (e.g., "60s..." → 60).
fn extract_leading_number(s: &str) -> Option<u64> {
    let num_str: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    num_str.parse().ok()
}

/// Default base URL for the Gemini API.
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Google Gemini LLM provider.
///
/// Uses two routes composed from `GeminiProtocol` + transport + auth:
/// - **chat_route**: `generateContent` endpoint via `HttpTransport`
/// - **stream_route**: `streamGenerateContent?alt=sse` endpoint via `HttpSseTransport`
pub struct GeminiProvider {
    config: ProviderConfig,
    chat_route: Route,
    stream_route: Route,
    model_cache: ModelCache,
}

impl GeminiProvider {
    /// Create a new Gemini provider with config validation.
    pub fn new(config: ProviderConfig) -> Result<Self> {
        Self::metadata().validate_config(&config)?;
        Self::build(config)
    }

    /// Create provider without config validation (for custom endpoints/proxies).
    pub fn new_without_validation(config: ProviderConfig) -> Result<Self> {
        Self::build(config)
    }

    fn build(config: ProviderConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Google API key is required. Set api_key in config or GOOGLE_API_KEY env var"
                )
            })?
            .clone();

        let timeout = config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let base = config
            .base_url
            .as_deref()
            .unwrap_or(DEFAULT_BASE_URL)
            .trim_end_matches('/');

        // Gemini uses x-goog-api-key header for authentication
        let auth = Box::new(ApiKeyHeaderAuth::new("x-goog-api-key", api_key));

        // Chat route: non-streaming generateContent
        let chat_endpoint = format!("{base}/v1beta/models/{{model}}:generateContent");
        let chat_route = Route::new(
            chat_endpoint,
            Box::new(GeminiProtocol),
            Box::new(HttpTransport::new(timeout)?),
            auth.clone_box(),
        )
        .with_name("gemini-chat");

        // Stream route: streaming streamGenerateContent with SSE
        let stream_endpoint =
            format!("{base}/v1beta/models/{{model}}:streamGenerateContent?alt=sse");
        let stream_route = Route::new(
            stream_endpoint,
            Box::new(GeminiProtocol),
            Box::new(HttpSseTransport::new(timeout)?),
            auth,
        )
        .with_name("gemini-stream");

        Ok(Self {
            config,
            chat_route,
            stream_route,
            model_cache: ModelCache::new(),
        })
    }

    /// Get metadata for this provider.
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "gemini".to_string(),
            display_name: "Google Gemini".to_string(),
            description: "Multimodal AI assistant with strong reasoning and creative capabilities".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Google API key from console.cloud.google.com".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("AIza...".to_string()),
                        default: None,
                        validation_pattern: Some("^AIza.*".to_string()),
                        validation_error: Some("API key must start with 'AIza'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "GEMINI_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are RustyCode, a coding assistant.\n\n{context}\n\n## Gemini Guidance\n- Order reasoning: Context → Task → Constraints\n- Place critical instructions at the END for strongest attention\n- State assumptions and proceed rather than asking for clarification\n- Separate expected vs actual results when verifying".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Keep function descriptions concise; include enum constraints for enum parameters.".to_string(),
                        "For large contexts, re-read key requirements before finalizing output.".to_string(),
                    ],
                },
                tool_format: ToolFormat::GeminiTools,
            },
            tool_calling: ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![
                ModelInfo {
                    model_id: "gemini-2.5-pro".to_string(),
                    display_name: "Gemini 2.5 Pro".to_string(),
                    description: "Latest model with advanced reasoning".to_string(),
                    context_window: 1_000_000,
                    supports_tools: true,
                    use_cases: vec!["Complex reasoning".to_string(), "Large context analysis".to_string()],
                    cost_tier: 4,
                },
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }

    /// Build the chat endpoint URL for a given model.
    pub fn endpoint(&self, model: &str) -> String {
        let base = self.config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        format!("{base}/v1beta/models/{model}:generateContent")
    }

    /// Build the streaming endpoint URL for a given model.
    pub fn stream_endpoint(&self, model: &str) -> String {
        let base = self.config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL);
        format!("{base}/v1beta/models/{model}:streamGenerateContent?alt=sse")
    }

    /// Resolve the model-specific endpoint by replacing `{model}` placeholder.
    fn resolve_chat_endpoint(&self, model: &str) -> String {
        self.chat_route.endpoint.replace("{model}", model)
    }

    /// Resolve the model-specific stream endpoint by replacing `{model}` placeholder.
    fn resolve_stream_endpoint(&self, model: &str) -> String {
        self.stream_route.endpoint.replace("{model}", model)
    }
}

#[async_trait]
impl LLMProvider for GeminiProvider {
    fn name(&self) -> &'static str {
        "gemini"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        const FALLBACK: &[&str] = &[
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.0-flash",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
            "gemini-1.5-flash-8b",
        ];
        let base = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://generativelanguage.googleapis.com")
            .trim_end_matches('/');
        let cache = &self.model_cache;
        let config = &self.config;
        cache
            .fetch_or_fallback(FALLBACK, || async {
                let client = reqwest::Client::new();
                let key = config
                    .api_key
                    .as_ref()
                    .ok_or_else(|| ProviderError::Auth("No API key".to_string()))?;
                let url = format!("{base}/v1beta/models?key={}", key.expose_secret());
                let resp = client
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                    .map_err(|e| ProviderError::Network(e.to_string()))?;
                let body: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| ProviderError::Api(e.to_string()))?;
                let models = body
                    .get("models")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| {
                                m.get("name")
                                    .and_then(|n| n.as_str())
                                    .map(|n| n.strip_prefix("models/").unwrap_or(n).to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(models)
            })
            .await
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        // Resolve the model-specific endpoint (replace {model} placeholder)
        let resolved_endpoint = self.resolve_chat_endpoint(&request.model);
        let temp_route = Route::new(
            resolved_endpoint,
            self.chat_route.protocol.clone_box(),
            Box::new(
                HttpTransport::new(self.config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            self.chat_route.auth.clone_box(),
        );

        let response = temp_route.execute(&request, None).await.map_err(|e| {
            let msg = e.to_string();
            // Classify errors for better retry/recovery
            if msg.contains("HTTP error 401") || msg.contains("HTTP error 403") {
                ProviderError::Auth(format!(
                    "Authentication failed. Check your GEMINI_API_KEY env var. {msg}"
                ))
            } else if msg.contains("HTTP error 404") {
                ProviderError::InvalidModel(msg)
            } else if msg.contains("HTTP error 429") {
                ProviderError::RateLimited {
                    retry_delay: extract_gemini_retry_delay(&msg),
                }
            } else if msg.contains("HTTP error 502")
                || msg.contains("HTTP error 503")
                || msg.contains("HTTP error 504")
            {
                ProviderError::Network(format!("Gemini service temporarily unavailable. {msg}"))
            } else {
                ProviderError::Api(msg)
            }
        })?;

        // Handle structured output extraction
        let wants_structured_output = request
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_some_and(|f| {
                matches!(f.format_type, crate::provider::OutputFormatType::JsonSchema)
            });

        let structured_output = if wants_structured_output {
            match serde_json::from_str::<serde_json::Value>(&response.content) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!("Structured output JSON parse failed: {e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(CompletionResponse {
            model: request.model.clone(),
            structured_output,
            ..response
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        // Resolve the model-specific endpoint (replace {model} placeholder)
        let resolved_endpoint = self.resolve_stream_endpoint(&request.model);
        let temp_route = Route::new(
            resolved_endpoint,
            self.stream_route.protocol.clone_box(),
            Box::new(
                HttpSseTransport::new(self.config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            self.stream_route.auth.clone_box(),
        );

        let stream = temp_route
            .execute_stream(&request, None)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("HTTP error 401") || msg.contains("HTTP error 403") {
                    ProviderError::Auth(format!(
                        "Authentication failed. Check your GEMINI_API_KEY env var. {msg}"
                    ))
                } else if msg.contains("HTTP error 404") {
                    ProviderError::InvalidModel(msg)
                } else if msg.contains("HTTP error 429") {
                    ProviderError::RateLimited {
                        retry_delay: extract_gemini_retry_delay(&msg),
                    }
                } else {
                    ProviderError::Api(msg)
                }
            })?;

        // Map Result<StreamEvent> to StreamChunk
        let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::Network(e.to_string())));

        Ok(Box::pin(chunk_stream))
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
            timeout_seconds: None,
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_requires_api_key() {
        assert!(GeminiProvider::new(make_config(None)).is_err());
    }

    #[test]
    fn test_creates_with_api_key() {
        assert!(GeminiProvider::new(make_config(Some("AIzaTest123"))).is_ok());
    }

    #[test]
    fn test_name() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert_eq!(p.name(), "gemini");
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("AIzaTest123"));
        config.base_url = Some("https://proxy.example.com".to_string());
        let p = GeminiProvider::new(config).unwrap();
        let endpoint = p.endpoint("gemini-pro");
        assert!(endpoint.contains("proxy.example.com"));
        assert!(!endpoint.contains("test-key"));
    }

    #[test]
    fn test_stream_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let endpoint = p.stream_endpoint("gemini-pro");
        assert!(endpoint.contains("streamGenerateContent"));
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = GeminiProvider::metadata();
        assert_eq!(metadata.display_name, "Google Gemini");
        assert_eq!(metadata.provider_id, "gemini");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = GeminiProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.streaming_support);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = GeminiProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"GEMINI_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_tool_format() {
        let metadata = GeminiProvider::metadata();
        assert!(matches!(
            metadata.prompt_template.tool_format,
            crate::provider_metadata::ToolFormat::GeminiTools
        ));
    }

    #[test]
    fn test_default_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let endpoint = p.endpoint("gemini-2.5-pro");
        assert!(endpoint.starts_with("https://generativelanguage.googleapis.com"));
        assert!(endpoint.contains("gemini-2.5-pro"));
        assert!(endpoint.contains("generateContent"));
    }

    #[test]
    fn test_custom_endpoint_used() {
        let mut config = make_config(Some("AIzaTest123"));
        config.base_url = Some("https://my-gemini-proxy.example.com".to_string());
        let p = GeminiProvider::new(config).unwrap();
        let endpoint = p.endpoint("gemini-pro");
        assert!(endpoint.starts_with("https://my-gemini-proxy.example.com"));
    }

    #[test]
    fn test_chat_route_has_model_placeholder() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert!(p.chat_route.endpoint.contains("{model}"));
        assert!(p.chat_route.endpoint.contains("generateContent"));
    }

    #[test]
    fn test_stream_route_has_model_placeholder() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert!(p.stream_route.endpoint.contains("{model}"));
        assert!(p.stream_route.endpoint.contains("streamGenerateContent"));
        assert!(p.stream_route.endpoint.contains("alt=sse"));
    }

    #[test]
    fn test_resolve_chat_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let resolved = p.resolve_chat_endpoint("gemini-2.5-pro");
        assert!(resolved.contains("gemini-2.5-pro"));
        assert!(!resolved.contains("{model}"));
    }

    #[test]
    fn test_resolve_stream_endpoint() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let resolved = p.resolve_stream_endpoint("gemini-2.0-flash");
        assert!(resolved.contains("gemini-2.0-flash"));
        assert!(!resolved.contains("{model}"));
    }

    #[test]
    fn test_route_names() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert_eq!(p.chat_route.name(), "gemini-chat");
        assert_eq!(p.stream_route.name(), "gemini-stream");
    }

    #[tokio::test]
    async fn test_list_models_returns_fallback_on_network_error() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        let models = p.list_models().await.unwrap();
        assert!(models
            .iter()
            .any(|m| m == "gemini-2.5-pro" || m == "gemini-2.0-flash"));
    }

    #[test]
    fn test_new_without_validation() {
        let config = make_config(Some("AIzaTest123"));
        let provider = GeminiProvider::new_without_validation(config).unwrap();
        assert_eq!(provider.name(), "gemini");
    }

    #[tokio::test]
    async fn test_is_available_with_key() {
        let p = GeminiProvider::new(make_config(Some("AIzaTest123"))).unwrap();
        assert!(p.is_available().await);
    }

    #[tokio::test]
    async fn test_is_available_without_key() {
        let result = GeminiProvider::new_without_validation(make_config(None));
        assert!(result.is_err());
    }
}
