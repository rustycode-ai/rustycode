//! Context isolation for tiered execution.
//!
//! Each tier (Musician, Editor, Composer) operates within its own context
//! budget and tool restriction policy. This module defines the isolation
//! boundaries and enforcement mechanisms.

use crate::types::ExecutionTier;
use rustycode_protocol::tool_names as tn;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Token budget tracking for a single tier's context window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextBudget {
    /// Maximum tokens this tier may consume.
    limit: u64,
    /// Tokens consumed so far.
    used: u64,
}

impl ContextBudget {
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// Create an unlimited budget (`u64::MAX` tokens).
    #[allow(clippy::missing_const_for_fn)]
    pub fn unlimited() -> Self {
        Self::new(u64::MAX)
    }

    /// Record token consumption. Saturates at the limit.
    #[allow(clippy::missing_const_for_fn)]
    pub fn add_tokens(&mut self, tokens: u64) {
        self.used = self.used.saturating_add(tokens);
    }

    /// Remaining tokens before exhaustion. Clamps to zero.
    pub const fn remaining_tokens(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Whether the budget has been fully consumed.
    pub const fn is_exhausted(&self) -> bool {
        self.used >= self.limit
    }

    /// The configured limit.
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Tokens consumed so far.
    pub const fn used(&self) -> u64 {
        self.used
    }

    /// Percentage of budget consumed (0.0 to 100.0).
    #[allow(clippy::cast_precision_loss)]
    pub fn usage_pct(&self) -> f64 {
        if self.limit == 0 {
            return 100.0;
        }
        (self.used as f64 / self.limit as f64) * 100.0
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::unlimited()
    }
}

/// Classification of tool capability levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolCapability {
    /// Read-only tools: read, grep, glob, ls
    Read,
    /// Write tools: write, edit
    Write,
    /// Execution tools: bash, sh
    Exec,
}

/// Tool restriction policy for a tier.
///
/// Maps tool names to their capability level and enforces which capabilities
/// are available at each tier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPolicy {
    /// The tier this policy applies to.
    tier: ExecutionTier,
    /// Which capabilities this tier is allowed.
    allowed_capabilities: Vec<ToolCapability>,
}

impl ToolPolicy {
    /// Musician (Tier 2): Full access -- read, write, exec.
    pub fn musician() -> Self {
        Self {
            tier: ExecutionTier::Musician,
            allowed_capabilities: vec![
                ToolCapability::Read,
                ToolCapability::Write,
                ToolCapability::Exec,
            ],
        }
    }

    /// Editor (Tier 3): Read + Write, no execution.
    pub fn editor() -> Self {
        Self {
            tier: ExecutionTier::Editor,
            allowed_capabilities: vec![ToolCapability::Read, ToolCapability::Write],
        }
    }

    /// Composer (Tier 4): Read-only. Research and planning only.
    pub fn composer() -> Self {
        Self {
            tier: ExecutionTier::Composer,
            allowed_capabilities: vec![ToolCapability::Read],
        }
    }

    /// Thinking (Tier 5): Read-only. Deep reasoning does not modify files.
    pub fn thinking() -> Self {
        Self {
            tier: ExecutionTier::Thinking,
            allowed_capabilities: vec![ToolCapability::Read],
        }
    }

    /// Get the policy for a given tier.
    pub fn for_tier(tier: ExecutionTier) -> Self {
        match tier {
            ExecutionTier::Musician => Self::musician(),
            ExecutionTier::Editor => Self::editor(),
            ExecutionTier::Composer => Self::composer(),
            ExecutionTier::Thinking => Self::thinking(),
        }
    }

    /// Whether a tool with the given capability is allowed at this tier.
    pub fn is_tool_allowed(&self, capability: ToolCapability) -> bool {
        self.allowed_capabilities.contains(&capability)
    }

    /// The tier this policy applies to.
    pub const fn tier(&self) -> ExecutionTier {
        self.tier
    }

    /// Which capabilities are allowed.
    pub fn allowed_capabilities(&self) -> &[ToolCapability] {
        &self.allowed_capabilities
    }
}

/// Classify a tool name into its capability level.
pub fn classify_tool(tool_name: &str) -> ToolCapability {
    match tool_name {
        tn::READ | "read" | tn::GREP | tn::GLOB | tn::LIST_DIR | tn::FIND | "Head" | "cat" => {
            ToolCapability::Read
        }
        tn::WRITE | "write" | tn::EDIT | "edit" | tn::NOTEBOOK_EDIT => ToolCapability::Write,
        _ => ToolCapability::Read,
    }
}

/// Configuration for per-tier context budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IsolationConfig {
    /// Token limit for Musician (Tier 2).
    pub tier_2_tokens: u64,
    /// Token limit for Editor (Tier 3).
    pub tier_3_tokens: u64,
    /// Token limit for Composer (Tier 4).
    pub tier_4_tokens: u64,
    /// Token limit for Thinking (Tier 5).
    pub tier_5_tokens: u64,
}

impl Default for IsolationConfig {
    fn default() -> Self {
        Self {
            tier_2_tokens: 100_000,
            tier_3_tokens: 80_000,
            tier_4_tokens: 60_000,
            tier_5_tokens: 50_000,
        }
    }
}

impl IsolationConfig {
    pub const fn new(
        tier_2_tokens: u64,
        tier_3_tokens: u64,
        tier_4_tokens: u64,
        tier_5_tokens: u64,
    ) -> Self {
        Self {
            tier_2_tokens,
            tier_3_tokens,
            tier_4_tokens,
            tier_5_tokens,
        }
    }

    /// Get the token limit for a specific tier.
    pub const fn limit_for_tier(&self, tier: ExecutionTier) -> u64 {
        match tier {
            ExecutionTier::Musician => self.tier_2_tokens,
            ExecutionTier::Editor => self.tier_3_tokens,
            ExecutionTier::Composer => self.tier_4_tokens,
            ExecutionTier::Thinking => self.tier_5_tokens,
        }
    }
}

/// Manages context isolation across all tiers.
///
/// Tracks per-tier context budgets and enforces tool restrictions.
/// Each tier gets its own budget and policy; no context leaks between tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierIsolation {
    /// Per-tier context budgets, keyed by tier number (2, 3, 4, 5).
    budgets: std::collections::HashMap<u8, ContextBudget>,
    /// Per-tier tool policies.
    policies: std::collections::HashMap<u8, ToolPolicy>,
}

impl TierIsolation {
    pub fn new(config: &IsolationConfig) -> Self {
        let mut budgets = std::collections::HashMap::new();
        let mut policies = std::collections::HashMap::new();

        for tier in [
            ExecutionTier::Musician,
            ExecutionTier::Editor,
            ExecutionTier::Composer,
            ExecutionTier::Thinking,
        ] {
            let tier_num = tier.as_u8();
            budgets.insert(tier_num, ContextBudget::new(config.limit_for_tier(tier)));
            policies.insert(tier_num, ToolPolicy::for_tier(tier));
        }

        Self { budgets, policies }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(&IsolationConfig::default())
    }

    /// Get the context budget for a specific tier.
    pub fn budget_for(&self, tier: u8) -> Option<&ContextBudget> {
        self.budgets.get(&tier)
    }

    /// Get the tool policy for a specific tier.
    pub fn policy_for(&self, tier: u8) -> Option<&ToolPolicy> {
        self.policies.get(&tier)
    }

    /// Check if a tool is allowed for the given tier.
    ///
    /// Returns `Ok(())` if allowed, `Err(IsolationError)` if blocked.
    pub fn check_tool_allowed(
        &self,
        tier: u8,
        tool_name: &str,
    ) -> std::result::Result<(), IsolationError> {
        let policy = self
            .policies
            .get(&tier)
            .ok_or(IsolationError::UnknownTier { tier })?;

        let capability = classify_tool(tool_name);

        if policy.is_tool_allowed(capability) {
            Ok(())
        } else {
            Err(IsolationError::ToolBlocked {
                tool: tool_name.to_string(),
                capability,
                tier: policy.tier(),
            })
        }
    }

    /// Record token usage for a tier. Returns error if budget exceeded.
    pub fn record_usage(
        &mut self,
        tier: u8,
        tokens: u64,
    ) -> std::result::Result<(), IsolationError> {
        let budget = self
            .budgets
            .get_mut(&tier)
            .ok_or(IsolationError::UnknownTier { tier })?;

        if budget.is_exhausted() {
            return Err(IsolationError::BudgetExhausted {
                tier,
                used: budget.used(),
                limit: budget.limit(),
            });
        }

        budget.add_tokens(tokens);
        Ok(())
    }

    /// Get a snapshot of all tier budgets (for handoff packages).
    pub fn budget_snapshot(&self) -> Vec<(u8, u64, u64)> {
        let mut snap: Vec<_> = self
            .budgets
            .iter()
            .map(|(&tier, budget)| (tier, budget.used(), budget.limit()))
            .collect();
        snap.sort_by_key(|(tier, _, _)| *tier);
        snap
    }
}

/// Errors from tier isolation enforcement.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum IsolationError {
    #[error("tool '{tool}' ({capability:?}) is blocked at tier {tier}")]
    ToolBlocked {
        tool: String,
        capability: ToolCapability,
        tier: ExecutionTier,
    },
    #[error("context budget exhausted for tier {tier}: {used}/{limit} tokens")]
    BudgetExhausted { tier: u8, used: u64, limit: u64 },
    #[error("unknown tier: {tier}")]
    UnknownTier { tier: u8 },
}

impl fmt::Display for ToolCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "read"),
            Self::Write => write!(f, "write"),
            Self::Exec => write!(f, "exec"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn context_budget_new_has_zero_usage() {
        let budget = ContextBudget::new(100);
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn context_budget_add_tokens_saturates_at_max() {
        let mut budget = ContextBudget::new(100);
        budget.add_tokens(150);
        assert!(budget.is_exhausted());
        assert_eq!(budget.remaining_tokens(), 0);
    }

    #[test]
    fn context_budget_remaining_tokens() {
        let mut budget = ContextBudget::new(100);
        budget.add_tokens(30);
        assert_eq!(budget.remaining_tokens(), 70);
    }

    #[test]
    fn context_budget_remaining_tokens_clamps_to_zero() {
        let mut budget = ContextBudget::new(100);
        budget.add_tokens(150);
        assert_eq!(budget.remaining_tokens(), 0);
    }

    #[test]
    fn context_budget_is_exhausted() {
        let mut budget = ContextBudget::new(100);
        assert!(!budget.is_exhausted());
        budget.add_tokens(100);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn context_budget_is_exhausted_at_zero_limit() {
        let budget = ContextBudget::new(0);
        assert!(budget.is_exhausted());
    }

    #[test]
    fn context_budget_with_limit_builder() {
        let budget = ContextBudget::new(500);
        assert_eq!(budget.limit(), 500);
    }

    #[test]
    fn tool_policy_musician_allows_all() {
        let policy = ToolPolicy::musician();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(policy.is_tool_allowed(ToolCapability::Write));
        assert!(policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tool_policy_editor_allows_read_write_blocks_exec() {
        let policy = ToolPolicy::editor();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(policy.is_tool_allowed(ToolCapability::Write));
        assert!(!policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tool_policy_composer_allows_read_only() {
        let policy = ToolPolicy::composer();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(!policy.is_tool_allowed(ToolCapability::Write));
        assert!(!policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tool_policy_is_tool_allowed_for_read_tool() {
        let policy = ToolPolicy::composer();
        assert!(policy.is_tool_allowed(classify_tool("Read")));
    }

    #[test]
    fn tool_policy_is_tool_allowed_for_write_tool() {
        let policy = ToolPolicy::editor();
        assert!(policy.is_tool_allowed(classify_tool("Write")));
    }

    #[test]
    fn tool_policy_is_tool_allowed_for_exec_tool() {
        let policy = ToolPolicy::musician();
        assert!(policy.is_tool_allowed(classify_tool("Bash")));
    }

    #[test]
    fn tool_policy_is_tool_allowed_for_unknown_tool() {
        let policy = ToolPolicy::composer();
        assert!(!policy.is_tool_allowed(classify_tool("unknown_tool")));
    }

    #[test]
    fn tier_isolation_new_creates_budgets_for_all_tiers() {
        let config = IsolationConfig::default();
        let isolation = TierIsolation::new(&config);
        assert!(isolation.budget_for(2).is_some());
        assert!(isolation.budget_for(3).is_some());
        assert!(isolation.budget_for(4).is_some());
        assert!(isolation.budget_for(5).is_some());
    }

    #[test]
    fn tier_isolation_new_uses_configured_limits() {
        let config = IsolationConfig::new(100, 200, 300, 400);
        let isolation = TierIsolation::new(&config);
        assert_eq!(isolation.budget_for(2).unwrap().limit(), 100);
        assert_eq!(isolation.budget_for(3).unwrap().limit(), 200);
        assert_eq!(isolation.budget_for(4).unwrap().limit(), 300);
        assert_eq!(isolation.budget_for(5).unwrap().limit(), 400);
    }

    #[test]
    fn tier_isolation_policy_for_musician_returns_full_access() {
        let isolation = TierIsolation::with_defaults();
        let policy = isolation.policy_for(2).unwrap();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(policy.is_tool_allowed(ToolCapability::Write));
        assert!(policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tier_isolation_policy_for_editor_returns_read_write() {
        let isolation = TierIsolation::with_defaults();
        let policy = isolation.policy_for(3).unwrap();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(policy.is_tool_allowed(ToolCapability::Write));
        assert!(!policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tier_isolation_policy_for_composer_returns_read_only() {
        let isolation = TierIsolation::with_defaults();
        let policy = isolation.policy_for(4).unwrap();
        assert!(policy.is_tool_allowed(ToolCapability::Read));
        assert!(!policy.is_tool_allowed(ToolCapability::Write));
        assert!(!policy.is_tool_allowed(ToolCapability::Exec));
    }

    #[test]
    fn tier_isolation_check_tool_allowed_musician_exec() {
        let isolation = TierIsolation::with_defaults();
        assert!(isolation.check_tool_allowed(2, "Bash").is_ok());
    }

    #[test]
    fn tier_isolation_check_tool_allowed_editor_exec_blocked() {
        let isolation = TierIsolation::with_defaults();
        assert!(isolation.check_tool_allowed(3, "Bash").is_err());
    }

    #[test]
    fn tier_isolation_check_tool_allowed_composer_write_blocked() {
        let isolation = TierIsolation::with_defaults();
        assert!(isolation.check_tool_allowed(4, "Write").is_err());
    }

    #[test]
    fn tier_isolation_check_tool_allowed_composer_read_ok() {
        let isolation = TierIsolation::with_defaults();
        assert!(isolation.check_tool_allowed(4, "Read").is_ok());
    }

    #[test]
    fn tier_isolation_record_usage_increments_budget() {
        let mut isolation = TierIsolation::with_defaults();
        isolation.record_usage(2, 50).unwrap();
        assert_eq!(isolation.budget_for(2).unwrap().used(), 50);
    }

    #[test]
    fn tier_isolation_record_usage_returns_error_on_exhaustion() {
        let mut isolation = TierIsolation::new(&IsolationConfig::new(100, 100, 100, 100));
        isolation.record_usage(2, 100).unwrap();
        assert!(isolation.record_usage(2, 1).is_err());
    }
}
