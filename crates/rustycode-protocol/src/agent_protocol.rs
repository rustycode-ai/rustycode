//! Agent Communication Protocol
//!
//! This module defines the structured protocol for inter-agent communication
//! in the team-based orchestration system. Each agent produces typed messages
//! that other agents can consume, validate, and respond to.
//!
//! # Design Principles
//!
//! 1. **Structured Output**: All agent responses are JSON-serializable structs
//! 2. **Explicit Signals**: Phase transitions are triggered by explicit fields
//! 3. **Validation**: Each message type has validation rules
//! 4. **Progressive Disclosure**: Agents see only the context they need
//!
//! # Message Flow
//!
//! ```text
//! Task → [Architect] → StructuralDeclaration
//!                    ↓
//!        [Builder] → ChangeProposal → [Skeptic] → ReviewVerdict
//!                                              ↓
//!        [Scalpel] → SurgicalFix → [Judge] → VerificationResult
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Formal language for agent actions
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", content = "args")]
pub enum AgentAction {
    /// Edit a file at the given path
    EditFile { path: String, content: String },
    /// Run a bash command, optionally in a specific directory
    Bash {
        command: String,
        cwd: Option<String>,
    },
    /// List files in a directory
    ListFiles { path: String },
    /// Signal completion with a final message
    Complete { message: String },
}

/// Generate the JSON schema for AgentAction
pub fn get_agent_action_schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(AgentAction))
        .unwrap_or_else(|_| serde_json::json!({}))
}

// ============================================================================
// Core Protocol Message Types
// ============================================================================

/// The envelope for any agent message.
///
/// Contains metadata for routing, tracing, and validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage<T> {
    /// Unique message ID for tracking.
    pub id: String,
    /// Which agent sent this message.
    pub from: AgentRole,
    /// Intended recipient (None = broadcast).
    pub to: Option<AgentRole>,
    /// The message payload.
    pub payload: T,
    /// Optional reference to a previous message this responds to.
    pub in_reply_to: Option<String>,
}

impl<T> AgentMessage<T> {
    pub fn new(from: AgentRole, payload: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            from,
            to: None,
            payload,
            in_reply_to: None,
        }
    }

    pub fn with_reply(mut self, reply_to: String) -> Self {
        self.in_reply_to = Some(reply_to);
        self
    }

    pub fn directed(mut self, to: AgentRole) -> Self {
        self.to = Some(to);
        self
    }
}

// ============================================================================
// Agent Roles
// ============================================================================

/// The specialized roles in the team system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentRole {
    /// Produces structural declarations before implementation.
    Architect,
    /// Implements changes within declared structure.
    Builder,
    /// Reviews changes for correctness and compliance.
    Skeptic,
    /// Verifies changes via compilation and tests.
    Judge,
    /// Makes targeted surgical fixes for compile errors.
    Scalpel,
    /// Orchestrates the overall flow (coordinator role).
    Coordinator,
    /// Analyzes code, writes plans, researches solutions.
    Planner,
    /// Executes approved plans, makes code changes.
    Worker,
    /// Reviews changes, tests, verifies correctness.
    Reviewer,
    /// Explores codebase, gathers context.
    Researcher,
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Architect => write!(f, "Architect"),
            Self::Builder => write!(f, "Builder"),
            Self::Skeptic => write!(f, "Skeptic"),
            Self::Judge => write!(f, "Judge"),
            Self::Scalpel => write!(f, "Scalpel"),
            Self::Coordinator => write!(f, "Coordinator"),
            Self::Planner => write!(f, "Planner"),
            Self::Worker => write!(f, "Worker"),
            Self::Reviewer => write!(f, "Reviewer"),
            Self::Researcher => write!(f, "Researcher"),
        }
    }
}

// ============================================================================
// Architect Protocol
// ============================================================================

/// The Architect's output — a binding structural contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectMessage {
    /// The structural declaration.
    pub declaration: StructuralDeclaration,
    /// Why this structure was chosen.
    pub rationale: String,
    /// Confidence 0.0–1.0.
    pub confidence: f64,
    /// Optional: signals that Architect should be invoked again.
    pub rearchitecture_triggers: Vec<String>,
}

impl ArchitectMessage {
    /// Validate the declaration for internal consistency.
    pub fn validate(&self) -> ValidationResult {
        let mut errors = Vec::new();

        // Check: no dependency in both add and remove
        for dep in &self.declaration.dependencies.add {
            if self.declaration.dependencies.remove.contains(&dep.name) {
                errors.push(format!("Dependency '{}' in both add and remove", dep.name));
            }
        }

        // Check: all interface implementors reference declared modules
        let declared_paths: std::collections::HashSet<&str> = self
            .declaration
            .modules
            .iter()
            .map(|m| m.path.as_str())
            .collect();

        for iface in &self.declaration.interfaces {
            for implementor in &iface.implementors {
                if !declared_paths.contains(implementor.as_str()) {
                    errors.push(format!(
                        "Interface '{}' implementor '{}' not a declared module",
                        iface.name, implementor
                    ));
                }
            }
        }

        // Check: confidence in valid range
        if self.confidence < 0.0 || self.confidence > 1.0 {
            errors.push(format!(
                "Confidence {} out of range [0.0, 1.0]",
                self.confidence
            ));
        }

        ValidationResult { errors }
    }
}

/// A structural declaration — the Architect's binding contract.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructuralDeclaration {
    /// Modules to create or modify.
    #[serde(default)]
    pub modules: Vec<ModuleDeclaration>,
    /// Interfaces/traits shared across modules.
    #[serde(default)]
    pub interfaces: Vec<InterfaceDeclaration>,
    /// Dependency changes.
    #[serde(default)]
    pub dependencies: DependencyChanges,
}

/// A module the Builder will create or modify.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDeclaration {
    /// Relative path from crate root.
    pub path: String,
    /// Create or modify.
    pub action: ModuleAction,
    /// Public symbols exported.
    #[serde(default)]
    pub exports: Vec<String>,
    /// Import paths this module uses.
    #[serde(default)]
    pub imports: Vec<String>,
    /// One-line purpose statement.
    pub purpose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ModuleAction {
    Create,
    Modify,
}

/// A trait/interface shared across modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterfaceDeclaration {
    /// Trait name.
    pub name: String,
    /// Which module defines this trait.
    pub defined_in: String,
    /// Method signatures.
    #[serde(default)]
    pub methods: Vec<String>,
    /// Modules that implement this trait.
    #[serde(default)]
    pub implementors: Vec<String>,
}

/// Cargo dependency changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyChanges {
    /// New crates to add.
    #[serde(default)]
    pub add: Vec<DependencySpec>,
    /// Existing crates to remove.
    #[serde(default)]
    pub remove: Vec<String>,
    /// Explicitly acknowledged retained deps.
    #[serde(default)]
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySpec {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub features: Vec<String>,
    pub reason: String,
}

// ============================================================================
// Builder Protocol
// ============================================================================

/// The Builder's output — proposed changes with escalation signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderMessage {
    /// What approach was taken.
    pub approach: String,
    /// Files changed.
    pub changes: Vec<FileChange>,
    /// What was accomplished.
    pub claims: Vec<String>,
    /// Confidence 0.0–1.0.
    pub confidence: f64,
    /// Whether the task is complete.
    pub done: bool,
    /// Optional: request escalation to another phase.
    #[serde(default)]
    pub escalation: Option<EscalationRequest>,
    /// Optional: signals for other agents.
    #[serde(default)]
    pub signals: AgentSignals,
}

impl BuilderMessage {
    /// Check if this Builder message requests escalation.
    pub fn needs_escalation(&self) -> bool {
        self.escalation.is_some()
    }

    /// Get the escalation request if present.
    pub fn escalation_request(&self) -> Option<&EscalationRequest> {
        self.escalation.as_ref()
    }

    /// Validate the message structure.
    pub fn validate(&self) -> ValidationResult {
        let mut errors = Vec::new();

        if self.changes.is_empty() && !self.done {
            errors.push("Builder produced no changes and task is not done".to_string());
        }

        if self.confidence < 0.0 || self.confidence > 1.0 {
            errors.push(format!("Confidence {} out of range", self.confidence));
        }

        ValidationResult { errors }
    }
}

/// A request for escalation to another agent/phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRequest {
    /// Which agent/phase to escalate to.
    pub target: EscalationTarget,
    /// Why escalation is needed.
    pub reason: String,
    /// Optional: specific question or concern.
    #[serde(default)]
    pub question: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EscalationTarget {
    /// Need architectural guidance.
    Architect,
    /// Security review needed.
    SecurityReview,
    /// Performance review needed.
    PerformanceReview,
    /// Human escalation.
    Human,
}

/// Signals from one agent to others.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSignals {
    /// Suggests Scalpel should review after this turn.
    #[serde(default)]
    pub suggest_scalpel: bool,
    /// Suggests Skeptic should do extra-deep review.
    #[serde(default)]
    pub suggest_deep_review: bool,
    /// Notes discovered during implementation.
    #[serde(default)]
    pub insights: Vec<String>,
}

// ============================================================================
// Skeptic Protocol
// ============================================================================

/// The Skeptic's output — review verdict with evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkepticMessage {
    /// The verdict.
    pub verdict: SkepticVerdict,
    /// Claims that were verified.
    #[serde(default)]
    pub verified: Vec<VerifiedClaim>,
    /// Claims that were refuted.
    #[serde(default)]
    pub refuted: Vec<RefutedClaim>,
    /// Insights discovered during review.
    #[serde(default)]
    pub insights: Vec<String>,
    /// Whether structural compliance was verified.
    #[serde(default)]
    pub structural_compliance: StructuralCompliance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SkepticVerdict {
    /// All claims verified, code is correct.
    Approve,
    /// Some claims wrong or code has issues.
    NeedsWork,
    /// Hallucination or critical security bug.
    Veto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiedClaim {
    /// The original claim.
    pub claim: String,
    /// Evidence that verifies it.
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefutedClaim {
    /// The original claim.
    pub claim: String,
    /// What's actually on disk.
    pub evidence: String,
    /// Severity of the issue.
    pub severity: IssueSeverity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum IssueSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Structural compliance check result.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructuralCompliance {
    /// Whether all modified files are in declared modules.
    pub files_compliant: bool,
    /// Whether all dependencies are declared.
    pub deps_compliant: bool,
    /// List of violations if any.
    #[serde(default)]
    pub violations: Vec<String>,
}

// ============================================================================
// Judge Protocol
// ============================================================================

/// The Judge's output — verification results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeMessage {
    /// Whether compilation succeeded.
    pub compiles: bool,
    /// Compilation errors if any.
    #[serde(default)]
    pub compile_errors: Vec<CompileError>,
    /// Test results.
    pub tests: TestSummary,
    /// Files that were modified.
    #[serde(default)]
    pub dirty_files: Vec<String>,
    /// Whether the Judge recommends Scalpel.
    #[serde(default)]
    pub recommend_scalpel: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileError {
    /// Error code (e.g., "E0308").
    #[serde(default)]
    pub code: Option<String>,
    /// The error message.
    pub message: String,
    /// File path.
    pub file: String,
    /// Line number.
    #[serde(default)]
    pub line: Option<u32>,
    /// Whether this is scalpel-appropriate.
    #[serde(default)]
    pub is_scalpel_appropriate: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestSummary {
    /// Total tests run.
    #[serde(default)]
    pub total: usize,
    /// Tests passed.
    #[serde(default)]
    pub passed: usize,
    /// Tests failed.
    #[serde(default)]
    pub failed: usize,
    /// Names of failing tests.
    #[serde(default)]
    pub failed_names: Vec<String>,
}

// ============================================================================
// Scalpel Protocol
// ============================================================================

/// The Scalpel's output — targeted surgical fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalpelMessage {
    /// The specific fixes applied.
    pub fixes: Vec<SurgicalFix>,
    /// Whether all targeted failures are resolved.
    pub done: bool,
    /// If not done, what remains.
    #[serde(default)]
    pub remaining_issues: Vec<String>,
    /// If the fix required exceeding scope.
    #[serde(default)]
    pub exceeded_scope: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurgicalFix {
    /// File that was fixed.
    pub file: String,
    /// The specific issue.
    pub issue: String,
    /// What was done.
    pub action: String,
    /// Lines changed.
    #[serde(default)]
    pub lines_changed: LinesChanged,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinesChanged {
    #[serde(default)]
    pub added: usize,
    #[serde(default)]
    pub removed: usize,
}

// ============================================================================
// File Changes
// ============================================================================

/// A file change with context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// File path.
    pub path: String,
    /// Summary of changes.
    pub summary: String,
    /// The diff hunk.
    #[serde(default)]
    pub diff_hunk: String,
    /// Lines added.
    #[serde(default)]
    pub lines_added: usize,
    /// Lines removed.
    #[serde(default)]
    pub lines_removed: usize,
}

// ============================================================================
// Validation
// ============================================================================

/// Result of validating a protocol message.
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// List of validation errors.
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn is_err(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn combine(mut self, other: ValidationResult) -> Self {
        self.errors.extend(other.errors);
        self
    }
}

impl fmt::Display for ValidationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.is_empty() {
            write!(f, "Validation OK")
        } else {
            write!(f, "Validation failed: {}", self.errors.join("; "))
        }
    }
}

// ============================================================================
// Protocol Constants
// ============================================================================

/// Maximum lines a Scalpel should change per file.
pub const SCALPEL_MAX_LINES_PER_FILE: usize = 10;

/// Minimum confidence threshold for Architect declarations.
pub const ARCHITECT_MIN_CONFIDENCE: f64 = 0.7;

/// Minimum confidence threshold for Builder claims.
pub const BUILDER_MIN_CONFIDENCE: f64 = 0.6;

/// Maximum files a Scalpel should touch in one turn.
pub const SCALPEL_MAX_FILES: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn architect_message_validates_correctly() {
        let msg = ArchitectMessage {
            declaration: StructuralDeclaration {
                modules: vec![ModuleDeclaration {
                    path: "src/auth.rs".to_string(),
                    action: ModuleAction::Create,
                    exports: vec!["AuthManager".to_string()],
                    imports: vec!["anyhow::Result".to_string()],
                    purpose: "Authentication management".to_string(),
                }],
                interfaces: vec![],
                dependencies: DependencyChanges {
                    add: vec![],
                    remove: vec![],
                    keep: vec!["anyhow".to_string()],
                },
            },
            rationale: "Simple auth module".to_string(),
            confidence: 0.9,
            rearchitecture_triggers: vec![],
        };

        let result = msg.validate();
        assert!(result.is_ok(), "Expected valid: {:?}", result.errors);
    }

    #[test]
    fn architect_message_rejects_dep_conflict() {
        let msg = ArchitectMessage {
            declaration: StructuralDeclaration {
                modules: vec![],
                interfaces: vec![],
                dependencies: DependencyChanges {
                    add: vec![DependencySpec {
                        name: "serde".to_string(),
                        version: "1".to_string(),
                        features: vec![],
                        reason: "test".to_string(),
                    }],
                    remove: vec!["serde".to_string()],
                    keep: vec![],
                },
            },
            rationale: "test".to_string(),
            confidence: 0.9,
            rearchitecture_triggers: vec![],
        };

        let result = msg.validate();
        assert!(result.is_err());
        assert!(result.errors.iter().any(|e| e.contains("serde")));
    }

    #[test]
    fn builder_message_detects_escalation() {
        let msg = BuilderMessage {
            approach: "test".to_string(),
            changes: vec![],
            claims: vec![],
            confidence: 0.8,
            done: false,
            escalation: Some(EscalationRequest {
                target: EscalationTarget::Architect,
                reason: "Unclear module boundaries".to_string(),
                question: Some("Should this be in auth or core?".to_string()),
            }),
            signals: AgentSignals::default(),
        };

        assert!(msg.needs_escalation());
        assert_eq!(
            msg.escalation_request().unwrap().target,
            EscalationTarget::Architect
        );
    }

    #[test]
    fn agent_message_envelope_serializes() {
        let msg: AgentMessage<BuilderMessage> = AgentMessage::new(
            AgentRole::Builder,
            BuilderMessage {
                approach: "test".to_string(),
                changes: vec![],
                claims: vec![],
                confidence: 0.8,
                done: false,
                escalation: None,
                signals: AgentSignals::default(),
            },
        );

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("builder"));
        assert!(json.contains("test"));

        let _back: AgentMessage<BuilderMessage> = serde_json::from_str(&json).unwrap();
    }

    // --- AgentRole serde and Display ---

    #[test]
    fn agent_role_serde_variants() {
        let variants = vec![
            AgentRole::Architect,
            AgentRole::Builder,
            AgentRole::Skeptic,
            AgentRole::Judge,
            AgentRole::Scalpel,
            AgentRole::Coordinator,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: AgentRole = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    #[test]
    fn agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Architect), "Architect");
        assert_eq!(format!("{}", AgentRole::Builder), "Builder");
        assert_eq!(format!("{}", AgentRole::Scalpel), "Scalpel");
        assert_eq!(format!("{}", AgentRole::Coordinator), "Coordinator");
    }

    #[test]
    fn agent_role_equality() {
        assert_eq!(AgentRole::Architect, AgentRole::Architect);
        assert_ne!(AgentRole::Architect, AgentRole::Builder);
    }

    // --- ModuleAction serde ---

    #[test]
    fn module_action_serde_variants() {
        let json1 = serde_json::to_string(&ModuleAction::Create).unwrap();
        let json2 = serde_json::to_string(&ModuleAction::Modify).unwrap();
        assert_eq!(json1, "\"create\"");
        assert_eq!(json2, "\"modify\"");
        let d1: ModuleAction = serde_json::from_str(&json1).unwrap();
        let d2: ModuleAction = serde_json::from_str(&json2).unwrap();
        assert_eq!(d1, ModuleAction::Create);
        assert_eq!(d2, ModuleAction::Modify);
    }

    // --- EscalationTarget serde ---

    #[test]
    fn escalation_target_serde_variants() {
        let variants = vec![
            EscalationTarget::Architect,
            EscalationTarget::SecurityReview,
            EscalationTarget::PerformanceReview,
            EscalationTarget::Human,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: EscalationTarget = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    // --- SkepticVerdict serde ---

    #[test]
    fn skeptic_verdict_serde_variants() {
        let variants = vec![
            SkepticVerdict::Approve,
            SkepticVerdict::NeedsWork,
            SkepticVerdict::Veto,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: SkepticVerdict = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    // --- IssueSeverity serde ---

    #[test]
    fn issue_severity_serde_variants() {
        let variants = vec![
            IssueSeverity::Low,
            IssueSeverity::Medium,
            IssueSeverity::High,
            IssueSeverity::Critical,
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let decoded: IssueSeverity = serde_json::from_str(&json).unwrap();
            assert_eq!(*v, decoded);
        }
    }

    // --- DependencySpec serde ---

    #[test]
    fn dependency_spec_serde_roundtrip() {
        let spec = DependencySpec {
            name: "serde".into(),
            version: "1.0".into(),
            features: vec!["derive".into()],
            reason: "serialization".into(),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let decoded: DependencySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "serde");
        assert_eq!(decoded.features.len(), 1);
    }

    // --- FileChange serde ---

    #[test]
    fn file_change_serde_roundtrip() {
        let change = FileChange {
            path: "src/main.rs".into(),
            summary: "added feature".into(),
            diff_hunk: "+new line".into(),
            lines_added: 5,
            lines_removed: 2,
        };
        let json = serde_json::to_string(&change).unwrap();
        let decoded: FileChange = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.path, "src/main.rs");
        assert_eq!(decoded.lines_added, 5);
        assert_eq!(decoded.lines_removed, 2);
    }

    // --- ValidationResult ---

    #[test]
    fn validation_result_ok() {
        let vr = ValidationResult { errors: vec![] };
        assert!(vr.is_ok());
        assert!(!vr.is_err());
        assert_eq!(format!("{}", vr), "Validation OK");
    }

    #[test]
    fn validation_result_err() {
        let vr = ValidationResult {
            errors: vec!["bad".into(), "worse".into()],
        };
        assert!(!vr.is_ok());
        assert!(vr.is_err());
        let s = format!("{}", vr);
        assert!(s.contains("bad"));
        assert!(s.contains("worse"));
    }

    #[test]
    fn validation_result_combine() {
        let a = ValidationResult {
            errors: vec!["e1".into()],
        };
        let b = ValidationResult {
            errors: vec!["e2".into()],
        };
        let combined = a.combine(b);
        assert_eq!(combined.errors.len(), 2);
    }

    // --- BuilderMessage validate edge cases ---

    #[test]
    fn builder_validate_no_changes_not_done() {
        let msg = BuilderMessage {
            approach: "test".into(),
            changes: vec![],
            claims: vec![],
            confidence: 0.8,
            done: false,
            escalation: None,
            signals: AgentSignals::default(),
        };
        let vr = msg.validate();
        assert!(vr.is_err());
    }

    #[test]
    fn builder_validate_no_changes_but_done() {
        let msg = BuilderMessage {
            approach: "test".into(),
            changes: vec![],
            claims: vec![],
            confidence: 0.8,
            done: true,
            escalation: None,
            signals: AgentSignals::default(),
        };
        let vr = msg.validate();
        assert!(vr.is_ok());
    }

    #[test]
    fn builder_validate_bad_confidence() {
        let msg = BuilderMessage {
            approach: "test".into(),
            changes: vec![FileChange {
                path: "a.rs".into(),
                summary: "x".into(),
                diff_hunk: String::new(),
                lines_added: 1,
                lines_removed: 0,
            }],
            claims: vec![],
            confidence: 1.5,
            done: true,
            escalation: None,
            signals: AgentSignals::default(),
        };
        let vr = msg.validate();
        assert!(vr.is_err());
    }

    // --- Architect validate: implementor not declared ---

    #[test]
    fn architect_validate_unregistered_implementor() {
        let msg = ArchitectMessage {
            declaration: StructuralDeclaration {
                modules: vec![ModuleDeclaration {
                    path: "src/a.rs".into(),
                    action: ModuleAction::Create,
                    exports: vec![],
                    imports: vec![],
                    purpose: "module a".into(),
                }],
                interfaces: vec![InterfaceDeclaration {
                    name: "Trait1".into(),
                    defined_in: "src/a.rs".into(),
                    methods: vec![],
                    implementors: vec!["src/b.rs".into()], // not declared!
                }],
                dependencies: DependencyChanges::default(),
            },
            rationale: "test".into(),
            confidence: 0.8,
            rearchitecture_triggers: vec![],
        };
        let vr = msg.validate();
        assert!(vr.is_err());
        assert!(vr.errors.iter().any(|e| e.contains("src/b.rs")));
    }

    #[test]
    fn architect_validate_bad_confidence() {
        let msg = ArchitectMessage {
            declaration: StructuralDeclaration::default(),
            rationale: "test".into(),
            confidence: -0.5,
            rearchitecture_triggers: vec![],
        };
        let vr = msg.validate();
        assert!(vr.is_err());
    }

    // --- ScalpelMessage / LinesChanged serde ---

    #[test]
    fn lines_changed_default() {
        let lc = LinesChanged::default();
        assert_eq!(lc.added, 0);
        assert_eq!(lc.removed, 0);
    }

    #[test]
    fn scalpel_message_serde_roundtrip() {
        let msg = ScalpelMessage {
            fixes: vec![SurgicalFix {
                file: "src/lib.rs".into(),
                issue: "missing semicolon".into(),
                action: "added semicolon".into(),
                lines_changed: LinesChanged {
                    added: 1,
                    removed: 0,
                },
            }],
            done: true,
            remaining_issues: vec![],
            exceeded_scope: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: ScalpelMessage = serde_json::from_str(&json).unwrap();
        assert!(decoded.done);
        assert_eq!(decoded.fixes.len(), 1);
        assert_eq!(decoded.fixes[0].file, "src/lib.rs");
    }

    // --- AgentSignals default ---

    #[test]
    fn agent_signals_default() {
        let s = AgentSignals::default();
        assert!(!s.suggest_scalpel);
        assert!(!s.suggest_deep_review);
        assert!(s.insights.is_empty());
    }

    // --- AgentMessage directed and with_reply ---

    #[test]
    fn agent_message_directed() {
        let msg = AgentMessage::new(AgentRole::Builder, "payload".to_string())
            .directed(AgentRole::Skeptic);
        assert_eq!(msg.from, AgentRole::Builder);
        assert_eq!(msg.to, Some(AgentRole::Skeptic));
    }

    #[test]
    fn agent_message_with_reply() {
        let msg = AgentMessage::new(AgentRole::Builder, "payload".to_string())
            .with_reply("msg-123".to_string());
        assert_eq!(msg.in_reply_to, Some("msg-123".to_string()));
    }

    // --- StructuralDeclaration default ---

    #[test]
    fn structural_declaration_default() {
        let sd = StructuralDeclaration::default();
        assert!(sd.modules.is_empty());
        assert!(sd.interfaces.is_empty());
        assert!(sd.dependencies.add.is_empty());
    }

    // --- DependencyChanges default ---

    #[test]
    fn dependency_changes_default() {
        let dc = DependencyChanges::default();
        assert!(dc.add.is_empty());
        assert!(dc.remove.is_empty());
        assert!(dc.keep.is_empty());
    }

    // --- CompileError serde ---

    #[test]
    fn compile_error_serde_roundtrip() {
        let ce = CompileError {
            code: Some("E0308".into()),
            message: "mismatched types".into(),
            file: "src/main.rs".into(),
            line: Some(42),
            is_scalpel_appropriate: true,
        };
        let json = serde_json::to_string(&ce).unwrap();
        let decoded: CompileError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.code, Some("E0308".into()));
        assert_eq!(decoded.line, Some(42));
    }

    // --- TestSummary default ---

    #[test]
    fn test_summary_default() {
        let ts = TestSummary::default();
        assert_eq!(ts.total, 0);
        assert_eq!(ts.passed, 0);
        assert_eq!(ts.failed, 0);
    }

    // --- JudgeMessage serde ---

    #[test]
    fn judge_message_serde_roundtrip() {
        let msg = JudgeMessage {
            compiles: true,
            compile_errors: vec![],
            tests: TestSummary {
                total: 10,
                passed: 10,
                failed: 0,
                failed_names: vec![],
            },
            dirty_files: vec!["src/lib.rs".into()],
            recommend_scalpel: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: JudgeMessage = serde_json::from_str(&json).unwrap();
        assert!(decoded.compiles);
        assert_eq!(decoded.tests.passed, 10);
    }

    // --- SkepticMessage serde ---

    #[test]
    fn skeptic_message_serde_roundtrip() {
        let msg = SkepticMessage {
            verdict: SkepticVerdict::Approve,
            verified: vec![VerifiedClaim {
                claim: "file exists".into(),
                evidence: "ls shows file".into(),
            }],
            refuted: vec![],
            insights: vec!["clean code".into()],
            structural_compliance: StructuralCompliance {
                files_compliant: true,
                deps_compliant: true,
                violations: vec![],
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SkepticMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.verdict, SkepticVerdict::Approve);
        assert_eq!(decoded.verified.len(), 1);
    }

    // --- RefutedClaim serde ---

    #[test]
    fn refuted_claim_serde_roundtrip() {
        let rc = RefutedClaim {
            claim: "tests pass".into(),
            evidence: "2 failures found".into(),
            severity: IssueSeverity::High,
        };
        let json = serde_json::to_string(&rc).unwrap();
        let decoded: RefutedClaim = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.severity, IssueSeverity::High);
    }

    // --- StructuralCompliance default ---

    #[test]
    fn structural_compliance_default() {
        let sc = StructuralCompliance::default();
        assert!(!sc.files_compliant);
        assert!(!sc.deps_compliant);
        assert!(sc.violations.is_empty());
    }

    // --- Constants ---

    #[test]
    fn protocol_constants() {
        assert_eq!(SCALPEL_MAX_LINES_PER_FILE, 10);
        assert!((ARCHITECT_MIN_CONFIDENCE - 0.7).abs() < f64::EPSILON);
        assert!((BUILDER_MIN_CONFIDENCE - 0.6).abs() < f64::EPSILON);
        assert_eq!(SCALPEL_MAX_FILES, 3);
    }

    // --- InterfaceDeclaration serde ---

    #[test]
    fn interface_declaration_serde_roundtrip() {
        let id = InterfaceDeclaration {
            name: "Handler".into(),
            defined_in: "src/handler.rs".into(),
            methods: vec!["handle(req: Request)".into()],
            implementors: vec!["src/async_handler.rs".into()],
        };
        let json = serde_json::to_string(&id).unwrap();
        let decoded: InterfaceDeclaration = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Handler");
        assert_eq!(decoded.methods.len(), 1);
    }
}
