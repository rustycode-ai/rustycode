//! Provider management for LLM providers
//!
//! This module provides provider information, configuration, and OAuth flow
//! functionality that can be reused across different UI implementations.
#![allow(dead_code)]

use crate::ui::model_selector::ModelInfo as SelectorModelInfo;

/// Information about an LLM provider
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderInfo {
    pub name: String,
    pub provider_type: String,
    pub description: String,
    pub api_key_env: String,
    pub default_model: String,
    pub is_configured: bool,
}

impl ProviderInfo {
    /// Create a new ProviderInfo
    pub fn new(
        name: impl Into<String>,
        provider_type: impl Into<String>,
        description: impl Into<String>,
        api_key_env: impl Into<String>,
        default_model: impl Into<String>,
    ) -> Self {
        let provider_type = provider_type.into();
        let api_key_env = api_key_env.into();
        let is_configured = if api_key_env == "N/A" {
            true
        } else {
            provider_has_credentials(&provider_type)
        };

        Self {
            name: name.into(),
            provider_type,
            description: description.into(),
            api_key_env,
            default_model: default_model.into(),
            is_configured,
        }
    }

    /// Get the API key from environment
    pub fn get_api_key(&self) -> Option<String> {
        if self.api_key_env == "N/A" {
            Some("no-api-key-required".to_string())
        } else {
            std::env::var(&self.api_key_env).ok()
        }
    }

    /// Check if provider is configured with valid credentials
    pub fn is_configured(&self) -> bool {
        self.is_configured
    }
}

/// Get list of available LLM providers
///
/// This function checks the environment for API keys and returns
/// a list of all supported providers with their configuration status.
///
/// # Returns
/// Vector of ProviderInfo objects
pub fn get_available_providers() -> Vec<ProviderInfo> {
    vec![
        ProviderInfo::new(
            "Anthropic Claude",
            "anthropic",
            "Advanced AI assistant for complex tasks",
            "ANTHROPIC_API_KEY",
            "claude-sonnet-4-6",
        ),
        ProviderInfo::new(
            "OpenAI GPT",
            "openai",
            "Versatile AI model for various tasks",
            "OPENAI_API_KEY",
            "gpt-4o",
        ),
        ProviderInfo::new(
            "OpenRouter (Free Models)",
            "openrouter",
            "Free LLM models including Gemma, Llama, Phi-3, Mistral",
            "OPENROUTER_API_KEY",
            "google/gemma-2-9b:free",
        ),
        ProviderInfo::new(
            "Ollama",
            "ollama",
            "Local LLM hosting (no API key required)",
            "N/A",
            "llama3",
        ),
        ProviderInfo::new(
            "Google Gemini",
            "gemini",
            "Google's multimodal AI assistant",
            "GEMINI_API_KEY",
            "gemini-pro",
        ),
        ProviderInfo::new(
            "GitHub Copilot",
            "copilot",
            "AI-powered code completion",
            "GITHUB_TOKEN",
            "gpt-4o-copilot",
        ),
        ProviderInfo::new(
            "Custom",
            "custom",
            "OpenAI-compatible API endpoint",
            "CUSTOM_API_KEY",
            "custom-model",
        ),
    ]
}

/// Check if a provider has credentials configured (env var or config.json)
fn provider_has_credentials(provider_type: &str) -> bool {
    // Ollama never needs a key
    if provider_type == "ollama" {
        return true;
    }

    // Check env var
    let env_name = rustycode_config::api_key_env_name(provider_type);
    if std::env::var(&env_name).is_ok() {
        return true;
    }

    // Check config.json for provider-specific key
    if let Some(config_path) = dirs::home_dir().map(|p| p.join(".rustycode").join("config.json")) {
        if config_path.exists() {
            if let Ok(contents) = std::fs::read_to_string(&config_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                    // Check providers.PROVIDER.api_key
                    if json
                        .get("providers")
                        .and_then(|p| p.get(provider_type))
                        .and_then(|p| p.get("api_key"))
                        .and_then(|v| v.as_str())
                        .is_some()
                    {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Get all available models, grouped by provider.
///
/// Models from providers with configured credentials appear first.
/// Models from unconfigured providers are still shown but marked.
///
/// # Returns
/// Vector of ModelInfo objects with provider, context window, costs, etc.
pub fn get_all_available_models() -> Vec<SelectorModelInfo> {
    let mut configured = Vec::new();
    let mut unconfigured = Vec::new();

    // Helper: add models from provider metadata
    let add_provider_models = |provider: &str, list: &mut Vec<SelectorModelInfo>| {
        if let Some(metadata) = rustycode_llm::provider_metadata::get_metadata(provider) {
            for (idx, model) in metadata.recommended_models.iter().enumerate() {
                list.push(
                    SelectorModelInfo::new(
                        model.model_id.clone(),
                        model.display_name.clone(),
                        provider.to_string(),
                        model.description.clone(),
                    )
                    .with_context_window(model.context_window)
                    .with_costs(model.cost_tier as f64 * 3.0, model.cost_tier as f64 * 3.0)
                    .with_capabilities(model.use_cases.clone())
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
                );
            }
        }
    };

    // Static model lists for providers without metadata
    let add_ollama_models = |list: &mut Vec<SelectorModelInfo>| {
        let models = vec![
            ("llama3", "Llama 3", "Powerful open-source model"),
            (
                "llama3.1",
                "Llama 3.1",
                "Latest Llama 3 with better reasoning",
            ),
            ("mistral", "Mistral", "Efficient open-source model"),
            ("codellama", "Code Llama", "Specialized for code generation"),
            ("phi3", "Phi-3", "Microsoft's efficient small model"),
        ];
        for (idx, (id, name, desc)) in models.into_iter().enumerate() {
            list.push(
                SelectorModelInfo::new(id, name, "ollama".to_string(), desc)
                    .with_context_window(8192)
                    .with_costs(0.0, 0.0)
                    .with_capabilities(vec!["local".to_string(), "free".to_string()])
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
            );
        }
    };

    let add_openrouter_models = |list: &mut Vec<SelectorModelInfo>| {
        let models = vec![
            (
                "google/gemma-2-9b:free",
                "Gemma 2 9B (Free)",
                "Google's efficient model",
            ),
            (
                "meta-llama-3.1-8b-instruct:free",
                "Llama 3.1 8B (Free)",
                "Meta's instruction model",
            ),
            (
                "microsoft/phi-3-mini-128k-instruct:free",
                "Phi-3 Mini (Free)",
                "Microsoft's small model",
            ),
        ];
        for (idx, (id, name, desc)) in models.into_iter().enumerate() {
            list.push(
                SelectorModelInfo::new(id, name, "openrouter".to_string(), desc)
                    .with_context_window(128000)
                    .with_costs(0.0, 0.0)
                    .with_capabilities(vec!["free".to_string(), "api".to_string()])
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
            );
        }
    };

    let add_litert_lm_models = |list: &mut Vec<SelectorModelInfo>| {
        let models: Vec<(&str, &str, &str, usize)> = vec![
            (
                "gemma-4-e2b-it",
                "Gemma 4 E2B (LiteRT-LM)",
                "Gemma 4 2B local inference",
                8192,
            ),
            (
                "gemma-4-e4b-it",
                "Gemma 4 E4B (LiteRT-LM)",
                "Gemma 4 4B local inference",
                8192,
            ),
            (
                "gemma-3n-e4b",
                "Gemma 3N E4B (LiteRT-LM)",
                "Gemma 3N local inference",
                8192,
            ),
            (
                "phi-4-mini",
                "Phi-4 Mini (LiteRT-LM)",
                "Phi-4 local inference",
                8192,
            ),
        ];
        for (idx, (id, name, desc, ctx)) in models.into_iter().enumerate() {
            list.push(
                SelectorModelInfo::new(id, name, "litert-lm".to_string(), desc)
                    .with_context_window(ctx)
                    .with_costs(0.0, 0.0)
                    .with_capabilities(vec!["local".to_string(), "free".to_string()])
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
            );
        }
    };

    // Partition models by whether the provider has credentials
    for provider in &["anthropic", "openai", "gemini"] {
        let target = if provider_has_credentials(provider) {
            &mut configured
        } else {
            &mut unconfigured
        };
        add_provider_models(provider, target);
    }

    // Ollama always configured
    add_ollama_models(&mut configured);

    // LiteRT-LM always available (local, no credentials needed)
    add_litert_lm_models(&mut configured);

    // OpenRouter needs credentials check
    if provider_has_credentials("openrouter") {
        add_openrouter_models(&mut configured);
    } else {
        add_openrouter_models(&mut unconfigured);
    }

    // ── Cross-provider models ──────────────────────────────────────────
    // Same popular models available through alternative providers.
    // These appear after the direct-provider entries.

    // OpenRouter cross-provider entries (if configured)
    if provider_has_credentials("openrouter") {
        let cross_models: Vec<(&str, &str, &str, usize, f64)> = vec![
            // (id, display_name, description, context_window, cost_tier)
            (
                "anthropic/claude-sonnet-4-20250514",
                "Claude Sonnet 4 (OpenRouter)",
                "Anthropic's balanced model via OpenRouter",
                200000,
                3.0,
            ),
            (
                "anthropic/claude-haiku-4-20250514",
                "Claude Haiku 4 (OpenRouter)",
                "Anthropic's fast model via OpenRouter",
                200000,
                1.0,
            ),
            (
                "openai/gpt-4o",
                "GPT-4o (OpenRouter)",
                "OpenAI's flagship via OpenRouter",
                128000,
                2.5,
            ),
            (
                "openai/gpt-4o-mini",
                "GPT-4o Mini (OpenRouter)",
                "OpenAI's efficient model via OpenRouter",
                128000,
                0.15,
            ),
            (
                "google/gemini-2.5-pro-preview",
                "Gemini 2.5 Pro (OpenRouter)",
                "Google's thinking model via OpenRouter",
                1000000,
                1.25,
            ),
            (
                "meta-llama/llama-3.1-405b-instruct",
                "Llama 3.1 405B (OpenRouter)",
                "Meta's largest model via OpenRouter",
                131072,
                0.8,
            ),
            (
                "deepseek/deepseek-chat",
                "DeepSeek V3 (OpenRouter)",
                "DeepSeek's reasoning model via OpenRouter",
                65536,
                0.14,
            ),
            (
                "deepseek/deepseek-r1",
                "DeepSeek R1 (OpenRouter)",
                "DeepSeek's reasoning model via OpenRouter",
                65536,
                0.55,
            ),
        ];
        for (idx, (id, name, desc, ctx, cost)) in cross_models.into_iter().enumerate() {
            configured.push(
                SelectorModelInfo::new(id, name, "openrouter".to_string(), desc)
                    .with_context_window(ctx)
                    .with_costs(cost * 3.0, cost * 3.0)
                    .with_capabilities(vec!["api".to_string()])
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
            );
        }
    }

    // GitHub Copilot cross-provider entries (if configured)
    if provider_has_credentials("copilot") {
        let copilot_models: Vec<(&str, &str, &str, usize)> = vec![
            (
                "gpt-4o",
                "GPT-4o (Copilot)",
                "OpenAI GPT-4o via GitHub Copilot",
                128000,
            ),
            (
                "gpt-4o-mini",
                "GPT-4o Mini (Copilot)",
                "GPT-4o Mini via GitHub Copilot",
                128000,
            ),
            (
                "claude-sonnet-4-6",
                "Claude Sonnet 4 (Copilot)",
                "Anthropic Claude via GitHub Copilot",
                200000,
            ),
            (
                "o1",
                "o1 (Copilot)",
                "OpenAI o1 reasoning via GitHub Copilot",
                200000,
            ),
            (
                "gemini-2.0-flash-001",
                "Gemini 2.0 Flash (Copilot)",
                "Google Gemini via GitHub Copilot",
                1000000,
            ),
        ];
        for (idx, (id, name, desc, ctx)) in copilot_models.into_iter().enumerate() {
            configured.push(
                SelectorModelInfo::new(id, name, "copilot".to_string(), desc)
                    .with_context_window(ctx)
                    .with_costs(0.0, 0.0) // Free with Copilot subscription
                    .with_capabilities(vec!["free".to_string(), "api".to_string()])
                    .with_shortcut(if idx < 4 { idx + 1 } else { 0 }),
            );
        }
    }

    // Configured providers first, unconfigured after
    configured.extend(unconfigured);
    configured
}

/// Get provider info by provider type
///
/// # Arguments
/// * `provider_type` - The provider type identifier (e.g., "anthropic", "openai")
///
/// # Returns
/// Option of ProviderInfo
#[cfg(test)]
pub fn get_provider_by_type(provider_type: &str) -> Option<ProviderInfo> {
    get_available_providers()
        .into_iter()
        .find(|p| p.provider_type == provider_type)
}

/// Filter providers by search query
///
/// # Arguments
/// * `query` - Search query string
/// * `providers` - Slice of providers to filter
///
/// # Returns
/// Vector of providers matching the query
#[cfg(test)]
pub fn filter_providers<'a>(query: &str, providers: &'a [ProviderInfo]) -> Vec<&'a ProviderInfo> {
    let query_lower = query.to_lowercase();
    providers
        .iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query_lower)
                || p.provider_type.to_lowercase().contains(&query_lower)
                || p.description.to_lowercase().contains(&query_lower)
        })
        .collect()
}

/// OAuth flow state for provider authentication
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub struct OAuthFlowState {
    pub provider_name: Option<String>,
    pub auth_url: Option<String>,
    pub code_input: String,
    pub is_active: bool,
}

#[cfg(test)]
impl OAuthFlowState {
    /// Create a new inactive OAuth flow
    pub fn new() -> Self {
        Self {
            provider_name: None,
            auth_url: None,
            code_input: String::new(),
            is_active: false,
        }
    }

    /// Start OAuth flow for a provider
    pub fn start(&mut self, provider_name: String, auth_url: String) {
        self.provider_name = Some(provider_name);
        self.auth_url = Some(auth_url);
        self.code_input.clear();
        self.is_active = true;
    }

    /// Close OAuth flow
    pub fn close(&mut self) {
        self.provider_name = None;
        self.auth_url = None;
        self.code_input.clear();
        self.is_active = false;
    }

    /// Submit OAuth code
    pub fn submit_code(&mut self, code: String) -> Result<String, String> {
        if !self.is_active {
            return Err("No active OAuth flow".to_string());
        }

        if code.trim().is_empty() {
            return Err("OAuth code cannot be empty".to_string());
        }

        let provider = self.provider_name.as_deref().unwrap_or("Provider");
        let message = format!(
            "✓ OAuth code received for {}. Token exchange would happen here in a future update.",
            provider
        );

        self.code_input = code;
        Ok(message)
    }

    /// Get current auth URL
    pub fn auth_url(&self) -> Option<&String> {
        self.auth_url.as_ref()
    }

    /// Get provider name
    pub fn provider_name(&self) -> Option<&str> {
        self.provider_name.as_deref()
    }
}

#[cfg(test)]
impl Default for OAuthFlowState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provider configuration cache for storing configuration temporarily
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderConfigCache {
    pub provider_type: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
}

impl ProviderConfigCache {
    /// Create a new provider configuration cache
    pub fn new(
        provider_type: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider_type: provider_type.into(),
            api_key: api_key.into(),
            base_url: None,
            model: model.into(),
        }
    }

    /// Create with custom base URL
    #[cfg(test)]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info_new() {
        let provider = ProviderInfo::new(
            "Test Provider",
            "test",
            "Test description",
            "TEST_API_KEY",
            "test-model",
        );

        assert_eq!(provider.name, "Test Provider");
        assert_eq!(provider.provider_type, "test");
        assert_eq!(provider.api_key_env, "TEST_API_KEY");
        assert_eq!(provider.default_model, "test-model");
    }

    #[test]
    fn test_get_available_providers() {
        let providers = get_available_providers();

        assert!(!providers.is_empty());
        assert!(providers.len() >= 6);

        // Check that Anthropic is in the list
        assert!(providers.iter().any(|p| p.provider_type == "anthropic"));
        assert!(providers.iter().any(|p| p.provider_type == "openai"));
        assert!(providers.iter().any(|p| p.provider_type == "ollama"));
    }

    #[test]
    fn test_get_provider_by_type() {
        let anthropic = get_provider_by_type("anthropic");
        assert!(anthropic.is_some());
        assert_eq!(anthropic.unwrap().provider_type, "anthropic");

        let nonexistent = get_provider_by_type("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_filter_providers() {
        let providers = get_available_providers();

        // Filter by name
        let results = filter_providers("anthropic", &providers);
        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|p| p.provider_type.contains("anthropic")));

        // Filter by description
        let results = filter_providers("local", &providers);
        assert!(!results.is_empty());

        // Empty query returns all
        let results = filter_providers("", &providers);
        assert_eq!(results.len(), providers.len());
    }

    #[test]
    fn test_oauth_flow_state() {
        let mut flow = OAuthFlowState::new();

        assert!(!flow.is_active);
        assert!(flow.provider_name().is_none());

        flow.start(
            "TestProvider".to_string(),
            "https://example.com/auth".to_string(),
        );

        assert!(flow.is_active);
        assert_eq!(flow.provider_name(), Some("TestProvider"));
        assert_eq!(
            flow.auth_url(),
            Some(&"https://example.com/auth".to_string())
        );

        let result = flow.submit_code("test-code".to_string());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("TestProvider"));

        flow.close();

        assert!(!flow.is_active);
        assert!(flow.provider_name().is_none());
    }

    #[test]
    fn test_oauth_flow_submit_without_active_flow() {
        let mut flow = OAuthFlowState::new();
        let result = flow.submit_code("code".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_oauth_flow_submit_empty_code() {
        let mut flow = OAuthFlowState::new();
        flow.start("Test".to_string(), "https://example.com".to_string());
        let result = flow.submit_code("   ".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_config_cache() {
        let cache = ProviderConfigCache::new("anthropic", "test-api-key", "claude-3-5-sonnet");

        assert_eq!(cache.provider_type, "anthropic");
        assert_eq!(cache.api_key, "test-api-key");
        assert_eq!(cache.model, "claude-3-5-sonnet");
        assert!(cache.base_url.is_none());

        let cache_with_url = cache.with_base_url("https://api.example.com".to_string());
        assert_eq!(
            cache_with_url.base_url,
            Some("https://api.example.com".to_string())
        );
    }

    #[test]
    fn test_provider_info_get_api_key() {
        // Test with a provider that requires API key
        let provider = ProviderInfo::new(
            "Test",
            "test",
            "Description",
            "NONEXISTENT_KEY_12345",
            "model",
        );
        // Should return None because env var doesn't exist
        assert!(provider.get_api_key().is_none());

        // Test with Ollama (N/A)
        let provider = ProviderInfo::new("Ollama", "ollama", "Desc", "N/A", "model");
        assert!(provider.get_api_key().is_some());
        assert_eq!(provider.get_api_key().unwrap(), "no-api-key-required");
    }

    #[test]
    fn test_ollama_always_configured() {
        let providers = get_available_providers();
        let ollama = providers
            .iter()
            .find(|p| p.provider_type == "ollama")
            .unwrap();

        // Ollama should always be configured since it doesn't require an API key
        assert!(ollama.is_configured());
    }
}
