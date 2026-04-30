use crate::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Event emitted when an agent completes a partial result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartialResultEvent {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub step_id: String,
    pub content: String,
}

impl Event for PartialResultEvent {
    fn event_type(&self) -> &'static str {
        "ensemble.partial_result"
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

/// Event emitted when an agent objects to a partial result
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectionEvent {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub reason: String,
}

impl Event for ObjectionEvent {
    fn event_type(&self) -> &'static str {
        "ensemble.objection"
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

/// Event emitted to signal escalation to the Conductor
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EscalationSignal {
    pub timestamp: DateTime<Utc>,
    pub task_id: String,
    pub level: String,
}

impl Event for EscalationSignal {
    fn event_type(&self) -> &'static str {
        "ensemble.escalation"
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
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;
    use crate::Event;
    use std::any::Any;

    // ── PartialResultEvent tests ─────────────

    #[test]
    fn partial_result_event_construction() {
        let ts = Utc::now();
        let event = PartialResultEvent {
            timestamp: ts,
            task_id: "task-1".to_string(),
            step_id: "step-1".to_string(),
            content: "partial output".to_string(),
        };
        assert_eq!(event.event_type(), "ensemble.partial_result");
        assert_eq!(event.task_id, "task-1");
        assert_eq!(event.step_id, "step-1");
        assert_eq!(event.content, "partial output");
    }

    #[test]
    fn partial_result_event_timestamp() {
        let before = Utc::now();
        let event = PartialResultEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: String::new(),
        };
        let after = Utc::now();
        assert!(event.timestamp() >= before);
        assert!(event.timestamp() <= after);
    }

    #[test]
    fn partial_result_event_as_any_downcast() {
        let event = PartialResultEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: "data".to_string(),
        };
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<PartialResultEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().content, "data");
    }

    #[test]
    fn partial_result_event_clone_box() {
        let event = PartialResultEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: "cloned".to_string(),
        };
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "ensemble.partial_result");
    }

    #[test]
    fn partial_result_event_serialize() {
        let event = PartialResultEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: "json".to_string(),
        };
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert_eq!(val["task_id"], "t");
        assert_eq!(val["step_id"], "s");
        assert_eq!(val["content"], "json");
    }

    #[test]
    fn partial_result_event_serde_roundtrip() {
        let event = PartialResultEvent {
            timestamp: Utc::now(),
            task_id: "task-42".to_string(),
            step_id: "step-7".to_string(),
            content: "result text".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: PartialResultEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn partial_result_event_equality() {
        let ts = Utc::now();
        let e1 = PartialResultEvent {
            timestamp: ts,
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: "c".to_string(),
        };
        let e2 = PartialResultEvent {
            timestamp: ts,
            task_id: "t".to_string(),
            step_id: "s".to_string(),
            content: "c".to_string(),
        };
        assert_eq!(e1, e2);
    }

    #[test]
    fn partial_result_event_inequality() {
        let ts = Utc::now();
        let e1 = PartialResultEvent {
            timestamp: ts,
            task_id: "t1".to_string(),
            step_id: "s".to_string(),
            content: "c".to_string(),
        };
        let e2 = PartialResultEvent {
            timestamp: ts,
            task_id: "t2".to_string(),
            step_id: "s".to_string(),
            content: "c".to_string(),
        };
        assert_ne!(e1, e2);
    }

    // ── ObjectionEvent tests ─────────────

    #[test]
    fn objection_event_construction() {
        let ts = Utc::now();
        let event = ObjectionEvent {
            timestamp: ts,
            task_id: "task-1".to_string(),
            reason: "inconsistent result".to_string(),
        };
        assert_eq!(event.event_type(), "ensemble.objection");
        assert_eq!(event.task_id, "task-1");
        assert_eq!(event.reason, "inconsistent result");
    }

    #[test]
    fn objection_event_as_any_downcast() {
        let event = ObjectionEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            reason: "bad data".to_string(),
        };
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<ObjectionEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().reason, "bad data");
    }

    #[test]
    fn objection_event_clone_box() {
        let event = ObjectionEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            reason: "r".to_string(),
        };
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "ensemble.objection");
    }

    #[test]
    fn objection_event_serde_roundtrip() {
        let event = ObjectionEvent {
            timestamp: Utc::now(),
            task_id: "task-99".to_string(),
            reason: "disagrees with partial".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: ObjectionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn objection_event_serialize_produces_valid_json() {
        let event = ObjectionEvent {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            reason: "test".to_string(),
        };
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert!(val.get("timestamp").is_some());
        assert_eq!(val["reason"], "test");
    }

    #[test]
    fn objection_event_equality_same_timestamp() {
        let ts = Utc::now();
        let e1 = ObjectionEvent {
            timestamp: ts,
            task_id: "t".to_string(),
            reason: "r".to_string(),
        };
        let e2 = ObjectionEvent {
            timestamp: ts,
            task_id: "t".to_string(),
            reason: "r".to_string(),
        };
        assert_eq!(e1, e2);
    }

    // ── EscalationSignal tests ─────────────

    #[test]
    fn escalation_signal_construction() {
        let ts = Utc::now();
        let event = EscalationSignal {
            timestamp: ts,
            task_id: "task-1".to_string(),
            level: "critical".to_string(),
        };
        assert_eq!(event.event_type(), "ensemble.escalation");
        assert_eq!(event.task_id, "task-1");
        assert_eq!(event.level, "critical");
    }

    #[test]
    fn escalation_signal_as_any_downcast() {
        let event = EscalationSignal {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            level: "warning".to_string(),
        };
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<EscalationSignal>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().level, "warning");
    }

    #[test]
    fn escalation_signal_clone_box() {
        let event = EscalationSignal {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            level: "info".to_string(),
        };
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "ensemble.escalation");
    }

    #[test]
    fn escalation_signal_serde_roundtrip() {
        let event = EscalationSignal {
            timestamp: Utc::now(),
            task_id: "task-5".to_string(),
            level: "high".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: EscalationSignal = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn escalation_signal_serialize_produces_valid_json() {
        let event = EscalationSignal {
            timestamp: Utc::now(),
            task_id: "t".to_string(),
            level: "critical".to_string(),
        };
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert_eq!(val["level"], "critical");
        assert!(val.get("timestamp").is_some());
    }

    #[test]
    fn escalation_signal_equality() {
        let ts = Utc::now();
        let e1 = EscalationSignal {
            timestamp: ts,
            task_id: "t".to_string(),
            level: "l".to_string(),
        };
        let e2 = EscalationSignal {
            timestamp: ts,
            task_id: "t".to_string(),
            level: "l".to_string(),
        };
        assert_eq!(e1, e2);
    }
}
