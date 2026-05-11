//! Cohere LLM provider implementation.
//!
//! This provider supports Cohere's v2 Chat API which provides access to
//! models like Command R, Command R+, and Command A with tool-use support.
//!
//! ## Configuration
//!
//! The provider requires:
//! - API key (from Cohere dashboard)
//! - Model name (e.g., "command-r-plus-08-2024", "command-a-03-2025")
//!
//! ## Environment Variables
//!
//! - `COHERE_API_KEY` - API key for authentication
//!
//! ## Tool Calling
//!
//! Cohere's v2 Chat API supports function calling with parallel tool use.
//! Tools are sent as `{ type: "function", function: { name, description, parameters } }`.
//! Responses include `tool_calls` with `id`, `type`, and `function` fields.
//! Tool results are sent as `{ role: "tool", tool_call_id, content }` messages.

use crate::auth::AuthMethod;
use crate::provider::{
    validate_extra_headers, CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig,
    ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::transport::HttpTransport;
use crate::wire::cohere::CohereProtocol;
use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;

/// Default Cohere API endpoint (v2 Chat API)
const COHERE_API_ENDPOINT: &str = "https://api.cohere.ai/v2/chat";

/// Cohere LLM provider
pub struct CohereProvider {
    config: ProviderConfig,
    route: Route,
}

impl CohereProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| COHERE_API_ENDPOINT.to_string());

        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(
            config
                .api_key
                .clone()
                .ok_or_else(|| ProviderError::auth("Missing API key"))?,
        ));

        let timeout = config.timeout_seconds.unwrap_or(180);

        let validated_headers = validate_extra_headers(&config.extra_headers)?;
        let extra_header_pairs: Vec<(String, String)> = validated_headers
            .into_iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
            .collect();

        let route = Route::new(
            endpoint,
            Box::new(CohereProtocol),
            Box::new(
                HttpTransport::new(timeout)
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_name("cohere-chat")
        .with_extra_headers(extra_header_pairs);

        Ok(Self { config, route })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "cohere".to_string(),
            display_name: "Cohere".to_string(),
            description: "Enterprise AI platform with Command R, Command R+, and Command A models"
                .to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Cohere API key from dashboard.cohere.com".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("your-api-key".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (optional)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some(COHERE_API_ENDPOINT.to_string()),
                    default: Some(COHERE_API_ENDPOINT.to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "COHERE_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by Cohere. {context}"
                    .to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Cohere models perform well with clear, direct instructions.".to_string(),
                        "Use conversational language when appropriate.".to_string(),
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
                    model_id: "command-a-03-2025".to_string(),
                    display_name: "Command A".to_string(),
                    description:
                        "Cohere's latest flagship model with strong tool use and reasoning"
                            .to_string(),
                    context_window: 256000,
                    supports_tools: true,
                    use_cases: vec![
                        "Tool calling".to_string(),
                        "Complex reasoning".to_string(),
                        "RAG applications".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "command-r-plus-08-2024".to_string(),
                    display_name: "Command R+".to_string(),
                    description:
                        "Cohere's flagship model with 128k context and strong RAG capabilities"
                            .to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "Document analysis".to_string(),
                        "RAG applications".to_string(),
                        "Tool calling".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "command-r-08-2024".to_string(),
                    display_name: "Command R".to_string(),
                    description: "Balanced model with good performance and lower cost".to_string(),
                    context_window: 128000,
                    supports_tools: true,
                    use_cases: vec![
                        "Chat applications".to_string(),
                        "Tool calling".to_string(),
                        "Content generation".to_string(),
                    ],
                    cost_tier: 3,
                },
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }

    /// Return the configured endpoint URL.
    #[cfg(test)]
    fn endpoint(&self) -> String {
        self.route.endpoint.clone()
    }
}

#[async_trait]
impl LLMProvider for CohereProvider {
    fn name(&self) -> &'static str {
        "cohere"
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "command-a-03-2025".to_string(),
            "command-r-plus-08-2024".to_string(),
            "command-r-08-2024".to_string(),
            "command-r7b-12-2024".to_string(),
            "command".to_string(),
            "command-light".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
        self.route
            .execute(&request, None)
            .await
            .map_err(|e| ProviderError::api(e.to_string()))
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let stream = self
            .route
            .execute_stream(&request, None)
            .await
            .map_err(|e| ProviderError::api(e.to_string()))?;

        let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::api(e.to_string())));

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
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_provider_name() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.name(), "cohere");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = CohereProvider::new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_metadata_display_name() {
        let metadata = CohereProvider::metadata();
        assert_eq!(metadata.display_name, "Cohere");
        assert_eq!(metadata.provider_id, "cohere");
    }

    #[test]
    fn test_metadata_tool_calling_supported() {
        let metadata = CohereProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_metadata_env_mappings() {
        let metadata = CohereProvider::metadata();
        assert_eq!(
            metadata.config_schema.env_mappings.get("api_key"),
            Some(&"COHERE_API_KEY".to_string())
        );
    }

    #[test]
    fn test_metadata_recommended_models() {
        let metadata = CohereProvider::metadata();
        let model_ids: Vec<&str> = metadata
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();
        assert!(model_ids.iter().any(|id| id.contains("command")));
        assert!(model_ids.iter().any(|id| id.contains("command-a")));
    }

    #[test]
    fn test_all_recommended_models_support_tools() {
        let metadata = CohereProvider::metadata();
        for model in &metadata.recommended_models {
            assert!(
                model.supports_tools,
                "Model {} should support tools",
                model.model_id
            );
        }
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(provider.endpoint(), COHERE_API_ENDPOINT);
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://custom-cohere.example.com/v2/chat".to_string());
        let provider = CohereProvider::new(config).unwrap();
        assert_eq!(
            provider.endpoint(),
            "https://custom-cohere.example.com/v2/chat"
        );
    }

    #[test]
    fn test_cohere_v2_request_serialization() {
        use crate::provider::ChatMessage;
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let request = CompletionRequest::new("command-r", vec![ChatMessage::user("What is Rust?")])
            .with_max_tokens(512)
            .with_temperature(0.3)
            .with_system_prompt("You are a coding assistant".to_string());

        let body = protocol.serialize_body(&request, None).unwrap();
        let json_str = serde_json::to_string(&body).unwrap();
        assert!(json_str.contains("\"model\""));
        assert!(json_str.contains("\"command-r\""));
        assert!(json_str.contains("\"preamble\""));
        assert!(json_str.contains("You are a coding assistant"));
    }

    #[test]
    fn test_cohere_v2_response_deserialization_text() {
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let json = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Rust is a systems programming language."}]
            },
            "finish_reason": "COMPLETE",
            "meta": {
                "usage": {
                    "tokens": {"input_tokens": 10, "output_tokens": 20}
                },
                "api_version": {"version": "2.0"}
            }
        });

        let response = protocol.parse_response(&json).unwrap();
        assert!(response
            .content
            .contains("Rust is a systems programming language."));
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 20);
    }

    #[test]
    fn test_cohere_v2_response_deserialization_with_tool_calls() {
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let json = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": [],
                "tool_plan": "I need to search for information.",
                "tool_calls": [
                    {
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "Search",
                            "arguments": "{\"query\": \"rust programming\"}"
                        }
                    },
                    {
                        "id": "call_def456",
                        "type": "function",
                        "function": {
                            "name": "Read",
                            "arguments": "{\"path\": \"/tmp/test.rs\"}"
                        }
                    }
                ]
            },
            "finish_reason": "TOOL_CALL"
        });

        let response = protocol.parse_response(&json).unwrap();
        assert!(response.content.contains("Search"));
        assert!(response.content.contains("Read"));
        assert!(response.content.contains("call_abc123"));
        assert!(response.content.contains("call_def456"));
        assert_eq!(response.stop_reason, Some("tool_use".to_string()));
    }

    #[test]
    fn test_extract_response_with_text() {
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let json = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello!"}]
            },
            "finish_reason": "COMPLETE"
        });

        let response = protocol.parse_response(&json).unwrap();
        assert_eq!(response.content, "Hello!");
    }

    #[test]
    fn test_extract_response_with_tool_calls() {
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let json = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": [],
                "tool_plan": "I need to search.",
                "tool_calls": [
                    {
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "Search",
                            "arguments": "{\"query\":\"rust\"}"
                        }
                    }
                ]
            },
            "finish_reason": "TOOL_CALL"
        });

        let response = protocol.parse_response(&json).unwrap();
        assert!(response.content.contains("I need to search."));
        assert!(response.content.contains("Search"));
        assert!(response.content.contains("call_123"));
    }

    #[test]
    fn test_convert_messages_simple() {
        use crate::provider::ChatMessage;
        use crate::wire::Protocol;

        let protocol = CohereProtocol;
        let request = CompletionRequest::new(
            "test",
            vec![
                ChatMessage::user("Hello"),
                ChatMessage::assistant("Hi there!"),
            ],
        );

        let body = protocol.serialize_body(&request, None).unwrap();
        let messages = body.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[1]["role"], "assistant");
    }

    #[test]
    fn test_convert_messages_with_tool_result() {
        use crate::provider::{ChatMessage, MessageRole};
        use crate::wire::Protocol;
        use rustycode_protocol::message::{ContentBlock, MessageContent};

        let protocol = CohereProtocol;
        let request = CompletionRequest::new(
            "test",
            vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: MessageContent::Blocks(vec![ContentBlock::Text {
                        text: "What is 2+2?".to_string(),
                        cache_control: None,
                    }]),
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                        id: "call_1".to_string(),
                        name: "calculator".to_string(),
                        input: serde_json::json!({"expr": "2+2"}),
                    }]),
                },
                ChatMessage {
                    role: MessageRole::Tool("call_1".to_string()),
                    content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                        tool_use_id: "call_1".to_string(),
                        content: "4".to_string(),
                        is_error: false,
                    }]),
                },
            ],
        );

        let body = protocol.serialize_body(&request, None).unwrap();
        let messages = body.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 3);
        // Assistant message should have tool_calls
        assert!(messages[1]["tool_calls"].is_array());
        let calls = messages[1]["tool_calls"].as_array().unwrap();
        assert_eq!(calls[0]["function"]["name"], "calculator");
        // Tool result message should have tool_call_id
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[2]["role"], "tool");
    }

    #[test]
    fn test_convert_tools() {
        use crate::schema::tool_schema::{JsonSchema, ToolSchema};
        use crate::wire::Protocol;

        let mut properties = std::collections::BTreeMap::new();
        properties.insert("expr".to_string(), JsonSchema::string("expression"));

        let tools = vec![ToolSchema::new(
            "calculator",
            "Evaluate math expressions",
            JsonSchema::object(properties, vec!["expr".to_string()]),
        )];

        let converted = CohereProtocol.serialize_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0]["type"], "function");
        assert_eq!(converted[0]["function"]["name"], "calculator");
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.contains("command")));
        assert!(models.iter().any(|m| m.contains("command-a")));
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = CohereProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }

    #[test]
    fn test_get_api_key_from_config() {
        let config = make_config(Some("my-cohere-key"));
        let provider = CohereProvider::new(config).unwrap();
        let cfg = provider.config().unwrap();
        assert!(cfg.api_key.is_some());
        let key = cfg.api_key.as_ref().unwrap();
        assert_eq!(key.expose_secret(), "my-cohere-key");
    }
}
