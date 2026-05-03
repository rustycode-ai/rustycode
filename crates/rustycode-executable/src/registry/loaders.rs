//! Unit loaders for populating the registry from various sources

use crate::{ExecutableError, ExecutableUnit};
use async_trait::async_trait;

/// Trait for loading `ExecutableUnits` from a source
#[async_trait]
pub trait UnitLoader: Send + Sync {
    /// Human-readable name for this loader
    fn name(&self) -> &str;

    /// Load all units from this source
    async fn load_units(&self) -> Result<Vec<ExecutableUnit>, ExecutableError>;

    /// Check if this source has been modified since last load
    async fn is_stale(&self) -> bool {
        false
    }

    /// Load a single unit by ID from this source.
    ///
    /// Default implementation scans all units and returns the first match.
    /// Loaders with more efficient lookup can override this.
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError> {
        let units = self.load_units().await?;
        units.into_iter().find(|u| u.id == id).ok_or_else(|| {
            ExecutableError::NotFound(format!("unit '{id}' not found in {}", self.name()))
        })
    }
}
