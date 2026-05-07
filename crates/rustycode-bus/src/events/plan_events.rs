// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Plan-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::{MilestoneId, MilestoneStatus, Plan, PlanId, SessionId};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Event emitted when a plan is created
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanCreatedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Plan details
    pub plan: Plan,

    /// Additional detail
    pub detail: String,
}

impl PlanCreatedEvent {
    pub fn new(session_id: SessionId, plan: Plan, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan,
            detail,
        }
    }
}

impl Event for PlanCreatedEvent {
    fn event_type(&self) -> &'static str {
        "plan.created"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when a plan is approved
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanApprovedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Additional detail
    pub detail: String,
}

impl PlanApprovedEvent {
    pub fn new(session_id: SessionId, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            detail,
        }
    }
}

impl Event for PlanApprovedEvent {
    fn event_type(&self) -> &'static str {
        "plan.approved"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when a plan is rejected
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRejectedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Additional detail
    pub detail: String,
}

impl PlanRejectedEvent {
    pub fn new(session_id: SessionId, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            detail,
        }
    }
}

impl Event for PlanRejectedEvent {
    fn event_type(&self) -> &'static str {
        "plan.rejected"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when plan execution starts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionStartedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Number of steps to execute
    pub step_count: usize,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionStartedEvent {
    /// Create a new `PlanExecutionStartedEvent`
    ///
    pub fn new(session_id: SessionId, plan_id: PlanId, step_count: usize, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            step_count,
            detail,
        }
    }
}

impl Event for PlanExecutionStartedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.started"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when plan execution completes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Number of steps executed
    pub steps_executed: usize,

    /// Number of steps that succeeded
    pub steps_succeeded: usize,

    /// Number of steps that failed
    pub steps_failed: usize,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionCompletedEvent {
    /// Create a new `PlanExecutionCompletedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        plan_id: PlanId,
        steps_executed: usize,
        steps_succeeded: usize,
        steps_failed: usize,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            steps_executed,
            steps_succeeded,
            steps_failed,
            detail,
        }
    }
}

impl Event for PlanExecutionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.completed"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when plan execution fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanExecutionFailedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub plan_id: PlanId,

    /// Error message
    pub error: String,

    /// Step index where failure occurred
    pub failed_at_step: Option<usize>,

    /// Additional detail
    pub detail: String,
}

impl PlanExecutionFailedEvent {
    /// Create a new `PlanExecutionFailedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        plan_id: PlanId,
        error: String,
        failed_at_step: Option<usize>,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            plan_id,
            error,
            failed_at_step,
            detail,
        }
    }
}

impl Event for PlanExecutionFailedEvent {
    fn event_type(&self) -> &'static str {
        "plan.execution.failed"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Event emitted when a milestone changes execution progress.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneProgressEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub milestone_id: MilestoneId,

    pub milestone_title: String,

    pub status: MilestoneStatus,

    pub plans_total: usize,

    pub plans_completed: usize,

    pub current_plan_summary: String,

    pub action_hint: String,
}

impl MilestoneProgressEvent {
    /// Create a new `MilestoneProgressEvent`.
    pub fn new(
        session_id: SessionId,
        milestone_id: MilestoneId,
        milestone_title: String,
        status: MilestoneStatus,
        plans_total: usize,
        plans_completed: usize,
        current_plan_summary: String,
        action_hint: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            milestone_id,
            milestone_title,
            status,
            plans_total,
            plans_completed,
            current_plan_summary,
            action_hint,
        }
    }
}

impl Event for MilestoneProgressEvent {
    fn event_type(&self) -> &'static str {
        "milestone.progress"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }

    fn serialize(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

    fn make_plan() -> Plan {
        Plan {
            id: PlanId::new(),
            session_id: SessionId::new(),
            task: "test".to_string(),
            created_at: Utc::now(),
            status: rustycode_protocol::PlanStatus::Draft,
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
    fn test_plan_approved_event() {
        let event = PlanApprovedEvent::new(SessionId::new(), "User approved".into());
        assert_eq!(event.event_type(), "plan.approved");
    }

    #[test]
    fn test_plan_rejected_event() {
        let event = PlanRejectedEvent::new(SessionId::new(), "User rejected".into());
        assert_eq!(event.event_type(), "plan.rejected");
    }

    #[test]
    fn test_plan_execution_started_event() {
        let event = PlanExecutionStartedEvent::new(
            SessionId::new(),
            PlanId::new(),
            5,
            "Starting execution".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.started");
        assert_eq!(event.step_count, 5);
    }

    #[test]
    fn test_plan_execution_completed_event() {
        let event = PlanExecutionCompletedEvent::new(
            SessionId::new(),
            PlanId::new(),
            5,
            4,
            1,
            "Done with errors".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.completed");
        assert_eq!(event.steps_executed, 5);
        assert_eq!(event.steps_succeeded, 4);
        assert_eq!(event.steps_failed, 1);
    }

    #[test]
    fn test_plan_execution_failed_event() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "Tool timeout".into(),
            Some(3),
            "Step 3 timed out".into(),
        );
        assert_eq!(event.event_type(), "plan.execution.failed");
        assert_eq!(event.failed_at_step, Some(3));
    }

    #[test]
    fn test_plan_approved_serialization_roundtrip() {
        let event = PlanApprovedEvent::new(SessionId::new(), "approved".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanApprovedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.detail, "approved");
    }

    #[test]
    fn test_plan_rejected_serialization_roundtrip() {
        let event = PlanRejectedEvent::new(SessionId::new(), "rejected".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanRejectedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.detail, "rejected");
    }

    #[test]
    fn test_plan_execution_started_serialization_roundtrip() {
        let event =
            PlanExecutionStartedEvent::new(SessionId::new(), PlanId::new(), 10, "starting".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionStartedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.step_count, 10);
        assert_eq!(decoded.detail, "starting");
    }

    #[test]
    fn test_plan_execution_completed_serialization_roundtrip() {
        let event = PlanExecutionCompletedEvent::new(
            SessionId::new(),
            PlanId::new(),
            8,
            7,
            1,
            "done".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.steps_executed, 8);
        assert_eq!(decoded.steps_succeeded, 7);
        assert_eq!(decoded.steps_failed, 1);
    }

    #[test]
    fn test_plan_execution_failed_serialization_roundtrip() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "timeout".into(),
            Some(5),
            "step 5 failed".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.error, "timeout");
        assert_eq!(decoded.failed_at_step, Some(5));
    }

    #[test]
    fn test_plan_execution_failed_no_step_serialization_roundtrip() {
        let event = PlanExecutionFailedEvent::new(
            SessionId::new(),
            PlanId::new(),
            "unknown".into(),
            None,
            "no step info".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PlanExecutionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.failed_at_step, None);
    }

    #[test]
    fn test_plan_created_event() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "New plan created".to_string());
        assert_eq!(event.event_type(), "plan.created");
        assert!(event.event_id.is_some());
        assert_eq!(event.detail, "New plan created");
    }

    #[test]
    fn test_plan_created_event_clone_box() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "d".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "plan.created");
    }

    #[test]
    fn test_plan_created_event_as_any_downcast() {
        let plan = make_plan();
        let event = PlanCreatedEvent::new(SessionId::new(), plan, "d".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PlanCreatedEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().detail, "d");
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
