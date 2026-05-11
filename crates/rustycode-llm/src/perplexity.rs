//! Perplexity AI LLM provider implementation.
//!
//! This provider supports Perplexity AI's API which provides access to
//! various LLM models including their own pplx models and others.

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

/// Default Perplexity API endpoint
const PERPLEXITY_API_ENDPOINT: &str = "https://api.perplexity.ai/chat/completions";

/// Perplexity AI LLM provider
pub struct PerplexityProvider {
    config: ProviderConfig,
    route: Route,
}

impl PerplexityProvider {
    pub fn new(config: ProviderConfig, _default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| PERPLEXITY_API_ENDPOINT.to_string());

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
        .with_name("perplexity-chat");

        Ok(Self { config, route })
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
            model_behavior_profiles: HashMap::new(),
        }
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
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "llama-3.1-sonar-huge-128k-online".to_string(),
            "llama-3.1-sonar-large-128k-online".to_string(),
            "llama-3.1-sonar-small-128k-online".to_string(),
            "mixtral-8x7b-instruct".to_string(),
        ])
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

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("pplx-test-key"));
        let provider = PerplexityProvider::new(config, "sonar".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
