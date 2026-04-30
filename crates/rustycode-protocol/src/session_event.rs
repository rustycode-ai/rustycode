//! Persisted, enriched session events.
//!
//! `SyncEvent` wraps a raw `StreamEvent` and enriches it with:
//! - `session_id`: which session produced it
//! - `sequence`: monotonically-increasing number (for ordering, replay)
//! - `at`: UTC timestamp (when the event was observed by the processor)
//!
//! These events are the ones written to the `SyncEventStore` (SQLite) and
//! replayed for crash recovery or state reconstruction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EventId, SessionId, StreamEvent};

/// A persistent, enriched event recorded to the event store.
///
/// Every `SyncEvent` corresponds to exactly one `StreamEvent` from the agent,
/// but with additional metadata that makes it suitable for durable storage
/// and later replay.
///
/// Sequence numbers begin at 1 and increment by 1 for each event in a session.
/// They provide total ordering even when timestamps are identical.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncEvent {
    /// Unique identifier for this event (stable for deduplication).
    pub id: EventId,
    /// Session this event belongs to.
    pub session_id: SessionId,
    /// Monotonically-increasing sequence number (per-session).
    pub sequence: u64,
    /// When the processor observed this event.
    pub at: DateTime<Utc>,
    /// The raw agent event (unchanged).
    pub payload: StreamEvent,
}

impl SyncEvent {
    /// Create a new `SyncEvent` from a raw stream event.
    ///
    /// The processor assigns the next sequence number and current timestamp.
    pub fn new(session_id: SessionId, sequence: u64, payload: StreamEvent) -> Self {
        Self {
            id: EventId::new(),
            session_id,
            sequence,
            at: Utc::now(),
            payload,
        }
    }

    /// Whether this event represents a terminal state for its session.
    ///
    /// `Done` is the only terminal event. After it appears, no further
    /// events for that session should be emitted.
    pub fn is_terminal(&self) -> bool {
        matches!(&self.payload, StreamEvent::Done)
    }

    /// Whether this event is a delta (interim data) rather than a boundary.
    ///
    /// Deltas are not critical for crash recovery — if they're lost,
    /// the session can resume from the last boundary event.
    pub fn is_delta(&self) -> bool {
        matches!(
            &self.payload,
            StreamEvent::TextDelta { .. }
                | StreamEvent::ThinkingDelta { .. }
                | StreamEvent::ToolInputDelta { .. }
                | StreamEvent::TokenUsage { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionId;

    #[test]
    fn test_sync_event_creation() {
        let session_id = SessionId::new();
        let ev = StreamEvent::TextDelta {
            content: "hi".into(),
        };
        let sync = SyncEvent::new(session_id.clone(), 1, ev);

        assert_eq!(sync.session_id, session_id);
        assert_eq!(sync.sequence, 1);
        assert!(sync.id.to_string().starts_with("evt_"));
        assert!(!sync.is_terminal());
        assert!(sync.is_delta());
    }

    #[test]
    fn test_terminal_events() {
        let session_id = SessionId::new();
        let done = SyncEvent::new(session_id.clone(), 1, StreamEvent::Done);
        assert!(done.is_terminal());
        // Error variant not implemented yet — future extension
    }

    #[test]
    fn test_sequence_ordering() {
        let session_id = SessionId::new();
        let ev1 = SyncEvent::new(
            session_id.clone(),
            1,
            StreamEvent::ToolCallStarted {
                id: "t1".into(),
                name: "bash".into(),
            },
        );
        let ev2 = SyncEvent::new(
            session_id.clone(),
            2,
            StreamEvent::ToolExecCompleted {
                id: "t1".into(),
                name: "bash".into(),
                output: "ok".into(),
                is_error: false,
            },
        );
        let ev3 = SyncEvent::new(session_id, 3, StreamEvent::Done);

        assert!(ev1.sequence < ev2.sequence);
        assert!(ev2.sequence < ev3.sequence);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let session_id = SessionId::new();
        let ev = StreamEvent::ToolExecCompleted {
            id: "t1".into(),
            name: "bash".into(),
            output: "ok".into(),
            is_error: false,
        };
        let sync = SyncEvent::new(session_id, 42, ev);

        let json = serde_json::to_string(&sync).unwrap();
        let decoded: SyncEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, sync);
    }
}
