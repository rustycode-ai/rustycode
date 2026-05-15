//! GitHub Copilot LLM provider implementation.
//!
//! GitHub Copilot uses an OpenAI-compatible API with GitHub-specific authentication.
//! The provider supports GitHub tokens and Copilot-specific models.

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

/// GitHub Copilot LLM provider
pub struct CopilotProvider {
    config: ProviderConfig,
    route: Route,
}

impl CopilotProvider {
    pub fn new(config: ProviderConfig) -> Result<Self, ProviderError> {
        Self::metadata().validate_config(&config)?;

        let endpoint = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.githubcopilot.com".to_string());

        let url = format!("{}/chat/completions", endpoint);

        let auth: Box<dyn AuthMethod> = Box::new(crate::auth::BearerAuth::new(
            config
                .api_key
                .clone()
                .ok_or_else(|| ProviderError::auth("Missing API key"))?,
        ));

        let extra_headers = vec![
            (
                "copilot-integration-id".to_string(),
                "vscode-chat".to_string(),
            ),
            ("editor-version".to_string(), "vscode/1.0.0".to_string()),
        ];

        let route = Route::new(
            url,
            Box::new(OpenAIChatProtocol),
            Box::new(
                HttpTransport::new(config.timeout_seconds.unwrap_or(120))
                    .map_err(|e| ProviderError::Configuration(e.to_string()))?,
            ),
            auth,
        )
        .with_extra_headers(extra_headers)
        .with_name("copilot-chat");

        Ok(Self { config, route })
    }

    pub fn metadata() -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "copilot".to_string(),
            display_name: "GitHub Copilot".to_string(),
            description: "GitHub Copilot's AI models with OpenAI-compatible API".to_string(),
            config_schema: ConfigSchema {
                required_fields: vec![ConfigField {
                    name: "api_key".to_string(),
                    label: "GitHub Token".to_string(),
                    description:
                        "Your GitHub personal access token from github.com/settings/tokens"
                            .to_string(),
                    field_type: ConfigFieldType::APIKey,
                    placeholder: Some("ghp_...".to_string()),
                    default: None,
                    validation_pattern: Some("^ghp_.*".to_string()),
                    validation_error: Some("GitHub token must start with 'ghp-'".to_string()),
                    sensitive: true,
                }],
                optional_fields: vec![ConfigField {
                    name: "base_url".to_string(),
                    label: "Base URL".to_string(),
                    description: "Custom API endpoint (optional)".to_string(),
                    field_type: ConfigFieldType::URL,
                    placeholder: Some("https://api.githubcopilot.com".to_string()),
                    default: Some("https://api.githubcopilot.com".to_string()),
                    validation_pattern: None,
                    validation_error: None,
                    sensitive: false,
                }],
                env_mappings: {
                    let mut map = HashMap::new();
                    map.insert("api_key".to_string(), "GITHUB_TOKEN".to_string());
                    map
                },
            },
            prompt_template: PromptTemplate {
                base_template:
                    "You are a helpful AI assistant powered by GitHub Copilot. {context}"
                        .to_string(),
                optimizations: PromptOptimizations {
                    prefer_xml_structure: false,
                    include_examples: true,
                    preferred_prompt_length: PromptLength::Medium,
                    special_instructions: vec![
                        "Copilot models are optimized for coding tasks.".to_string(),
                        "Provide clear, concise code examples when helpful.".to_string(),
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
                    model_id: "gpt-5.4-copilot".to_string(),
                    display_name: "GPT-5.4 Copilot".to_string(),
                    description: "High-performance GPT-5.4 with 1M context via Copilot".to_string(),
                    context_window: 1_000_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex coding tasks".to_string(),
                        "Architecture design".to_string(),
                        "Code refactoring".to_string(),
                    ],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "gpt-5.5-copilot".to_string(),
                    display_name: "GPT-5.5 Copilot".to_string(),
                    description: "Most capable OpenAI model via Copilot".to_string(),
                    context_window: 1_000_000,
                    supports_tools: true,
                    use_cases: vec![
                        "Complex reasoning".to_string(),
                        "Advanced coding".to_string(),
                    ],
                    cost_tier: 5,
                },
                ModelInfo {
                    model_id: "claude-sonnet-4-6-copilot".to_string(),
                    display_name: "Claude Sonnet 4.6 Copilot".to_string(),
                    description: "Anthropic Claude Sonnet 4.6 via Copilot".to_string(),
                    context_window: 200_000,
                    supports_tools: true,
                    use_cases: vec![
                        "General coding".to_string(),
                        "Code explanation".to_string(),
                        "Debugging".to_string(),
                    ],
                    cost_tier: 4,
                },
            ],
            model_behavior_profiles: HashMap::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for CopilotProvider {
    fn name(&self) -> &'static str {
        "copilot"
    }

    async fn is_available(&self) -> bool {
        self.config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.expose_secret().is_empty())
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec![
            "gpt-4.1-copilot".to_string(),
            "gpt-4o-copilot".to_string(),
            "gpt-4o-mini-copilot".to_string(),
            "o1-copilot".to_string(),
            "o1-mini-copilot".to_string(),
            "o3-mini-copilot".to_string(),
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

    fn make_config(token: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            api_key: token.map(|s| SecretString::new(s.to_string().into())),
            base_url: None,
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        }
    }

    #[test]
    fn test_requires_token() {
        assert!(CopilotProvider::new(make_config(None)).is_err());
    }

    #[test]
    fn test_creates_with_token() {
        assert!(CopilotProvider::new(make_config(Some("ghp_test123"))).is_ok());
    }

    #[test]
    fn test_provider_name() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert_eq!(p.name(), "copilot");
    }

    #[tokio::test]
    async fn test_is_available() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert!(p.is_available().await);
    }

    #[tokio::test]
    async fn test_config_returns_some() {
        let p = CopilotProvider::new(make_config(Some("ghp_test"))).unwrap();
        assert!(p.config().is_some());
    }
}
