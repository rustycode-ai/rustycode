use crate::Event;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::Any;

/// Event emitted when an autonomous task harness starts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarnessStartedEvent {
    pub timestamp: DateTime<Utc>,
    pub harness: String,
    pub task: String,
    pub confidence: u32, // Stored as percentage integer
}

impl Event for HarnessStartedEvent {
    fn event_type(&self) -> &'static str {
        "harness.started"
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

    #[test]
    fn harness_started_event_construction() {
        let ts = Utc::now();
        let event = HarnessStartedEvent {
            timestamp: ts,
            harness: "agent-1".to_string(),
            task: "build project".to_string(),
            confidence: 85,
        };
        assert_eq!(event.event_type(), "harness.started");
        assert_eq!(event.harness, "agent-1");
        assert_eq!(event.task, "build project");
        assert_eq!(event.confidence, 85);
    }

    #[test]
    fn harness_started_event_timestamp_matches() {
        let before = Utc::now();
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 50,
        };
        let after = Utc::now();
        assert!(event.timestamp() >= before);
        assert!(event.timestamp() <= after);
    }

    #[test]
    fn harness_started_event_as_any_downcast() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "harness-a".to_string(),
            task: "task-x".to_string(),
            confidence: 92,
        };
        let any_ref: &dyn Any = event.as_any();
        let downcast = any_ref.downcast_ref::<HarnessStartedEvent>();
        assert!(downcast.is_some());
        assert_eq!(downcast.unwrap().confidence, 92);
    }

    #[test]
    fn harness_started_event_clone_box() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 100,
        };
        let boxed: Box<dyn Event> = event.clone_box();
        assert_eq!(boxed.event_type(), "harness.started");
    }

    #[test]
    fn harness_started_event_serialize_valid_json() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "my-harness".to_string(),
            task: "my-task".to_string(),
            confidence: 75,
        };
        let val = Event::serialize(&event);
        assert!(!val.is_null());
        assert_eq!(val["harness"], "my-harness");
        assert_eq!(val["task"], "my-task");
        assert_eq!(val["confidence"], 75);
        assert!(val.get("timestamp").is_some());
    }

    #[test]
    fn harness_started_event_serde_roundtrip() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "agent-42".to_string(),
            task: "run benchmarks".to_string(),
            confidence: 99,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: HarnessStartedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn harness_started_event_equality() {
        let ts = Utc::now();
        let e1 = HarnessStartedEvent {
            timestamp: ts,
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 50,
        };
        let e2 = HarnessStartedEvent {
            timestamp: ts,
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 50,
        };
        assert_eq!(e1, e2);
    }

    #[test]
    fn harness_started_event_inequality() {
        let ts = Utc::now();
        let e1 = HarnessStartedEvent {
            timestamp: ts,
            harness: "h1".to_string(),
            task: "t".to_string(),
            confidence: 50,
        };
        let e2 = HarnessStartedEvent {
            timestamp: ts,
            harness: "h2".to_string(),
            task: "t".to_string(),
            confidence: 50,
        };
        assert_ne!(e1, e2);
    }

    #[test]
    fn harness_started_event_confidence_zero() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 0,
        };
        assert_eq!(event.confidence, 0);
        let val = Event::serialize(&event);
        assert_eq!(val["confidence"], 0);
    }

    #[test]
    fn harness_started_event_confidence_max() {
        let event = HarnessStartedEvent {
            timestamp: Utc::now(),
            harness: "h".to_string(),
            task: "t".to_string(),
            confidence: 100,
        };
        assert_eq!(event.confidence, 100);
    }
}
