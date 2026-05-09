//! Dynamic tool activation tiers.
//!
//! Tools are loaded progressively based on task demands:
//! - **Default**: Core tools always available (`read`, `edit`, `write`, `bash`, `grep`, `glob`)
//! - **Extended**: Additional tools activated on demand (`web_fetch`, `lsp`, `notebook_edit`, etc.)
//! - **Full**: All registered tools available
//!
//! This module provides tier management, per-session usage tracking, and skill-based
//! scoping that intersects with the active tier.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::isolation::ToolCapability;

// ToolTier enum

/// Tool activation tier. Tools are loaded progressively based on task demands.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum ToolTier {
    /// Core tools always available: Read, Edit, Write, Bash, Grep, Glob.
    #[default]
    Default,
    /// Additional tools activated on demand: `WebFetch`, `NotebookEdit`, `LSP` tools, etc.
    Extended,
    /// All registered tools available.
    Full,
}

impl std::fmt::Display for ToolTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Extended => write!(f, "extended"),
            Self::Full => write!(f, "full"),
        }
    }
}

// Static tool sets

/// The default tool set -- always available in every session.
pub fn default_tool_set() -> HashSet<&'static str> {
    ["Read", "Edit", "Write", "Bash", "Grep", "Glob"].into()
}

/// The extended tool set -- activated when the task requires more capabilities.
pub fn extended_tool_set() -> HashSet<&'static str> {
    [
        "WebFetch",
        "NotebookEdit",
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
        "todo_update",
        "todo_read",
        "memory_search",
        "memory_list",
        "list_dir",
        "git_status",
        "git_diff",
        "git_log",
    ]
    .into()
}

/// Map a tool name to its tier.
pub fn tier_for_tool(tool_name: &str) -> ToolTier {
    if default_tool_set().contains(tool_name) {
        ToolTier::Default
    } else if extended_tool_set().contains(tool_name) {
        ToolTier::Extended
    } else {
        ToolTier::Full
    }
}

/// Map a tool name to its capability using the isolation module's classifier.
pub fn capability_for_tool(tool_name: &str) -> ToolCapability {
    crate::isolation::classify_tool(tool_name)
}

// UsageTracker

/// Per-tool usage statistics.
#[derive(Debug, Clone, Default)]
struct ToolUsageEntry {
    invocations: u64,
    successes: u64,
}

/// Per-session usage tracking for tool invocations.
#[derive(Debug, Clone, Default)]
pub struct UsageTracker {
    entries: HashMap<String, ToolUsageEntry>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool invocation.
    pub fn record(&mut self, tool_name: &str, success: bool) {
        let entry = self.entries.entry(tool_name.to_string()).or_default();
        entry.invocations = entry.invocations.saturating_add(1);
        if success {
            entry.successes = entry.successes.saturating_add(1);
        }
    }

    /// How many times a tool was invoked.
    pub fn invocation_count(&self, tool_name: &str) -> u64 {
        self.entries.get(tool_name).map_or(0, |e| e.invocations)
    }

    /// Success rate for a tool (0.0 to 1.0). Returns 0.0 for unknown tools.
    pub fn success_rate(&self, tool_name: &str) -> f64 {
        self.entries.get(tool_name).map_or(0.0, |e| {
            if e.invocations == 0 {
                return 0.0;
            }
            #[allow(clippy::cast_precision_loss)]
            {
                e.successes as f64 / e.invocations as f64
            }
        })
    }

    /// Top N most-used tools, sorted descending by invocation count.
    pub fn most_used(&self, n: usize) -> Vec<(&str, u64)> {
        let mut v: Vec<_> = self
            .entries
            .iter()
            .map(|(k, e)| (k.as_str(), e.invocations))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(n);
        v
    }

    /// Clear all usage data.
    pub fn reset(&mut self) {
        self.entries.clear();
    }
}

// ToolActivationManager

/// Manages which tools are currently active based on tier and skill scoping.
///
/// The manager starts at [`ToolTier::Extended`] (LSP and advanced tools available
/// from the start) and can only be promoted upward.
/// A skill scope can further restrict available tools to an intersection of the
/// current tier's tool set and the skill's allowed list.
#[derive(Debug, Clone)]
pub struct ToolActivationManager {
    tier: ToolTier,
    /// When set, only these tools are active (intersection with tier tools).
    scope: Option<HashSet<String>>,
    /// Usage tracking for this activation session.
    usage: UsageTracker,
}

impl ToolActivationManager {
    /// Create a new manager starting at the Extended tier with no scope restriction.
    pub fn new() -> Self {
        Self {
            tier: ToolTier::Extended,
            scope: None,
            usage: UsageTracker::new(),
        }
    }

    /// Current activation tier.
    pub const fn current_tier(&self) -> ToolTier {
        self.tier
    }

    /// Promote to a higher tier. Demotion is a no-op.
    pub fn promote(&mut self, tier: ToolTier) {
        if tier > self.tier {
            self.tier = tier;
        }
    }

    /// Check if a specific tool is active (available for use).
    pub fn is_active(&self, tool_name: &str) -> bool {
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

        // If a skill scope is active, further restrict.
        if let Some(scope) = &self.scope {
            return scope.contains(tool_name);
        }

        true
    }

    /// Get a snapshot of currently active tool names.
    ///
    /// Returns an empty vec for Full tier (too many tools to enumerate).
    pub fn active_tools(&self) -> Vec<&str> {
        let base: HashSet<&str> = match self.tier {
            ToolTier::Default => default_tool_set(),
            ToolTier::Extended => {
                let mut s = default_tool_set();
                s.extend(extended_tool_set());
                s
            }
            ToolTier::Full => return vec![],
        };

        let mut result: Vec<&str> = if let Some(scope) = &self.scope {
            base.into_iter().filter(|t| scope.contains(*t)).collect()
        } else {
            base.into_iter().collect()
        };
        result.sort_unstable();
        result
    }

    /// Restrict active tools to the intersection of current tier and given scope.
    pub fn intersect_scope(&mut self, allowed: &[String]) {
        self.scope = Some(allowed.iter().cloned().collect());
    }

    /// Clear any skill scope restriction, restoring tier-based access.
    pub fn clear_scope(&mut self) {
        self.scope = None;
    }

    /// Access the usage tracker for this activation session.
    pub const fn usage(&self) -> &UsageTracker {
        &self.usage
    }

    /// Mutably access the usage tracker.
    #[allow(clippy::missing_const_for_fn)]
    pub fn usage_mut(&mut self) -> &mut UsageTracker {
        &mut self.usage
    }

    /// Determine whether the usage pattern suggests a tier promotion.
    ///
    /// Returns `Some(ToolTier)` if promotion is recommended, `None` otherwise.
    pub fn suggest_promotion(&self) -> Option<ToolTier> {
        if self.tier >= ToolTier::Extended {
            // Already at extended or full; no further suggestion needed for
            // default->extended promotion.
            return if self.tier < ToolTier::Full {
                let top = self.usage.most_used(5);
                let has_non_extended = top.iter().any(|(name, _)| {
                    !default_tool_set().contains(name) && !extended_tool_set().contains(name)
                });
                if has_non_extended {
                    Some(ToolTier::Full)
                } else {
                    None
                }
            } else {
                None
            };
        }

        // Check if any non-default tool was attempted.
        let top = self.usage.most_used(5);
        let needs_extended = top
            .iter()
            .any(|(name, _)| !default_tool_set().contains(name));

        if needs_extended {
            Some(ToolTier::Extended)
        } else {
            None
        }
    }
}

impl Default for ToolActivationManager {
    fn default() -> Self {
        Self::new()
    }
}

// Tests

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // -- ToolTier tests --

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
    fn tool_tier_display() {
        assert_eq!(ToolTier::Default.to_string(), "default");
        assert_eq!(ToolTier::Extended.to_string(), "extended");
        assert_eq!(ToolTier::Full.to_string(), "full");
    }

    #[test]
    fn tool_tier_serde_roundtrip() {
        for tier in [ToolTier::Default, ToolTier::Extended, ToolTier::Full] {
            let json = serde_json::to_string(&tier).unwrap();
            let decoded: ToolTier = serde_json::from_str(&json).unwrap();
            assert_eq!(tier, decoded);
        }
    }

    // -- Default tool set tests --

    #[test]
    fn default_tools_contains_core_six() {
        let defaults = default_tool_set();
        assert!(defaults.contains("Read"));
        assert!(defaults.contains("Edit"));
        assert!(defaults.contains("Write"));
        assert!(defaults.contains("Bash"));
        assert!(defaults.contains("Grep"));
        assert!(defaults.contains("Glob"));
        assert_eq!(defaults.len(), 6);
    }

    #[test]
    fn extended_tools_contains_expected() {
        let extended = extended_tool_set();
        assert!(extended.contains("WebFetch"));
        assert!(extended.contains("NotebookEdit"));
        assert!(extended.contains("lsp_diagnostics"));
        assert!(extended.contains("lsp_hover"));
        assert!(extended.contains("lsp_definition"));
        assert!(extended.contains("lsp_references"));
        assert!(extended.contains("lsp_completion"));
        assert!(extended.contains("todo_write"));
        assert!(extended.contains("memory_search"));
        assert!(extended.contains("memory_list"));
        assert!(extended.contains("list_dir"));
        assert!(extended.contains("git_status"));
    }

    #[test]
    fn tier_for_tool_classification() {
        assert_eq!(tier_for_tool("Read"), ToolTier::Default);
        assert_eq!(tier_for_tool("Bash"), ToolTier::Default);
        assert_eq!(tier_for_tool("WebFetch"), ToolTier::Extended);
        assert_eq!(tier_for_tool("lsp_hover"), ToolTier::Extended);
        assert_eq!(tier_for_tool("custom_tool_xyz"), ToolTier::Full);
    }

    #[test]
    fn capability_for_tool_delegates_to_isolation() {
        assert_eq!(capability_for_tool("Read"), ToolCapability::Read);
        assert_eq!(capability_for_tool("Write"), ToolCapability::Write);
        assert_eq!(capability_for_tool("Bash"), ToolCapability::Exec);
    }

    // -- UsageTracker tests --

    #[test]
    fn usage_tracker_records_invocation() {
        let mut tracker = UsageTracker::new();
        tracker.record("Read", true);
        tracker.record("Read", true);
        tracker.record("Bash", false);

        assert_eq!(tracker.invocation_count("Read"), 2);
        assert_eq!(tracker.invocation_count("Bash"), 1);
        assert_eq!(tracker.invocation_count("Write"), 0);
    }

    #[test]
    fn usage_tracker_tracks_success_rate() {
        let mut tracker = UsageTracker::new();
        tracker.record("Bash", true);
        tracker.record("Bash", true);
        tracker.record("Bash", false);

        let rate = tracker.success_rate("Bash");
        assert!((rate - 0.667).abs() < 0.05);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn usage_tracker_success_rate_unknown_tool_is_zero() {
        let tracker = UsageTracker::new();
        assert_eq!(tracker.success_rate("nonexistent"), 0.0);
    }

    #[test]
    fn usage_tracker_most_used_tools() {
        let mut tracker = UsageTracker::new();
        tracker.record("Bash", true);
        tracker.record("Bash", true);
        tracker.record("Bash", true);
        tracker.record("Read", true);
        tracker.record("Read", true);

        let top = tracker.most_used(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "Bash");
        assert_eq!(top[0].1, 3);
        assert_eq!(top[1].0, "Read");
        assert_eq!(top[1].1, 2);
    }

    #[test]
    fn usage_tracker_reset_clears_all() {
        let mut tracker = UsageTracker::new();
        tracker.record("Bash", true);
        tracker.reset();
        assert_eq!(tracker.invocation_count("Bash"), 0);
    }

    #[test]
    fn usage_tracker_saturating_add_on_overflow() {
        let mut tracker = UsageTracker::new();
        tracker.record("tool", true);
        // Directly overflow to test saturating behavior
        let entry = tracker.entries.get_mut("tool").unwrap();
        entry.invocations = u64::MAX;
        tracker.record("tool", true); // should not panic
        assert_eq!(tracker.invocation_count("tool"), u64::MAX);
    }

    #[test]
    fn usage_tracker_most_used_fewer_than_n() {
        let mut tracker = UsageTracker::new();
        tracker.record("Bash", true);
        let top = tracker.most_used(5);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, "Bash");
    }

    // -- ToolActivationManager tests --

    #[test]
    fn activation_manager_starts_at_extended_tier() {
        let manager = ToolActivationManager::new();
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        assert!(manager.is_active("Read"));
        assert!(manager.is_active("Bash"));
        assert!(manager.is_active("WebFetch"));
        assert!(manager.is_active("lsp_hover"));
    }

    #[test]
    fn activation_manager_promote_to_extended() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        assert_eq!(manager.current_tier(), ToolTier::Extended);
        assert!(manager.is_active("WebFetch"));
        assert!(manager.is_active("Read")); // still active
    }

    #[test]
    fn activation_manager_promote_to_full() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Full);
        assert!(manager.is_active("any_custom_tool"));
        assert!(manager.is_active("WebFetch"));
    }

    #[test]
    fn activation_manager_cannot_demote() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        manager.promote(ToolTier::Default); // no-op
        assert_eq!(manager.current_tier(), ToolTier::Extended);
    }

    #[test]
    fn activation_manager_active_tools_snapshot() {
        let manager = ToolActivationManager::new();
        let tools = manager.active_tools();
        assert!(tools.contains(&"Read"));
        assert!(tools.contains(&"Bash"));
        assert!(tools.contains(&"WebFetch")); // extended tools are active by default
    }

    #[test]
    fn activation_manager_with_scope_intersection() {
        let mut manager = ToolActivationManager::new();
        let scope = vec!["Read".to_string(), "Grep".to_string()];
        manager.intersect_scope(&scope);
        assert!(manager.is_active("Read"));
        assert!(manager.is_active("Grep"));
        assert!(!manager.is_active("Bash")); // restricted by scope
    }

    #[test]
    fn activation_manager_scope_with_extended_tier() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        let scope = vec![
            "Read".to_string(),
            "WebFetch".to_string(),
            "Bash".to_string(),
        ];
        manager.intersect_scope(&scope);
        assert!(manager.is_active("Read"));
        assert!(manager.is_active("WebFetch")); // in both extended tier and scope
        assert!(manager.is_active("Bash")); // bash is in both default tier and scope
        assert!(!manager.is_active("Edit")); // not in scope
        assert!(!manager.is_active("lsp_hover")); // not in scope
    }

    #[test]
    fn activation_manager_clear_scope_restores_tier() {
        let mut manager = ToolActivationManager::new();
        let scope = vec!["Read".to_string()];
        manager.intersect_scope(&scope);
        assert!(!manager.is_active("Bash"));
        manager.clear_scope();
        assert!(manager.is_active("Bash")); // restored
    }

    #[test]
    fn activation_manager_suggest_promotion_to_full_from_extended() {
        let mut manager = ToolActivationManager::new(); // starts at Extended
        manager.usage_mut().record("Read", true);
        manager.usage_mut().record("Bash", true);
        manager.usage_mut().record("custom_mcp_tool", true); // non-extended tool used
        assert_eq!(manager.suggest_promotion(), Some(ToolTier::Full));
    }

    #[test]
    fn activation_manager_suggest_promotion_none_when_default_sufficient() {
        let mut manager = ToolActivationManager::new();
        manager.usage_mut().record("Read", true);
        manager.usage_mut().record("Bash", true);
        manager.usage_mut().record("Grep", true);
        assert_eq!(manager.suggest_promotion(), None);
    }

    #[test]
    fn activation_manager_suggest_promotion_to_full() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Extended);
        manager.usage_mut().record("custom_mcp_tool", true);
        assert_eq!(manager.suggest_promotion(), Some(ToolTier::Full));
    }

    #[test]
    fn activation_manager_suggest_promotion_none_at_full() {
        let mut manager = ToolActivationManager::new();
        manager.promote(ToolTier::Full);
        manager.usage_mut().record("anything", true);
        assert_eq!(manager.suggest_promotion(), None);
    }

    #[test]
    fn activation_manager_usage_tracking() {
        let mut manager = ToolActivationManager::new();
        manager.usage_mut().record("Bash", true);
        manager.usage_mut().record("Bash", false);
        assert_eq!(manager.usage().invocation_count("Bash"), 2);
    }

    #[test]
    fn activation_manager_default() {
        let manager = ToolActivationManager::default();
        assert_eq!(manager.current_tier(), ToolTier::Extended);
    }
}
