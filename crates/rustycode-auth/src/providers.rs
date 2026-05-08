//! Per-provider OAuth configuration registry.
//!
//! Defines `ProviderAuthConfig` for each supported LLM provider,
//! including OAuth endpoints, scopes, and client IDs.

use crate::oauth::OAuthConfig;

/// Authentication method a provider supports.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProviderAuthMethod {
    /// API key entered by user.
    ApiKey,
    /// OAuth 2.0 device flow (no browser callback needed).
    DeviceFlow,
    /// OAuth 2.0 authorization code flow with PKCE (browser callback).
    AuthorizationCode,
}

/// Configuration for a single authentication provider.
#[derive(Debug, Clone)]
pub struct ProviderAuthConfig {
    /// Unique provider identifier (e.g., "openai", "google", "copilot").
    pub id: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Supported authentication methods (in order of preference).
    pub auth_methods: &'static [ProviderAuthMethod],
    /// OAuth configuration, if the provider supports OAuth.
    pub oauth: Option<fn(redirect_url: &str) -> OAuthConfig>,
    /// Environment variable name for API key fallback.
    pub env_key: &'static str,
}

/// OpenAI provider configuration.
///
/// Uses the Codex OAuth flow ("Sign in with ChatGPT").
/// Client ID from the public Codex CLI.
pub fn openai_config(redirect_url: &str) -> OAuthConfig {
    OAuthConfig {
        client_id: "codex-cli".to_string(),
        client_secret: None,
        auth_url: "https://auth.openai.com/authorize".to_string(),
        token_url: "https://auth.openai.com/oauth/token".to_string(),
        redirect_url: redirect_url.to_string(),
        scopes: vec!["openid".to_string(), "offline_access".to_string()],
    }
}

/// Google/Gemini provider configuration.
///
/// Uses Google's OAuth 2.0 endpoints.
pub fn google_config(redirect_url: &str) -> OAuthConfig {
    OAuthConfig {
        client_id: "rustycode-gemini".to_string(),
        client_secret: None,
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        token_url: "https://oauth2.googleapis.com/token".to_string(),
        redirect_url: redirect_url.to_string(),
        scopes: vec![
            "openid".to_string(),
            "https://www.googleapis.com/auth/cloud-platform".to_string(),
        ],
    }
}

/// GitHub Copilot provider configuration.
///
/// Uses GitHub's device flow (not authorization code flow).
pub fn copilot_config(_redirect_url: &str) -> OAuthConfig {
    // Copilot uses device flow, but we provide a config for consistency.
    // The actual device flow uses GitHubCopilotAuth directly.
    OAuthConfig {
        client_id: "Iv1.b507a08c87ecfe98".to_string(),
        client_secret: None,
        auth_url: "https://github.com/login/device/code".to_string(),
        token_url: "https://github.com/login/oauth/access_token".to_string(),
        redirect_url: String::new(),
        scopes: vec![],
    }
}

/// All supported providers, in the order they should be displayed.
pub static PROVIDERS: &[ProviderAuthConfig] = &[
    ProviderAuthConfig {
        id: "openai",
        display_name: "OpenAI",
        auth_methods: &[
            ProviderAuthMethod::ApiKey,
            ProviderAuthMethod::AuthorizationCode,
        ],
        oauth: Some(openai_config),
        env_key: "OPENAI_API_KEY",
    },
    ProviderAuthConfig {
        id: "anthropic",
        display_name: "Anthropic",
        auth_methods: &[ProviderAuthMethod::ApiKey],
        oauth: None,
        env_key: "ANTHROPIC_API_KEY",
    },
    ProviderAuthConfig {
        id: "google",
        display_name: "Google Gemini",
        auth_methods: &[
            ProviderAuthMethod::ApiKey,
            ProviderAuthMethod::AuthorizationCode,
        ],
        oauth: Some(google_config),
        env_key: "GOOGLE_API_KEY",
    },
    ProviderAuthConfig {
        id: "copilot",
        display_name: "GitHub Copilot",
        auth_methods: &[ProviderAuthMethod::DeviceFlow],
        oauth: Some(copilot_config),
        env_key: "GITHUB_TOKEN",
    },
    ProviderAuthConfig {
        id: "openrouter",
        display_name: "OpenRouter",
        auth_methods: &[ProviderAuthMethod::ApiKey],
        oauth: None,
        env_key: "OPENROUTER_API_KEY",
    },
    ProviderAuthConfig {
        id: "ollama",
        display_name: "Ollama",
        auth_methods: &[ProviderAuthMethod::ApiKey],
        oauth: None,
        env_key: "OLLAMA_HOST",
    },
];

/// Look up a provider config by ID.
pub fn find_provider(id: &str) -> Option<&'static ProviderAuthConfig> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Get the API key for a provider from environment variables.
pub fn env_api_key(provider_id: &str) -> Option<String> {
    let provider = find_provider(provider_id)?;
    std::env::var(provider.env_key).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_defined() {
        assert!(!PROVIDERS.is_empty());
    }

    #[test]
    fn find_provider_returns_known() {
        assert!(find_provider("openai").is_some());
        assert!(find_provider("anthropic").is_some());
        assert!(find_provider("google").is_some());
        assert!(find_provider("copilot").is_some());
        assert!(find_provider("openrouter").is_some());
        assert!(find_provider("ollama").is_some());
    }

    #[test]
    fn find_provider_returns_none_for_unknown() {
        assert!(find_provider("nonexistent").is_none());
    }

    #[test]
    fn all_providers_have_ids() {
        for p in PROVIDERS {
            assert!(!p.id.is_empty(), "provider has empty id");
            assert!(
                !p.display_name.is_empty(),
                "provider {} has empty display name",
                p.id
            );
            assert!(
                !p.auth_methods.is_empty(),
                "provider {} has no auth methods",
                p.id
            );
            assert!(!p.env_key.is_empty(), "provider {} has empty env key", p.id);
        }
    }

    #[test]
    fn openai_oauth_config_has_required_fields() {
        let config = openai_config("http://localhost:9090/callback");
        assert_eq!(config.client_id, "codex-cli");
        assert!(config.client_secret.is_none());
        assert!(config.auth_url.contains("openai.com"));
        assert!(config.token_url.contains("openai.com"));
        assert_eq!(config.redirect_url, "http://localhost:9090/callback");
        assert!(!config.scopes.is_empty());
    }

    #[test]
    fn google_oauth_config_has_required_fields() {
        let config = google_config("http://localhost:9090/callback");
        assert!(config.auth_url.contains("google"));
        assert!(config.token_url.contains("google"));
        assert!(!config.scopes.is_empty());
    }

    #[test]
    fn copilot_oauth_config_has_client_id() {
        let config = copilot_config("http://localhost:9090/callback");
        assert!(!config.client_id.is_empty());
    }

    #[test]
    fn oauth_providers_have_config_function() {
        for p in PROVIDERS {
            let has_oauth = p.auth_methods.iter().any(|m| {
                matches!(
                    m,
                    ProviderAuthMethod::AuthorizationCode | ProviderAuthMethod::DeviceFlow
                )
            });
            if has_oauth {
                assert!(
                    p.oauth.is_some(),
                    "provider {} has OAuth method but no config",
                    p.id
                );
            }
        }
    }

    #[test]
    fn provider_ids_are_unique() {
        let mut ids = std::collections::HashSet::new();
        for p in PROVIDERS {
            assert!(ids.insert(p.id), "duplicate provider id: {}", p.id);
        }
    }

    #[test]
    fn provider_auth_method_equality() {
        assert_eq!(ProviderAuthMethod::ApiKey, ProviderAuthMethod::ApiKey);
        assert_ne!(ProviderAuthMethod::ApiKey, ProviderAuthMethod::DeviceFlow);
    }
}
