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
        Ok(Self {
            convoy_plan,
            planned_in_phase: phase,
            validated_at: None,
        })
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
            Err(PlanValidationError::NotApproved {
                phase: self.planned_in_phase,
            })
        }
    }

    pub fn summary(&self) -> &str {
        &self.convoy_plan.summary
    }
    pub fn files(&self) -> &[FilePlan] {
        &self.convoy_plan.files_to_modify
    }
    pub fn commands(&self) -> &[CommandPlan] {
        &self.convoy_plan.commands_to_run
    }
    pub fn success_criteria(&self) -> &[String] {
        &self.convoy_plan.success_criteria
    }
    pub fn risks(&self) -> &[ConvoyRisk] {
        &self.convoy_plan.risks
    }
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
            summary: summary.into(),
            approach: approach.into(),
            files: vec![],
            commands: vec![],
            risks: vec![],
            success_criteria: vec![],
            estimated_cost_usd: 0.0,
        }
    }

    pub fn file(mut self, path: impl Into<String>, description: impl Into<String>) -> Self {
        self.files.push(FilePlan {
            path: path.into(),
            description: description.into(),
        });
        self
    }
    pub fn command(mut self, command: impl Into<String>, description: impl Into<String>) -> Self {
        self.commands.push(CommandPlan {
            command: command.into(),
            description: description.into(),
        });
        self
    }
    pub fn risk(
        mut self,
        level: crate::team::RiskLevel,
        description: impl Into<String>,
        mitigation: impl Into<String>,
    ) -> Self {
        self.risks.push(ConvoyRisk {
            level,
            description: description.into(),
            mitigation: mitigation.into(),
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
            summary: self.summary,
            approach: self.approach,
            files_to_modify: self.files,
            commands_to_run: self.commands,
            risks: self.risks,
            estimated_cost_usd: self.estimated_cost_usd,
            success_criteria: self.success_criteria,
            approval: PlanApproval::default(),
            created_at: Utc::now(),
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
        let r = ExecutionPlanBuilder::new("", "a")
            .file("f.rs", "d")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptySummary)));
    }
    #[test]
    fn empty_approach_fails() {
        let r = ExecutionPlanBuilder::new("s", "  ")
            .file("f.rs", "d")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyApproach)));
    }
    #[test]
    fn no_actions_fails() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::NoActions)));
    }
    #[test]
    fn no_success_criteria_fails() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .file("f.rs", "d")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::NoSuccessCriteria)));
    }
    #[test]
    fn empty_file_path_fails() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .file("", "d")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyFilePath)));
    }
    #[test]
    fn empty_file_description_fails() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .file("f.rs", "")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyFileDescription)));
    }
    #[test]
    fn empty_command_fails() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .command("", "d")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(matches!(r, Err(PlanValidationError::EmptyCommand)));
    }
    #[test]
    fn unmitigated_risk_fails() {
        let r = valid_builder()
            .risk(RiskLevel::High, "data loss", "")
            .build(ExecutionPhase::Plan);
        assert!(matches!(
            r,
            Err(PlanValidationError::UnmitigatedRisk { .. })
        ));
    }
    #[test]
    fn mitigated_risk_passes() {
        let r = valid_builder()
            .risk(RiskLevel::Moderate, "perf", "benchmark")
            .build(ExecutionPhase::Plan);
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
        let convoy = valid_builder()
            .build(ExecutionPhase::Plan)
            .unwrap()
            .convoy_plan;
        let plan = ExecutionPlan::validate_now(convoy, ExecutionPhase::Plan).unwrap();
        assert!(plan.validated_at.is_some());
    }
    #[test]
    fn plan_with_only_commands_valid() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .command("cargo test", "t")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
        assert!(r.is_ok());
    }
    #[test]
    fn plan_with_only_files_valid() {
        let r = ExecutionPlanBuilder::new("s", "a")
            .file("src/lib.rs", "m")
            .success_criterion("ok")
            .build(ExecutionPhase::Plan);
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
            .build(ExecutionPhase::Plan)
            .unwrap();
        assert_eq!(plan.files().len(), 1);
        assert_eq!(plan.commands().len(), 1);
        assert_eq!(plan.risks().len(), 1);
        assert!(plan.success_criteria().len() >= 2);
    }
}
