use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

// Re-export message content types and Usage from protocol
pub use rustycode_protocol::llm::Usage;
pub use rustycode_protocol::{ContentBlock, ImageSource, MessageContent};

// Re-export message types from types/message
pub use crate::types::config::{
    build_gemini_response_schema, build_openai_response_format, EffortLevel, OutputConfig,
    OutputFormat, OutputFormatType, ProviderConfig, ThinkingConfig, ThinkingDisplay, ThinkingType,
};
pub use crate::types::error::{sanitize_error_message, ProviderError};
pub use crate::types::message::{
    resolve_image_to_base64, ApiMode, ChatMessage, MessageRole, ProviderType, SkillRef,
};
pub use crate::types::request::CompletionRequest;
pub use crate::types::response::{
    normalize_stop_reason, Citation, CompletionResponse, ThinkingBlock,
};
pub use crate::types::streaming::StreamChunk;
#[allow(unused_imports)]
pub(crate) use crate::types::streaming::{ContentBlockType, ContentDelta, SSEEvent};

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Whether this provider can emit streamed turn events.
    ///
    /// Providers that only support request/response completions should
    /// override this to `false` so higher-level callers can skip the
    /// streaming path entirely.
    fn supports_streaming(&self) -> bool {
        true
    }

    async fn is_available(&self) -> bool;

    async fn list_models(&self) -> Result<Vec<String>, ProviderError>;

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError>;

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError>;

    fn config(&self) -> Option<&ProviderConfig> {
        None
    }
}

// Macros for reducing boilerplate in provider implementations

/// Macro for getting shared global HTTP client
///
/// # Usage
/// ```ignore
/// impl MyProvider {
///     fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
///         let client = shared_client!();
///         Ok(Self { config, client })
///     }
/// }
/// ```
#[macro_export]
macro_rules! shared_client {
    () => {{
        use $crate::client_pool::global_client;
        (*global_client()).clone()
    }};
}

/// Macro for building HTTP request with provider-specific headers
///
/// # Usage
/// ```ignore
/// let api_key = self.config.api_key.as_ref().unwrap().expose_secret();
/// let mut req = build_request!(
///     self.client.post(&url),
///     headers = [
///         ("Authorization", format!("Bearer {}", api_key)),
///         ("Content-Type", "application/json"),
///     ],
///     extra_headers = &self.config.extra_headers
/// );
/// ```
#[macro_export]
macro_rules! build_request {
    ($base_req:expr, headers = [$(($key:expr, $val:expr)),* $(,)?], extra_headers = $extra_headers:expr) => {{
        let mut req = $base_req;

        // Add standard headers
        $(
            req = req.header($key, $val);
        )*

        // Add extra headers from config (if provided)
        if let Some(extra) = &$extra_headers {
            use $crate::provider::validate_extra_headers;
            let validated = match validate_extra_headers(&Some(extra.clone())) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Skipping invalid extra_headers: {}", e);
                    Vec::new()
                }
            };
            for (name, value) in validated {
                req = req.header(name, value);
            }
        }

        req
    }};
}

/// Macro for generating API key retrieval logic with environment variable fallback
///
/// # Usage
/// ```ignore
/// impl MyProvider {
///     get_api_key!(self, "MY_PROVIDER_API_KEY");
/// }
/// ```
#[macro_export]
macro_rules! get_api_key {
    ($self:expr, $env_var:expr) => {{
        {
            let config_key = $self
                .config
                .api_key
                .as_ref()
                .map(|k| k.expose_secret().to_string());
            let env_key = std::env::var($env_var).ok();
            config_key.or(env_key).ok_or_else(|| {
                $crate::provider::ProviderError::Configuration(
                    concat!(
                        "API key required. Set api_key in config or ",
                        $env_var,
                        " env var"
                    )
                    .to_string(),
                )
            })
        }
    }};
}

/// Macro for implementing standard LLMProvider trait methods
///
/// # Usage
/// ```ignore
/// impl LLMProvider for MyProvider {
///     provider_common!(my_provider, vec!["model1".to_string(), "model2".to_string()]);
///
///     async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ProviderError> {
///         // custom implementation
///     }
///
///     async fn complete_stream(&self, request: CompletionRequest) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
///         // custom implementation
///     }
/// }
/// ```
#[macro_export]
macro_rules! provider_common {
    ($name:expr, $models:expr) => {
        fn name(&self) -> &'static str {
            $name
        }

        async fn is_available(&self) -> bool {
            self.config
                .api_key
                .as_ref()
                .is_some_and(|k| !k.expose_secret().is_empty())
        }

        async fn list_models(&self) -> Result<Vec<String>, $crate::provider::ProviderError> {
            Ok($models)
        }

        fn config(&self) -> Option<&$crate::provider::ProviderConfig> {
            Some(&self.config)
        }
    };
}

/// Macro for converting ChatMessage to provider-specific message format
///
/// # Usage
/// ```ignore
/// let messages: Vec<ProviderMessage> = convert_messages!(request.messages, ProviderMessage {
///     role: msg.role,
///     content: msg.content,
/// });
/// ```
#[macro_export]
macro_rules! convert_messages {
    ($input:expr, $msg_ctor:expr) => {{
        $input.into_iter().map(|msg| $msg_ctor).collect::<Vec<_>>()
    }};
}

/// Macro for parsing OpenAI-compatible SSE streaming responses
///
/// # Usage
/// ```ignore
/// let sse_stream = bytes_stream.map(|chunk_result| -> StreamChunk {
///     let chunk = chunk_result.map_err(|e| ProviderError::Network(format!("Failed to read chunk: {}", e)))?;
///     let text = String::from_utf8_lossy(&chunk);
///     let mut chunks = Vec::new();
///
///     parse_openai_sse!(text, chunks);
///
///     Ok(chunks.join(""))
/// });
/// ```
#[macro_export]
macro_rules! parse_openai_sse {
    ($text:expr, $chunks:expr) => {
        for line in $text.lines() {
            if line.is_empty() {
                continue;
            }
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
                                        if !content_str.is_empty() {
                                            $chunks.push(content_str.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
}

/// Validate and sanitize extra headers for security
///
/// This function prevents header injection attacks by:
/// 1. Whitelisting allowed headers
/// 2. Blocking override of security-critical headers
/// 3. Validating header values for CRLF injection
///
pub fn validate_extra_headers(
    extra_headers: &Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<(reqwest::header::HeaderName, reqwest::header::HeaderValue)>, ProviderError> {
    let mut validated_headers = Vec::new();

    if let Some(headers) = extra_headers {
        for (key, value) in headers {
            // Block override of security-critical headers
            match key.to_lowercase().as_str() {
                "authorization"
                | "proxy-authorization"
                | "www-authenticate"
                | "proxy-authenticate" => {
                    return Err(ProviderError::Configuration(format!(
                        "cannot override security header '{}' via extra_headers",
                        key
                    )));
                }
                "host" | "content-type" | "content-length" | "transfer-encoding" => {
                    return Err(ProviderError::Configuration(format!(
                        "cannot override '{}' header via extra_headers",
                        key
                    )));
                }
                _ => {}
            }

            // Validate for CRLF injection (prevent header splitting)
            if value.contains('\r') || value.contains('\n') {
                return Err(ProviderError::Configuration(format!(
                    "header value for '{}' contains invalid newline characters",
                    key
                )));
            }

            // Parse header name and value
            let header_name =
                reqwest::header::HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
                    ProviderError::Configuration(format!("invalid header name '{}': {}", key, e))
                })?;

            let header_value = value.parse().map_err(|e| {
                ProviderError::Configuration(format!("invalid header value for '{}': {}", key, e))
            })?;

            validated_headers.push((header_name, header_value));
        }
    }

    Ok(validated_headers)
}

/// Macro for building HTTP client with standard timeout and headers
///
/// # Usage
/// ```ignore
/// let client = build_http_client!(
///     config,
///     headers,
///     timeout_seconds = 120,
///     connect_timeout = 10
/// );
/// ```
#[macro_export]
macro_rules! build_http_client {
    ($config:expr, $headers:expr, timeout_seconds = $timeout_secs:expr, connect_timeout = $connect_secs:expr) => {{
        use std::time::Duration;

        let timeout = Duration::from_secs($config.timeout_seconds.unwrap_or($timeout_secs));

        reqwest::Client::builder()
            .default_headers($headers)
            .timeout(timeout)
            .connect_timeout(Duration::from_secs($connect_secs))
            .build()
            .map_err(|e| {
                $crate::provider::ProviderError::Configuration(format!(
                    "failed to build HTTP client: {}",
                    e
                ))
            })
    }};
}

/// Validate an endpoint URL for security and correctness.
///
/// Ensures the endpoint uses HTTPS (or HTTP for localhost), does not embed
/// credentials, and does not include query strings or fragments.
pub fn validate_endpoint(endpoint: &str) -> anyhow::Result<()> {
    let url = reqwest::Url::parse(endpoint)?;

    match url.scheme() {
        "https" => {}
        "http" if matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1")) => {}
        scheme => anyhow::bail!("unsupported endpoint scheme: {}", scheme),
    }

    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("endpoint must not embed credentials");
    }

    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("endpoint must not include query strings or fragments");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::streaming::SSEEvent;
    use serde_json::{json, Value};

    #[test]
    fn test_thinking_type_serialization() {
        // Test Adaptive serialization/deserialization
        let adaptive_json = r#""adaptive""#;
        let thinking_type: ThinkingType = serde_json::from_str(adaptive_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Adaptive);
        assert_eq!(
            serde_json::to_string(&thinking_type).unwrap(),
            adaptive_json
        );

        // Test Enabled serialization
        let enabled_json = r#""enabled""#;
        let thinking_type: ThinkingType = serde_json::from_str(enabled_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Enabled);
        assert_eq!(serde_json::to_string(&thinking_type).unwrap(), enabled_json);

        // Test Disabled serialization
        let disabled_json = r#""disabled""#;
        let thinking_type: ThinkingType = serde_json::from_str(disabled_json).unwrap();
        assert_eq!(thinking_type, ThinkingType::Disabled);
        assert_eq!(
            serde_json::to_string(&thinking_type).unwrap(),
            disabled_json
        );
    }

    #[test]
    fn test_thinking_display_serialization() {
        let summarized = json!("summarized");
        let display: ThinkingDisplay = serde_json::from_str(&summarized.to_string()).unwrap();
        assert_eq!(display, ThinkingDisplay::Summarized);

        let omitted = json!("omitted");
        let display: ThinkingDisplay = serde_json::from_str(&omitted.to_string()).unwrap();
        assert_eq!(display, ThinkingDisplay::Omitted);
    }

    #[test]
    fn test_thinking_config_adaptive() {
        let config = ThinkingConfig::adaptive();
        assert_eq!(config.thinking_type, ThinkingType::Adaptive);
        assert!(config.display.is_none());
        assert!(config.budget_tokens.is_none());
    }

    #[test]
    fn test_thinking_config_enabled() {
        let config = ThinkingConfig::enabled(10000);
        assert_eq!(config.thinking_type, ThinkingType::Enabled);
        assert_eq!(config.budget_tokens, Some(10000));
        assert!(config.display.is_none());
    }

    #[test]
    fn test_thinking_config_with_display() {
        let config = ThinkingConfig::enabled(10000).with_display(ThinkingDisplay::Omitted);
        assert_eq!(config.thinking_type, ThinkingType::Enabled);
        assert_eq!(config.budget_tokens, Some(10000));
        assert_eq!(config.display, Some(ThinkingDisplay::Omitted));
    }

    #[test]
    fn test_thinking_config_with_budget() {
        let config = ThinkingConfig::adaptive().with_budget(20000);
        assert_eq!(config.thinking_type, ThinkingType::Adaptive);
        assert_eq!(config.budget_tokens, Some(20000));
    }

    #[test]
    fn test_thinking_config_serialization() {
        let config = ThinkingConfig::enabled(10000).with_display(ThinkingDisplay::Omitted);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["type"], "enabled");
        assert_eq!(value["budget_tokens"], 10000);
        assert_eq!(value["display"], "omitted");
    }

    #[test]
    fn test_thinking_config_serialization_adaptive() {
        let config = ThinkingConfig::adaptive().with_budget(20000);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["type"], "adaptive");
        assert_eq!(value["budget_tokens"], 20000);
        // display should be omitted if None
        assert!(value.get("display").is_none());
    }

    #[test]
    fn test_effort_level_serialization() {
        assert_eq!(
            serde_json::to_string(&EffortLevel::Max).unwrap(),
            r#""max""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Xhigh).unwrap(),
            r#""xhigh""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::High).unwrap(),
            r#""high""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Medium).unwrap(),
            r#""medium""#
        );
        assert_eq!(
            serde_json::to_string(&EffortLevel::Low).unwrap(),
            r#""low""#
        );
    }

    #[test]
    fn test_output_config_with_effort() {
        let config = OutputConfig::with_effort(EffortLevel::High);
        assert_eq!(config.effort, Some(EffortLevel::High));

        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(value["effort"], "high");
    }

    #[test]
    fn test_output_config_serialization_no_effort() {
        let config = OutputConfig {
            effort: None,
            format: None,
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();
        // When effort is None, the entire object might serialize differently
        // depending on skip_serializing_if
        assert!(value.get("effort").is_none());
    }

    #[test]
    fn test_completion_request_with_thinking_config() {
        let request = CompletionRequest::new(
            "claude-opus-4-6".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_thinking_config(ThinkingConfig::adaptive());

        assert!(request.thinking.is_some());
        assert_eq!(
            request.thinking.as_ref().unwrap().thinking_type,
            ThinkingType::Adaptive
        );
    }

    #[test]
    fn test_completion_request_with_output_config() {
        let request = CompletionRequest::new(
            "claude-sonnet-4-20250214".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_output_config(OutputConfig::with_effort(EffortLevel::Max));

        assert!(request.output_config.is_some());
        assert_eq!(
            request.output_config.as_ref().unwrap().effort,
            Some(EffortLevel::Max)
        );
    }

    #[test]
    fn test_completion_request_with_effort() {
        let request = CompletionRequest::new(
            "claude-opus-4-6".to_string(),
            vec![ChatMessage::user("Test".to_string())],
        )
        .with_effort(EffortLevel::High);

        assert!(request.output_config.is_some());
        assert_eq!(
            request.output_config.as_ref().unwrap().effort,
            Some(EffortLevel::High)
        );
    }

    #[test]
    fn test_output_config_with_json_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer"}
            },
            "required": ["name", "age"]
        });
        let config = OutputConfig::with_json_schema(schema.clone());

        assert!(config.effort.is_none());
        assert!(config.format.is_some());
        let format = config.format.as_ref().unwrap();
        assert_eq!(format.format_type, OutputFormatType::JsonSchema);
        assert_eq!(format.json_schema.as_ref(), Some(&schema));
    }

    #[test]
    fn test_output_config_json_schema_serialization() {
        let schema = json!({"type": "object", "properties": {"x": {"type": "number"}}});
        let config = OutputConfig::with_json_schema(schema);
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert!(value.get("format").is_some());
        let format = &value["format"];
        assert_eq!(format["type"], "json_schema");
        assert!(format.get("schema").is_some());
    }

    #[test]
    fn test_output_config_effort_and_format_together() {
        let schema = json!({"type": "object"});
        let config = OutputConfig {
            effort: Some(EffortLevel::High),
            format: Some(OutputFormat::json_schema(schema)),
        };
        let serialized = serde_json::to_string(&config).unwrap();
        let value: Value = serde_json::from_str(&serialized).unwrap();

        assert_eq!(value["effort"], "high");
        assert!(value.get("format").is_some());
    }

    #[test]
    fn test_thinking_type_supports_opus_4_5() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.5-20250514"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-6"));
    }

    #[test]
    fn test_thinking_type_supports_opus_4_6() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.6-20250214"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-20250214"));
    }

    #[test]
    fn test_thinking_type_supports_sonnet_4_5() {
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.5-20250514"));
    }

    #[test]
    fn test_thinking_type_supports_sonnet_4_6() {
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.6-20250214"));
    }

    #[test]
    fn test_thinking_type_unsupported_for_disabled() {
        assert!(!ThinkingType::Disabled.supports_model("claude-opus-4-6"));
        assert!(!ThinkingType::Disabled.supports_model("claude-sonnet-4-20250214"));
    }

    #[test]
    fn test_thinking_type_unsupported_for_old_models() {
        assert!(!ThinkingType::Adaptive.supports_model("claude-3-opus-20240229"));
        assert!(!ThinkingType::Adaptive.supports_model("claude-3-sonnet-20240229"));
        assert!(!ThinkingType::Enabled.supports_model("claude-3-haiku-20240307"));
    }

    #[test]
    fn test_thinking_type_case_insensitive() {
        assert!(ThinkingType::Adaptive.supports_model("CLAUDE-OPUS-4-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("Claude-Sonnet-4-20250214"));
    }

    #[test]
    fn test_thinking_display_default() {
        assert_eq!(ThinkingDisplay::default(), ThinkingDisplay::Summarized);
    }

    #[test]
    fn test_sse_event_is_thinking() {
        let thinking_event = SSEEvent::ThinkingDelta {
            thinking: "reasoning...".to_string(),
        };
        assert!(thinking_event.is_thinking());

        let text_event = SSEEvent::Text {
            text: "Hello".to_string(),
        };
        assert!(!text_event.is_thinking());
    }

    #[test]
    fn test_sse_event_as_thinking() {
        let thinking_event = SSEEvent::ThinkingDelta {
            thinking: "my reasoning".to_string(),
        };
        assert_eq!(
            thinking_event.as_thinking(),
            Some("my reasoning".to_string())
        );

        let text_event = SSEEvent::Text {
            text: "Hello".to_string(),
        };
        assert_eq!(text_event.as_thinking(), None);
    }

    #[test]
    fn test_stream_chunk_alias_uses_stream_event() {
        let chunk: StreamChunk = Ok(rustycode_protocol::stream_event::StreamEvent::Done);
        assert!(matches!(
            chunk,
            Ok(rustycode_protocol::stream_event::StreamEvent::Done)
        ));
    }

    #[test]
    fn test_provider_config_debug_redacts_api_key() {
        use secrecy::SecretString;

        let config_with_key = ProviderConfig {
            api_key: Some(SecretString::new(
                "sk-ant-api03-secret-key".to_string().into(),
            )),
            base_url: Some("https://api.example.com".to_string()),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let debug_str = format!("{:?}", config_with_key);

        // Verify API key is redacted
        assert!(debug_str.contains("***REDACTED***"));
        assert!(!debug_str.contains("sk-ant-api03"));
        assert!(!debug_str.contains("secret-key"));

        // Verify other fields are present
        assert!(debug_str.contains("https://api.example.com"));
        assert!(debug_str.contains("120"));
    }

    #[test]
    fn test_provider_config_debug_with_none_api_key() {
        let config_without_key = ProviderConfig {
            api_key: None,
            base_url: None,
            timeout_seconds: Some(30),
            extra_headers: None,
            retry_config: None,
        };

        let debug_str = format!("{:?}", config_without_key);

        // Should still work when api_key is None
        assert!(debug_str.contains("ProviderConfig"));
        assert!(debug_str.contains("30"));
    }

    #[test]
    fn test_thinking_type_supports_opus_4_7() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-7"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.7-20260401"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-7"));
    }

    #[test]
    fn test_validate_thinking_rejects_enabled_on_opus_4_7() {
        let request = CompletionRequest::new("claude-opus-4-7".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        let result = request.validate_thinking();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("adaptive"));
    }

    #[test]
    fn test_validate_thinking_allows_adaptive_on_opus_4_7() {
        let request = CompletionRequest::new("claude-opus-4-7".to_string(), vec![])
            .with_thinking_type(ThinkingType::Adaptive);
        assert!(request.validate_thinking().is_ok());
    }

    // ─── Additional coverage: supports_model edge cases ────────────────

    #[test]
    fn test_supports_model_short_form_ids() {
        // Short-form IDs like "claude-opus-4-6" and "claude-sonnet-4-6"
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-6"));
        assert!(ThinkingType::Enabled.supports_model("claude-opus-4-6"));
        assert!(ThinkingType::Enabled.supports_model("claude-sonnet-4-6"));
    }

    #[test]
    fn test_supports_model_haiku_never_supports_thinking() {
        assert!(!ThinkingType::Adaptive.supports_model("claude-haiku-4-5"));
        assert!(!ThinkingType::Enabled.supports_model("claude-3-5-haiku-20241022"));
        assert!(!ThinkingType::Adaptive.supports_model("claude-haiku-4-6"));
    }

    #[test]
    fn test_supports_model_empty_and_garbage() {
        assert!(!ThinkingType::Adaptive.supports_model(""));
        assert!(!ThinkingType::Adaptive.supports_model("not-a-model"));
        assert!(!ThinkingType::Adaptive.supports_model("gpt-4"));
        assert!(!ThinkingType::Enabled.supports_model("random-string"));
    }

    #[test]
    fn test_supports_model_date_format_variants() {
        // Opus 4.5 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250514"));
        // Opus 4.6 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4-20250214"));
        // Sonnet 4.5 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250514"));
        // Sonnet 4.6 date format
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4-20250214"));
    }

    #[test]
    fn test_supports_model_dotted_format() {
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.5-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("claude-opus-4.6-20250214"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.5-20250514"));
        assert!(ThinkingType::Adaptive.supports_model("claude-sonnet-4.6-20250214"));
    }

    #[test]
    fn test_validate_thinking_enabled_ok_on_older_models() {
        // Opus 4.5 and 4.6 should accept Enabled
        let req = CompletionRequest::new("claude-opus-4.5-20250514".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        assert!(req.validate_thinking().is_ok());

        let req = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![])
            .with_thinking_type(ThinkingType::Enabled);
        assert!(req.validate_thinking().is_ok());
    }

    #[test]
    fn test_validate_thinking_disabled_always_ok() {
        for model in &[
            "claude-opus-4-6",
            "claude-opus-4-7",
            "claude-sonnet-4-6",
            "claude-3-opus",
        ] {
            let req = CompletionRequest::new(model.to_string(), vec![])
                .with_thinking_type(ThinkingType::Disabled);
            assert!(
                req.validate_thinking().is_ok(),
                "Disabled should be ok for {}",
                model
            );
        }
    }

    #[test]
    fn test_validate_thinking_no_config_is_ok() {
        // No thinking config at all should be fine
        let req = CompletionRequest::new("claude-opus-4-7".to_string(), vec![]);
        assert!(req.validate_thinking().is_ok());
    }

    // --- validate_extra_headers tests ---

    #[test]
    fn test_validate_extra_headers_blocks_authorization() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("security header"));
    }

    #[test]
    fn test_validate_extra_headers_blocks_host() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Host".to_string(), "evil.com".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extra_headers_blocks_crlf_injection() {
        let mut headers = std::collections::HashMap::new();
        headers.insert(
            "X-Custom".to_string(),
            "value\r\nInjected: true".to_string(),
        );
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("newline"));
    }

    #[test]
    fn test_validate_extra_headers_allows_valid() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Request-ID".to_string(), "abc123".to_string());
        headers.insert("X-Custom-Header".to_string(), "value".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated.len(), 2);
    }

    #[test]
    fn test_validate_extra_headers_none_is_ok() {
        let result = validate_extra_headers(&None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_extra_headers_empty_map() {
        let headers = std::collections::HashMap::new();
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_validate_extra_headers_blocks_invalid_name() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Invalid Name\n".to_string(), "value".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_extra_headers_case_insensitive_security() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("authorization".to_string(), "Bearer token".to_string());
        let result = validate_extra_headers(&Some(headers));
        assert!(result.is_err());
    }

    #[test]
    fn test_with_model_preserves_retry_delay() {
        let err = ProviderError::RateLimited {
            retry_delay: Some(std::time::Duration::from_secs(30)),
        };
        let tagged = err.with_model("claude-opus-4-7");
        assert!(tagged.is_rate_limited());
        assert_eq!(
            tagged.retry_delay(),
            Some(std::time::Duration::from_secs(30))
        );
    }

    #[test]
    fn test_with_model_preserves_top_up_url() {
        let err = ProviderError::CreditsExhausted {
            details: "out of credits".to_string(),
            top_up_url: Some("https://console.example.com/billing".to_string()),
        };
        let tagged = err.with_model("gpt-4o");
        assert!(tagged.is_credits_exhausted());
        assert_eq!(
            tagged.top_up_url(),
            Some("https://console.example.com/billing")
        );
        assert!(tagged.to_string().contains("[model: gpt-4o]"));
    }

    #[test]
    fn test_with_model_prefixes_string_variants() {
        let cases: Vec<(ProviderError, &str)> = vec![
            (ProviderError::Auth("bad key".into()), "bad key"),
            (ProviderError::Network("timeout".into()), "timeout"),
            (ProviderError::Api("overloaded".into()), "overloaded"),
            (ProviderError::Timeout("30s".into()), "30s"),
            (ProviderError::InvalidModel("foo".into()), "foo"),
        ];
        for (err, original_msg) in cases {
            let tagged = err.with_model("test-model");
            let display = tagged.to_string();
            assert!(
                display.contains("[model: test-model]"),
                "missing model prefix in: {display}"
            );
            assert!(
                display.contains(original_msg),
                "missing original message in: {display}"
            );
        }
    }

    #[test]
    fn test_usage_saturating_add_no_overflow() {
        let usage = Usage::new(u32::MAX, u32::MAX);
        assert_eq!(usage.total_tokens, u32::MAX);
    }

    #[test]
    fn test_usage_with_cache_saturating() {
        let usage = Usage::with_cache(u32::MAX, 100, 50, 25);
        // total = cache_read + cache_creation + input = MAX + 50 + 25 → saturates
        assert_eq!(usage.total_tokens, u32::MAX);
    }

    #[test]
    fn test_sanitize_error_message_redacts_bearer() {
        let msg = "Request failed: Authorization: Bearer sk-ant-api03-secret123";
        let sanitized = sanitize_error_message(msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-ant-api03-secret123"));
    }

    #[test]
    fn test_sanitize_error_message_redacts_query_token() {
        let msg = "GET /api?token=abc123&key=secret failed";
        let sanitized = sanitize_error_message(msg);
        assert!(!sanitized.contains("abc123"));
        assert!(!sanitized.contains("secret"));
        assert!(sanitized.contains("token=[REDACTED]"));
        assert!(sanitized.contains("key=[REDACTED]"));
    }

    #[test]
    fn test_sanitize_error_message_redacts_x_api_key() {
        let msg = "x-api-key: sk-proj-abc123def456";
        let sanitized = sanitize_error_message(msg);
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("sk-proj-abc123def456"));
    }

    #[test]
    fn test_sanitize_error_message_preserves_normal_text() {
        let msg = "Request to https://api.example.com/v1/models failed with timeout";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, msg);
    }

    #[test]
    fn test_sanitize_error_message_empty_input() {
        assert_eq!(sanitize_error_message(""), "");
    }

    // --- SkillRef / with_skills tests ---

    #[test]
    fn test_skill_ref_serialization() {
        let skill = SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "pptx".into(),
            version: "latest".into(),
        };
        let json = serde_json::to_value(&skill).unwrap();
        assert_eq!(json["type"], "anthropic");
        assert_eq!(json["skill_id"], "pptx");
        assert_eq!(json["version"], "latest");
    }

    #[test]
    fn test_skill_ref_roundtrip() {
        let skill = SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "xlsx".into(),
            version: "1.0".into(),
        };
        let serialized = serde_json::to_string(&skill).unwrap();
        let deserialized: SkillRef = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.skill_type, "anthropic");
        assert_eq!(deserialized.skill_id, "xlsx");
        assert_eq!(deserialized.version, "1.0");
    }

    #[test]
    fn test_with_skills_produces_container_json() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "latest".into(),
                },
            ]);

        let container = request.container.unwrap();
        let skills = container["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0]["type"], "anthropic");
        assert_eq!(skills[0]["skill_id"], "pptx");
        assert_eq!(skills[0]["version"], "latest");
    }

    #[test]
    fn test_with_skills_multiple_skills() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "1.0".into(),
                },
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "xlsx".into(),
                    version: "2.0".into(),
                },
            ]);

        let container = request.container.unwrap();
        let skills = container["skills"].as_array().unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0]["skill_id"], "pptx");
        assert_eq!(skills[1]["skill_id"], "xlsx");
    }

    #[test]
    fn test_container_none_by_default() {
        let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]);
        assert!(request.container.is_none());
    }

    #[test]
    fn test_container_omitted_from_serialization_when_none() {
        let request = CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("container").is_none());
    }

    #[test]
    fn test_container_present_in_serialization_when_set() {
        let request =
            CompletionRequest::new("claude-sonnet-4-6".to_string(), vec![]).with_skills(vec![
                SkillRef {
                    skill_type: "anthropic".into(),
                    skill_id: "pptx".into(),
                    version: "latest".into(),
                },
            ]);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("container").is_some());
        assert!(json["container"]["skills"].is_array());
    }
}
