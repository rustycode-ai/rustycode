// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Todo-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Serializable snapshot of a single todo item, carried by [`TodoUpdatedEvent`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoSnapshot {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
}

impl TodoSnapshot {
    pub fn new(id: impl Into<String>, title: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status: status.into(),
            active_form: None,
        }
    }
}

/// Event emitted when the todo list is updated.
///
/// Carries a snapshot of all current todo items so subscribers (e.g. the TUI)
/// can update their view without needing to read the `TodoState` mutex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoUpdatedEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub todos: Vec<TodoSnapshot>,
}

impl TodoUpdatedEvent {
    pub fn new(session_id: String, todos: Vec<TodoSnapshot>) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            todos,
        }
    }
}

impl Event for TodoUpdatedEvent {
    fn event_type(&self) -> &'static str {
        "todo.updated"
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

    #[test]
    fn test_event_type() {
        let event = TodoUpdatedEvent::new("sess-1".into(), vec![]);
        assert_eq!(event.event_type(), "todo.updated");
    }

    #[test]
    fn test_event_has_id_and_timestamp() {
        let event = TodoUpdatedEvent::new("sess-1".into(), vec![]);
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
    }

    #[test]
    fn test_event_carries_todos() {
        let todos = vec![
            TodoSnapshot::new("1", "Task A", "pending"),
            TodoSnapshot {
                id: "2".into(),
                title: "Task B".into(),
                status: "in_progress".into(),
                active_form: Some("Working on B".into()),
            },
        ];
        let event = TodoUpdatedEvent::new("sess-1".into(), todos);
        assert_eq!(event.todos.len(), 2);
        assert_eq!(event.todos[0].id, "1");
        assert_eq!(event.todos[1].status, "in_progress");
    }

    #[test]
    fn test_serialize_valid_json() {
        let event = TodoUpdatedEvent::new("sess-1".into(), vec![]);
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert!(val.get("timestamp").is_some());
        assert_eq!(val["session_id"], "sess-1");
    }

    #[test]
    fn test_downcast() {
        let event = TodoUpdatedEvent::new("sess-1".into(), vec![]);
        let as_any: &dyn Any = event.as_any();
        assert!(as_any.is::<TodoUpdatedEvent>());
    }

    #[test]
    fn test_clone_box() {
        let event = TodoUpdatedEvent::new("sess-1".into(), vec![]);
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "todo.updated");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let todos = vec![TodoSnapshot::new("1", "Task", "completed")];
        let event = TodoUpdatedEvent::new("sess-42".into(), todos);
        let json = serde_json::to_value(&event).unwrap();
        let decoded: TodoUpdatedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.session_id, "sess-42");
        assert_eq!(decoded.todos.len(), 1);
        assert_eq!(decoded.todos[0].id, "1");
    }
}
