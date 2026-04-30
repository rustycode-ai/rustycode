//! Plan types for RustyCode
//!
//! Plans structure the implementation of tasks into ordered steps with clear outcomes
//! and rollback strategies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{PlanId, SessionId};

// Forward declaration - ToolCall is defined in tool.rs
// This is a simplified placeholder for type checking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    /// Unique identifier for this tool call
    pub call_id: String,
    /// Name of the tool being called
    pub name: String,
    /// Arguments to pass to the tool
    pub arguments: serde_json::Value,
}

/// A structured plan for implementing a task or feature.
///
/// Plans break down complex tasks into ordered, executable steps with clear outcomes
/// and rollback strategies. They are created during the planning phase and executed
/// during the execution phase.
///
/// # Plan Lifecycle
///
/// 1. **Draft** - Initial plan creation
/// 2. **Ready** - Plan ready for review
/// 3. **Approved** - User approved the plan
/// 4. **Executing** - Currently executing steps
/// 5. **Completed** - All steps completed successfully
/// 6. **Failed** - Execution encountered an error
/// 7. **Rejected** - User rejected the plan
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    /// Unique identifier for this plan
    pub id: PlanId,
    /// The session this plan belongs to
    pub session_id: SessionId,
    /// The task this plan implements
    pub task: String,
    /// When the plan was created
    pub created_at: DateTime<Utc>,
    /// Current status of the plan
    pub status: PlanStatus,
    /// One-line description of the plan
    pub summary: String,
    /// Implementation strategy (free-form prose)
    pub approach: String,
    /// Ordered steps to execute
    pub steps: Vec<PlanStep>,
    /// Relative paths of files that will be modified
    pub files_to_modify: Vec<String>,
    /// Potential issues or caveats
    pub risks: Vec<String>,
    /// Execution progress: current step index (0-based)
    #[serde(default)]
    pub current_step_index: Option<usize>,
    /// Timestamp when execution started
    pub execution_started_at: Option<DateTime<Utc>>,
    /// Timestamp when execution completed
    pub execution_completed_at: Option<DateTime<Utc>>,
    /// Execution error message if failed
    pub execution_error: Option<String>,
    /// Task profile used to generate this plan (for strategy, risk, etc.)
    #[serde(default)]
    pub task_profile: Option<crate::team::TaskProfile>,
}

/// The current status of a plan in its lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlanStatus {
    /// Plan is being drafted
    #[default]
    Draft,
    /// Plan is ready for review
    Ready,
    /// Plan has been approved by user
    Approved,
    /// Plan was rejected by user
    Rejected,
    /// Plan is currently executing
    Executing,
    /// Plan completed successfully
    Completed,
    /// Plan execution failed
    Failed,
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Draft => write!(f, "Draft"),
            Self::Ready => write!(f, "Ready"),
            Self::Approved => write!(f, "Approved"),
            Self::Rejected => write!(f, "Rejected"),
            Self::Executing => write!(f, "Executing"),
            Self::Completed => write!(f, "Completed"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// A single step in a plan.
///
/// Each step represents a discrete unit of work that can be executed independently.
/// Steps track their execution status, tool invocations, results, and any errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    /// Order in the plan sequence (0-based)
    pub order: usize,
    /// Human-readable title
    pub title: String,
    /// Detailed description of what this step does
    pub description: String,
    /// Tool names that this step will use
    pub tools: Vec<String>,
    /// Expected outcome of this step
    pub expected_outcome: String,
    /// Hint for rolling back this step if needed
    pub rollback_hint: String,
    /// Execution status of this step
    #[serde(default)]
    pub execution_status: StepStatus,
    /// Tool calls made during execution (planned calls)
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    /// Detailed execution records of tool invocations
    #[serde(default)]
    pub tool_executions: Vec<StepToolExecution>,
    /// Results from executed tools
    #[serde(default)]
    pub results: Vec<String>,
    /// Errors encountered during execution
    #[serde(default)]
    pub errors: Vec<String>,
    /// Timestamp when execution started
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when execution completed
    pub completed_at: Option<DateTime<Utc>>,
}

/// Status of a plan step execution.
///
/// Tracks the lifecycle of a plan step from creation through completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StepStatus {
    /// Step has not yet started
    #[default]
    Pending,
    /// Step is currently running
    InProgress,
    /// Step completed successfully
    Completed,
    /// Step failed during execution
    Failed,
}

/// Record of a tool invocation during step execution.
///
/// Tracks each tool call made during the execution of a plan step, including
/// arguments, output, errors, and timing information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepToolExecution {
    /// Name of the tool that was invoked
    pub tool_name: String,
    /// Arguments passed to the tool (serialized)
    pub args: String,
    /// Output produced by the tool
    pub output: String,
    /// Error if the tool failed
    pub error: Option<String>,
    /// When the tool was invoked
    pub timestamp: DateTime<Utc>,
}

impl Plan {
    /// Get the current step being executed
    pub fn current_step(&self) -> Option<&PlanStep> {
        self.current_step_index.and_then(|i| self.steps.get(i))
    }

    /// Check if the plan has completed successfully
    pub fn is_completed(&self) -> bool {
        self.status == PlanStatus::Completed
    }

    /// Check if the plan has failed
    pub fn is_failed(&self) -> bool {
        self.status == PlanStatus::Failed
    }

    /// Check if execution is in progress
    pub fn is_executing(&self) -> bool {
        self.status == PlanStatus::Executing
    }
}

impl PlanStep {
    /// Check if the step has completed successfully
    pub fn is_completed(&self) -> bool {
        self.execution_status == StepStatus::Completed
    }

    /// Check if the step has failed
    pub fn is_failed(&self) -> bool {
        self.execution_status == StepStatus::Failed
    }

    /// Check if the step is currently running
    pub fn is_in_progress(&self) -> bool {
        self.execution_status == StepStatus::InProgress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_status_display() {
        assert_eq!(format!("{}", PlanStatus::Draft), "Draft");
        assert_eq!(format!("{}", PlanStatus::Completed), "Completed");
    }

    #[test]
    fn test_step_status_default() {
        let status = StepStatus::default();
        assert_eq!(status, StepStatus::Pending);
    }

    #[test]
    fn test_plan_step_checks() {
        let mut step = PlanStep {
            order: 0,
            title: "Test".to_string(),
            description: "Test step".to_string(),
            tools: vec![],
            expected_outcome: "Success".to_string(),
            rollback_hint: "N/A".to_string(),
            execution_status: StepStatus::default(),
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };

        assert!(!step.is_completed());
        assert!(!step.is_failed());
        assert!(!step.is_in_progress());

        step.execution_status = StepStatus::Completed;
        assert!(step.is_completed());
    }

    #[test]
    fn test_plan_checks() {
        let mut plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "Test plan".to_string(),
            approach: "Test approach".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };

        assert!(!plan.is_completed());
        assert!(!plan.is_failed());
        assert!(!plan.is_executing());

        plan.status = PlanStatus::Completed;
        assert!(plan.is_completed());
    }

    #[test]
    fn test_plan_status_all_display() {
        assert_eq!(PlanStatus::Draft.to_string(), "Draft");
        assert_eq!(PlanStatus::Ready.to_string(), "Ready");
        assert_eq!(PlanStatus::Approved.to_string(), "Approved");
        assert_eq!(PlanStatus::Rejected.to_string(), "Rejected");
        assert_eq!(PlanStatus::Executing.to_string(), "Executing");
        assert_eq!(PlanStatus::Completed.to_string(), "Completed");
        assert_eq!(PlanStatus::Failed.to_string(), "Failed");
    }

    #[test]
    fn test_plan_status_default() {
        assert_eq!(PlanStatus::default(), PlanStatus::Draft);
    }

    #[test]
    fn test_plan_status_serde_roundtrip() {
        let variants = vec![
            PlanStatus::Draft,
            PlanStatus::Ready,
            PlanStatus::Approved,
            PlanStatus::Rejected,
            PlanStatus::Executing,
            PlanStatus::Completed,
            PlanStatus::Failed,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let back: PlanStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, back);
        }
    }

    #[test]
    fn test_step_status_all_variants() {
        let variants = vec![
            StepStatus::Pending,
            StepStatus::InProgress,
            StepStatus::Completed,
            StepStatus::Failed,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let back: StepStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, back);
        }
    }

    #[test]
    fn test_plan_current_step_with_index() {
        let step = PlanStep {
            order: 0,
            title: "Step 1".to_string(),
            description: "First".to_string(),
            tools: vec!["Read".to_string()],
            expected_outcome: "Files read".to_string(),
            rollback_hint: "None".to_string(),
            execution_status: StepStatus::InProgress,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec![],
            started_at: None,
            completed_at: None,
        };
        let plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Plan".to_string(),
            approach: "Approach".to_string(),
            steps: vec![step],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: Some(0),
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };

        let current = plan.current_step().unwrap();
        assert_eq!(current.title, "Step 1");
        assert!(current.is_in_progress());
    }

    #[test]
    fn test_plan_current_step_out_of_bounds() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Executing,
            summary: "Plan".to_string(),
            approach: "Approach".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: Some(5),
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };
        assert!(plan.current_step().is_none());
    }

    #[test]
    fn test_plan_is_failed() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Failed,
            summary: "Plan".to_string(),
            approach: "Approach".to_string(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: Some("Something broke".to_string()),
            task_profile: None,
        };
        assert!(plan.is_failed());
        assert!(!plan.is_completed());
        assert!(!plan.is_executing());
    }

    #[test]
    fn test_step_is_failed() {
        let step = PlanStep {
            order: 0,
            title: "Step".to_string(),
            description: "desc".to_string(),
            tools: vec![],
            expected_outcome: "ok".to_string(),
            rollback_hint: "none".to_string(),
            execution_status: StepStatus::Failed,
            tool_calls: vec![],
            tool_executions: vec![],
            results: vec![],
            errors: vec!["error".to_string()],
            started_at: None,
            completed_at: None,
        };
        assert!(step.is_failed());
        assert!(!step.is_completed());
        assert!(!step.is_in_progress());
    }

    #[test]
    fn test_tool_call_serde() {
        let tc = ToolCall {
            call_id: "call-1".to_string(),
            name: "Read".to_string(),
            arguments: serde_json::json!({"path": "/tmp/test.rs"}),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let back: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, back);
    }

    #[test]
    fn test_step_tool_execution_serde() {
        let exec = StepToolExecution {
            tool_name: "Bash".to_string(),
            args: "ls -la".to_string(),
            output: "file1.rs\nfile2.rs".to_string(),
            error: None,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&exec).unwrap();
        let back: StepToolExecution = serde_json::from_str(&json).unwrap();
        assert_eq!(exec, back);
    }

    #[test]
    fn test_full_plan_serde_roundtrip() {
        let plan = Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "Implement feature X".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Approved,
            summary: "Add feature X to module Y".to_string(),
            approach: "Step-by-step implementation".to_string(),
            steps: vec![PlanStep {
                order: 0,
                title: "Write tests".to_string(),
                description: "TDD approach".to_string(),
                tools: vec!["Write".to_string()],
                expected_outcome: "Tests pass".to_string(),
                rollback_hint: "Delete test file".to_string(),
                execution_status: StepStatus::Pending,
                tool_calls: vec![],
                tool_executions: vec![],
                results: vec![],
                errors: vec![],
                started_at: None,
                completed_at: None,
            }],
            files_to_modify: vec!["src/lib.rs".to_string()],
            risks: vec!["May break existing tests".to_string()],
            current_step_index: Some(0),
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
        };

        let json = serde_json::to_string(&plan).unwrap();
        let back: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan, back);
    }
}
