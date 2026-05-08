// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Event types for the `RustyCode` event bus
//!
//! This module defines the core event types used throughout `RustyCode`.
//! Event types are organized into domain-specific sub-modules.

pub mod ast_events;
pub mod compaction_events;
pub mod inspection_events;
pub mod plan_events;
pub mod session_events;
pub mod skill_events;
pub mod todo_events;
pub mod tool_events;

// Re-exports for backward compatibility — all types remain accessible
// from `rustycode_bus::events::TypeName` and `rustycode_bus::TypeName`.
pub use ast_events::AstPhaseEvent;
pub use compaction_events::{PostCompactEvent, PreCompactEvent};
pub use inspection_events::InspectionCompletedEvent;
pub use plan_events::{
    MilestoneProgressEvent, PlanApprovedEvent, PlanCreatedEvent, PlanExecutionCompletedEvent,
    PlanExecutionFailedEvent, PlanExecutionStartedEvent, PlanRejectedEvent,
};
pub use session_events::{
    ContextAssembledEvent, ModeChangedEvent, SessionCompletedEvent, SessionFailedEvent,
    SessionStartedEvent,
};
pub use skill_events::{
    SkillActivatedEvent, SkillDeactivatedEvent, SkillQualityAssessedEvent, SkillSuggestedEvent,
};
pub use todo_events::{TodoSnapshot, TodoUpdatedEvent};
pub use tool_events::{ToolBlockedEvent, ToolExecutedEvent};

/// Cross-cutting test: verifies that all event types produce valid JSON via
/// the `Event::serialize` trait method. Kept here because it exercises types
/// from every sub-module.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Event;
    use chrono::Utc;
    use rustycode_protocol::{ContextPlan, Plan, PlanId, PlanStatus, SessionId};

    #[test]
    fn test_event_serialize_produces_valid_json() {
        let events: Vec<Box<dyn Event>> = vec![
            Box::new(SessionStartedEvent::new(
                SessionId::new(),
                "t".into(),
                "d".into(),
            )),
            Box::new(SessionCompletedEvent::new(
                SessionId::new(),
                "t".into(),
                "ok".into(),
                "d".into(),
            )),
            Box::new(SessionFailedEvent::new(
                SessionId::new(),
                "t".into(),
                "err".into(),
                "d".into(),
            )),
            Box::new(ModeChangedEvent::new(
                SessionId::new(),
                "a".into(),
                "b".into(),
                "d".into(),
            )),
            Box::new(ToolExecutedEvent::new(
                SessionId::new(),
                "tool".into(),
                serde_json::json!({}),
                true,
                "out".into(),
                None,
            )),
            Box::new(ToolBlockedEvent::new(
                SessionId::new(),
                "tool".into(),
                serde_json::json!({}),
                "reason".into(),
                "d".into(),
            )),
            Box::new(PlanApprovedEvent::new(SessionId::new(), "d".into())),
            Box::new(PlanRejectedEvent::new(SessionId::new(), "d".into())),
            Box::new(InspectionCompletedEvent::new(
                "/tmp".into(),
                "clean".into(),
                0,
                0,
                0,
                "d".into(),
            )),
            Box::new(SkillActivatedEvent::new("test".into(), "auto".into())),
            Box::new(SkillDeactivatedEvent::new(
                "test".into(),
                "done".into(),
                10.0,
                100,
            )),
            Box::new(SkillSuggestedEvent::new(
                "test".into(),
                "match".into(),
                0.8,
                vec![],
            )),
            Box::new(SkillQualityAssessedEvent::new(
                "test".into(),
                "good".into(),
                0.9,
                0.7,
                0.8,
                0.6,
            )),
        ];

        for event in &events {
            let serialized = event.serialize();
            assert!(
                !serialized.is_null(),
                "serialize() returned Null for {}",
                event.event_type()
            );
            assert!(
                serialized.get("timestamp").is_some(),
                "missing timestamp for {}",
                event.event_type()
            );
        }
    }

    #[test]
    fn test_context_assembled_event() {
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        };

        let event =
            ContextAssembledEvent::new(SessionId::new(), context_plan, "context ready".to_string());

        assert_eq!(event.event_type(), "context.assembled");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
    }

    // Helper for plan-related tests
    fn make_plan() -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "test".to_string(),
            created_at: Utc::now(),
            status: PlanStatus::Draft,
            summary: "test plan".to_string(),
            approach: String::new(),
            steps: vec![],
            files_to_modify: vec![],
            risks: vec![],
            current_step_index: None,
            execution_started_at: None,
            execution_completed_at: None,
            execution_error: None,
            task_profile: None,
            milestone_id: None,
        }
    }

    #[test]
    fn test_plan_created_event_serialize_valid_json() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "created".to_string());
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert!(val.get("timestamp").is_some());
        assert_eq!(val["detail"], "created");
    }
}
