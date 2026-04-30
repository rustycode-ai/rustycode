use crate::tool_selection::UsageTracker;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ToolTier {
    #[default]
    Default,
    Extended,
    Full,
}

#[must_use]
pub fn default_tool_set() -> HashSet<&'static str> {
    [
        "read_file",
        "edit_file",
        "write_file",
        "bash",
        "grep",
        "glob",
    ]
    .into_iter()
    .collect()
}

#[must_use]
pub fn extended_tool_set() -> HashSet<&'static str> {
    [
        "web_fetch",
        "notebook_edit",
        "lsp_diagnostics",
        "lsp_hover",
        "lsp_definition",
        "lsp_references",
        "lsp_completion",
        "lsp_implementation",
        "lsp_incoming_calls",
        "lsp_outgoing_calls",
        "lsp_document_symbols",
        "todo_write",
        "todo_read",
        "memory_search",
        "memory_list",
        "list_dir",
        "git_status",
        "git_diff",
        "git_log",
    ]
    .into_iter()
    .collect()
}

#[derive(Debug, Clone)]
pub struct ToolActivationManager {
    tier: ToolTier,
    scoped_tools: Option<HashSet<String>>,
    usage: UsageTracker,
}

impl Default for ToolActivationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolActivationManager {
    pub fn new() -> Self {
        Self {
            tier: ToolTier::Default,
            scoped_tools: None,
            usage: UsageTracker::new(),
        }
    }

    pub const fn current_tier(&self) -> ToolTier {
        self.tier
    }

    pub fn promote(&mut self, tier: ToolTier) {
        if tier > self.tier {
            self.tier = tier;
        }
    }

    pub fn with_scope(mut self, tools: Vec<String>) -> Self {
        self.scoped_tools = Some(tools.into_iter().collect());
        self
    }

    pub fn set_scope(&mut self, tools: Vec<String>) {
        self.scoped_tools = Some(tools.into_iter().collect());
    }

    pub fn clear_scope(&mut self) {
        self.scoped_tools = None;
    }

    pub const fn usage(&self) -> &UsageTracker {
        &self.usage
    }

    pub const fn usage_mut(&mut self) -> &mut UsageTracker {
        &mut self.usage
    }

    pub fn record_use(&mut self, tool: &str, success: bool) {
        self.usage.record(tool, success);
    }

    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        let tier_allows = match self.tier {
            ToolTier::Default => default_tool_set().contains(tool_name),
            ToolTier::Extended => {
                default_tool_set().contains(tool_name) || extended_tool_set().contains(tool_name)
            }
            ToolTier::Full => true,
        };

        if !tier_allows {
            return false;
        }

        self.scoped_tools
            .as_ref()
            .is_none_or(|scope| scope.contains(tool_name))
    }

    pub fn allowed_tools(&self) -> Vec<String> {
        let base: Vec<String> = match self.tier {
            ToolTier::Default => default_tool_set().into_iter().map(String::from).collect(),
            ToolTier::Extended => default_tool_set()
                .into_iter()
                .chain(extended_tool_set())
                .map(String::from)
                .collect(),
            ToolTier::Full => self
                .scoped_tools
                .as_ref()
                .map_or_else(Vec::new, |scope| scope.iter().cloned().collect()),
        };

        if self.tier == ToolTier::Full {
            return base;
        }

        match &self.scoped_tools {
            Some(scope) => base
                .into_iter()
                .filter(|tool| scope.contains(tool))
                .collect(),
            None => base,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn tool_tier_default_is_default() {
        assert_eq!(ToolTier::default(), ToolTier::Default);
    }

    #[test]
    fn tool_tier_ordering() {
        assert!(ToolTier::Default < ToolTier::Extended);
        assert!(ToolTier::Extended < ToolTier::Full);
    }

    #[test]
    fn default_tools_contains_core_six() {
        let defaults = default_tool_set();
        assert!(defaults.contains("read_file"));
        assert!(defaults.contains("edit_file"));
        assert!(defaults.contains("write_file"));
        assert!(defaults.contains("bash"));
        assert!(defaults.contains("grep"));
        assert!(defaults.contains("glob"));
        assert_eq!(defaults.len(), 6);
    }

    #[test]
    fn extended_tools_contains_expected() {
        let extended = extended_tool_set();
        assert!(extended.contains("web_fetch"));
        assert!(extended.contains("notebook_edit"));
        assert!(extended.contains("lsp_diagnostics"));
        assert!(extended.contains("lsp_hover"));
        assert!(extended.contains("lsp_definition"));
        assert!(extended.contains("lsp_references"));
        assert!(extended.contains("lsp_completion"));
        assert!(extended.contains("todo_write"));
        assert!(extended.contains("memory_search"));
        assert!(extended.contains("memory_list"));
    }

    #[test]
    fn usage_tracker_records_invocation() {
        let mut tracker = UsageTracker::new();
        tracker.record("read_file", true);
        tracker.record("read_file", true);
        tracker.record("bash", false);

        assert_eq!(tracker.invocation_count("read_file"), 2);
        assert_eq!(tracker.invocation_count("bash"), 1);
        assert_eq!(tracker.invocation_count("write_file"), 0);
    }

    #[test]
    fn usage_tracker_tracks_success_rate() {
        let mut tracker = UsageTracker::new();
        tracker.record("bash", true);
        tracker.record("bash", true);
        tracker.record("bash", false);

        let rate = tracker.success_rate("bash");
        assert!((rate - 0.667).abs() < 0.05);
    }

    #[test]
    fn usage_tracker_success_rate_unknown_tool_is_zero() {
        let tracker = UsageTracker::new();
        assert_eq!(tracker.success_rate("nonexistent"), 0.0);
    }

    #[test]
    fn activation_manager_starts_default() {
        let manager = ToolActivationManager::new();
        assert_eq!(manager.current_tier(), ToolTier::Default);
        assert!(manager.is_tool_allowed("read_file"));
        assert!(!manager.is_tool_allowed("web_fetch"));
    }

    #[test]
    fn activation_manager_promotes_monotonically() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        manager.promote(ToolTier::Default);
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        manager.promote(ToolTier::Full);
        assert_eq!(manager.current_tier(), ToolTier::Full);
    }

    #[test]
    fn activation_manager_scope_filters_allowed_tools() {
        let manager = ToolActivationManager::new()
            .with_scope(vec!["read_file".to_string(), "bash".to_string()]);
        assert!(manager.is_tool_allowed("read_file"));
        assert!(manager.is_tool_allowed("bash"));
        assert!(!manager.is_tool_allowed("write_file"));
    }

    #[test]
    fn activation_manager_allowed_tools_respects_tier() {
        let mut manager = ToolActivationManager::new();
        let defaults = manager.allowed_tools();
        assert!(defaults.contains(&"read_file".to_string()));
        assert!(!defaults.contains(&"web_fetch".to_string()));

        manager.promote(ToolTier::Extended);
        let extended = manager.allowed_tools();
        assert!(extended.contains(&"web_fetch".to_string()));
    }
}
