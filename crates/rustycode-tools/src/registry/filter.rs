//! Filtered registry implementation
//!
//! Provides a wrapper around the `ToolRegistry` that applies filtering criteria
//! to the tool list, ensuring the LLM only sees tools relevant to the current task.

use crate::ToolRegistry;
use rustycode_protocol::tool_filter::ToolFilterCriteria;
use rustycode_tools_api::ToolInfo;

pub struct FilteredRegistry {
    registry: ToolRegistry,
    criteria: ToolFilterCriteria,
}

impl FilteredRegistry {
    pub fn new(registry: ToolRegistry, criteria: ToolFilterCriteria) -> Self {
        Self { registry, criteria }
    }

    /// Returns the filtered list of tools based on criteria
    pub fn list(&self) -> Vec<ToolInfo> {
        let all_tools = self.registry.list();

        all_tools
            .into_iter()
            .filter(|tool| {
                // Apply filtering logic:
                // 1. Tag filtering
                if let Some(ref allowed_tags) = self.criteria.allowed_tags {
                    // Assuming tool description or name contains tags, or we could add tags to Tool trait
                    // For now, we assume a simple check. This will evolve as we tag tools.
                    if !allowed_tags.iter().any(|tag| tool.name.contains(tag)) {
                        return false;
                    }
                }

                // 2. Permission filtering
                if self
                    .criteria
                    .excluded_permissions
                    .contains(&format!("{:?}", tool.permission))
                {
                    return false;
                }

                true
            })
            .collect()
    }
}
