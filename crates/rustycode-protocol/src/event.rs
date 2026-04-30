//! Event types for RustyCode
//!
//! Events track significant state changes and actions throughout the session lifecycle,
//! providing an audit trail and enabling observability.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::SessionId;

/// Types of events that can occur during a session.
///
/// Events track significant state changes and actions throughout the session lifecycle,
/// providing an audit trail and enabling observability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EventKind {
    // Session lifecycle
    /// Session was started
    SessionStarted,
    /// Session completed successfully
    SessionCompleted,
    /// Session failed with an error
    SessionFailed,
    // Context
    /// Context was assembled for the LLM
    ContextAssembled,
    /// Codebase inspection completed
    InspectionCompleted,
    // Tool execution
    /// A tool was executed
    ToolExecuted,
    /// Tool was blocked due to planning mode restrictions
    ToolBlockedInPlanningMode,
    // Plan lifecycle
    /// A plan was created
    PlanCreated,
    /// Plan was approved by the user
    PlanApproved,
    /// Plan was rejected by the user
    PlanRejected,
    /// Plan execution started
    PlanExecutionStarted,
    /// Plan execution completed
    PlanExecutionCompleted,
    /// Plan execution failed
    PlanExecutionFailed,
    /// Other event types with custom name
    Other(String),
}

/// An event that occurred during a session.
///
/// Events provide a chronological log of significant actions and state changes,
/// useful for debugging, auditing, and understanding session flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    /// The session this event belongs to
    pub session_id: SessionId,
    /// When the event occurred
    pub at: DateTime<Utc>,
    /// The type of event
    pub kind: EventKind,
    /// Human-readable details about the event
    pub detail: String,
}

impl SessionEvent {
    /// Create a new session event
    pub fn new(session_id: SessionId, kind: EventKind, detail: impl Into<String>) -> Self {
        Self {
            session_id,
            at: Utc::now(),
            kind,
            detail: detail.into(),
        }
    }

    /// Create a session started event
    pub fn session_started(session_id: SessionId) -> Self {
        Self::new(session_id, EventKind::SessionStarted, "Session started")
    }

    /// Create a session completed event
    pub fn session_completed(session_id: SessionId) -> Self {
        Self::new(session_id, EventKind::SessionCompleted, "Session completed")
    }

    /// Create a session failed event
    pub fn session_failed(session_id: SessionId, error: impl Into<String>) -> Self {
        Self::new(session_id, EventKind::SessionFailed, error)
    }

    /// Create a tool executed event
    pub fn tool_executed(session_id: SessionId, tool_name: impl Into<String>) -> Self {
        Self::new(
            session_id,
            EventKind::ToolExecuted,
            format!("Tool {} executed", tool_name.into()),
        )
    }

    /// Create a plan created event
    pub fn plan_created(session_id: SessionId) -> Self {
        Self::new(session_id, EventKind::PlanCreated, "Plan created")
    }

    /// Create a plan approved event
    pub fn plan_approved(session_id: SessionId) -> Self {
        Self::new(session_id, EventKind::PlanApproved, "Plan approved")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_event_creation() {
        let session_id = SessionId::new();
        let event = SessionEvent::session_started(session_id.clone());

        assert_eq!(event.session_id, session_id);
        assert!(matches!(event.kind, EventKind::SessionStarted));
    }

    #[test]
    fn test_tool_executed_event() {
        let session_id = SessionId::new();
        let event = SessionEvent::tool_executed(session_id, "read_file");

        assert!(matches!(event.kind, EventKind::ToolExecuted));
        assert!(event.detail.contains("read_file"));
    }

    #[test]
    fn test_plan_events() {
        let session_id = SessionId::new();

        let created = SessionEvent::plan_created(session_id.clone());
        assert!(matches!(created.kind, EventKind::PlanCreated));

        let approved = SessionEvent::plan_approved(session_id);
        assert!(matches!(approved.kind, EventKind::PlanApproved));
    }

    #[test]
    fn test_session_completed_event() {
        let session_id = SessionId::new();
        let event = SessionEvent::session_completed(session_id.clone());
        assert_eq!(event.session_id, session_id);
        assert!(matches!(event.kind, EventKind::SessionCompleted));
        assert_eq!(event.detail, "Session completed");
    }

    #[test]
    fn test_session_failed_event() {
        let session_id = SessionId::new();
        let event = SessionEvent::session_failed(session_id.clone(), "OOM");
        assert!(matches!(event.kind, EventKind::SessionFailed));
        assert_eq!(event.detail, "OOM");
    }

    #[test]
    fn test_event_kind_serialization() {
        let kinds = vec![
            EventKind::SessionStarted,
            EventKind::SessionCompleted,
            EventKind::SessionFailed,
            EventKind::ContextAssembled,
            EventKind::InspectionCompleted,
            EventKind::ToolExecuted,
            EventKind::ToolBlockedInPlanningMode,
            EventKind::PlanCreated,
            EventKind::PlanApproved,
            EventKind::PlanRejected,
            EventKind::PlanExecutionStarted,
            EventKind::PlanExecutionCompleted,
            EventKind::PlanExecutionFailed,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let decoded: EventKind = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, kind);
        }
    }

    #[test]
    fn test_event_kind_other() {
        let kind = EventKind::Other("custom_event".to_string());
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("custom_event"));
    }

    #[test]
    fn test_event_kind_equality() {
        assert_eq!(EventKind::SessionStarted, EventKind::SessionStarted);
        assert_ne!(EventKind::SessionStarted, EventKind::SessionCompleted);
        assert_eq!(
            EventKind::Other("x".to_string()),
            EventKind::Other("x".to_string())
        );
    }

    #[test]
    fn test_session_event_has_timestamp() {
        let event = SessionEvent::session_started(SessionId::new());
        // Timestamp should be near now
        let diff = Utc::now().signed_duration_since(event.at);
        assert!(diff.num_seconds() <= 1);
    }

    #[test]
    fn test_session_event_custom_detail() {
        let event = SessionEvent::new(
            SessionId::new(),
            EventKind::ContextAssembled,
            "Assembled 42 tokens",
        );
        assert_eq!(event.detail, "Assembled 42 tokens");
    }

    #[test]
    fn test_session_event_roundtrip() {
        let event = SessionEvent::session_started(SessionId::new());
        let json = serde_json::to_string(&event).unwrap();
        let decoded: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, event.session_id);
        assert_eq!(decoded.detail, event.detail);
    }

    #[test]
    fn test_session_event_tool_executed_detail() {
        let event = SessionEvent::tool_executed(SessionId::new(), "bash");
        assert_eq!(event.detail, "Tool bash executed");
    }

    #[test]
    fn test_session_event_snake_case_rename() {
        let kind = EventKind::ToolBlockedInPlanningMode;
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("tool_blocked_in_planning_mode"));
    }
}
