//! Zhipu AI LLM provider implementation.
//!
//! This provider supports Zhipu AI's API (GLM models) which are mostly
//! OpenAI-compatible with some GLM-specific features.

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

/// Default Zhipu AI API endpoint
const ZHIPU_DEFAULT_ENDPOINT: &str = "https://api.z.ai/api/coding/paas/v4";

/// Zhipu AI LLM provider
pub struct ZhipuProvider {
    config: ProviderConfig,
    route: Route,
}

impl ZhipuProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| ZHIPU_DEFAULT_ENDPOINT.to_string());

        let url = format!("{}/chat/completions", endpoint);

        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(
            config
                .api_key
                .clone()
                .ok_or_else(|| ProviderError::auth("Missing API key"))?,
        ));

        let route = Route::new(
            url,
            Box::new(OpenAIChatProtocol),
            Box::new(
                HttpTransport::new(config.timeout_seconds.unwrap_or(300))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_name("zhipu-chat");

        Ok(Self { config, route })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "zhipu".to_string(),
            display_name: "Zhipu AI".to_string(),
            description: "GLM models from Zhipu AI".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "API Key".to_string(),
                    description: "Your Zhipu AI API key from z.ai".to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("...".to_string()),
                    default: None,
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "API endpoint (defaults to https://api.z.ai/api/coding/paas/v4)"
                        .to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    default: Some(ZHIPU_DEFAULT_ENDPOINT.to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "ZHIPU_API_KEY".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant powered by GLM.".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![],
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
                    model_id: "glm-5.1".to_string(),
                    display_name: "GLM-5.1".to_string(),
                    description: "Flagship — 8h autonomous work, matches Claude Opus 4.6 (200K context)"
                        .to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Long-horizon agents".to_string(),
                        "Complex reasoning".to_string(),
                        "Code generation".to_string(),
                    ],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "glm-5".to_string(),
                    display_name: "GLM-5".to_string(),
                    description: "Strong coding, reliable multi-step reasoning (200K context)"
                        .to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex reasoning".to_string(),
                        "Code generation".to_string(),
                        "Agent tasks".to_string(),
                    ],
                    cost_tier: 4,
                },
                ModelInfo {
                    model_id: "glm-5-turbo".to_string(),
                    display_name: "GLM-5 Turbo".to_string(),
                    description: "Fast GLM-5 optimized for dynamic long-chain tasks (200K context)"
                        .to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Quick reasoning".to_string(),
                        "Code generation".to_string(),
                        "Chat".to_string(),
                    ],
                    cost_tier: 3,
                },
                ModelInfo {
                    model_id: "glm-4.7-flash".to_string(),
                    display_name: "GLM-4.7 Flash".to_string(),
                    description: "30B lightweight model, outperforms similar-scale open-source (128K context)"
                        .to_string(),
                    context_window: 128_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Fast response".to_string(),
                        "Budget workloads".to_string(),
                    ],
                    cost_tier: 1,
                },
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for ZhipuProvider {
    fn name(&self) -> &'static str {
        "zhipu"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "glm-5".to_string(),
            "glm-4-plus".to_string(),
            "glm-4-flash".to_string(),
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
            base_url: None,
            timeout_seconds: Some(300),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_zhipu_provider_creation() {
        let config = make_config(Some("test-key"));
        let provider = ZhipuProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = ZhipuProvider::new(config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("test-key"));
        let provider = ZhipuProvider::new(config).unwrap();
        assert!(provider.config().is_some());
    }

    #[test]
    fn glm5_has_200k_context_window() {
        let meta = ZhipuProvider::metadata();
        let glm5 = meta
            .recommended_models
            .iter()
            .find(|m| m.model_id == "glm-5")
            .expect("GLM-5 should be in recommended_models");

        assert_eq!(
            glm5.context_window, 200_000,
            "GLM-5 must have 200K context window for SWE-bench"
        );
        assert!(
            glm5.supports_tools,
            "GLM-5 must support tool calling for SWE-bench"
        );
        assert!(
            glm5.description.contains("200K"),
            "GLM-5 description should mention 200K context"
        );
    }

    #[test]
    fn zhipu_metadata_has_required_models() {
        let meta = ZhipuProvider::metadata();
        let model_ids: Vec<&str> = meta
            .recommended_models
            .iter()
            .map(|m| m.model_id.as_str())
            .collect();

        assert!(model_ids.contains(&"glm-5"), "Missing glm-5");
        assert!(model_ids.contains(&"glm-4-plus"), "Missing glm-4-plus");
        assert!(model_ids.contains(&"glm-4-flash"), "Missing glm-4-flash");
    }
}
