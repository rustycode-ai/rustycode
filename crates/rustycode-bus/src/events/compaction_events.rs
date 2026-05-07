// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Compaction-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::SessionId;
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

// Pre-compact event emitted for a session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreCompactEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    // Path where the compaction snapshot was persisted
    pub snapshot_path: String,
    // Optional detail message
    pub detail: String,
}

impl PreCompactEvent {
    pub fn new(session_id: SessionId, snapshot_path: String, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            snapshot_path,
            detail,
        }
    }
}

impl Event for PreCompactEvent {
    fn event_type(&self) -> &'static str {
        "compaction.pre"
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

// Post-compact event emitted after compaction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PostCompactEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    // Indicates whether the restore succeeded
    pub restored: bool,
    pub detail: String,
}

impl PostCompactEvent {
    pub fn new(session_id: SessionId, restored: bool, detail: String) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            restored,
            detail,
        }
    }
}

impl Event for PostCompactEvent {
    fn event_type(&self) -> &'static str {
        "compaction.post"
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
    fn test_pre_compact_event() {
        let event = PreCompactEvent::new(
            SessionId::new(),
            "/snapshots/snap-1.json".to_string(),
            "Pre-compact snapshot".to_string(),
        );
        assert_eq!(event.event_type(), "compaction.pre");
        assert_eq!(event.snapshot_path, "/snapshots/snap-1.json");
        assert!(event.event_id.is_some());
    }

    #[test]
    fn test_pre_compact_event_serde_roundtrip() {
        let event = PreCompactEvent::new(
            SessionId::new(),
            "/tmp/snap.db".to_string(),
            "saving state".to_string(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PreCompactEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.snapshot_path, "/tmp/snap.db");
        assert_eq!(decoded.detail, "saving state");
    }

    #[test]
    fn test_pre_compact_event_clone_box() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "compaction.pre");
    }

    #[test]
    fn test_pre_compact_event_as_any_downcast() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PreCompactEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().snapshot_path, "/path");
    }

    #[test]
    fn test_post_compact_event_restored() {
        let event =
            PostCompactEvent::new(SessionId::new(), true, "Restored from snapshot".to_string());
        assert_eq!(event.event_type(), "compaction.post");
        assert!(event.restored);
    }

    #[test]
    fn test_post_compact_event_not_restored() {
        let event = PostCompactEvent::new(SessionId::new(), false, "Restore failed".to_string());
        assert!(!event.restored);
    }

    #[test]
    fn test_post_compact_event_serde_roundtrip() {
        let event = PostCompactEvent::new(SessionId::new(), true, "ok".to_string());
        let json = serde_json::to_value(&event).unwrap();
        let decoded: PostCompactEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.restored);
        assert_eq!(decoded.detail, "ok");
    }

    #[test]
    fn test_post_compact_event_clone_box() {
        let event = PostCompactEvent::new(SessionId::new(), false, "detail".to_string());
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "compaction.post");
    }

    #[test]
    fn test_post_compact_event_as_any_downcast() {
        let event = PostCompactEvent::new(SessionId::new(), true, "detail".to_string());
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PostCompactEvent>();
        assert!(downcast.is_some());
        assert!(downcast.unwrap().restored);
    }

    #[test]
    fn test_pre_compact_serialization_in_trait_object_list() {
        let event =
            PreCompactEvent::new(SessionId::new(), "/path".to_string(), "detail".to_string());
        let serialized = Event::serialize(&event);
        assert!(!serialized.is_null());
        assert!(serialized.get("timestamp").is_some());
        assert_eq!(serialized["snapshot_path"], "/path");
    }

    #[test]
    fn test_post_compact_serialization_in_trait_object_list() {
        let event = PostCompactEvent::new(SessionId::new(), true, "detail".to_string());
        let serialized = Event::serialize(&event);
        assert!(!serialized.is_null());
        assert!(serialized.get("restored").is_some());
    }
}
