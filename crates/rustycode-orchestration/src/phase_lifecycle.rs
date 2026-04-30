//! Phase lifecycle manager for Explore -> Plan -> Act.

use rustycode_protocol::{
    ConvoyPlan, ExecutionPhase, ExecutionPlan, PhaseSkipConfig, PhaseTransitionError,
    PlanValidationError,
};

/// Lifecycle state for a single task.
#[derive(Debug, Clone)]
pub struct PhaseLifecycleManager {
    current_phase: ExecutionPhase,
    skip_config: PhaseSkipConfig,
    execution_plan: Option<ExecutionPlan>,
}

impl PhaseLifecycleManager {
    /// Create a new lifecycle manager using the provided skip config.
    #[must_use]
    pub fn new(skip_config: PhaseSkipConfig) -> Self {
        Self {
            current_phase: skip_config.starting_phase(),
            skip_config,
            execution_plan: None,
        }
    }

    /// Current phase.
    pub const fn current_phase(&self) -> ExecutionPhase {
        self.current_phase
    }

    /// Current skip config.
    pub const fn skip_config(&self) -> PhaseSkipConfig {
        self.skip_config
    }

    /// Attach a plan produced in Plan phase.
    pub fn submit_plan(&mut self, plan: ConvoyPlan) -> Result<(), PlanValidationError> {
        let plan = ExecutionPlan::validate(plan, ExecutionPhase::Plan)?;
        self.execution_plan = Some(plan);
        self.current_phase = ExecutionPhase::Plan;
        Ok(())
    }

    /// Mark the plan as approved and transition to Act.
    pub fn approve_plan(&mut self) -> Result<(), PlanValidationError> {
        let plan = self
            .execution_plan
            .as_mut()
            .ok_or(PlanValidationError::EmptySummary)?;

        plan.convoy_plan.approval.approved = true;
        plan.require_approved()?;
        self.current_phase = ExecutionPhase::Act;
        Ok(())
    }

    /// Transition to Plan from Explore.
    pub fn enter_plan(&mut self) -> Result<(), PhaseTransitionError> {
        self.current_phase.transition_to(ExecutionPhase::Plan)?;
        self.current_phase = ExecutionPhase::Plan;
        Ok(())
    }

    /// Transition to Act from Plan.
    pub fn enter_act(&mut self) -> Result<(), PhaseTransitionError> {
        self.current_phase.transition_to(ExecutionPhase::Act)?;
        self.current_phase = ExecutionPhase::Act;
        Ok(())
    }

    /// Return the current plan if one has been submitted.
    pub const fn execution_plan(&self) -> Option<&ExecutionPlan> {
        self.execution_plan.as_ref()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rustycode_protocol::{CommandPlan, ConvoyRisk, FilePlan, PlanApproval, RiskLevel};

    fn make_plan() -> ConvoyPlan {
        ConvoyPlan {
            id: "plan-1".into(),
            summary: "Add feature".into(),
            approach: "Use current stack".into(),
            files_to_modify: vec![FilePlan {
                path: "src/lib.rs".into(),
                description: "update exports".into(),
            }],
            commands_to_run: vec![CommandPlan {
                command: "cargo test".into(),
                description: "run tests".into(),
            }],
            risks: vec![ConvoyRisk {
                level: RiskLevel::Moderate,
                description: "integration".into(),
                mitigation: "tests".into(),
            }],
            estimated_cost_usd: 0.0,
            success_criteria: vec!["ok".into()],
            approval: PlanApproval::default(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn starts_in_explore() {
        let manager = PhaseLifecycleManager::new(PhaseSkipConfig::new());
        assert_eq!(manager.current_phase(), ExecutionPhase::Explore);
    }

    #[test]
    fn skip_to_act_starts_in_act() {
        let manager = PhaseLifecycleManager::new(PhaseSkipConfig::skip_to_act());
        assert_eq!(manager.current_phase(), ExecutionPhase::Act);
    }

    #[test]
    fn plan_then_act_flow() {
        let mut manager = PhaseLifecycleManager::new(PhaseSkipConfig::new());
        manager.enter_plan().unwrap();
        manager.submit_plan(make_plan()).unwrap();
        assert_eq!(manager.current_phase(), ExecutionPhase::Plan);
        manager.approve_plan().unwrap();
        assert_eq!(manager.current_phase(), ExecutionPhase::Act);
        assert!(manager.execution_plan().is_some());
    }
}
