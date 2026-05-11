//! Together AI LLM provider implementation.
//!
//! This provider supports Together AI's API which provides access to
//! many open-source models like Llama, Mixtral, and more.

use anyhow::Result;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::pin::Pin;

use crate::auth::AuthMethod;
use crate::provider::{
    CompletionRequest, CompletionResponse, LLMProvider, ProviderConfig, ProviderError, StreamChunk,
};
use crate::provider_metadata::{
    ConfigField, ConfigFieldType, ConfigSchema, ModelInfo, PromptLength, PromptOptimizations,
    PromptTemplate, ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::transport::HttpTransport;
use crate::wire::openai_chat::OpenAIChatProtocol;

/// Default Together AI API endpoint
const TOGETHER_API_ENDPOINT: &str = "https://api.together.xyz/v1/chat/completions";

/// Together AI LLM provider
pub struct TogetherProvider {
    config: ProviderConfig,
    route: Route,
}

impl TogetherProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| TOGETHER_API_ENDPOINT.to_string());

        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(
            config
                .api_key
                .clone()
                .ok_or_else(|| ProviderError::auth("Missing API key"))?,
        ));

        let route = Route::new(
            endpoint,
            Box::new(OpenAIChatProtocol),
            Box::new(
                HttpTransport::new(config.timeout_seconds.unwrap_or(120))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_name("together-chat");

        Ok(Self { config, route })
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
}

#[async_trait]
impl LLMProvider for TogetherProvider {
    fn name(&self) -> &'static str {
        "together"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["mistralai/Mixtral-8x7B-Instruct-v0.1".to_string()])
    }

    fn config(&self) -> Option<&ProviderConfig> {
        Some(&self.config)
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
