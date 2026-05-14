//! Provider context for agent sessions.

/// Provider and configuration context for an agent run.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderContext {
    /// Provider name (e.g. "anthropic", "openai", "bedrock").
    pub provider_name: String,
    /// Model name/identifier.
    pub model: String,
    /// Provider auth key (redacted in logs).
    pub auth_key: String,
    /// Rate limit settings.
    pub rate_limit_settings: RateLimitSettings,
}

impl ProviderContext {
    /// Create a new provider context.
    pub fn new(
        provider_name: impl Into<String>,
        model: impl Into<String>,
        auth_key: impl Into<String>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            model: model.into(),
            auth_key: auth_key.into(),
            rate_limit_settings: RateLimitSettings::default(),
        }
    }

    /// Set custom rate limits.
    pub fn with_rate_limits(mut self, rpm: u64, tpm: u64) -> Self {
        self.rate_limit_settings = RateLimitSettings { rpm, tpm };
        self
    }
}

/// Rate limit configuration for a provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RateLimitSettings {
    /// Requests per minute.
    pub rpm: u64,
    /// Tokens per minute.
    pub tpm: u64,
}

impl RateLimitSettings {
    /// Default RPM (60 requests/min).
    pub const DEFAULT_RPM: u64 = 60;
    /// Default TPM (100K tokens/min).
    pub const DEFAULT_TPM: u64 = 100_000;
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            rpm: Self::DEFAULT_RPM,
            tpm: Self::DEFAULT_TPM,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn new_provider_context() {
        let ctx = ProviderContext::new("anthropic", "claude-sonnet-4-6", "sk-test");
        assert_eq!(ctx.provider_name, "anthropic");
        assert_eq!(ctx.model, "claude-sonnet-4-6");
        assert_eq!(ctx.auth_key, "sk-test");
        assert_eq!(ctx.rate_limit_settings.rpm, 60);
        assert_eq!(ctx.rate_limit_settings.tpm, 100_000);
    }

    #[test]
    fn custom_rate_limits() {
        let ctx = ProviderContext::new("openai", "gpt-4", "key").with_rate_limits(120, 200_000);
        assert_eq!(ctx.rate_limit_settings.rpm, 120);
        assert_eq!(ctx.rate_limit_settings.tpm, 200_000);
    }

    #[test]
    fn default_values() {
        let ctx = ProviderContext::default();
        assert!(ctx.provider_name.is_empty());
        assert!(ctx.model.is_empty());
        assert!(ctx.auth_key.is_empty());
    }

    #[test]
    fn serialization_round_trip() {
        let ctx = ProviderContext::new("bedrock", "claude-opus-4-7", "secret")
            .with_rate_limits(30, 50_000);
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: ProviderContext = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider_name, "bedrock");
        assert_eq!(parsed.model, "claude-opus-4-7");
        assert_eq!(parsed.rate_limit_settings.rpm, 30);
    }
}
