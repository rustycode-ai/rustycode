//! Runtime permission modes for controlling how tools are approved.
//!
//! Each mode controls how tool permissions are decided at runtime:
//!
//! - **Default**: Ask the user for each tool invocation
//! - **Plan**: Read-only tools auto-allowed; write/exec blocked
//! - **Auto**: AI-classified permissions based on rules
//! - **AcceptEdits**: Auto-accept file writes; ask for exec
//! - **Bypass**: Allow all tools without asking
//!
//! # Example
//!
//! ```ignore
//! use rustycode_protocol::permission_modes::{PermissionMode, PermissionRule, PermissionBehavior, PermissionDecision};
//!
//! let mode = PermissionMode::default();
//! assert_eq!(mode, PermissionMode::Default);
//!
//! // Check if a tool should be auto-allowed
//! let rules = vec![
//!     PermissionRule::allow("Read"),
//!     PermissionRule::deny("Bash"),
//! ];
//! let decision = PermissionMode::Default.decide("Read", &rules);
//! assert_eq!(decision, PermissionDecision::Allow);
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::tool_names as tn;

/// Runtime permission mode controlling how tool invocations are approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PermissionMode {
    /// Ask the user for each tool invocation.
    #[default]
    Default,
    /// Plan mode: only read-only tools auto-allowed, write/exec blocked.
    Plan,
    /// Auto mode: AI-classified permissions based on configured rules.
    Auto,
    /// Auto-accept file edits; still ask for command execution.
    AcceptEdits,
    /// No prompts, but deny rules still enforced.
    DontAsk,
    /// Subagent escalation: defer to parent agent's permission decision.
    Bubble,
    /// Allow all tools without asking (use with caution).
    Bypass,
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Plan => write!(f, "plan"),
            Self::Auto => write!(f, "auto"),
            Self::AcceptEdits => write!(f, "acceptEdits"),
            Self::DontAsk => write!(f, "dontAsk"),
            Self::Bubble => write!(f, "bubble"),
            Self::Bypass => write!(f, "bypass"),
        }
    }
}

impl PermissionMode {
    /// Parse a permission mode from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default" => Some(Self::Default),
            "plan" => Some(Self::Plan),
            "auto" => Some(Self::Auto),
            "acceptedits" | "accept-edits" | "accept_edits" => Some(Self::AcceptEdits),
            "bypass" | "bypasspermissions" | "yolo" => Some(Self::Bypass),
            "dontask" | "dont-ask" | "dont_ask" => Some(Self::DontAsk),
            "bubble" => Some(Self::Bubble),
            _ => None,
        }
    }

    /// Decide whether a tool invocation should be allowed, denied, or asked about.
    ///
    /// Decision order:
    /// 1. Check explicit deny rules
    /// 2. Check explicit allow rules
    /// 3. Apply mode-specific defaults
    pub fn decide(&self, tool_name: &str, rules: &[PermissionRule]) -> PermissionDecision {
        // 1. Check deny rules first (highest priority)
        for rule in rules {
            if rule.behavior == PermissionBehavior::Deny && rule.matches(tool_name) {
                return PermissionDecision::Deny {
                    reason: format!("denied by rule: {}", rule.tool_pattern),
                };
            }
        }

        // 2. Check allow rules
        for rule in rules {
            if rule.behavior == PermissionBehavior::Allow && rule.matches(tool_name) {
                return PermissionDecision::Allow {
                    reason: format!("allowed by rule: {}", rule.tool_pattern),
                };
            }
        }

        // 3. Mode-specific defaults
        match self {
            Self::Bypass => PermissionDecision::Allow {
                reason: "bypass mode".to_string(),
            },
            Self::Plan => {
                if is_read_only_tool(tool_name) {
                    PermissionDecision::Allow {
                        reason: "read-only tool in plan mode".to_string(),
                    }
                } else {
                    PermissionDecision::Deny {
                        reason: format!("{} blocked in plan mode", tool_name),
                    }
                }
            }
            Self::AcceptEdits => {
                if is_edit_tool(tool_name) {
                    PermissionDecision::Allow {
                        reason: "edit tool auto-accepted".to_string(),
                    }
                } else if is_read_only_tool(tool_name) {
                    PermissionDecision::Allow {
                        reason: "read-only tool".to_string(),
                    }
                } else {
                    PermissionDecision::Ask {
                        message: format!("Allow {} to execute?", tool_name),
                    }
                }
            }
            Self::Auto => PermissionDecision::Ask {
                message: format!("Allow {}?", tool_name),
            },
            Self::DontAsk => {
                // No prompts — if no explicit allow rule matched, deny by default.
                // Deny rules were already checked above, so this only fires for
                // tools that have no rule and aren't covered by mode defaults.
                if is_read_only_tool(tool_name) {
                    PermissionDecision::Allow {
                        reason: "read-only tool in dontAsk mode".to_string(),
                    }
                } else {
                    PermissionDecision::Deny {
                        reason: format!(
                            "{} has no allow rule and dontAsk mode won't prompt",
                            tool_name
                        ),
                    }
                }
            }
            Self::Bubble => PermissionDecision::Deny {
                reason: format!(
                    "bubble mode: {} requires parent agent escalation",
                    tool_name
                ),
            },
            Self::Default => PermissionDecision::Ask {
                message: format!("Allow {}?", tool_name),
            },
        }
    }
}

/// Whether a tool is used for plan submission/review (no side effects on codebase).
pub(crate) fn is_plan_tool(name: &str) -> bool {
    matches!(
        name,
        "SubmitPlan" | "ReviewPlan" | "ApprovePlan" | "RejectPlan" | "ListPlans" | "GetPlan"
    )
}

/// Whether a tool is read-only (no side effects).
pub(crate) fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        tn::READ
            | tn::LIST_DIR
            | tn::GREP
            | tn::GLOB
            | tn::FIND
            | tn::INSPECT
            | tn::WEB_SEARCH
            | tn::WEB_FETCH
    )
}

/// Whether a tool modifies files.
fn is_edit_tool(name: &str) -> bool {
    matches!(name, tn::WRITE | tn::EDIT | tn::ATOMIC_WRITE)
}

/// The outcome of a permission decision.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PermissionDecision {
    /// Tool invocation is allowed.
    Allow {
        /// Why the tool was allowed.
        reason: String,
    },
    /// Tool invocation is denied.
    Deny {
        /// Why the tool was denied.
        reason: String,
    },
    /// User should be asked for permission.
    Ask {
        /// Message to show the user.
        message: String,
    },
}

impl PermissionDecision {
    /// Check if the decision is to allow.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// Check if the decision is to deny.
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Deny { .. })
    }

    /// Check if the user should be asked.
    pub fn is_ask(&self) -> bool {
        matches!(self, Self::Ask { .. })
    }
}

/// The behavior a permission rule enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PermissionBehavior {
    /// Always allow the matched tool.
    Allow,
    /// Always deny the matched tool.
    Deny,
    /// Always ask the user about the matched tool.
    Ask,
}

/// Source of a permission rule, for precedence and auditing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum PermissionRuleSource {
    /// User's global settings (~/.claude/settings.json).
    UserSettings,
    /// Project-level settings (.claude/settings.json in repo root).
    ProjectSettings,
    /// Local directory settings.
    LocalSettings,
    /// Organization policy (read-only, cannot be overridden).
    Policy,
    /// Set via CLI argument.
    CliArg,
    /// Runtime session rule (not persisted).
    Session,
}

impl PermissionRuleSource {
    /// Precedence order: Policy > CliArg > Session > ProjectSettings > LocalSettings > UserSettings
    pub fn precedence(&self) -> u8 {
        match self {
            Self::Policy => 6,
            Self::CliArg => 5,
            Self::Session => 4,
            Self::ProjectSettings => 3,
            Self::LocalSettings => 2,
            Self::UserSettings => 1,
        }
    }
}

/// A single permission rule that matches tool names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    /// What to do when the rule matches.
    pub behavior: PermissionBehavior,
    /// Tool name or glob pattern to match (e.g., "Read", "Bash", "*").
    pub tool_pattern: String,
    /// Where this rule came from.
    pub source: PermissionRuleSource,
}

impl PermissionRule {
    /// Create an allow rule for a tool pattern.
    pub fn allow(pattern: impl Into<String>) -> Self {
        Self {
            behavior: PermissionBehavior::Allow,
            tool_pattern: pattern.into(),
            source: PermissionRuleSource::Session,
        }
    }

    /// Create a deny rule for a tool pattern.
    pub fn deny(pattern: impl Into<String>) -> Self {
        Self {
            behavior: PermissionBehavior::Deny,
            tool_pattern: pattern.into(),
            source: PermissionRuleSource::Session,
        }
    }

    /// Create an ask rule for a tool pattern.
    pub fn ask(pattern: impl Into<String>) -> Self {
        Self {
            behavior: PermissionBehavior::Ask,
            tool_pattern: pattern.into(),
            source: PermissionRuleSource::Session,
        }
    }

    /// Set the source of this rule.
    pub fn with_source(mut self, source: PermissionRuleSource) -> Self {
        self.source = source;
        self
    }

    /// Check if this rule matches the given tool name.
    pub fn matches(&self, tool_name: &str) -> bool {
        if self.tool_pattern == "*" {
            return true;
        }
        // Exact match
        if self.tool_pattern == tool_name {
            return true;
        }
        // Simple glob: "bash*" matches "Bash", "bash_tool"
        if let Some(prefix) = self.tool_pattern.strip_suffix('*') {
            return tool_name.starts_with(prefix);
        }
        // Simple glob: "*bash" matches "run_bash"
        if let Some(suffix) = self.tool_pattern.strip_prefix('*') {
            return tool_name.ends_with(suffix);
        }
        false
    }
}

/// A collection of permission rules with precedence-based evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionRuleSet {
    /// All rules, evaluated in precedence order.
    pub rules: Vec<PermissionRule>,
}

impl PermissionRuleSet {
    /// Create an empty rule set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a rule to the set.
    pub fn add(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
    }

    /// Remove all rules matching the given tool pattern and behavior.
    pub fn remove(&mut self, tool_pattern: &str, behavior: PermissionBehavior) {
        self.rules
            .retain(|r| !(r.tool_pattern == tool_pattern && r.behavior == behavior));
    }

    /// Get rules sorted by source precedence (highest first).
    pub fn sorted_by_precedence(&self) -> Vec<&PermissionRule> {
        let mut refs: Vec<_> = self.rules.iter().collect();
        refs.sort_by_key(|r| std::cmp::Reverse(r.source.precedence()));
        refs
    }

    /// Decide permission for a tool using these rules and a mode.
    pub fn decide(&self, tool_name: &str, mode: PermissionMode) -> PermissionDecision {
        // First check explicit ask rules (they override mode defaults)
        let sorted = self.sorted_by_precedence();
        for rule in &sorted {
            if rule.behavior == PermissionBehavior::Ask && rule.matches(tool_name) {
                return PermissionDecision::Ask {
                    message: format!("Rule requires confirmation for {}", tool_name),
                };
            }
        }

        // Then delegate to mode-based decision (which checks deny/allow rules)
        mode.decide(tool_name, &self.rules)
    }

    /// Check if there are any rules at all.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_mode_default_str_roundtrip() {
        assert_eq!(
            PermissionMode::from_str_loose("default"),
            Some(PermissionMode::Default)
        );
        assert_eq!(
            PermissionMode::from_str_loose("plan"),
            Some(PermissionMode::Plan)
        );
        assert_eq!(
            PermissionMode::from_str_loose("auto"),
            Some(PermissionMode::Auto)
        );
        assert_eq!(
            PermissionMode::from_str_loose("acceptEdits"),
            Some(PermissionMode::AcceptEdits)
        );
        assert_eq!(
            PermissionMode::from_str_loose("accept-edits"),
            Some(PermissionMode::AcceptEdits)
        );
        assert_eq!(
            PermissionMode::from_str_loose("bypass"),
            Some(PermissionMode::Bypass)
        );
        assert_eq!(PermissionMode::from_str_loose("unknown"), None);
    }

    #[test]
    fn bypass_mode_allows_everything() {
        let rules = vec![];
        assert!(PermissionMode::Bypass.decide("Bash", &rules).is_allowed());
        assert!(PermissionMode::Bypass.decide("Write", &rules).is_allowed());
        assert!(PermissionMode::Bypass.decide("Read", &rules).is_allowed());
    }

    #[test]
    fn plan_mode_allows_readonly_blocks_writes() {
        let rules = vec![];
        assert!(PermissionMode::Plan.decide("Read", &rules).is_allowed());
        assert!(PermissionMode::Plan.decide("ListDir", &rules).is_allowed());
        assert!(PermissionMode::Plan.decide("Write", &rules).is_denied());
        assert!(PermissionMode::Plan.decide("Bash", &rules).is_denied());
    }

    #[test]
    fn accept_edits_allows_edits_asks_exec() {
        let rules = vec![];
        assert!(PermissionMode::AcceptEdits
            .decide("Write", &rules)
            .is_allowed());
        assert!(PermissionMode::AcceptEdits
            .decide("Edit", &rules)
            .is_allowed());
        assert!(PermissionMode::AcceptEdits
            .decide("Read", &rules)
            .is_allowed());
        assert!(PermissionMode::AcceptEdits.decide("Bash", &rules).is_ask());
    }

    #[test]
    fn deny_rules_override_mode() {
        let rules = vec![PermissionRule::deny("Read")];
        // Even bypass mode should respect explicit deny rules
        let decision = PermissionMode::Bypass.decide("Read", &rules);
        assert!(decision.is_denied());
    }

    #[test]
    fn allow_rules_override_mode() {
        let rules = vec![PermissionRule::allow("Bash")];
        // Plan mode would deny bash, but explicit allow overrides
        let decision = PermissionMode::Plan.decide("Bash", &rules);
        assert!(decision.is_allowed());
    }

    #[test]
    fn rule_pattern_matching() {
        let rule = PermissionRule::allow("Bash*");
        assert!(rule.matches("Bash"));
        assert!(rule.matches("Bash_tool"));
        assert!(!rule.matches("Read"));

        let rule = PermissionRule::deny("*");
        assert!(rule.matches("anything"));
        assert!(rule.matches("Bash"));
    }

    #[test]
    fn rule_set_precedence() {
        let mut rules = PermissionRuleSet::new();
        rules.add(PermissionRule::allow("Bash").with_source(PermissionRuleSource::UserSettings));
        rules.add(PermissionRule::deny("Bash").with_source(PermissionRuleSource::Policy));

        // Policy deny should override user allow
        let decision = rules.decide("Bash", PermissionMode::Default);
        assert!(decision.is_denied());
    }

    #[test]
    fn rule_set_ask_overrides_mode() {
        let mut rules = PermissionRuleSet::new();
        rules.add(PermissionRule::ask("Read"));

        // Even in bypass mode, explicit ask rule should prompt
        let decision = rules.decide("Read", PermissionMode::Bypass);
        assert!(decision.is_ask());
    }

    #[test]
    fn display_format() {
        assert_eq!(format!("{}", PermissionMode::Default), "default");
        assert_eq!(format!("{}", PermissionMode::Bypass), "bypass");
        assert_eq!(format!("{}", PermissionMode::AcceptEdits), "acceptEdits");
        assert_eq!(format!("{}", PermissionMode::DontAsk), "dontAsk");
        assert_eq!(format!("{}", PermissionMode::Bubble), "bubble");
    }

    #[test]
    fn dont_ask_mode_allows_readonly_denies_unknown() {
        let rules = vec![];
        assert!(PermissionMode::DontAsk.decide("Read", &rules).is_allowed());
        assert!(PermissionMode::DontAsk.decide("Bash", &rules).is_denied());
        assert!(PermissionMode::DontAsk.decide("Write", &rules).is_denied());
    }

    #[test]
    fn dont_ask_mode_respects_explicit_allow_rules() {
        let rules = vec![PermissionRule::allow("Bash")];
        assert!(PermissionMode::DontAsk.decide("Bash", &rules).is_allowed());
    }

    #[test]
    fn dont_ask_mode_respects_deny_rules() {
        let rules = vec![PermissionRule::deny("Read")];
        assert!(PermissionMode::DontAsk.decide("Read", &rules).is_denied());
    }

    #[test]
    fn bubble_mode_denies_everything() {
        let rules = vec![];
        assert!(PermissionMode::Bubble.decide("Read", &rules).is_denied());
        assert!(PermissionMode::Bubble.decide("Bash", &rules).is_denied());
    }

    #[test]
    fn bubble_mode_allows_explicit_rules() {
        let rules = vec![PermissionRule::allow("Read")];
        assert!(PermissionMode::Bubble.decide("Read", &rules).is_allowed());
    }

    #[test]
    fn from_str_loose_new_modes() {
        assert_eq!(
            PermissionMode::from_str_loose("dontAsk"),
            Some(PermissionMode::DontAsk)
        );
        assert_eq!(
            PermissionMode::from_str_loose("dont-ask"),
            Some(PermissionMode::DontAsk)
        );
        assert_eq!(
            PermissionMode::from_str_loose("bubble"),
            Some(PermissionMode::Bubble)
        );
    }
}
