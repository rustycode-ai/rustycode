//! Event store for persisting and replaying `SyncEvent`s.
//!
//! The `EventStore` provides durable storage for `SyncEvent`s produced by
//! the agent system, and supports replay to reconstruct state from events.
//!
//! ## Design
//!
//! Events are persisted to SQLite with the following schema:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS sync_events (
//!     id TEXT PRIMARY KEY,                    -- EventId
//!     session_id TEXT NOT NULL,               -- SessionId
//!     sequence INTEGER NOT NULL,              -- per-session sequence number
//!     at TEXT NOT NULL,                       -- RFC3339 timestamp
//!     kind TEXT NOT NULL,                     -- StreamEvent variant
//!     payload TEXT NOT NULL,                  -- JSON serialized StreamEvent
//!     UNIQUE(session_id, sequence)
//! );
//! ```
//!
//! ### Replay Strategy
//!
//! To reconstruct state from events:
//! 1. Load all non-delta events for a session (or all events)
//! 2. Sort by sequence number
//! 3. Feed each event through an `AgentEvents` handler
//! 4. The handler rebuilds internal state incrementally
//!
//! Delta events (TextDelta, ThinkingDelta, TokenUsage) can be skipped during
//! recovery since they don't affect final state.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};
use tracing::instrument;

use rustycode_protocol::{
    EventId, SessionId, StreamEvent, SyncEvent,
};

/// Event store for persisting and replaying `SyncEvent`s.
#[derive(Debug, Clone)]
pub struct EventStore {
    conn: Arc<StdMutex<Connection>>,
}

/// Configuration for loading events from the store.
#[derive(Debug, Clone, Default)]
pub struct EventLoadConfig {
    /// If true, include delta events (TextDelta, ThinkingDelta, TokenUsage,
    /// ToolInputDelta) in the result. These are typically skipped during
    /// replay as they don\'t affect final state.
    pub include_deltas: bool,
    /// Load events starting from this sequence number (inclusive).
    pub from_sequence: Option<u64>,
    /// Load events up to this sequence number (inclusive).
    pub to_sequence: Option<u64>,
    /// Maximum number of events to load.
    pub limit: Option<usize>,
}

impl EventStore {
    /// Open or create an event store at the given database path.
    ///
    /// This is a low-level constructor. In most cases, use `Storage::event_store()`.
    /// The store must be created within a `Storage` instance to share the same
    /// database connection pool.
    ///
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        let store = Self {
            conn: Arc::new(StdMutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Create an event store from an existing connection.
    ///
    /// This is the primary constructor when used within a `Storage` instance.
    pub(crate) fn from_connection(conn: Arc<StdMutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Run database migrations for the event store.
    #[instrument(skip(self))]
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sync_events (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                at TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL,
                UNIQUE(session_id, sequence)
            );
            CREATE INDEX IF NOT EXISTS idx_sync_events_session 
                ON sync_events(session_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_sync_events_at 
                ON sync_events(at);
            ",
        )?;
        Ok(())
    }

    /// Persist a `SyncEvent` to the store.
    ///
    #[instrument(skip(self, event), fields(id = %event.id, session_id = %event.session_id, seq = event.sequence))]
    pub fn insert_event(&self, event: &SyncEvent) -> Result<()> {
        let payload = serde_json::to_string(&event.payload)
            .context("failed to serialize event payload")?;
        let kind = event_kind(&event.payload);

        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO sync_events (id, session_id, sequence, at, kind, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id, sequence) DO UPDATE SET
                id = excluded.id,
                at = excluded.at,
                kind = excluded.kind,
                payload = excluded.payload",
            params![
                event.id.to_string(),
                event.session_id.to_string(),
                event.sequence as i64,
                event.at.to_rfc3339(),
                kind,
                payload,
            ],
        )?;
        Ok(())
    }

    /// Persist multiple events in a single transaction.
    ///
    /// This is more efficient than calling `insert_event` in a loop.
    #[instrument(skip(self, events), fields(count = events.len()))]
    pub fn insert_events(&self, events: &[SyncEvent]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;

        for event in events {
            let payload = serde_json::to_string(&event.payload)
                .context("failed to serialize event payload")?;
            let kind = event_kind(&event.payload);

            tx.execute(
                "INSERT INTO sync_events (id, session_id, sequence, at, kind, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(session_id, sequence) DO UPDATE SET
                    id = excluded.id,
                    at = excluded.at,
                    kind = excluded.kind,
                    payload = excluded.payload",
                params![
                    event.id.to_string(),
                    event.session_id.to_string(),
                    event.sequence as i64,
                    event.at.to_rfc3339(),
                    kind,
                    payload,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Load events for a specific session.
    ///
    /// Events are returned in sequence order (ascending).
    #[instrument(skip(self), fields(session_id = %session_id))]
    pub fn events_for_session(
        &self,
        session_id: &SessionId,
        config: Option<EventLoadConfig>,
    ) -> Result<Vec<SyncEvent>> {
        let config = config.unwrap_or_default();
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        let mut query = "SELECT id, session_id, sequence, at, kind, payload FROM sync_events WHERE session_id = ?1".to_string();
        let mut param_idx = 2u32;
        let mut from_val: Option<i64> = None;
        let mut to_val: Option<i64> = None;
        let mut limit_val: Option<i64> = None;

        if !config.include_deltas {
            query.push_str(" AND kind NOT IN ('text_delta', 'thinking_delta', 'token_usage', 'tool_input_delta')");
        }

        if let Some(from) = config.from_sequence {
            query.push_str(&format!(" AND sequence >= ?{param_idx}"));
            from_val = Some(from as i64);
            param_idx += 1;
        }
        if let Some(to) = config.to_sequence {
            query.push_str(&format!(" AND sequence <= ?{param_idx}"));
            to_val = Some(to as i64);
            param_idx += 1;
        }

        query.push_str(" ORDER BY sequence ASC");

        if let Some(limit) = config.limit {
            query.push_str(&format!(" LIMIT ?{param_idx}"));
            limit_val = Some(limit as i64);
        }

        let mut stmt = conn.prepare(&query)?;
        let params: Vec<Box<dyn rusqlite::types::ToSql>> = std::iter::once(Box::new(session_id.to_string()) as Box<dyn rusqlite::types::ToSql>)
            .chain(from_val.map(|v| Box::new(v) as Box<dyn rusqlite::types::ToSql>))
            .chain(to_val.map(|v| Box::new(v) as Box<dyn rusqlite::types::ToSql>))
            .chain(limit_val.map(|v| Box::new(v) as Box<dyn rusqlite::types::ToSql>))
            .collect();
        let rows = stmt.query_map(params.as_slice(), row_to_sync_event)?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    /// Get the latest sequence number for a session.
    ///
    /// Returns `Ok(None)` if no events exist for the session.
    #[instrument(skip(self), fields(session_id = %session_id))]
    pub fn latest_sequence(&self, session_id: &SessionId) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let seq: Option<i64> = conn.query_row(
            "SELECT MAX(sequence) FROM sync_events WHERE session_id = ?1",
            params![session_id.to_string()],
            |row| row.get(0),
        ).optional()?;
        Ok(seq.map(|s| s as u64))
    }

    /// Get the last (highest sequence) event for a session.
    #[instrument(skip(self), fields(session_id = %session_id))]
    pub fn last_event(&self, session_id: &SessionId) -> Result<Option<SyncEvent>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, session_id, sequence, at, kind, payload 
             FROM sync_events 
             WHERE session_id = ?1 
             ORDER BY sequence DESC 
             LIMIT 1",
        )?;
        let event = stmt.query_row(params![session_id.to_string()], row_to_sync_event).optional()?;
        Ok(event)
    }

    /// Count events for a session.
    #[instrument(skip(self), fields(session_id = %session_id))]
    pub fn event_count(&self, session_id: &SessionId) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sync_events WHERE session_id = ?1",
            params![session_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// List all session IDs that have events in the store.
    #[instrument(skip(self))]
    pub fn list_sessions(&self) -> Result<Vec<SessionId>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT DISTINCT session_id FROM sync_events ORDER BY MIN(sequence)",
        )?;
        let rows = stmt.query_map([], |row| {
            let s: String = row.get(0)?;
            Ok(SessionId::parse(&s).unwrap_or_else(|_| {
                tracing::warn!(session_id = s, "Corrupted session ID in database, generating new one");
                SessionId::new()
            }))
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    /// Delete all events for a session.
    #[instrument(skip(self), fields(session_id = %session_id))]
    pub fn clear_session_events(&self, session_id: &SessionId) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM sync_events WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        Ok(())
    }


    /// Replay events for a session, feeding each through the given callback.
    pub fn replay(
        &self,
        session_id: &SessionId,
        mut handler: impl FnMut(&StreamEvent),
    ) -> Result<()> {
        let events = self.events_for_session(session_id, Some(EventLoadConfig {
            include_deltas: true,
            ..Default::default()
        }))?;

        for event in &events {
            handler(&event.payload);
        }

        Ok(())
    }

    /// Delete all events older than the given timestamp.
    #[instrument(skip(self))]
    pub fn prune_events_before(&self, cutoff: DateTime<Utc>) -> Result<usize> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let deleted = conn.execute(
            "DELETE FROM sync_events WHERE at < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        Ok(deleted)
    }
}

/// Convert a row from the sync_events table to a `SyncEvent`.
fn row_to_sync_event(row: &Row) -> Result<SyncEvent, rusqlite::Error> {
    let id: String = row.get(0)?;
    let session_id: String = row.get(1)?;
    let sequence: i64 = row.get(2)?;
    let at: String = row.get(3)?;
    let _kind: String = row.get(4)?; // kept for potential future use
    let payload_json: String = row.get(5)?;

    let payload: StreamEvent = serde_json::from_str(&payload_json)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;

    let at_dt = DateTime::parse_from_rfc3339(&at)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e)))?
        .with_timezone(&Utc);

    Ok(SyncEvent {
        id: EventId::parse(&id).unwrap_or_else(|_| {
            tracing::warn!(id, "Corrupted event ID in database, generating new one");
            EventId::new()
        }),
        session_id: SessionId::parse(&session_id).unwrap_or_else(|_| {
            tracing::warn!(session_id, "Corrupted session ID in database, generating new one");
            SessionId::new()
        }),
        sequence: sequence as u64,
        at: at_dt,
        payload,
    })
}

/// Get a string identifier for a stream event kind.
fn event_kind(event: &StreamEvent) -> &'static str {
    match event {
        StreamEvent::TextDelta { .. } => "text_delta",
        StreamEvent::ThinkingDelta { .. } => "thinking_delta",
        StreamEvent::ToolCallStarted { .. } => "tool_call_started",
        StreamEvent::ToolInputDelta { .. } => "tool_input_delta",
        StreamEvent::ToolExecStarted { .. } => "tool_exec_started",
        StreamEvent::ToolExecCompleted { .. } => "tool_exec_completed",
        StreamEvent::TurnStarted { .. } => "turn_started",
        StreamEvent::TokenUsage { .. } => "token_usage",
        StreamEvent::Done => "done",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn test_store() -> (EventStore, NamedTempFile) {
        let tmp = NamedTempFile::new().unwrap();
        let store = EventStore::open(tmp.path()).unwrap();
        (store, tmp)
    }
}

    #[test]
    fn test_insert_duplicate_sequence_overwrites() {
        let (store, _tmp) = test_store();
        let session_id = SessionId::new();

        let event1 = SyncEvent::new(
            session_id.clone(),
            1,
            StreamEvent::TextDelta { content: "First".into() },
        );
        store.insert_event(&event1).unwrap();

        let event2 = SyncEvent::new(
            session_id.clone(),
            1,
            StreamEvent::TextDelta { content: "Second".into() },
        );
        store.insert_event(&event2).unwrap();

        let events = store.events_for_session(&session_id, None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].payload,
            StreamEvent::TextDelta { content: "Second".into() }
        );
    }

    #[test]
    fn test_latest_sequence() {
        let (store, _tmp) = test_store();
        let session_id = SessionId::new();

        assert_eq!(store.latest_sequence(&session_id).unwrap(), None);

        let ev1 = SyncEvent::new(session_id.clone(), 1, StreamEvent::TextDelta { content: "a".into() });
        let ev2 = SyncEvent::new(session_id.clone(), 2, StreamEvent::TextDelta { content: "b".into() });
        store.insert_event(&ev1).unwrap();
        store.insert_event(&ev2).unwrap();

        assert_eq!(store.latest_sequence(&session_id).unwrap(), Some(2));
    }

    #[test]
    fn test_list_sessions() {
        let (store, _tmp) = test_store();
        let sid1 = SessionId::new();
        let sid2 = SessionId::new();

        let ev1 = SyncEvent::new(sid1.clone(), 1, StreamEvent::TextDelta { content: "a".into() });
        let ev2 = SyncEvent::new(sid2.clone(), 1, StreamEvent::TextDelta { content: "b".into() });
        store.insert_event(&ev1).unwrap();
        store.insert_event(&ev2).unwrap();

        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.contains(&sid1));
        assert!(sessions.contains(&sid2));
    }

    #[test]
    fn test_clear_session_events() {
        let (store, _tmp) = test_store();
        let sid = SessionId::new();

        let ev = SyncEvent::new(sid.clone(), 1, StreamEvent::TextDelta { content: "Hi".into() });
        store.insert_event(&ev).unwrap();
        assert_eq!(store.event_count(&sid).unwrap(), 1);

        store.clear_session_events(&sid).unwrap();
        assert_eq!(store.event_count(&sid).unwrap(), 0);
    }

    #[test]
    fn test_prune_events_before() {
        let (store, _tmp) = test_store();
        let sid = SessionId::new();

        let ev1 = SyncEvent::new(sid.clone(), 1, StreamEvent::TextDelta { content: "old".into() });
        let ev2 = SyncEvent::new(sid.clone(), 2, StreamEvent::TextDelta { content: "new".into() });
        store.insert_event(&ev1).unwrap();
        // Make sure ev2 has a later timestamp
        std::thread::sleep(std::time::Duration::from_millis(10));
        store.insert_event(&ev2).unwrap();

        let cutoff = chrono::Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let ev3 = SyncEvent::new(sid.clone(), 3, StreamEvent::TextDelta { content: "newer".into() });
        store.insert_event(&ev3).unwrap();

        let deleted = store.prune_events_before(cutoff).unwrap();
        assert!(deleted >= 2); // ev1 and ev2 should be deleted

        let remaining = store.events_for_session(&sid, None).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].sequence, 3);
    }

    #[test]
    fn test_event_count() {
        let (store, _tmp) = test_store();
        let sid = SessionId::new();

        assert_eq!(store.event_count(&sid).unwrap(), 0);

        let ev = SyncEvent::new(sid.clone(), 1, StreamEvent::TextDelta { content: "x".into() });
        store.insert_event(&ev).unwrap();
        assert_eq!(store.event_count(&sid).unwrap(), 1);

        let ev2 = SyncEvent::new(sid.clone(), 2, StreamEvent::TextDelta { content: "y".into() });
        store.insert_event(&ev2).unwrap();
        assert_eq!(store.event_count(&sid).unwrap(), 2);
    }

    #[test]
    fn test_last_event() {
        let (store, _tmp) = test_store();
        let sid = SessionId::new();

        assert!(store.last_event(&sid).unwrap().is_none());

        let ev1 = SyncEvent::new(sid.clone(), 1, StreamEvent::TextDelta { content: "first".into() });
        store.insert_event(&ev1).unwrap();
        let last = store.last_event(&sid).unwrap().unwrap();
        assert_eq!(last.sequence, 1);

        let ev2 = SyncEvent::new(sid.clone(), 2, StreamEvent::TextDelta { content: "second".into() });
        store.insert_event(&ev2).unwrap();
        let last = store.last_event(&sid).unwrap().unwrap();
        assert_eq!(last.sequence, 2);
    }

    #[test]
    fn test_replay_with_callback() {
        let (store, _tmp) = test_store();
        let sid = SessionId::new();

        let events = vec![
            SyncEvent::new(sid.clone(), 1, StreamEvent::TextDelta { content: "Hello".into() }),
            SyncEvent::new(sid.clone(), 2, StreamEvent::ToolCallStarted { id: "t1".into(), name: "cmd".into() }),
            SyncEvent::new(sid.clone(), 3, StreamEvent::Done),
        ];
        store.insert_events(&events).unwrap();

        let mut received: Vec<String> = Vec::new();
        store.replay(&sid, |event| {
            if let StreamEvent::TextDelta { content } = event {
                received.push(content.clone());
            }
        }).unwrap();

        assert_eq!(received, vec!["Hello"]);
    }
}
