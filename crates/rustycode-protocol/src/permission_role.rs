use crate::agent_protocol::AgentRole;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Permissions roles for high-level agent tasking and tool access gating.
///
/// **Deprecated in favor of `AgentRole`.** `PermissionRole` overlaps with
/// `AgentRole` but has fewer variants. New code should use `AgentRole` directly.
/// This type is kept for backwards compatibility with serialized data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionRole {
    /// Autonomous task executor. Can read and write.
    Worker,
    /// Plan-only agent. Can read and write plans, but not application code.
    Planner,
    /// Verification-only agent. Can read and run tests, but not write code.
    Reviewer,
    /// Research agent. Read-only exploration.
    Researcher,
    /// Strategic agent. Can define architecture and review plans.
    Architect,
    /// Critical reviewer. Only allowed to read and verify.
    Skeptic,
    /// Final decider. Allows terminal verification and high-level approval.
    Judge,
}

impl PermissionRole {
    /// Whether this role is intended to modify application code.
    pub fn can_write_code(&self) -> bool {
        matches!(self, Self::Worker | Self::Architect)
    }

    /// Whether this role is intended to create/modify plans.
    pub fn can_manage_plans(&self) -> bool {
        matches!(self, Self::Worker | Self::Planner | Self::Architect)
    }
}

impl fmt::Display for PermissionRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Worker => write!(f, "worker"),
            Self::Planner => write!(f, "planner"),
            Self::Reviewer => write!(f, "reviewer"),
            Self::Researcher => write!(f, "researcher"),
            Self::Architect => write!(f, "architect"),
            Self::Skeptic => write!(f, "skeptic"),
            Self::Judge => write!(f, "judge"),
        }
    }
}

/// Reason why a tool invocation was blocked by the permission system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ToolBlockedReason {
    /// The agent's role does not have permission to use this tool.
    NotAllowedForRole { tool: String, role: AgentRole },
    /// Plan approval is required before this tool can be used.
    RequiresApproval,
    /// The associated convoy plan has not been approved yet.
    ConvoyPlanNotApproved,
    /// The agent role is unknown to the permission system.
    UnknownRole(AgentRole),
}

impl fmt::Display for ToolBlockedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAllowedForRole { tool, role } => {
                write!(f, "Tool '{}' not allowed for {:?} role", tool, role)
            }
            Self::RequiresApproval => {
                write!(f, "Plan approval required before tool access")
            }
            Self::ConvoyPlanNotApproved => {
                write!(f, "Convoy plan not yet approved")
            }
            Self::UnknownRole(role) => {
                write!(f, "Unknown agent role: {:?}", role)
            }
        }
    }
}

impl std::error::Error for ToolBlockedReason {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_write_code() {
        assert!(PermissionRole::Worker.can_write_code());
        assert!(PermissionRole::Architect.can_write_code());
        assert!(!PermissionRole::Planner.can_write_code());
        assert!(!PermissionRole::Reviewer.can_write_code());
        assert!(!PermissionRole::Researcher.can_write_code());
        assert!(!PermissionRole::Skeptic.can_write_code());
        assert!(!PermissionRole::Judge.can_write_code());
    }

    #[test]
    fn test_can_manage_plans() {
        assert!(PermissionRole::Worker.can_manage_plans());
        assert!(PermissionRole::Planner.can_manage_plans());
        assert!(PermissionRole::Architect.can_manage_plans());
        assert!(!PermissionRole::Reviewer.can_manage_plans());
        assert!(!PermissionRole::Researcher.can_manage_plans());
    }

    #[test]
    fn test_display() {
        assert_eq!(PermissionRole::Worker.to_string(), "worker");
        assert_eq!(PermissionRole::Planner.to_string(), "planner");
        assert_eq!(PermissionRole::Reviewer.to_string(), "reviewer");
        assert_eq!(PermissionRole::Researcher.to_string(), "researcher");
        assert_eq!(PermissionRole::Architect.to_string(), "architect");
        assert_eq!(PermissionRole::Skeptic.to_string(), "skeptic");
        assert_eq!(PermissionRole::Judge.to_string(), "judge");
    }

    #[test]
    fn test_tool_blocked_reason_display() {
        let reason = ToolBlockedReason::NotAllowedForRole {
            tool: "Bash".into(),
            role: AgentRole::Reviewer,
        };
        assert!(reason.to_string().contains("Bash"));
        assert!(reason.to_string().contains("Reviewer"));

        assert!(ToolBlockedReason::RequiresApproval
            .to_string()
            .contains("approval"));
        assert!(ToolBlockedReason::ConvoyPlanNotApproved
            .to_string()
            .contains("not yet"));
        assert!(ToolBlockedReason::UnknownRole(AgentRole::Worker)
            .to_string()
            .contains("Unknown"));
    }

    #[test]
    fn test_permission_role_serde() {
        let role = PermissionRole::Architect;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"architect\"");
        let back: PermissionRole = serde_json::from_str(&json).unwrap();
        assert_eq!(back, role);
    }
}
