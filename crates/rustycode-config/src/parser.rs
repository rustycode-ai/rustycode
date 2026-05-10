#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
/// Get the environment variable name for a provider's API key
pub fn api_key_env_name(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "openai" => "OPENAI_API_KEY".to_string(),
        "anthropic" => "ANTHROPIC_API_KEY".to_string(),
        "openrouter" => "OPENROUTER_API_KEY".to_string(),
        "nvidia" => "NVIDIA_API_KEY".to_string(),
        "copilot" | "github" => "GITHUB_TOKEN".to_string(),
        "litert-lm" | "litert_lm" | "litert" => "N/A".to_string(),
        _ => format!("{}_API_KEY", provider.to_uppercase().replace('-', "_")),
    }
}

/// Resolve an API key from environment variables for the given provider.
///
/// Checks the primary env var first, then provider-specific aliases.
/// For Anthropic: `ANTHROPIC_API_KEY` → `ANTHROPIC_AUTH_TOKEN`
pub fn resolve_api_key_from_env(provider: &str) -> Option<String> {
    let primary = api_key_env_name(provider);
    if let Ok(key) = std::env::var(&primary) {
        if !key.is_empty() {
            return Some(key);
        }
    }

    // Provider-specific fallbacks
    match provider.to_lowercase().as_str() {
        "anthropic" => std::env::var("ANTHROPIC_AUTH_TOKEN")
            .ok()
            .filter(|k| !k.is_empty()),
        _ => None,
    }
}

/// Get the default model for a provider
pub fn default_model_for_provider(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        // Keep legacy mappings expected by backward-compat tests
        "anthropic" => "claude-sonnet-4-6".to_string(),
        "google" => "gemini-pro".to_string(),
        "github" | "copilot" => "gpt-4o-copilot".to_string(),
        "ollama" => "llama3".to_string(),
        "litert-lm" | "litert_lm" | "litert" => "gemma-3n-e4b".to_string(),
        // OpenRouter default - use the generic free tier
        "openrouter" => "openrouter/free".to_string(),
        "nvidia" => "meta/llama-4-maverick-17b-128e-instruct".to_string(),
        // Default to gpt-4o for openai and unknown providers for backward compatibility
        _ => "gpt-4o".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_env_name_known_providers() {
        assert_eq!(api_key_env_name("openai"), "OPENAI_API_KEY");
        assert_eq!(api_key_env_name("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(api_key_env_name("openrouter"), "OPENROUTER_API_KEY");
        assert_eq!(api_key_env_name("copilot"), "GITHUB_TOKEN");
        assert_eq!(api_key_env_name("litert-lm"), "N/A");
    }

    #[test]
    fn test_api_key_env_name_custom_provider() {
        assert_eq!(api_key_env_name("custom"), "CUSTOM_API_KEY");
        assert_eq!(api_key_env_name("my-provider"), "MY_PROVIDER_API_KEY");
        assert_eq!(api_key_env_name("DeepSeek"), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn test_api_key_env_name_case_insensitive() {
        assert_eq!(api_key_env_name("OPENAI"), "OPENAI_API_KEY");
        assert_eq!(api_key_env_name("Anthropic"), "ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_resolve_api_key_fallback_to_auth_token() {
        // When ANTHROPIC_API_KEY is not set but ANTHROPIC_AUTH_TOKEN is,
        // resolve_api_key_from_env should return the token value.
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "test-token-123");
        assert_eq!(
            resolve_api_key_from_env("anthropic"),
            Some("test-token-123".to_string())
        );
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn test_resolve_api_key_prefers_primary() {
        // ANTHROPIC_API_KEY takes precedence over ANTHROPIC_AUTH_TOKEN
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
        std::env::set_var("ANTHROPIC_API_KEY", "primary-key");
        std::env::set_var("ANTHROPIC_AUTH_TOKEN", "fallback-token");
        assert_eq!(
            resolve_api_key_from_env("anthropic"),
            Some("primary-key".to_string())
        );
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn test_default_model_for_provider_known() {
        assert_eq!(default_model_for_provider("openai"), "gpt-4o");
        assert_eq!(default_model_for_provider("google"), "gemini-pro");
        assert_eq!(default_model_for_provider("github"), "gpt-4o-copilot");
        assert_eq!(default_model_for_provider("copilot"), "gpt-4o-copilot");
        assert_eq!(default_model_for_provider("ollama"), "llama3");
        assert_eq!(default_model_for_provider("litert-lm"), "gemma-3n-e4b");
        assert_eq!(default_model_for_provider("openrouter"), "openrouter/free");
    }

    #[test]
    fn test_default_model_for_provider_unknown() {
        assert_eq!(default_model_for_provider("unknown"), "gpt-4o");
        assert_eq!(default_model_for_provider("custom"), "gpt-4o");
    }

    #[test]
    fn test_default_model_for_provider_case_insensitive() {
        assert_eq!(default_model_for_provider("OPENAI"), "gpt-4o");
        assert_eq!(default_model_for_provider("OpenAI"), "gpt-4o");
    }
}
