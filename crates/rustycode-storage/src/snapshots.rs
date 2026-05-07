//! Session snapshot storage methods.
//!
//! Contains `SessionSnapshot` struct and `impl Storage` methods for
//! persisting, loading, and deleting session snapshots.

use std::collections::HashMap;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::params;
use rustycode_protocol::{PlanId, SessionId};

use crate::Storage;

/// A snapshot of session state for persistence across restarts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSnapshot {
    pub session_id: SessionId,
    pub captured_at: DateTime<Utc>,
    pub conversation_json: String,
    pub active_plan_id: Option<PlanId>,
    pub metadata: HashMap<String, String>,
}

impl Storage {
    /// Persist a session snapshot, replacing any existing snapshot for this session.
    pub fn save_snapshot(&self, snapshot: &SessionSnapshot) -> Result<()> {
        let json =
            serde_json::to_string(snapshot).context("failed to serialize session snapshot")?;
        let captured_at = snapshot.captured_at.to_rfc3339();
        let session_id_str = snapshot.session_id.to_string();
        let conn = &mut *self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert or replace into session_snapshots (session_id, captured_at, snapshot_json) values (?1, ?2, ?3)",
            params![session_id_str, captured_at, json],
        ).context("failed to save session snapshot")?;

        // Also update FTS index (best-effort, non-fatal)
        let fts_content = json;
        if let Err(e) = conn.execute(
            "INSERT OR REPLACE INTO conversation_fts(session_id, content) VALUES (?1, ?2)",
            params![session_id_str, fts_content],
        ) {
            tracing::debug!("Failed to update FTS index: {}", e);
        }

        Ok(())
    }

    /// Load the most recent snapshot for a session, or None if not found.
    pub fn load_snapshot(&self, session_id: &SessionId) -> Result<Option<SessionSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select snapshot_json from session_snapshots where session_id = ?1")
            .context("failed to prepare load_snapshot")?;
        let mut rows = stmt
            .query(params![session_id.to_string()])
            .context("failed to query session snapshot")?;
        if let Some(row) = rows.next().context("failed to read snapshot row")? {
            let json: String = row.get(0).context("failed to get snapshot_json")?;
            let snapshot: SessionSnapshot =
                serde_json::from_str(&json).context("failed to deserialize session snapshot")?;
            Ok(Some(snapshot))
        } else {
            Ok(None)
        }
    }

    /// Delete all snapshots for a session.
    pub fn delete_snapshots(&self, session_id: &SessionId) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "delete from session_snapshots where session_id = ?1",
                params![session_id.to_string()],
            )
            .context("failed to delete session snapshots")?;
        Ok(())
    }

    /// List all session IDs that have stored snapshots.
    pub fn list_snapshot_sessions(&self) -> Result<Vec<SessionId>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select session_id from session_snapshots order by captured_at desc")
            .context("failed to prepare list_snapshot_sessions")?;
        let ids = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(id)
            })
            .context("failed to query snapshot session ids")?
            .collect::<rusqlite::Result<Vec<String>>>()
            .context("failed to collect snapshot session ids")?;
        ids.into_iter()
            .map(|s| SessionId::parse(&s).map_err(|e| anyhow::anyhow!("invalid session id: {e}")))
            .collect()
    }
}
