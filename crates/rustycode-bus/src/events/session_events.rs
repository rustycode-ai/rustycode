// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Session-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::{ContextPlan, SessionId};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Event emitted when a new session is started
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionStartedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task description
    pub task: String,

    /// Additional detail
    pub detail: String,
}

impl SessionStartedEvent {
    /// Create a new `SessionStartedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            detail,
        }
    }
}

impl Event for SessionStartedEvent {
    fn event_type(&self) -> &'static str {
        "session.started"
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

/// Event emitted when context is assembled for a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextAssembledEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub context_plan: ContextPlan,

    /// Additional detail
    pub detail: String,
}

impl ContextAssembledEvent {
    /// Create a new `ContextAssembledEvent`
    ///
    pub fn new(session_id: SessionId, context_plan: ContextPlan, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            context_plan,
            detail,
        }
    }
}

impl Event for ContextAssembledEvent {
    fn event_type(&self) -> &'static str {
        "context.assembled"
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

/// Event emitted when a session is completed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task that was completed
    pub task: String,

    /// Final session status
    pub status: String,

    /// Additional detail
    pub detail: String,
}

impl SessionCompletedEvent {
    /// Create a new `SessionCompletedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, status: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            status,
            detail,
        }
    }
}

impl Event for SessionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "session.completed"
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

/// Event emitted when a session fails
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionFailedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Task that failed
    pub task: String,

    /// Error message
    pub error: String,

    /// Additional detail
    pub detail: String,
}

impl SessionFailedEvent {
    /// Create a new `SessionFailedEvent`
    ///
    pub fn new(session_id: SessionId, task: String, error: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            task,
            error,
            detail,
        }
    }
}

impl Event for SessionFailedEvent {
    fn event_type(&self) -> &'static str {
        "session.failed"
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

/// Event emitted when session mode changes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeChangedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Previous mode
    pub old_mode: String,

    pub new_mode: String,

    /// Additional detail
    pub detail: String,
}

impl ModeChangedEvent {
    /// Create a new `ModeChangedEvent`
    ///
    pub fn new(session_id: SessionId, old_mode: String, new_mode: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            old_mode,
            new_mode,
            detail,
        }
    }
}

impl Event for ModeChangedEvent {
    fn event_type(&self) -> &'static str {
        "mode.changed"
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

    #[test]
    fn test_session_started_event() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        assert_eq!(event.event_type(), "session.started");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
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

    #[test]
    fn test_event_serialization() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let serialized = Event::serialize(&event);
        assert!(serialized.is_object());
        assert!(serialized.get("timestamp").is_some());
        assert!(serialized.get("task").is_some());
    }

    #[test]
    fn test_event_downcast() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let as_any: &dyn Any = event.as_any();
        assert!(as_any.is::<SessionStartedEvent>());

        let downcasted = as_any.downcast_ref::<SessionStartedEvent>();
        assert!(downcasted.is_some());
        assert_eq!(downcasted.unwrap().task, "test task");
    }

    #[test]
    fn test_event_clone_box() {
        let event = SessionStartedEvent::new(
            SessionId::new(),
            "test task".to_string(),
            "test detail".to_string(),
        );

        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "session.started");

        let as_any: &dyn Any = boxed.as_any();
        assert!(as_any.is::<SessionStartedEvent>());
    }

    #[test]
    fn test_session_completed_event() {
        let event = SessionCompletedEvent::new(
            SessionId::new(),
            "Analyze codebase".into(),
            "completed".into(),
            "Done".into(),
        );
        assert_eq!(event.event_type(), "session.completed");
        assert_eq!(event.status, "completed");
        assert!(event.event_id.is_some());
    }

    #[test]
    fn test_session_failed_event() {
        let event = SessionFailedEvent::new(
            SessionId::new(),
            "Build project".into(),
            "Network error".into(),
            "Connection refused".into(),
        );
        assert_eq!(event.event_type(), "session.failed");
        assert_eq!(event.error, "Network error");
    }

    #[test]
    fn test_mode_changed_event() {
        let event = ModeChangedEvent::new(
            SessionId::new(),
            "chat".into(),
            "planning".into(),
            "User requested planning".into(),
        );
        assert_eq!(event.event_type(), "mode.changed");
        assert_eq!(event.old_mode, "chat");
        assert_eq!(event.new_mode, "planning");
    }

    #[test]
    fn test_session_started_serialization_roundtrip() {
        let event = SessionStartedEvent::new(SessionId::new(), "test".into(), "detail".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionStartedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "test");
        assert_eq!(decoded.detail, "detail");
    }

    #[test]
    fn test_session_completed_serialization_roundtrip() {
        let event = SessionCompletedEvent::new(
            SessionId::new(),
            "task".into(),
            "completed".into(),
            "done".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "task");
        assert_eq!(decoded.status, "completed");
        assert_eq!(decoded.detail, "done");
    }

    #[test]
    fn test_session_failed_serialization_roundtrip() {
        let event = SessionFailedEvent::new(
            SessionId::new(),
            "task".into(),
            "err msg".into(),
            "detail".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: SessionFailedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.task, "task");
        assert_eq!(decoded.error, "err msg");
    }

    #[test]
    fn test_mode_changed_serialization_roundtrip() {
        let event = ModeChangedEvent::new(
            SessionId::new(),
            "chat".into(),
            "code".into(),
            "switched".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ModeChangedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.old_mode, "chat");
        assert_eq!(decoded.new_mode, "code");
    }

    #[test]
    fn test_session_started_event_id_is_some() {
        let event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        assert!(event.event_id.is_some());
        // Verify it roundtrips with Some uuid
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("event_id").unwrap().is_string());
    }

    #[test]
    fn test_session_started_event_id_none_serialization() {
        // Construct event with event_id = None manually
        let mut event = SessionStartedEvent::new(SessionId::new(), "task".into(), "detail".into());
        event.event_id = None;
        let json = serde_json::to_value(&event).unwrap();
        // skip_serializing_if should omit event_id
        assert!(json.get("event_id").is_none());
        // Should still deserialize correctly
        let decoded: SessionStartedEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.event_id.is_none());
    }

    #[test]
    fn test_session_started_equality() {
        let sid = SessionId::new();
        let e1 = SessionStartedEvent::new(sid, "task".into(), "detail".into());
        let mut e2 = e1.clone();
        // Force same event_id for equality check
        e2.event_id = e1.event_id;
        e2.timestamp = e1.timestamp;
        assert_eq!(e1, e2);
    }

    #[test]
    fn test_session_started_inequality() {
        let e1 = SessionStartedEvent::new(SessionId::new(), "task1".into(), "detail".into());
        let e2 = SessionStartedEvent::new(SessionId::new(), "task2".into(), "detail".into());
        assert_ne!(e1, e2);
    }

    #[test]
    fn test_context_assembled_serialization_roundtrip() {
        let context_plan = ContextPlan {
            total_budget: 200000,
            reserved_budget: 150000,
            sections: vec![],
        };
        let event =
            ContextAssembledEvent::new(SessionId::new(), context_plan.clone(), "ready".into());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ContextAssembledEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.context_plan, context_plan);
        assert_eq!(decoded.detail, "ready");
    }
}
