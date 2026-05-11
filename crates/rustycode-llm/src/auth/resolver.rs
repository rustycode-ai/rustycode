//! Credential resolution logic (env → config → keyring → prompt).

use anyhow::Result;
use secrecy::SecretString;
use std::collections::HashMap;

use super::{ApiKeyHeaderAuth, AuthMethod, BearerAuth, NoAuth};

pub struct AuthResolver;

impl AuthResolver {
    /// Resolve an Auth implementation based on provider and config.
    pub fn resolve(
        provider_id: &str,
        config_api_key: Option<SecretString>,
        env_vars: &HashMap<String, String>,
    ) -> Result<Box<dyn AuthMethod>> {
        // 1. Check config-provided API key
        if let Some(key) = config_api_key {
            return Ok(Self::create_auth_for_provider(provider_id, key));
        }

        // 2. Check environment variables
        let env_key = match provider_id {
            "anthropic" => env_vars.get("ANTHROPIC_API_KEY"),
            "openai" => env_vars.get("OPENAI_API_KEY"),
            "gemini" => env_vars.get("GOOGLE_API_KEY"),
            "cohere" => env_vars.get("COHERE_API_KEY"),
            "bedrock" => env_vars.get("AWS_ACCESS_KEY_ID"), // Simplified
            _ => None,
        };

        if let Some(key) = env_key {
            return Ok(Self::create_auth_for_provider(
                provider_id,
                SecretString::from(key.clone()),
            ));
        }

        // 3. Fallback to NoAuth (e.g. for local LiteRT)
        Ok(Box::new(NoAuth))
    }

    fn create_auth_for_provider(provider_id: &str, key: SecretString) -> Box<dyn AuthMethod> {
        match provider_id {
            "anthropic" => Box::new(ApiKeyHeaderAuth::new("x-api-key", key)),
            "gemini" => Box::new(ApiKeyHeaderAuth::new("x-goog-api-key", key)),
            "bedrock" => Box::new(ApiKeyHeaderAuth::new("x-api-key", key)), // Proxied Bedrock
            _ => Box::new(BearerAuth::new(key)), // OpenAI, Cohere, etc use Bearer
        }
    }
}
