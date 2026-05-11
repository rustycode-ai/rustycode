//! Azure OpenAI LLM provider implementation.
//!
//! This provider supports Azure OpenAI Service which provides access to
//! OpenAI models (GPT-3.5, GPT-4, etc.) hosted on Microsoft Azure.

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
use crate::provider_metadata::{ConfigField, ConfigFieldType, ConfigSchema, ProviderMetadata};
use crate::route::Route;
use crate::transport::HttpTransport;
use crate::wire::openai_chat::OpenAIChatProtocol;

/// Default Azure OpenAI API version
const DEFAULT_API_VERSION: &str = "2024-02-15-preview";

/// Azure OpenAI LLM provider
pub struct AzureProvider {
    config: ProviderConfig,
    route: Route,
}

impl AzureProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .ok_or_else(|| ProviderError::Configuration("Missing base_url".to_string()))?;
        let deployment =
            std::env::var("AZURE_OPENAI_DEPLOYMENT").unwrap_or_else(|_| "gpt-4".to_string());
        let api_version = std::env::var("AZURE_OPENAI_API_VERSION")
            .unwrap_or_else(|_| DEFAULT_API_VERSION.to_string());

        let url = format!(
            "{}/openai/deployments/{}/chat/completions?api-version={}",
            endpoint, deployment, api_version
        );

        let api_key = config
            .api_key
            .clone()
            .ok_or_else(|| ProviderError::Auth("Missing API key".to_string()))?;
        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(api_key));

        let route = Route::new(
            url,
            Box::new(OpenAIChatProtocol),
            Box::new(
                HttpTransport::new(config.timeout_seconds.unwrap_or(180))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_name("azure-chat");

        Ok(Self { config, route })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "azure".to_string(),
            display_name: "Azure OpenAI Service".to_string(),
            description: "OpenAI models hosted on Microsoft Azure with enterprise-grade security and compliance".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Azure OpenAI API key from the Azure portal".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("your-azure-api-key".to_string()),
                        default: None,
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: true,
                    },
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Endpoint URL".to_string(),
                        description: "Your Azure OpenAI endpoint (e.g., https://my-resource.openai.azure.com)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://my-resource.openai.azure.com".to_string()),
                        default: None,
                        validation_pattern: Some("^https?://.*\\.openai\\.azure\\.com.*".to_string()),
                        validation_error: Some("Endpoint must be a valid Azure OpenAI URL".to_string()),
                        sensitive: false,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "deployment".to_string(),
                        label: "Deployment Name".to_string(),
                        description: "The deployment name (not the base model name)".to_string(),
                        field_type: ConfigFieldType::String,
                        placeholder: Some("gpt-4".to_string()),
                        default: Some("gpt-4".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "AZURE_OPENAI_API_KEY".to_string());
                    map.insert("base_url".to_string(), "AZURE_OPENAI_ENDPOINT".to_string());
                    map.insert("deployment".to_string(), "AZURE_OPENAI_DEPLOYMENT".to_string());
                    map
                },
            },
            prompt_template: crate::provider_metadata::PromptTemplate {
                base_template: "You are an AI assistant hosted on Azure OpenAI.\n\n{context}".to_string(),
                optimizations: crate::provider_metadata::PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "Follow Azure OpenAI best practices.".to_string(),
                        "Provide enterprise-grade responses.".to_string(),
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
            recommended_models: vec![
                crate::provider_metadata::ModelInfo {
                    model_id: "gpt-4o".to_string(),
                    display_name: "GPT-4o".to_string(),
                    description: "Fastest and most capable omni model".to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec!["General purpose".to_string(), "Fast responses".to_string()],
                    cost_tier: 3,
                },
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for AzureProvider {
    fn name(&self) -> &'static str {
        "azure"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "o3".to_string(),
            "o3-mini".to_string(),
            "o1".to_string(),
            "o1-mini".to_string(),
            "gpt-4.1".to_string(),
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-4".to_string(),
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

    fn make_config(key: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: key.map(|k| SecretString::new(k.to_string().into())),
            base_url: Some("https://test.openai.azure.com".to_string()),
            timeout_seconds: None,
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_azure_provider_new() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_azure_provider_missing_key() {
        let config = make_config(None);
        let provider = AzureProvider::new(config);
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = AzureProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }
}
