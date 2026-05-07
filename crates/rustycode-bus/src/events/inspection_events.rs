// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Inspection-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Event emitted when a doctor inspection is completed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InspectionCompletedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    /// Working directory that was inspected
    pub working_dir: String,

    pub git_status: String,

    /// Number of LSP servers found
    pub lsp_server_count: usize,

    /// Number of memory entries
    pub memory_entry_count: usize,

    /// Number of skills discovered
    pub skill_count: usize,

    /// Additional detail
    pub detail: String,
}

impl InspectionCompletedEvent {
    /// Create a new `InspectionCompletedEvent`
    ///
    pub fn new(
        working_dir: String,
        git_status: String,
        lsp_server_count: usize,
        memory_entry_count: usize,
        skill_count: usize,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            working_dir,
            git_status,
            lsp_server_count,
            memory_entry_count,
            skill_count,
            detail,
        }
    }
}

impl Event for InspectionCompletedEvent {
    fn event_type(&self) -> &'static str {
        "inspection.completed"
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
    fn test_inspection_completed_event() {
        let event = InspectionCompletedEvent::new(
            "/project".into(),
            "main branch, clean".into(),
            2,
            5,
            3,
            "Done".into(),
        );
        assert_eq!(event.event_type(), "inspection.completed");
        assert_eq!(event.lsp_server_count, 2);
        assert_eq!(event.memory_entry_count, 5);
        assert_eq!(event.skill_count, 3);
    }

    #[test]
    fn test_inspection_completed_serialization() {
        let event = InspectionCompletedEvent::new(
            "/tmp".into(),
            "clean".into(),
            0,
            0,
            0,
            "no items".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("working_dir").is_some());
        assert!(json.get("lsp_server_count").is_some());
    }

    #[test]
    fn test_inspection_completed_serialization_roundtrip() {
        let event = InspectionCompletedEvent::new(
            "/project".into(),
            "dirty".into(),
            2,
            10,
            4,
            "complete".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: InspectionCompletedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.working_dir, "/project");
        assert_eq!(decoded.git_status, "dirty");
        assert_eq!(decoded.lsp_server_count, 2);
        assert_eq!(decoded.memory_entry_count, 10);
        assert_eq!(decoded.skill_count, 4);
    }
}
