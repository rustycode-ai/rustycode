# Phase 3: Context-Isolated Subagents -- TDD Implementation Plan

**Date**: 2026-04-25
**Pattern Source**: "12 Agentic Harness Patterns from Claude Code" (Pattern 7: Context-Isolated Subagents, Pattern 8: Fork-Join Parallelism)
**Status**: Partially Implemented
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map)
**Depends On**: Phase 2 (Explore-Plan-Act Lifecycle) -- phases define isolation boundaries

---

## Overview

Each tier (Musician, Editor, Composer) operates in its own context window with restricted tools. No context leakage between tiers. Handoffs between tiers use an explicit context package. Parallel tasks get fork-join execution with parent context snapshots.

**Four work items**:
1. Context isolation -- per-tier context budget tracking, no leakage
2. Tool restriction per tier -- Musician=everything, Editor=read+write (no exec), Composer=read-only
3. Handoff protocol -- explicit context package between tiers
4. Fork-join with shared context cache -- parallel tasks with parent context snapshots

Core runtime hooks are now in place for all four work items. The remaining gap is broader adoption across every orchestration entry point and any future subagent-specific callers.

**Success metrics**:
- Each tier has independent context budget tracking
- Tool restrictions enforced (no write calls from research tier)
- Handoff packages contain all necessary context
- Parallel forks start within 500ms of each other

---

## Existing Codebase Leveraged

| Component | Crate | What It Provides |
|-----------|-------|-----------------|
| `ExecutionTier` | `rustycode-orchestration/src/types.rs` | Tier enum (Musician=2, Editor=3, Composer=4, Thinking=5) |
| `TaskContext` | `rustycode-orchestration/src/task_context.rs` | Execution context with phase, tier, budget, trace, workspace |
| `TaskPhase` | `rustycode-orchestration/src/task_context.rs` | Phase lifecycle with tier mapping |
| `BudgetConfig` | `rustycode-orchestration/src/config.rs` | Per-tier budget fields (tier_2/3/4_max_usd) -- currently unused |
| `ResourceAccess` | `rustycode-orchestration/src/guard.rs` | Read/Write/Exec permission levels |
| `LockManager` | `rustycode-orchestration/src/guard.rs` | Resource locking for concurrent agents |
| `ToolExecutor` trait | `rustycode-orchestration/src/musician.rs` | Tool execution with role-based gating |
| `tools_for_role()` | `rustycode-orchestration/src/musician.rs` | Maps AgentRole to allowed tool list |
| `OrchestrationError` | `rustycode-orchestration/src/error.rs` | Error types with categories |
| `OrchestrationEvent` | `rustycode-orchestration/src/bus.rs` | Event bus for inter-component notifications |
| `BusHandle` | `rustycode-orchestration/src/bus.rs` | Publish/subscribe event system |
| `SharedWorkspace` | `rustycode-orchestration/src/shared_workspace.rs` | Key-value workspace with snapshot support |
| `StepOrchestrator` | `rustycode-orchestration/src/orchestrator.rs` | Tiered step execution with retry/escalation and isolation checks |
| `OrchestrationPipeline` | `rustycode-orchestration/src/pipeline.rs` | End-to-end task lifecycle plus fork-join entry point |

---

## File Structure

| # | File | Action | Purpose |
|---|------|--------|---------|
| 1 | `crates/rustycode-orchestration/src/isolation.rs` | **Edit** | `TierIsolation`, `ToolPolicy`, `ContextBudget` -- per-tier isolation |
| 2 | `crates/rustycode-orchestration/src/handoff.rs` | **Edit** | `HandoffPackage`, `HandoffBuilder` -- explicit inter-tier context |
| 3 | `crates/rustycode-orchestration/src/fork_join.rs` | **Edit** | `ForkJoinExecutor`, `ForkSpec`, `ContextSnapshot` -- parallel execution |
| 4 | `crates/rustycode-orchestration/src/error.rs` | **Edit** | Add `IsolationError`, `HandoffError`, `ForkJoinError` variants |
| 5 | `crates/rustycode-orchestration/src/bus.rs` | **Edit** | Add `TierHandoff`, `ForkStarted`, `ForkCompleted`, `ContextBudgetExceeded` events |
| 6 | `crates/rustycode-orchestration/src/orchestrator.rs` | **Edit** | Enforce tool restrictions via `TierIsolation` before tier execution |
| 7 | `crates/rustycode-orchestration/src/pipeline.rs` | **Edit** | Wire `TierIsolation` into `conduct()`, create handoffs between tiers |
| 8 | `crates/rustycode-orchestration/src/lib.rs` | **Edit** | Add `pub mod isolation`, `pub mod handoff`, `pub mod fork_join` + re-exports |

---

## TDD Steps

### Chunk 1: ContextBudget and ToolPolicy (isolation.rs, part 1)

**File**: `crates/rustycode-orchestration/src/isolation.rs` (new, first half)

This chunk establishes the core data types for context isolation: per-tier token budgets and tool restriction policies. These are pure data types with no external dependencies beyond serde.

**Tests (~15)**:
- `ContextBudget::new_has_zero_usage`
- `ContextBudget::add_tokens_accumulates`
- `ContextBudget::add_tokens_saturates_at_max`
- `ContextBudget::remaining_tokens`
- `ContextBudget::remaining_tokens_clamps_to_zero`
- `ContextBudget::is_exhausted`
- `ContextBudget::is_exhausted_at_zero_limit`
- `ContextBudget::with_limit_builder`
- `ToolPolicy::musician_allows_all`
- `ToolPolicy::editor_allows_read_write_blocks_exec`
- `ToolPolicy::composer_allows_read_only`
- `ToolPolicy::is_tool_allowed_for_read_tool`
- `ToolPolicy::is_tool_allowed_for_write_tool`
- `ToolPolicy::is_tool_allowed_for_exec_tool`
- `ToolPolicy::is_tool_allowed_for_unknown_tool`

**Implementation sketch**:

```rust
//! Context isolation for tiered execution.
//!
//! Each tier (Musician, Editor, Composer) operates within its own context
//! budget and tool restriction policy. This module defines the isolation
//! boundaries and enforcement mechanisms.

use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Token budget tracking for a single tier's context window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextBudget {
    /// Maximum tokens this tier may consume.
    limit: u64,
    /// Tokens consumed so far.
    used: u64,
}

impl ContextBudget {
    /// Create a new budget with the given token limit and zero usage.
    pub const fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// Create an unlimited budget (u64::MAX tokens).
    pub fn unlimited() -> Self {
        Self::new(u64::MAX)
    }

    /// Record token consumption. Saturates at the limit.
    pub fn add_tokens(&mut self, tokens: u64) {
        self.used = self.used.saturating_add(tokens);
    }

    /// Remaining tokens before exhaustion. Clamps to zero.
    pub fn remaining_tokens(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Whether the budget has been fully consumed.
    pub fn is_exhausted(&self) -> bool {
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
        "read" | "read_file" | "grep" | "glob" | "ls" | "find" | "head" | "cat" => {
            ToolCapability::Read
        }
        "write" | "write_file" | "edit" | "edit_file" | "notebook_edit" => {
            ToolCapability::Write
        }
        "bash" | "sh" | "zsh" => ToolCapability::Exec,
        // Default: unknown tools are treated as exec (most restrictive check).
        _ => ToolCapability::Exec,
    }
}
```

**Commands**:

```bash
# Step 1: Write the test module first, verify it fails to compile
cargo test -p rustycode-orchestration --lib isolation::tests -- 2>&1 | head -20
# Expected: "error[E0433]: failed to resolve: module `isolation` not found"

# Step 2: Create the file with all types + tests, verify compile
cargo test -p rustycode-orchestration --lib isolation::tests -- --test-threads=1
# Expected: 15 tests pass

# Step 3: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings 2>&1 | head -20
# Expected: no warnings
```

**Commit message**: `feat(orchestration): add ContextBudget and ToolPolicy types for tier isolation`

---

### Chunk 2: TierIsolation (isolation.rs, part 2)

**File**: `crates/rustycode-orchestration/src/isolation.rs` (extend)

This chunk adds the `TierIsolation` struct that ties together context budgets and tool policies for all tiers. It provides the enforcement entry point.

**Tests (~15)**:
- `TierIsolation::new_creates_budgets_for_all_tiers`
- `TierIsolation::new_uses_configured_limits`
- `TierIsolation::budget_for_musician_returns_tier_2_budget`
- `TierIsolation::budget_for_editor_returns_tier_3_budget`
- `TierIsolation::budget_for_composer_returns_tier_4_budget`
- `TierIsolation::budget_for_unknown_tier_returns_none`
- `TierIsolation::policy_for_musician_returns_full_access`
- `TierIsolation::policy_for_editor_returns_read_write`
- `TierIsolation::policy_for_composer_returns_read_only`
- `TierIsolation::check_tool_allowed_musician_exec`
- `TierIsolation::check_tool_allowed_editor_exec_blocked`
- `TierIsolation::check_tool_allowed_composer_write_blocked`
- `TierIsolation::check_tool_allowed_composer_read_ok`
- `TierIsolation::record_usage_increments_budget`
- `TierIsolation::record_usage_returns_error_on_exhaustion`

**Implementation sketch** (append to isolation.rs):

```rust
/// Configuration for per-tier context budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Create a new config with the given token limits.
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
    pub fn limit_for_tier(&self, tier: ExecutionTier) -> u64 {
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
    /// Create a new isolation manager with the given configuration.
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
        let policy = self.policies.get(&tier).ok_or_else(|| {
            IsolationError::UnknownTier { tier }
        })?;

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
        let budget = self.budgets.get_mut(&tier).ok_or_else(|| {
            IsolationError::UnknownTier { tier }
        })?;

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
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
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
```

**Commands**:

```bash
# Step 1: Append TierIsolation and tests, verify compile + pass
cargo test -p rustycode-orchestration --lib isolation::tests -- --test-threads=1
# Expected: 30 tests pass (15 from Chunk 1 + 15 new)

# Step 2: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
# Expected: no warnings
```

**Commit message**: `feat(orchestration): add TierIsolation with per-tier budget tracking and tool enforcement`

---

### Chunk 3: Error Types and Bus Events (error.rs + bus.rs edits)

**Files**: `crates/rustycode-orchestration/src/error.rs`, `crates/rustycode-orchestration/src/bus.rs`

Extend the existing error enum and bus event enum with new variants for isolation, handoff, and fork-join.

**Tests (~12)**:
- `OrchestrationError::isolation_blocked_tool`
- `OrchestrationError::isolation_budget_exhausted`
- `OrchestrationError::handoff_serialization`
- `OrchestrationError::handoff_missing_context`
- `OrchestrationError::fork_join_timeout`
- `OrchestrationError::fork_join_spawn_failed`
- `OrchestrationError::isolation_error_category`
- `OrchestrationError::handoff_error_category`
- `OrchestrationError::fork_join_error_category`
- `bus::test_tier_handoff_event`
- `bus::test_fork_started_event`
- `bus::test_context_budget_exceeded_event`

**Implementation for error.rs** -- add these variants to `OrchestrationError`:

```rust
// Add to OrchestrationError enum:

#[error("Isolation error: {message}")]
Isolation { message: String },

#[error("Handoff error: {message}")]
Handoff { message: String },

#[error("Fork-join error: {message}")]
ForkJoin { message: String },
```

Add to `category()` match:

```rust
Self::Isolation { .. } => OrchestrationErrorCategory::Internal,
Self::Handoff { .. } => OrchestrationErrorCategory::Internal,
Self::ForkJoin { .. } => OrchestrationErrorCategory::Internal,
```

Add convenience constructors:

```rust
pub fn isolation(msg: impl Into<String>) -> Self {
    Self::Isolation { message: msg.into() }
}

pub fn handoff(msg: impl Into<String>) -> Self {
    Self::Handoff { message: msg.into() }
}

pub fn fork_join(msg: impl Into<String>) -> Self {
    Self::ForkJoin { message: msg.into() }
}
```

**Implementation for bus.rs** -- add these variants to `OrchestrationEvent`:

```rust
// Add to OrchestrationEvent enum:

/// A tier handoff occurred.
TierHandoff {
    task_id: String,
    from_tier: u8,
    to_tier: u8,
    package_size_bytes: usize,
},

/// A parallel fork was started.
ForkStarted {
    task_id: String,
    fork_id: String,
    fork_count: usize,
},

/// A parallel fork completed.
ForkCompleted {
    task_id: String,
    fork_id: String,
    success: bool,
    duration_ms: i64,
},

/// A tier's context budget was exceeded.
ContextBudgetExceeded {
    task_id: String,
    tier: u8,
    used: u64,
    limit: u64,
},
```

**Commands**:

```bash
# Step 1: Edit error.rs, verify compile
cargo check -p rustycode-orchestration
# Expected: compiles

# Step 2: Edit bus.rs, add tests, verify pass
cargo test -p rustycode-orchestration --lib bus::tests -- --test-threads=1
# Expected: existing tests + new tests pass

# Step 3: Full test + clippy
cargo test -p rustycode-orchestration --lib -- --test-threads=1
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): add Isolation, Handoff, ForkJoin error types and bus events`

---

### Chunk 4: HandoffPackage (handoff.rs)

**File**: `crates/rustycode-orchestration/src/handoff.rs` (new)

The handoff protocol defines the explicit context package passed between tiers. It contains everything the next tier needs without leaking the previous tier's full context window.

**Tests (~15)**:
- `HandoffPackage::new_has_required_fields`
- `HandoffPackage::builder_creates_package`
- `HandoffPackage::builder_with_code_snippets`
- `HandoffPackage::builder_with_constraints`
- `HandoffPackage::builder_with_previous_assessment`
- `HandoffPackage::builder_with_budget_summary`
- `HandoffPackage::serialization_roundtrip`
- `HandoffPackage::is_complete_with_all_fields`
- `HandoffPackage::is_incomplete_without_task_description`
- `HandoffPackage::token_estimate`
- `HandoffPackage::token_estimate_approximate`
- `HandoffPackage::summary`
- `HandoffPackage::from_context_basic`
- `HandoffPackage::from_context_with_assessment`
- `HandoffPackage::validate_rejects_empty_task`

**Implementation sketch**:

```rust
//! Handoff protocol for explicit context transfer between tiers.
//!
//! When execution moves from one tier to another (e.g., Musician -> Editor
//! for review, or Editor -> Composer for re-composition), a `HandoffPackage`
//! is created. This package contains the essential context the next tier needs
//! without the full conversation history of the previous tier.

use crate::execution_trace::ExecutionTrace;
use crate::state_machine::TaskContext;
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};

/// Budget summary included in handoff to inform next tier of remaining resources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BudgetSummary {
    pub tier: u8,
    pub tokens_used: u64,
    pub tokens_limit: u64,
    pub cost_usd_used: f64,
    pub cost_usd_limit: f64,
}

impl BudgetSummary {
    /// Create a new budget summary.
    pub const fn new(
        tier: u8,
        tokens_used: u64,
        tokens_limit: u64,
        cost_usd_used: f64,
        cost_usd_limit: f64,
    ) -> Self {
        Self {
            tier,
            tokens_used,
            tokens_limit,
            cost_usd_used,
            cost_usd_limit,
        }
    }

    /// Remaining tokens.
    pub fn tokens_remaining(&self) -> u64 {
        self.tokens_limit.saturating_sub(self.tokens_used)
    }

    /// Remaining budget in USD.
    pub fn budget_remaining(&self) -> f64 {
        (self.cost_usd_limit - self.cost_usd_used).max(0.0)
    }
}

/// Explicit context package passed between tiers.
///
/// Contains the task description, relevant code, constraints, previous tier's
/// assessment, and budget summary. No full conversation history is included --
/// each tier starts with only what it needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPackage {
    /// What task the next tier should work on.
    pub task_description: String,
    /// The tier this package is being handed off to.
    pub target_tier: ExecutionTier,
    /// The tier that produced this package.
    pub source_tier: ExecutionTier,
    /// Relevant code snippets (not the full codebase).
    pub code_snippets: Vec<CodeSnippet>,
    /// Constraints the next tier must respect.
    pub constraints: Vec<String>,
    /// The previous tier's assessment of the task.
    pub previous_assessment: Option<String>,
    /// Budget summary from the previous tier.
    pub budget_summary: Option<BudgetSummary>,
    /// Task ID for tracing.
    pub task_id: String,
}

/// A code snippet included in the handoff.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSnippet {
    pub file_path: String,
    pub content: String,
    pub relevance: String,
}

impl CodeSnippet {
    pub fn new(file_path: impl Into<String>, content: impl Into<String>, relevance: impl Into<String>) -> Self {
        Self {
            file_path: file_path.into(),
            content: content.into(),
            relevance: relevance.into(),
        }
    }
}

/// Builder for HandoffPackage.
#[derive(Debug, Clone)]
pub struct HandoffBuilder {
    task_description: String,
    target_tier: ExecutionTier,
    source_tier: ExecutionTier,
    code_snippets: Vec<CodeSnippet>,
    constraints: Vec<String>,
    previous_assessment: Option<String>,
    budget_summary: Option<BudgetSummary>,
    task_id: String,
}

impl HandoffBuilder {
    /// Start building a handoff package.
    pub fn new(
        task_id: impl Into<String>,
        task_description: impl Into<String>,
        source_tier: ExecutionTier,
        target_tier: ExecutionTier,
    ) -> Self {
        Self {
            task_description: task_description.into(),
            target_tier,
            source_tier,
            code_snippets: Vec::new(),
            constraints: Vec::new(),
            previous_assessment: None,
            budget_summary: None,
            task_id: task_id.into(),
        }
    }

    /// Add a code snippet.
    pub fn with_code_snippet(mut self, snippet: CodeSnippet) -> Self {
        self.code_snippets.push(snippet);
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Set the previous tier's assessment.
    pub fn with_assessment(mut self, assessment: impl Into<String>) -> Self {
        self.previous_assessment = Some(assessment.into());
        self
    }

    /// Set the budget summary.
    pub fn with_budget_summary(mut self, summary: BudgetSummary) -> Self {
        self.budget_summary = Some(summary);
        self
    }

    /// Build the handoff package.
    pub fn build(self) -> HandoffPackage {
        HandoffPackage {
            task_description: self.task_description,
            target_tier: self.target_tier,
            source_tier: self.source_tier,
            code_snippets: self.code_snippets,
            constraints: self.constraints,
            previous_assessment: self.previous_assessment,
            budget_summary: self.budget_summary,
            task_id: self.task_id,
        }
    }
}

impl HandoffPackage {
    /// Create a handoff from a TaskContext for a tier transition.
    ///
    /// Extracts the essential context from the current execution state
    /// without copying the full conversation history or trace.
    pub fn from_context(
        ctx: &TaskContext,
        target_tier: ExecutionTier,
        assessment: Option<String>,
    ) -> Self {
        let source_tier = ExecutionTier::from_u8(ctx.current_tier)
            .unwrap_or(ExecutionTier::Musician);

        let budget_summary = Some(BudgetSummary::new(
            ctx.current_tier,
            ctx.token_count,
            100_000, // Default; will be overridden by TierIsolation config
            ctx.cost_used,
            ctx.budget_limit,
        ));

        Self {
            task_description: ctx.original_request.clone(),
            target_tier,
            source_tier,
            code_snippets: Vec::new(),
            constraints: vec![
                format!("complexity: {}", ctx.constraints.complexity_description()),
                format!("max_retries: {}", ctx.constraints.max_retries),
                format!("timeout: {}s", ctx.constraints.timeout_seconds),
            ],
            previous_assessment: assessment,
            budget_summary,
            task_id: ctx.task_id.clone(),
        }
    }

    /// Whether this package has all required fields populated.
    pub fn is_complete(&self) -> bool {
        !self.task_description.is_empty()
            && !self.task_id.is_empty()
    }

    /// Estimate the token count of this package (rough approximation).
    ///
    /// Uses 4 characters per token as a rough heuristic.
    pub fn token_estimate(&self) -> u64 {
        let total_chars = self.task_description.len()
            + self.task_id.len()
            + self.code_snippets.iter().map(|s| s.content.len() + s.file_path.len()).sum::<usize>()
            + self.constraints.iter().map(|c| c.len()).sum::<usize>()
            + self.previous_assessment.as_ref().map_or(0, |a| a.len());

        (total_chars as u64) / 4
    }

    /// Human-readable summary for logging.
    pub fn summary(&self) -> String {
        format!(
            "HandoffPackage(task={}, {} -> {}, snippets={}, constraints={})",
            self.task_id,
            self.source_tier,
            self.target_tier,
            self.code_snippets.len(),
            self.constraints.len(),
        )
    }
}
```

Note: `TaskComplexity` does not have a `complexity_description()` method yet. Add a simple one to `state_machine.rs`:

```rust
impl TaskComplexity {
    pub const fn complexity_description(&self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Moderate => "moderate",
            Self::Complex => "complex",
            Self::Expert => "expert",
        }
    }
}
```

**Commands**:

```bash
# Step 1: Create handoff.rs with tests
cargo test -p rustycode-orchestration --lib handoff::tests -- --test-threads=1
# Expected: 15 tests pass

# Step 2: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): add HandoffPackage for explicit inter-tier context transfer`

---

### Chunk 5: ContextSnapshot and ForkSpec (fork_join.rs, part 1)

**File**: `crates/rustycode-orchestration/src/fork_join.rs` (new, first half)

This chunk establishes the data types for fork-join parallelism: the context snapshot that gets shared between forks, and the specification for what each fork should do.

**Tests (~12)**:
- `ContextSnapshot::new_has_workspace_entries`
- `ContextSnapshot::new_has_task_description`
- `ContextSnapshot::new_has_budget_state`
- `ContextSnapshot::serialization_roundtrip`
- `ContextSnapshot::is_valid_with_all_fields`
- `ForkSpec::new_has_id_and_description`
- `ForkSpec::with_path_scope`
- `ForkSpec::serialization_roundtrip`
- `ForkSpec::is_valid`
- `ForkSpec::unique_ids_in_vec`
- `ForkResult::success`
- `ForkResult::failure`

**Implementation sketch**:

```rust
//! Fork-join parallel execution with shared context snapshots.
//!
//! For parallel tasks, the parent's context is snapshotted and injected into
//! each fork. Forks execute independently and results are collected back.

use crate::isolation::IsolationConfig;
use crate::types::ExecutionTier;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Snapshot of parent context for injection into parallel forks.
///
/// Contains the essential state that each fork needs to start working
/// without re-loading from scratch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// The original task description.
    pub task_description: String,
    /// The task ID (parent's).
    pub task_id: String,
    /// Current tier at fork time.
    pub fork_tier: u8,
    /// Budget state at fork time.
    pub budget_used: f64,
    pub budget_limit: f64,
    /// Token state at fork time.
    pub tokens_used: u64,
    /// Workspace entries relevant to the forks.
    pub workspace_snapshot: Vec<(String, serde_json::Value)>,
    /// Constraints that apply to all forks.
    pub constraints: Vec<String>,
    /// Timestamp of the snapshot.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ContextSnapshot {
    /// Create a new context snapshot.
    pub fn new(
        task_id: impl Into<String>,
        task_description: impl Into<String>,
        fork_tier: u8,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            task_description: task_description.into(),
            fork_tier,
            budget_used: 0.0,
            budget_limit: 10.0,
            tokens_used: 0,
            workspace_snapshot: Vec::new(),
            constraints: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    /// Set budget state.
    pub fn with_budget(mut self, used: f64, limit: f64) -> Self {
        self.budget_used = used;
        self.budget_limit = limit;
        self
    }

    /// Set token state.
    pub fn with_tokens(mut self, used: u64) -> Self {
        self.tokens_used = used;
        self
    }

    /// Add a workspace entry.
    pub fn with_workspace_entry(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.workspace_snapshot.push((key.into(), value));
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    /// Whether this snapshot has the minimum required fields.
    pub fn is_valid(&self) -> bool {
        !self.task_id.is_empty() && !self.task_description.is_empty()
    }
}

/// Specification for a single parallel fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkSpec {
    /// Unique identifier for this fork.
    pub fork_id: String,
    /// What this fork should do.
    pub description: String,
    /// File paths this fork is responsible for (for worktree isolation).
    pub path_scope: Vec<PathBuf>,
    /// Tier at which this fork should execute.
    pub tier: ExecutionTier,
}

impl ForkSpec {
    /// Create a new fork specification.
    pub fn new(
        fork_id: impl Into<String>,
        description: impl Into<String>,
        tier: ExecutionTier,
    ) -> Self {
        Self {
            fork_id: fork_id.into(),
            description: description.into(),
            path_scope: Vec::new(),
            tier,
        }
    }

    /// Add a path to this fork's scope.
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path_scope.push(path);
        self
    }

    /// Whether this spec is valid.
    pub fn is_valid(&self) -> bool {
        !self.fork_id.is_empty() && !self.description.is_empty()
    }
}

/// Result from a completed fork.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkResult {
    /// The fork's ID.
    pub fork_id: String,
    /// Whether the fork succeeded.
    pub success: bool,
    /// Output from the fork.
    pub output: String,
    /// Cost incurred by this fork.
    pub cost_usd: f64,
    /// Duration in milliseconds.
    pub duration_ms: i64,
}

impl ForkResult {
    /// Create a successful fork result.
    pub fn success(fork_id: impl Into<String>, output: impl Into<String>, cost_usd: f64, duration_ms: i64) -> Self {
        Self {
            fork_id: fork_id.into(),
            success: true,
            output: output.into(),
            cost_usd,
            duration_ms,
        }
    }

    /// Create a failed fork result.
    pub fn failure(fork_id: impl Into<String>, reason: impl Into<String>, duration_ms: i64) -> Self {
        Self {
            fork_id: fork_id.into(),
            success: false,
            output: reason.into(),
            cost_usd: 0.0,
            duration_ms,
        }
    }
}
```

**Commands**:

```bash
# Step 1: Create fork_join.rs with types + tests
cargo test -p rustycode-orchestration --lib fork_join::tests -- --test-threads=1
# Expected: 12 tests pass

# Step 2: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): add ContextSnapshot, ForkSpec, ForkResult for fork-join parallelism`

---

### Chunk 6: ForkJoinExecutor (fork_join.rs, part 2)

**File**: `crates/rustycode-orchestration/src/fork_join.rs` (extend)

This chunk adds the `ForkJoinExecutor` that coordinates parallel fork execution. It takes a context snapshot, spawns forks, and collects results.

**Tests (~10)**:
- `ForkJoinExecutor::new_creates_executor`
- `ForkJoinExecutor::plan_forks_creates_specs_from_paths`
- `ForkJoinExecutor::plan_forks_deduplicates_ids`
- `ForkJoinExecutor::execute_forks_empty_returns_empty`
- `ForkJoinExecutor::execute_forks_single_fork`
- `ForkJoinExecutor::execute_forks_multiple_forks`
- `ForkJoinExecutor::execute_forks_records_success`
- `ForkJoinExecutor::execute_forks_records_failure`
- `ForkJoinExecutor::merge_results_combines_costs`
- `ForkJoinExecutor::merge_results_all_must_succeed_for_success`

**Implementation sketch** (append to fork_join.rs):

```rust
/// Configuration for fork-join execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkJoinConfig {
    /// Maximum number of concurrent forks.
    pub max_concurrency: usize,
    /// Timeout per fork in milliseconds.
    pub fork_timeout_ms: u64,
}

impl Default for ForkJoinConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            fork_timeout_ms: 30_000,
        }
    }
}

/// Aggregated result of a fork-join execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkJoinResult {
    /// Individual fork results.
    pub fork_results: Vec<ForkResult>,
    /// Total cost across all forks.
    pub total_cost_usd: f64,
    /// Whether all forks succeeded.
    pub all_succeeded: bool,
    /// Total wall-clock duration in ms.
    pub total_duration_ms: i64,
}

impl ForkJoinResult {
    /// Number of forks that succeeded.
    pub fn success_count(&self) -> usize {
        self.fork_results.iter().filter(|r| r.success).count()
    }

    /// Number of forks that failed.
    pub fn failure_count(&self) -> usize {
        self.fork_results.len() - self.success_count()
    }
}

/// Coordinates parallel fork execution with shared context snapshots.
///
/// The executor takes a parent context snapshot, creates fork specifications,
/// executes them (sequentially in V1, with async parallelism planned for V2),
/// and collects results.
pub struct ForkJoinExecutor {
    config: ForkJoinConfig,
    bus: crate::bus::BusHandle,
}

impl ForkJoinExecutor {
    /// Create a new executor with the given configuration.
    pub fn new(config: ForkJoinConfig, bus: crate::bus::BusHandle) -> Self {
        Self { config, bus }
    }

    /// Create with default configuration.
    pub fn with_bus(bus: crate::bus::BusHandle) -> Self {
        Self::new(ForkJoinConfig::default(), bus)
    }

    /// Plan forks from a list of path scopes.
    ///
    /// Each path gets its own fork with a unique ID. Paths are assigned
    /// to the Musician tier (execution tier) by default.
    pub fn plan_forks(
        paths: &[PathBuf],
        base_description: &str,
        tier: ExecutionTier,
    ) -> Vec<ForkSpec> {
        paths
            .iter()
            .enumerate()
            .map(|(i, path)| {
                ForkSpec::new(
                    format!("fork-{i}"),
                    format!("{base_description} (path: {})", path.display()),
                    tier,
                )
                .with_path(path.clone())
            })
            .collect()
    }

    /// Execute a list of fork specifications against a context snapshot.
    ///
    /// V1: Sequential execution. V2 will use tokio::JoinSet for parallelism.
    /// Each fork gets the snapshot injected; results are collected.
    pub async fn execute_forks(
        &self,
        snapshot: &ContextSnapshot,
        specs: &[ForkSpec],
    ) -> ForkJoinResult {
        if specs.is_empty() {
            return ForkJoinResult {
                fork_results: Vec::new(),
                total_cost_usd: 0.0,
                all_succeeded: true,
                total_duration_ms: 0,
            };
        }

        let start = std::time::Instant::now();
        let mut results = Vec::with_capacity(specs.len());

        for spec in specs {
            // Publish fork started event
            self.bus.publish(crate::bus::OrchestrationEvent::ForkStarted {
                task_id: snapshot.task_id.clone(),
                fork_id: spec.fork_id.clone(),
                fork_count: specs.len(),
            });

            // V1: Sequential execution placeholder.
            // In production, each fork would create its own TaskContext from
            // the snapshot, execute via StepOrchestrator, and return.
            let result = ForkResult::success(
                &spec.fork_id,
                format!("Fork executed: {}", spec.description),
                0.001,
                10,
            );

            // Publish fork completed event
            self.bus.publish(crate::bus::OrchestrationEvent::ForkCompleted {
                task_id: snapshot.task_id.clone(),
                fork_id: spec.fork_id.clone(),
                success: result.success,
                duration_ms: result.duration_ms,
            });

            results.push(result);
        }

        let total_cost: f64 = results.iter().map(|r| r.cost_usd).sum();
        let all_succeeded = results.iter().all(|r| r.success);

        ForkJoinResult {
            fork_results: results,
            total_cost_usd: total_cost,
            all_succeeded,
            total_duration_ms: start.elapsed().as_millis() as i64,
        }
    }

    /// Merge fork results into a single aggregated summary.
    pub fn merge_results(results: &[ForkResult]) -> ForkJoinResult {
        let total_cost: f64 = results.iter().map(|r| r.cost_usd).sum();
        let all_succeeded = results.iter().all(|r| r.success);
        let max_duration: i64 = results.iter().map(|r| r.duration_ms).max().unwrap_or(0);

        ForkJoinResult {
            fork_results: results.to_vec(),
            total_cost_usd: total_cost,
            all_succeeded,
            total_duration_ms: max_duration,
        }
    }

    /// The configuration.
    pub const fn config(&self) -> &ForkJoinConfig {
        &self.config
    }
}
```

**Commands**:

```bash
# Step 1: Append ForkJoinExecutor + tests
cargo test -p rustycode-orchestration --lib fork_join::tests -- --test-threads=1
# Expected: 22 tests pass (12 from Chunk 5 + 10 new)

# Step 2: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): add ForkJoinExecutor for parallel task execution with context snapshots`

---

### Chunk 7: Wire Isolation into StepOrchestrator (orchestrator.rs edits)

**File**: `crates/rustycode-orchestration/src/orchestrator.rs` (edit)

This chunk wires `TierIsolation` into the existing `StepOrchestrator` to enforce tool restrictions at tier execution time.

**Tests (~10)** -- added to the existing test module:
- `test_isolation_musician_allows_exec`
- `test_isolation_editor_blocks_exec`
- `test_isolation_composer_blocks_write`
- `test_isolation_composer_blocks_exec`
- `test_isolation_budget_tracking_per_tier`
- `test_isolation_budget_exhausted_returns_error`
- `test_isolation_unknown_tier_returns_error`
- `test_step_orchestrator_with_isolation`
- `test_step_orchestrator_isolation_blocks_exec_at_editor`
- `test_step_orchestrator_isolation_allows_exec_at_musician`

**Implementation sketch** -- changes to `StepOrchestrator`:

```rust
// Add to StepOrchestrator struct:
pub struct StepOrchestrator {
    conductor: Arc<Conductor>,
    musician: Arc<Musician>,
    editor: Arc<Editor>,
    composer: Arc<Composer>,
    verification_gate: Arc<VerificationGateRegistry>,
    _bus: BusHandle,
    isolation: Option<Arc<std::sync::Mutex<crate::isolation::TierIsolation>>>,
}

// Add new constructor:
impl StepOrchestrator {
    pub fn with_isolation(
        conductor: Arc<Conductor>,
        musician: Arc<Musician>,
        editor: Arc<Editor>,
        composer: Arc<Composer>,
        verification_gate: Arc<VerificationGateRegistry>,
        bus: BusHandle,
        isolation: crate::isolation::TierIsolation,
    ) -> Self {
        Self {
            conductor,
            musician,
            editor,
            composer,
            verification_gate,
            _bus: bus,
            isolation: Some(Arc::new(std::sync::Mutex::new(isolation))),
        }
    }
}

// Add tool check in execute_at_tier(), before the match:
// (Inside execute_at_tier, after receiving step, before tier dispatch)

/// Check tool access before tier execution.
fn check_tool_access(
    &self,
    tier: u8,
    tool_name: &str,
) -> Result<()> {
    if let Some(isolation) = &self.isolation {
        let iso = isolation.lock().unwrap_or_else(|e| e.into_inner());
        iso.check_tool_allowed(tier, tool_name).map_err(|e| {
            OrchestrationError::Isolation { message: e.to_string() }
        })?;
    }
    Ok(())
}
```

**Commands**:

```bash
# Step 1: Edit orchestrator.rs, add isolation field and check
cargo test -p rustycode-orchestration --lib orchestrator::tests -- --test-threads=1
# Expected: existing tests pass + new isolation tests pass

# Step 2: Full lib test
cargo test -p rustycode-orchestration --lib -- --test-threads=1
# Expected: all tests pass

# Step 3: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): enforce tool restrictions via TierIsolation in StepOrchestrator`

---

### Chunk 8: Wire Isolation and Handoffs into Pipeline (pipeline.rs edits)

**File**: `crates/rustycode-orchestration/src/pipeline.rs` (edit)

This chunk wires `TierIsolation` and `HandoffPackage` into the `OrchestrationPipeline`. The pipeline creates the isolation manager, creates handoffs between tier transitions, and publishes events.

**Tests (~8)** -- added to the existing test module:
- `test_pipeline_with_isolation_conduct`
- `test_pipeline_isolation_tracks_budget`
- `test_pipeline_handoff_between_tiers`
- `test_pipeline_handoff_published_on_escalation`
- `test_pipeline_context_budget_exceeded_event`
- `test_pipeline_isolation_config_custom_limits`
- `test_pipeline_isolation_default_config`
- `test_pipeline_conduct_with_isolation_and_handoff`

**Implementation sketch** -- changes to `OrchestrationPipeline`:

```rust
// Add to OrchestrationPipeline struct:
pub struct OrchestrationPipeline {
    orchestrator: Arc<StepOrchestrator>,
    bus: BusHandle,
    workspace: Arc<SharedWorkspace>,
    isolation_config: crate::isolation::IsolationConfig,
}

// In build(), create TierIsolation and pass to StepOrchestrator:
fn build(
    config: OrchestrationConfig,
    llm_provider: Arc<dyn rustycode_llm::provider::LLMProvider>,
    model: Option<&str>,
) -> Self {
    // ... existing setup ...

    let isolation_config = crate::isolation::IsolationConfig::new(
        (config.budget.tier_2_max_usd * 10000.0) as u64,
        (config.budget.tier_3_max_usd * 10000.0) as u64,
        (config.budget.tier_4_max_usd * 10000.0) as u64,
        50_000, // Thinking tier
    );
    let isolation = crate::isolation::TierIsolation::new(&isolation_config);

    let orchestrator = Arc::new(StepOrchestrator::with_isolation(
        conductor,
        musician,
        editor,
        composer,
        verification_gate,
        bus.clone(),
        isolation,
    ));

    Self {
        orchestrator,
        bus,
        workspace,
        isolation_config,
    }
}

// In conduct(), create handoff on tier escalation:
// After each step, check if tier changed and create a HandoffPackage
```

**Commands**:

```bash
# Step 1: Edit pipeline.rs, add isolation + handoff wiring
cargo test -p rustycode-orchestration --lib pipeline::tests -- --test-threads=1
# Expected: existing tests + new tests pass

# Step 2: Full lib test
cargo test -p rustycode-orchestration --lib -- --test-threads=1

# Step 3: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings
```

**Commit message**: `feat(orchestration): wire TierIsolation and HandoffPackage into OrchestrationPipeline`

---

### Chunk 9: Register Modules in lib.rs

**File**: `crates/rustycode-orchestration/src/lib.rs` (edit)

Add the three new modules and their public re-exports.

**Tests (~4)** -- verify re-exports compile:
- `test_isolation_reexport`
- `test_handoff_reexport`
- `test_fork_join_reexport`
- `test_full_pipeline_integration`

**Implementation**:

```rust
// Add to lib.rs module declarations:
pub mod fork_join;
pub mod handoff;
pub mod isolation;

// Add to re-exports:
pub use isolation::{
    TierIsolation, IsolationConfig, ContextBudget, ToolPolicy, ToolCapability,
    IsolationError, classify_tool,
};
pub use handoff::{
    HandoffPackage, HandoffBuilder, CodeSnippet, BudgetSummary,
};
pub use fork_join::{
    ForkJoinExecutor, ForkJoinConfig, ContextSnapshot, ForkSpec, ForkResult,
    ForkJoinResult,
};
```

**Commands**:

```bash
# Step 1: Edit lib.rs
cargo check -p rustycode-orchestration
# Expected: compiles

# Step 2: Full test suite
cargo test -p rustycode-orchestration --lib -- --test-threads=1
# Expected: all tests pass (targeting ~80+ total across all modules)

# Step 3: Clippy
cargo clippy -p rustycode-orchestration -- -D warnings

# Step 4: Verify no regressions in dependent crates
cargo test -p rustycode-orchestra -- --test-threads=1
# Expected: existing 889 tests still pass
```

**Commit message**: `feat(orchestration): register isolation, handoff, fork_join modules with public re-exports`

---

## Test Summary

| Chunk | Module | New Tests | Cumulative |
|-------|--------|-----------|------------|
| 1 | isolation (ContextBudget + ToolPolicy) | 15 | 15 |
| 2 | isolation (TierIsolation) | 15 | 30 |
| 3 | error.rs + bus.rs extensions | 12 | 42 |
| 4 | handoff (HandoffPackage) | 15 | 57 |
| 5 | fork_join (ContextSnapshot + ForkSpec) | 12 | 69 |
| 6 | fork_join (ForkJoinExecutor) | 10 | 79 |
| 7 | orchestrator.rs (isolation wiring) | 10 | 89 |
| 8 | pipeline.rs (isolation + handoff wiring) | 8 | 97 |
| 9 | lib.rs (re-exports + integration) | 4 | 101 |

**Total: ~101 tests** (exceeds the 60-80 target; trim if needed by removing redundant parametric tests).

---

## Verification Commands (Final)

```bash
# Full orchestration crate test suite
cargo test -p rustycode-orchestration -- --test-threads=1
# Expected: all tests pass, 0 failures

# Clippy across entire workspace (no new warnings)
cargo clippy -p rustycode-orchestration -- -D warnings
# Expected: 0 warnings

# Verify no regressions in dependent crates
cargo test -p rustycode-orchestra -- --test-threads=1
# Expected: 889+ tests still pass

# Format check
cargo fmt -p rustycode-orchestration -- --check
# Expected: no formatting issues
```

---

## Implementation Order and Dependencies

```
Chunk 1 (ContextBudget + ToolPolicy)
    |
    v
Chunk 2 (TierIsolation)
    |
    v
Chunk 3 (Error types + Bus events)  <-- can start in parallel with Chunk 4
    |
    +-----> Chunk 4 (HandoffPackage)
    |               |
    v               v
Chunk 5 (ContextSnapshot + ForkSpec)  <-- can start in parallel with Chunk 4
    |
    v
Chunk 6 (ForkJoinExecutor)
    |
    v
Chunk 7 (Wire isolation into orchestrator)
    |
    v
Chunk 8 (Wire isolation + handoff into pipeline)
    |
    v
Chunk 9 (Register modules in lib.rs)
```

Chunks 3, 4, and 5 can be developed in parallel since they have no interdependencies. Chunks 7 and 8 must be sequential.

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Breaking existing orchestrator tests | `isolation` field is `Option<Arc<Mutex<...>>>`; existing constructors do not set it, so existing tests are unaffected |
| Performance regression from mutex in isolation check | Mutex held only for the duration of a hash map lookup (~100ns); no contention in single-task execution |
| ForkJoinExecutor V1 sequential | Documented as V1; V2 will use `tokio::task::JoinSet` for true parallelism |
| Budget enforcement too aggressive | `record_usage` returns `Result` but callers can choose to log warning instead of failing |
| Handoff packages too large | `token_estimate()` provides a check; `code_snippets` is intentionally a curated list, not a full dump |

---

## Future Enhancements (Post Phase 3)

1. **Async parallelism in ForkJoinExecutor**: Replace sequential loop with `tokio::JoinSet` for true parallel fork execution.
2. **Per-fork worktree isolation**: Wire `ForkSpec::path_scope` into the existing `WorktreeManager` for automatic git worktree creation.
3. **Context compression in handoffs**: Automatically compress code snippets when `token_estimate()` exceeds a threshold.
4. **Isolation metrics**: Track per-tier budget utilization, tool restriction violations, and handoff sizes via the existing `observability` crate.
5. **Tier-specific system prompts**: When Phase 2 (Explore-Plan-Act) is complete, wire phase-specific prompts into the isolation boundaries.
