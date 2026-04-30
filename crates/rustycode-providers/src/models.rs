//! Provider and model metadata structures
//!
//! This module defines the core data structures for provider and model information.

use serde::{Deserialize, Serialize};

/// Authentication method for providers
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum AuthMethod {
    /// API key authentication (simple key in header)
    #[default]
    ApiKey,
    /// OAuth 2.0 authentication
    OAuth {
        /// OAuth authorization URL
        auth_url: String,
        /// OAuth token URL
        token_url: String,
        /// OAuth scopes required
        scopes: Vec<String>,
    },
}

/// Metadata about an LLM provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Unique provider identifier (e.g., "anthropic", "openai")
    pub id: String,

    /// Human-readable provider name
    pub name: String,

    /// Base API URL
    pub base_url: String,

    /// Environment variable name for API key (for ApiKey auth)
    pub api_key_env: String,

    /// Authentication method
    #[serde(default)]
    pub auth_method: AuthMethod,

    /// Provider capabilities
    pub capabilities: ProviderCapabilities,

    /// Pricing information (default for provider)
    pub pricing: super::PricingInfo,
}

/// Provider capabilities and limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Whether streaming is supported
    pub supports_streaming: bool,

    /// Whether function/tool calling is supported
    pub supports_function_calling: bool,

    /// Whether vision/image input is supported
    pub supports_vision: bool,

    /// Maximum output tokens
    pub max_tokens: u32,

    /// Maximum context window
    pub max_context_window: usize,
}

/// Metadata about a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g., "claude-3-5-sonnet", "gpt-4o")
    pub id: String,

    /// Human-readable model name
    pub name: String,

    /// Provider this model belongs to
    pub provider_id: String,

    /// Model description
    pub description: String,

    /// Context window size
    pub context_window: usize,

    /// Whether this model supports tool calling
    pub supports_tools: bool,

    /// Whether this model supports vision
    pub supports_vision: bool,

    /// Maximum output tokens
    pub max_tokens: u32,

    /// Input cost per 1k tokens
    pub input_cost_per_1k: f64,

    /// Output cost per 1k tokens
    pub output_cost_per_1k: f64,

    /// Recommended use cases
    pub use_cases: Vec<String>,

    /// Cost tier (1=free, 2=low, 3=medium, 4=high, 5=premium)
    pub cost_tier: u8,
}

impl ModelInfo {
    /// Calculate cost for a given number of tokens
    pub fn calculate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let input_cost = (input_tokens as f64 / 1000.0) * self.input_cost_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * self.output_cost_per_1k;
        input_cost + output_cost
    }

    /// Get full model identifier (provider/model)
    pub fn full_id(&self) -> String {
        format!("{}/{}", self.provider_id, self.id)
    }

    /// Check if model is free
    pub fn is_free(&self) -> bool {
        self.input_cost_per_1k == 0.0 && self.output_cost_per_1k == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info() {
        let model = ModelInfo {
            id: "claude-3-5-sonnet".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            provider_id: "anthropic".to_string(),
            description: "Most capable model".to_string(),
            context_window: 200_000,
            supports_tools: true,
            supports_vision: true,
            max_tokens: 8192,
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
            use_cases: vec!["Complex reasoning".to_string()],
            cost_tier: 4,
        };

        assert_eq!(model.id, "claude-3-5-sonnet");
        assert_eq!(model.full_id(), "anthropic/claude-3-5-sonnet");
        assert!(!model.is_free());

        let cost = model.calculate_cost(1000, 500);
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_free_model() {
        let model = ModelInfo {
            id: "llama-3-8b:free".to_string(),
            name: "Llama 3 8B (Free)".to_string(),
            provider_id: "openrouter".to_string(),
            description: "Free model".to_string(),
            context_window: 8192,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 4096,
            input_cost_per_1k: 0.0,
            output_cost_per_1k: 0.0,
            use_cases: vec!["Testing".to_string()],
            cost_tier: 1,
        };

        assert!(model.is_free());
        assert_eq!(model.calculate_cost(1000, 1000), 0.0);
    }

    #[test]
    fn test_provider_capabilities() {
        let caps = ProviderCapabilities {
            supports_streaming: true,
            supports_function_calling: true,
            supports_vision: true,
            max_tokens: 8192,
            max_context_window: 200_000,
        };

        assert!(caps.supports_streaming);
        assert!(caps.supports_function_calling);
        assert!(caps.supports_vision);
        assert_eq!(caps.max_tokens, 8192);
        assert_eq!(caps.max_context_window, 200_000);
    }

    #[test]
    fn test_auth_method_default_is_api_key() {
        let method = AuthMethod::default();
        assert!(matches!(method, AuthMethod::ApiKey));
    }

    #[test]
    fn test_auth_method_api_key_serde() {
        let method = AuthMethod::ApiKey;
        let json = serde_json::to_string(&method).unwrap();
        let decoded: AuthMethod = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, AuthMethod::ApiKey));
    }

    #[test]
    fn test_auth_method_oauth_serde() {
        let method = AuthMethod::OAuth {
            auth_url: "https://auth.example.com".to_string(),
            token_url: "https://token.example.com".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        let json = serde_json::to_string(&method).unwrap();
        let decoded: AuthMethod = serde_json::from_str(&json).unwrap();
        match decoded {
            AuthMethod::OAuth { scopes, .. } => assert_eq!(scopes.len(), 2),
            _ => panic!("Expected OAuth variant"),
        }
    }

    #[test]
    fn test_provider_metadata_roundtrip() {
        let meta = ProviderMetadata {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            base_url: "https://api.test.com/v1".to_string(),
            api_key_env: "TEST_API_KEY".to_string(),
            auth_method: AuthMethod::ApiKey,
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: false,
                supports_vision: false,
                max_tokens: 4096,
                max_context_window: 8192,
            },
            pricing: crate::PricingInfo {
                input_cost_per_1k: 0.001,
                output_cost_per_1k: 0.002,
                currency: crate::Currency::Usd,
            },
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: ProviderMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "test-provider");
        assert_eq!(decoded.base_url, "https://api.test.com/v1");
        assert!(decoded.capabilities.supports_streaming);
    }

    #[test]
    fn test_model_info_full_id_format() {
        let model = ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider_id: "openai".to_string(),
            description: "Test".to_string(),
            context_window: 128_000,
            supports_tools: true,
            supports_vision: true,
            max_tokens: 4096,
            input_cost_per_1k: 0.005,
            output_cost_per_1k: 0.015,
            use_cases: vec![],
            cost_tier: 4,
        };
        assert_eq!(model.full_id(), "openai/gpt-4o");
    }

    #[test]
    fn test_model_info_zero_tokens_cost() {
        let model = ModelInfo {
            id: "test".to_string(),
            name: "Test".to_string(),
            provider_id: "test".to_string(),
            description: "Test".to_string(),
            context_window: 4096,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 1024,
            input_cost_per_1k: 0.01,
            output_cost_per_1k: 0.03,
            use_cases: vec![],
            cost_tier: 2,
        };
        assert_eq!(model.calculate_cost(0, 0), 0.0);
    }

    #[test]
    fn test_provider_capabilities_no_streaming() {
        let caps = ProviderCapabilities {
            supports_streaming: false,
            supports_function_calling: true,
            supports_vision: false,
            max_tokens: 2048,
            max_context_window: 4096,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let decoded: ProviderCapabilities = serde_json::from_str(&json).unwrap();
        assert!(!decoded.supports_streaming);
        assert!(decoded.supports_function_calling);
        assert_eq!(decoded.max_tokens, 2048);
    }

    #[test]
    fn test_model_info_serde_roundtrip() {
        let model = ModelInfo {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            provider_id: "test-provider".to_string(),
            description: "A model for testing".to_string(),
            context_window: 128_000,
            supports_tools: true,
            supports_vision: false,
            max_tokens: 4096,
            input_cost_per_1k: 0.005,
            output_cost_per_1k: 0.015,
            use_cases: vec!["coding".to_string(), "analysis".to_string()],
            cost_tier: 3,
        };
        let json = serde_json::to_string(&model).unwrap();
        let decoded: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "test-model");
        assert_eq!(decoded.provider_id, "test-provider");
        assert_eq!(decoded.context_window, 128_000);
        assert!(decoded.supports_tools);
        assert!(!decoded.supports_vision);
        assert_eq!(decoded.use_cases.len(), 2);
        assert_eq!(decoded.cost_tier, 3);
    }

    #[test]
    fn test_auth_method_oauth_fields() {
        let oauth = AuthMethod::OAuth {
            auth_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            scopes: vec!["openid".to_string(), "profile".to_string()],
        };
        if let AuthMethod::OAuth {
            auth_url,
            token_url,
            scopes,
        } = oauth
        {
            assert_eq!(auth_url, "https://auth.example.com/authorize");
            assert_eq!(token_url, "https://auth.example.com/token");
            assert_eq!(scopes.len(), 2);
            assert!(scopes.contains(&"openid".to_string()));
        } else {
            panic!("Expected OAuth variant");
        }
    }

    #[test]
    fn test_auth_method_oauth_empty_scopes() {
        let oauth = AuthMethod::OAuth {
            auth_url: "https://auth.test.com".to_string(),
            token_url: "https://token.test.com".to_string(),
            scopes: vec![],
        };
        let json = serde_json::to_string(&oauth).unwrap();
        let decoded: AuthMethod = serde_json::from_str(&json).unwrap();
        if let AuthMethod::OAuth { scopes, .. } = decoded {
            assert!(scopes.is_empty());
        } else {
            panic!("Expected OAuth variant");
        }
    }

    #[test]
    fn test_model_info_calculate_cost_large() {
        let model = ModelInfo {
            id: "expensive".to_string(),
            name: "Expensive".to_string(),
            provider_id: "test".to_string(),
            description: "test".to_string(),
            context_window: 4096,
            supports_tools: false,
            supports_vision: false,
            max_tokens: 1024,
            input_cost_per_1k: 0.01,
            output_cost_per_1k: 0.03,
            use_cases: vec![],
            cost_tier: 5,
        };
        // 10k input, 5k output
        let cost = model.calculate_cost(10_000, 5_000);
        // (10000/1000)*0.01 + (5000/1000)*0.03 = 0.1 + 0.15 = 0.25
        assert!((cost - 0.25).abs() < 0.0001);
    }

    #[test]
    fn test_provider_metadata_serde_with_oauth() {
        let meta = ProviderMetadata {
            id: "oauth-provider".to_string(),
            name: "OAuth Provider".to_string(),
            base_url: "https://api.oauth.com".to_string(),
            api_key_env: "".to_string(),
            auth_method: AuthMethod::OAuth {
                auth_url: "https://auth.oauth.com".to_string(),
                token_url: "https://token.oauth.com".to_string(),
                scopes: vec!["read".to_string()],
            },
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_function_calling: true,
                supports_vision: true,
                max_tokens: 8192,
                max_context_window: 200_000,
            },
            pricing: crate::PricingInfo {
                input_cost_per_1k: 0.003,
                output_cost_per_1k: 0.015,
                currency: crate::Currency::Usd,
            },
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: ProviderMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "oauth-provider");
        assert!(decoded.capabilities.supports_vision);
        if let AuthMethod::OAuth { scopes, .. } = decoded.auth_method {
            assert_eq!(scopes.len(), 1);
        } else {
            panic!("Expected OAuth variant");
        }
    }
}
