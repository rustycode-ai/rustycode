pub mod agent_loader;
pub mod loaders;
pub mod native_tool_loader;
pub mod skill_loader;

// Re-export loader types for convenience
pub use agent_loader::AgentLoader;
pub use loaders::UnitLoader;
pub use native_tool_loader::NativeToolLoader;
pub use skill_loader::SkillLoader;

use crate::{ExecutableError, ExecutableUnit, UnitCapabilities};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct UnitMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub search_hints: Vec<String>,
    pub capabilities: UnitCapabilities,
    pub full_loaded: bool,
}

pub struct ExecutableRegistry {
    units: Arc<RwLock<HashMap<String, ExecutableUnit>>>,
    metadata_cache: Arc<RwLock<HashMap<String, UnitMetadata>>>,
}

impl ExecutableRegistry {
    pub fn new() -> Self {
        Self {
            units: Arc::new(RwLock::new(HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, unit: ExecutableUnit) -> Result<(), ExecutableError> {
        let id = unit.id.clone();
        let metadata = UnitMetadata {
            id: id.clone(),
            name: unit.name.clone(),
            description: unit.description.clone(),
            search_hints: unit.advanced_metadata.search_hints.clone(),
            capabilities: unit.capabilities.clone(),
            full_loaded: !unit.advanced_metadata.defer_loading,
        };

        {
            let mut units = futures::executor::block_on(self.units.write());

            if units.contains_key(&id) {
                return Err(ExecutableError::ExecutionFailed(format!(
                    "unit {id} already registered"
                )));
            }

            units.insert(id.clone(), unit);
        }

        {
            let mut metadata_cache = futures::executor::block_on(self.metadata_cache.write());
            metadata_cache.insert(id, metadata);
        }

        Ok(())
    }

    pub fn get_sync(&self, unit_id: &str) -> Option<ExecutableUnit> {
        let units = futures::executor::block_on(self.units.read());
        units.get(unit_id).cloned()
    }

    pub async fn get(&self, unit_id: &str) -> Option<ExecutableUnit> {
        let units = self.units.read().await;
        units.get(unit_id).cloned()
    }

    pub async fn list_metadata(&self) -> Vec<UnitMetadata> {
        let cache = self.metadata_cache.read().await;
        cache.values().cloned().collect()
    }

    /// Register all units loaded by the given loader.
    ///
    /// Returns an error on the first duplicate registration.
    pub async fn register_from_loader(
        &self,
        loader: &dyn crate::registry::loaders::UnitLoader,
    ) -> Result<(), ExecutableError> {
        let units = loader.load_units().await?;
        for unit in units {
            let id = unit.id.clone();
            let metadata = UnitMetadata {
                id: id.clone(),
                name: unit.name.clone(),
                description: unit.description.clone(),
                search_hints: unit.advanced_metadata.search_hints.clone(),
                capabilities: unit.capabilities.clone(),
                full_loaded: !unit.advanced_metadata.defer_loading,
            };

            {
                let mut map = self.units.write().await;
                if map.contains_key(&id) {
                    return Err(ExecutableError::ExecutionFailed(format!(
                        "unit {id} already registered"
                    )));
                }
                map.insert(id.clone(), unit);
            }

            {
                let mut cache = self.metadata_cache.write().await;
                cache.insert(id, metadata);
            }
        }
        Ok(())
    }

    pub async fn discover(
        &self,
        query: &str,
        _context: Option<crate::ExecutionContext>,
    ) -> Vec<UnitMetadata> {
        let metadata = self.list_metadata().await;
        let query_lower = query.to_lowercase();

        metadata
            .into_iter()
            .filter(|m| {
                m.name.to_lowercase().contains(&query_lower)
                    || m.description.to_lowercase().contains(&query_lower)
                    || m.search_hints
                        .iter()
                        .any(|hint| hint.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

impl Default for ExecutableRegistry {
    fn default() -> Self {
        Self::new()
    }
}
