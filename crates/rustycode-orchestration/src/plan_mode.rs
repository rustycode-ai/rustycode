//! Compatibility `PlanMode` API for TUI and CLI callers.

use rustycode_protocol::{permission_role::ToolBlockedReason, AgentRole, ConvoyPlan, RiskLevel};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlanModeConfig {
    pub enabled: bool,
    pub require_approval: bool,
    pub cost_threshold: f64,
}

impl Default for PlanModeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: false,
            cost_threshold: 1.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ApprovalTrigger {
    HighRisk(RiskLevel),
    HighCost { estimated_usd: f64, threshold: f64 },
    ExternalChanges { examples: Vec<String> },
}

#[derive(Clone, Debug)]
pub struct ApprovalToken {
    pub plan_id: String,
    pub approved_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum PlanModeError {
    #[error("no plan to approve")]
    NoPlanToApprove,
    #[error("plan already approved")]
    AlreadyApproved,
    #[error("plan id mismatch: expected {expected}, got {actual}")]
    PlanIdMismatch { expected: String, actual: String },
    #[error("{0}")]
    ToolBlocked(String),
}

#[derive(Clone, Debug)]
pub struct PlanMode {
    config: PlanModeConfig,
    role_tool_matrix: HashMap<AgentRole, HashSet<&'static str>>,
    approval_triggers: Vec<ApprovalTrigger>,
    current_role: AgentRole,
    current_plan: Option<ConvoyPlan>,
}

impl Default for PlanMode {
    fn default() -> Self {
        Self::new(PlanModeConfig::default())
    }
}

impl PlanMode {
    pub fn new(config: PlanModeConfig) -> Self {
        let mut role_tool_matrix: HashMap<AgentRole, HashSet<&'static str>> = HashMap::new();
        role_tool_matrix.insert(
            AgentRole::Planner,
            HashSet::from([
                "read_file",
                "write_file",
                "bash",
                "task",
                "create_plan_from_template",
                "approve_plan",
            ]),
        );
        role_tool_matrix.insert(AgentRole::Architect, HashSet::from(["read_file", "task"]));
        role_tool_matrix.insert(AgentRole::Researcher, HashSet::from(["read_file", "task"]));
        role_tool_matrix.insert(AgentRole::Coordinator, HashSet::from(["read_file", "task"]));
        role_tool_matrix.insert(
            AgentRole::Worker,
            HashSet::from([
                "read_file",
                "task",
                "bash",
                "write_file",
                "edit_file",
                "glob",
                "grep",
            ]),
        );
        role_tool_matrix.insert(AgentRole::Builder, HashSet::from(["read_file", "task"]));
        role_tool_matrix.insert(AgentRole::Reviewer, HashSet::from(["read_file"]));
        role_tool_matrix.insert(AgentRole::Skeptic, HashSet::from(["read_file"]));
        role_tool_matrix.insert(AgentRole::Judge, HashSet::from(["read_file", "bash"]));

        Self {
            approval_triggers: vec![
                ApprovalTrigger::HighRisk(RiskLevel::High),
                ApprovalTrigger::HighCost {
                    estimated_usd: 0.0,
                    threshold: config.cost_threshold,
                },
            ],
            role_tool_matrix,
            config,
            current_role: AgentRole::Planner,
            current_plan: None,
        }
    }

    pub const fn config(&self) -> &PlanModeConfig {
        &self.config
    }

    pub const fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub const fn requires_approval(&self) -> bool {
        self.config.require_approval
    }

    pub const fn current_role(&self) -> AgentRole {
        self.current_role
    }

    pub const fn set_role(&mut self, role: AgentRole) {
        self.current_role = role;
    }

    pub const fn current_plan(&self) -> Option<&ConvoyPlan> {
        self.current_plan.as_ref()
    }

    pub fn submit_plan(&mut self, plan: ConvoyPlan) {
        self.current_plan = Some(plan);
        self.current_role = AgentRole::Planner;
    }

    pub const fn current_phase(&self) -> &'static str {
        match self.current_role {
            AgentRole::Planner => "planning",
            _ => "implementation",
        }
    }

    pub fn can_use_tool(&self, role: AgentRole, tool: &str) -> Result<(), ToolBlockedReason> {
        if !self.config.enabled {
            return Ok(());
        }

        let known_to_any_role = self
            .role_tool_matrix
            .values()
            .any(|tools| tools.contains(tool));
        if !known_to_any_role {
            return Ok(());
        }

        let allowed = self
            .role_tool_matrix
            .get(&role)
            .ok_or(ToolBlockedReason::UnknownRole(role))?;

        if !allowed.contains(tool) {
            return Err(ToolBlockedReason::NotAllowedForRole {
                tool: tool.to_string(),
                role,
            });
        }

        if self.is_sensitive_tool(tool)
            && self.config.require_approval
            && self
                .current_plan
                .as_ref()
                .is_some_and(|plan| self.assess_approval_required(plan) && !plan.approval.approved)
            && matches!(
                role,
                AgentRole::Worker | AgentRole::Builder | AgentRole::Architect
            )
        {
            return Err(ToolBlockedReason::ConvoyPlanNotApproved);
        }

        Ok(())
    }

    pub fn is_tool_allowed(&self, tool: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(self.current_role, tool)
    }

    pub fn approve(&mut self) -> Result<ApprovalToken, PlanModeError> {
        let plan = self
            .current_plan
            .as_mut()
            .ok_or(PlanModeError::NoPlanToApprove)?;

        if plan.approval.approved {
            return Err(PlanModeError::AlreadyApproved);
        }

        plan.approval.approved = true;
        plan.approval.approved_at = Some(chrono::Utc::now());
        plan.approval.approved_by = Some("User".to_string());
        self.current_role = AgentRole::Worker;

        let approved_at = plan.approval.approved_at.unwrap_or_else(chrono::Utc::now);

        Ok(ApprovalToken {
            plan_id: plan.id.clone(),
            approved_at,
        })
    }

    pub fn approve_plan(&mut self, plan_id: &str) -> Result<ApprovalToken, PlanModeError> {
        let plan = self
            .current_plan
            .as_mut()
            .ok_or(PlanModeError::NoPlanToApprove)?;

        if plan.id != plan_id {
            return Err(PlanModeError::PlanIdMismatch {
                expected: plan.id.clone(),
                actual: plan_id.to_string(),
            });
        }

        if plan.approval.approved {
            return Err(PlanModeError::AlreadyApproved);
        }

        plan.approval.approved = true;
        plan.approval.approved_at = Some(chrono::Utc::now());
        plan.approval.approved_by = Some("User".to_string());
        self.current_role = AgentRole::Worker;

        let approved_at = plan.approval.approved_at.unwrap_or_else(chrono::Utc::now);

        Ok(ApprovalToken {
            plan_id: plan_id.to_string(),
            approved_at,
        })
    }

    pub fn reset(&mut self) {
        self.current_role = AgentRole::Planner;
        self.current_plan = None;
    }

    pub fn assess_approval_required(&self, plan: &ConvoyPlan) -> bool {
        if !self.config.require_approval {
            return false;
        }

        self.approval_triggers.iter().any(|trigger| match trigger {
            ApprovalTrigger::HighRisk(level) => plan.risks.iter().any(|r| r.level >= *level),
            ApprovalTrigger::HighCost { threshold, .. } => plan.estimated_cost_usd > *threshold,
            ApprovalTrigger::ExternalChanges { .. } => false,
        })
    }

    #[allow(clippy::unused_self)]
    fn is_sensitive_tool(&self, tool: &str) -> bool {
        matches!(
            tool,
            "bash"
                | "write_file"
                | "replace_symbol_body"
                | "insert_before_symbol"
                | "insert_after_symbol"
                | "delete_file"
        )
    }
}

impl rustycode_tools::ToolGate for PlanMode {
    fn check_access(&self, role: AgentRole, tool_name: &str) -> Result<(), ToolBlockedReason> {
        self.can_use_tool(role, tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plan() -> ConvoyPlan {
        ConvoyPlan {
            id: "plan-1".into(),
            summary: "Test plan summary".into(),
            approach: "Test approach".into(),
            files_to_modify: vec![],
            commands_to_run: vec![],
            risks: vec![],
            estimated_cost_usd: 0.5,
            success_criteria: vec![],
            approval: rustycode_protocol::convoy_plan::PlanApproval {
                approved: false,
                approved_at: None,
                approved_by: None,
            },
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_plan_mode_default() {
        let pm = PlanMode::default();
        assert!(pm.is_enabled());
        assert!(!pm.requires_approval());
        assert_eq!(pm.current_role(), AgentRole::Planner);
        assert!(pm.current_plan().is_none());
    }

    #[test]
    fn test_plan_mode_config_default() {
        let config = PlanModeConfig::default();
        assert!(config.enabled);
        assert!(!config.require_approval);
        assert!((config.cost_threshold - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_submit_plan() {
        let mut pm = PlanMode::default();
        pm.submit_plan(test_plan());
        assert!(pm.current_plan().is_some());
        assert_eq!(pm.current_plan().unwrap().id, "plan-1");
    }

    #[test]
    fn test_set_role() {
        let mut pm = PlanMode::default();
        pm.set_role(AgentRole::Worker);
        assert_eq!(pm.current_role(), AgentRole::Worker);
    }

    #[test]
    fn test_current_phase() {
        let mut pm = PlanMode::default();
        assert_eq!(pm.current_phase(), "planning");
        pm.set_role(AgentRole::Worker);
        assert_eq!(pm.current_phase(), "implementation");
    }

    #[test]
    fn test_can_use_tool_planner() {
        let pm = PlanMode::default();
        assert!(pm.can_use_tool(AgentRole::Planner, "read_file").is_ok());
        assert!(pm.can_use_tool(AgentRole::Planner, "write_file").is_ok());
        assert!(pm.can_use_tool(AgentRole::Planner, "bash").is_ok());
    }

    #[test]
    fn test_can_use_tool_reviewer() {
        let pm = PlanMode::default();
        assert!(pm.can_use_tool(AgentRole::Reviewer, "read_file").is_ok());
        assert!(pm.can_use_tool(AgentRole::Reviewer, "bash").is_err());
    }

    #[test]
    fn test_can_use_tool_unknown_tool_allowed() {
        let pm = PlanMode::default();
        assert!(pm
            .can_use_tool(AgentRole::Reviewer, "custom_tool_xyz")
            .is_ok());
    }

    #[test]
    fn test_can_use_tool_disabled_plan_mode() {
        let mut pm = PlanMode::new(PlanModeConfig {
            enabled: false,
            ..Default::default()
        });
        pm.set_role(AgentRole::Reviewer);
        // Everything allowed when plan mode disabled
        assert!(pm.is_tool_allowed("bash").is_ok());
    }

    #[test]
    fn test_approve_no_plan() {
        let mut pm = PlanMode::default();
        let result = pm.approve();
        assert!(matches!(result, Err(PlanModeError::NoPlanToApprove)));
    }

    #[test]
    fn test_approve_plan_success() {
        let mut pm = PlanMode::default();
        pm.submit_plan(test_plan());
        let token = pm.approve().unwrap();
        assert_eq!(token.plan_id, "plan-1");
        assert_eq!(pm.current_role(), AgentRole::Worker);
        // Already approved
        assert!(matches!(pm.approve(), Err(PlanModeError::AlreadyApproved)));
    }

    #[test]
    fn test_approve_plan_by_id() {
        let mut pm = PlanMode::default();
        pm.submit_plan(test_plan());
        let token = pm.approve_plan("plan-1").unwrap();
        assert_eq!(token.plan_id, "plan-1");
    }

    #[test]
    fn test_approve_plan_wrong_id() {
        let mut pm = PlanMode::default();
        pm.submit_plan(test_plan());
        let result = pm.approve_plan("wrong-id");
        assert!(matches!(result, Err(PlanModeError::PlanIdMismatch { .. })));
    }

    #[test]
    fn test_reset() {
        let mut pm = PlanMode::default();
        pm.submit_plan(test_plan());
        pm.set_role(AgentRole::Worker);
        pm.reset();
        assert_eq!(pm.current_role(), AgentRole::Planner);
        assert!(pm.current_plan().is_none());
    }

    #[test]
    fn test_assess_approval_not_required() {
        let pm = PlanMode::new(PlanModeConfig {
            require_approval: false,
            ..Default::default()
        });
        assert!(!pm.assess_approval_required(&test_plan()));
    }

    #[test]
    fn test_assess_approval_cost_trigger() {
        let pm = PlanMode::new(PlanModeConfig {
            require_approval: true,
            cost_threshold: 0.1,
            ..Default::default()
        });
        let plan = test_plan(); // cost = 0.5 > 0.1
        assert!(pm.assess_approval_required(&plan));
    }

    #[test]
    fn test_is_sensitive_tool() {
        let pm = PlanMode::default();
        assert!(pm.is_sensitive_tool("bash"));
        assert!(pm.is_sensitive_tool("write_file"));
        assert!(!pm.is_sensitive_tool("read_file"));
        assert!(!pm.is_sensitive_tool("glob"));
    }

    #[test]
    fn test_tool_gate_trait() {
        let pm = PlanMode::default();
        let gate: &dyn rustycode_tools::ToolGate = &pm;
        assert!(gate.check_access(AgentRole::Planner, "bash").is_ok());
    }
}
