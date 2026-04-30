//! Registry for model capabilities, allowing dynamic selection based on
//! required features (thinking, tools, auth) rather than just name/cost.

use crate::config::{ModelConfig, OrchestrationConfig};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    ExtendedThinking,
    ToolCalling,
    Streaming,
    AuthRequired,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub config: ModelConfig,
    pub capabilities: Vec<Capability>,
}

pub struct ModelRegistry {
    registry: HashMap<String, ModelInfo>,
}

impl ModelRegistry {
    pub fn new(config: &OrchestrationConfig) -> Self {
        let mut registry = HashMap::new();
        for configs in config.models.values() {
            for cfg in configs {
                let mut capabilities = Vec::new();
                if cfg.supports_extended_thinking.unwrap_or(false) {
                    capabilities.push(Capability::ExtendedThinking);
                }

                // Static capability inference
                capabilities.push(Capability::ToolCalling);
                capabilities.push(Capability::Streaming);

                registry.insert(
                    cfg.name.clone(),
                    ModelInfo {
                        config: cfg.clone(),
                        capabilities,
                    },
                );
            }
        }
        Self { registry }
    }

    pub fn select_best(
        &self,
        _target_tier: u8,
        required_capabilities: &[Capability],
    ) -> Option<&ModelInfo> {
        self.registry
            .values()
            .filter(|info| {
                required_capabilities
                    .iter()
                    .all(|c| info.capabilities.contains(c))
            })
            // Simple selection logic: Cheapest model that meets requirements
            .min_by(|a, b| {
                a.config
                    .cost_per_1m_tokens_input
                    .partial_cmp(&b.config.cost_per_1m_tokens_input)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}
