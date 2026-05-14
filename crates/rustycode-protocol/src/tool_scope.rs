//! Scoped tool access for sub-agents.
//! Controls which tools an agent can use, inherited and restricted from parent scope.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Controls which tools an agent has access to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolScope {
    /// Tools explicitly allowed. If empty, all non-denied tools are allowed.
    allowed: HashSet<String>,
    /// Tools explicitly denied (takes precedence over allowed).
    denied: HashSet<String>,
}

impl ToolScope {
    /// Full access — all tools available.
    #[must_use]
    pub fn full() -> Self {
        Self {
            allowed: HashSet::new(),
            denied: HashSet::new(),
        }
    }

    /// Allow only specific tools.
    #[must_use]
    pub fn allow_only(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed: tools.into_iter().map(Into::into).collect(),
            denied: HashSet::new(),
        }
    }

    /// Deny specific tools, allow everything else.
    #[must_use]
    pub fn deny_only(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            allowed: HashSet::new(),
            denied: tools.into_iter().map(Into::into).collect(),
        }
    }

    /// Check if a tool is allowed in this scope.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        if self.denied.contains(tool_name) {
            return false;
        }
        // If allowed is empty, everything non-denied is allowed
        self.allowed.is_empty() || self.allowed.contains(tool_name)
    }

    /// Create a restricted sub-scope from this scope.
    /// The sub-scope can only allow tools that this scope allows.
    #[must_use]
    pub fn restrict(&self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let requested: HashSet<String> = tools.into_iter().map(Into::into).collect();
        let allowed = if self.allowed.is_empty() {
            // Parent allows everything not denied — sub gets requested minus parent denied
            &requested - &self.denied
        } else {
            // Parent has explicit allow list — sub gets intersection minus denied
            &(&requested & &self.allowed) - &self.denied
        };
        Self {
            allowed,
            denied: self.denied.clone(),
        }
    }

    /// List all explicitly allowed tools (empty = all non-denied).
    pub fn allowed_tools(&self) -> Vec<&str> {
        self.allowed.iter().map(|s| s.as_str()).collect()
    }

    /// List all explicitly denied tools.
    pub fn denied_tools(&self) -> Vec<&str> {
        self.denied.iter().map(|s| s.as_str()).collect()
    }
}

impl Default for ToolScope {
    fn default() -> Self {
        Self::full()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_access_allows_everything() {
        let scope = ToolScope::full();
        assert!(scope.is_allowed("bash"));
        assert!(scope.is_allowed("edit"));
        assert!(scope.is_allowed("read"));
    }

    #[test]
    fn allow_only_restricts() {
        let scope = ToolScope::allow_only(["read", "grep"]);
        assert!(scope.is_allowed("read"));
        assert!(scope.is_allowed("grep"));
        assert!(!scope.is_allowed("bash"));
        assert!(!scope.is_allowed("edit"));
    }

    #[test]
    fn deny_only_blocks() {
        let scope = ToolScope::deny_only(["bash", "write"]);
        assert!(!scope.is_allowed("bash"));
        assert!(!scope.is_allowed("write"));
        assert!(scope.is_allowed("read"));
        assert!(scope.is_allowed("edit"));
    }

    #[test]
    fn deny_overrides_allow() {
        let mut scope = ToolScope::allow_only(["read", "bash", "edit"]);
        scope.denied.insert("bash".to_string());
        assert!(scope.is_allowed("read"));
        assert!(!scope.is_allowed("bash")); // denied wins
        assert!(scope.is_allowed("edit"));
    }

    #[test]
    fn restrict_from_full() {
        let parent = ToolScope::full();
        let child = parent.restrict(["read", "grep", "bash"]);
        assert!(child.is_allowed("read"));
        assert!(child.is_allowed("grep"));
        assert!(child.is_allowed("bash"));
        assert!(!child.is_allowed("edit")); // not in requested set
    }

    #[test]
    fn restrict_from_allow_only() {
        let parent = ToolScope::allow_only(["read", "grep", "edit"]);
        let child = parent.restrict(["read", "bash", "edit"]);
        assert!(child.is_allowed("read"));
        assert!(!child.is_allowed("bash")); // not in parent's allowed
        assert!(child.is_allowed("edit"));
        assert!(!child.is_allowed("grep")); // not in requested set
    }

    #[test]
    fn restrict_denied_propagates() {
        let parent = ToolScope::deny_only(["bash"]);
        let child = parent.restrict(["read", "bash", "edit"]);
        assert!(child.is_allowed("read"));
        assert!(!child.is_allowed("bash")); // inherited deny
        assert!(child.is_allowed("edit"));
    }

    #[test]
    fn default_is_full() {
        let scope = ToolScope::default();
        assert!(scope.is_allowed("any_tool"));
    }

    #[test]
    fn serialization_round_trip() {
        let scope = ToolScope::allow_only(["read", "grep"]);
        let json = serde_json::to_string(&scope).unwrap();
        let back: ToolScope = serde_json::from_str(&json).unwrap();
        assert!(back.is_allowed("read"));
        assert!(!back.is_allowed("bash"));
    }
}
