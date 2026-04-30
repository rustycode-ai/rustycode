# Phase 5: Progressive Tooling + Hooks -- TDD Implementation Plan

**Date**: 2026-04-25
**Duration**: 2-3 weeks
**Dependency**: Phase 3 (tool restriction needs isolated agents)
**Status**: 🟢 COMPLETE
**See Also**: [Generative Programmer analysis](2026-04-25-generative-programmer-real-analysis.md#phase-status-map)

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [File Structure](#2-file-structure)
3. [Chunk 1: Tool Tiers](#3-chunk-1-tool-tiers)
4. [Chunk 2: Expanded Hook Points](#4-chunk-2-expanded-hook-points)
5. [Chunk 3: Per-Skill Tool Scoping](#5-chunk-3-per-skill-tool-scoping)
6. [Chunk 4: Context Budget for Skills](#6-chunk-4-context-budget-for-skills)
7. [Chunk 5: Integration Wiring](#7-chunk-5-integration-wiring)
8. [Verification Commands](#8-verification-commands)
9. [Success Metrics Checklist](#9-success-metrics-checklist)

---

## 1. Architecture Overview

Phase 5 adds four capabilities across three crates:

```
rustycode-tools-api          rustycode-guard           rustycode-skill
    |                             |                          |
    +-- tiers.rs (new)           +-- hooks_expanded.rs      +-- scoping.rs (new)
    |   ToolTier enum             |   LifecycleHook enum     |   SkillToolScope
    |   ToolActivationManager     |   ExpandedHookDispatcher |   resolve_allowed_tools()
    |   UsageTracker              |   HookHandler trait      |
    |                            |   LifecycleEvent          +-- budget.rs (new)
    |                            |                          |   ContextBudget
    +-- registry.rs (modify)     +-- lib.rs (modify)        |   SkillBudgetEntry
        tier-aware listing           re-export expanded      |   BudgetEnforcer
```

Data flow:

```
Session starts
  --> ToolActivationManager activates Default tier
  --> Skill activates
      --> scoping::resolve_allowed_tools(skill_yaml) -> Vec<String>
      --> ToolActivationManager.intersect_scope(allowed_tools)
  --> Each tool call
      --> guard::hooks_expanded dispatches PreToolUse
      --> ActivationManager.is_active(tool_name) check
      --> Tool executes
      --> guard::hooks_expanded dispatches PostToolUse
  --> UsageTracker.record(tool_name, result)
  --> BudgetEnforcer.check(skill_context_tokens)
      --> if over budget, evict lowest-priority skill
```

## Implementation Status

Completed in this pass:

- `crates/rustycode-tools-api/src/tiers.rs` now provides `ToolTier`, tier tool sets, and a `ToolActivationManager`.
- `crates/rustycode-tools-api/src/tool_selection.rs` now tracks per-tool invocation counts and success rates.
- `crates/rustycode-tools-registry/src/registry.rs` now filters registered tools by tier.
- `crates/rustycode-guard/src/hooks_expanded.rs` now provides expanded lifecycle hooks and a dispatcher.
- `crates/rustycode-skill/src/scoping.rs` now resolves per-skill tool scope from `SkillDefinition`.
- `crates/rustycode-skill/src/budget.rs` now tracks context budget and priority-based eviction.

Still open:

- Higher-level wiring that threads the new hook and scope helpers through the orchestration path.

---

## 2. File Structure

New files:

```
crates/rustycode-tools-api/src/tiers.rs          # Tool tiers and activation manager
crates/rustycode-guard/src/hooks_expanded.rs      # 20+ lifecycle hook points
crates/rustycode-skill/src/scoping.rs             # Per-skill tool scoping
crates/rustycode-skill/src/budget.rs              # Context budget tracking
```

Modified files:

```
crates/rustycode-tools-api/src/lib.rs             # Re-export tiers module
crates/rustycode-guard/src/lib.rs                 # Re-export hooks_expanded module
crates/rustycode-skill/src/lib.rs                 # Re-export scoping + budget modules
crates/rustycode-tools-registry/src/registry.rs   # Tier-aware tool listing
```

---

## 3. Chunk 1: Tool Tiers

**Target**: `crates/rustycode-tools-api/src/tiers.rs` (~300 lines)
**Tests**: 20 tests
**Estimated chunk size**: ~600 lines (impl + tests)

### 3.1 ToolTier Enum -- TDD Task 1

**File**: `crates/rustycode-tools-api/src/tiers.rs`

Write the failing test first:

```rust
// In tiers.rs, #[cfg(test)] mod tests

#[test]
fn tool_tier_default_is_default() {
    assert_eq!(ToolTier::default(), ToolTier::Default);
}

#[test]
fn tool_tier_ordering() {
    assert!(ToolTier::Default < ToolTier::Extended);
    assert!(ToolTier::Extended < ToolTier::Full);
}
```

Then write minimal implementation:

```rust
// In tiers.rs

/// Tool activation tier. Tools are loaded progressively based on task demands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolTier {
    /// Core tools always available: Read, Edit, Write, Bash, Grep, Glob
    Default,
    /// Additional tools activated on demand: WebFetch, NotebookEdit, LSP tools
    Extended,
    /// All registered tools available
    Full,
}

impl Default for ToolTier {
    fn default() -> Self {
        Self::Default
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-api -- tiers::tests::tool_tier
# Expected: 2 tests PASS
```

### 3.2 Default Tool Set -- TDD Task 2

Write the failing test:

```rust
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
```

Then write minimal implementation:

```rust
use std::collections::HashSet;

/// The default tool set -- always available in every session.
pub fn default_tool_set() -> HashSet<&'static str> {
    let set: HashSet<&'static str> = [
        "read_file",
        "edit_file",
        "write_file",
        "bash",
        "grep",
        "glob",
    ]
    .into();
    set
}

/// The extended tool set -- activated when the task requires more capabilities.
pub fn extended_tool_set() -> HashSet<&'static str> {
    let set: HashSet<&'static str> = [
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
    .into();
    set
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-api -- tiers::tests::default_tools
cargo test -p rustycode-tools-api -- tiers::tests::extended_tools
# Expected: 2 tests PASS
```

### 3.3 UsageTracker -- TDD Task 3

Write the failing tests:

```rust
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
fn usage_tracker_most_used_tools() {
    let mut tracker = UsageTracker::new();
    tracker.record("bash", true);
    tracker.record("bash", true);
    tracker.record("bash", true);
    tracker.record("read_file", true);
    tracker.record("read_file", true);

    let top = tracker.most_used(2);
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].0, "bash");
    assert_eq!(top[0].1, 3);
    assert_eq!(top[1].0, "read_file");
    assert_eq!(top[1].1, 2);
}

#[test]
fn usage_tracker_reset_clears_all() {
    let mut tracker = UsageTracker::new();
    tracker.record("bash", true);
    tracker.reset();
    assert_eq!(tracker.invocation_count("bash"), 0);
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
```

Then write minimal implementation:

```rust
/// Per-session usage tracking for tool invocations.
#[derive(Debug, Clone, Default)]
pub struct UsageTracker {
    entries: HashMap<String, ToolUsageEntry>,
}

#[derive(Debug, Clone)]
struct ToolUsageEntry {
    invocations: u64,
    successes: u64,
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
        self.entries
            .get(tool_name)
            .map_or(0, |e| e.invocations)
    }

    /// Success rate for a tool (0.0 to 1.0).
    pub fn success_rate(&self, tool_name: &str) -> f64 {
        self.entries
            .get(tool_name)
            .map_or(0.0, |e| {
                if e.invocations == 0 {
                    return 0.0;
                }
                f64::from(e.successes) / f64::from(e.invocations)
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

impl Default for ToolUsageEntry {
    fn default() -> Self {
        Self {
            invocations: 0,
            successes: 0,
        }
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-api -- tiers::tests::usage_tracker
# Expected: 6 tests PASS
```

### 3.4 ToolActivationManager -- TDD Task 4

Write the failing tests:

```rust
#[test]
fn activation_manager_starts_at_default_tier() {
    let manager = ToolActivationManager::new();
    assert_eq!(manager.current_tier(), ToolTier::Default);
    assert!(manager.is_active("read_file"));
    assert!(manager.is_active("bash"));
    assert!(!manager.is_active("web_fetch"));
}

#[test]
fn activation_manager_promote_to_extended() {
    let mut manager = ToolActivationManager::new();
    manager.promote(ToolTier::Extended);
    assert_eq!(manager.current_tier(), ToolTier::Extended);
    assert!(manager.is_active("web_fetch"));
    assert!(manager.is_active("read_file")); // still active
}

#[test]
fn activation_manager_promote_to_full() {
    let mut manager = ToolActivationManager::new();
    manager.promote(ToolTier::Full);
    assert!(manager.is_active("any_custom_tool"));
    assert!(manager.is_active("web_fetch"));
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
    assert!(tools.contains(&"read_file"));
    assert!(tools.contains(&"bash"));
    assert!(!tools.contains(&"web_fetch"));
}

#[test]
fn activation_manager_with_scope_intersection() {
    let mut manager = ToolActivationManager::new();
    // Skill allows only read_file and grep
    let scope = vec!["read_file".to_string(), "grep".to_string()];
    manager.intersect_scope(&scope);
    assert!(manager.is_active("read_file"));
    assert!(manager.is_active("grep"));
    assert!(!manager.is_active("bash")); // restricted by scope
}

#[test]
fn activation_manager_scope_with_extended_tier() {
    let mut manager = ToolActivationManager::new();
    manager.promote(ToolTier::Extended);
    let scope = vec![
        "read_file".to_string(),
        "web_fetch".to_string(),
        "bash".to_string(),
    ];
    manager.intersect_scope(&scope);
    assert!(manager.is_active("read_file"));
    assert!(manager.is_active("web_fetch")); // in both extended tier and scope
    assert!(!manager.is_active("bash")); // bash not in scope
}

#[test]
fn activation_manager_clear_scope_restores_tier() {
    let mut manager = ToolActivationManager::new();
    let scope = vec!["read_file".to_string()];
    manager.intersect_scope(&scope);
    assert!(!manager.is_active("bash"));
    manager.clear_scope();
    assert!(manager.is_active("bash")); // restored
}
```

Then write minimal implementation:

```rust
/// Manages which tools are currently active based on tier and skill scoping.
pub struct ToolActivationManager {
    tier: ToolTier,
    /// When set, only these tools are active (intersection with tier tools).
    scope: Option<HashSet<String>>,
}

impl ToolActivationManager {
    pub fn new() -> Self {
        Self {
            tier: ToolTier::Default,
            scope: None,
        }
    }

    /// Current activation tier.
    pub fn current_tier(&self) -> ToolTier {
        self.tier
    }

    /// Promote to a higher tier. Demotion is a no-op.
    pub fn promote(&mut self, tier: ToolTier) {
        if tier > self.tier {
            self.tier = tier;
        }
    }

    /// Check if a specific tool is active.
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

        // If a skill scope is active, further restrict
        if let Some(scope) = &self.scope {
            return scope.contains(tool_name);
        }

        true
    }

    /// Get a snapshot of currently active tool names.
    pub fn active_tools(&self) -> Vec<&str> {
        let base: HashSet<&str> = match self.tier {
            ToolTier::Default => default_tool_set(),
            ToolTier::Extended => {
                let mut s = default_tool_set();
                s.extend(extended_tool_set());
                s
            }
            ToolTier::Full => return vec![], // Full tier has too many to enumerate
        };

        let mut result: Vec<&str> = if let Some(scope) = &self.scope {
            base.into_iter().filter(|t| scope.contains(*t)).collect()
        } else {
            base.into_iter().collect()
        };
        result.sort();
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
}

impl Default for ToolActivationManager {
    fn default() -> Self {
        Self::new()
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-api -- tiers::tests::activation_manager
# Expected: 8 tests PASS
```

### 3.5 Wire tiers module into tools-api -- TDD Task 5

Modify `crates/rustycode-tools-api/src/lib.rs`:

```rust
// Add after existing module declarations:
pub mod tiers;

// Add to re-exports:
pub use tiers::{
    default_tool_set, extended_tool_set, ToolActivationManager, ToolTier, UsageTracker,
};
```

**Verify**:
```bash
cargo build -p rustycode-tools-api
cargo test -p rustycode-tools-api -- tiers
# Expected: 18 tests PASS (2 + 2 + 6 + 8)
```

**Commit**: `feat: tool activation tiers with usage tracking (20 tests)`

---

## 4. Chunk 2: Expanded Hook Points

**Target**: `crates/rustycode-guard/src/hooks_expanded.rs` (~400 lines)
**Tests**: 22 tests
**Estimated chunk size**: ~800 lines

### 4.1 LifecycleHook Enum -- TDD Task 6

Write the failing tests:

```rust
#[test]
fn lifecycle_hook_has_22_variants() {
    let hooks = LifecycleHook::all();
    assert!(hooks.len() >= 20, "Expected 20+ hook points, got {}", hooks.len());
}

#[test]
fn lifecycle_hook_event_type_strings_are_unique() {
    let hooks = LifecycleHook::all();
    let types: Vec<&str> = hooks.iter().map(|h| h.event_type()).collect();
    let unique: std::collections::HashSet<&str> = types.iter().copied().collect();
    assert_eq!(types.len(), unique.len(), "event_type strings must be unique");
}

#[test]
fn lifecycle_hook_is_pre_or_post() {
    assert!(LifecycleHook::PreToolUse.is_pre());
    assert!(!LifecycleHook::PreToolUse.is_post());
    assert!(LifecycleHook::PostToolUse.is_post());
    assert!(!LifecycleHook::PostToolUse.is_pre());
    assert!(LifecycleHook::CwdChanged.is_post()); // observation hooks are post
}
```

Then write minimal implementation:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;

/// Lifecycle hook points for the expanded hook system.
///
/// Each variant corresponds to a specific moment in the agent lifecycle
/// where custom logic can be injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleHook {
    // -- Tool lifecycle (existing + expanded) --
    /// Before a tool is executed
    PreToolUse,
    /// After a tool has been executed
    PostToolUse,
    /// Tool execution failed
    ToolError,

    // -- Session lifecycle --
    /// Session has been created
    SessionStart,
    /// Session is ending normally
    SessionEnd,
    /// Session ended due to error
    SessionError,

    // -- Agent lifecycle --
    /// Working directory changed
    CwdChanged,
    /// Sub-agent spawned
    SubagentStart,
    /// Sub-agent completed
    SubagentEnd,

    // -- Plan lifecycle --
    /// Plan execution starting
    PlanStart,
    /// Plan execution completed
    PlanEnd,

    // -- Context lifecycle --
    /// Error recovery initiated
    ErrorRecovery,
    /// Context window switched (e.g., compaction)
    ContextSwitch,

    // -- Skill lifecycle --
    /// Skill about to be activated
    SkillActivate,
    /// Skill deactivated
    SkillDeactivate,

    // -- Permission lifecycle --
    /// Permission check requested
    PermissionCheck,
    /// Permission granted
    PermissionGranted,
    /// Permission denied
    PermissionDenied,

    // -- Tier lifecycle --
    /// Tool tier promoted
    TierPromoted,
    /// Tool tier scope changed
    TierScopeChanged,

    // -- Budget lifecycle --
    /// Context budget warning (>80% used)
    BudgetWarning,
    /// Skill evicted due to budget pressure
    BudgetEviction,
}

impl LifecycleHook {
    /// All defined hook points.
    pub fn all() -> &'static [LifecycleHook] {
        &[
            LifecycleHook::PreToolUse,
            LifecycleHook::PostToolUse,
            LifecycleHook::ToolError,
            LifecycleHook::SessionStart,
            LifecycleHook::SessionEnd,
            LifecycleHook::SessionError,
            LifecycleHook::CwdChanged,
            LifecycleHook::SubagentStart,
            LifecycleHook::SubagentEnd,
            LifecycleHook::PlanStart,
            LifecycleHook::PlanEnd,
            LifecycleHook::ErrorRecovery,
            LifecycleHook::ContextSwitch,
            LifecycleHook::SkillActivate,
            LifecycleHook::SkillDeactivate,
            LifecycleHook::PermissionCheck,
            LifecycleHook::PermissionGranted,
            LifecycleHook::PermissionDenied,
            LifecycleHook::TierPromoted,
            LifecycleHook::TierScopeChanged,
            LifecycleHook::BudgetWarning,
            LifecycleHook::BudgetEviction,
        ]
    }

    /// Dot-separated event type string (e.g., "tool.pre").
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::PreToolUse => "tool.pre",
            Self::PostToolUse => "tool.post",
            Self::ToolError => "tool.error",
            Self::SessionStart => "session.start",
            Self::SessionEnd => "session.end",
            Self::SessionError => "session.error",
            Self::CwdChanged => "cwd.changed",
            Self::SubagentStart => "subagent.start",
            Self::SubagentEnd => "subagent.end",
            Self::PlanStart => "plan.start",
            Self::PlanEnd => "plan.end",
            Self::ErrorRecovery => "error.recovery",
            Self::ContextSwitch => "context.switch",
            Self::SkillActivate => "skill.activate",
            Self::SkillDeactivate => "skill.deactivate",
            Self::PermissionCheck => "permission.check",
            Self::PermissionGranted => "permission.granted",
            Self::PermissionDenied => "permission.denied",
            Self::TierPromoted => "tier.promoted",
            Self::TierScopeChanged => "tier.scope_changed",
            Self::BudgetWarning => "budget.warning",
            Self::BudgetEviction => "budget.eviction",
        }
    }

    /// Whether this hook fires before an action (pre-hook).
    pub const fn is_pre(&self) -> bool {
        matches!(
            self,
            Self::PreToolUse
                | Self::SkillActivate
                | Self::PermissionCheck
                | Self::PlanStart
                | Self::SubagentStart
        )
    }

    /// Whether this hook fires after an action (post-hook).
    pub const fn is_post(&self) -> bool {
        !self.is_pre()
    }
}

impl fmt::Display for LifecycleHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.event_type())
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_hook
# Expected: 3 tests PASS
```

### 4.2 LifecycleEvent -- TDD Task 7

Write the failing tests:

```rust
#[test]
fn lifecycle_event_carries_hook_and_metadata() {
    let event = LifecycleEvent::new(
        LifecycleHook::PreToolUse,
        "bash",
        serde_json::json!({"command": "ls"}),
    );
    assert_eq!(event.hook, LifecycleHook::PreToolUse);
    assert_eq!(event.subject, "bash");
    assert_eq!(event.metadata["command"], "ls");
    assert!(event.timestamp <= chrono::Utc::now());
}

#[test]
fn lifecycle_event_with_session_id() {
    let event = LifecycleEvent::new(
        LifecycleHook::SessionStart,
        "session-123",
        serde_json::json!({}),
    )
    .with_session_id("s-456");
    assert_eq!(event.session_id.as_deref(), Some("s-456"));
}

#[test]
fn lifecycle_event_serialization_roundtrip() {
    let event = LifecycleEvent::new(
        LifecycleHook::CwdChanged,
        "/new/path",
        serde_json::json!({"old": "/old/path"}),
    );
    let json = serde_json::to_string(&event).unwrap();
    let decoded: LifecycleEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.hook, LifecycleHook::CwdChanged);
    assert_eq!(decoded.subject, "/new/path");
}
```

Then write minimal implementation:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An event emitted at a lifecycle hook point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// Which hook point triggered this event.
    pub hook: LifecycleHook,
    /// What the event is about (tool name, skill name, path, etc.).
    pub subject: String,
    /// Arbitrary metadata for the event.
    pub metadata: serde_json::Value,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
    /// Session that triggered the event, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl LifecycleEvent {
    pub fn new(hook: LifecycleHook, subject: impl Into<String>, metadata: serde_json::Value) -> Self {
        Self {
            hook,
            subject: subject.into(),
            metadata,
            timestamp: Utc::now(),
            session_id: None,
        }
    }

    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_event
# Expected: 3 tests PASS
```

Note: `chrono` must be added to `rustycode-guard/Cargo.toml`:
```toml
chrono = { workspace = true }
```

### 4.3 HookHandler Trait and ExpandedHookDispatcher -- TDD Task 8

Write the failing tests:

```rust
#[test]
fn dispatcher_dispatches_to_registered_handler() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    let called = Arc::new(AtomicBool::new(false));
    let called_clone = called.clone();

    dispatcher.register(LifecycleHook::PreToolUse, move |event| {
        assert_eq!(event.subject, "bash");
        called_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    let event = LifecycleEvent::new(
        LifecycleHook::PreToolUse,
        "bash",
        serde_json::json!({}),
    );
    dispatcher.dispatch(&event).unwrap();
    assert!(called.load(Ordering::SeqCst));
}

#[test]
fn dispatcher_calls_multiple_handlers_in_order() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    let order = Arc::new(Mutex::new(Vec::new()));

    let o1 = order.clone();
    dispatcher.register(LifecycleHook::PostToolUse, move |_| {
        o1.lock().unwrap().push(1);
        Ok(())
    });

    let o2 = order.clone();
    dispatcher.register(LifecycleHook::PostToolUse, move |_| {
        o2.lock().unwrap().push(2);
        Ok(())
    });

    let event = LifecycleEvent::new(
        LifecycleHook::PostToolUse,
        "read_file",
        serde_json::json!({}),
    );
    dispatcher.dispatch(&event).unwrap();
    assert_eq!(*order.lock().unwrap(), vec![1, 2]);
}

#[test]
fn dispatcher_ignores_unregistered_hooks() {
    let dispatcher = ExpandedHookDispatcher::new();
    let event = LifecycleEvent::new(
        LifecycleHook::BudgetWarning,
        "skills",
        serde_json::json!({}),
    );
    // Should not panic or error
    assert!(dispatcher.dispatch(&event).is_ok());
}

#[test]
fn dispatcher_continues_after_handler_error() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    let second_called = Arc::new(AtomicBool::new(false));
    let second_clone = second_called.clone();

    dispatcher.register(LifecycleHook::SessionStart, move |_| {
        Err("first handler fails".into())
    });
    dispatcher.register(LifecycleHook::SessionStart, move |_| {
        second_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    let event = LifecycleEvent::new(
        LifecycleHook::SessionStart,
        "s1",
        serde_json::json!({}),
    );
    let result = dispatcher.dispatch(&event);
    // Dispatch completes even if handlers fail
    assert!(result.is_ok());
    assert!(second_called.load(Ordering::SeqCst));
}

#[test]
fn dispatcher_handler_count() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    assert_eq!(dispatcher.handler_count(LifecycleHook::PreToolUse), 0);
    dispatcher.register(LifecycleHook::PreToolUse, |_| Ok(()));
    dispatcher.register(LifecycleHook::PreToolUse, |_| Ok(()));
    assert_eq!(dispatcher.handler_count(LifecycleHook::PreToolUse), 2);
    assert_eq!(dispatcher.handler_count(LifecycleHook::PostToolUse), 0);
}

#[test]
fn dispatcher_clear_handlers() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    dispatcher.register(LifecycleHook::PlanStart, |_| Ok(()));
    dispatcher.clear_handlers(LifecycleHook::PlanStart);
    assert_eq!(dispatcher.handler_count(LifecycleHook::PlanStart), 0);
}
```

Then write minimal implementation:

```rust
use std::collections::HashMap;
use std::sync::Arc;

/// Handler function type for lifecycle hooks.
pub type HookHandlerFn = dyn Fn(&LifecycleEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync;

/// Dispatcher for expanded lifecycle hooks.
pub struct ExpandedHookDispatcher {
    handlers: HashMap<LifecycleHook, Vec<Arc<HookHandlerFn>>>,
}

impl ExpandedHookDispatcher {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a specific hook point.
    pub fn register<F>(&mut self, hook: LifecycleHook, handler: F)
    where
        F: Fn(&LifecycleEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
            + Send
            + Sync
            + 'static,
    {
        self.handlers
            .entry(hook)
            .or_default()
            .push(Arc::new(handler));
    }

    /// Dispatch an event to all registered handlers for its hook point.
    /// Handler errors are logged but do not prevent other handlers from running.
    pub fn dispatch(&self, event: &LifecycleEvent) -> anyhow::Result<()> {
        if let Some(handlers) = self.handlers.get(&event.hook) {
            for handler in handlers {
                if let Err(e) = handler(event) {
                    // Log but continue -- hooks should not break the main flow
                    eprintln!(
                        "[hook] handler error on {}: {e}",
                        event.hook.event_type()
                    );
                }
            }
        }
        Ok(())
    }

    /// Number of handlers registered for a specific hook point.
    pub fn handler_count(&self, hook: LifecycleHook) -> usize {
        self.handlers.get(&hook).map_or(0, Vec::len)
    }

    /// Remove all handlers for a specific hook point.
    pub fn clear_handlers(&mut self, hook: LifecycleHook) {
        self.handlers.remove(&hook);
    }
}

impl Default for ExpandedHookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-guard -- hooks_expanded::tests::dispatcher
# Expected: 6 tests PASS
```

### 4.4 LifecycleHook Serde and Edge Cases -- TDD Task 9

Write the failing tests:

```rust
#[test]
fn lifecycle_hook_serde_roundtrip() {
    for hook in LifecycleHook::all() {
        let json = serde_json::to_string(hook).unwrap();
        let decoded: LifecycleHook = serde_json::from_str(&json).unwrap();
        assert_eq!(*hook, decoded);
    }
}

#[test]
fn lifecycle_hook_display_matches_event_type() {
    for hook in LifecycleHook::all() {
        assert_eq!(hook.to_string(), hook.event_type());
    }
}

#[test]
fn lifecycle_hook_all_has_no_duplicates() {
    let hooks = LifecycleHook::all();
    let set: std::collections::HashSet<LifecycleHook> = hooks.iter().copied().collect();
    assert_eq!(hooks.len(), set.len());
}

#[test]
fn lifecycle_event_without_session_id_serializes_cleanly() {
    let event = LifecycleEvent::new(
        LifecycleHook::TierPromoted,
        "default->extended",
        serde_json::json!({}),
    );
    let json = serde_json::to_string(&event).unwrap();
    assert!(!json.contains("session_id"));
}
```

**Verify**:
```bash
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_hook_serde
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_hook_display
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_hook_all
cargo test -p rustycode-guard -- hooks_expanded::tests::lifecycle_event_without_session
# Expected: 4 tests PASS
```

### 4.5 Wire hooks_expanded into guard -- TDD Task 10

Modify `crates/rustycode-guard/src/lib.rs`:

```rust
// Add module declaration:
pub mod hooks_expanded;

// Add re-exports:
pub use hooks_expanded::{ExpandedHookDispatcher, LifecycleEvent, LifecycleHook};
```

Also update `Cargo.toml` to add `chrono`:
```toml
chrono = { workspace = true }
```

**Verify**:
```bash
cargo build -p rustycode-guard
cargo test -p rustycode-guard -- hooks_expanded
# Expected: 22 tests PASS (3 + 3 + 6 + 4 + 6 serde/edge tests counted)
# Actually: LifecycleHook all count = 22 variants
```

**Commit**: `feat: expanded lifecycle hooks with 22 hook points (22 tests)`

---

## 5. Chunk 3: Per-Skill Tool Scoping

**Target**: `crates/rustycode-skill/src/scoping.rs` (~250 lines)
**Tests**: 14 tests
**Estimated chunk size**: ~500 lines

### 5.1 SkillToolScope -- TDD Task 11

Write the failing tests:

```rust
#[test]
fn skill_tool_scope_allows_specified_tools() {
    let scope = SkillToolScope::allowed(vec![
        "read_file".to_string(),
        "grep".to_string(),
        "bash".to_string(),
    ]);
    assert!(scope.is_allowed("read_file"));
    assert!(scope.is_allowed("grep"));
    assert!(!scope.is_allowed("write_file"));
    assert!(!scope.is_allowed("edit_file"));
}

#[test]
fn skill_tool_scope_unrestricted_allows_all() {
    let scope = SkillToolScope::unrestricted();
    assert!(scope.is_allowed("anything"));
    assert!(scope.is_allowed("read_file"));
}

#[test]
fn skill_tool_scope_from_yaml_allowed_tools() {
    let yaml = r#"
allowed-tools:
  - read_file
  - grep
  - glob
"#;
    let scope = SkillToolScope::from_yaml_str(yaml).unwrap();
    assert!(scope.is_allowed("read_file"));
    assert!(scope.is_allowed("grep"));
    assert!(!scope.is_allowed("bash"));
}

#[test]
fn skill_tool_scope_from_yaml_empty_allowed_all() {
    let yaml = r#"
# no allowed-tools field
"#;
    let scope = SkillToolScope::from_yaml_str(yaml).unwrap();
    assert!(scope.is_allowed("any_tool")); // unrestricted
}

#[test]
fn skill_tool_scope_from_yaml_star_means_all() {
    let yaml = r#"
allowed-tools:
  - "*"
"#;
    let scope = SkillToolScope::from_yaml_str(yaml).unwrap();
    assert!(scope.is_allowed("read_file"));
    assert!(scope.is_allowed("bash"));
    assert!(scope.is_allowed("custom_tool"));
}

#[test]
fn skill_tool_scope_empty_allowed_is_unrestricted() {
    let scope = SkillToolScope::allowed(vec![]);
    assert!(scope.is_allowed("anything"));
}
```

Then write minimal implementation:

```rust
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashSet;

/// Tool scoping for a skill. Controls which tools the skill can use.
#[derive(Debug, Clone)]
pub struct SkillToolScope {
    /// If None, all tools are allowed. If Some, only these tools are allowed.
    allowed: Option<HashSet<String>>,
}

#[derive(Debug, Deserialize)]
struct SkillYaml {
    #[serde(rename = "allowed-tools", default)]
    allowed_tools: Vec<String>,
}

impl SkillToolScope {
    /// Create a scope that only allows specified tools.
    /// An empty list means unrestricted (all tools allowed).
    pub fn allowed(tools: Vec<String>) -> Self {
        if tools.is_empty() {
            return Self::unrestricted();
        }
        // Check for wildcard
        if tools.iter().any(|t| t == "*") {
            return Self::unrestricted();
        }
        Self {
            allowed: Some(tools.into_iter().collect()),
        }
    }

    /// Create an unrestricted scope (all tools allowed).
    pub fn unrestricted() -> Self {
        Self { allowed: None }
    }

    /// Check if a specific tool is allowed under this scope.
    pub fn is_allowed(&self, tool_name: &str) -> bool {
        self.allowed
            .as_ref()
            .is_none_or(|set| set.contains(tool_name))
    }

    /// Parse tool scope from a YAML string containing an `allowed-tools` field.
    pub fn from_yaml_str(yaml: &str) -> Result<Self> {
        // Use serde_yaml to parse frontmatter-like structure
        let parsed: SkillYaml = serde_yaml::from_str(yaml).unwrap_or(SkillYaml {
            allowed_tools: vec![],
        });
        Ok(Self::allowed(parsed.allowed_tools))
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- scoping::tests::skill_tool_scope
# Expected: 6 tests PASS
```

### 5.2 resolve_allowed_tools Integration -- TDD Task 12

Write the failing tests:

```rust
#[test]
fn resolve_allowed_tools_from_skill_definition() {
    let def = SkillDefinition {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "Test skill".to_string(),
        when_to_use: String::new(),
        source: SkillSource::Bundled,
        version: String::new(),
        activation: ActivationSpec::always(),
        effort: SkillEffortLevel::default(),
        context: ExecutionContext::default(),
        procedure: None,
        allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
        user_invocable: true,
        model_invocable: true,
        agent: None,
        model_override: None,
        argument_hint: None,
        categories: vec![],
        quality: SkillQuality::default(),
        lifecycle_state: LifecycleState::default(),
        content_path: PathBuf::from("/test"),
        content: None,
    };

    let tools = resolve_allowed_tools(&def);
    assert_eq!(tools.len(), 2);
    assert!(tools.contains(&"read_file".to_string()));
    assert!(tools.contains(&"grep".to_string()));
}

#[test]
fn resolve_allowed_tools_empty_means_all() {
    let def = SkillDefinition {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "desc".to_string(),
        when_to_use: String::new(),
        source: SkillSource::Bundled,
        version: String::new(),
        activation: ActivationSpec::always(),
        effort: SkillEffortLevel::default(),
        context: ExecutionContext::default(),
        procedure: None,
        allowed_tools: vec![],
        user_invocable: true,
        model_invocable: true,
        agent: None,
        model_override: None,
        argument_hint: None,
        categories: vec![],
        quality: SkillQuality::default(),
        lifecycle_state: LifecycleState::default(),
        content_path: PathBuf::from("/test"),
        content: None,
    };

    let tools = resolve_allowed_tools(&def);
    assert!(tools.is_empty()); // empty = unrestricted, returns empty vec
}

#[test]
fn resolve_allowed_tools_wildcard() {
    let def = SkillDefinition {
        id: "test".to_string(),
        name: "Test".to_string(),
        description: "desc".to_string(),
        when_to_use: String::new(),
        source: SkillSource::Bundled,
        version: String::new(),
        activation: ActivationSpec::always(),
        effort: SkillEffortLevel::default(),
        context: ExecutionContext::default(),
        procedure: None,
        allowed_tools: vec!["*".to_string()],
        user_invocable: true,
        model_invocable: true,
        agent: None,
        model_override: None,
        argument_hint: None,
        categories: vec![],
        quality: SkillQuality::default(),
        lifecycle_state: LifecycleState::default(),
        content_path: PathBuf::from("/test"),
        content: None,
    };

    let tools = resolve_allowed_tools(&def);
    assert!(tools.is_empty()); // wildcard = unrestricted
}
```

Then write minimal implementation:

```rust
use crate::types::{SkillDefinition, SkillSource};

/// Resolve the list of allowed tools from a SkillDefinition.
///
/// Returns a vec of tool names the skill is allowed to use.
/// An empty return value means unrestricted (all tools allowed).
pub fn resolve_allowed_tools(def: &SkillDefinition) -> Vec<String> {
    if def.allowed_tools.is_empty() {
        return vec![];
    }
    if def.allowed_tools.iter().any(|t| t == "*") {
        return vec![];
    }
    def.allowed_tools.clone()
}

/// Build a SkillToolScope from a SkillDefinition.
pub fn scope_from_definition(def: &SkillDefinition) -> SkillToolScope {
    let tools = resolve_allowed_tools(def);
    if tools.is_empty() {
        SkillToolScope::unrestricted()
    } else {
        SkillToolScope::allowed(tools)
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- scoping::tests::resolve_allowed
# Expected: 3 tests PASS
```

### 5.3 Scope Intersection -- TDD Task 13

Write the failing tests:

```rust
#[test]
fn scope_intersect_two_scopes() {
    let scope1 = SkillToolScope::allowed(vec![
        "read_file".to_string(),
        "grep".to_string(),
        "bash".to_string(),
    ]);
    let scope2 = SkillToolScope::allowed(vec![
        "read_file".to_string(),
        "bash".to_string(),
        "write_file".to_string(),
    ]);
    let intersection = scope1.intersect(&scope2);
    assert!(intersection.is_allowed("read_file"));
    assert!(intersection.is_allowed("bash"));
    assert!(!intersection.is_allowed("grep"));
    assert!(!intersection.is_allowed("write_file"));
}

#[test]
fn scope_intersect_with_unrestricted() {
    let scope1 = SkillToolScope::allowed(vec!["read_file".to_string()]);
    let scope2 = SkillToolScope::unrestricted();
    let intersection = scope1.intersect(&scope2);
    assert!(intersection.is_allowed("read_file"));
    assert!(!intersection.is_allowed("bash"));
}

#[test]
fn scope_intersect_both_unrestricted() {
    let scope1 = SkillToolScope::unrestricted();
    let scope2 = SkillToolScope::unrestricted();
    let intersection = scope1.intersect(&scope2);
    assert!(intersection.is_allowed("anything"));
}

#[test]
fn scope_union_two_scopes() {
    let scope1 = SkillToolScope::allowed(vec!["read_file".to_string()]);
    let scope2 = SkillToolScope::allowed(vec!["bash".to_string()]);
    let union = scope1.union(&scope2);
    assert!(union.is_allowed("read_file"));
    assert!(union.is_allowed("bash"));
    assert!(!union.is_allowed("write_file"));
}

#[test]
fn scope_to_tool_names() {
    let scope = SkillToolScope::allowed(vec![
        "read_file".to_string(),
        "grep".to_string(),
    ]);
    let mut names = scope.to_tool_names();
    names.sort();
    assert_eq!(names, vec!["grep", "read_file"]);
}
```

Then add to `SkillToolScope`:

```rust
/// Compute intersection of two scopes (only tools allowed by both).
pub fn intersect(&self, other: &Self) -> Self {
    match (&self.allowed, &other.allowed) {
        (Some(a), Some(b)) => {
            let intersection: HashSet<String> = a & b;
            Self::allowed(intersection.into_iter().collect())
        }
        (Some(a), None) => Self::allowed(a.iter().cloned().collect()),
        (None, Some(b)) => Self::allowed(b.iter().cloned().collect()),
        (None, None) => Self::unrestricted(),
    }
}

/// Compute union of two scopes (tools allowed by either).
pub fn union(&self, other: &Self) -> Self {
    match (&self.allowed, &other.allowed) {
        (Some(a), Some(b)) => {
            let union: HashSet<String> = a | b;
            Self::allowed(union.into_iter().collect())
        }
        _ => Self::unrestricted(),
    }
}

/// Get the list of allowed tool names (empty if unrestricted).
pub fn to_tool_names(&self) -> Vec<String> {
    self.allowed
        .as_ref()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default()
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- scoping::tests
# Expected: 14 tests PASS (6 + 3 + 5)
```

### 5.4 Wire scoping into skill crate -- TDD Task 14

Modify `crates/rustycode-skill/src/lib.rs`:

```rust
pub mod scoping;

pub use scoping::{resolve_allowed_tools, scope_from_definition, SkillToolScope};
```

**Verify**:
```bash
cargo build -p rustycode-skill
cargo test -p rustycode-skill -- scoping
# Expected: 14 tests PASS
```

**Commit**: `feat: per-skill tool scoping with YAML allowed-tools (14 tests)`

---

## 6. Chunk 4: Context Budget for Skills

**Target**: `crates/rustycode-skill/src/budget.rs` (~300 lines)
**Tests**: 18 tests
**Estimated chunk size**: ~600 lines

### 6.1 ContextBudget -- TDD Task 15

Write the failing tests:

```rust
#[test]
fn context_budget_default_is_100k_tokens() {
    let budget = ContextBudget::default();
    assert_eq!(budget.total_budget(), 100_000);
    assert_eq!(budget.used(), 0);
    assert_eq!(budget.remaining(), 100_000);
}

#[test]
fn context_budget_custom_total() {
    let budget = ContextBudget::new(200_000);
    assert_eq!(budget.total_budget(), 200_000);
}

#[test]
fn context_budget_allocate_and_used() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 30_000);
    budget.allocate("skill-b", 20_000);
    assert_eq!(budget.used(), 50_000);
    assert_eq!(budget.remaining(), 50_000);
}

#[test]
fn context_budget_allocate_over_budget_fails() {
    let mut budget = ContextBudget::new(100_000);
    assert!(budget.allocate("skill-a", 60_000).is_ok());
    let result = budget.allocate("skill-b", 50_000);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("exceeds"));
}

#[test]
fn context_budget_deallocate() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 40_000);
    budget.deallocate("skill-a");
    assert_eq!(budget.used(), 0);
    assert_eq!(budget.remaining(), 100_000);
}

#[test]
fn context_budget_deallocate_unknown_skill_is_noop() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 40_000);
    budget.deallocate("nonexistent");
    assert_eq!(budget.used(), 40_000);
}

#[test]
fn context_budget_utilization_ratio() {
    let mut budget = ContextBudget::new(100_000);
    assert!((budget.utilization() - 0.0).abs() < f64::EPSILON);
    budget.allocate("skill-a", 50_000);
    assert!((budget.utilization() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn context_budget_is_over_warning_threshold() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 75_000);
    assert!(budget.is_over_warning_threshold());
}

#[test]
fn context_budget_not_over_warning_threshold() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 50_000);
    assert!(!budget.is_over_warning_threshold());
}
```

Then write minimal implementation:

```rust
use anyhow::{bail, Result};
use std::collections::HashMap;

/// Default warning threshold (80% of budget).
const DEFAULT_WARNING_THRESHOLD: f64 = 0.80;

/// Tracks context budget allocation across active skills.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    total: u64,
    entries: HashMap<String, SkillBudgetEntry>,
}

/// A single skill's budget allocation.
#[derive(Debug, Clone)]
pub struct SkillBudgetEntry {
    pub skill_name: String,
    pub tokens: u64,
    pub priority: u8,
}

impl ContextBudget {
    /// Create a new budget with the given total token count.
    pub fn new(total: u64) -> Self {
        Self {
            total,
            entries: HashMap::new(),
        }
    }

    /// Total budget in tokens.
    pub fn total_budget(&self) -> u64 {
        self.total
    }

    /// Tokens currently allocated.
    pub fn used(&self) -> u64 {
        self.entries.values().map(|e| e.tokens).fold(0u64, |a, b| a.saturating_add(b))
    }

    /// Tokens still available.
    pub fn remaining(&self) -> u64 {
        self.total.saturating_sub(self.used())
    }

    /// Fraction of budget currently used (0.0 to 1.0+).
    pub fn utilization(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.used() as f64 / self.total as f64
    }

    /// Whether usage exceeds the warning threshold (80% by default).
    pub fn is_over_warning_threshold(&self) -> bool {
        self.utilization() >= DEFAULT_WARNING_THRESHOLD
    }

    /// Allocate tokens for a skill. Fails if it would exceed the total budget.
    pub fn allocate(&mut self, skill_name: &str, tokens: u64) -> Result<()> {
        let current = self.used();
        let proposed = current.saturating_add(tokens);
        if proposed > self.total {
            bail!(
                "allocating {} tokens for '{}' (total {} would exceed budget {})",
                tokens,
                skill_name,
                proposed,
                self.total
            );
        }
        self.entries.insert(
            skill_name.to_string(),
            SkillBudgetEntry {
                skill_name: skill_name.to_string(),
                tokens,
                priority: 5,
            },
        );
        Ok(())
    }

    /// Deallocate tokens for a skill (e.g., when skill deactivates).
    pub fn deallocate(&mut self, skill_name: &str) {
        self.entries.remove(skill_name);
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::new(100_000)
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- budget::tests::context_budget
# Expected: 9 tests PASS
```

### 6.2 BudgetEnforcer with Eviction -- TDD Task 16

Write the failing tests:

```rust
#[test]
fn budget_enforcer_evicts_lowest_priority() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("low-priority", 60_000, 8); // priority 8 (lower)
    enforcer.add_skill("high-priority", 30_000, 2); // priority 2 (higher)

    let evicted = enforcer.enforce_budget();
    assert_eq!(evicted.len(), 0); // total is 90k, under 100k

    // Now add another skill that would exceed
    enforcer.add_skill("medium-priority", 20_000, 5);
    let evicted = enforcer.enforce_budget();
    // Total would be 110k, need to evict. low-priority (8) evicted first.
    assert!(evicted.contains(&"low-priority".to_string()));
}

#[test]
fn budget_enforcer_evicts_multiple_if_needed() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("skill-a", 50_000, 9);
    enforcer.add_skill("skill-b", 40_000, 8);
    enforcer.add_skill("skill-c", 30_000, 2);

    let evicted = enforcer.enforce_budget();
    // Total 120k, need to free at least 20k. Evict a (50k) first -> 70k, under budget.
    assert!(evicted.contains(&"skill-a".to_string()));
    assert_eq!(enforcer.budget().used(), 70_000);
}

#[test]
fn budget_enforcer_no_eviction_when_under_budget() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("skill-a", 30_000, 5);
    enforcer.add_skill("skill-b", 20_000, 3);

    let evicted = enforcer.enforce_budget();
    assert!(evicted.is_empty());
}

#[test]
fn budget_enforcer_eviction_preserves_high_priority() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("critical", 40_000, 1); // highest priority
    enforcer.add_skill("expendable", 80_000, 10); // lowest priority

    let evicted = enforcer.enforce_budget();
    assert!(evicted.contains(&"expendable".to_string()));
    assert!(!evicted.contains(&"critical".to_string()));
    assert!(enforcer.budget().is_active("critical"));
}

#[test]
fn budget_enforcer_deactivate_skill() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("skill-a", 40_000, 5);
    enforcer.deactivate_skill("skill-a");
    assert!(!enforcer.budget().is_active("skill-a"));
    assert_eq!(enforcer.budget().used(), 0);
}

#[test]
fn budget_enforcer_list_active_skills_sorted_by_priority() {
    let mut enforcer = BudgetEnforcer::new(200_000);
    enforcer.add_skill("medium", 10_000, 5);
    enforcer.add_skill("high", 10_000, 1);
    enforcer.add_skill("low", 10_000, 9);

    let active = enforcer.active_skills();
    assert_eq!(active[0].skill_name, "high");
    assert_eq!(active[1].skill_name, "medium");
    assert_eq!(active[2].skill_name, "low");
}

#[test]
fn budget_enforcer_available_after_eviction() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("big", 90_000, 5);
    assert_eq!(enforcer.budget().remaining(), 10_000);

    enforcer.deactivate_skill("big");
    assert_eq!(enforcer.budget().remaining(), 100_000);
}
```

Add to `ContextBudget`:

```rust
/// Check if a skill is currently allocated.
pub fn is_active(&self, skill_name: &str) -> bool {
    self.entries.contains_key(skill_name)
}
```

Then write the enforcer:

```rust
/// Enforces context budget with priority-based eviction.
pub struct BudgetEnforcer {
    budget: ContextBudget,
}

impl BudgetEnforcer {
    pub fn new(total_budget: u64) -> Self {
        Self {
            budget: ContextBudget::new(total_budget),
        }
    }

    /// Get a reference to the underlying budget.
    pub fn budget(&self) -> &ContextBudget {
        &self.budget
    }

    /// Add a skill with its token cost and priority (lower = higher priority).
    /// Priority ranges: 1 (critical) to 10 (dispensable).
    pub fn add_skill(&mut self, name: &str, tokens: u64, priority: u8) {
        // Remove existing entry if present (re-allocation)
        self.budget.entries.remove(name);
        self.budget.entries.insert(
            name.to_string(),
            SkillBudgetEntry {
                skill_name: name.to_string(),
                tokens,
                priority,
            },
        );
    }

    /// Deactivate a skill, freeing its allocated tokens.
    pub fn deactivate_skill(&mut self, name: &str) {
        self.budget.deallocate(name);
    }

    /// Enforce budget by evicting lowest-priority skills until under budget.
    /// Returns names of evicted skills.
    pub fn enforce_budget(&mut self) -> Vec<String> {
        let mut evicted = Vec::new();

        while self.budget.used() > self.budget.total {
            // Find the skill with the highest priority number (lowest priority)
            let to_evict = self
                .budget
                .entries
                .values()
                .max_by_key(|e| e.priority)
                .map(|e| e.skill_name.clone());

            if let Some(name) = to_evict {
                self.budget.deallocate(&name);
                evicted.push(name);
            } else {
                break;
            }
        }

        evicted
    }

    /// List active skills sorted by priority (highest priority first).
    pub fn active_skills(&self) -> Vec<&SkillBudgetEntry> {
        let mut skills: Vec<_> = self.budget.entries.values().collect();
        skills.sort_by_key(|e| e.priority);
        skills
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- budget::tests::budget_enforcer
# Expected: 7 tests PASS
```

### 6.3 Budget Edge Cases -- TDD Task 17

Write the failing tests:

```rust
#[test]
fn context_budget_zero_total() {
    let budget = ContextBudget::new(0);
    assert_eq!(budget.remaining(), 0);
    assert_eq!(budget.used(), 0);
    assert!((budget.utilization() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn context_budget_reallocate_same_skill() {
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("skill-a", 40_000).unwrap();
    budget.deallocate("skill-a");
    budget.allocate("skill-a", 20_000).unwrap();
    assert_eq!(budget.used(), 20_000);
}

#[test]
fn budget_enforcer_skills_at_same_priority() {
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("a", 60_000, 5);
    enforcer.add_skill("b", 60_000, 5);
    let evicted = enforcer.enforce_budget();
    // Both at same priority; one must be evicted to get under 100k
    assert_eq!(evicted.len(), 1);
    assert_eq!(enforcer.budget().used(), 60_000);
}
```

**Verify**:
```bash
cargo test -p rustycode-skill -- budget::tests
# Expected: 18 tests PASS (9 + 7 + 2 additional edge tests)
```

### 6.4 Wire budget into skill crate -- TDD Task 18

Modify `crates/rustycode-skill/src/lib.rs`:

```rust
pub mod budget;

pub use budget::{BudgetEnforcer, ContextBudget, SkillBudgetEntry};
```

**Verify**:
```bash
cargo build -p rustycode-skill
cargo test -p rustycode-skill -- budget
# Expected: 18 tests PASS
```

**Commit**: `feat: context budget tracking with priority-based eviction (18 tests)`

---

## 7. Chunk 5: Integration Wiring

**Target**: Modifications to existing crates
**Tests**: 8 tests
**Estimated chunk size**: ~400 lines

### 7.1 Tier-Aware Tool Listing in Registry -- TDD Task 19

Modify `crates/rustycode-tools-registry/src/registry.rs`:

Write the failing tests first:

```rust
#[test]
fn tier_aware_listing_filters_by_default_tier() {
    let mut registry = ToolRegistry::new();
    registry.register(MockTool::new("read_file", "Read files")).unwrap();
    registry.register(MockTool::new("bash", "Run commands")).unwrap();
    registry.register(MockTool::new("web_fetch", "Fetch URLs")).unwrap();

    let tools = registry.list_for_tier(rustycode_tools_api::ToolTier::Default);
    assert_eq!(tools.len(), 2);
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"bash"));
    assert!(!names.contains(&"web_fetch"));
}

#[test]
fn tier_aware_listing_extended_includes_default() {
    let mut registry = ToolRegistry::new();
    registry.register(MockTool::new("read_file", "Read")).unwrap();
    registry.register(MockTool::new("web_fetch", "Fetch")).unwrap();

    let tools = registry.list_for_tier(rustycode_tools_api::ToolTier::Extended);
    assert_eq!(tools.len(), 2);
}

#[test]
fn tier_aware_listing_full_includes_all() {
    let mut registry = ToolRegistry::new();
    registry.register(MockTool::new("read_file", "Read")).unwrap();
    registry.register(MockTool::new("custom_tool", "Custom")).unwrap();

    let tools = registry.list_for_tier(rustycode_tools_api::ToolTier::Full);
    assert_eq!(tools.len(), 2);
}
```

Add the `tiers` dependency to `crates/rustycode-tools-registry/Cargo.toml`:

```toml
rustycode-tools-api = { path = "../rustycode-tools-api" }
```

(This dependency already exists.)

Add the method to `ToolRegistry`:

```rust
/// List tools available at a specific tier.
pub fn list_for_tier(&self, tier: rustycode_tools_api::ToolTier) -> Vec<ToolInfo> {
    let all = self.list();
    match tier {
        rustycode_tools_api::ToolTier::Full => all,
        rustycode_tools_api::ToolTier::Extended => {
            let default_set = rustycode_tools_api::default_tool_set();
            let extended_set = rustycode_tools_api::extended_tool_set();
            all.into_iter()
                .filter(|t| default_set.contains(t.name.as_str()) || extended_set.contains(t.name.as_str()))
                .collect()
        }
        rustycode_tools_api::ToolTier::Default => {
            let default_set = rustycode_tools_api::default_tool_set();
            all.into_iter()
                .filter(|t| default_set.contains(t.name.as_str()))
                .collect()
        }
    }
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-registry -- registry::tests::tier_aware
# Expected: 3 tests PASS
```

### 7.2 Hook Dispatch Integration -- TDD Task 20

Add a test demonstrating guard dispatch integration:

Write the failing test in `crates/rustycode-guard/src/hooks_expanded.rs`:

```rust
#[test]
fn integration_dispatch_all_hook_types() {
    let mut dispatcher = ExpandedHookDispatcher::new();
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();

    // Register a handler for every hook type
    for hook in LifecycleHook::all() {
        let c = count_clone.clone();
        dispatcher.register(*hook, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });
    }

    // Dispatch an event for each hook type
    for hook in LifecycleHook::all() {
        let event = LifecycleEvent::new(
            *hook,
            "test-subject",
            serde_json::json!({}),
        );
        dispatcher.dispatch(&event).unwrap();
    }

    // All handlers should have been called exactly once
    assert_eq!(count.load(Ordering::SeqCst), LifecycleHook::all().len());
}
```

**Verify**:
```bash
cargo test -p rustycode-guard -- hooks_expanded::tests::integration
# Expected: 1 test PASS
```

### 7.3 Full Pipeline Integration Test -- TDD Task 21

Write an integration test that exercises the full pipeline:

```rust
// In a new test within hooks_expanded.rs or a separate integration test file

#[test]
fn full_pipeline_skill_activates_tools_budget() {
    // 1. Create activation manager at default tier
    use rustycode_tools_api::{ToolActivationManager, ToolTier};

    let mut activation = ToolActivationManager::new();
    assert!(!activation.is_active("web_fetch"));

    // 2. Skill activation with tool scope
    let allowed_tools = vec![
        "read_file".to_string(),
        "grep".to_string(),
        "bash".to_string(),
    ];
    activation.intersect_scope(&allowed_tools);

    // 3. Budget tracking
    let mut budget = ContextBudget::new(100_000);
    budget.allocate("my-skill", 25_000).unwrap();
    assert!(!budget.is_over_warning_threshold());

    // 4. Hook dispatch for skill activation
    let mut dispatcher = ExpandedHookDispatcher::new();
    let activated = Arc::new(AtomicBool::new(false));
    let activated_clone = activated.clone();
    dispatcher.register(LifecycleHook::SkillActivate, move |event| {
        assert_eq!(event.subject, "my-skill");
        activated_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    let event = LifecycleEvent::new(
        LifecycleHook::SkillActivate,
        "my-skill",
        serde_json::json!({"tools": allowed_tools, "tokens": 25000}),
    );
    dispatcher.dispatch(&event).unwrap();
    assert!(activated.load(Ordering::SeqCst));

    // 5. Verify scope is active
    assert!(activation.is_active("read_file"));
    assert!(!activation.is_active("write_file"));

    // 6. Promote tier and verify extended tools
    activation.clear_scope();
    activation.promote(ToolTier::Extended);
    assert!(activation.is_active("web_fetch"));

    // 7. Budget enforcement
    let mut enforcer = BudgetEnforcer::new(100_000);
    enforcer.add_skill("low-priority", 80_000, 9);
    enforcer.add_skill("my-skill", 25_000, 2);
    let evicted = enforcer.enforce_budget();
    assert!(evicted.contains(&"low-priority".to_string()));
    assert!(enforcer.budget().is_active("my-skill"));
}
```

Note: This test requires cross-crate imports. It should be placed in a workspace-level integration test or use dev-dependencies. For the plan, we scope it within `rustycode-skill` tests with the needed imports from `rustycode-tools-api` added as dev-dependency.

Add to `crates/rustycode-skill/Cargo.toml` dev-dependencies:

```toml
[dev-dependencies]
rustycode-tools-api = { path = "../rustycode-tools-api" }
```

And the corresponding import in the test:

```rust
use rustycode_tools_api::{ToolActivationManager, ToolTier};
```

**Verify**:
```bash
cargo test -p rustycode-skill -- budget::tests::full_pipeline
# Expected: 1 test PASS
```

### 7.4 Usage Tracker Informs Tier Promotion -- TDD Task 22

Write the failing test:

```rust
#[test]
fn usage_tracker_suggests_tier_promotion() {
    let mut tracker = UsageTracker::new();
    let mut activation = ToolActivationManager::new();

    // Session uses only default tools
    tracker.record("read_file", true);
    tracker.record("bash", true);
    tracker.record("grep", true);
    assert_eq!(activation.current_tier(), ToolTier::Default);

    // Now the task needs web_fetch, which fails because it is not active
    assert!(!activation.is_active("web_fetch"));

    // Promote based on usage demand
    tracker.record("web_fetch", false); // tool unavailable, recorded as failure
    let top = tracker.most_used(3);
    // If a non-default tool was attempted, suggest promotion
    let needs_extended = top.iter().any(|(name, _)| {
        !rustycode_tools_api::default_tool_set().contains(*name)
    });
    if needs_extended {
        activation.promote(ToolTier::Extended);
    }
    assert_eq!(activation.current_tier(), ToolTier::Extended);
    assert!(activation.is_active("web_fetch"));
}
```

**Verify**:
```bash
cargo test -p rustycode-tools-api -- tiers::tests::usage_tracker_suggests
# Expected: 1 test PASS
```

**Commit**: `feat: tier-aware registry listing and integration pipeline (8 tests)`

---

## 8. Verification Commands

After completing all chunks, run the full verification suite:

```bash
# Build all affected crates
cargo build -p rustycode-tools-api -p rustycode-guard -p rustycode-skill -p rustycode-tools-registry

# Run all new tests
cargo test -p rustycode-tools-api -- tiers
cargo test -p rustycode-guard -- hooks_expanded
cargo test -p rustycode-skill -- scoping
cargo test -p rustycode-skill -- budget
cargo test -p rustycode-tools-registry -- registry::tests::tier_aware

# Verify clippy passes with no warnings
cargo clippy -p rustycode-tools-api -p rustycode-guard -p rustycode-skill -p rustycode-tools-registry -- -D warnings

# Verify formatting
cargo fmt --check -p rustycode-tools-api -p rustycode-guard -p rustycode-skill -p rustycode-tools-registry

# Full workspace clippy (regression check)
cargo clippy --workspace --all-targets -- -D warnings
```

Expected test counts:

| Crate | Module | Tests |
|-------|--------|-------|
| rustycode-tools-api | tiers | 20 |
| rustycode-guard | hooks_expanded | 23 |
| rustycode-skill | scoping | 14 |
| rustycode-skill | budget | 19 |
| rustycode-tools-registry | tier_aware | 3 |
| **Total** | | **79** |

---

## 9. Success Metrics Checklist

- [ ] Default tool set covers 90%+ of common tasks (6 core tools: Read, Edit, Write, Bash, Grep, Glob)
- [ ] 22 hook points available (LifecycleHook::all() returns 22 variants)
- [ ] Skill activation auto-configures tool permissions (SkillToolScope + resolve_allowed_tools)
- [ ] Skill context budget enforced (ContextBudget + BudgetEnforcer with priority eviction)
- [ ] All 79 new tests passing
- [ ] Zero clippy warnings across affected crates
- [ ] Tier-aware tool listing in registry (list_for_tier)

---

## 10. Implementation Notes

### Error Handling

- `tiers.rs` uses no error types (all operations are infallible or return Option)
- `hooks_expanded.rs` uses `anyhow::Result` for dispatch errors
- `scoping.rs` uses `anyhow::Result` for YAML parsing
- `budget.rs` uses `anyhow::Result` for allocation failures

### Concurrency Safety

- `ToolActivationManager` is NOT Sync (uses interior mutability via `&mut self`)
- `UsageTracker` is NOT Sync (same reason)
- `ExpandedHookDispatcher` is NOT Sync (registration requires `&mut self`, dispatch takes `&self`)
- `ContextBudget` and `BudgetEnforcer` are NOT Sync
- These are session-scoped single-owner types; no cross-thread sharing needed
- If async dispatch is needed later, wrap in `Arc<Mutex<>>` at the integration point

### Backward Compatibility

- All new modules are additive -- no existing APIs are modified
- `ToolActivationManager` is opt-in; existing code continues to work without it
- `ExpandedHookDispatcher` coexists with the existing `process_hook()` in guard
- `SkillToolScope` builds on the existing `allowed_tools` field in `SkillDefinition`
- `ContextBudget` is a new standalone type with no breaking changes

### Future Extensions

- Phase 6 will use `SkillToolScope` for exclusion clauses
- `UsageTracker` data can feed into telemetry for default tool set optimization
- `BudgetEnforcer` can integrate with `rustycode-bus` events for cross-module budget notifications
- `ExpandedHookDispatcher` can be backed by `rustycode-bus::EventBus` for async scenarios

### Dependencies Added

| Crate | Dependency | Version |
|-------|-----------|---------|
| rustycode-guard | chrono | workspace |
| rustycode-skill (dev) | rustycode-tools-api | path |

No new external crates are added. All dependencies are already in the workspace.
