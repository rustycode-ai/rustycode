//! Hugging Face Inference API LLM provider implementation.
//!
//! This provider supports Hugging Face's Inference API which provides access to
//! thousands of models hosted on the Hugging Face Hub.

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

/// Default Hugging Face Inference API endpoint
const HF_API_ENDPOINT: &str = "https://api-inference.huggingface.co/v1/chat/completions";

/// Hugging Face Inference API LLM provider
pub struct HuggingFaceProvider {
    config: ProviderConfig,
    route: Route,
}

impl HuggingFaceProvider {
    pub fn new(config: ProviderConfig, _default_model: String) -> Result<Self, ProviderError> {
        // Validate config using provider metadata
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| HF_API_ENDPOINT.to_string());

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
        .with_name("huggingface-chat");

        Ok(Self { config, route })
    }

    /// Get metadata for this provider
    pub fn metadata() -> ProviderMetadata {
        let mut env_mappings = HashMap::new();
        env_mappings.insert("api_key".to_string(), "HF_TOKEN".to_string());
        env_mappings.insert("api_key".to_string(), "HUGGINGFACE_API_KEY".to_string());

        ProviderMetadata {
            provider_id: "huggingface".to_string(),
            display_name: "Hugging Face".to_string(),
            description: "Access thousands of models on the Hugging Face Hub via Inference API".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![
                    ConfigField {
                        name: "api_key".to_string(),
                        label: "API Key".to_string(),
                        description: "Your Hugging Face API token (from hf.co/settings/tokens)".to_string(),
                        field_type: ConfigFieldType::APIKey,
                        placeholder: Some("hf_...".to_string()),
                        default: None,
                        validation_pattern: Some("^hf_.*".to_string()),
                        validation_error: Some("API key must start with 'hf_'".to_string()),
                        sensitive: true,
                    },
                ],
                optional_fields: vec![
                    ConfigField {
                        name: "base_url".to_string(),
                        label: "Base URL".to_string(),
                        description: "Custom API endpoint (optional)".to_string(),
                        field_type: ConfigFieldType::URL,
                        placeholder: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        default: Some("https://api-inference.huggingface.co/v1/chat/completions".to_string()),
                        validation_pattern: None,
                        validation_error: None,
                        sensitive: false,
                    },
                ],
                env_mappings,
            },
            prompt_template: PromptTemplate {
                base_template: "You are a helpful AI assistant accessed via the Hugging Face Inference API.\n\n=== CONTEXT ===\n{context}\n\n=== GUIDELINES ===\n- Provide clear, accurate responses\n- When writing code, include explanations\n- Ask clarifying questions when needed\n- Be thorough but concise".to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: false,
                    preferred_prompt_length: crate::provider_metadata::PromptLength::Medium,
                    special_instructions: vec![
                        "You can access various models from the Hugging Face Hub.".to_string(),
                        "Model capabilities may vary depending on the selected model.".to_string(),
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
impl LLMProvider for HuggingFaceProvider {
    fn name(&self) -> &'static str {
        "huggingface"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "meta-llama/Llama-4-8B-Instruct".to_string(),
            "meta-llama/Llama-3.3-70B-Instruct".to_string(),
            "meta-llama/Llama-3.1-405B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-8B-Instruct".to_string(),
            "meta-llama/Meta-Llama-3-70B-Instruct".to_string(),
            "mistralai/Mistral-7B-Instruct-v0.2".to_string(),
            "mistralai/Mixtral-8x7B-Instruct-v0.1".to_string(),
            "mistralai/Mixtral-8x22B-Instruct-v0.1".to_string(),
            "google/gemma-7b".to_string(),
            "tiiuae/falcon-7b-instruct".to_string(),
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
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert_eq!(provider.name(), "huggingface");
    }

    #[test]
    fn test_creates_provider() {
        let config = make_config(Some("hf_test-key"));
        let provider = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(provider.is_ok());
    }

    #[test]
    fn test_missing_api_key_fails() {
        let config = make_config(None);
        let result = HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let config = make_config(Some("hf_test-key"));
        let provider =
            HuggingFaceProvider::new(config, "meta-llama/Llama-3-70b".to_string()).unwrap();
        assert!(provider.config().is_some());
    }
}
