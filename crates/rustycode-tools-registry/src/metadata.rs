//! Tool metadata management

use crate::registry::ToolMetadata;

/// Trait for providing tool metadata
pub trait MetadataProvider {
    /// Get metadata for a tool
    fn get_metadata(&self, name: &str) -> Option<ToolMetadata>;

    /// List all available metadata
    fn list_metadata(&self) -> Vec<ToolMetadata>;
}

/// Default metadata provider implementation
pub struct DefaultMetadataProvider;

impl Default for DefaultMetadataProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultMetadataProvider {
    /// Create a new default metadata provider
    pub const fn new() -> Self {
        Self
    }
}

impl MetadataProvider for DefaultMetadataProvider {
    fn get_metadata(&self, _name: &str) -> Option<ToolMetadata> {
        None
    }

    fn list_metadata(&self) -> Vec<ToolMetadata> {
        vec![]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_err_used)]
mod tests {
    use super::*;

    #[test]
    fn default_provider_returns_none_for_any_name() {
        let provider = DefaultMetadataProvider::new();
        assert!(provider.get_metadata("bash").is_none());
        assert!(provider.get_metadata("").is_none());
    }

    #[test]
    fn default_provider_lists_empty() {
        let provider = DefaultMetadataProvider::new();
        assert!(provider.list_metadata().is_empty());
    }

    #[test]
    fn default_provider_default_trait() {
        let provider: DefaultMetadataProvider = DefaultMetadataProvider;
        assert!(provider.get_metadata("anything").is_none());
    }

    #[test]
    fn metadata_provider_trait_obj() {
        let provider: Box<dyn MetadataProvider> = Box::new(DefaultMetadataProvider::new());
        assert!(provider.get_metadata("tool").is_none());
        assert!(provider.list_metadata().is_empty());
    }
}
