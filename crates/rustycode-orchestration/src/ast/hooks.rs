//! Hook bridge wiring the AST pipeline into the supervisor/conductor orchestration.
//!
//! The AST module has a 6-phase pipeline:
//! `CLASSIFY -> RESEARCH -> SKELETON -> EXPAND -> EXECUTE -> VERIFY`
//!
//! The spec (section 9) defines 6 hook points where the orchestrator can inject
//! context or override decisions. This module provides:
//!
//! - [`AstHookPoint`] -- enum of the 6 hook points
//! - [`AstHookPayload`] -- data payload passed at each hook
//! - [`AstHookResponse`] -- what the orchestrator can return
//! - [`AstHookBridge`] -- bridges AST hooks to the existing [`HookRegistry`]
//! - [`AstPhaseController`] -- wraps an [`AstPipeline`] and emits hook events
//!
//! # Integration functions
//!
//! - [`ast_phase_to_supervision_event`] converts AST events to supervisor events
//! - [`supervision_directive_to_hook_response`] converts supervisor directives back

use super::types::*;
use crate::hook_points::{HookContext, HookPoint, HookRegistry};
use crate::supervisor::{SupervisionDirective, SupervisionEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// AstHookPoint
// ---------------------------------------------------------------------------

/// Hook points corresponding to the 6 AST pipeline phases.
///
/// Each variant fires after the corresponding phase completes, allowing
/// the orchestrator to inject context, override decisions, or request
/// recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AstHookPoint {
    /// After Phase 0 (CLASSIFY): conductor may correct complexity, inject constraints.
    ClassifyComplete,
    /// After Phase 1 (RESEARCH): may inject context, docs, examples.
    ResearchComplete,
    /// After Phase 2 (SKELETON): may validate completeness, reorder milestones.
    SkeletonComplete,
    /// After Phase 3a (EXPAND): may inject research, narrow/widen batch.
    ExpandComplete,
    /// After Phase 3b (EXECUTE): may surface recovery strategy.
    ExecuteComplete,
    /// After Phase 4 (VERIFY): may request human review, approve partial.
    VerifyComplete,
}

impl AstHookPoint {
    /// All defined AST hook points (6 variants).
    pub const fn all() -> &'static [Self] {
        &[
            Self::ClassifyComplete,
            Self::ResearchComplete,
            Self::SkeletonComplete,
            Self::ExpandComplete,
            Self::ExecuteComplete,
            Self::VerifyComplete,
        ]
    }

    /// Map this AST hook point to the closest existing [`HookPoint`] variant.
    ///
    /// The mapping is:
    /// - `ClassifyComplete` -> `PlanStart`
    /// - `ResearchComplete` -> `ContextSwitch`
    /// - `SkeletonComplete` -> `PlanStart`
    /// - `ExpandComplete` -> `PlanStart`
    /// - `ExecuteComplete` -> `PostToolUse`
    /// - `VerifyComplete` -> `PlanEnd`
    #[allow(clippy::match_same_arms)]
    pub const fn to_hook_point(&self) -> HookPoint {
        match self {
            Self::ClassifyComplete => HookPoint::PlanStart,
            Self::ResearchComplete => HookPoint::ContextSwitch,
            Self::SkeletonComplete => HookPoint::PlanStart,
            Self::ExpandComplete => HookPoint::PlanStart,
            Self::ExecuteComplete => HookPoint::PostToolUse,
            Self::VerifyComplete => HookPoint::PlanEnd,
        }
    }

    /// Map an [`AstPhase`] to the corresponding AST hook point, if applicable.
    ///
    /// Only the 6 main pipeline phases produce hooks; `Complete` and `Failed`
    /// do not.
    pub const fn from_phase(phase: AstPhase) -> Option<Self> {
        match phase {
            AstPhase::Classify => Some(Self::ClassifyComplete),
            AstPhase::Research => Some(Self::ResearchComplete),
            AstPhase::Skeleton => Some(Self::SkeletonComplete),
            AstPhase::Expand => Some(Self::ExpandComplete),
            AstPhase::Execute => Some(Self::ExecuteComplete),
            AstPhase::Verify => Some(Self::VerifyComplete),
            AstPhase::Complete | AstPhase::Failed => None,
        }
    }

    /// Dot-separated event type string (e.g., "ast.classify.complete").
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::ClassifyComplete => "ast.classify.complete",
            Self::ResearchComplete => "ast.research.complete",
            Self::SkeletonComplete => "ast.skeleton.complete",
            Self::ExpandComplete => "ast.expand.complete",
            Self::ExecuteComplete => "ast.execute.complete",
            Self::VerifyComplete => "ast.verify.complete",
        }
    }
}

impl fmt::Display for AstHookPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.event_type())
    }
}

// ---------------------------------------------------------------------------
// AstHookPayload
// ---------------------------------------------------------------------------

/// Data payload passed to AST hook callbacks.
///
/// Fields are `Option`-wrapped or empty when the corresponding phase has
/// not yet produced its artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstHookPayload {
    /// Task identifier.
    pub task_id: String,
    /// Which phase just completed.
    pub phase: AstPhase,
    /// Assessment produced by Phase 0 (CLASSIFY).
    pub assessment: Option<TaskAssessment>,
    /// Context brief produced by Phase 1 (RESEARCH).
    pub brief: Option<ContextBrief>,
    /// Skeleton produced by Phase 2 (SKELETON).
    pub skeleton: Option<MilestoneSkeleton>,
    /// Currently expanded execution segments (Phase 3a).
    pub active_segments: Vec<ExecutionSegment>,
    /// Evidence collected per milestone (Phase 3b).
    pub evidence: HashMap<usize, Vec<StepEvidence>>,
    /// Verification report from Phase 4.
    pub report: Option<VerificationReport>,
}

impl Default for AstHookPayload {
    fn default() -> Self {
        Self {
            task_id: String::new(),
            phase: AstPhase::Classify,
            assessment: None,
            brief: None,
            skeleton: None,
            active_segments: Vec::new(),
            evidence: HashMap::new(),
            report: None,
        }
    }
}

// ---------------------------------------------------------------------------
// AstHookResponse
// ---------------------------------------------------------------------------

/// Response returned by an AST hook callback.
///
/// Each variant represents an action the orchestrator can take to
/// influence the next phase of the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstHookResponse {
    /// Proceed with the pipeline unchanged.
    Proceed,
    /// Override the complexity classification from Phase 0.
    OverrideComplexity {
        new_complexity: ComplexityLevel,
        reason: String,
    },
    /// Inject additional context into the pipeline.
    InjectContext {
        files: Vec<String>,
        constraints: Vec<String>,
    },
    /// Modify the milestone skeleton (reorder or drop milestones).
    ModifyMilestones {
        reorder: Option<Vec<usize>>,
        drop: Option<Vec<usize>>,
    },
    /// Request a recovery action for a specific milestone.
    RequestRecovery {
        milestone_id: usize,
        strategy: String,
    },
    /// Request human review before proceeding.
    RequestHumanReview { reason: String },
}

// ---------------------------------------------------------------------------
// AstHookCallbackFn
// ---------------------------------------------------------------------------

/// Type alias for AST hook callbacks.
///
/// Takes a reference to the payload and returns a response or an error.
pub type AstHookCallbackFn =
    dyn Fn(&AstHookPayload) -> anyhow::Result<AstHookResponse> + Send + Sync;

// ---------------------------------------------------------------------------
// AstHookBridge
// ---------------------------------------------------------------------------

/// Bridge between AST hook points and the existing [`HookRegistry`].
///
/// Maintains its own set of AST-specific callbacks and also forwards
/// events to the underlying [`HookRegistry`] for integration with the
/// broader orchestration hook system.
pub struct AstHookBridge {
    /// AST-specific handlers keyed by hook point.
    handlers: HashMap<AstHookPoint, Vec<Arc<AstHookCallbackFn>>>,
    /// Reference to the existing hook registry for forwarding.
    registry: HookRegistry,
}

impl fmt::Debug for AstHookBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AstHookBridge")
            .field("handler_count", &self.total_handlers())
            .finish()
    }
}

impl Default for AstHookBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl AstHookBridge {
    /// Create a new bridge with an empty hook registry.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            registry: HookRegistry::new(),
        }
    }

    /// Create a new bridge wrapping an existing hook registry.
    pub fn with_registry(registry: HookRegistry) -> Self {
        Self {
            handlers: HashMap::new(),
            registry,
        }
    }

    /// Register an AST-specific callback for the given hook point.
    ///
    /// Callbacks are executed in registration order.
    pub fn register<F>(&mut self, hook: AstHookPoint, callback: F)
    where
        F: Fn(&AstHookPayload) -> anyhow::Result<AstHookResponse> + Send + Sync + 'static,
    {
        self.handlers
            .entry(hook)
            .or_default()
            .push(Arc::new(callback));
    }

    /// Fire all handlers registered for the given phase's hook point.
    ///
    /// Also forwards a [`HookContext`] to the underlying [`HookRegistry`]
    /// so the broader orchestration system is notified.
    ///
    /// Returns a list of responses from AST-specific handlers.
    /// Errors from individual handlers are logged but do not prevent
    /// other handlers from running.
    pub fn fire(&self, phase: AstPhase, payload: &AstHookPayload) -> Vec<AstHookResponse> {
        let Some(hook_point) = AstHookPoint::from_phase(phase) else {
            return Vec::new();
        };

        // Forward to the underlying registry.
        let mapped = hook_point.to_hook_point();
        let metadata = serde_json::to_value(payload).unwrap_or_else(|_| serde_json::json!({}));
        let ctx = HookContext::new(mapped, hook_point.event_type(), metadata);
        if let Err(e) = self.registry.trigger(&ctx) {
            tracing::warn!(hook = %hook_point, error = %e, "[ast-hook] registry forward error");
        }

        // Fire AST-specific handlers.
        let mut responses = Vec::new();
        if let Some(handlers) = self.handlers.get(&hook_point) {
            for handler in handlers {
                match handler(payload) {
                    Ok(response) => {
                        responses.push(response);
                    }
                    Err(e) => {
                        tracing::warn!(
                            hook = %hook_point,
                            error = %e,
                            "[ast-hook] callback error"
                        );
                    }
                }
            }
        }

        responses
    }

    /// Number of AST-specific handlers registered for the given hook point.
    pub fn handler_count(&self, hook: AstHookPoint) -> usize {
        self.handlers.get(&hook).map_or(0, Vec::len)
    }

    /// Remove all AST-specific handlers for the given hook point.
    pub fn deregister(&mut self, hook: AstHookPoint) {
        self.handlers.remove(&hook);
    }

    /// Total number of AST-specific handlers across all hook points.
    pub fn total_handlers(&self) -> usize {
        self.handlers.values().map(Vec::len).sum()
    }

    /// Remove all AST-specific handlers.
    pub fn clear_all(&mut self) {
        self.handlers.clear();
    }
}

// ---------------------------------------------------------------------------
// AstPhaseController
// ---------------------------------------------------------------------------

/// Controller that wraps the AST pipeline and emits hook events after
/// each phase completes.
///
/// After each phase, the controller:
/// 1. Fires the corresponding hook via [`AstHookBridge`].
/// 2. Applies any override responses (e.g., `OverrideComplexity`).
/// 3. Converts `RequestRecovery` into a [`SupervisionDirective::Replan`].
#[derive(Debug)]
pub struct AstPhaseController {
    bridge: AstHookBridge,
}

impl Default for AstPhaseController {
    fn default() -> Self {
        Self::new()
    }
}

impl AstPhaseController {
    /// Create a new controller with a fresh bridge.
    pub fn new() -> Self {
        Self {
            bridge: AstHookBridge::new(),
        }
    }

    /// Create a controller wrapping an existing bridge.
    pub const fn with_bridge(bridge: AstHookBridge) -> Self {
        Self { bridge }
    }

    /// Access the underlying bridge for registration.
    pub const fn bridge(&self) -> &AstHookBridge {
        &self.bridge
    }

    /// Access the underlying bridge mutably for registration.
    pub const fn bridge_mut(&mut self) -> &mut AstHookBridge {
        &mut self.bridge
    }

    /// Fire hooks for the given phase and apply overrides to the payload.
    ///
    /// Returns the (possibly modified) payload and any non-`Proceed`
    /// responses that require external action.
    pub fn after_phase(
        &self,
        phase: AstPhase,
        mut payload: AstHookPayload,
    ) -> (AstHookPayload, Vec<AstHookResponse>) {
        let responses = self.bridge.fire(phase, &payload);
        let mut action_required = Vec::new();

        for response in &responses {
            match response {
                AstHookResponse::Proceed => {}
                AstHookResponse::OverrideComplexity {
                    new_complexity,
                    reason,
                } => {
                    tracing::info!(
                        phase = %phase,
                        new_complexity = ?new_complexity,
                        reason = %reason,
                        "[ast-controller] overriding complexity"
                    );
                    if let Some(ref mut assessment) = payload.assessment {
                        assessment.complexity = *new_complexity;
                        assessment.route = PhaseRoute::from(*new_complexity);
                    }
                }
                other => {
                    action_required.push(other.clone());
                }
            }
        }

        (payload, action_required)
    }

    /// Convert any `RequestRecovery` responses into a supervision directive.
    ///
    /// Returns the first recovery directive found, or `None`.
    pub fn extract_recovery_directive(
        responses: &[AstHookResponse],
    ) -> Option<SupervisionDirective> {
        for response in responses {
            if let AstHookResponse::RequestRecovery {
                milestone_id,
                ref strategy,
            } = response
            {
                return Some(SupervisionDirective::Replan {
                    reason: format!("recovery requested for milestone {milestone_id}: {strategy}"),
                });
            }
        }
        None
    }

    /// Convert any `RequestHumanReview` responses into a supervision directive.
    ///
    /// Returns the first review directive found, or `None`.
    pub fn extract_review_directive(responses: &[AstHookResponse]) -> Option<SupervisionDirective> {
        for response in responses {
            if let AstHookResponse::RequestHumanReview { ref reason } = response {
                return Some(SupervisionDirective::PauseForReview {
                    reason: reason.clone(),
                });
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Integration functions
// ---------------------------------------------------------------------------

/// Convert an AST phase and payload into a [`SupervisionEvent`].
///
/// Maps the AST lifecycle onto the supervision event stream so the
/// supervisor can react to phase completions.
pub fn ast_phase_to_supervision_event(
    phase: AstPhase,
    payload: &AstHookPayload,
) -> SupervisionEvent {
    match phase {
        AstPhase::Classify => SupervisionEvent::PhaseChanged {
            from: rustycode_protocol::ExecutionPhase::Explore,
            to: rustycode_protocol::ExecutionPhase::Plan,
        },
        AstPhase::Research => SupervisionEvent::PhaseChanged {
            from: rustycode_protocol::ExecutionPhase::Explore,
            to: rustycode_protocol::ExecutionPhase::Explore,
        },
        AstPhase::Skeleton => SupervisionEvent::PhaseChanged {
            from: rustycode_protocol::ExecutionPhase::Plan,
            to: rustycode_protocol::ExecutionPhase::Plan,
        },
        AstPhase::Expand => SupervisionEvent::PhaseChanged {
            from: rustycode_protocol::ExecutionPhase::Plan,
            to: rustycode_protocol::ExecutionPhase::Act,
        },
        AstPhase::Execute => {
            // Check if execution produced failures.
            let has_failure = payload
                .evidence
                .values()
                .flat_map(Vec::as_slice)
                .any(|e| e.exit_code != 0);
            if has_failure {
                SupervisionEvent::ToolFailed {
                    tool: "ast_execute".into(),
                    error: "one or more steps failed during execution".into(),
                }
            } else {
                SupervisionEvent::ToolFinished {
                    tool: "ast_execute".into(),
                    success: true,
                }
            }
        }
        AstPhase::Verify => {
            let quality = payload
                .report
                .as_ref()
                .map_or(0.0, |report| match report.overall {
                    VerificationStatus::Pass => 5.0,
                    VerificationStatus::Partial => 3.0,
                    VerificationStatus::Fail => 1.0,
                });
            SupervisionEvent::QualitySignal {
                score: quality,
                details: format!("verification phase completed: {phase}"),
            }
        }
        AstPhase::Complete | AstPhase::Failed => SupervisionEvent::PhaseChanged {
            from: rustycode_protocol::ExecutionPhase::Act,
            to: rustycode_protocol::ExecutionPhase::Act,
        },
    }
}

/// Convert a [`SupervisionDirective`] back into an [`AstHookResponse`], if
/// applicable.
///
/// Not all supervision directives have a meaningful AST representation.
/// Returns `None` for directives that cannot be translated.
pub fn supervision_directive_to_hook_response(
    directive: &SupervisionDirective,
) -> Option<AstHookResponse> {
    match directive {
        SupervisionDirective::Continue => Some(AstHookResponse::Proceed),
        SupervisionDirective::Replan { reason } => Some(AstHookResponse::RequestRecovery {
            milestone_id: 0,
            strategy: reason.clone(),
        }),
        SupervisionDirective::PauseForReview { reason } => {
            Some(AstHookResponse::RequestHumanReview {
                reason: reason.clone(),
            })
        }
        SupervisionDirective::ExploreAlternatives { reason, .. } => {
            Some(AstHookResponse::RequestRecovery {
                milestone_id: 0,
                strategy: format!("explore alternatives: {reason}"),
            })
        }
        SupervisionDirective::ExpandScope { reason, .. } => Some(AstHookResponse::InjectContext {
            files: Vec::new(),
            constraints: vec![reason.clone()],
        }),
        SupervisionDirective::ReviseScope { reason, .. } => {
            Some(AstHookResponse::OverrideComplexity {
                new_complexity: ComplexityLevel::Moderate,
                reason: reason.clone(),
            })
        }
        SupervisionDirective::EscalateTier { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    // -- AstHookPoint tests --

    #[test]
    fn ast_hook_point_has_exactly_6_variants() {
        assert_eq!(AstHookPoint::all().len(), 6);
    }

    #[test]
    fn ast_hook_point_event_types_are_unique() {
        let hooks = AstHookPoint::all();
        let types: Vec<&str> = hooks.iter().map(super::AstHookPoint::event_type).collect();
        let unique: std::collections::HashSet<&str> = types.iter().copied().collect();
        assert_eq!(types.len(), unique.len());
    }

    #[test]
    fn ast_hook_point_display_matches_event_type() {
        for hook in AstHookPoint::all() {
            assert_eq!(hook.to_string(), hook.event_type());
        }
    }

    #[test]
    fn ast_hook_point_serde_roundtrip() {
        for hook in AstHookPoint::all() {
            let json = serde_json::to_string(hook).unwrap();
            let decoded: AstHookPoint = serde_json::from_str(&json).unwrap();
            assert_eq!(*hook, decoded);
        }
    }

    #[test]
    fn ast_hook_point_to_hook_point_mapping() {
        assert_eq!(
            AstHookPoint::ClassifyComplete.to_hook_point(),
            HookPoint::PlanStart
        );
        assert_eq!(
            AstHookPoint::ResearchComplete.to_hook_point(),
            HookPoint::ContextSwitch
        );
        assert_eq!(
            AstHookPoint::SkeletonComplete.to_hook_point(),
            HookPoint::PlanStart
        );
        assert_eq!(
            AstHookPoint::ExpandComplete.to_hook_point(),
            HookPoint::PlanStart
        );
        assert_eq!(
            AstHookPoint::ExecuteComplete.to_hook_point(),
            HookPoint::PostToolUse
        );
        assert_eq!(
            AstHookPoint::VerifyComplete.to_hook_point(),
            HookPoint::PlanEnd
        );
    }

    #[test]
    fn ast_hook_point_from_phase_main_phases() {
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Classify),
            Some(AstHookPoint::ClassifyComplete)
        );
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Research),
            Some(AstHookPoint::ResearchComplete)
        );
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Skeleton),
            Some(AstHookPoint::SkeletonComplete)
        );
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Expand),
            Some(AstHookPoint::ExpandComplete)
        );
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Execute),
            Some(AstHookPoint::ExecuteComplete)
        );
        assert_eq!(
            AstHookPoint::from_phase(AstPhase::Verify),
            Some(AstHookPoint::VerifyComplete)
        );
    }

    #[test]
    fn ast_hook_point_from_phase_terminal_states() {
        assert_eq!(AstHookPoint::from_phase(AstPhase::Complete), None);
        assert_eq!(AstHookPoint::from_phase(AstPhase::Failed), None);
    }

    // -- AstHookPayload tests --

    #[test]
    fn ast_hook_payload_default() {
        let payload = AstHookPayload::default();
        assert!(payload.task_id.is_empty());
        assert_eq!(payload.phase, AstPhase::Classify);
        assert!(payload.assessment.is_none());
        assert!(payload.brief.is_none());
        assert!(payload.skeleton.is_none());
        assert!(payload.active_segments.is_empty());
        assert!(payload.evidence.is_empty());
        assert!(payload.report.is_none());
    }

    #[test]
    fn ast_hook_payload_serialization_roundtrip() {
        let payload = AstHookPayload {
            task_id: "t-42".into(),
            phase: AstPhase::Execute,
            assessment: Some(TaskAssessment {
                task_summary: "fix bug".into(),
                complexity: ComplexityLevel::Moderate,
                success_criteria: vec![],
                route: PhaseRoute::StandardSequence,

                clarity: None,
            }),
            brief: None,
            skeleton: None,
            active_segments: vec![],
            evidence: HashMap::new(),
            report: None,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let back: AstHookPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(back.task_id, "t-42");
        assert_eq!(back.phase, AstPhase::Execute);
        assert!(back.assessment.is_some());
    }

    // -- AstHookResponse tests --

    #[test]
    fn ast_hook_response_proceed_serialization() {
        let resp = AstHookResponse::Proceed;
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn ast_hook_response_override_complexity_serialization() {
        let resp = AstHookResponse::OverrideComplexity {
            new_complexity: ComplexityLevel::Complex,
            reason: "bigger than expected".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn ast_hook_response_inject_context_serialization() {
        let resp = AstHookResponse::InjectContext {
            files: vec!["src/main.rs".into()],
            constraints: vec!["no unsafe".into()],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn ast_hook_response_modify_milestones_serialization() {
        let resp = AstHookResponse::ModifyMilestones {
            reorder: Some(vec![2, 0, 1]),
            drop: Some(vec![3]),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn ast_hook_response_request_recovery_serialization() {
        let resp = AstHookResponse::RequestRecovery {
            milestone_id: 5,
            strategy: "retry with backoff".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    #[test]
    fn ast_hook_response_request_human_review_serialization() {
        let resp = AstHookResponse::RequestHumanReview {
            reason: "partial pass".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: AstHookResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, back);
    }

    // -- AstHookBridge tests --

    #[test]
    fn bridge_new_is_empty() {
        let bridge = AstHookBridge::new();
        assert_eq!(bridge.total_handlers(), 0);
        for hook in AstHookPoint::all() {
            assert_eq!(bridge.handler_count(*hook), 0);
        }
    }

    #[test]
    fn bridge_register_and_fire() {
        let mut bridge = AstHookBridge::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        bridge.register(AstHookPoint::ClassifyComplete, move |_| {
            called_clone.store(true, Ordering::SeqCst);
            Ok(AstHookResponse::Proceed)
        });

        assert_eq!(bridge.handler_count(AstHookPoint::ClassifyComplete), 1);

        let payload = AstHookPayload::default();
        let responses = bridge.fire(AstPhase::Classify, &payload);
        assert!(called.load(Ordering::SeqCst));
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0], AstHookResponse::Proceed);
    }

    #[test]
    fn bridge_fire_multiple_handlers_in_order() {
        let mut bridge = AstHookBridge::new();
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));

        let o1 = order.clone();
        bridge.register(AstHookPoint::ExecuteComplete, move |_| {
            o1.lock().unwrap().push(1);
            Ok(AstHookResponse::Proceed)
        });

        let o2 = order.clone();
        bridge.register(AstHookPoint::ExecuteComplete, move |_| {
            o2.lock().unwrap().push(2);
            Ok(AstHookResponse::Proceed)
        });

        let payload = AstHookPayload::default();
        bridge.fire(AstPhase::Execute, &payload);
        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn bridge_fire_unregistered_returns_empty() {
        let bridge = AstHookBridge::new();
        let payload = AstHookPayload::default();
        let responses = bridge.fire(AstPhase::Research, &payload);
        assert!(responses.is_empty());
    }

    #[test]
    fn bridge_fire_terminal_phase_returns_empty() {
        let mut bridge = AstHookBridge::new();
        bridge.register(AstHookPoint::ClassifyComplete, |_| {
            Ok(AstHookResponse::Proceed)
        });

        let payload = AstHookPayload::default();
        let responses = bridge.fire(AstPhase::Complete, &payload);
        assert!(responses.is_empty(), "Complete phase has no hook point");
    }

    #[test]
    fn bridge_fire_continues_after_handler_error() {
        let mut bridge = AstHookBridge::new();
        let second_called = Arc::new(AtomicBool::new(false));
        let second_clone = second_called.clone();

        bridge.register(AstHookPoint::ExpandComplete, |_| {
            Err(anyhow::anyhow!("first handler fails"))
        });
        bridge.register(AstHookPoint::ExpandComplete, move |_| {
            second_clone.store(true, Ordering::SeqCst);
            Ok(AstHookResponse::Proceed)
        });

        let payload = AstHookPayload::default();
        let responses = bridge.fire(AstPhase::Expand, &payload);
        assert!(second_called.load(Ordering::SeqCst));
        assert_eq!(responses.len(), 1);
    }

    #[test]
    fn bridge_deregister_removes_handlers() {
        let mut bridge = AstHookBridge::new();
        bridge.register(AstHookPoint::VerifyComplete, |_| {
            Ok(AstHookResponse::Proceed)
        });
        assert_eq!(bridge.handler_count(AstHookPoint::VerifyComplete), 1);
        bridge.deregister(AstHookPoint::VerifyComplete);
        assert_eq!(bridge.handler_count(AstHookPoint::VerifyComplete), 0);
    }

    #[test]
    fn bridge_clear_all_removes_everything() {
        let mut bridge = AstHookBridge::new();
        bridge.register(AstHookPoint::ClassifyComplete, |_| {
            Ok(AstHookResponse::Proceed)
        });
        bridge.register(AstHookPoint::ExecuteComplete, |_| {
            Ok(AstHookResponse::Proceed)
        });
        assert_eq!(bridge.total_handlers(), 2);
        bridge.clear_all();
        assert_eq!(bridge.total_handlers(), 0);
    }

    // -- AstPhaseController tests --

    #[test]
    fn controller_after_phase_returns_proceed_when_no_hooks() {
        let controller = AstPhaseController::new();
        let payload = AstHookPayload::default();
        let (returned, actions) = controller.after_phase(AstPhase::Classify, payload);
        assert!(actions.is_empty());
        assert!(returned.assessment.is_none());
    }

    #[test]
    fn controller_after_phase_applies_complexity_override() {
        let mut controller = AstPhaseController::new();
        controller
            .bridge_mut()
            .register(AstHookPoint::ClassifyComplete, |_| {
                Ok(AstHookResponse::OverrideComplexity {
                    new_complexity: ComplexityLevel::Complex,
                    reason: "bigger scope".into(),
                })
            });

        let payload = AstHookPayload {
            task_id: "t-1".into(),
            phase: AstPhase::Classify,
            assessment: Some(TaskAssessment {
                task_summary: "test".into(),
                complexity: ComplexityLevel::Trivial,
                success_criteria: vec![],
                route: PhaseRoute::DirectExecute,

                clarity: None,
            }),
            ..AstHookPayload::default()
        };

        let (returned, _actions) = controller.after_phase(AstPhase::Classify, payload);
        let assessment = returned.assessment.as_ref().unwrap();
        assert_eq!(assessment.complexity, ComplexityLevel::Complex);
        assert_eq!(assessment.route, PhaseRoute::RollingWave);
    }

    #[test]
    fn controller_after_phase_passes_through_non_override_responses() {
        let mut controller = AstPhaseController::new();
        controller
            .bridge_mut()
            .register(AstHookPoint::VerifyComplete, |_| {
                Ok(AstHookResponse::RequestHumanReview {
                    reason: "partial pass".into(),
                })
            });

        let payload = AstHookPayload {
            phase: AstPhase::Verify,
            ..AstHookPayload::default()
        };

        let (_returned, actions) = controller.after_phase(AstPhase::Verify, payload);
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            AstHookResponse::RequestHumanReview { reason } if reason == "partial pass"
        ));
    }

    #[test]
    fn controller_extract_recovery_directive() {
        let responses = vec![
            AstHookResponse::Proceed,
            AstHookResponse::RequestRecovery {
                milestone_id: 3,
                strategy: "retry".into(),
            },
        ];
        let directive = AstPhaseController::extract_recovery_directive(&responses);
        assert!(matches!(
            directive,
            Some(SupervisionDirective::Replan { reason }) if reason.contains("milestone 3")
        ));
    }

    #[test]
    fn controller_extract_recovery_directive_none_when_no_recovery() {
        let responses = vec![
            AstHookResponse::Proceed,
            AstHookResponse::InjectContext {
                files: vec![],
                constraints: vec![],
            },
        ];
        let directive = AstPhaseController::extract_recovery_directive(&responses);
        assert!(directive.is_none());
    }

    #[test]
    fn controller_extract_review_directive() {
        let responses = vec![AstHookResponse::RequestHumanReview {
            reason: "needs eyes".into(),
        }];
        let directive = AstPhaseController::extract_review_directive(&responses);
        assert!(matches!(
            directive,
            Some(SupervisionDirective::PauseForReview { reason }) if reason == "needs eyes"
        ));
    }

    #[test]
    fn controller_extract_review_directive_none_when_no_review() {
        let responses = vec![AstHookResponse::Proceed];
        let directive = AstPhaseController::extract_review_directive(&responses);
        assert!(directive.is_none());
    }

    // -- Integration function tests --

    #[test]
    fn ast_phase_to_supervision_event_classify() {
        let payload = AstHookPayload::default();
        let event = ast_phase_to_supervision_event(AstPhase::Classify, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::PhaseChanged {
                from: rustycode_protocol::ExecutionPhase::Explore,
                to: rustycode_protocol::ExecutionPhase::Plan,
            }
        ));
    }

    #[test]
    fn ast_phase_to_supervision_event_execute_success() {
        let payload = AstHookPayload {
            evidence: HashMap::new(),
            ..AstHookPayload::default()
        };
        let event = ast_phase_to_supervision_event(AstPhase::Execute, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::ToolFinished { tool, success: true } if tool == "ast_execute"
        ));
    }

    #[test]
    fn ast_phase_to_supervision_event_execute_failure() {
        let mut evidence = HashMap::new();
        evidence.insert(
            0,
            vec![StepEvidence {
                step_index: 0,
                command_run: None,
                exit_code: 1,
                stdout_summary: String::new(),
                stderr_summary: "error".into(),
                changed_files: vec![],
                verification_passed: None,
            }],
        );
        let payload = AstHookPayload {
            evidence,
            ..AstHookPayload::default()
        };
        let event = ast_phase_to_supervision_event(AstPhase::Execute, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::ToolFailed { tool, .. } if tool == "ast_execute"
        ));
    }

    #[test]
    fn ast_phase_to_supervision_event_verify_pass() {
        let payload = AstHookPayload {
            report: Some(VerificationReport {
                results: vec![],
                overall: VerificationStatus::Pass,
            }),
            ..AstHookPayload::default()
        };
        let event = ast_phase_to_supervision_event(AstPhase::Verify, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::QualitySignal { score: 5.0, .. }
        ));
    }

    #[test]
    fn ast_phase_to_supervision_event_verify_fail() {
        let payload = AstHookPayload {
            report: Some(VerificationReport {
                results: vec![],
                overall: VerificationStatus::Fail,
            }),
            ..AstHookPayload::default()
        };
        let event = ast_phase_to_supervision_event(AstPhase::Verify, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::QualitySignal { score: 1.0, .. }
        ));
    }

    #[test]
    fn ast_phase_to_supervision_event_verify_no_report() {
        let payload = AstHookPayload {
            report: None,
            ..AstHookPayload::default()
        };
        let event = ast_phase_to_supervision_event(AstPhase::Verify, &payload);
        assert!(matches!(
            event,
            SupervisionEvent::QualitySignal { score: 0.0, .. }
        ));
    }

    #[test]
    fn supervision_directive_to_hook_response_continue() {
        let directive = SupervisionDirective::Continue;
        let resp = supervision_directive_to_hook_response(&directive);
        assert_eq!(resp, Some(AstHookResponse::Proceed));
    }

    #[test]
    fn supervision_directive_to_hook_response_replan() {
        let directive = SupervisionDirective::Replan {
            reason: "stuck".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(matches!(
            resp,
            Some(AstHookResponse::RequestRecovery { milestone_id: 0, strategy }) if strategy == "stuck"
        ));
    }

    #[test]
    fn supervision_directive_to_hook_response_pause_for_review() {
        let directive = SupervisionDirective::PauseForReview {
            reason: "budget".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(matches!(
            resp,
            Some(AstHookResponse::RequestHumanReview { reason }) if reason == "budget"
        ));
    }

    #[test]
    fn supervision_directive_to_hook_response_escalate_tier_returns_none() {
        let directive = SupervisionDirective::EscalateTier {
            to_tier: 4,
            reason: "exhausted".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(resp.is_none());
    }

    #[test]
    fn supervision_directive_to_hook_response_explore_alternatives() {
        let directive = SupervisionDirective::ExploreAlternatives {
            branches: 2,
            reason: "failures".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(matches!(
            resp,
            Some(AstHookResponse::RequestRecovery { milestone_id: 0, strategy }) if strategy.contains("failures")
        ));
    }

    #[test]
    fn supervision_directive_to_hook_response_expand_scope() {
        let directive = SupervisionDirective::ExpandScope {
            allowed_tools: vec!["write".into()],
            reason: "need write access".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(matches!(
            resp,
            Some(AstHookResponse::InjectContext { files, constraints }) if constraints.contains(&"need write access".to_string()) && files.is_empty()
        ));
    }

    #[test]
    fn supervision_directive_to_hook_response_revise_scope() {
        let directive = SupervisionDirective::ReviseScope {
            reduced_goal: "auth only".into(),
            reason: "scope creep".into(),
        };
        let resp = supervision_directive_to_hook_response(&directive);
        assert!(matches!(
            resp,
            Some(AstHookResponse::OverrideComplexity { new_complexity: ComplexityLevel::Moderate, reason }) if reason == "scope creep"
        ));
    }

    #[test]
    fn bridge_forward_to_registry() {
        let mut registry = HookRegistry::new();
        let registry_called = Arc::new(AtomicBool::new(false));
        let rc = registry_called.clone();
        registry.register(HookPoint::PlanStart, move |_| {
            rc.store(true, Ordering::SeqCst);
            Ok(crate::hook_points::HookResult::Continue)
        });

        let bridge = AstHookBridge::with_registry(registry);
        let payload = AstHookPayload::default();
        bridge.fire(AstPhase::Classify, &payload);
        assert!(registry_called.load(Ordering::SeqCst));
    }
}
