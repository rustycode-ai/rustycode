// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Tool-related event types.

use crate::Event;
use chrono::{DateTime, Utc};
use rustycode_protocol::SessionId;
use serde::{Deserialize, Serialize};
use std::any::Any;
use uuid::Uuid;

/// Event emitted when a tool is executed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolExecutedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    pub tool_name: String,

    /// Tool arguments
    pub arguments: serde_json::Value,

    /// Success flag
    pub success: bool,

    /// Output text
    pub output: String,

    /// Error message (if any)
    pub error: Option<String>,
}

impl ToolExecutedEvent {
    /// Create a new `ToolExecutedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        success: bool,
        output: String,
        error: Option<String>,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            tool_name,
            arguments,
            success,
            output,
            error,
        }
    }
}

impl Event for ToolExecutedEvent {
    fn event_type(&self) -> &'static str {
        "tool.executed"
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

/// Event emitted when a tool is blocked due to planning mode restrictions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolBlockedEvent {
    /// Unique event identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<Uuid>,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    pub session_id: SessionId,

    /// Tool name that was blocked
    pub tool_name: String,

    /// Tool arguments
    pub arguments: serde_json::Value,

    /// Reason for blocking
    pub reason: String,

    /// Additional detail
    pub detail: String,
}

impl ToolBlockedEvent {
    /// Create a new `ToolBlockedEvent`
    ///
    pub fn new(
        session_id: SessionId,
        tool_name: String,
        arguments: serde_json::Value,
        reason: String,
        detail: String,
    ) -> Self {
        Self {
            event_id: Some(Uuid::new_v4()),
            timestamp: Utc::now(),
            session_id,
            tool_name,
            arguments,
            reason,
            detail,
        }
    }
}

impl Event for ToolBlockedEvent {
    fn event_type(&self) -> &'static str {
        "tool.blocked"
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
    fn test_tool_executed_event() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "read_file".to_string(),
            serde_json::json!({ "path": "/test" }),
            true,
            "success".to_string(),
            None,
        );

        assert_eq!(event.event_type(), "tool.executed");
        assert!(event.event_id.is_some());
        assert!(event.timestamp <= Utc::now());
        assert!(event.success);
        assert!(event.error.is_none());
    }

    #[test]
    fn test_tool_blocked_event() {
        let event = ToolBlockedEvent::new(
            SessionId::new(),
            "write_file".into(),
            serde_json::json!({"path": "test.txt"}),
            "Planning mode".into(),
            "Not permitted".into(),
        );
        assert_eq!(event.event_type(), "tool.blocked");
        assert_eq!(event.tool_name, "write_file");
        assert_eq!(event.reason, "Planning mode");
    }

    #[test]
    fn test_tool_executed_with_error() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"command": "false"}),
            false,
            String::new(),
            Some("exit code 1".into()),
        );
        assert!(!event.success);
        assert_eq!(event.error.as_deref(), Some("exit code 1"));
    }

    #[test]
    fn test_tool_executed_serialization_roundtrip() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"cmd": "ls"}),
            true,
            "output".into(),
            None,
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tool_name, "bash");
        assert!(decoded.success);
    }

    #[test]
    fn test_tool_blocked_serialization_roundtrip() {
        let event = ToolBlockedEvent::new(
            SessionId::new(),
            "write_file".into(),
            serde_json::json!({"path": "/tmp/test"}),
            "Planning mode".into(),
            "not allowed".into(),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolBlockedEvent = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.tool_name, "write_file");
        assert_eq!(decoded.reason, "Planning mode");
    }

    #[test]
    fn test_tool_executed_with_error_serialization_roundtrip() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "bash".into(),
            serde_json::json!({"cmd": "rm -rf /"}),
            false,
            "failed output".into(),
            Some("permission denied".into()),
        );
        let json = serde_json::to_value(&event).unwrap();
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert!(!decoded.success);
        assert_eq!(decoded.error, Some("permission denied".to_string()));
    }

    #[test]
    fn test_tool_executed_no_error_serialization() {
        let event = ToolExecutedEvent::new(
            SessionId::new(),
            "read".into(),
            serde_json::json!(null),
            true,
            "ok".into(),
            None,
        );
        let json = serde_json::to_value(&event).unwrap();
        // error field should be null (not omitted)
        assert!(json.get("error").is_some());
        let decoded: ToolExecutedEvent = serde_json::from_value(json).unwrap();
        assert!(decoded.error.is_none());
    }
}
