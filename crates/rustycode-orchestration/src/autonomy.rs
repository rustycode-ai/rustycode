//! Autonomy levels and permission gating for agent operations.
//!
//! Defines L0-L4 autonomy levels that control how much freedom the agent has
//! when executing operations. Each level determines whether actions can execute,
//! whether the user must be notified, and whether pre-approval is required.
//!
//! # Autonomy Levels
//!
//! | Level | Name              | Behavior                        |
//! |-------|-------------------|---------------------------------|
//! | L0    | Suggest Only      | Advisory, no execution          |
//! | L1    | Supervised        | Ask permission before each step |
//! | L2    | Guided            | Execute, notify before          |
//! | L3    | Autonomous        | Execute, notify after           |
//! | L4    | Fully Autonomous  | No notification (CI/CD)         |

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Autonomy level controlling how much freedom the agent has.
///
/// Levels escalate from suggest-only (L0) to full autonomy (L4).
/// Each level determines whether the agent can execute actions,
/// whether it must notify the user, and whether it needs pre-approval.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum AutonomyLevel {
    /// Suggest only -- no action taken, purely advisory.
    L0,
    /// Ask permission before executing any action.
    #[default]
    L1,
    /// Execute actions, notify user before proceeding.
    L2,
    /// Execute actions, notify user after completion.
    L3,
    /// Full autonomy -- no notification, for CI/CD only.
    L4,
}

impl AutonomyLevel {
    /// Whether the agent is allowed to execute actions at this level.
    #[must_use]
    pub const fn can_execute(&self) -> bool {
        matches!(self, Self::L2 | Self::L3 | Self::L4)
    }

    /// Whether the user must be notified before or during execution.
    #[must_use]
    pub const fn requires_notification(&self) -> bool {
        matches!(self, Self::L1 | Self::L2)
    }

    /// Whether the agent must get explicit approval before executing.
    #[must_use]
    pub const fn requires_pre_approval(&self) -> bool {
        matches!(self, Self::L1)
    }

    /// Maximum number of consecutive tool calls allowed at this level.
    /// L0 always returns 0. Higher levels allow more iterations.
    #[must_use]
    pub const fn max_iterations(&self) -> u32 {
        match self {
            Self::L0 => 0,
            Self::L1 => 5,
            Self::L2 => 15,
            Self::L3 => 25,
            Self::L4 => 50,
        }
    }

    /// Whether a specific tool is allowed at this autonomy level.
    #[must_use]
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        let dangerous_tools = [
            "bash",
            "subprocess",
            "write_file",
            "edit_file",
            "apply_patch",
            "multi_edit",
        ];

        match self {
            Self::L0 => false,
            Self::L1 | Self::L2 => !dangerous_tools.contains(&tool_name) || self.can_execute(),
            Self::L3 | Self::L4 => true,
        }
    }
}

impl fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::L0 => "L0 (suggest only)",
            Self::L1 => "L1 (ask permission)",
            Self::L2 => "L2 (execute, notify)",
            Self::L3 => "L3 (execute, notify after)",
            Self::L4 => "L4 (full autonomy)",
        };
        write!(f, "{label}")
    }
}

impl std::str::FromStr for AutonomyLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            "L4" => Ok(Self::L4),
            other => Err(format!("invalid autonomy level: {other}")),
        }
    }
}

/// Per-task-type autonomy configuration with overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutonomyConfig {
    /// Default autonomy level for all tasks.
    #[serde(default)]
    pub default_level: AutonomyLevel,
    /// Per-task-type autonomy overrides.
    /// Keys are task type names (e.g., `code_review`, `database_migration`).
    #[serde(default)]
    pub overrides: HashMap<String, AutonomyLevel>,
    /// Global tool allowlist override. If set, only these tools are allowed
    /// regardless of autonomy level.
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    /// Global maximum iterations override.
    #[serde(default)]
    pub max_iterations_override: Option<u32>,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            default_level: AutonomyLevel::L1,
            overrides: HashMap::new(),
            allowed_tools: None,
            max_iterations_override: None,
        }
    }
}

impl AutonomyConfig {
    /// Create a new autonomy config with the specified default level.
    #[must_use]
    pub fn new(level: AutonomyLevel) -> Self {
        Self {
            default_level: level,
            ..Default::default()
        }
    }

    /// Resolve the effective autonomy level for a given task type.
    ///
    /// Checks per-task overrides first, then falls back to the default.
    #[must_use]
    pub fn resolve_level(&self, task_type: &str) -> AutonomyLevel {
        self.overrides
            .get(task_type)
            .copied()
            .unwrap_or(self.default_level)
    }

    /// Whether a tool is allowed for a given task type.
    #[must_use]
    pub fn can_use_tool(&self, tool_name: &str, task_type: &str) -> bool {
        // Check global allowlist first
        if let Some(ref allowed) = self.allowed_tools {
            if !allowed.iter().any(|t| t == tool_name) {
                return false;
            }
        }
        let level = self.resolve_level(task_type);
        level.is_tool_allowed(tool_name)
    }

    /// Effective maximum iterations, considering override.
    #[must_use]
    pub fn effective_max_iterations(&self, task_type: &str) -> u32 {
        if let Some(max) = self.max_iterations_override {
            return max;
        }
        self.resolve_level(task_type).max_iterations()
    }
}

/// Type of operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    /// Read-only operation (no side effects).
    Read,
    /// Write operation (modifies files).
    Write,
    /// Execute operation (runs commands).
    Execute,
    /// Unknown operation type.
    Unknown,
}

impl OperationType {
    /// Classify an operation by tool name.
    #[must_use]
    pub fn from_tool(tool_name: &str) -> Self {
        match tool_name {
            "read_file"
            | "list_dir"
            | "grep"
            | "glob"
            | "find"
            | "web_fetch"
            | "web_search"
            | "lsp_diagnostics"
            | "lsp_hover"
            | "lsp_definition"
            | "lsp_references"
            | "lsp_document_symbols"
            | "todo_read" => Self::Read,
            "write_file" | "edit_file" | "text_editor" | "search_replace" | "apply_patch"
            | "multi_edit" | "todo_write" | "notebook_edit" => Self::Write,
            "bash" | "subprocess" => Self::Execute,
            _ => Self::Unknown,
        }
    }
}

/// The autonomy decision for a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomyDecision {
    /// Operation is allowed without notification.
    Allow { reason: String },
    /// Operation is allowed but user must be notified.
    AllowWithNotification { reason: String, message: String },
    /// Operation requires explicit user approval before proceeding.
    RequireApproval { reason: String },
    /// Operation is blocked at this autonomy level.
    Blocked { reason: String },
}

impl AutonomyDecision {
    /// Whether the decision allows the operation (with or without notification).
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(
            self,
            Self::Allow { .. } | Self::AllowWithNotification { .. }
        )
    }

    /// Whether the decision blocks the operation entirely.
    #[must_use]
    pub const fn is_blocked(&self) -> bool {
        matches!(self, Self::Blocked { .. })
    }
}

/// Task category for control tuning calibration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCategory {
    CodeReview,
    DatabaseMigration,
    Refactoring,
    BugFix,
    FeatureImplementation,
    Deployment,
    Documentation,
    General,
}

impl fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::CodeReview => "code_review",
            Self::DatabaseMigration => "database_migration",
            Self::Refactoring => "refactoring",
            Self::BugFix => "bug_fix",
            Self::FeatureImplementation => "feature",
            Self::Deployment => "deployment",
            Self::Documentation => "documentation",
            Self::General => "general",
        };
        write!(f, "{s}")
    }
}

/// Classifies task type strings into task categories.
pub struct TaskTypeClassifier;

impl TaskTypeClassifier {
    /// Map a task type string to a category.
    #[must_use]
    pub fn classify(task_type: &str) -> TaskCategory {
        match task_type {
            "code_review" | "review" | "code-review" => TaskCategory::CodeReview,
            "database_migration" | "db-migration" | "migration" => TaskCategory::DatabaseMigration,
            "refactoring" | "refactor" => TaskCategory::Refactoring,
            "bug_fix" | "bugfix" | "fix" => TaskCategory::BugFix,
            "feature" | "implementation" | "new_feature" => TaskCategory::FeatureImplementation,
            "deployment" | "deploy" | "release" => TaskCategory::Deployment,
            "documentation" | "docs" | "readme" => TaskCategory::Documentation,
            _ => TaskCategory::General,
        }
    }
}

/// Per-task-category freedom calibration.
///
/// Controls which operation types can be auto-approved
/// without user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlTuning {
    /// Whether read operations can be auto-approved.
    pub can_auto_approve_read: bool,
    /// Whether write operations can be auto-approved.
    pub can_auto_approve_write: bool,
    /// Whether exec operations can be auto-approved.
    pub can_auto_approve_exec: bool,
}

impl Default for ControlTuning {
    fn default() -> Self {
        Self::moderate_freedom()
    }
}

impl ControlTuning {
    /// High freedom: auto-approve everything including exec.
    #[must_use]
    pub const fn high_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: true,
            can_auto_approve_exec: true,
        }
    }

    /// Moderate freedom: auto-approve read and write, ask for exec.
    #[must_use]
    pub const fn moderate_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: true,
            can_auto_approve_exec: false,
        }
    }

    /// Low freedom: auto-approve read only.
    #[must_use]
    pub const fn low_freedom() -> Self {
        Self {
            can_auto_approve_read: true,
            can_auto_approve_write: false,
            can_auto_approve_exec: false,
        }
    }
}

impl TaskCategory {
    /// Get the default control tuning for this task category.
    #[must_use]
    pub const fn control_tuning(&self) -> ControlTuning {
        match self {
            // High-freedom tasks: the agent can be trusted to act autonomously
            Self::CodeReview | Self::Documentation | Self::Refactoring => {
                ControlTuning::high_freedom()
            }
            // Moderate-freedom tasks: standard implementation work
            Self::BugFix | Self::FeatureImplementation | Self::General => {
                ControlTuning::moderate_freedom()
            }
            // Low-freedom tasks: destructive or irreversible operations
            Self::DatabaseMigration | Self::Deployment => ControlTuning::low_freedom(),
        }
    }
}

/// Bridges autonomy configuration with tool permission decisions.
pub struct AutonomyDecider<'a> {
    config: &'a AutonomyConfig,
}

impl<'a> AutonomyDecider<'a> {
    /// Create a new decider for the given autonomy config.
    pub const fn new(config: &'a AutonomyConfig) -> Self {
        Self { config }
    }

    /// Decide whether a tool invocation is allowed for the given task category.
    pub fn decide(&self, tool_name: &str, task_category: TaskCategory) -> AutonomyDecision {
        let op_type = OperationType::from_tool(tool_name);

        // Read operations are always allowed regardless of autonomy level
        if op_type == OperationType::Read {
            return AutonomyDecision::Allow {
                reason: "read operation".to_string(),
            };
        }

        let task_type_str = task_category.to_string();
        let level = self.config.resolve_level(&task_type_str);
        let tuning = task_category.control_tuning();

        match level {
            AutonomyLevel::L0 => AutonomyDecision::Blocked {
                reason: format!("blocked at {level}: suggest-only mode"),
            },
            AutonomyLevel::L1 => {
                if op_type == OperationType::Write && !tuning.can_auto_approve_write {
                    return AutonomyDecision::RequireApproval {
                        reason: format!("write requires approval at {level}"),
                    };
                }
                if op_type == OperationType::Execute && !tuning.can_auto_approve_exec {
                    return AutonomyDecision::RequireApproval {
                        reason: format!("exec requires approval at {level}"),
                    };
                }
                AutonomyDecision::RequireApproval {
                    reason: format!("requires approval at {level}"),
                }
            }
            AutonomyLevel::L2 => {
                if op_type == OperationType::Write && tuning.can_auto_approve_write {
                    AutonomyDecision::AllowWithNotification {
                        reason: "write allowed with notification".to_string(),
                        message: format!("Executing {tool_name} for {task_category}"),
                    }
                } else if op_type == OperationType::Execute && tuning.can_auto_approve_exec {
                    AutonomyDecision::AllowWithNotification {
                        reason: "exec allowed with notification".to_string(),
                        message: format!("Executing {tool_name} for {task_category}"),
                    }
                } else {
                    AutonomyDecision::RequireApproval {
                        reason: format!(
                            "{op_type:?} requires approval at {level} for {task_category}"
                        ),
                    }
                }
            }
            AutonomyLevel::L3 => {
                if (op_type == OperationType::Write && tuning.can_auto_approve_write)
                    || (op_type == OperationType::Execute && tuning.can_auto_approve_exec)
                {
                    AutonomyDecision::Allow {
                        reason: format!("auto-approved at {level} for {task_category}"),
                    }
                } else {
                    AutonomyDecision::RequireApproval {
                        reason: format!(
                            "{op_type:?} not auto-approved for {task_category} at {level}"
                        ),
                    }
                }
            }
            AutonomyLevel::L4 => AutonomyDecision::Allow {
                reason: format!("full autonomy ({level})"),
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- AutonomyLevel tests ---

    #[test]
    fn autonomy_level_from_str() {
        assert_eq!("L0".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L0));
        assert_eq!("L1".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L1));
        assert_eq!("L2".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L2));
        assert_eq!("L3".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L3));
        assert_eq!("L4".parse::<AutonomyLevel>(), Ok(AutonomyLevel::L4));
        assert!("L5".parse::<AutonomyLevel>().is_err());
        assert!("invalid".parse::<AutonomyLevel>().is_err());
    }

    #[test]
    fn autonomy_level_display() {
        assert_eq!(format!("{}", AutonomyLevel::L0), "L0 (suggest only)");
        assert_eq!(format!("{}", AutonomyLevel::L1), "L1 (ask permission)");
        assert_eq!(format!("{}", AutonomyLevel::L2), "L2 (execute, notify)");
        assert_eq!(
            format!("{}", AutonomyLevel::L3),
            "L3 (execute, notify after)"
        );
        assert_eq!(format!("{}", AutonomyLevel::L4), "L4 (full autonomy)");
    }

    #[test]
    fn autonomy_level_ordering() {
        assert!(AutonomyLevel::L0 < AutonomyLevel::L1);
        assert!(AutonomyLevel::L1 < AutonomyLevel::L2);
        assert!(AutonomyLevel::L2 < AutonomyLevel::L3);
        assert!(AutonomyLevel::L3 < AutonomyLevel::L4);
    }

    #[test]
    fn autonomy_level_serde_roundtrip() {
        for level in [
            AutonomyLevel::L0,
            AutonomyLevel::L1,
            AutonomyLevel::L2,
            AutonomyLevel::L3,
            AutonomyLevel::L4,
        ] {
            let yaml = serde_yaml::to_string(&level).unwrap();
            let decoded: AutonomyLevel = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(decoded, level);
        }
    }

    #[test]
    fn autonomy_can_execute_at_each_level() {
        assert!(!AutonomyLevel::L0.can_execute()); // suggest only
        assert!(!AutonomyLevel::L1.can_execute()); // ask first
        assert!(AutonomyLevel::L2.can_execute()); // execute, notify
        assert!(AutonomyLevel::L3.can_execute()); // execute, notify after
        assert!(AutonomyLevel::L4.can_execute()); // full autonomy
    }

    #[test]
    fn autonomy_requires_notification_at_each_level() {
        assert!(!AutonomyLevel::L0.requires_notification());
        assert!(AutonomyLevel::L1.requires_notification()); // ask = notify
        assert!(AutonomyLevel::L2.requires_notification()); // notify before
        assert!(!AutonomyLevel::L3.requires_notification()); // deferred
        assert!(!AutonomyLevel::L4.requires_notification()); // silent
    }

    #[test]
    fn autonomy_requires_pre_approval() {
        assert!(!AutonomyLevel::L0.requires_pre_approval()); // never executes
        assert!(AutonomyLevel::L1.requires_pre_approval()); // ask first
        assert!(!AutonomyLevel::L2.requires_pre_approval()); // just notify
        assert!(!AutonomyLevel::L3.requires_pre_approval());
        assert!(!AutonomyLevel::L4.requires_pre_approval());
    }

    #[test]
    fn autonomy_max_iterations() {
        assert_eq!(AutonomyLevel::L0.max_iterations(), 0);
        assert_eq!(AutonomyLevel::L1.max_iterations(), 5);
        assert_eq!(AutonomyLevel::L2.max_iterations(), 15);
        assert_eq!(AutonomyLevel::L3.max_iterations(), 25);
        assert_eq!(AutonomyLevel::L4.max_iterations(), 50);
    }

    #[test]
    fn autonomy_level_default_is_l1() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::L1);
    }

    // --- AutonomyConfig tests ---

    #[test]
    fn autonomy_config_default_is_l1() {
        let config = AutonomyConfig::default();
        assert_eq!(config.default_level, AutonomyLevel::L1);
        assert!(config.overrides.is_empty());
        assert!(config.allowed_tools.is_none());
    }

    #[test]
    fn autonomy_config_resolve_level_with_overrides() {
        let config = AutonomyConfig {
            default_level: AutonomyLevel::L2,
            overrides: {
                let mut map = HashMap::new();
                map.insert("code_review".to_string(), AutonomyLevel::L3);
                map.insert("database_migration".to_string(), AutonomyLevel::L0);
                map
            },
            ..Default::default()
        };

        assert_eq!(config.resolve_level("code_review"), AutonomyLevel::L3);
        assert_eq!(
            config.resolve_level("database_migration"),
            AutonomyLevel::L0
        );
        assert_eq!(config.resolve_level("unknown"), AutonomyLevel::L2);
    }

    #[test]
    fn autonomy_config_effective_max_iterations() {
        let config = AutonomyConfig {
            max_iterations_override: Some(100),
            ..Default::default()
        };
        assert_eq!(config.effective_max_iterations("any_task"), 100);
    }

    #[test]
    fn autonomy_config_effective_max_iterations_no_override() {
        let config = AutonomyConfig {
            default_level: AutonomyLevel::L3,
            ..Default::default()
        };
        assert_eq!(config.effective_max_iterations("any_task"), 25);
    }

    // --- OperationType tests ---

    #[test]
    fn operation_classification() {
        assert_eq!(OperationType::from_tool("read_file"), OperationType::Read);
        assert_eq!(OperationType::from_tool("write_file"), OperationType::Write);
        assert_eq!(OperationType::from_tool("edit_file"), OperationType::Write);
        assert_eq!(OperationType::from_tool("bash"), OperationType::Execute);
        assert_eq!(OperationType::from_tool("grep"), OperationType::Read);
        assert_eq!(OperationType::from_tool("unknown"), OperationType::Unknown);
    }

    // --- TaskCategory / TaskTypeClassifier tests ---

    #[test]
    fn resolve_tuning_for_known_task_types() {
        assert_eq!(
            TaskTypeClassifier::classify("code_review"),
            TaskCategory::CodeReview
        );
        assert_eq!(
            TaskTypeClassifier::classify("database_migration"),
            TaskCategory::DatabaseMigration
        );
        assert_eq!(
            TaskTypeClassifier::classify("refactoring"),
            TaskCategory::Refactoring
        );
        assert_eq!(
            TaskTypeClassifier::classify("bug_fix"),
            TaskCategory::BugFix
        );
        assert_eq!(
            TaskTypeClassifier::classify("feature"),
            TaskCategory::FeatureImplementation
        );
        assert_eq!(
            TaskTypeClassifier::classify("deployment"),
            TaskCategory::Deployment
        );
        assert_eq!(
            TaskTypeClassifier::classify("documentation"),
            TaskCategory::Documentation
        );
    }

    #[test]
    fn unknown_task_classifies_as_general() {
        assert_eq!(
            TaskTypeClassifier::classify("unknown_task"),
            TaskCategory::General
        );
    }

    #[test]
    fn task_category_tuning_calibration() {
        // Code review = high freedom
        let tuning = TaskCategory::CodeReview.control_tuning();
        assert!(tuning.can_auto_approve_write);

        // Database migration = low freedom
        let tuning = TaskCategory::DatabaseMigration.control_tuning();
        assert!(!tuning.can_auto_approve_write);

        // Deployment = low freedom
        let tuning = TaskCategory::Deployment.control_tuning();
        assert!(!tuning.can_auto_approve_exec);
    }

    #[test]
    fn all_task_categories_have_tuning() {
        for category in [
            TaskCategory::CodeReview,
            TaskCategory::DatabaseMigration,
            TaskCategory::Refactoring,
            TaskCategory::BugFix,
            TaskCategory::FeatureImplementation,
            TaskCategory::Deployment,
            TaskCategory::Documentation,
            TaskCategory::General,
        ] {
            let tuning = category.control_tuning();
            assert!(
                tuning.can_auto_approve_read,
                "{category:?} should allow read"
            );
        }
    }

    // --- AutonomyDecider tests ---

    fn test_config(autonomy: AutonomyLevel) -> AutonomyConfig {
        AutonomyConfig::new(autonomy)
    }

    #[test]
    fn l0_always_denies_execution() {
        let config = test_config(AutonomyLevel::L0);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(decision, AutonomyDecision::Blocked { .. }));
    }

    #[test]
    fn l1_requires_approval_for_writes() {
        let config = test_config(AutonomyLevel::L1);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(decision, AutonomyDecision::RequireApproval { .. }));
    }

    #[test]
    fn l2_executes_with_notification() {
        let config = test_config(AutonomyLevel::L2);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("write_file", TaskCategory::FeatureImplementation);
        assert!(matches!(
            decision,
            AutonomyDecision::AllowWithNotification { .. }
        ));
    }

    #[test]
    fn l3_allows_for_high_freedom_tasks() {
        let config = test_config(AutonomyLevel::L3);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("write_file", TaskCategory::CodeReview);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    #[test]
    fn l4_allows_everything() {
        let config = test_config(AutonomyLevel::L4);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("bash", TaskCategory::Deployment);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    #[test]
    fn read_operations_always_allowed() {
        let config = test_config(AutonomyLevel::L0);
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("read_file", TaskCategory::General);
        assert!(matches!(decision, AutonomyDecision::Allow { .. }));
    }

    #[test]
    fn autonomy_override_applied() {
        let config = AutonomyConfig {
            default_level: AutonomyLevel::L3,
            overrides: {
                let mut map = HashMap::new();
                map.insert("database_migration".to_string(), AutonomyLevel::L0);
                map
            },
            ..Default::default()
        };
        let decider = AutonomyDecider::new(&config);
        let decision = decider.decide("write_file", TaskCategory::DatabaseMigration);
        assert!(matches!(decision, AutonomyDecision::Blocked { .. }));
    }

    // --- AutonomyDecision tests ---

    #[test]
    fn autonomy_decision_is_allowed() {
        assert!(AutonomyDecision::Allow {
            reason: "test".into()
        }
        .is_allowed());
        assert!(!AutonomyDecision::Blocked {
            reason: "test".into()
        }
        .is_allowed());
        assert!(!AutonomyDecision::RequireApproval {
            reason: "test".into()
        }
        .is_allowed());
        assert!(AutonomyDecision::AllowWithNotification {
            reason: "test".into(),
            message: "msg".into(),
        }
        .is_allowed());
    }

    #[test]
    fn autonomy_decision_is_blocked() {
        assert!(AutonomyDecision::Blocked {
            reason: "test".into()
        }
        .is_blocked());
        assert!(!AutonomyDecision::Allow {
            reason: "test".into()
        }
        .is_blocked());
    }

    // --- ControlTuning tests ---

    #[test]
    fn high_freedom_allows_more_operations() {
        let tuning = ControlTuning::high_freedom();
        assert!(tuning.can_auto_approve_read);
        assert!(tuning.can_auto_approve_write);
        assert!(tuning.can_auto_approve_exec);
    }

    #[test]
    fn low_freedom_restricts_operations() {
        let tuning = ControlTuning::low_freedom();
        assert!(tuning.can_auto_approve_read);
        assert!(!tuning.can_auto_approve_write);
        assert!(!tuning.can_auto_approve_exec);
    }

    #[test]
    fn default_control_tuning_is_moderate() {
        let tuning = ControlTuning::default();
        assert!(tuning.can_auto_approve_read);
        assert!(tuning.can_auto_approve_write);
        assert!(!tuning.can_auto_approve_exec);
    }

    #[test]
    fn control_tuning_serde_roundtrip() {
        let tuning = ControlTuning::high_freedom();
        let json = serde_json::to_string(&tuning).unwrap();
        let decoded: ControlTuning = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, tuning);
    }

    // --- TaskCategory display ---

    #[test]
    fn task_category_display() {
        assert_eq!(TaskCategory::CodeReview.to_string(), "code_review");
        assert_eq!(TaskCategory::Deployment.to_string(), "deployment");
        assert_eq!(TaskCategory::General.to_string(), "general");
    }
}
