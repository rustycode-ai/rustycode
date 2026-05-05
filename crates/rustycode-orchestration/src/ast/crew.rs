//! Construction Crew Pattern for AST (Adaptive Structured Thinking).
//!
//! Implements the crew orchestration topology with sequential handoffs through
//! research, planning, execution, and verification roles. The crew enforces a
//! strict handoff protocol where each role produces a specific artifact consumed
//! by the next role in the chain.
//!
//! ```text
//! FOREMAN -> SCOUT -> ARCHITECT -> BUILDER -> INSPECTOR
//!               |        |          |          |
//!            research   plan     execute    verify
//!                        |
//!                     CONSULTANT (on-call)
//! ```
//!
//! Handoff chain:
//! ```text
//! Foreman -(TaskAssessment)-> Scout -(ContextBrief)-> Architect
//!   -(ExecutionSegment)-> Builder -(ExecutionEvidence)-> Inspector
//!   -(VerificationReport)-> Foreman
//! ```

use serde::{Deserialize, Serialize};

use super::types::ComplexityLevel;

// Crew role

/// Crew role in the AST construction crew.
///
/// Each role has a single responsibility in the pipeline and produces a
/// well-defined artifact for the next role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CrewRole {
    /// Receives task request, classifies, owns ledger, dispatches other roles.
    /// Does NOT become main researcher/planner/builder.
    Foreman,
    /// Runs Phase 1 (Research). Produces `ContextBrief`.
    Scout,
    /// Runs Phase 2 (Skeleton) + batch of Phase 3 (Expand). Produces
    /// `MilestoneSkeleton` + `ExecutionSegment`.
    Architect,
    /// Runs Phase 3b (Execute). Thinking OFF during execution. Produces evidence.
    Builder,
    /// Runs Phase 4 (Verify). Two-stage: spec compliance + code quality.
    Inspector,
    /// Escalation role for systemic blockers. Deep analysis, proposes reclassification.
    Consultant,
}

impl CrewRole {
    /// Expected duration range in seconds for this role.
    ///
    /// Returns `(min_seconds, max_seconds)`.
    pub const fn duration_range(&self) -> (u64, u64) {
        match self {
            Self::Foreman => (1, 3),
            Self::Scout => (2, 15),
            Self::Architect => (3, 10),
            Self::Builder => (1, 30),
            Self::Inspector => (5, 30),
            Self::Consultant => (5, 20),
        }
    }
}

impl std::fmt::Display for CrewRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Foreman => write!(f, "FOREMAN"),
            Self::Scout => write!(f, "SCOUT"),
            Self::Architect => write!(f, "ARCHITECT"),
            Self::Builder => write!(f, "BUILDER"),
            Self::Inspector => write!(f, "INSPECTOR"),
            Self::Consultant => write!(f, "CONSULTANT"),
        }
    }
}

// Handoff types

/// Status of a crew handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HandoffStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
}

/// Kinds of artifacts passed between crew roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    TaskAssessment,
    ContextBrief,
    MilestoneSkeleton,
    ExecutionSegment,
    ExecutionEvidence,
    VerificationReport,
    ConsultationReport,
}

/// A structured handoff between crew roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrewHandoff {
    pub id: String,
    pub source_role: CrewRole,
    pub target_role: CrewRole,
    pub source_artifact: ArtifactKind,
    pub target_artifact: ArtifactKind,
    pub status: HandoffStatus,
    pub ledger_event_id: String,
    pub timestamp: String,
    pub evidence_pointers: Vec<String>,
    /// Mandatory requirements that the target role must acknowledge before starting.
    ///
    /// Populated by the Architect during the Architect -> Builder handoff to prevent
    /// critical requirements from being lost during context transfer (v0.4 Implication 3).
    /// The builder must verify each requirement is addressed in its execution plan.
    #[serde(default)]
    pub requirements_checklist: Vec<String>,
}

impl CrewHandoff {
    /// Validate that all requirements have been acknowledged by the target role.
    ///
    /// Returns a list of unacknowledged requirement descriptions.
    pub fn unacknowledged_requirements(&self, acknowledged: &[String]) -> Vec<String> {
        let acknowledged_set: std::collections::HashSet<_> = acknowledged.iter().collect();
        self.requirements_checklist
            .iter()
            .filter(|r| !acknowledged_set.contains(r))
            .cloned()
            .collect()
    }
}

/// A consultation report from the Consultant role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsultationReport {
    pub blocker_description: String,
    pub failure_pattern: String,
    pub proposed_reclassification: Option<ComplexityLevel>,
    pub proposed_scope_expansion: Vec<String>,
    pub proposed_strategy_change: Option<String>,
    pub findings: Vec<String>,
}

// Legal handoff table

/// Returns the set of legal `(source, target, source_artifact)` triples.
///
/// The handoff protocol enforces strict sequential ordering with no backward
/// or cross-branch handoffs.
fn legal_handoffs() -> &'static [(CrewRole, CrewRole, ArtifactKind, ArtifactKind)] {
    static TABLE: &[(CrewRole, CrewRole, ArtifactKind, ArtifactKind)] = &[
        // Main chain
        (
            CrewRole::Foreman,
            CrewRole::Scout,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
        ),
        (
            CrewRole::Scout,
            CrewRole::Architect,
            ArtifactKind::ContextBrief,
            ArtifactKind::MilestoneSkeleton,
        ),
        (
            CrewRole::Architect,
            CrewRole::Builder,
            ArtifactKind::ExecutionSegment,
            ArtifactKind::ExecutionEvidence,
        ),
        (
            CrewRole::Builder,
            CrewRole::Inspector,
            ArtifactKind::ExecutionEvidence,
            ArtifactKind::VerificationReport,
        ),
        (
            CrewRole::Inspector,
            CrewRole::Foreman,
            ArtifactKind::VerificationReport,
            ArtifactKind::VerificationReport,
        ),
        // Escalation path
        (
            CrewRole::Foreman,
            CrewRole::Consultant,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ConsultationReport,
        ),
        (
            CrewRole::Consultant,
            CrewRole::Foreman,
            ArtifactKind::ConsultationReport,
            ArtifactKind::TaskAssessment,
        ),
    ];
    TABLE
}

/// Returns the artifact a role expects to receive as input, if any.
const fn input_artifact_for(role: CrewRole) -> Option<ArtifactKind> {
    match role {
        CrewRole::Scout | CrewRole::Consultant => Some(ArtifactKind::TaskAssessment),
        CrewRole::Architect => Some(ArtifactKind::ContextBrief),
        CrewRole::Builder => Some(ArtifactKind::ExecutionSegment),
        CrewRole::Inspector => Some(ArtifactKind::ExecutionEvidence),
        CrewRole::Foreman => None, // Foreman receives from Inspector or Consultant, both variable
    }
}

/// Returns the primary artifact a role is expected to produce.
#[allow(clippy::unnecessary_wraps)]
const fn output_artifact_for(role: CrewRole) -> Option<ArtifactKind> {
    match role {
        CrewRole::Foreman => Some(ArtifactKind::TaskAssessment),
        CrewRole::Scout => Some(ArtifactKind::ContextBrief),
        CrewRole::Architect => Some(ArtifactKind::ExecutionSegment),
        CrewRole::Builder => Some(ArtifactKind::ExecutionEvidence),
        CrewRole::Inspector => Some(ArtifactKind::VerificationReport),
        CrewRole::Consultant => Some(ArtifactKind::ConsultationReport),
    }
}

// CrewDispatcher

/// Number of consecutive failures on the same milestone before escalation.
const ESCALATION_THRESHOLD: u32 = 3;

/// Validates and enforces the crew handoff protocol.
///
/// The dispatcher tracks all completed handoffs and uses them to determine
/// whether a role has its prerequisites met and can start work.
pub struct CrewDispatcher {
    handoffs: Vec<CrewHandoff>,
}

impl CrewDispatcher {
    pub const fn new() -> Self {
        Self {
            handoffs: Vec::new(),
        }
    }

    /// Validate that a handoff from `source` to `target` with the given
    /// source artifact is legal according to the crew protocol.
    ///
    /// Rules enforced:
    /// - Only explicitly listed `(source, target, artifact)` triples are valid.
    /// - No backward handoffs (e.g. Builder -> Architect).
    /// - No cross-branch handoffs (e.g. Scout -> Builder).
    pub fn validate_handoff(
        &self,
        source: CrewRole,
        target: CrewRole,
        artifact: &ArtifactKind,
    ) -> bool {
        legal_handoffs()
            .iter()
            .any(|(s, t, a, _)| *s == source && *t == target && *a == *artifact)
    }

    /// Record a completed handoff in the dispatcher history.
    pub fn record_handoff(&mut self, handoff: CrewHandoff) {
        self.handoffs.push(handoff);
    }

    /// Check if a role can start -- meaning its prerequisite handoff has been
    /// completed.
    ///
    /// - Foreman can always start (it initiates the pipeline).
    /// - Consultant can always start (escalation path).
    /// - Other roles require a completed handoff from their predecessor.
    pub fn can_start(&self, role: CrewRole) -> bool {
        match role {
            CrewRole::Foreman | CrewRole::Consultant => true,
            CrewRole::Scout => self.has_completed_handoff_from(CrewRole::Foreman, CrewRole::Scout),
            CrewRole::Architect => {
                self.has_completed_handoff_from(CrewRole::Scout, CrewRole::Architect)
            }
            CrewRole::Builder => {
                self.has_completed_handoff_from(CrewRole::Architect, CrewRole::Builder)
            }
            CrewRole::Inspector => {
                self.has_completed_handoff_from(CrewRole::Builder, CrewRole::Inspector)
            }
        }
    }

    /// Get the expected input artifact for a role.
    pub const fn expected_input(&self, role: CrewRole) -> Option<ArtifactKind> {
        input_artifact_for(role)
    }

    /// Get the expected output artifact for a role.
    pub const fn expected_output(&self, role: CrewRole) -> Option<ArtifactKind> {
        output_artifact_for(role)
    }

    /// Check if the consultant should be engaged due to repeated failure.
    ///
    /// Returns `true` when `failure_count >= ESCALATION_THRESHOLD`.
    pub const fn should_escalate(&self, failure_count: u32) -> bool {
        failure_count >= ESCALATION_THRESHOLD
    }

    /// Returns the number of recorded handoffs.
    pub const fn handoff_count(&self) -> usize {
        self.handoffs.len()
    }

    /// Returns a reference to the recorded handoffs.
    pub fn handoffs(&self) -> &[CrewHandoff] {
        &self.handoffs
    }

    // -- private helpers ---------------------------------------------------

    /// Check whether a completed handoff from `source` to `target` exists.
    fn has_completed_handoff_from(&self, source: CrewRole, target: CrewRole) -> bool {
        self.handoffs.iter().any(|h| {
            h.source_role == source
                && h.target_role == target
                && h.status == HandoffStatus::Complete
        })
    }
}

impl Default for CrewDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// Dynamic Roles (Gap 4)

/// Configuration for how many of each role to dispatch based on task complexity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleDispatchConfig {
    pub scout_count: u8,
    pub architect_count: u8,
    pub builder_count: u8,
    pub enable_consultant: bool,
}

/// Dispatch role counts based on task complexity.
///
/// - TRIVIAL: 1 scout, 1 builder, no consultant.
/// - MODERATE: 1 scout, 1 architect, 1 builder, consultant on-call.
/// - COMPLEX: 2 scouts, 1 architect, 2 builders, consultant on-call.
pub const fn dispatch_roles(complexity: ComplexityLevel) -> RoleDispatchConfig {
    match complexity {
        ComplexityLevel::Trivial => RoleDispatchConfig {
            scout_count: 1,
            architect_count: 0,
            builder_count: 1,
            enable_consultant: false,
        },
        ComplexityLevel::Moderate => RoleDispatchConfig {
            scout_count: 1,
            architect_count: 1,
            builder_count: 1,
            enable_consultant: true,
        },
        ComplexityLevel::Complex => RoleDispatchConfig {
            scout_count: 2,
            architect_count: 1,
            builder_count: 2,
            enable_consultant: true,
        },
    }
}

/// Assign subroles (role + description pairs) based on the dispatch config.
///
/// Multiple scouts get differentiated focus areas (e.g., codebase scan, docs scan).
/// Multiple builders get differentiated focus areas (e.g., primary, support).
pub fn assign_subroles(roles: &RoleDispatchConfig) -> Vec<(CrewRole, String)> {
    let mut subroles = Vec::new();

    // Foreman is always present.
    subroles.push((CrewRole::Foreman, "task classification and dispatch".into()));

    // Scouts with differentiated focus areas.
    let scout_focus = if roles.scout_count >= 2 {
        vec!["codebase_scan", "docs_scan"]
    } else {
        vec!["full_scan"]
    };
    for focus in scout_focus.iter().take(roles.scout_count as usize) {
        subroles.push((CrewRole::Scout, (*focus).into()));
    }

    // Architects.
    for _ in 0..roles.architect_count {
        subroles.push((CrewRole::Architect, "plan_and_skeleton".into()));
    }

    // Builders with differentiated focus areas.
    let builder_focus = if roles.builder_count >= 2 {
        vec!["primary_implementation", "support_implementation"]
    } else {
        vec!["implementation"]
    };
    for focus in builder_focus.iter().take(roles.builder_count as usize) {
        subroles.push((CrewRole::Builder, (*focus).into()));
    }

    // Inspector is always present for verification.
    subroles.push((CrewRole::Inspector, "verification".into()));

    // Consultant is on-call if enabled.
    if roles.enable_consultant {
        subroles.push((CrewRole::Consultant, "escalation_on_call".into()));
    }

    subroles
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a handoff with sensible defaults for testing.
    fn make_handoff(
        source: CrewRole,
        target: CrewRole,
        source_artifact: ArtifactKind,
        target_artifact: ArtifactKind,
        status: HandoffStatus,
    ) -> CrewHandoff {
        CrewHandoff {
            id: uuid::Uuid::new_v4().to_string(),
            source_role: source,
            target_role: target,
            source_artifact,
            target_artifact,
            status,
            ledger_event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            evidence_pointers: vec![],
            requirements_checklist: vec![],
        }
    }

    // -- Display -----------------------------------------------------------

    #[test]
    fn crew_role_display() {
        assert_eq!(CrewRole::Foreman.to_string(), "FOREMAN");
        assert_eq!(CrewRole::Scout.to_string(), "SCOUT");
        assert_eq!(CrewRole::Architect.to_string(), "ARCHITECT");
        assert_eq!(CrewRole::Builder.to_string(), "BUILDER");
        assert_eq!(CrewRole::Inspector.to_string(), "INSPECTOR");
        assert_eq!(CrewRole::Consultant.to_string(), "CONSULTANT");
    }

    // -- Duration ranges ---------------------------------------------------

    #[test]
    fn duration_ranges_are_reasonable() {
        let ranges = [
            CrewRole::Foreman,
            CrewRole::Scout,
            CrewRole::Architect,
            CrewRole::Builder,
            CrewRole::Inspector,
            CrewRole::Consultant,
        ];
        for role in ranges {
            let (min, max) = role.duration_range();
            assert!(min > 0, "{role}: minimum duration must be positive");
            assert!(max > min, "{role}: maximum duration must exceed minimum");
            assert!(
                max <= 60,
                "{role}: maximum duration should be at most 60 seconds"
            );
        }
    }

    #[test]
    fn foreman_is_fast() {
        let (foreman_min, foreman_max) = CrewRole::Foreman.duration_range();
        // Foreman is a coordinator, should be quick
        assert!(
            foreman_max <= 5,
            "Foreman should complete quickly (max {foreman_max}s)"
        );
        assert!(
            foreman_min > 0,
            "Foreman should have positive minimum duration"
        );
    }

    // -- Valid handoff chain -----------------------------------------------

    #[test]
    fn valid_handoff_chain() {
        let mut dispatcher = CrewDispatcher::new();

        // Foreman -> Scout
        assert!(dispatcher.validate_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            &ArtifactKind::TaskAssessment
        ));

        // Record and verify Scout can start
        let h1 = make_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
            HandoffStatus::Complete,
        );
        dispatcher.record_handoff(h1);
        assert!(dispatcher.can_start(CrewRole::Scout));

        // Scout -> Architect
        assert!(dispatcher.validate_handoff(
            CrewRole::Scout,
            CrewRole::Architect,
            &ArtifactKind::ContextBrief
        ));

        let h2 = make_handoff(
            CrewRole::Scout,
            CrewRole::Architect,
            ArtifactKind::ContextBrief,
            ArtifactKind::MilestoneSkeleton,
            HandoffStatus::Complete,
        );
        dispatcher.record_handoff(h2);
        assert!(dispatcher.can_start(CrewRole::Architect));

        // Architect -> Builder
        assert!(dispatcher.validate_handoff(
            CrewRole::Architect,
            CrewRole::Builder,
            &ArtifactKind::ExecutionSegment
        ));

        let h3 = make_handoff(
            CrewRole::Architect,
            CrewRole::Builder,
            ArtifactKind::ExecutionSegment,
            ArtifactKind::ExecutionEvidence,
            HandoffStatus::Complete,
        );
        dispatcher.record_handoff(h3);
        assert!(dispatcher.can_start(CrewRole::Builder));

        // Builder -> Inspector
        assert!(dispatcher.validate_handoff(
            CrewRole::Builder,
            CrewRole::Inspector,
            &ArtifactKind::ExecutionEvidence
        ));

        let h4 = make_handoff(
            CrewRole::Builder,
            CrewRole::Inspector,
            ArtifactKind::ExecutionEvidence,
            ArtifactKind::VerificationReport,
            HandoffStatus::Complete,
        );
        dispatcher.record_handoff(h4);
        assert!(dispatcher.can_start(CrewRole::Inspector));

        // Inspector -> Foreman
        assert!(dispatcher.validate_handoff(
            CrewRole::Inspector,
            CrewRole::Foreman,
            &ArtifactKind::VerificationReport
        ));

        assert_eq!(dispatcher.handoff_count(), 4);
    }

    // -- Invalid handoffs --------------------------------------------------

    #[test]
    fn backward_handoff_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Builder -> Architect is backward and illegal
        assert!(!dispatcher.validate_handoff(
            CrewRole::Builder,
            CrewRole::Architect,
            &ArtifactKind::ExecutionEvidence
        ));
    }

    #[test]
    fn cross_branch_handoff_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Scout -> Builder skips Architect
        assert!(!dispatcher.validate_handoff(
            CrewRole::Scout,
            CrewRole::Builder,
            &ArtifactKind::ContextBrief
        ));
    }

    #[test]
    fn wrong_artifact_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Foreman -> Scout must carry TaskAssessment, not ContextBrief
        assert!(!dispatcher.validate_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            &ArtifactKind::ContextBrief
        ));
    }

    #[test]
    fn architect_to_builder_with_skeleton_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Architect -> Builder must carry ExecutionSegment, not MilestoneSkeleton
        assert!(!dispatcher.validate_handoff(
            CrewRole::Architect,
            CrewRole::Builder,
            &ArtifactKind::MilestoneSkeleton
        ));
    }

    #[test]
    fn inspector_to_scout_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Inspector cannot hand off to Scout
        assert!(!dispatcher.validate_handoff(
            CrewRole::Inspector,
            CrewRole::Scout,
            &ArtifactKind::VerificationReport
        ));
    }

    #[test]
    fn builder_to_foreman_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Builder must go through Inspector, not directly to Foreman
        assert!(!dispatcher.validate_handoff(
            CrewRole::Builder,
            CrewRole::Foreman,
            &ArtifactKind::ExecutionEvidence
        ));
    }

    #[test]
    fn consultant_to_builder_rejected() {
        let dispatcher = CrewDispatcher::new();
        // Consultant can only hand off to Foreman
        assert!(!dispatcher.validate_handoff(
            CrewRole::Consultant,
            CrewRole::Builder,
            &ArtifactKind::ConsultationReport
        ));
    }

    // -- Escalation path ---------------------------------------------------

    #[test]
    fn escalation_handoff_valid() {
        let dispatcher = CrewDispatcher::new();
        assert!(dispatcher.validate_handoff(
            CrewRole::Foreman,
            CrewRole::Consultant,
            &ArtifactKind::TaskAssessment
        ));
        assert!(dispatcher.validate_handoff(
            CrewRole::Consultant,
            CrewRole::Foreman,
            &ArtifactKind::ConsultationReport
        ));
    }

    // -- can_start with prerequisites --------------------------------------

    #[test]
    fn can_start_foreman_always() {
        let dispatcher = CrewDispatcher::new();
        assert!(dispatcher.can_start(CrewRole::Foreman));
    }

    #[test]
    fn can_start_consultant_always() {
        let dispatcher = CrewDispatcher::new();
        assert!(dispatcher.can_start(CrewRole::Consultant));
    }

    #[test]
    fn can_start_scout_requires_foreman_handoff() {
        let dispatcher = CrewDispatcher::new();
        // No handoffs recorded yet
        assert!(!dispatcher.can_start(CrewRole::Scout));
    }

    #[test]
    fn can_start_roles_without_prerequisites() {
        let mut dispatcher = CrewDispatcher::new();

        // Nothing can start except Foreman and Consultant
        assert!(!dispatcher.can_start(CrewRole::Scout));
        assert!(!dispatcher.can_start(CrewRole::Architect));
        assert!(!dispatcher.can_start(CrewRole::Builder));
        assert!(!dispatcher.can_start(CrewRole::Inspector));

        // Complete the full chain one by one and verify each unlocks
        dispatcher.record_handoff(make_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
            HandoffStatus::Complete,
        ));
        assert!(dispatcher.can_start(CrewRole::Scout));
        assert!(!dispatcher.can_start(CrewRole::Architect));

        dispatcher.record_handoff(make_handoff(
            CrewRole::Scout,
            CrewRole::Architect,
            ArtifactKind::ContextBrief,
            ArtifactKind::MilestoneSkeleton,
            HandoffStatus::Complete,
        ));
        assert!(dispatcher.can_start(CrewRole::Architect));
        assert!(!dispatcher.can_start(CrewRole::Builder));

        dispatcher.record_handoff(make_handoff(
            CrewRole::Architect,
            CrewRole::Builder,
            ArtifactKind::ExecutionSegment,
            ArtifactKind::ExecutionEvidence,
            HandoffStatus::Complete,
        ));
        assert!(dispatcher.can_start(CrewRole::Builder));
        assert!(!dispatcher.can_start(CrewRole::Inspector));

        dispatcher.record_handoff(make_handoff(
            CrewRole::Builder,
            CrewRole::Inspector,
            ArtifactKind::ExecutionEvidence,
            ArtifactKind::VerificationReport,
            HandoffStatus::Complete,
        ));
        assert!(dispatcher.can_start(CrewRole::Inspector));
    }

    #[test]
    fn pending_handoff_does_not_unlock() {
        let mut dispatcher = CrewDispatcher::new();
        dispatcher.record_handoff(make_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
            HandoffStatus::Pending,
        ));
        assert!(!dispatcher.can_start(CrewRole::Scout));
    }

    #[test]
    fn failed_handoff_does_not_unlock() {
        let mut dispatcher = CrewDispatcher::new();
        dispatcher.record_handoff(make_handoff(
            CrewRole::Foreman,
            CrewRole::Scout,
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
            HandoffStatus::Failed,
        ));
        assert!(!dispatcher.can_start(CrewRole::Scout));
    }

    // -- Escalation trigger ------------------------------------------------

    #[test]
    fn escalation_trigger_at_three_failures() {
        let dispatcher = CrewDispatcher::new();
        assert!(!dispatcher.should_escalate(0));
        assert!(!dispatcher.should_escalate(1));
        assert!(!dispatcher.should_escalate(2));
        assert!(dispatcher.should_escalate(3));
        assert!(dispatcher.should_escalate(4));
        assert!(dispatcher.should_escalate(10));
    }

    // -- Expected input / output -------------------------------------------

    #[test]
    fn expected_input_for_each_role() {
        let dispatcher = CrewDispatcher::new();

        assert_eq!(dispatcher.expected_input(CrewRole::Foreman), None);
        assert_eq!(
            dispatcher.expected_input(CrewRole::Scout),
            Some(ArtifactKind::TaskAssessment)
        );
        assert_eq!(
            dispatcher.expected_input(CrewRole::Architect),
            Some(ArtifactKind::ContextBrief)
        );
        assert_eq!(
            dispatcher.expected_input(CrewRole::Builder),
            Some(ArtifactKind::ExecutionSegment)
        );
        assert_eq!(
            dispatcher.expected_input(CrewRole::Inspector),
            Some(ArtifactKind::ExecutionEvidence)
        );
        assert_eq!(
            dispatcher.expected_input(CrewRole::Consultant),
            Some(ArtifactKind::TaskAssessment)
        );
    }

    #[test]
    fn expected_output_for_each_role() {
        let dispatcher = CrewDispatcher::new();

        assert_eq!(
            dispatcher.expected_output(CrewRole::Foreman),
            Some(ArtifactKind::TaskAssessment)
        );
        assert_eq!(
            dispatcher.expected_output(CrewRole::Scout),
            Some(ArtifactKind::ContextBrief)
        );
        assert_eq!(
            dispatcher.expected_output(CrewRole::Architect),
            Some(ArtifactKind::ExecutionSegment)
        );
        assert_eq!(
            dispatcher.expected_output(CrewRole::Builder),
            Some(ArtifactKind::ExecutionEvidence)
        );
        assert_eq!(
            dispatcher.expected_output(CrewRole::Inspector),
            Some(ArtifactKind::VerificationReport)
        );
        assert_eq!(
            dispatcher.expected_output(CrewRole::Consultant),
            Some(ArtifactKind::ConsultationReport)
        );
    }

    // -- Serialization roundtrip -------------------------------------------

    #[test]
    fn crew_role_serialization_roundtrip() {
        let roles = [
            CrewRole::Foreman,
            CrewRole::Scout,
            CrewRole::Architect,
            CrewRole::Builder,
            CrewRole::Inspector,
            CrewRole::Consultant,
        ];
        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let back: CrewRole = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
    }

    #[test]
    fn handoff_status_serialization_roundtrip() {
        let statuses = [
            HandoffStatus::Pending,
            HandoffStatus::InProgress,
            HandoffStatus::Complete,
            HandoffStatus::Failed,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let back: HandoffStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn artifact_kind_serialization_roundtrip() {
        let artifacts = [
            ArtifactKind::TaskAssessment,
            ArtifactKind::ContextBrief,
            ArtifactKind::MilestoneSkeleton,
            ArtifactKind::ExecutionSegment,
            ArtifactKind::ExecutionEvidence,
            ArtifactKind::VerificationReport,
            ArtifactKind::ConsultationReport,
        ];
        for artifact in artifacts {
            let json = serde_json::to_string(&artifact).unwrap();
            let back: ArtifactKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, artifact);
        }
    }

    #[test]
    fn crew_handoff_serialization_roundtrip() {
        let handoff = CrewHandoff {
            id: "test-id".into(),
            source_role: CrewRole::Foreman,
            target_role: CrewRole::Scout,
            source_artifact: ArtifactKind::TaskAssessment,
            target_artifact: ArtifactKind::ContextBrief,
            status: HandoffStatus::Complete,
            ledger_event_id: "ledger-123".into(),
            timestamp: "2026-04-26T12:00:00Z".into(),
            evidence_pointers: vec!["file.rs".into()],
            requirements_checklist: vec!["verify completeness".into()],
        };
        let json = serde_json::to_string(&handoff).unwrap();
        let back: CrewHandoff = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-id");
        assert_eq!(back.source_role, CrewRole::Foreman);
        assert_eq!(back.target_role, CrewRole::Scout);
        assert_eq!(back.status, HandoffStatus::Complete);
        assert_eq!(back.evidence_pointers, vec!["file.rs"]);
    }

    #[test]
    fn consultation_report_serialization_roundtrip() {
        let report = ConsultationReport {
            blocker_description: "Cannot resolve import".into(),
            failure_pattern: "Build fails on CI only".into(),
            proposed_reclassification: Some(ComplexityLevel::Complex),
            proposed_scope_expansion: vec!["Add CI config analysis".into()],
            proposed_strategy_change: Some("Switch to RollingWave".into()),
            findings: vec!["Missing dependency in CI".into()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: ConsultationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.blocker_description, "Cannot resolve import");
        assert_eq!(
            back.proposed_reclassification,
            Some(ComplexityLevel::Complex)
        );
        assert_eq!(back.findings.len(), 1);
    }

    // -- Default impl ------------------------------------------------------

    #[test]
    fn dispatcher_default_matches_new() {
        let new = CrewDispatcher::new();
        let default = CrewDispatcher::default();
        assert_eq!(new.handoff_count(), default.handoff_count());
        assert!(new.handoffs().is_empty());
        assert!(default.handoffs().is_empty());
    }

    // -- Gap 4: Dynamic Roles tests -----------------------------------------

    #[test]
    fn dispatch_roles_trivial() {
        let config = dispatch_roles(ComplexityLevel::Trivial);
        assert_eq!(config.scout_count, 1);
        assert_eq!(config.architect_count, 0);
        assert_eq!(config.builder_count, 1);
        assert!(!config.enable_consultant);
    }

    #[test]
    fn dispatch_roles_moderate() {
        let config = dispatch_roles(ComplexityLevel::Moderate);
        assert_eq!(config.scout_count, 1);
        assert_eq!(config.architect_count, 1);
        assert_eq!(config.builder_count, 1);
        assert!(config.enable_consultant);
    }

    #[test]
    fn dispatch_roles_complex() {
        let config = dispatch_roles(ComplexityLevel::Complex);
        assert_eq!(config.scout_count, 2);
        assert_eq!(config.architect_count, 1);
        assert_eq!(config.builder_count, 2);
        assert!(config.enable_consultant);
    }

    #[test]
    fn assign_subroles_trivial() {
        let config = dispatch_roles(ComplexityLevel::Trivial);
        let subroles = assign_subroles(&config);

        // Should have: Foreman, 1 Scout, 1 Builder, 1 Inspector (4 total)
        assert_eq!(subroles.len(), 4);
        assert_eq!(subroles[0].0, CrewRole::Foreman);
        assert_eq!(subroles[1].0, CrewRole::Scout);
        assert_eq!(subroles[1].1, "full_scan");
        assert_eq!(subroles[2].0, CrewRole::Builder);
        assert_eq!(subroles[2].1, "implementation");
        assert_eq!(subroles[3].0, CrewRole::Inspector);
    }

    #[test]
    fn assign_subroles_moderate() {
        let config = dispatch_roles(ComplexityLevel::Moderate);
        let subroles = assign_subroles(&config);

        // Should have: Foreman, 1 Scout, 1 Architect, 1 Builder, 1 Inspector, 1 Consultant (6 total)
        assert_eq!(subroles.len(), 6);
        assert_eq!(subroles[0].0, CrewRole::Foreman);
        assert_eq!(subroles[1].0, CrewRole::Scout);
        assert_eq!(subroles[2].0, CrewRole::Architect);
        assert_eq!(subroles[3].0, CrewRole::Builder);
        assert_eq!(subroles[4].0, CrewRole::Inspector);
        assert_eq!(subroles[5].0, CrewRole::Consultant);
    }

    #[test]
    fn assign_subroles_complex() {
        let config = dispatch_roles(ComplexityLevel::Complex);
        let subroles = assign_subroles(&config);

        // Should have: Foreman, 2 Scouts, 1 Architect, 2 Builders, 1 Inspector, 1 Consultant (8 total)
        assert_eq!(subroles.len(), 8);
        assert_eq!(subroles[0].0, CrewRole::Foreman);

        // Two scouts with differentiated focus
        assert_eq!(subroles[1].0, CrewRole::Scout);
        assert_eq!(subroles[1].1, "codebase_scan");
        assert_eq!(subroles[2].0, CrewRole::Scout);
        assert_eq!(subroles[2].1, "docs_scan");

        // Architect
        assert_eq!(subroles[3].0, CrewRole::Architect);

        // Two builders with differentiated focus
        assert_eq!(subroles[4].0, CrewRole::Builder);
        assert_eq!(subroles[4].1, "primary_implementation");
        assert_eq!(subroles[5].0, CrewRole::Builder);
        assert_eq!(subroles[5].1, "support_implementation");

        // Inspector and Consultant
        assert_eq!(subroles[6].0, CrewRole::Inspector);
        assert_eq!(subroles[7].0, CrewRole::Consultant);
    }

    #[test]
    fn role_dispatch_config_serialization_roundtrip() {
        let config = RoleDispatchConfig {
            scout_count: 2,
            architect_count: 1,
            builder_count: 2,
            enable_consultant: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: RoleDispatchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, config);
    }

    #[test]
    fn assign_subroles_no_consultant_when_disabled() {
        let config = RoleDispatchConfig {
            scout_count: 1,
            architect_count: 1,
            builder_count: 1,
            enable_consultant: false,
        };
        let subroles = assign_subroles(&config);
        assert!(
            !subroles
                .iter()
                .any(|(role, _)| *role == CrewRole::Consultant),
            "Consultant should not be assigned when disabled"
        );
    }

    #[test]
    fn assign_subroles_consultant_when_enabled() {
        let config = RoleDispatchConfig {
            scout_count: 1,
            architect_count: 1,
            builder_count: 1,
            enable_consultant: true,
        };
        let subroles = assign_subroles(&config);
        assert!(
            subroles
                .iter()
                .any(|(role, desc)| *role == CrewRole::Consultant && desc == "escalation_on_call"),
            "Consultant should be assigned as on-call when enabled"
        );
    }
}
