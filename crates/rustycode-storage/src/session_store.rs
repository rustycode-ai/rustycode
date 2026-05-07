//! Session and event storage methods.
//!
//! Contains `impl Storage` methods for opening the database, and CRUD
//! operations on sessions and events.

use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rustycode_bus::Event;
use rustycode_protocol::{EventKind, Session, SessionEvent, SessionId};

use crate::records::EventRecord;
use crate::search::session_from_row;
use crate::Storage;

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        let storage = Self {
            conn: Arc::new(StdMutex::new(conn)),
        };
        storage.migrate()?;
        // Best-effort FTS index creation (non-fatal if FTS5 not available)
        if let Err(e) = storage.ensure_fts_index() {
            tracing::debug!(
                "FTS5 index not available, search will use LIKE fallback: {}",
                e
            );
        }
        Ok(storage)
    }

    // Event persistence is now primarily handled via the EventSubscriber struct
    // which provides more robust lifecycle management and graceful shutdown.

    // -- Sessions -----------------------------------------------------------------

    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.conn.lock().unwrap_or_else(std::sync::PoisonError::into_inner).execute(
            "insert into sessions (id, task, created_at, mode, status, plan_path) values (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session.id.to_string(),
                session.task,
                session.created_at.to_rfc3339(),
                serde_json::to_string(&session.mode)?,
                serde_json::to_string(&session.status)?,
                session.plan_path,
            ],
        )?;
        Ok(())
    }

    pub fn update_session(&self, session: &Session) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "update sessions set mode = ?1, status = ?2, plan_path = ?3 where id = ?4",
                params![
                    serde_json::to_string(&session.mode)?,
                    serde_json::to_string(&session.status)?,
                    session.plan_path,
                    session.id.to_string(),
                ],
            )?;
        Ok(())
    }

    pub fn load_session(&self, session_id: &SessionId) -> Result<Option<Session>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "select id, task, created_at, mode, status, plan_path from sessions where id = ?1",
        )?;
        let mut rows = stmt.query_map(params![session_id.to_string()], session_from_row)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_event(&self, event: &SessionEvent) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert into events (session_id, at, kind, detail) values (?1, ?2, ?3, ?4)",
                params![
                    event.session_id.to_string(),
                    event.at.to_rfc3339(),
                    serde_json::to_string(&event.kind)?,
                    event.detail
                ],
            )?;
        Ok(())
    }

    /// Insert an event from the event bus into the events table
    ///
    /// This method persists any event that implements the Event trait, storing
    /// its type, serialized data, and timestamp.
    ///
    pub fn insert_event_bus(&self, event: &dyn Event) -> Result<()> {
        let serialized = event.serialize();
        let event_data =
            serde_json::to_string(&serialized).context("failed to serialize event data")?;
        let created_at = event.timestamp().to_rfc3339();
        let event_type = event.event_type();

        // Extract session_id from event data if available
        let session_id = serialized
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        self.conn
            .lock()
            .map_err(|e| anyhow::anyhow!("database lock poisoned: {e}"))?
            .execute(
                "INSERT INTO events (session_id, at, kind, detail) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, created_at, event_type, event_data],
            )?;
        Ok(())
    }

    /// Retrieve recent events from the events table
    ///
    /// Returns events ordered by creation time (most recent first).
    ///
    pub fn events(&self, limit: usize) -> Result<Vec<EventRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt =
            conn.prepare("SELECT id, kind, detail, at FROM events ORDER BY at DESC LIMIT ?1")?;

        let rows = stmt.query_map([limit as i64], |row| {
            Ok(EventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                event_data: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn session_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_row("select count(*) from sessions", [], |row| row.get(0))?)
    }

    pub fn event_count_for_session(&self, session_id: &str) -> Result<i64> {
        Ok(self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_row(
                "select count(*) from events where session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )?)
    }

    pub fn recent_tasks(
        &self,
        limit: usize,
        exclude_session_id: Option<&str>,
    ) -> Result<Vec<String>> {
        let limit = i64::try_from(limit)?;
        let mut values = Vec::new();
        if let Some(session_id) = exclude_session_id {
            let conn = self
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut statement = conn.prepare(
                "select task from sessions where id != ?1 order by created_at desc limit ?2",
            )?;
            let tasks =
                statement.query_map(params![session_id, limit], |row| row.get::<_, String>(0))?;
            for task in tasks {
                values.push(task?);
            }
        } else {
            let conn = self
                .conn
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut statement =
                conn.prepare("select task from sessions order by created_at desc limit ?1")?;
            let tasks = statement.query_map(params![limit], |row| row.get::<_, String>(0))?;
            for task in tasks {
                values.push(task?);
            }
        }
        Ok(values)
    }

    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let limit = i64::try_from(limit)?;
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut statement = conn.prepare(
            "select id, task, created_at, mode, status, plan_path from sessions order by created_at desc limit ?1",
        )?;
        let rows = statement.query_map(params![limit], session_from_row)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)
    }

    pub fn session_events(&self, session_id: &SessionId) -> Result<Vec<SessionEvent>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut statement = conn.prepare(
            "select at, kind, detail from events where session_id = ?1 order by at asc, id asc",
        )?;
        let rows = statement.query_map(params![session_id.to_string()], |row| {
            let at: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let detail: String = row.get(2)?;
            Ok((at, kind, detail))
        })?;

        let mut events = Vec::new();
        for row in rows {
            let (at, kind, detail) = row?;
            events.push(SessionEvent {
                session_id: session_id.clone(),
                at: DateTime::parse_from_rfc3339(&at)?.with_timezone(&Utc),
                kind: serde_json::from_str::<EventKind>(&kind)?,
                detail,
            });
        }
        Ok(events)
    }
}
