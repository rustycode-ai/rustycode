//! Checkpoint, rewind snapshot, hook execution, and API call record storage.
//!
//! Contains `impl Storage` methods for CRUD operations on checkpoint records,
//! rewind snapshots, hook execution records, and API call records. Also
//! includes conversation search and session cost methods.

use anyhow::{Context, Result};
use rusqlite::params;

use crate::records::{ApiCallRecord, CheckpointRecord, HookExecutionRecord, RewindSnapshot};
use crate::search::ConversationSearchHit;
use crate::Storage;

impl Storage {
    // -- Checkpoint Store ---------------------------------------------------------

    /// Save a checkpoint record.
    pub fn save_checkpoint(&self, rec: &CheckpointRecord) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "insert or replace into checkpoints (id, session_id, label, commit_sha, files_json, created_at) values (?1, ?2, ?3, ?4, ?5, ?6)",
                params![rec.id, rec.session_id, rec.label, rec.commit_sha, rec.files_json, rec.created_at],
            )
            .context("failed to save checkpoint")?;
        Ok(())
    }

    /// Load a checkpoint by ID.
    pub fn load_checkpoint(&self, id: &str) -> Result<Option<CheckpointRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, label, commit_sha, files_json, created_at from checkpoints where id = ?1")
            .context("failed to prepare load_checkpoint")?;
        let mut rows = stmt
            .query(params![id])
            .context("failed to query checkpoint")?;
        if let Some(row) = rows.next().context("failed to read checkpoint row")? {
            Ok(Some(CheckpointRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                label: row.get(2)?,
                commit_sha: row.get(3)?,
                files_json: row.get(4)?,
                created_at: row.get(5)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Delete a checkpoint by ID.
    pub fn delete_checkpoint(&self, id: &str) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let changed = conn
            .execute("delete from checkpoints where id = ?1", params![id])
            .context("failed to delete checkpoint")?;
        Ok(changed)
    }

    /// List checkpoints for a session, newest first.
    pub fn list_checkpoints(&self, session_id: &str) -> Result<Vec<CheckpointRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, label, commit_sha, files_json, created_at from checkpoints where session_id = ?1 order by created_at desc")
            .context("failed to prepare list_checkpoints")?;
        let records = stmt
            .query_map(params![session_id], |row| {
                Ok(CheckpointRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    label: row.get(2)?,
                    commit_sha: row.get(3)?,
                    files_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .context("failed to query checkpoints")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect checkpoints")?;
        Ok(records)
    }

    // -- Rewind Snapshot Store ----------------------------------------------------

    /// Save a rewind snapshot.
    pub fn save_rewind_snapshot(&self, snap: &RewindSnapshot) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert into rewind_snapshots (session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![snap.session_id, snap.interaction_number, snap.role, snap.content_preview, snap.tools_used_json, snap.checkpoint_id, snap.captured_at],
        )
        .context("failed to save rewind snapshot")?;
        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// Load a rewind snapshot by interaction number within a session.
    pub fn load_rewind_snapshot(
        &self,
        session_id: &str,
        interaction_number: i64,
    ) -> Result<Option<RewindSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at from rewind_snapshots where session_id = ?1 and interaction_number = ?2")
            .context("failed to prepare load_rewind_snapshot")?;
        let mut rows = stmt
            .query(params![session_id, interaction_number])
            .context("failed to query rewind snapshot")?;
        if let Some(row) = rows.next().context("failed to read rewind row")? {
            Ok(Some(RewindSnapshot {
                id: row.get(0)?,
                session_id: row.get(1)?,
                interaction_number: row.get(2)?,
                role: row.get(3)?,
                content_preview: row.get(4)?,
                tools_used_json: row.get(5)?,
                checkpoint_id: row.get(6)?,
                captured_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// List all rewind snapshots for a session, ordered by interaction number.
    pub fn list_rewind_snapshots(&self, session_id: &str) -> Result<Vec<RewindSnapshot>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, interaction_number, role, content_preview, tools_used_json, checkpoint_id, captured_at from rewind_snapshots where session_id = ?1 order by interaction_number asc")
            .context("failed to prepare list_rewind_snapshots")?;
        let snaps = stmt
            .query_map(params![session_id], |row| {
                Ok(RewindSnapshot {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    interaction_number: row.get(2)?,
                    role: row.get(3)?,
                    content_preview: row.get(4)?,
                    tools_used_json: row.get(5)?,
                    checkpoint_id: row.get(6)?,
                    captured_at: row.get(7)?,
                })
            })
            .context("failed to query rewind snapshots")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect rewind snapshots")?;
        Ok(snaps)
    }

    // -- Hook Execution Store -----------------------------------------------------

    /// Save a hook execution record.
    pub fn save_hook_execution(&self, rec: &HookExecutionRecord) -> Result<i64> {
        let blocked_int = i32::from(rec.blocked);
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert into hook_executions (session_id, trigger_type, hook_name, command, status, stdout, stderr, exit_code, blocked, duration_ms, executed_at) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![rec.session_id, rec.trigger_type, rec.hook_name, rec.command, rec.status, rec.stdout, rec.stderr, rec.exit_code, blocked_int, rec.duration_ms, rec.executed_at],
        )
        .context("failed to save hook execution")?;
        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// List recent hook executions for a session.
    pub fn list_hook_executions(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<HookExecutionRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, trigger_type, hook_name, command, status, stdout, stderr, exit_code, blocked, duration_ms, executed_at from hook_executions where session_id = ?1 order by executed_at desc limit ?2")
            .context("failed to prepare list_hook_executions")?;
        let records = stmt
            .query_map(params![session_id, limit as i64], |row| {
                let blocked_int: i32 = row.get(9)?;
                Ok(HookExecutionRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    trigger_type: row.get(2)?,
                    hook_name: row.get(3)?,
                    command: row.get(4)?,
                    status: row.get(5)?,
                    stdout: row.get(6)?,
                    stderr: row.get(7)?,
                    exit_code: row.get(8)?,
                    blocked: blocked_int != 0,
                    duration_ms: row.get(10)?,
                    executed_at: row.get(11)?,
                })
            })
            .context("failed to query hook executions")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect hook executions")?;
        Ok(records)
    }

    // -- API Call Store -----------------------------------------------------------

    /// Save an API call record.
    pub fn save_api_call(&self, rec: &ApiCallRecord) -> Result<i64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute(
            "insert into api_calls (session_id, model, input_tokens, output_tokens, cost_usd, tool_name, provider, called_at, cache_read_tokens, cache_creation_tokens, cache_savings_usd) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![rec.session_id, rec.model, rec.input_tokens, rec.output_tokens, rec.cost_usd, rec.tool_name, rec.provider, rec.called_at, rec.cache_read_tokens, rec.cache_creation_tokens, rec.cache_savings_usd],
        )
        .context("failed to save api call")?;
        let id = conn.last_insert_rowid();
        Ok(id)
    }

    /// List API calls for a session.
    pub fn list_api_calls(&self, session_id: &str) -> Result<Vec<ApiCallRecord>> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut stmt = conn
            .prepare("select id, session_id, model, input_tokens, output_tokens, cost_usd, tool_name, provider, called_at, cache_read_tokens, cache_creation_tokens, cache_savings_usd from api_calls where session_id = ?1 order by called_at asc")
            .context("failed to prepare list_api_calls")?;
        let records = stmt
            .query_map(params![session_id], |row| {
                Ok(ApiCallRecord {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    model: row.get(2)?,
                    input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    cost_usd: row.get(5)?,
                    tool_name: row.get(6)?,
                    provider: row.get(7)?,
                    called_at: row.get(8)?,
                    cache_read_tokens: row.get(9).unwrap_or(0),
                    cache_creation_tokens: row.get(10).unwrap_or(0),
                    cache_savings_usd: row.get(11).unwrap_or(0.0),
                })
            })
            .context("failed to query api calls")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect api calls")?;
        Ok(records)
    }

    /// Get total cost for a session.
    pub fn session_cost(&self, session_id: &str) -> Result<f64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let total: f64 = conn
            .query_row(
                "select coalesce(sum(cost_usd), 0.0) from api_calls where session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0.0);
        Ok(total)
    }

    // -- Conversation Search ------------------------------------------------------

    /// Search across all session conversations for a text query.
    ///
    /// Uses `SQLite` FTS5 full-text search on conversation content. Falls back
    /// to LIKE-based search if FTS tables are not available.
    ///
    pub fn search_conversations(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ConversationSearchHit>> {
        let limit = limit.unwrap_or(20);
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Try FTS5 first; if the table doesn't exist, fall back to LIKE
        let fts_exists: bool = {
            let mut check = conn
                .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='conversation_fts' LIMIT 1")
                .ok();
            check
                .take()
                .is_some_and(|mut stmt| stmt.exists([]).unwrap_or(false))
        };

        if fts_exists {
            // Use FTS5 for fast full-text search
            let mut stmt = conn
                .prepare(
                    "SELECT c.session_id, s.captured_at, snippet(conversation_fts, 1, '>>', '<<', '...', 32) as snippet
                     FROM conversation_fts c
                     JOIN session_snapshots s ON s.session_id = c.session_id
                     WHERE conversation_fts MATCH ?1
                     ORDER BY rank
                     LIMIT ?2",
                )
                .context("failed to prepare FTS search")?;

            let hits = stmt
                .query_map(params![query, limit as i64], |row| {
                    let session_id: String = row.get(0)?;
                    let captured_at: String = row.get(1)?;
                    let snippet: String = row.get(2)?;
                    Ok(ConversationSearchHit {
                        session_id,
                        captured_at,
                        snippet,
                    })
                })
                .context("failed to execute FTS search")?
                .collect::<rusqlite::Result<Vec<_>>>()
                .context("failed to collect search hits")?;
            return Ok(hits);
        }

        // Fallback: LIKE-based search on snapshot_json
        let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = conn
            .prepare(
                "SELECT session_id, captured_at, snapshot_json
                 FROM session_snapshots
                 WHERE snapshot_json LIKE ?1
                 ORDER BY captured_at DESC
                 LIMIT ?2",
            )
            .context("failed to prepare LIKE search")?;

        let hits = stmt
            .query_map(params![pattern, limit as i64], |row| {
                let session_id: String = row.get(0)?;
                let captured_at: String = row.get(1)?;
                let json: String = row.get(2)?;

                // Extract a snippet around the match
                let lower_json = json.to_lowercase();
                let lower_query = query.to_lowercase();
                let snippet = if let Some(pos) = lower_json.find(&lower_query) {
                    let start = json.floor_char_boundary(pos.saturating_sub(64));
                    let end = json.floor_char_boundary((pos + query.len() + 64).min(json.len()));
                    let raw = &json[start..end];
                    // Truncate at character boundary to avoid panics
                    let raw = raw.chars().take(200).collect::<String>();
                    format!("...{}...", raw.trim())
                } else {
                    String::new()
                };

                Ok(ConversationSearchHit {
                    session_id,
                    captured_at,
                    snippet,
                })
            })
            .context("failed to execute LIKE search")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("failed to collect search hits")?;

        Ok(hits)
    }
}
