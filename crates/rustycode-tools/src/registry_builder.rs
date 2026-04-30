//! Tool provider trait for modular registry building

use crate::ToolRegistry;
use anyhow::Result;

/// A provider of a set of tools
pub trait ToolProvider: Send + Sync {
    /// Register tools to the provided registry
    fn register_tools(&self, registry: &mut ToolRegistry) -> Result<()>;
}

/// Builder for creating context-aware tool registries
pub struct RegistryBuilder {
    registry: ToolRegistry,
}

impl Default for RegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::new(),
        }
    }

    pub fn with_provider(mut self, provider: Box<dyn ToolProvider>) -> Result<Self> {
        provider.register_tools(&mut self.registry)?;
        Ok(self)
    }

    pub fn build(self) -> ToolRegistry {
        self.registry
    }
}
