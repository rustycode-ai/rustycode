//! AWS Bedrock LLM provider implementation.
//!
//! This provider supports AWS Bedrock which offers access to foundation models
//! from Anthropic, AI21, Meta, Mistral, and more through a single API.
//!
//! ## Architecture
//!
//! Uses the Route composition pattern:
//! - **chat_route**: `/model/{model-id}/converse` via `HttpTransport`
//! - **stream_route**: `/model/{model-id}/converse-stream` via `HttpSseTransport`
//! - **BedrockProtocol**: handles Converse API serialization
//! - **Auth**: AWS Sigv4 (default) or x-api-key header (when API key provided)
//!
//! ## Configuration
//!
//! The provider can be configured with:
//! - Direct AWS credentials (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_REGION)
//! - API key via the `api_key` field (for simpler setups / proxies)
//! - Custom endpoint for AWS Bedrock proxies
//!
//! ## Supported Models
//!
//! - Anthropic Claude (claude-4-opus, claude-4-sonnet, claude-4-haiku)
//! - Meta Llama (llama3-8b, llama3-70b, llama4)
//! - Mistral AI (mistral-large, mistral-small)
//! - AI21 Jurassic (jamba-1-5-large)

use crate::auth::{ApiKeyHeaderAuth, AuthMethod, AwsSigv4Auth};
use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{ConfigField, ConfigFieldType, ConfigSchema, ProviderMetadata};
use crate::route::Route;
use crate::transport::HttpTransport;
use crate::wire::bedrock::BedrockProtocol;

use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;
use std::pin::Pin;

/// Default timeout in seconds for Bedrock requests.
const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// Default AWS region.
const DEFAULT_REGION: &str = "us-east-1";

/// AWS Bedrock LLM provider.
///
/// Composes two routes from `BedrockProtocol` + transport + auth:
/// - **chat_route**: non-streaming Converse endpoint
/// - **stream_route**: streaming Converse endpoint
pub struct BedrockProvider {
    config: ProviderConfig,
    chat_route: Route,
    stream_route: Route,
    region: String,
}

impl BedrockProvider {
    /// Create a new Bedrock provider with config validation.
    pub fn new(config: ProviderConfig, model: String) -> Result<Self> {
        Self::metadata().validate_config(&config)?;
        Self::build(config, model)
    }

    /// Create provider without config validation (for custom endpoints/proxies).
    pub fn new_without_validation(config: ProviderConfig, model: String) -> Result<Self> {
        Self::build(config, model)
    }

    fn build(config: ProviderConfig, _model: String) -> Result<Self> {
        // Resolve region from environment or default
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| DEFAULT_REGION.to_string());

        let timeout = config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS);
        let base = config
            .base_url
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/');
        let base = if base.is_empty() {
            format!("https://bedrock-runtime.{}.amazonaws.com", region)
        } else {
            base.to_string()
        };

        // Auth: x-api-key header if API key provided, AWS Sigv4 otherwise
        let auth: Box<dyn AuthMethod> = if let Some(key) = config.api_key.clone() {
            Box::new(ApiKeyHeaderAuth::new("x-api-key", key))
        } else {
            let access_key = std::env::var("AWS_ACCESS_KEY_ID")
                .map(|s| SecretString::new(s.into_boxed_str()))
                .map_err(|_| {
                    anyhow::anyhow!(
                        "AWS credentials required. Set AWS_ACCESS_KEY_ID and \
                         AWS_SECRET_ACCESS_KEY env vars, or provide api_key in config"
                    )
                })?;
            let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
                .map(|s| SecretString::new(s.into_boxed_str()))
                .map_err(|_| {
                    anyhow::anyhow!(
                        "AWS credentials required. Set AWS_ACCESS_KEY_ID and \
                         AWS_SECRET_ACCESS_KEY env vars, or provide api_key in config"
                    )
                })?;
            Box::new(AwsSigv4Auth::new(
                access_key,
                secret_key,
                region.clone(),
                "bedrock-runtime".to_string(),
            ))
        };

        // Chat route: non-streaming converse
        let chat_endpoint = format!("{base}/model/{{model}}/converse");
        let chat_route = Route::new(
            chat_endpoint,
            Box::new(BedrockProtocol),
            Box::new(HttpTransport::new(timeout)?),
            auth.clone_box(),
        )
        .with_name("bedrock-converse");

        // Stream route: streaming converse-stream
        let stream_endpoint = format!("{base}/model/{{model}}/converse-stream");
        let stream_route = Route::new(
            stream_endpoint,
            Box::new(BedrockProtocol),
            Box::new(HttpTransport::new(timeout)?),
            auth,
        )
        .with_name("bedrock-stream");

        Ok(Self {
            config,
            chat_route,
            stream_route,
            region,
        })
    }

    /// Get metadata for this provider.
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "bedrock".to_string(),
            display_name: "AWS Bedrock".to_string(),
            description: "Foundation models from Anthropic, Meta, Mistral, and more through AWS"
                .to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![],
                optional_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "AWS access key ID or custom endpoint API key".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Custom Endpoint".to_string(),
                        description: "Custom Bedrock endpoint (for proxies or custom deployments)"
                            .to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some(
                            "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
                        ),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "AWS_ACCESS_KEY_ID".to_string());
                    map
                },
            },
            prompt_template: crate::provider_metadata::PromptTemplate {
                base_template: "You are an AI assistant hosted on AWS Bedrock.\n\n{context}"
                    .to_string(),
                optimizations: crate::provider_metadata::PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Follow AWS best practices.".to_string(),
                        "Provide secure, enterprise-grade responses.".to_string(),
                    ],
                },
                tool_format: crate::provider_metadata::ToolFormat::OpenAIFunctionCalling,
            },
            tool_calling: crate::provider_metadata::ToolCallingMetadata {
                supported: true,
                max_tools_per_call: None,
                parallel_calling: true,
                streaming_support: true,
            },
            recommended_models: vec![crate::provider_metadata::ModelInfo {
                model_id: "anthropic.claude-3-5-sonnet-20240620-v1:0".to_string(),
                display_name: "Claude 3.5 Sonnet".to_string(),
                description: "Balanced performance and speed".to_string(),
                context_window: 200_000,
                supports_tools: true,
                use_cases: vec!["General assistance".to_string(), "Coding".to_string()],
                cost_tier: 3,
            }],
            model_behavior_profiles: HashMap::new(),
        }
    }

    /// Build the chat endpoint URL for a given model.
    pub fn endpoint(&self, model: &str) -> String {
        let base = self.config.base_url.as_deref().unwrap_or("");
        let base = if base.is_empty() {
            format!("https://bedrock-runtime.{}.amazonaws.com", self.region)
        } else {
            base.trim_end_matches('/').to_string()
        };
        format!("{base}/model/{model}/converse")
    }

    /// Get the AWS region for this provider.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Resolve the model-specific chat endpoint by replacing `{model}` placeholder.
    fn resolve_chat_endpoint(&self, model: &str) -> String {
        self.chat_route.endpoint.replace("{model}", model)
    }

    /// Resolve the model-specific stream endpoint by replacing `{model}` placeholder.
    fn resolve_stream_endpoint(&self, model: &str) -> String {
        self.stream_route.endpoint.replace("{model}", model)
    }
}

#[async_trait]
impl LLMProvider for BedrockProvider {
    fn name(&self) -> &'static str {
        "bedrock"
    }

    async fn is_available(&self) -> bool {
        let has_credentials = std::env::var("AWS_ACCESS_KEY_ID").is_ok()
            && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok();

        let has_api_key = self
            .config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty());

        has_credentials || has_api_key
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            // Claude 4.x series (latest)
            "anthropic.claude-opus-v4:0".to_string(),
            "anthropic.claude-sonnet-v4:0".to_string(),
            "anthropic.claude-haiku-v4:0".to_string(),
            // Claude 3.7 (latest Claude 3)
            "anthropic.claude-3-7-sonnet-20250219-v1:0".to_string(),
            // Claude 3.5 (stable)
            "anthropic.claude-3-5-sonnet-20241022-v2:0".to_string(),
            "anthropic.claude-3-5-haiku-20241022-v1:0".to_string(),
            // Claude 3 Opus
            "anthropic.claude-3-opus-20240229-v1:0".to_string(),
            // Llama 4.x
            "meta.llama4-8b-instruct-v1:0".to_string(),
            "meta.llama4-405b-instruct-v1:0".to_string(),
            // Llama 3.x
            "meta.llama3-3-70b-instruct-v1:0".to_string(),
            "meta.llama3-1-405b-instruct-v1:0".to_string(),
            "meta.llama3-8b-instruct-v1:0".to_string(),
            "meta.llama3-70b-instruct-v1:0".to_string(),
            // Mistral
            "mistral.mistral-large-2407-v1:0".to_string(),
            "mistral.mistral-small-2402-v1:0".to_string(),
            // AI21
            "ai21.jamba-1-5-large-v1:0".to_string(),
        ])
    }

    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ProviderError> {
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
            if msg.contains("HTTP error 401") || msg.contains("HTTP error 403") {
                ProviderError::Auth(format!(
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {msg}"
                ))
            } else if msg.contains("HTTP error 404") {
                ProviderError::InvalidModel(request.model.clone())
            } else if msg.contains("HTTP error 429") {
                ProviderError::RateLimited { retry_delay: None }
            } else if msg.contains("HTTP error 502")
                || msg.contains("HTTP error 503")
                || msg.contains("HTTP error 504")
            {
                ProviderError::Network(format!("Bedrock service temporarily unavailable. {msg}"))
            } else {
                ProviderError::Api(msg)
            }
        })?;

        Ok(CompletionResponse {
            model: request.model,
            ..response
        })
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, ProviderError> {
        let resolved_endpoint = self.resolve_stream_endpoint(&request.model);
        let temp_route = Route::new(
            resolved_endpoint,
            self.stream_route.protocol.clone_box(),
            Box::new(
                HttpTransport::new(self.config.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS))
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
                    "Bedrock authentication failed. Check AWS credentials (aws configure). {msg}"
                ))
                } else if msg.contains("HTTP error 404") {
                    ProviderError::InvalidModel(format!(
                        "model not found: {}. Check model ID and region in AWS Bedrock console",
                        request.model
                    ))
                } else if msg.contains("HTTP error 429") {
                    ProviderError::RateLimited { retry_delay: None }
                } else {
                    ProviderError::Api(msg)
                }
            })?;

        // Map Result<StreamEvent> to StreamChunk
        let chunk_stream = stream.map(|res| res.map_err(|e| ProviderError::Network(e.to_string())));

        Ok(Box::pin(chunk_stream))
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ChatMessage, MessageRole};
    use crate::types::request::ToolChoice;
    use crate::wire::bedrock::BedrockProtocol;
    use crate::wire::Protocol;
    use rustycode_protocol::{ContentBlock, MessageContent};
    use serde_json::json;

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
        let provider =
            BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        assert_eq!(<BedrockProvider as LLMProvider>::name(&provider), "bedrock");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_creates_without_api_key() {
        // Without API key, it tries AWS env vars. Those are likely missing in test env,
        // so this should fail with a clear error.
        let config = make_config(None);
        let provider = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string());
        // In test env without AWS creds, this should fail
        assert!(provider.is_err());
    }

    #[test]
    fn test_metadata_tool_calling_enabled() {
        let metadata = BedrockProvider::metadata();
        assert!(metadata.tool_calling.supported);
        assert!(metadata.tool_calling.parallel_calling);
    }

    #[test]
    fn test_convert_messages_simple() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let protocol = BedrockProtocol;
        // Serialize a request with these messages to verify conversion
        let request = CompletionRequest::new("test-model", msgs);
        let body = protocol.serialize_body(&request, None).unwrap();
        let messages = body.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"][0]["text"], "hello");
    }

    #[test]
    fn test_convert_messages_tool_result() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::tool_result("t1", "result text")]),
        }];
        let protocol = BedrockProtocol;
        let request = CompletionRequest::new("test-model", msgs);
        let body = protocol.serialize_body(&request, None).unwrap();
        let messages = body.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        let tr = &messages[0]["content"][0]["toolResult"];
        assert_eq!(tr["toolUseId"], "t1");
        assert_eq!(tr["status"], "success");
    }

    #[test]
    fn test_convert_messages_tool_use() {
        let msgs = vec![ChatMessage {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t2".to_string(),
                name: "Bash".to_string(),
                input: json!({"command": "ls"}),
            }]),
        }];
        let protocol = BedrockProtocol;
        let request = CompletionRequest::new("test-model", msgs);
        let body = protocol.serialize_body(&request, None).unwrap();
        let messages = body.get("messages").unwrap().as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "assistant");
        let tu = &messages[0]["content"][0]["toolUse"];
        assert_eq!(tu["name"], "Bash");
        assert_eq!(tu["toolUseId"], "t2");
    }

    #[test]
    fn test_convert_tools_via_protocol() {
        let tools = vec![json!({
            "name": "Read", "description": "Read file",
            "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
        })];
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let protocol = BedrockProtocol;
        let request = CompletionRequest::new("test-model", msgs).with_tools(tools);
        let body = protocol.serialize_body(&request, None).unwrap();
        let tool_config = body.get("toolConfig").unwrap();
        let tools_arr = tool_config.get("tools").unwrap().as_array().unwrap();
        assert_eq!(tools_arr.len(), 1);
        assert_eq!(tools_arr[0]["toolSpec"]["name"], "Read");
    }

    #[test]
    fn test_convert_tool_choice_via_protocol() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let protocol = BedrockProtocol;

        // auto
        let req = CompletionRequest::new("test-model", msgs.clone())
            .with_tool_choice(ToolChoice::Auto)
            .with_tools(vec![json!({"name": "X"})]);
        let body = protocol.serialize_body(&req, None).unwrap();
        assert_eq!(body["toolConfig"]["toolChoice"], json!({"auto": {}}));

        // required
        let req = CompletionRequest::new("test-model", msgs.clone())
            .with_tool_choice(ToolChoice::Required)
            .with_tools(vec![json!({"name": "X"})]);
        let body = protocol.serialize_body(&req, None).unwrap();
        assert_eq!(body["toolConfig"]["toolChoice"], json!({"any": {}}));

        // named tool
        let req = CompletionRequest::new("test-model", msgs.clone())
            .with_tool_choice(ToolChoice::Named("Bash".to_string()))
            .with_tools(vec![json!({"name": "Bash"})]);
        let body = protocol.serialize_body(&req, None).unwrap();
        assert_eq!(
            body["toolConfig"]["toolChoice"],
            json!({"tool": {"name": "Bash"}})
        );

        // none — toolConfig must be absent
        let req = CompletionRequest::new("test-model", msgs)
            .with_tool_choice(ToolChoice::None)
            .with_tools(vec![json!({"name": "X"})]);
        let body = protocol.serialize_body(&req, None).unwrap();
        assert!(body.get("toolConfig").is_none());
    }

    #[test]
    fn test_bedrock_request_serialization() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let request = CompletionRequest::new("test-model", msgs)
            .with_system_prompt("You are helpful.".to_string())
            .with_max_tokens(1024)
            .with_temperature(0.5);
        let protocol = BedrockProtocol;
        let body = protocol.serialize_body(&request, None).unwrap();
        let body_str = serde_json::to_string(&body).unwrap();
        assert!(body_str.contains("inferenceConfig"));
        assert!(body_str.contains("maxTokens"));
        assert!(!body_str.contains("toolConfig"));
    }

    #[test]
    fn test_bedrock_request_with_tools() {
        let msgs = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::simple("hello"),
        }];
        let tools = vec![json!({
            "name": "Bash",
            "description": "Run command",
            "input_schema": {"type": "object"}
        })];
        let request = CompletionRequest::new("test-model", msgs)
            .with_tools(tools)
            .with_tool_choice(ToolChoice::Auto);
        let protocol = BedrockProtocol;
        let body = protocol.serialize_body(&request, None).unwrap();
        let body_str = serde_json::to_string(&body).unwrap();
        assert!(body_str.contains("toolConfig"));
        assert!(body_str.contains("toolSpec"));
        assert!(body_str.contains("Bash"));
    }

    #[test]
    fn test_chat_route_has_model_placeholder() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        assert!(p.chat_route.endpoint.contains("{model}"));
        assert!(p.chat_route.endpoint.contains("converse"));
    }

    #[test]
    fn test_stream_route_has_model_placeholder() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        assert!(p.stream_route.endpoint.contains("{model}"));
        assert!(p.stream_route.endpoint.contains("converse-stream"));
    }

    #[test]
    fn test_resolve_chat_endpoint() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        let resolved = p.resolve_chat_endpoint("anthropic.claude-3-5-sonnet-v1:0");
        assert!(resolved.contains("anthropic.claude-3-5-sonnet-v1:0"));
        assert!(!resolved.contains("{model}"));
        assert!(resolved.contains("/converse"));
    }

    #[test]
    fn test_resolve_stream_endpoint() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        let resolved = p.resolve_stream_endpoint("meta.llama3-70b-instruct-v1:0");
        assert!(resolved.contains("meta.llama3-70b-instruct-v1:0"));
        assert!(!resolved.contains("{model}"));
        assert!(resolved.contains("/converse-stream"));
    }

    #[test]
    fn test_route_names() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        assert_eq!(p.chat_route.name(), "bedrock-converse");
        assert_eq!(p.stream_route.name(), "bedrock-stream");
    }

    #[test]
    fn test_default_endpoint() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        let endpoint = p.endpoint("anthropic.claude-3-5-sonnet-v1:0");
        assert!(endpoint.starts_with("https://bedrock-runtime."));
        assert!(endpoint.contains(".amazonaws.com"));
        assert!(endpoint.contains("anthropic.claude-3-5-sonnet-v1:0"));
        assert!(endpoint.contains("/converse"));
    }

    #[test]
    fn test_custom_endpoint() {
        let mut config = make_config(Some("test-key"));
        config.base_url = Some("https://my-bedrock-proxy.example.com".to_string());
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        let endpoint = p.endpoint("test-model");
        assert!(endpoint.starts_with("https://my-bedrock-proxy.example.com"));
        assert!(endpoint.contains("test-model"));
    }

    #[test]
    fn test_new_without_validation() {
        let config = make_config(Some("test-key"));
        let provider = BedrockProvider::new_without_validation(
            config,
            "anthropic.claude-3-sonnet".to_string(),
        )
        .unwrap();
        assert_eq!(provider.name(), "bedrock");
    }

    #[tokio::test]
    async fn test_list_models_returns_known_models() {
        let config = make_config(Some("test-key"));
        let p = BedrockProvider::new(config, "anthropic.claude-3-sonnet".to_string()).unwrap();
        let models = p.list_models().await.unwrap();
        assert!(models
            .iter()
            .any(|m| m == "anthropic.claude-3-5-sonnet-20241022-v2:0"));
        assert!(models.iter().any(|m| m == "meta.llama3-70b-instruct-v1:0"));
    }
}
