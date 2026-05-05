//! Lazy Tool Loading
//!
//! On-demand loading of tools to reduce startup time and memory footprint.
//! Tools are loaded only when first used and cached for subsequent accesses.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

/// Loading state of a tool
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolLoadState {
    /// Tool is registered but not yet loaded
    Registered,
    /// Tool is currently being loaded
    Loading,
    /// Tool is loaded and ready
    Loaded,
    /// Tool failed to load
    Failed(String),
}

/// Tool factory function for lazy initialization
pub type ToolFactory = Box<dyn Fn() -> Result<Arc<dyn ToolDelegate>> + Send + Sync>;

/// Delegate trait for lazy-loaded tools
pub trait ToolDelegate: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Check if the tool is expensive to initialize
    fn is_expensive(&self) -> bool;
}

/// Metadata for a tool before loading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazyToolMetadata {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Estimated initialization time in ms
    pub init_cost_ms: u64,
    /// Memory footprint estimate in bytes
    pub memory_estimate: usize,
    /// Whether the tool requires external resources
    pub requires_external: bool,
}

/// A lazy-loaded tool entry
struct LazyToolEntry {
    /// Tool metadata
    metadata: LazyToolMetadata,
    /// Current load state
    state: ToolLoadState,
    /// The loaded tool (if loaded)
    tool: Option<Arc<dyn ToolDelegate>>,
    /// Factory function for loading
    factory: Option<ToolFactory>,
    /// Time when loaded
    loaded_at: Option<Instant>,
    /// Time spent loading
    load_duration_ms: Option<u64>,
}

impl LazyToolEntry {
    /// Create a new lazy tool entry
    fn new(metadata: LazyToolMetadata, factory: ToolFactory) -> Self {
        Self {
            metadata,
            state: ToolLoadState::Registered,
            tool: None,
            factory: Some(factory),
            loaded_at: None,
            load_duration_ms: None,
        }
    }

    /// Load the tool using its factory
    fn load(&mut self) -> Result<()> {
        if matches!(self.state, ToolLoadState::Loaded) {
            return Ok(());
        }

        self.state = ToolLoadState::Loading;
        let start = Instant::now();

        let factory = self
            .factory
            .take()
            .ok_or_else(|| anyhow::anyhow!("Factory already consumed"))?;

        match factory() {
            Ok(tool) => {
                self.load_duration_ms = Some(start.elapsed().as_millis() as u64);
                self.loaded_at = Some(Instant::now());
                self.tool = Some(tool);
                self.state = ToolLoadState::Loaded;
                Ok(())
            }
            Err(e) => {
                self.state = ToolLoadState::Failed(e.to_string());
                Err(e)
            }
        }
    }

    /// Check if the tool is loaded
    fn is_loaded(&self) -> bool {
        matches!(self.state, ToolLoadState::Loaded)
    }

    /// Get the tool (loads if necessary)
    fn get(&mut self) -> Result<Arc<dyn ToolDelegate>> {
        if !self.is_loaded() {
            self.load()?;
        }

        self.tool
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Tool not loaded"))
    }

    /// Unload the tool to free memory
    fn unload(&mut self) {
        self.tool = None;
        self.state = ToolLoadState::Registered;
        self.loaded_at = None;
    }
}

/// Lazy tool loader registry
pub struct LazyToolLoader {
    /// Registered tools
    tools: RwLock<HashMap<String, LazyToolEntry>>,
    /// Configuration
    config: LoaderConfig,
}

/// Configuration for the lazy loader
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Maximum number of tools to keep loaded
    pub max_loaded: usize,
    /// Idle time before unloading (in seconds)
    pub idle_timeout_secs: u64,
    /// Preload commonly used tools
    pub preload_common: bool,
    /// Tools to always preload
    pub preload_list: Vec<String>,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            max_loaded: 50,
            idle_timeout_secs: 300, // 5 minutes
            preload_common: true,
            preload_list: vec![
                "read_file".to_string(),
                "write_file".to_string(),
                "bash".to_string(),
            ],
        }
    }
}

impl LazyToolLoader {
    pub fn new(config: LoaderConfig) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(LoaderConfig::default())
    }

    /// Register a tool for lazy loading
    pub fn register(&self, metadata: LazyToolMetadata, factory: ToolFactory) -> Result<()> {
        let tool_name = metadata.name.clone();
        let mut tools = self
            .tools
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        if tools.contains_key(&tool_name) {
            return Err(anyhow::anyhow!("Tool already registered: {tool_name}"));
        }

        let entry = LazyToolEntry::new(metadata, factory);
        tools.insert(tool_name, entry);

        Ok(())
    }

    /// Get a tool, loading it if necessary
    pub fn get(&self, name: &str) -> Result<Arc<dyn ToolDelegate>> {
        let mut tools = self
            .tools
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let entry = tools
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not registered: {name}"))?;

        entry.get()
    }

    /// Check if a tool is currently loaded
    pub fn is_loaded(&self, name: &str) -> bool {
        if let Ok(tools) = self.tools.read() {
            if let Some(entry) = tools.get(name) {
                return entry.is_loaded();
            }
        }
        false
    }

    /// Preload commonly used tools
    pub fn preload(&self) -> Result<()> {
        if !self.config.preload_common {
            return Ok(());
        }

        let tools_to_load = if self.config.preload_list.is_empty() {
            vec!["read_file".to_string(), "write_file".to_string()]
        } else {
            self.config.preload_list.clone()
        };

        for tool_name in tools_to_load {
            if let Ok(mut tools) = self.tools.write() {
                if let Some(entry) = tools.get_mut(&tool_name) {
                    let _ = entry.load();
                }
            }
        }

        Ok(())
    }

    /// Unload idle tools to free memory
    pub fn unload_idle(&self) -> Result<usize> {
        let mut tools = self
            .tools
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let now = Instant::now();
        let timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);
        let mut unloaded = 0;

        for entry in tools.values_mut() {
            if entry.is_loaded() {
                if let Some(loaded_at) = entry.loaded_at {
                    if now.duration_since(loaded_at) > timeout {
                        entry.unload();
                        unloaded += 1;
                    }
                }
            }
        }

        Ok(unloaded)
    }

    /// Get statistics about loaded tools
    pub fn stats(&self) -> LoaderStats {
        let tools = self
            .tools
            .read()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))
            .unwrap();

        let mut registered = 0;
        let mut loaded = 0;
        let mut loading = 0;
        let mut failed = 0;
        let mut total_load_time_ms = 0;
        let mut expensive_loaded = 0;

        for entry in tools.values() {
            registered += 1;
            match entry.state {
                ToolLoadState::Registered => {}
                ToolLoadState::Loading => loading += 1,
                ToolLoadState::Loaded => {
                    loaded += 1;
                    if entry.metadata.init_cost_ms > 100 {
                        expensive_loaded += 1;
                    }
                    total_load_time_ms += entry.load_duration_ms.unwrap_or(0);
                }
                ToolLoadState::Failed(_) => failed += 1,
            }
        }

        LoaderStats {
            registered,
            loaded,
            loading,
            failed,
            expensive_loaded,
            total_load_time_ms,
            avg_load_time_ms: if loaded > 0 {
                total_load_time_ms / loaded as u64
            } else {
                0
            },
        }
    }

    /// List all registered tools with their state
    pub fn list_tools(&self) -> Vec<(String, ToolLoadState, LazyToolMetadata)> {
        self.tools
            .read()
            .ok()
            .map(|tools| {
                tools
                    .iter()
                    .map(|(name, entry)| {
                        (name.clone(), entry.state.clone(), entry.metadata.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Force unload a specific tool
    pub fn unload_tool(&self, name: &str) -> Result<()> {
        let mut tools = self
            .tools
            .write()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        let entry = tools
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("Tool not registered: {name}"))?;

        entry.unload();
        Ok(())
    }
}

/// Statistics about the loader
#[derive(Debug, Clone)]
pub struct LoaderStats {
    /// Total registered tools
    pub registered: usize,
    /// Currently loaded tools
    pub loaded: usize,
    /// Currently loading
    pub loading: usize,
    /// Failed to load
    pub failed: usize,
    /// Number of expensive tools loaded
    pub expensive_loaded: usize,
    /// Total time spent loading tools
    pub total_load_time_ms: u64,
    /// Average load time per tool
    pub avg_load_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock tool delegate
    struct MockTool {
        name: String,
        expensive: bool,
    }

    impl ToolDelegate for MockTool {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "Mock tool"
        }

        fn is_expensive(&self) -> bool {
            self.expensive
        }
    }

    fn mock_factory(name: &'static str) -> ToolFactory {
        Box::new(move || {
            Ok(Arc::new(MockTool {
                name: name.to_string(),
                expensive: false,
            }))
        })
    }

    #[test]
    fn test_register_tool() {
        let loader = LazyToolLoader::default_config();

        let metadata = LazyToolMetadata {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            init_cost_ms: 10,
            memory_estimate: 1024,
            requires_external: false,
        };

        let result = loader.register(metadata.clone(), mock_factory("test_tool"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_duplicate_register_fails() {
        let loader = LazyToolLoader::default_config();

        let metadata = LazyToolMetadata {
            name: "test_tool".to_string(),
            description: "Test tool".to_string(),
            init_cost_ms: 10,
            memory_estimate: 1024,
            requires_external: false,
        };

        loader
            .register(metadata.clone(), mock_factory("test_tool"))
            .unwrap();
        let result = loader.register(metadata, mock_factory("test_tool"));
        assert!(result.is_err());
    }

    #[test]
    fn test_lazy_loading() {
        let loader = LazyToolLoader::default_config();

        let metadata = LazyToolMetadata {
            name: "lazy_tool".to_string(),
            description: "Lazy tool".to_string(),
            init_cost_ms: 50,
            memory_estimate: 2048,
            requires_external: false,
        };

        loader
            .register(metadata, mock_factory("lazy_tool"))
            .unwrap();

        // Tool should not be loaded initially
        assert!(!loader.is_loaded("lazy_tool"));

        // Getting the tool should load it
        let tool = loader.get("lazy_tool");
        assert!(tool.is_ok());
        assert!(loader.is_loaded("lazy_tool"));
    }

    #[test]
    fn test_stats() {
        let loader = LazyToolLoader::default_config();

        let metadata = LazyToolMetadata {
            name: "test_tool".to_string(),
            description: "Test".to_string(),
            init_cost_ms: 10,
            memory_estimate: 1024,
            requires_external: false,
        };

        loader
            .register(metadata, mock_factory("test_tool"))
            .unwrap();

        let stats = loader.stats();
        assert_eq!(stats.registered, 1);
        assert_eq!(stats.loaded, 0);

        // Trigger loading
        loader.get("test_tool").unwrap();

        let stats = loader.stats();
        assert_eq!(stats.loaded, 1);
    }

    #[test]
    fn test_unload_tool() {
        let loader = LazyToolLoader::default_config();

        let metadata = LazyToolMetadata {
            name: "test_tool".to_string(),
            description: "Test".to_string(),
            init_cost_ms: 10,
            memory_estimate: 1024,
            requires_external: false,
        };

        loader
            .register(metadata, mock_factory("test_tool"))
            .unwrap();

        // Load the tool
        loader.get("test_tool").unwrap();
        assert!(loader.is_loaded("test_tool"));

        // Unload it
        loader.unload_tool("test_tool").unwrap();
        assert!(!loader.is_loaded("test_tool"));
    }
}
