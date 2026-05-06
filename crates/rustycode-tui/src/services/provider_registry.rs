//! Provider Registry Service
//!
//! Encapsulates LLM provider configuration and instantiation.

use anyhow::{Context, Result};
use rustycode_llm::provider::LLMProvider;
use std::sync::Arc;

pub struct ProviderRegistry;

impl ProviderRegistry {
    pub fn new() -> Self {
        Self
    }

    /// Create an LLM provider based on environment configuration with a fallback.
    pub fn create_llm_provider(&self) -> Result<(Arc<dyn LLMProvider>, String)> {
        let (provider_type, model, v2_config) = rustycode_llm::load_provider_config_from_env()
            .unwrap_or_else(|_| {
                (
                    "anthropic".to_string(),
                    "claude-3-5-sonnet-20241022".to_string(),
                    Default::default(),
                )
            });

        let provider =
            rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config)
                .or_else(|_| {
                    rustycode_llm::create_provider("anthropic", "claude-3-5-sonnet-20241022")
                })
                .context("No LLM provider available. Set ANTHROPIC_API_KEY or configure a provider with /provider.")?;

        Ok((provider, model))
    }
}
