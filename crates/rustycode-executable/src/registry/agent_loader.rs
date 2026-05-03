//! Loader for `RustyCode` agents

use crate::registry::loaders::UnitLoader;
use crate::{
    AdvancedToolMetadata, ExecutableError, ExecutableUnit, ExecutionMode, UnitCapabilities,
    UnitSource,
};
use async_trait::async_trait;
use std::path::PathBuf;

/// Loads agents from the `RustyCode` agents directory
pub struct AgentLoader {
    agents_dir: PathBuf,
}

impl AgentLoader {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(agents_dir: PathBuf) -> Self {
        Self { agents_dir }
    }

    fn scan_agents(&self) -> Vec<ExecutableUnit> {
        let mut units = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&self.agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    || path
                        .extension()
                        .is_some_and(|ext| ext == "md" || ext == "yaml")
                {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    units.push(ExecutableUnit {
                        id: format!("agent:{name}"),
                        name: name.clone(),
                        description: format!("Agent: {name}"),
                        capabilities: UnitCapabilities {
                            can_execute_directly: true,
                            can_bundle_knowledge: true,
                            can_reason_autonomously: true,
                        },
                        advanced_metadata: AdvancedToolMetadata {
                            examples: vec![],
                            defer_loading: false,
                            search_hints: vec![name.clone(), "agent".to_string()],
                            execution_strategy: ExecutionMode::Autonomous,
                            result_processor: None,
                        },
                        handler: std::sync::Arc::new(crate::types::callable::NoOpCallable),
                        source: UnitSource::BundledAgent {
                            path: path.to_string_lossy().to_string(),
                        },
                        schema: None,
                        tags: vec!["agent".to_string()],
                        version: None,
                    });
                }
            }
        }

        units
    }
}

#[async_trait]
impl UnitLoader for AgentLoader {
    fn name(&self) -> &'static str {
        "agents"
    }

    async fn load_units(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(self.scan_agents())
    }
}
