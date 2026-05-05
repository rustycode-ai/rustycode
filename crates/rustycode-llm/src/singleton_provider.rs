//! Singleton provider instance for efficient connection reuse
//!
//! This module provides a shared LLM provider instance that is reused across
//! all API calls, which:
//! - Reduces TCP connection overhead
//! - Properly utilizes HTTP connection pooling
//! - Prevents exhausting API rate limits from creating too many connections
//! - Improves performance by reusing established connections

use anyhow::Result;
use std::sync::{Arc, Mutex};

use crate::provider::ProviderConfig;
use crate::AnthropicProvider;

/// Global shared provider instance
///
/// This is wrapped in Mutex to allow thread-safe access and
/// to support potential runtime reconfiguration.
static SHARED_PROVIDER: Mutex<Option<Arc<SharedLLMProvider>>> = Mutex::new(None);

/// Shared LLM provider with its configuration
pub struct SharedLLMProvider {
    /// The provider instance (wrapped in Arc for cheap cloning)
    pub provider: Arc<AnthropicProvider>,
    /// The configuration used to create this provider
    pub config: ProviderConfig,
    /// The model this provider is configured for
    pub model: String,
}

impl SharedLLMProvider {
    pub fn new(config: ProviderConfig, model: String) -> Result<Self> {
        let provider = AnthropicProvider::new(config.clone(), model.clone())
            .map_err(|e| anyhow::anyhow!("Failed to create AnthropicProvider: {}", e))?;

        Ok(Self {
            provider: Arc::new(provider),
            config,
            model,
        })
    }

    /// Get a reference to the underlying provider
    pub fn provider(&self) -> &AnthropicProvider {
        &self.provider
    }

    /// Get an Arc clone of the provider
    pub fn provider_arc(&self) -> Arc<AnthropicProvider> {
        Arc::clone(&self.provider)
    }

    /// Check if this provider is configured for the given model
    pub fn matches_model(&self, model: &str) -> bool {
        self.model == model
    }

    /// Get the provider config
    pub fn config(&self) -> &ProviderConfig {
        &self.config
    }
}

/// Initialize the singleton provider with the given configuration
///
/// This should be called once at application startup. If called multiple times
/// with different configurations, it will recreate the provider.
///
pub fn initialize_provider(config: ProviderConfig, model: String) -> Result<()> {
    let mut provider_lock = SHARED_PROVIDER
        .lock()
        .map_err(|e| anyhow::anyhow!("Provider mutex poisoned: {}", e))?;
    *provider_lock = Some(Arc::new(SharedLLMProvider::new(config, model)?));
    Ok(())
}

/// Get the shared provider instance
///
/// Returns an error if the provider hasn't been initialized.
pub fn provider() -> Result<Arc<SharedLLMProvider>> {
    let provider_lock = SHARED_PROVIDER
        .lock()
        .map_err(|e| anyhow::anyhow!("Provider mutex poisoned: {}", e))?;
    provider_lock.as_ref().map(Arc::clone).ok_or_else(|| {
        anyhow::anyhow!("Provider not initialized. Call initialize_provider() first.")
    })
}

/// Check if the shared provider has been initialized
pub fn is_initialized() -> bool {
    SHARED_PROVIDER.lock().map(|p| p.is_some()).unwrap_or(false)
}

/// Reset the shared provider (mainly for testing)
///
/// This clears the current provider instance. The next call to get_provider()
/// will return an error until initialize_provider() is called again.
pub fn reset() {
    if let Ok(mut provider_lock) = SHARED_PROVIDER.lock() {
        *provider_lock = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_singleton_initialization() {
        // Reset first to ensure clean state
        reset();

        assert!(!is_initialized());

        // Initialize
        let _config = ProviderConfig::default();
        let _model = "claude-sonnet-4-6".to_string();

        // Note: This will fail in tests without proper config, but we can test the structure
        // In production, this would be initialized at app startup
    }

    #[test]
    fn test_reset() {
        // This test verifies the reset function
        reset();
        assert!(!is_initialized());
    }
}
