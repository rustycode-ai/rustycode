//! Milestone grouping types for multi-plan autonomous execution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::{MilestoneId, Plan, PlanId, SessionId};

/// A dependency edge for a plan inside a milestone DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDependency {
    pub plan_id: PlanId,
    pub depends_on: Vec<PlanId>,
}

/// A milestone groups multiple plans into a dependency-aware execution unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Milestone {
    pub id: MilestoneId,
    pub session_id: SessionId,
    pub title: String,
    pub description: String,
    pub status: MilestoneStatus,
    #[serde(default)]
    pub plan_ids: Vec<PlanId>,
    #[serde(default)]
    pub plan_dependencies: Vec<PlanDependency>,
    #[serde(default)]
    pub success_criteria: Vec<String>,
    pub validation_command: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Lifecycle state for a milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MilestoneStatus {
    #[default]
    Draft,
    Planning,
    Ready,
    Active,
    Validating,
    Completed,
    Paused,
    Failed,
}

impl MilestoneStatus {
    /// Return `true` if the transition is allowed by the milestone lifecycle.
    #[allow(clippy::match_like_matches_macro, clippy::unnested_or_patterns)]
    pub const fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (Self::Draft, Self::Planning)
            | (Self::Draft, Self::Paused)
            | (Self::Draft, Self::Failed)
            | (Self::Planning, Self::Ready)
            | (Self::Planning, Self::Paused)
            | (Self::Planning, Self::Failed)
            | (Self::Ready, Self::Active)
            | (Self::Ready, Self::Paused)
            | (Self::Ready, Self::Failed)
            | (Self::Active, Self::Validating)
            | (Self::Active, Self::Paused)
            | (Self::Active, Self::Failed)
            | (Self::Validating, Self::Completed)
            | (Self::Validating, Self::Paused)
            | (Self::Validating, Self::Failed)
            | (Self::Paused, Self::Draft)
            | (Self::Paused, Self::Planning)
            | (Self::Paused, Self::Ready)
            | (Self::Paused, Self::Active)
            | (Self::Paused, Self::Validating)
            | (Self::Paused, Self::Failed)
            | (Self::Failed, Self::Draft)
            | (Self::Failed, Self::Planning)
            | (Self::Failed, Self::Paused) => true,
            _ => false,
        }
    }
}

impl fmt::Display for MilestoneStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => write!(f, "Draft"),
            Self::Planning => write!(f, "Planning"),
            Self::Ready => write!(f, "Ready"),
            Self::Active => write!(f, "Active"),
            Self::Validating => write!(f, "Validating"),
            Self::Completed => write!(f, "Completed"),
            Self::Paused => write!(f, "Paused"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

impl Milestone {
    /// Compute the plan IDs that are ready to execute.
    ///
    /// A plan is ready when:
    /// - it belongs to this milestone
    /// - its status is `Draft` or `Ready`
    /// - all declared dependencies are completed
    pub fn ready_plans(&self, plans: &[Plan]) -> Vec<PlanId> {
        let completed: HashSet<PlanId> = plans
            .iter()
            .filter(|plan| plan.status == crate::PlanStatus::Completed)
            .map(|plan| plan.id.clone())
            .collect();

        let plan_lookup: HashMap<PlanId, &Plan> =
            plans.iter().map(|plan| (plan.id.clone(), plan)).collect();
        let dependency_lookup: HashMap<PlanId, Vec<PlanId>> = self
            .plan_dependencies
            .iter()
            .map(|dependency| (dependency.plan_id.clone(), dependency.depends_on.clone()))
            .collect();
        let milestone_plans: HashSet<PlanId> = self.plan_ids.iter().cloned().collect();

        self.plan_ids
            .iter()
            .filter_map(|plan_id| {
                let plan = plan_lookup.get(plan_id)?;
                if plan.milestone_id.as_ref() != Some(&self.id)
                    && !milestone_plans.contains(&plan.id)
                {
                    return None;
                }

                if !matches!(plan.status, crate::PlanStatus::Draft | crate::PlanStatus::Ready) {
                    return None;
                }

                let dependencies = dependency_lookup.get(&plan.id).cloned().unwrap_or_default();
                if dependencies.iter().all(|dependency| completed.contains(dependency)) {
                    Some(plan.id.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PlanStatus, PlanStep, StepStatus};

    #[test]
    fn status_transition_matrix_is_exhaustive() {
        assert!(MilestoneStatus::Draft.can_transition_to(MilestoneStatus::Planning));
        assert!(MilestoneStatus::Planning.can_transition_to(MilestoneStatus::Ready));
        assert!(MilestoneStatus::Ready.can_transition_to(MilestoneStatus::Active));
        assert!(MilestoneStatus::Active.can_transition_to(MilestoneStatus::Validating));
        assert!(MilestoneStatus::Validating.can_transition_to(MilestoneStatus::Completed));
        assert!(!MilestoneStatus::Completed.can_transition_to(MilestoneStatus::Draft));
    }

    #[test]
    fn ready_plans_honors_dependencies() {
        let session_id = SessionId::new();
        let milestone_id = MilestoneId::new();
        let plan_a = Plan {
            id: PlanId::new(),
            session_id: session_id.clone(),
            milestone_id: Some(milestone_id.clone()),
            task: "plan a".into(),
            created_at: Utc::now(),
            status: PlanStatus::Completed,
            summary: "A".into(),
            approach: String::new(),
            steps: vec![PlanStep {
                order: 0,
                title: "step".into(),
                description: "step".into(),
                tools: vec![],
                expected_outcome: String::new(),
                rollback_hint: String::new(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        let plan_b = Plan {
            id: PlanId::new(),
            session_id,
            milestone_id: Some(milestone_id.clone()),
            task: "plan b".into(),
            created_at: Utc::now(),
            status: PlanStatus::Ready,
            summary: "B".into(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        let milestone = Milestone {
            id: milestone_id,
            session_id: plan_b.session_id.clone(),
            title: "Milestone".into(),
            description: "Test".into(),
            status: MilestoneStatus::Active,
            plan_ids: vec![plan_a.id.clone(), plan_b.id.clone()],
            plan_dependencies: vec![PlanDependency {
                plan_id: plan_b.id.clone(),
                depends_on: vec![plan_a.id.clone()],
            }],
            success_criteria: vec![],
            validation_command: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        };

        assert_eq!(milestone.ready_plans(&[plan_a, plan_b]).len(), 1);
    }
}
