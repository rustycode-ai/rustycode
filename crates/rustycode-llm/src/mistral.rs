//! Mistral AI LLM provider implementation.
//!
//! This provider supports Mistral AI's API which provides access to
//! language models like Mistral 7B, Mixtral 8x7B, Mistral Large, etc.

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
    ConfigField, ConfigFieldType, ConfigSchema, PromptOptimizations, PromptTemplate,
    ProviderMetadata, ToolCallingMetadata, ToolFormat,
};
use crate::route::Route;
use crate::transport::HttpTransport;
use crate::wire::openai_chat::OpenAIChatProtocol;

/// Default Mistral AI API endpoint
const MISTRAL_API_ENDPOINT: &str = "https://api.mistral.ai/v1/chat/completions";

/// Mistral AI LLM provider
pub struct MistralProvider {
    config: ProviderConfig,
    route: Route,
}

impl MistralProvider {
    pub fn new(config: ProviderConfig, _model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| MISTRAL_API_ENDPOINT.to_string());

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
        .with_name("mistral-chat");

        Ok(Self { config, route })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "MISTRAL_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "mistral".to_string(),
            display_name: "Mistral AI".to_string(),
            description: "Advanced AI models including Mistral Large, Mixtral, and Codestral".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Mistral API key from console.mistral.ai".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("Your API key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        default: Some("https://api.mistral.ai/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant provided by Mistral AI.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate, and well-structured responses\n- When writing code, include explanations and best practices\n- Ask clarifying questions when requirements are ambiguous\n- Be thorough but concise in your answers".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Mistral models excel at reasoning and code generation.".to_string(),
                        "Use structured formatting for complex responses.".to_string(),
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
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for MistralProvider {
    fn name(&self) -> &'static str {
        "mistral"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "mistral-large-2407".to_string(),
            "mixtral-8x22b-2407".to_string(),
            "mixtral-8x7b-2407".to_string(),
            "mistral-medium-2312".to_string(),
            "mistral-small-2409".to_string(),
            "codestral-2405".to_string(),
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
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string()).unwrap();
        assert_eq!(provider.name(), "mistral");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("test-key"));
        let provider = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = MistralProvider::new(config, "mistral-large-latest".to_string());
        assert!(result.is_err());
    }
}
