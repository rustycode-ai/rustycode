use crate::team::RiskLevel;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Plan for a single file modification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePlan {
    pub path: String,
    pub description: String,
}

/// Plan for a single command execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandPlan {
    pub command: String,
    pub description: String,
}

/// Risk associated with a convoy plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConvoyRisk {
    pub level: RiskLevel,
    pub description: String,
    pub mitigation: String,
}

/// Approval status for a convoy plan.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PlanApproval {
    pub approved: bool,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// Execution plan for a convoy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConvoyPlan {
    /// Unique plan identifier.
    pub id: String,
    /// High-level summary of the feature/change.
    pub summary: String,
    /// The technical approach/strategy.
    pub approach: String,
    /// List of files expected to be modified.
    pub files_to_modify: Vec<FilePlan>,
    /// List of commands expected to be run.
    pub commands_to_run: Vec<CommandPlan>,
    /// Risks identified during planning.
    pub risks: Vec<ConvoyRisk>,
    pub estimated_cost_usd: f64,
    /// Success criteria defined during planning.
    pub success_criteria: Vec<String>,
    /// Approval status and tracking.
    pub approval: PlanApproval,
    /// When this plan was created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_approval_default() {
        let approval = PlanApproval::default();
        assert!(!approval.approved);
        assert!(approval.approved_by.is_none());
        assert!(approval.approved_at.is_none());
    }

    #[test]
    fn test_convoy_plan_construction() {
        let now = Utc::now();
        let plan = ConvoyPlan {
            id: "plan-1".into(),
            summary: "Add auth".into(),
            approach: "JWT tokens".into(),
            files_to_modify: vec![FilePlan {
                path: "src/auth.rs".into(),
                description: "Add JWT validation".into(),
            }],
            commands_to_run: vec![CommandPlan {
                command: "cargo test".into(),
                description: "Run tests".into(),
            }],
            risks: vec![ConvoyRisk {
                level: RiskLevel::Moderate,
                description: "Token expiry".into(),
                mitigation: "Refresh tokens".into(),
            }],
            estimated_cost_usd: 0.5,
            success_criteria: vec!["Tests pass".into()],
            approval: PlanApproval::default(),
            created_at: now,
        };
        assert_eq!(plan.id, "plan-1");
        assert_eq!(plan.files_to_modify.len(), 1);
        assert_eq!(plan.commands_to_run.len(), 1);
        assert_eq!(plan.risks.len(), 1);
        assert!(!plan.approval.approved);
    }

    #[test]
    fn test_file_plan_equality() {
        let a = FilePlan {
            path: "a.rs".into(),
            description: "desc".into(),
        };
        let b = FilePlan {
            path: "a.rs".into(),
            description: "desc".into(),
        };
        assert_eq!(a, b);
    }

    #[test]
    fn test_convoy_plan_serde_roundtrip() {
        let plan = ConvoyPlan {
            id: "p1".into(),
            summary: "test".into(),
            approach: "approach".into(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![],
            estimated_cost_usd: 1.0,
            success_criteria: vec!["ok".into()],
            approval: PlanApproval::default(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_string(&plan).unwrap();
        let back: ConvoyPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "p1");
        assert_eq!(back.summary, "test");
    }
}
