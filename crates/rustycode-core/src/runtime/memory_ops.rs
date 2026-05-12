//! Runtime memory and skill prompt operations.

use std::path::Path;

use anyhow::Result;

use super::Runtime;

impl Runtime {
    /// Upsert a memory entry.
    pub fn upsert_memory(&self, scope: &str, key: &str, value: &str) -> Result<()> {
        self.storage.upsert_memory(scope, key, value)
    }

    /// Get all memory entries for a scope.
    pub fn memory(&self, scope: &str) -> Result<Vec<rustycode_storage::MemoryRecord>> {
        self.storage.memory(scope)
    }

    /// Get a single memory entry.
    pub fn memory_entry(&self, scope: &str, key: &str) -> Result<Option<String>> {
        self.storage.memory_entry(scope, key)
    }

    /// Build a system prompt augmented with active skill guidance.
    pub(crate) fn build_skill_augmented_prompt(&self, task: &str, cwd: Option<&Path>) -> String {
        use crate::headless::config::HEADLESS_SYSTEM_PROMPT;

        let Ok(mut guard) = self.skill_manager.lock() else {
            return HEADLESS_SYSTEM_PROMPT.to_string();
        };
        let Some(mgr) = guard.as_mut() else {
            return HEADLESS_SYSTEM_PROMPT.to_string();
        };

        // Context-based activation
        mgr.activate_for_context(task);

        // Path-based activation: scan top-level cwd entries
        if let Some(cwd) = cwd {
            if let Ok(entries) = std::fs::read_dir(cwd) {
                let names: Vec<String> = entries
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect();
                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                mgr.activate_for_paths(&refs);
            }
        }

        // Build skill section from active definitions
        let active = mgr.active_definitions();
        if active.is_empty() {
            return HEADLESS_SYSTEM_PROMPT.to_string();
        }

        let skill_guidance = active
            .iter()
            .map(|def| {
                format!(
                    "### {}\n{}\n\nWhen to use: {}",
                    def.name, def.description, def.when_to_use
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!(
            "{}\n\n# Active Skills\n\nThe following skills are relevant to this task:\n\n{}",
            HEADLESS_SYSTEM_PROMPT, skill_guidance
        )
    }
}
