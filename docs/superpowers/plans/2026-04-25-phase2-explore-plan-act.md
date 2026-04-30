# Phase 2: Explore-Plan-Act Lifecycle Implementation Plan

**Date**: 2026-04-25
**Pattern Source**: "12 Agentic Harness Patterns from Claude Code" (Generative Programmer, Pattern 6)
**Status**: Implemented
**Depends On**: Phase 1 (Memory Architecture) -- independent, can proceed in parallel

---

## Overview

Enforce a three-phase execution model with distinct permissions, prompts, and validation gates:

1. **Explore** -- Read-only. Agent reads code, searches, gathers context. No modifications.
2. **Plan** -- Discuss approach. Agent proposes changes, user reviews. Still no writes.
3. **Act** -- Full tool access. Agent executes the approved plan.

Each phase uses distinct system prompts. Plan prompts emphasize architecture and trade-offs; Act prompts emphasize precision and verification.

## Implementation Status

This phase has been implemented across the protocol, prompt, and orchestration crates:

- `ExecutionPhase`, `PhaseSkipConfig`, and `PhaseTransitionError` now live in `rustycode-protocol`.
- `ExecutionPlan` validates the Plan -> Act handoff and tracks approval state.
- Phase-specific prompt fragments now exist for Explore, Plan, and Act.
- `TaskContext`, `OrchestrationEvent`, and the pipeline now track and emit phase transitions.
- `PhaseLifecycleManager` enforces the forward-only Explore -> Plan -> Act lifecycle.

Verification:

- `cargo test -p rustycode-protocol --lib --tests`
- `cargo test -p rustycode-prompt --lib --tests`
- `cargo test -p rustycode-orchestration --lib --tests`

The orchestration crate's normal test suite passed; the live-model bench tests in `tests/real_model_bench.rs` are long-running and depend on external API configuration, so they are left as an optional follow-up.

---

## Existing Codebase Leveraged

| Component | Crate | What It Provides |
|-----------|-------|-----------------|
| `PermissionMode::Plan` | `rustycode-protocol` | Read-only tool allow-list, write/exec block |
| `is_read_only_tool()` | `rustycode-protocol/src/permission_modes.rs` | Tool classification |
| `PermissionRuleSet` | `rustycode-protocol/src/permission_modes.rs` | Precedence-based rule evaluation |
| `PlanMode` | `rustycode-orchestra/src/plan_mode.rs` | Role-based tool gating, plan approval lifecycle |
| `ConvoyPlan` | `rustycode-protocol/src/convoy_plan.rs` | Structured plan schema (summary, approach, files, risks, criteria) |
| `TaskPhase` | `rustycode-orchestration/src/task_context.rs` | Phase enum with transition, tier mapping |
| `TaskContext` | `rustycode-orchestration/src/task_context.rs` | Execution context carrying phase, trace, workspace |
| `PromptBuilder` | `rustycode-prompt/src/layered.rs` | Layered prompt assembly (base, model, env, project) |
| `WorkingMode` | `rustycode-protocol/src/modes.rs` | Mode-specific system prompts and temperature |
| `StepOrchestrator` | `rustycode-orchestration/src/orchestrator.rs` | Tiered step execution with retry/escalate |
| `OrchestrationPipeline` | `rustycode-orchestration/src/pipeline.rs` | End-to-end task lifecycle |
| `OrchestrationEvent` | `rustycode-orchestration/src/bus.rs` | Event bus for phase transition notifications |

---

## File Structure

| # | File | Action | Purpose |
|---|------|--------|---------|
| 1 | `crates/rustycode-protocol/src/execution_phase.rs` | **Create** | `ExecutionPhase` enum, transition rules, skip flags |
| 2 | `crates/rustycode-protocol/src/lib.rs` | **Edit** | Add `pub mod execution_phase` + re-exports |
| 3 | `crates/rustycode-protocol/src/permission_modes.rs` | **Edit** | `ExecutionPhase::permission_mode()` bridge, `is_plan_tool()` |
| 4 | `crates/rustycode-prompt/src/phase_prompts.rs` | **Create** | Per-phase system prompt fragments + builder extension |
| 5 | `crates/rustycode-prompt/src/lib.rs` | **Edit** | Add `pub mod phase_prompts` |
| 6 | `crates/rustycode-prompt/prompts/explore.txt` | **Create** | Explore-phase prompt template |
| 7 | `crates/rustycode-prompt/prompts/plan.txt` | **Create** | Plan-phase prompt template |
| 8 | `crates/rustycode-prompt/prompts/act.txt` | **Create** | Act-phase prompt template |
| 9 | `crates/rustycode-protocol/src/execution_plan.rs` | **Create** | `ExecutionPlan` schema + validation |
| 10 | `crates/rustycode-protocol/src/lib.rs` | **Edit** | Add `pub mod execution_plan` + re-exports |
| 11 | `crates/rustycode-orchestration/src/task_context.rs` | **Edit** | Add `execution_phase` field + transition methods |
| 12 | `crates/rustycode-orchestration/src/bus.rs` | **Edit** | Add `PhaseTransition` event variant |
| 13 | `crates/rustycode-orchestration/src/pipeline.rs` | **Edit** | Wire phase lifecycle into `conduct()` |
| 14 | `crates/rustycode-orchestration/src/phase_lifecycle.rs` | **Create** | `PhaseLifecycleManager` enforcing transitions |
| 15 | `crates/rustycode-orchestration/src/lib.rs` | **Edit** | Add `pub mod phase_lifecycle` |
| 16 | `crates/rustycode-orchestration/tests/phase_lifecycle.rs` | **Create** | Integration tests |

---

## TDD Steps

### Chunk 1: ExecutionPhase Enum and Transitions

**File**: `crates/rustycode-protocol/src/execution_phase.rs` (new)

```rust
//! Three-phase execution lifecycle: Explore -> Plan -> Act.
//!
//! Each phase restricts available tools and applies distinct system prompts.
//! Phase transitions are one-directional (no going backward).

use serde::{Deserialize, Serialize};
use std::fmt;

/// The current execution phase in the Explore-Plan-Act lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExecutionPhase {
    /// Read-only exploration: gather context, search code, understand structure.
    #[default]
    Explore,
    /// Plan proposal: discuss approach, propose changes, still no writes.
    Plan,
    /// Full execution: implement the approved plan with all tools.
    Act,
}

impl ExecutionPhase {
    /// Ordered phases for iteration.
    pub const fn all() -> &'static [ExecutionPhase] {
        &[ExecutionPhase::Explore, ExecutionPhase::Plan, ExecutionPhase::Act]
    }

    /// Which phase comes after this one. Returns `None` for `Act`.
    pub const fn next(&self) -> Option<ExecutionPhase> {
        match self {
            Self::Explore => Some(Self::Plan),
            Self::Plan => Some(Self::Act),
            Self::Act => None,
        }
    }

    /// Attempt to transition to a target phase.
    /// Valid transitions: Explore -> Plan, Plan -> Act.
    pub fn transition_to(&self, target: ExecutionPhase) -> Result<(), PhaseTransitionError> {
        if *self == target {
            return Ok(());
        }
        match self.next() {
            Some(next) if next == target => Ok(()),
            Some(next) => Err(PhaseTransitionError::OutOfOrder {
                from: *self,
                attempted: target,
                expected: next,
            }),
            None => Err(PhaseTransitionError::AlreadyComplete { from: *self }),
        }
    }

    /// Whether this phase allows file writes and command execution.
    pub const fn allows_writes(&self) -> bool {
        matches!(self, Self::Act)
    }

    /// Whether this phase allows plan submission/review tools.
    pub const fn allows_planning(&self) -> bool {
        matches!(self, Self::Plan | Self::Act)
    }

    /// Human-readable label for the phase.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Explore => "Explore",
            Self::Plan => "Plan",
            Self::Act => "Act",
        }
    }

    /// Index for ordering (0 = Explore, 1 = Plan, 2 = Act).
    pub const fn index(&self) -> u8 {
        match self {
            Self::Explore => 0,
            Self::Plan => 1,
            Self::Act => 2,
        }
    }
}

impl fmt::Display for ExecutionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Skip-ahead configuration for impatient workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PhaseSkipConfig {
    /// Skip the Explore phase, start at Plan.
    pub skip_explore: bool,
    /// Skip both Explore and Plan, start at Act.
    pub skip_plan: bool,
}

impl PhaseSkipConfig {
    /// Create a new skip config with both flags false.
    pub const fn new() -> Self {
        Self { skip_explore: false, skip_plan: false }
    }

    /// Resolve the effective starting phase given skip flags.
    pub fn starting_phase(&self) -> ExecutionPhase {
        if self.skip_plan {
            ExecutionPhase::Act
        } else if self.skip_explore {
            ExecutionPhase::Plan
        } else {
            ExecutionPhase::Explore
        }
    }

    /// Skip Explore only.
    pub const fn skip_explore() -> Self {
        Self { skip_explore: true, skip_plan: false }
    }

    /// Skip Explore and Plan (jump to Act).
    pub const fn skip_to_act() -> Self {
        Self { skip_explore: true, skip_plan: true }
    }
}

/// Error from an invalid phase transition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhaseTransitionError {
    #[error("cannot transition from {from:?} to {attempted:?}; expected {expected:?}")]
    OutOfOrder { from: ExecutionPhase, attempted: ExecutionPhase, expected: ExecutionPhase },
    #[error("no transitions available from {from:?}")]
    AlreadyComplete { from: ExecutionPhase },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explore_transitions_to_plan() {
        assert_eq!(ExecutionPhase::Explore.next(), Some(ExecutionPhase::Plan));
    }
    #[test]
    fn plan_transitions_to_act() {
        assert_eq!(ExecutionPhase::Plan.next(), Some(ExecutionPhase::Act));
    }
    #[test]
    fn act_has_no_next() {
        assert_eq!(ExecutionPhase::Act.next(), None);
    }
    #[test]
    fn valid_transition_explore_to_plan() {
        assert!(ExecutionPhase::Explore.transition_to(ExecutionPhase::Plan).is_ok());
    }
    #[test]
    fn valid_transition_plan_to_act() {
        assert!(ExecutionPhase::Plan.transition_to(ExecutionPhase::Act).is_ok());
    }
    #[test]
    fn invalid_transition_explore_to_act() {
        let err = ExecutionPhase::Explore.transition_to(ExecutionPhase::Act).unwrap_err();
        assert!(matches!(err, PhaseTransitionError::OutOfOrder { .. }));
    }
    #[test]
    fn invalid_transition_act_to_plan() {
        let err = ExecutionPhase::Act.transition_to(ExecutionPhase::Plan).unwrap_err();
        assert!(matches!(err, PhaseTransitionError::AlreadyComplete { .. }));
    }
    #[test]
    fn same_phase_transition_is_ok() {
        assert!(ExecutionPhase::Explore.transition_to(ExecutionPhase::Explore).is_ok());
        assert!(ExecutionPhase::Plan.transition_to(ExecutionPhase::Plan).is_ok());
        assert!(ExecutionPhase::Act.transition_to(ExecutionPhase::Act).is_ok());
    }
    #[test]
    fn permission_flags_explore() {
        assert!(!ExecutionPhase::Explore.allows_writes());
        assert!(!ExecutionPhase::Explore.allows_planning());
    }
    #[test]
    fn permission_flags_plan() {
        assert!(!ExecutionPhase::Plan.allows_writes());
        assert!(ExecutionPhase::Plan.allows_planning());
    }
    #[test]
    fn permission_flags_act() {
        assert!(ExecutionPhase::Act.allows_writes());
        assert!(ExecutionPhase::Act.allows_planning());
    }
    #[test]
    fn display_labels() {
        assert_eq!(ExecutionPhase::Explore.to_string(), "Explore");
        assert_eq!(ExecutionPhase::Plan.to_string(), "Plan");
        assert_eq!(ExecutionPhase::Act.to_string(), "Act");
    }
    #[test]
    fn phase_index_ordering() {
        assert!(ExecutionPhase::Explore.index() < ExecutionPhase::Plan.index());
        assert!(ExecutionPhase::Plan.index() < ExecutionPhase::Act.index());
    }
    #[test]
    fn all_returns_three_phases() {
        assert_eq!(ExecutionPhase::all().len(), 3);
    }
    #[test]
    fn skip_config_default_starts_at_explore() {
        assert_eq!(PhaseSkipConfig::default().starting_phase(), ExecutionPhase::Explore);
    }
    #[test]
    fn skip_config_skip_explore_starts_at_plan() {
        let config = PhaseSkipConfig::skip_explore();
        assert_eq!(config.starting_phase(), ExecutionPhase::Plan);
        assert!(config.skip_explore);
        assert!(!config.skip_plan);
    }
    #[test]
    fn skip_config_skip_to_act_starts_at_act() {
        let config = PhaseSkipConfig::skip_to_act();
        assert_eq!(config.starting_phase(), ExecutionPhase::Act);
    }
    #[test]
    fn skip_config_new_is_default() {
        assert_eq!(PhaseSkipConfig::new(), PhaseSkipConfig::default());
    }
    #[test]
    fn serde_roundtrip() {
        let json = serde_json::to_string(&ExecutionPhase::Plan).unwrap();
        assert_eq!(json, "\"plan\"");
        let back: ExecutionPhase = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ExecutionPhase::Plan);
    }
    #[test]
    fn skip_config_serde_roundtrip() {
        let config = PhaseSkipConfig { skip_explore: true, skip_plan: false };
        let json = serde_json::to_string(&config).unwrap();
        let back: PhaseSkipConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }
}
```

**Edit**: `crates/rustycode-protocol/src/lib.rs` -- add:

```rust
pub mod execution_phase;
pub use execution_phase::{ExecutionPhase, PhaseSkipConfig, PhaseTransitionError};
```

**Tests**: 20 tests.

---

### Chunk 2: Phase-Aware Permission Enforcement

**File**: `crates/rustycode-protocol/src/permission_modes.rs` (edit)

Add to existing file. Import `ExecutionPhase` and add two items:

```rust
use crate::execution_phase::ExecutionPhase;

/// Whether a tool is used for plan submission/review (no side effects on codebase).
fn is_plan_tool(name: &str) -> bool {
    matches!(name, "submit_plan" | "review_plan" | "approve_plan" | "reject_plan" | "list_plans" | "get_plan")
}
```

Add `impl ExecutionPhase` block in this file for permission coupling:

```rust
impl ExecutionPhase {
    /// Map this execution phase to the corresponding permission mode.
    pub fn permission_mode(&self) -> PermissionMode {
        match self {
            Self::Explore | Self::Plan => PermissionMode::Plan,
            Self::Act => PermissionMode::AcceptEdits,
        }
    }

    /// Decide whether a tool is allowed in this execution phase.
    pub fn decide_tool(&self, tool_name: &str) -> PermissionDecision {
        if is_read_only_tool(tool_name) {
            return PermissionDecision::Allow {
                reason: format!("read-only tool allowed in {} phase", self.label()),
            };
        }
        match self {
            Self::Explore => PermissionDecision::Deny {
                reason: format!("{} blocked in Explore phase (read-only)", tool_name),
            },
            Self::Plan => {
                if is_plan_tool(tool_name) {
                    PermissionDecision::Allow { reason: "plan tool allowed in Plan phase".into() }
                } else {
                    PermissionDecision::Deny {
                        reason: format!("{} blocked in Plan phase (read-only + plan tools)", tool_name),
                    }
                }
            }
            Self::Act => PermissionDecision::Allow {
                reason: format!("{} allowed in Act phase", tool_name),
            },
        }
    }
}
```

**New tests** (add to existing `mod tests`):

```rust
    #[test]
    fn execution_phase_explore_blocks_write() {
        assert!(ExecutionPhase::Explore.decide_tool("write_file").is_denied());
    }
    #[test]
    fn execution_phase_explore_blocks_bash() {
        assert!(ExecutionPhase::Explore.decide_tool("bash").is_denied());
    }
    #[test]
    fn execution_phase_explore_allows_read() {
        assert!(ExecutionPhase::Explore.decide_tool("read_file").is_allowed());
    }
    #[test]
    fn execution_phase_explore_allows_grep() {
        assert!(ExecutionPhase::Explore.decide_tool("grep").is_allowed());
    }
    #[test]
    fn execution_phase_plan_blocks_write() {
        assert!(ExecutionPhase::Plan.decide_tool("write_file").is_denied());
    }
    #[test]
    fn execution_phase_plan_blocks_bash() {
        assert!(ExecutionPhase::Plan.decide_tool("bash").is_denied());
    }
    #[test]
    fn execution_phase_plan_allows_plan_tools() {
        assert!(ExecutionPhase::Plan.decide_tool("submit_plan").is_allowed());
        assert!(ExecutionPhase::Plan.decide_tool("review_plan").is_allowed());
    }
    #[test]
    fn execution_phase_act_allows_everything() {
        assert!(ExecutionPhase::Act.decide_tool("write_file").is_allowed());
        assert!(ExecutionPhase::Act.decide_tool("bash").is_allowed());
    }
    #[test]
    fn execution_phase_permission_mode_mapping() {
        assert_eq!(ExecutionPhase::Explore.permission_mode(), PermissionMode::Plan);
        assert_eq!(ExecutionPhase::Plan.permission_mode(), PermissionMode::Plan);
        assert_eq!(ExecutionPhase::Act.permission_mode(), PermissionMode::AcceptEdits);
    }
    #[test]
    fn is_plan_tool_classification() {
        assert!(is_plan_tool("submit_plan"));
        assert!(is_plan_tool("approve_plan"));
        assert!(!is_plan_tool("write_file"));
        assert!(!is_plan_tool("bash"));
    }
```

**Tests**: 11 new tests (all existing tests continue passing).

---

### Chunk 3: Distinct System Prompts Per Phase

**File**: `crates/rustycode-prompt/src/phase_prompts.rs` (new)

```rust
//! Phase-specific system prompt fragments for the Explore-Plan-Act lifecycle.

use crate::environment::EnvironmentContext;
use anyhow::Result;
use std::path::Path;

/// Phase-specific prompt fragment provider.
#[derive(Debug, Clone)]
pub struct PhasePromptProvider {
    explore_prompt: String,
    plan_prompt: String,
    act_prompt: String,
}

impl PhasePromptProvider {
    pub fn new() -> Self {
        Self {
            explore_prompt: include_str!("../prompts/explore.txt").to_string(),
            plan_prompt: include_str!("../prompts/plan.txt").to_string(),
            act_prompt: include_str!("../prompts/act.txt").to_string(),
        }
    }

    /// Get the prompt fragment for a specific execution phase.
    pub fn phase_prompt(&self, phase: rustycode_protocol::ExecutionPhase) -> &str {
        match phase {
            rustycode_protocol::ExecutionPhase::Explore => &self.explore_prompt,
            rustycode_protocol::ExecutionPhase::Plan => &self.plan_prompt,
            rustycode_protocol::ExecutionPhase::Act => &self.act_prompt,
        }
    }

    /// Build a complete prompt with the phase-specific layer injected.
    pub async fn build_phase_prompt(
        &self,
        phase: rustycode_protocol::ExecutionPhase,
        model_id: &str,
        file: Option<&Path>,
        env: &EnvironmentContext,
    ) -> Result<String> {
        let builder = crate::layered::PromptBuilder::new();
        let mut base = builder.build(model_id, file, env).await?;
        let phase_fragment = self.phase_prompt(phase);
        if !phase_fragment.is_empty() {
            base.push_str("\n\n---\n\n## Execution Phase: ");
            base.push_str(phase.label());
            base.push('\n');
            base.push_str(phase_fragment.trim());
        }
        Ok(base)
    }
}

impl Default for PhasePromptProvider {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_env() -> EnvironmentContext {
        EnvironmentContext {
            working_directory: PathBuf::from("/tmp"),
            workspace_root: PathBuf::from("/tmp"),
            is_git_repo: false,
            platform: "test".into(),
            date: "2026-01-01".into(),
            git_status: None,
        }
    }

    #[test]
    fn provider_loads_all_prompts() {
        let p = PhasePromptProvider::new();
        let _ = p.phase_prompt(rustycode_protocol::ExecutionPhase::Explore);
        let _ = p.phase_prompt(rustycode_protocol::ExecutionPhase::Plan);
        let _ = p.phase_prompt(rustycode_protocol::ExecutionPhase::Act);
    }

    #[test]
    fn default_equals_new() {
        let a = PhasePromptProvider::new();
        let b = PhasePromptProvider::default();
        assert_eq!(a.phase_prompt(rustycode_protocol::ExecutionPhase::Explore),
                   b.phase_prompt(rustycode_protocol::ExecutionPhase::Explore));
    }

    #[tokio::test]
    async fn build_phase_prompt_explore_contains_label() {
        let provider = PhasePromptProvider::new();
        let prompt = provider.build_phase_prompt(
            rustycode_protocol::ExecutionPhase::Explore, "claude-3", None, &test_env(),
        ).await.unwrap();
        assert!(prompt.contains("Execution Phase: Explore"));
    }

    #[tokio::test]
    async fn build_phase_prompt_all_phases_have_labels() {
        let provider = PhasePromptProvider::new();
        for phase in rustycode_protocol::ExecutionPhase::all() {
            let prompt = provider.build_phase_prompt(*phase, "claude-3", None, &test_env())
                .await.unwrap();
            assert!(prompt.contains(&format!("Execution Phase: {}", phase)));
        }
    }
}
```

**Edit**: `crates/rustycode-prompt/src/lib.rs` -- add `pub mod phase_prompts;`

**Template Files** (create three):

`crates/rustycode-prompt/prompts/explore.txt`:
```
You are in the EXPLORE phase of a task. Your goal is to gather context without making any changes.

## Rules
- You may ONLY read files, search code, and explore the project structure.
- Do NOT modify any files or execute commands.
- Focus on understanding the codebase structure, relevant modules, and dependencies.
- Build a mental model of how the relevant code works.

## Your Objectives
1. Understand the task requirements fully.
2. Identify all files and modules relevant to the task.
3. Map dependencies and call chains.
4. Note any constraints, patterns, or conventions that apply.
5. Summarize your findings before transitioning to the Plan phase.

## When You Have Enough Context
Signal that you are ready to plan by summarizing:
- What you understood about the task
- Which files/modules are relevant
- What constraints or patterns you discovered
```

`crates/rustycode-prompt/prompts/plan.txt`:
```
You are in the PLAN phase. Propose a structured approach for the task.

## Rules
- You may read files and submit/review plans, but NOT modify files or execute commands.
- Think architecturally: consider trade-offs, edge cases, and testing strategy.
- Produce a structured plan before any implementation.

## Plan Structure
Your plan should include:
1. **Summary**: One-paragraph description of the approach.
2. **Files to Modify**: List each file with what will change and why.
3. **Implementation Order**: Step-by-step sequence respecting dependencies.
4. **Testing Strategy**: How you will verify correctness at each step.
5. **Risks**: Potential pitfalls and mitigations.
6. **Success Criteria**: How to confirm the task is complete.

## Principles
- Prefer minimal, precise changes over broad refactors.
- Each step should be independently verifiable.
- Flag any assumptions that need validation.
- If the plan requires more than 8 files, consider splitting the task.
```

`crates/rustycode-prompt/prompts/act.txt`:
```
You are in the ACT phase. Execute the approved plan with precision.

## Rules
- You have full access to all tools.
- Follow the plan precisely. Do not improvise changes outside the plan.
- Verify each step before proceeding to the next.

## Execution Principles
- Make surgical, minimal changes.
- After each modification, verify the change is correct (compile check, test, etc.).
- If you encounter an issue the plan did not anticipate:
  1. Document the deviation.
  2. Make the minimal adjustment needed.
  3. Do not expand scope.
- Run the test suite after all modifications are complete.
- If tests fail, fix the root cause, not the symptom.

## Completion
When finished, confirm:
- All planned changes are implemented.
- All tests pass.
- No unintended side effects.
- Summary of what was done and any deviations from the plan.
```

**Tests**: 4 tests.

---

### Chunk 4: Structured Plan Schema and Validation

**File**: `crates/rustycode-protocol/src/execution_plan.rs` (new)

```rust
//! Execution plan schema for the Explore-Plan-Act lifecycle.
//!
//! Extends `ConvoyPlan` with validation logic and execution-phase metadata.

use crate::convoy_plan::{CommandPlan, ConvoyPlan, ConvoyRisk, FilePlan, PlanApproval};
use crate::execution_phase::ExecutionPhase;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Validation error for an execution plan.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanValidationError {
    #[error("plan summary is empty")]
    EmptySummary,
    #[error("plan approach is empty")]
    EmptyApproach,
    #[error("plan has no files to modify and no commands to run")]
    NoActions,
    #[error("plan has no success criteria")]
    NoSuccessCriteria,
    #[error("file plan has empty path")]
    EmptyFilePath,
    #[error("file plan has empty description")]
    EmptyFileDescription,
    #[error("command plan has empty command")]
    EmptyCommand,
    #[error("risk '{description}' has no mitigation")]
    UnmitigatedRisk { description: String },
    #[error("plan not approved (current phase: {phase:?})")]
    NotApproved { phase: ExecutionPhase },
}

/// A validated execution plan ready for the Act phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecutionPlan {
    /// The underlying convoy plan.
    pub convoy_plan: ConvoyPlan,
    /// Which phase produced this plan.
    pub planned_in_phase: ExecutionPhase,
    /// When the plan was validated.
    pub validated_at: Option<DateTime<Utc>>,
}

impl ExecutionPlan {
    /// Validate a convoy plan for the given phase.
    pub fn validate(
        convoy_plan: ConvoyPlan,
        phase: ExecutionPhase,
    ) -> Result<Self, PlanValidationError> {
        if convoy_plan.summary.trim().is_empty() {
            return Err(PlanValidationError::EmptySummary);
        }
        if convoy_plan.approach.trim().is_empty() {
            return Err(PlanValidationError::EmptyApproach);
        }
        if convoy_plan.files_to_modify.is_empty() && convoy_plan.commands_to_run.is_empty() {
            return Err(PlanValidationError::NoActions);
        }
        if convoy_plan.success_criteria.is_empty() {
            return Err(PlanValidationError::NoSuccessCriteria);
        }
        for fp in &convoy_plan.files_to_modify {
            if fp.path.trim().is_empty() {
                return Err(PlanValidationError::EmptyFilePath);
            }
            if fp.description.trim().is_empty() {
                return Err(PlanValidationError::EmptyFileDescription);
            }
        }
        for cp in &convoy_plan.commands_to_run {
            if cp.command.trim().is_empty() {
                return Err(PlanValidationError::EmptyCommand);
            }
        }
        for risk in &convoy_plan.risks {
            if risk.mitigation.trim().is_empty() {
                return Err(PlanValidationError::UnmitigatedRisk {
                    description: risk.description.clone(),
                });
            }
        }
        Ok(Self { convoy_plan, planned_in_phase: phase, validated_at: None })
    }

    /// Validate and mark validation time.
    pub fn validate_now(
        convoy_plan: ConvoyPlan,
        phase: ExecutionPhase,
    ) -> Result<Self, PlanValidationError> {
        let mut plan = Self::validate(convoy_plan, phase)?;
        plan.validated_at = Some(Utc::now());
        Ok(plan)
    }

    /// Check whether this plan is approved for execution.
    pub fn is_approved(&self) -> bool {
        self.convoy_plan.approval.approved
    }

    /// Ensure plan is approved before transitioning to Act phase.
    pub fn require_approved(&self) -> Result<(), PlanValidationError> {
        if self.convoy_plan.approval.approved {
            Ok(())
        } else {
            Err(PlanValidationError::NotApproved { phase: self.planned_in_phase })
        }
    }

    pub fn summary(&self) -> &str { &self.convoy_plan.summary }
    pub fn files(&self) -> &[FilePlan] { &self.convoy_plan.files_to_modify }
    pub fn commands(&self) -> &[CommandPlan] { &self.convoy_plan.commands_to_run }
    pub fn success_criteria(&self) -> &[String] { &self.convoy_plan.success_criteria }
    pub fn risks(&self) -> &[ConvoyRisk] { &self.convoy_plan.risks }
}

/// Builder for creating execution plans programmatically.
#[derive(Debug, Clone)]
pub struct ExecutionPlanBuilder {
    summary: String,
    approach: String,
    files: Vec<FilePlan>,
    commands: Vec<CommandPlan>,
    risks: Vec<ConvoyRisk>,
    success_criteria: Vec<String>,
    estimated_cost_usd: f64,
}

impl ExecutionPlanBuilder {
    pub fn new(summary: impl Into<String>, approach: impl Into<String>) -> Self {
        Self {
            summary: summary.into(), approach: approach.into(),
            files: vec![], commands: vec![], risks: vec![],
            success_criteria: vec![], estimated_cost_usd: 0.0,
        }
    }

    pub fn file(mut self, path: impl Into<String>, description: impl Into<String>) -> Self {
        self.files.push(FilePlan { path: path.into(), description: description.into() });
        self
    }
    pub fn command(mut self, command: impl Into<String>, description: impl Into<String>) -> Self {
        self.commands.push(CommandPlan { command: command.into(), description: description.into() });
        self
    }
    pub fn risk(
        mut self, level: crate::team::RiskLevel,
        description: impl Into<String>, mitigation: impl Into<String>,
    ) -> Self {
        self.risks.push(ConvoyRisk {
            level, description: description.into(), mitigation: mitigation.into(),
        });
        self
    }
    pub fn success_criterion(mut self, criterion: impl Into<String>) -> Self {
        self.success_criteria.push(criterion.into());
        self
    }
    pub fn estimated_cost(mut self, cost_usd: f64) -> Self {
        self.estimated_cost_usd = cost_usd;
        self
    }

    /// Build and validate the execution plan.
    pub fn build(self, phase: ExecutionPhase) -> Result<ExecutionPlan, PlanValidationError> {
        let convoy_plan = ConvoyPlan {
            id: format!("plan-{}", uuid::Uuid::new_v4()),
            summary: self.summary, approach: self.approach,
            files_to_modify: self.files, commands_to_run: self.commands,
            risks: self.risks, estimated_cost_usd: self.estimated_cost_usd,
            success_criteria: self.success_criteria,
            approval: PlanApproval::default(), created_at: Utc::now(),
        };
        ExecutionPlan::validate(convoy_plan, phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::team::RiskLevel;

    fn valid_builder() -> ExecutionPlanBuilder {
        ExecutionPlanBuilder::new("Add feature X", "Implement via module Y")
            .file("src/y.rs", "Add X implementation")
            .command("cargo test", "Run tests")
            .success_criterion("Tests pass")
    }

    #[test]
    fn valid_plan_passes() {
        let plan = valid_builder().build(ExecutionPhase::Plan).unwrap();
        assert_eq!(plan.summary(), "Add feature X");
        assert_eq!(plan.files().len(), 1);
    }
    #[test]
    fn empty_summary_fails() {
        let r = ExecutionPlanBuilder::new("", "a").file("f.rs","d").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptySummary)));
    }
    #[test]
    fn empty_approach_fails() {
        let r = ExecutionPlanBuilder::new("s", "  ").file("f.rs","d").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyApproach)));
    }
    #[test]
    fn no_actions_fails() {
        let r = ExecutionPlanBuilder::new("s","a").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::NoActions)));
    }
    #[test]
    fn no_success_criteria_fails() {
        let r = ExecutionPlanBuilder::new("s","a").file("f.rs","d").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::NoSuccessCriteria)));
    }
    #[test]
    fn empty_file_path_fails() {
        let r = ExecutionPlanBuilder::new("s","a").file("","d").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyFilePath)));
    }
    #[test]
    fn empty_file_description_fails() {
        let r = ExecutionPlanBuilder::new("s","a").file("f.rs","").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyFileDescription)));
    }
    #[test]
    fn empty_command_fails() {
        let r = ExecutionPlanBuilder::new("s","a").command("","d").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyCommand)));
    }
    #[test]
    fn unmitigated_risk_fails() {
        let r = valid_builder().risk(RiskLevel::High, "data loss", "").build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::UnmitigatedRisk { .. })));
    }
    #[test]
    fn mitigated_risk_passes() {
        let r = valid_builder().risk(RiskLevel::Moderate, "perf", "benchmark").build(ExecutionPhase::Plan);
        assert!(r.is_ok());
    }
    #[test]
    fn unapproved_fails_require() {
        let plan = valid_builder().build(ExecutionPhase::Plan).unwrap();
        assert!(plan.require_approved().is_err());
    }
    #[test]
    fn approved_passes_require() {
        let mut plan = valid_builder().build(ExecutionPhase::Plan).unwrap();
        plan.convoy_plan.approval.approved = true;
        assert!(plan.require_approved().is_ok());
    }
    #[test]
    fn validate_now_sets_timestamp() {
        let convoy = valid_builder().build(ExecutionPhase::Plan).unwrap().convoy_plan;
        let plan = ExecutionPlan::validate_now(convoy, ExecutionPhase::Plan).unwrap();
        assert!(plan.validated_at.is_some());
    }
    #[test]
    fn plan_with_only_commands_valid() {
        let r = ExecutionPlanBuilder::new("s","a").command("cargo test","t").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(r.is_ok());
    }
    #[test]
    fn plan_with_only_files_valid() {
        let r = ExecutionPlanBuilder::new("s","a").file("src/lib.rs","m").success_criterion("ok").build(ExecutionPhase::Plan);
        assert!(r.is_ok());
    }
    #[test]
    fn serde_roundtrip() {
        let plan = valid_builder().build(ExecutionPhase::Plan).unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let back: ExecutionPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.summary(), plan.summary());
    }
    #[test]
    fn accessors() {
        let plan = valid_builder()
            .risk(RiskLevel::Low, "minor", "ignore")
            .success_criterion("extra")
            .build(ExecutionPhase::Plan).unwrap();
        assert_eq!(plan.files().len(), 1);
        assert_eq!(plan.commands().len(), 1);
        assert_eq!(plan.risks().len(), 1);
        assert!(plan.success_criteria().len() >= 2);
    }
}
```

**Edit**: `crates/rustycode-protocol/src/lib.rs` -- add:

```rust
pub mod execution_plan;
pub use execution_plan::{ExecutionPlan, ExecutionPlanBuilder, PlanValidationError};
```

**Tests**: 18 tests.

---

### Chunk 5: Phase Enforcement in Orchestration Pipeline

**Edit**: `crates/rustycode-orchestration/src/task_context.rs`

Add to imports:
```rust
use rustycode_protocol::execution_phase::{ExecutionPhase, PhaseSkipConfig, PhaseTransitionError};
```

Add fields to `TaskContext` struct:
```rust
pub execution_phase: ExecutionPhase,
pub skip_config: PhaseSkipConfig,
```

In `TaskContext::new()`, add to initializer:
```rust
execution_phase: ExecutionPhase::Explore,
skip_config: PhaseSkipConfig::default(),
```

Add methods:
```rust
    /// Create a TaskContext with phase skip configuration.
    pub fn with_skip_config(
        task_id: String, original_request: String, skip_config: PhaseSkipConfig,
    ) -> Self {
        Self {
            execution_phase: skip_config.starting_phase(),
            skip_config,
            ..Self::new(task_id, original_request)
        }
    }

    /// Transition to the next execution phase.
    pub fn advance_execution_phase(&mut self) -> Result<ExecutionPhase, PhaseTransitionError> {
        let current = self.execution_phase;
        let next = current.next()
            .ok_or(PhaseTransitionError::AlreadyComplete { from: current })?;
        current.transition_to(next)?;
        self.execution_phase = next;
        tracing::info!(task_id = %self.task_id, from = %current, to = %next, "Execution phase transitioned");
        Ok(next)
    }

    /// Force-set the execution phase (for skip-ahead and testing).
    pub fn set_execution_phase(&mut self, phase: ExecutionPhase) {
        tracing::info!(task_id = %self.task_id, to = %phase, "Execution phase set directly");
        self.execution_phase = phase;
    }
```

**Edit**: `crates/rustycode-orchestration/src/bus.rs`

Add to `OrchestrationEvent` enum:
```rust
PhaseTransition {
    task_id: String,
    from: String,
    to: String,
},
```

**Edit**: `crates/rustycode-orchestration/src/error.rs`

Add variant:
```rust
#[error("tool '{tool}' denied in {phase} phase")]
PermissionDenied { tool: String, phase: String },
```

**File**: `crates/rustycode-orchestration/src/phase_lifecycle.rs` (new)

```rust
//! Phase lifecycle manager enforcing Explore-Plan-Act transitions.

use crate::bus::{BusHandle, OrchestrationEvent};
use crate::error::{OrchestrationError, Result};
use crate::task_context::TaskContext;
use rustycode_protocol::execution_phase::{ExecutionPhase, PhaseSkipConfig};
use rustycode_protocol::execution_plan::ExecutionPlan;

/// Manages the Explore-Plan-Act lifecycle for a task.
#[derive(Debug)]
pub struct PhaseLifecycleManager {
    bus: BusHandle,
}

impl PhaseLifecycleManager {
    pub fn new(bus: BusHandle) -> Self { Self { bus } }

    /// Initialize a TaskContext for the Explore-Plan-Act lifecycle.
    pub fn create_context(
        &self, task_id: String, task: String, skip_config: PhaseSkipConfig,
    ) -> TaskContext {
        TaskContext::with_skip_config(task_id, task, skip_config)
    }

    /// Check if a tool is allowed in the current execution phase.
    pub fn check_tool_access(
        &self, ctx: &TaskContext, tool_name: &str,
    ) -> std::result::Result<(), OrchestrationError> {
        let decision = ctx.execution_phase.decide_tool(tool_name);
        if decision.is_allowed() {
            Ok(())
        } else {
            Err(OrchestrationError::PermissionDenied {
                tool: tool_name.to_string(),
                phase: ctx.execution_phase.to_string(),
            })
        }
    }

    /// Transition to the next phase, publishing a bus event.
    pub fn advance_phase(&self, ctx: &mut TaskContext) -> Result<ExecutionPhase> {
        let from = ctx.execution_phase;
        let new_phase = ctx.advance_execution_phase().map_err(|e| OrchestrationError::Internal {
            message: e.to_string(),
        })?;
        self.bus.publish(OrchestrationEvent::PhaseTransition {
            task_id: ctx.task_id.clone(),
            from: from.to_string(),
            to: new_phase.to_string(),
        });
        Ok(new_phase)
    }

    /// Validate a plan before transitioning from Plan to Act.
    pub fn validate_plan_for_act(
        &self, plan: &ExecutionPlan,
    ) -> std::result::Result<(), OrchestrationError> {
        plan.require_approved().map_err(|e| OrchestrationError::Internal {
            message: e.to_string(),
        })
    }
}
```

**Edit**: `crates/rustycode-orchestration/src/lib.rs` -- add `pub mod phase_lifecycle;`

**New task_context tests** (add to existing `mod tests`):

```rust
    #[test]
    fn test_execution_phase_default_is_explore() {
        let ctx = TaskContext::new("t1".into(), "task".into());
        assert_eq!(ctx.execution_phase, rustycode_protocol::ExecutionPhase::Explore);
    }
    #[test]
    fn test_advance_execution_phase_explore_to_plan() {
        let mut ctx = TaskContext::new("t1".into(), "task".into());
        assert_eq!(ctx.advance_execution_phase().unwrap(), rustycode_protocol::ExecutionPhase::Plan);
    }
    #[test]
    fn test_advance_execution_phase_plan_to_act() {
        let mut ctx = TaskContext::new("t1".into(), "task".into());
        ctx.execution_phase = rustycode_protocol::ExecutionPhase::Plan;
        assert_eq!(ctx.advance_execution_phase().unwrap(), rustycode_protocol::ExecutionPhase::Act);
    }
    #[test]
    fn test_advance_execution_phase_act_fails() {
        let mut ctx = TaskContext::new("t1".into(), "task".into());
        ctx.execution_phase = rustycode_protocol::ExecutionPhase::Act;
        assert!(ctx.advance_execution_phase().is_err());
    }
    #[test]
    fn test_with_skip_config_skip_explore() {
        let ctx = TaskContext::with_skip_config("t1".into(), "task".into(),
            rustycode_protocol::PhaseSkipConfig::skip_explore());
        assert_eq!(ctx.execution_phase, rustycode_protocol::ExecutionPhase::Plan);
    }
    #[test]
    fn test_with_skip_config_skip_to_act() {
        let ctx = TaskContext::with_skip_config("t1".into(), "task".into(),
            rustycode_protocol::PhaseSkipConfig::skip_to_act());
        assert_eq!(ctx.execution_phase, rustycode_protocol::ExecutionPhase::Act);
    }
    #[test]
    fn test_set_execution_phase() {
        let mut ctx = TaskContext::new("t1".into(), "task".into());
        ctx.set_execution_phase(rustycode_protocol::ExecutionPhase::Act);
        assert_eq!(ctx.execution_phase, rustycode_protocol::ExecutionPhase::Act);
    }
```

**Tests**: 7 new task_context tests.

---

### Chunk 6: Integration Tests

**File**: `crates/rustycode-orchestration/tests/phase_lifecycle.rs` (new)

```rust
//! Integration tests for the Explore-Plan-Act lifecycle.

use rustycode_orchestration::bus::BusHandle;
use rustycode_orchestration::phase_lifecycle::PhaseLifecycleManager;
use rustycode_orchestration::task_context::TaskContext;
use rustycode_protocol::execution_phase::{ExecutionPhase, PhaseSkipConfig};
use rustycode_protocol::execution_plan::{ExecutionPlanBuilder, PlanValidationError};
use rustycode_protocol::team::RiskLevel;

fn make_mgr() -> PhaseLifecycleManager { PhaseLifecycleManager::new(BusHandle::new(64)) }

// --- Explore phase tool gating ---
#[test]
fn explore_blocks_write() { assert!(make_mgr().check_tool_access(&TaskContext::new("t".into(),"t".into()), "write_file").is_err()); }
#[test]
fn explore_blocks_bash() { assert!(make_mgr().check_tool_access(&TaskContext::new("t".into(),"t".into()), "bash").is_err()); }
#[test]
fn explore_allows_read() { assert!(make_mgr().check_tool_access(&TaskContext::new("t".into(),"t".into()), "read_file").is_ok()); }
#[test]
fn explore_allows_grep() { assert!(make_mgr().check_tool_access(&TaskContext::new("t".into(),"t".into()), "grep").is_ok()); }
#[test]
fn explore_allows_glob() { assert!(make_mgr().check_tool_access(&TaskContext::new("t".into(),"t".into()), "glob").is_ok()); }

// --- Plan phase tool gating ---
#[test]
fn plan_blocks_write() {
    let mut ctx = TaskContext::new("t".into(),"t".into());
    ctx.execution_phase = ExecutionPhase::Plan;
    assert!(make_mgr().check_tool_access(&ctx, "write_file").is_err());
}
#[test]
fn plan_blocks_bash() {
    let mut ctx = TaskContext::new("t".into(),"t".into());
    ctx.execution_phase = ExecutionPhase::Plan;
    assert!(make_mgr().check_tool_access(&ctx, "bash").is_err());
}
#[test]
fn plan_allows_plan_tools() {
    let mut ctx = TaskContext::new("t".into(),"t".into());
    ctx.execution_phase = ExecutionPhase::Plan;
    let mgr = make_mgr();
    assert!(mgr.check_tool_access(&ctx, "submit_plan").is_ok());
    assert!(mgr.check_tool_access(&ctx, "review_plan").is_ok());
}

// --- Act phase tool gating ---
#[test]
fn act_allows_all() {
    let mut ctx = TaskContext::new("t".into(),"t".into());
    ctx.execution_phase = ExecutionPhase::Act;
    let mgr = make_mgr();
    assert!(mgr.check_tool_access(&ctx, "write_file").is_ok());
    assert!(mgr.check_tool_access(&ctx, "bash").is_ok());
    assert!(mgr.check_tool_access(&ctx, "read_file").is_ok());
}

// --- Phase transition sequence ---
#[test]
fn full_transition_sequence() {
    let mgr = make_mgr();
    let mut ctx = mgr.create_context("t".into(),"task".into(), PhaseSkipConfig::new());
    assert_eq!(ctx.execution_phase, ExecutionPhase::Explore);
    assert_eq!(mgr.advance_phase(&mut ctx).unwrap(), ExecutionPhase::Plan);
    assert_eq!(mgr.advance_phase(&mut ctx).unwrap(), ExecutionPhase::Act);
}
#[test]
fn act_cannot_advance() {
    let mgr = make_mgr();
    let mut ctx = mgr.create_context("t".into(),"task".into(), PhaseSkipConfig::skip_to_act());
    assert!(mgr.advance_phase(&mut ctx).is_err());
}

// --- Skip-ahead flags ---
#[test]
fn skip_explore_starts_at_plan() {
    let ctx = make_mgr().create_context("t".into(),"task".into(), PhaseSkipConfig::skip_explore());
    assert_eq!(ctx.execution_phase, ExecutionPhase::Plan);
}
#[test]
fn skip_to_act_starts_at_act() {
    let ctx = make_mgr().create_context("t".into(),"task".into(), PhaseSkipConfig::skip_to_act());
    assert_eq!(ctx.execution_phase, ExecutionPhase::Act);
}
#[test]
fn skip_explore_can_advance_to_act() {
    let mgr = make_mgr();
    let mut ctx = mgr.create_context("t".into(),"task".into(), PhaseSkipConfig::skip_explore());
    assert_eq!(mgr.advance_phase(&mut ctx).unwrap(), ExecutionPhase::Act);
}

// --- Plan validation ---
#[test]
fn unapproved_plan_fails_act_gate() {
    let plan = ExecutionPlanBuilder::new("X","Y").file("a.rs","b").success_criterion("ok").build(ExecutionPhase::Plan).unwrap();
    assert!(make_mgr().validate_plan_for_act(&plan).is_err());
}
#[test]
fn approved_plan_passes_act_gate() {
    let mut plan = ExecutionPlanBuilder::new("X","Y").file("a.rs","b").success_criterion("ok").build(ExecutionPhase::Plan).unwrap();
    plan.convoy_plan.approval.approved = true;
    assert!(make_mgr().validate_plan_for_act(&plan).is_ok());
}
#[test]
fn invalid_plan_fails_build() {
    assert!(matches!(
        ExecutionPlanBuilder::new("","a").file("f","d").success_criterion("ok").build(ExecutionPhase::Plan),
        Err(PlanValidationError::EmptySummary)
    ));
}

// --- Bus events ---
#[test]
fn transition_publishes_event() {
    let bus = BusHandle::new(64);
    let mut rx = bus.subscribe();
    let mgr = PhaseLifecycleManager::new(bus);
    let mut ctx = mgr.create_context("t1".into(),"task".into(), PhaseSkipConfig::new());
    mgr.advance_phase(&mut ctx).unwrap();
    match rx.try_recv().unwrap() {
        rustycode_orchestration::bus::OrchestrationEvent::PhaseTransition { from, to, .. } => {
            assert_eq!(from, "Explore");
            assert_eq!(to, "Plan");
        }
        other => panic!("Expected PhaseTransition, got {:?}", other),
    }
}
#[test]
fn multiple_transitions_publish_multiple_events() {
    let bus = BusHandle::new(64);
    let mut rx = bus.subscribe();
    let mgr = PhaseLifecycleManager::new(bus);
    let mut ctx = mgr.create_context("t1".into(),"task".into(), PhaseSkipConfig::new());
    mgr.advance_phase(&mut ctx).unwrap();
    mgr.advance_phase(&mut ctx).unwrap();
    assert!(matches!(rx.try_recv().unwrap(), rustycode_orchestration::bus::OrchestrationEvent::PhaseTransition { .. }));
    assert!(matches!(rx.try_recv().unwrap(), rustycode_orchestration::bus::OrchestrationEvent::PhaseTransition { .. }));
}

// --- Comprehensive tool access matrix ---
#[test]
fn read_tools_allowed_in_all_phases() {
    let mgr = make_mgr();
    for tool in &["read_file", "grep", "glob", "list_dir", "web_search"] {
        for phase in ExecutionPhase::all() {
            let mut ctx = TaskContext::new("t".into(),"t".into());
            ctx.execution_phase = *phase;
            assert!(mgr.check_tool_access(&ctx, tool).is_ok(), "{} should be allowed in {:?}", tool, phase);
        }
    }
}
#[test]
fn write_tools_blocked_in_explore_and_plan() {
    let mgr = make_mgr();
    for tool in &["write_file", "edit_file", "bash"] {
        for phase in &[ExecutionPhase::Explore, ExecutionPhase::Plan] {
            let mut ctx = TaskContext::new("t".into(),"t".into());
            ctx.execution_phase = *phase;
            assert!(mgr.check_tool_access(&ctx, tool).is_err(), "{} should be blocked in {:?}", tool, phase);
        }
    }
}
```

**Tests**: 25 integration tests.

---

## Execution Order

```
Chunk 1: ExecutionPhase enum       [protocol crate, no deps]
   |
   +---> Chunk 2: Permission enforcement  [parallel]
   +---> Chunk 3: Phase prompts            [parallel]
   +---> Chunk 4: ExecutionPlan schema     [parallel]
   |
   v
Chunk 5: Pipeline integration       [depends on Chunks 1-4]
   |
   v
Chunk 6: Integration tests          [depends on all chunks]
```

Chunks 2, 3, and 4 are independent after Chunk 1 and can proceed in parallel.

---

## Estimated Test Count

| Chunk | New Tests |
|-------|-----------|
| Chunk 1 | 20 |
| Chunk 2 | 11 |
| Chunk 3 | 4 |
| Chunk 4 | 18 |
| Chunk 5 | 7 |
| Chunk 6 | 25 |
| **Total** | **~85** |

---

## Relationship Between ExecutionPhase and TaskPhase

These two enums serve different purposes and coexist on `TaskContext`:

- **`ExecutionPhase`** (Explore/Plan/Act): Controls tool access permissions and system prompts. Governs WHAT the agent can do.
- **`TaskPhase`** (Planning/Tier2/Tier3/Tier4/Tier5/Completed): Controls escalation tier and execution strategy. Governs HOW the agent executes.

A task in `ExecutionPhase::Act` may still escalate from `TaskPhase::Tier2Execution` to `TaskPhase::Tier3Review` if a step fails verification. The two are orthogonal.

---

## Non-Goals (Deferred)

1. **Automatic phase detection** -- Agent deciding when to transition (requires LLM integration beyond scope)
2. **Phase rollback** -- Going back from Plan to Explore (violates forward progress principle)
3. **Per-subagent phase tracking** -- Each subagent in a team having independent phase state
4. **Phase persistence** -- Storing phase state across sessions (depends on Phase 1 memory architecture)
5. **TUI/CLI wiring** -- Connecting the phase lifecycle to user-facing commands (separate integration task)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| `PermissionMode::Plan` reuse for Explore may be too restrictive | Low | `ExecutionPhase::decide_tool()` provides fine-grained control; `PermissionMode` is a coarse fallback |
| Skip-ahead flags may bypass validation | Medium | `PhaseSkipConfig` only affects starting phase; plan validation still runs when a plan is submitted |
| Phase prompts add token overhead | Low | Phase fragments are ~200 tokens each; acceptable for the structure they provide |
| Two phase enums may cause confusion | Medium | Documented above: `ExecutionPhase` = permissions, `TaskPhase` = escalation tier |
