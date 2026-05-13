//! StateRuntime - SQLite state manager for derived indexing from rollouts.
//!
//! Maintains a `threads` table as a derived index from rollout JSONL files,
//! providing fast queries for session listing and metadata.

use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

/// Thread metadata derived from rollout files.
#[derive(Debug, Clone)]
pub struct ThreadMetadata {
    pub id: String,
    pub rollout_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub title: Option<String>,
    pub task: Option<String>,
    pub mode: Option<String>,
    pub status: Option<String>,
    pub tokens_used: u64,
    pub item_count: u64,
    pub bytes_written: u64,
    pub forked_from_id: Option<String>,
    pub workspace_path: Option<String>,
    pub git_branch: Option<String>,
}

/// Token usage for a specific turn.
#[derive(Debug, Clone)]
pub struct TurnTokenUsage {
    pub id: i64,
    pub thread_id: String,
    pub turn: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub created_at: String,
}

/// Tool execution record.
#[derive(Debug, Clone)]
pub struct ToolExecution {
    pub id: i64,
    pub thread_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub input_json: String,
    pub success: bool,
    pub output: String,
    pub output_size: i64,
    pub duration_ms: i64,
    pub created_at: String,
}

/// Plan history record.
#[derive(Debug, Clone)]
pub struct PlanHistory {
    pub id: i64,
    pub thread_id: String,
    pub plan_id: String,
    pub title: String,
    pub status: String,
    pub steps_count: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// StateRuntime maintains SQLite state derived from rollout files.
pub struct StateRuntime {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for StateRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateRuntime").finish()
    }
}

impl StateRuntime {
    /// Open the state database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let runtime = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        runtime.init_schema()?;
        Ok(runtime)
    }

    /// Create a new StateRuntime from an existing connection.
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Initialize database schema with all tables, indexes, and triggers.
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute_batch(
            r"
            BEGIN;

            -- Primary threads table (derived from rollout files)
            CREATE TABLE IF NOT EXISTS threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                title TEXT,
                task TEXT,
                mode TEXT,
                status TEXT,
                tokens_used INTEGER DEFAULT 0,
                item_count INTEGER DEFAULT 0,
                bytes_written INTEGER DEFAULT 0,
                forked_from_id TEXT,
                forked_at TEXT,
                workspace_path TEXT,
                git_branch TEXT,
                FOREIGN KEY (forked_from_id) REFERENCES threads(id) ON DELETE SET NULL
            );

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_threads_created_at ON threads(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_threads_updated_at ON threads(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_threads_status ON threads(status);
            CREATE INDEX IF NOT EXISTS idx_threads_forked_from ON threads(forked_from_id);

            -- Full-text search on title and task
            CREATE VIRTUAL TABLE IF NOT EXISTS threads_fts USING fts5(
                title,
                task,
                content=threads,
                content_rowid=rowid,
                tokenize='porter unicode61'
            );

            -- Trigger to keep FTS index in sync
            CREATE TRIGGER IF NOT EXISTS threads_fts_insert AFTER INSERT ON threads BEGIN
                INSERT INTO threads_fts(rowid, title, task)
                VALUES (NEW.rowid, NEW.title, NEW.task);
            END;

            CREATE TRIGGER IF NOT EXISTS threads_fts_delete AFTER DELETE ON threads BEGIN
                DELETE FROM threads_fts WHERE rowid = OLD.rowid;
            END;

            CREATE TRIGGER IF NOT EXISTS threads_fts_update AFTER UPDATE OF title, task ON threads BEGIN
                UPDATE threads_fts SET title = NEW.title, task = NEW.task
                WHERE rowid = NEW.rowid;
            END;

            -- Token usage by turn (for detailed analytics)
            CREATE TABLE IF NOT EXISTS turn_token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id TEXT NOT NULL,
                turn INTEGER NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cache_read_tokens INTEGER NOT NULL,
                cache_creation_tokens INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_turn_tokens_thread_turn ON turn_token_usage(thread_id, turn);

            -- Tool execution history (for debugging and replay)
            CREATE TABLE IF NOT EXISTS tool_executions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id TEXT NOT NULL,
                tool_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                input_json TEXT NOT NULL,
                success BOOLEAN NOT NULL,
                output TEXT NOT NULL,
                output_size INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tool_executions_thread ON tool_executions(thread_id);
            CREATE INDEX IF NOT EXISTS idx_tool_executions_tool_name ON tool_executions(tool_name);

            -- Plan history (for tracking plan execution across sessions)
            CREATE TABLE IF NOT EXISTS plan_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id TEXT NOT NULL,
                plan_id TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                steps_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                completed_at TEXT,
                FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_plan_history_thread ON plan_history(thread_id);
            CREATE INDEX IF NOT EXISTS idx_plan_history_plan_id ON plan_history(plan_id);

            -- Schema version tracking
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );

            INSERT OR IGNORE INTO schema_version (version, applied_at)
            VALUES (6, '2026-05-12T00:00:00Z');

            COMMIT;
            ",
        )?;
        Ok(())
    }

    /// Create a new thread row.
    pub fn create_thread(
        &self,
        thread_id: &str,
        rollout_path: &str,
        task: &str,
        mode: &str,
        workspace_path: Option<&Path>,
        git_branch: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, task, mode, status, workspace_path, git_branch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'executing', ?7, ?8)",
            params![
                thread_id,
                rollout_path,
                now,
                now,
                task,
                mode,
                workspace_path.map(|p| p.to_string_lossy().to_string()),
                git_branch,
            ],
        )?;
        Ok(())
    }

    /// Update thread metadata (called on each rollout write).
    pub fn update_thread(
        &self,
        thread_id: &str,
        item_count_delta: i64,
        bytes_delta: i64,
        tokens_delta: i64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute(
            "UPDATE threads
             SET updated_at = ?1,
                 item_count = item_count + ?2,
                 bytes_written = bytes_written + ?3,
                 tokens_used = tokens_used + ?4
             WHERE id = ?5",
            params![now, item_count_delta, bytes_delta, tokens_delta, thread_id],
        )?;
        Ok(())
    }

    /// Update thread title.
    pub fn update_thread_title(&self, thread_id: &str, title: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute(
            "UPDATE threads SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, thread_id],
        )?;
        Ok(())
    }

    /// Mark thread as completed.
    pub fn complete_thread(&self, thread_id: &str, final_token_count: u64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute(
            "UPDATE threads SET status = 'completed', updated_at = ?1, tokens_used = ?2 WHERE id = ?3",
            params![now, final_token_count as i64, thread_id],
        )?;
        Ok(())
    }

    /// Mark thread as errored.
    pub fn error_thread(&self, thread_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute(
            "UPDATE threads SET status = 'error', updated_at = ?1 WHERE id = ?2",
            params![now, thread_id],
        )?;
        Ok(())
    }

    /// Get recent threads.
    pub fn recent_threads(&self, limit: usize) -> Result<Vec<ThreadMetadata>> {
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "SELECT id, rollout_path, created_at, updated_at, title, task, mode, status,
                    tokens_used, item_count, bytes_written, forked_from_id, workspace_path, git_branch
             FROM threads
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let threads = stmt
            .query_map(params![limit as i64], |row| {
                Ok(ThreadMetadata {
                    id: row.get(0)?,
                    rollout_path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    title: row.get(4)?,
                    task: row.get(5)?,
                    mode: row.get(6)?,
                    status: row.get(7)?,
                    tokens_used: row.get::<_, i64>(8)? as u64,
                    item_count: row.get::<_, i64>(9)? as u64,
                    bytes_written: row.get::<_, i64>(10)? as u64,
                    forked_from_id: row.get(11)?,
                    workspace_path: row.get(12)?,
                    git_branch: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    /// Search threads by title/task using FTS.
    pub fn search_threads(&self, query: &str, limit: usize) -> Result<Vec<ThreadMetadata>> {
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "SELECT t.id, t.rollout_path, t.created_at, t.updated_at, t.title, t.task, t.mode, t.status,
                    t.tokens_used, t.item_count, t.bytes_written, t.forked_from_id, t.workspace_path, t.git_branch
             FROM threads t
             JOIN threads_fts f ON t.rowid = f.rowid
             WHERE threads_fts MATCH ?1
             ORDER BY t.created_at DESC
             LIMIT ?2",
        )?;

        let threads = stmt
            .query_map(params![query, limit as i64], |row| {
                Ok(ThreadMetadata {
                    id: row.get(0)?,
                    rollout_path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    title: row.get(4)?,
                    task: row.get(5)?,
                    mode: row.get(6)?,
                    status: row.get(7)?,
                    tokens_used: row.get::<_, i64>(8)? as u64,
                    item_count: row.get::<_, i64>(9)? as u64,
                    bytes_written: row.get::<_, i64>(10)? as u64,
                    forked_from_id: row.get(11)?,
                    workspace_path: row.get(12)?,
                    git_branch: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    /// Get a specific thread by ID.
    pub fn get_thread(&self, thread_id: &str) -> Result<Option<ThreadMetadata>> {
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        let mut stmt = conn.prepare(
            "SELECT id, rollout_path, created_at, updated_at, title, task, mode, status,
                    tokens_used, item_count, bytes_written, forked_from_id, workspace_path, git_branch
             FROM threads
             WHERE id = ?1",
        )?;

        let threads = stmt
            .query_map(params![thread_id], |row| {
                Ok(ThreadMetadata {
                    id: row.get(0)?,
                    rollout_path: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    title: row.get(4)?,
                    task: row.get(5)?,
                    mode: row.get(6)?,
                    status: row.get(7)?,
                    tokens_used: row.get::<_, i64>(8)? as u64,
                    item_count: row.get::<_, i64>(9)? as u64,
                    bytes_written: row.get::<_, i64>(10)? as u64,
                    forked_from_id: row.get(11)?,
                    workspace_path: row.get(12)?,
                    git_branch: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(threads.into_iter().next())
    }

    /// Delete a thread by ID.
    pub fn delete_thread(&self, thread_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        conn.execute("DELETE FROM threads WHERE id = ?1", params![thread_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_temp_runtime() -> (StateRuntime, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("state.sqlite");
        let runtime = StateRuntime::open(&db_path).unwrap();
        (runtime, dir)
    }

    #[test]
    fn test_create_and_get_thread() {
        let (runtime, _dir) = create_temp_runtime();

        runtime
            .create_thread(
                "thread-1",
                "/sessions/thread-1.jsonl",
                "Implement feature X",
                "executing",
                Some(Path::new("/workspace")),
                Some("main"),
            )
            .unwrap();

        let thread = runtime.get_thread("thread-1").unwrap();
        assert!(thread.is_some());
        let thread = thread.unwrap();
        assert_eq!(thread.id, "thread-1");
        assert_eq!(thread.task, Some("Implement feature X".to_string()));
        assert_eq!(thread.mode, Some("executing".to_string()));
        assert_eq!(thread.status, Some("executing".to_string()));
        assert_eq!(thread.workspace_path, Some("/workspace".to_string()));
        assert_eq!(thread.git_branch, Some("main".to_string()));
        assert_eq!(thread.item_count, 0);
        assert_eq!(thread.tokens_used, 0);
    }
}
