//! AI Agent configuration and LLM provider factory.
//!
//! This module provides:
//! - LLM provider configuration from files/env vars
//! - Factory functions to create providers
//! - Helper functions for AI-powered features

use anyhow::{Context, Result};
use rustycode_llm::{
    AnthropicProvider, AzureProvider, BedrockProvider, CohereProvider, CopilotProvider,
    GeminiProvider, HuggingFaceProvider, LLMProvider, MistralProvider, OllamaProvider,
    OpenAiProvider, OpenRouterProvider, PerplexityProvider, ProviderConfig, ProviderError,
    ProviderType, TogetherProvider,
};
use serde::{Deserialize, Serialize};
// Use shared config parsing utilities from rustycode-config
use rustycode_config::{api_key_env_name, default_model_for_provider};
use secrecy::SecretString;
use std::path::PathBuf;

/// Message role in agent conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Message in an agent conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub tool_use_id: Option<String>,
}

/// Load provider config from JSON format
fn load_json_config(contents: &str) -> Result<(String, String, ProviderConfig)> {
    let json: serde_json::Value =
        serde_json::from_str(contents).context("Failed to parse config.json as JSON")?;

    // Get model (required field)
    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .context("Config missing 'model' field")?
        .to_string();

    // Get provider type (defaults to anthropic if not specified)
    let provider_type = json
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("anthropic")
        .to_string();

    // Try API key from multiple locations in JSON:
    // 1. providers.PROVIDER.api_key (new format)
    // 2. api_key (old format at root)
    // 3. Environment variable
    let api_key = json
        .get("providers")
        .and_then(|p| p.get(&provider_type))
        .and_then(|p| p.get("api_key"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            json.get("api_key")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
        })
        .or_else(|| std::env::var(api_key_env_name(&provider_type)).ok());

    // Try base_url from multiple locations in JSON:
    // 1. providers.PROVIDER.base_url (new format)
    // 2. base_url (old format at root)
    let base_url = json
        .get("providers")
        .and_then(|p| p.get(&provider_type))
        .and_then(|p| p.get("base_url"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            json.get("base_url")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from)
        });

    tracing::info!(
        "Loaded JSON config for provider: {}, model: {}, base_url: {:?}, api_key_present: {}",
        provider_type,
        model,
        base_url,
        api_key.is_some()
    );

    Ok((
        provider_type.clone(),
        model.clone(),
        ProviderConfig {
            api_key: api_key.map(|s| SecretString::new(s.into())),
            base_url,
            timeout_seconds: None,
            extra_headers: None,
            retry_config: None,
        },
    ))
}

/// Load LLM provider configuration from config file or environment
pub fn load_provider_config() -> Result<(String, String, ProviderConfig)> {
    // Try multiple config file locations (JSON only)
    let config_paths = vec![
        // Standard config location
        dirs::home_dir().map(|p| p.join(".rustycode").join("config.json")),
        // Workspace config
        std::env::current_dir()
            .ok()
            .map(|d| d.join(".rustycode").join("config.json")),
        // Fallback
        Some(PathBuf::from(".rustycode/config.json")),
    ];

    // Find the first existing config file
    let (_config_path, contents) = {
        let mut found = None;
        for config_path in config_paths {
            let config_path = match config_path {
                Some(p) => p,
                None => continue,
            };

            if config_path.exists() {
                match std::fs::read_to_string(&config_path) {
                    Ok(contents) => {
                        found = Some((config_path, contents));
                        break;
                    }
                    Err(_) => continue,
                }
            }
        }

        found.unwrap_or_else(|| (PathBuf::from(".rustycode/config.json"), String::new()))
    };

    if contents.is_empty() {
        // Fall back to environment variables
        let provider_type =
            std::env::var("RUSTYCODE_PROVIDER").unwrap_or_else(|_| "anthropic".to_string());
        let model = default_model_for_provider(&provider_type);
        let api_key = std::env::var(api_key_env_name(&provider_type)).ok();

        Ok((
            provider_type.clone(),
            model,
            ProviderConfig {
                api_key: api_key.map(|s| SecretString::new(s.into())),
                base_url: None,
                timeout_seconds: None,
                extra_headers: None,
                retry_config: None,
            },
        ))
    } else {
        // Parse JSON config
        load_json_config(&contents)
    }
}

/// Create an LLM provider from configuration
pub fn create_provider(
    provider_type: String,
    config: ProviderConfig,
) -> Result<Box<dyn LLMProvider>, ProviderError> {
    let provider_type = parse_provider_type(&provider_type)
        .map_err(|e| ProviderError::Configuration(e.to_string()))?;

    match provider_type {
        ProviderType::OpenAI => Ok(Box::new(
            OpenAiProvider::new(config, default_model_for_provider("openai").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::Anthropic => Ok(Box::new(
            AnthropicProvider::new(config, default_model_for_provider("anthropic").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::Ollama => {
            Ok(Box::new(OllamaProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Gemini => {
            Ok(Box::new(GeminiProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Copilot => {
            Ok(Box::new(CopilotProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Bedrock => Ok(Box::new(
            BedrockProvider::new(config, default_model_for_provider("bedrock").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::Azure => {
            Ok(Box::new(AzureProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Cohere => {
            Ok(Box::new(CohereProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Mistral => Ok(Box::new(
            MistralProvider::new(config, default_model_for_provider("mistral").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::Together => {
            Ok(Box::new(TogetherProvider::new(config).map_err(|e| {
                ProviderError::Configuration(e.to_string())
            })?))
        }
        ProviderType::Perplexity => Ok(Box::new(
            PerplexityProvider::new(config, default_model_for_provider("perplexity").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::HuggingFace => Ok(Box::new(
            HuggingFaceProvider::new(config, default_model_for_provider("huggingface").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::OpenRouter => Ok(Box::new(
            OpenRouterProvider::new(config, default_model_for_provider("openrouter").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )),
        ProviderType::Custom => Ok(Box::new(
            OpenAiProvider::new(config, default_model_for_provider("openai").clone())
                .map_err(|e| ProviderError::Configuration(e.to_string()))?,
        )), // Custom uses OpenAI-compatible API
        #[allow(unreachable_patterns)]
        _ => Err(ProviderError::Configuration(format!(
            "Unsupported provider type: {:?}",
            provider_type
        ))),
    }
}

/// Create provider from loaded config
pub fn create_provider_from_config() -> Result<Box<dyn LLMProvider>> {
    let (provider_type, _model, config) = load_provider_config()?;
    create_provider(provider_type, config).map_err(|e| anyhow::anyhow!("{}", e))
}

/// Parse provider type from string
fn parse_provider_type(s: &str) -> Result<ProviderType> {
    match s.to_lowercase().as_str() {
        "anthropic" | "claude" => Ok(ProviderType::Anthropic),
        "openai" | "gpt" => Ok(ProviderType::OpenAI),
        "ollama" | "llama" => Ok(ProviderType::Ollama),
        "gemini" | "google" => Ok(ProviderType::Gemini),
        "copilot" | "github" => Ok(ProviderType::Copilot),
        "bedrock" | "aws" => Ok(ProviderType::Bedrock),
        "azure" | "azure_openai" | "microsoft" => Ok(ProviderType::Azure),
        "cohere" => Ok(ProviderType::Cohere),
        "mistral" | "mistral_ai" => Ok(ProviderType::Mistral),
        "together" | "together_ai" => Ok(ProviderType::Together),
        "perplexity" | "pplx" => Ok(ProviderType::Perplexity),
        "huggingface" | "hf" => Ok(ProviderType::HuggingFace),
        "openrouter" => Ok(ProviderType::OpenRouter),
        "custom" => Ok(ProviderType::Custom),
        _ => Err(anyhow::anyhow!("Unknown provider type: {}", s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider_type() {
        assert!(matches!(
            parse_provider_type("anthropic"),
            Ok(ProviderType::Anthropic)
        ));
        assert!(matches!(
            parse_provider_type("claude"),
            Ok(ProviderType::Anthropic)
        ));
        assert!(matches!(
            parse_provider_type("openai"),
            Ok(ProviderType::OpenAI)
        ));
        assert!(parse_provider_type("unknown").is_err());
    }

    // --- MessageRole ---

    #[test]
    fn message_role_variants_distinct() {
        let roles = [
            MessageRole::System,
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Tool,
        ];
        for (i, a) in roles.iter().enumerate() {
            for (j, b) in roles.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn message_role_serde_roundtrip() {
        for role in [
            MessageRole::System,
            MessageRole::User,
            MessageRole::Assistant,
            MessageRole::Tool,
        ] {
            let json = serde_json::to_string(&role).unwrap();
            let decoded: MessageRole = serde_json::from_str(&json).unwrap();
            assert_eq!(role, decoded);
        }
    }

    // --- AgentMessage ---

    #[test]
    fn agent_message_fields() {
        let msg = AgentMessage {
            role: MessageRole::User,
            content: "Hello".into(),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
        };
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.tool_use_id.is_none());
    }

    #[test]
    fn agent_message_with_tool_use_id() {
        let msg = AgentMessage {
            role: MessageRole::Tool,
            content: "result".into(),
            timestamp: chrono::Utc::now(),
            tool_use_id: Some("call_123".into()),
        };
        assert_eq!(msg.tool_use_id, Some("call_123".into()));
    }

    #[test]
    fn agent_message_serde_roundtrip() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "Response".into(),
            timestamp: chrono::Utc::now(),
            tool_use_id: Some("call_456".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, MessageRole::Assistant);
        assert_eq!(decoded.content, "Response");
        assert_eq!(decoded.tool_use_id, Some("call_456".into()));
    }

    // --- parse_provider_type exhaustive ---

    #[test]
    fn parse_provider_type_case_insensitive() {
        assert!(matches!(
            parse_provider_type("Anthropic"),
            Ok(ProviderType::Anthropic)
        ));
        assert!(matches!(
            parse_provider_type("OPENAI"),
            Ok(ProviderType::OpenAI)
        ));
        assert!(matches!(
            parse_provider_type("Gemini"),
            Ok(ProviderType::Gemini)
        ));
    }

    #[test]
    fn parse_provider_type_aliases() {
        assert!(matches!(
            parse_provider_type("gpt"),
            Ok(ProviderType::OpenAI)
        ));
        assert!(matches!(
            parse_provider_type("google"),
            Ok(ProviderType::Gemini)
        ));
        assert!(matches!(
            parse_provider_type("github"),
            Ok(ProviderType::Copilot)
        ));
        assert!(matches!(
            parse_provider_type("aws"),
            Ok(ProviderType::Bedrock)
        ));
        assert!(matches!(
            parse_provider_type("microsoft"),
            Ok(ProviderType::Azure)
        ));
        assert!(matches!(
            parse_provider_type("mistral_ai"),
            Ok(ProviderType::Mistral)
        ));
        assert!(matches!(
            parse_provider_type("together_ai"),
            Ok(ProviderType::Together)
        ));
        assert!(matches!(
            parse_provider_type("pplx"),
            Ok(ProviderType::Perplexity)
        ));
        assert!(matches!(
            parse_provider_type("hf"),
            Ok(ProviderType::HuggingFace)
        ));
    }

    #[test]
    fn parse_provider_type_all_variants() {
        let cases = [
            ("anthropic", ProviderType::Anthropic),
            ("openai", ProviderType::OpenAI),
            ("ollama", ProviderType::Ollama),
            ("gemini", ProviderType::Gemini),
            ("copilot", ProviderType::Copilot),
            ("bedrock", ProviderType::Bedrock),
            ("azure", ProviderType::Azure),
            ("cohere", ProviderType::Cohere),
            ("mistral", ProviderType::Mistral),
            ("together", ProviderType::Together),
            ("perplexity", ProviderType::Perplexity),
            ("huggingface", ProviderType::HuggingFace),
            ("openrouter", ProviderType::OpenRouter),
            ("custom", ProviderType::Custom),
        ];
        for (name, expected) in cases {
            assert!(
                matches!(parse_provider_type(name), Ok(ref e) if std::mem::discriminant(e) == std::mem::discriminant(&expected)),
                "Failed for: {name}"
            );
        }
    }

    // --- load_json_config ---

    #[test]
    fn load_json_config_minimal() {
        let json = r#"{"model": "gpt-4"}"#;
        let (provider_type, model, config) = load_json_config(json).unwrap();
        assert_eq!(provider_type, "anthropic"); // default
        assert_eq!(model, "gpt-4");
        assert!(config.api_key.is_none());
        assert!(config.base_url.is_none());
    }

    #[test]
    fn load_json_config_with_provider() {
        let json = r#"{"model": "claude-3", "provider": "openai"}"#;
        let (provider_type, model, _config) = load_json_config(json).unwrap();
        assert_eq!(provider_type, "openai");
        assert_eq!(model, "claude-3");
    }

    #[test]
    fn load_json_config_missing_model() {
        let json = r#"{"provider": "anthropic"}"#;
        assert!(load_json_config(json).is_err());
    }

    #[test]
    fn load_json_config_with_base_url() {
        let json = r#"{"model": "m", "base_url": "http://localhost:8080"}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert_eq!(config.base_url, Some("http://localhost:8080".to_string()));
    }

    #[test]
    fn load_json_config_with_nested_provider_key() {
        let json = r#"{"model": "m", "provider": "openai", "providers": {"openai": {"api_key": "sk-test"}}}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert!(config.api_key.is_some());
    }

    #[test]
    fn load_json_config_empty_api_key_ignored() {
        let json = r#"{"model": "m", "api_key": ""}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert!(config.api_key.is_none());
    }

    #[test]
    fn load_json_config_invalid_json() {
        assert!(load_json_config("not json").is_err());
    }

    // =========================================================================
    // Terminal-bench: 15 additional tests for agent
    // =========================================================================

    // 1. load_json_config with root-level api_key
    #[test]
    fn load_json_config_root_level_api_key() {
        let json = r#"{"model": "gpt-4", "api_key": "sk-abc123"}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert!(config.api_key.is_some());
    }

    // 2. load_json_config with providers section base_url
    #[test]
    fn load_json_config_nested_provider_base_url() {
        let json = r#"{"model": "m", "provider": "openai", "providers": {"openai": {"base_url": "http://proxy:8080"}}}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert_eq!(config.base_url, Some("http://proxy:8080".to_string()));
    }

    // 3. load_json_config with empty string base_url ignored
    #[test]
    fn load_json_config_empty_base_url_ignored() {
        let json = r#"{"model": "m", "base_url": ""}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert!(config.base_url.is_none());
    }

    // 4. load_json_config with both providers.api_key and root api_key prefers nested
    #[test]
    fn load_json_config_nested_api_key_priority() {
        let json = r#"{"model": "m", "provider": "openai", "api_key": "root-key", "providers": {"openai": {"api_key": "nested-key"}}}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert!(config.api_key.is_some());
        // Nested key should be preferred over root-level key
    }

    // 5. parse_provider_type with azure_openai alias
    #[test]
    fn parse_provider_type_azure_openai() {
        assert!(matches!(
            parse_provider_type("azure_openai"),
            Ok(ProviderType::Azure)
        ));
    }

    // 6. parse_provider_type with llama alias
    #[test]
    fn parse_provider_type_llama() {
        assert!(matches!(
            parse_provider_type("llama"),
            Ok(ProviderType::Ollama)
        ));
    }

    // 7. MessageRole System serde roundtrip
    #[test]
    fn message_role_system_serde() {
        let json = serde_json::to_string(&MessageRole::System).unwrap();
        let decoded: MessageRole = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, MessageRole::System);
    }

    // 8. AgentMessage with System role serde roundtrip
    #[test]
    fn agent_message_system_role_serde() {
        let msg = AgentMessage {
            role: MessageRole::System,
            content: "You are a helpful assistant".into(),
            timestamp: chrono::Utc::now(),
            tool_use_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, MessageRole::System);
        assert_eq!(decoded.content, "You are a helpful assistant");
    }

    // 9. load_json_config with empty string providers api_key falls through
    #[test]
    fn load_json_config_empty_provider_api_key_falls_through() {
        let json =
            r#"{"model": "m", "provider": "openai", "providers": {"openai": {"api_key": ""}}}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        // Empty string in nested providers.api_key should be filtered out
        assert!(config.api_key.is_none());
    }

    // 10. load_json_config with root-level base_url
    #[test]
    fn load_json_config_root_base_url() {
        let json = r#"{"model": "m", "base_url": "http://localhost:11434"}"#;
        let (_ptype, _model, config) = load_json_config(json).unwrap();
        assert_eq!(config.base_url, Some("http://localhost:11434".to_string()));
    }

    // 11. AgentMessage with all fields populated serde roundtrip
    #[test]
    fn agent_message_all_fields_serde() {
        let msg = AgentMessage {
            role: MessageRole::Tool,
            content: "file contents here".into(),
            timestamp: chrono::Utc::now(),
            tool_use_id: Some("call_abc_123".into()),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.role, MessageRole::Tool);
        assert_eq!(decoded.content, "file contents here");
        assert_eq!(decoded.tool_use_id, Some("call_abc_123".into()));
    }

    // 12. parse_provider_type with remaining aliases
    #[test]
    fn parse_provider_type_remaining_aliases() {
        assert!(matches!(
            parse_provider_type("azure"),
            Ok(ProviderType::Azure)
        ));
        assert!(matches!(
            parse_provider_type("microsoft"),
            Ok(ProviderType::Azure)
        ));
        assert!(matches!(
            parse_provider_type("custom"),
            Ok(ProviderType::Custom)
        ));
        assert!(matches!(
            parse_provider_type("openrouter"),
            Ok(ProviderType::OpenRouter)
        ));
    }

    // 13. load_json_config with extra unknown fields ignores them
    #[test]
    fn load_json_config_ignores_extra_fields() {
        let json = r#"{"model": "m", "unknown_field": 42, "another": true}"#;
        let (ptype, model, _config) = load_json_config(json).unwrap();
        assert_eq!(ptype, "anthropic");
        assert_eq!(model, "m");
    }

    // 14. AgentMessage timestamp survives serde roundtrip
    #[test]
    fn agent_message_timestamp_serde() {
        let now = chrono::Utc::now();
        let msg = AgentMessage {
            role: MessageRole::User,
            content: "test".into(),
            timestamp: now,
            tool_use_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&json).unwrap();
        // Timestamps should roundtrip within 1 second
        let diff = (decoded.timestamp - now).num_milliseconds().abs();
        assert!(diff < 1000);
    }

    // 15. parse_provider_type with empty string returns error
    #[test]
    fn parse_provider_type_empty_string() {
        assert!(parse_provider_type("").is_err());
    }
}
