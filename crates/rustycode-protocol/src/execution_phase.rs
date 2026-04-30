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
        &[
            ExecutionPhase::Explore,
            ExecutionPhase::Plan,
            ExecutionPhase::Act,
        ]
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

    /// Map this execution phase to the corresponding permission mode.
    pub fn permission_mode(&self) -> crate::permission_modes::PermissionMode {
        match self {
            Self::Explore | Self::Plan => crate::permission_modes::PermissionMode::Plan,
            Self::Act => crate::permission_modes::PermissionMode::AcceptEdits,
        }
    }

    /// Decide whether a tool is allowed in this execution phase.
    pub fn decide_tool(&self, tool_name: &str) -> crate::permission_modes::PermissionDecision {
        use crate::permission_modes::{is_plan_tool, is_read_only_tool, PermissionDecision};
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
                    PermissionDecision::Allow {
                        reason: "plan tool allowed in Plan phase".into(),
                    }
                } else {
                    PermissionDecision::Deny {
                        reason: format!(
                            "{} blocked in Plan phase (read-only + plan tools)",
                            tool_name
                        ),
                    }
                }
            }
            Self::Act => PermissionDecision::Allow {
                reason: format!("{} allowed in Act phase", tool_name),
            },
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
        Self {
            skip_explore: false,
            skip_plan: false,
        }
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
        Self {
            skip_explore: true,
            skip_plan: false,
        }
    }

    /// Skip Explore and Plan (jump to Act).
    pub const fn skip_to_act() -> Self {
        Self {
            skip_explore: true,
            skip_plan: true,
        }
    }
}

/// Error from an invalid phase transition.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PhaseTransitionError {
    #[error("cannot transition from {from:?} to {attempted:?}; expected {expected:?}")]
    OutOfOrder {
        from: ExecutionPhase,
        attempted: ExecutionPhase,
        expected: ExecutionPhase,
    },
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
        assert!(ExecutionPhase::Explore
            .transition_to(ExecutionPhase::Plan)
            .is_ok());
    }
    #[test]
    fn valid_transition_plan_to_act() {
        assert!(ExecutionPhase::Plan
            .transition_to(ExecutionPhase::Act)
            .is_ok());
    }
    #[test]
    fn invalid_transition_explore_to_act() {
        let err = ExecutionPhase::Explore
            .transition_to(ExecutionPhase::Act)
            .unwrap_err();
        assert!(matches!(err, PhaseTransitionError::OutOfOrder { .. }));
    }
    #[test]
    fn invalid_transition_act_to_plan() {
        let err = ExecutionPhase::Act
            .transition_to(ExecutionPhase::Plan)
            .unwrap_err();
        assert!(matches!(err, PhaseTransitionError::AlreadyComplete { .. }));
    }
    #[test]
    fn same_phase_transition_is_ok() {
        assert!(ExecutionPhase::Explore
            .transition_to(ExecutionPhase::Explore)
            .is_ok());
        assert!(ExecutionPhase::Plan
            .transition_to(ExecutionPhase::Plan)
            .is_ok());
        assert!(ExecutionPhase::Act
            .transition_to(ExecutionPhase::Act)
            .is_ok());
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
        assert_eq!(
            PhaseSkipConfig::default().starting_phase(),
            ExecutionPhase::Explore
        );
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
        let config = PhaseSkipConfig {
            skip_explore: true,
            skip_plan: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: PhaseSkipConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }
}
