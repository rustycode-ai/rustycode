use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolProfile {
    Explore,
    Implement,
    Debug,
    Ops,
    All,
}

impl ToolProfile {
    /// Detect a tool profile from prompt content using keyword scoring.
    ///
    /// Delegates to [`crate::profile_from_prompt`].
    pub fn from_prompt(prompt: &str) -> Self {
        crate::profile_from_prompt(prompt)
    }

    pub const fn available_tools(&self) -> &[&'static str] {
        match self {
            Self::Explore => &[
                "read_file",
                "list_dir",
                "grep",
                "glob",
                "tool_search",
                "web_fetch",
                "lsp_hover",
                "lsp_definition",
                "semantic_search",
            ],
            Self::Implement => &["write_file", "edit", "bash", "read_file", "test", "grep"],
            Self::Debug => &[
                "lsp_diagnostics",
                "lsp_hover",
                "bash",
                "grep",
                "read_file",
                "test",
                "semantic_search",
            ],
            Self::Ops => &[
                "bash",
                "git_commit",
                "git_diff",
                "git_status",
                "web_fetch",
                "list_dir",
            ],
            Self::All => &[
                "bash",
                "read_file",
                "write_file",
                "edit",
                "list_dir",
                "grep",
                "glob",
                "web_fetch",
                "web_search",
                "tool_search",
                "lsp_diagnostics",
                "lsp_hover",
                "lsp_definition",
                "lsp_completion",
                "git_commit",
                "git_diff",
                "git_status",
                "git_log",
                "test",
                "todo_write",
                "todo_update",
                "semantic_search",
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageTracker {
    uses: HashMap<String, usize>,
    #[serde(default)]
    successes: HashMap<String, usize>,
    last_used: HashMap<String, u64>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, tool: &str, success: bool) {
        let tool_owned = tool.to_string();
        let count = self.uses.entry(tool_owned.clone()).or_insert(0);
        *count = count.saturating_add(1);
        if success {
            let success_count = self.successes.entry(tool_owned.clone()).or_insert(0);
            *success_count = success_count.saturating_add(1);
        }
        self.last_used.insert(
            tool_owned,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
    }

    pub fn record_use(&mut self, tool: &str) {
        self.record(tool, true);
    }

    pub fn usage_count(&self, tool: &str) -> usize {
        self.uses.get(tool).copied().unwrap_or(0)
    }

    pub fn invocation_count(&self, tool: &str) -> usize {
        self.usage_count(tool)
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn success_rate(&self, tool: &str) -> f64 {
        let count = self.usage_count(tool);
        if count == 0 {
            return 0.0;
        }
        let successes = self.successes.get(tool).copied().unwrap_or(0);
        successes as f64 / count as f64
    }

    pub fn recent_tools(&self, limit: usize) -> Vec<String> {
        let mut tools: Vec<_> = self
            .last_used
            .iter()
            .map(|(tool, time)| (tool.clone(), *time))
            .collect();

        tools.sort_by_key(|a| std::cmp::Reverse(a.1));
        tools
            .into_iter()
            .take(limit)
            .map(|(tool, _)| tool)
            .collect()
    }

    pub fn frequent_tools(&self, limit: usize) -> Vec<String> {
        let mut tools: Vec<_> = self
            .uses
            .iter()
            .map(|(tool, count)| (tool.clone(), *count))
            .collect();

        tools.sort_by_key(|a| std::cmp::Reverse(a.1));
        tools
            .into_iter()
            .take(limit)
            .map(|(tool, _)| tool)
            .collect()
    }

    pub fn get_statistics(&self) -> Vec<(String, usize, Option<u64>)> {
        let all_tools: std::collections::HashSet<_> =
            self.uses.keys().chain(self.last_used.keys()).collect();

        all_tools
            .into_iter()
            .map(|tool| {
                let count = self.usage_count(tool);
                let last_used = self.last_used.get(tool).copied();
                (tool.clone(), count, last_used)
            })
            .collect()
    }

    pub fn total_uses(&self) -> usize {
        self.uses.values().sum()
    }

    pub fn unique_tools(&self) -> usize {
        self.uses.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_profile_explore_tools() {
        let tools = ToolProfile::Explore.available_tools();
        assert!(tools.contains(&"read_file"));
        assert!(tools.contains(&"grep"));
        assert!(!tools.contains(&"write_file"));
    }

    #[test]
    fn tool_profile_implement_tools() {
        let tools = ToolProfile::Implement.available_tools();
        assert!(tools.contains(&"write_file"));
        assert!(tools.contains(&"bash"));
        assert!(tools.contains(&"edit"));
    }

    #[test]
    fn tool_profile_debug_tools() {
        let tools = ToolProfile::Debug.available_tools();
        assert!(tools.contains(&"bash"));
        assert!(tools.contains(&"grep"));
    }

    #[test]
    fn tool_profile_ops_tools() {
        let tools = ToolProfile::Ops.available_tools();
        assert!(tools.contains(&"bash"));
        assert!(tools.contains(&"git_commit"));
    }

    #[test]
    fn tool_profile_all_includes_all_categories() {
        let tools = ToolProfile::All.available_tools();
        assert!(tools.contains(&"bash"));
        assert!(tools.contains(&"read_file"));
        assert!(tools.contains(&"write_file"));
        assert!(tools.contains(&"grep"));
        assert!(tools.contains(&"git_commit"));
    }

    #[test]
    fn usage_tracker_new_is_empty() {
        let tracker = UsageTracker::new();
        assert_eq!(tracker.total_uses(), 0);
        assert_eq!(tracker.unique_tools(), 0);
    }

    #[test]
    fn usage_tracker_record_and_count() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("bash");
        tracker.record_use("bash");
        tracker.record_use("read");

        assert_eq!(tracker.usage_count("bash"), 2);
        assert_eq!(tracker.usage_count("read"), 1);
        assert_eq!(tracker.usage_count("write"), 0);
        assert_eq!(tracker.total_uses(), 3);
        assert_eq!(tracker.unique_tools(), 2);
    }

    #[test]
    fn usage_tracker_recent_tools() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("bash");
        tracker.record_use("read");
        tracker.record_use("write");

        let recent = tracker.recent_tools(2);
        assert_eq!(recent.len(), 2);
    }

    #[test]
    fn usage_tracker_frequent_tools() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("bash");
        tracker.record_use("bash");
        tracker.record_use("bash");
        tracker.record_use("read");

        let frequent = tracker.frequent_tools(1);
        assert_eq!(frequent.len(), 1);
        assert_eq!(frequent[0], "bash");
    }

    #[test]
    fn usage_tracker_get_statistics() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("bash");
        tracker.record_use("read");

        let stats = tracker.get_statistics();
        assert_eq!(stats.len(), 2);
        for (name, count, last_used) in &stats {
            if name == "bash" {
                assert_eq!(*count, 1);
                assert!(last_used.is_some());
            }
        }
    }

    #[test]
    fn usage_tracker_saturating_add() {
        let mut tracker = UsageTracker::new();
        // Simulate extreme usage count
        *tracker.uses.entry("tool".to_string()).or_insert(0) = usize::MAX;
        tracker.record_use("tool");
        assert_eq!(tracker.usage_count("tool"), usize::MAX);
    }

    #[test]
    fn tool_profile_from_prompt_explore() {
        assert_eq!(
            ToolProfile::from_prompt("what does this function do"),
            ToolProfile::Explore
        );
    }

    #[test]
    fn tool_profile_from_prompt_implement() {
        assert_eq!(
            ToolProfile::from_prompt("create a new module"),
            ToolProfile::Implement
        );
    }

    #[test]
    fn tool_profile_from_prompt_debug() {
        assert_eq!(
            ToolProfile::from_prompt("debug the error in main"),
            ToolProfile::Debug
        );
    }

    #[test]
    fn tool_profile_from_prompt_ops() {
        assert_eq!(
            ToolProfile::from_prompt("deploy to production"),
            ToolProfile::Ops
        );
    }

    #[test]
    fn tool_profile_from_prompt_ambiguous_returns_all() {
        // "test" has low score, might fall below threshold
        let profile = ToolProfile::from_prompt("random ambiguous text xyz");
        assert_eq!(profile, ToolProfile::All);
    }
}
