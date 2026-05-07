//! Schema management, database statistics, and cleanup methods.
//!
//! Contains `impl Storage` methods for database migration, statistics
//! collection, cleanup operations, and FTS index management.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::params;

use crate::{CleanupStats, DatabaseStats, Storage};

impl Storage {
    // -- Migration ----------------------------------------------------------------

    pub(crate) fn migrate(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        conn.execute_batch(
            "create table if not exists sessions (
                id text primary key,
                task text not null,
                created_at text not null,
                mode text not null default '\"executing\"',
                status text not null default '\"executing\"',
                plan_path text
            );
            create table if not exists events (
                id integer primary key autoincrement,
                session_id text not null,
                at text not null,
                kind text not null,
                detail text not null
            );
            create table if not exists memory (
                scope text not null,
                key text not null,
                value text not null,
                updated_at text not null,
                primary key (scope, key)
            );
            create table if not exists plans (
                id text primary key,
                session_id text not null,
                milestone_id text,
                task text not null,
                created_at text not null,
                status text not null,
                summary text not null,
                approach text not null,
                steps text not null,
                files_to_modify text not null,
                risks text not null,
                foreign key (session_id) references sessions(id)
            );
            create table if not exists milestones (
                id text primary key,
                session_id text not null,
                title text not null,
                description text not null,
                status text not null,
                plan_ids text not null default '[]',
                plan_dependencies text not null default '[]',
                success_criteria text not null default '[]',
                validation_command text,
                created_at text not null,
                updated_at text not null,
                completed_at text,
                foreign key (session_id) references sessions(id)
            );
            create table if not exists session_snapshots (
                session_id text not null,
                captured_at text not null,
                snapshot_json text not null,
                primary key (session_id)
            );
            create table if not exists checkpoints (
                id text primary key,
                session_id text not null,
                label text not null,
                commit_sha text,
                files_json text not null,
                created_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_checkpoints_session on checkpoints(session_id);
            create table if not exists rewind_snapshots (
                id integer primary key autoincrement,
                session_id text not null,
                interaction_number integer not null,
                role text not null,
                content_preview text not null,
                tools_used_json text,
                checkpoint_id text,
                captured_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade,
                foreign key (checkpoint_id) references checkpoints(id) on delete set null
            );
            create index if not exists idx_rewind_session on rewind_snapshots(session_id);
            create index if not exists idx_rewind_interaction on rewind_snapshots(session_id, interaction_number);
            create table if not exists hook_executions (
                id integer primary key autoincrement,
                session_id text not null,
                trigger_type text not null,
                hook_name text not null,
                command text not null,
                status text not null,
                stdout text,
                stderr text,
                exit_code integer,
                blocked integer not null default 0,
                duration_ms integer,
                executed_at text not null,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_hooks_session on hook_executions(session_id);
            create index if not exists idx_hooks_trigger on hook_executions(trigger_type);
            create table if not exists api_calls (
                id integer primary key autoincrement,
                session_id text not null,
                model text not null,
                input_tokens integer not null,
                output_tokens integer not null,
                cost_usd real not null,
                tool_name text,
                provider text,
                called_at text not null,
                cache_read_tokens integer not null default 0,
                cache_creation_tokens integer not null default 0,
                cache_savings_usd real not null default 0.0,
                foreign key (session_id) references sessions(id) on delete cascade
            );
            create index if not exists idx_api_calls_session on api_calls(session_id);
            create index if not exists idx_api_calls_model on api_calls(model);
            create table if not exists projects (
                id text primary key,
                path text not null,
                created_at text not null
            );
            create unique index if not exists idx_projects_path on projects(path);
            create table if not exists todos (
                id text primary key,
                session_id text not null,
                project_id text not null,
                content text not null,
                status text not null default 'pending',
                priority text not null default 'medium',
                position integer not null,
                created_at text not null,
                updated_at text not null,
                foreign key (project_id) references projects(id) on delete cascade
            );
            create index if not exists idx_todos_session on todos(session_id);
            create index if not exists idx_todos_project on todos(project_id);
            create table if not exists tasks (
                id text primary key,
                project_id text not null,
                session_id text,
                description text not null,
                status text not null default 'pending',
                owner text,
                dependencies text not null default '[]',
                output text,
                created_at text not null,
                updated_at text not null,
                started_at text,
                completed_at text,
                foreign key (project_id) references projects(id) on delete cascade
            );
            create index if not exists idx_tasks_project on tasks(project_id);
            create index if not exists idx_tasks_status on tasks(status);
            create index if not exists idx_tasks_owner on tasks(owner);",
        )?;
        Ok(())
    }

    // -- Database Stats -----------------------------------------------------------

    /// Get statistics about the database.
    pub fn db_stats(&self) -> Result<DatabaseStats> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let session_count: i64 = conn
            .query_row("SELECT count(*) FROM sessions", [], |row| row.get(0))
            .unwrap_or(0);
        let event_count: i64 = conn
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .unwrap_or(0);
        let api_call_count: i64 = conn
            .query_row("SELECT count(*) FROM api_calls", [], |row| row.get(0))
            .unwrap_or(0);
        let hook_execution_count: i64 = conn
            .query_row("SELECT count(*) FROM hook_executions", [], |row| row.get(0))
            .unwrap_or(0);
        let checkpoint_count: i64 = conn
            .query_row("SELECT count(*) FROM checkpoints", [], |row| row.get(0))
            .unwrap_or(0);

        let oldest_session: Option<String> = conn
            .query_row(
                "SELECT created_at FROM sessions ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();
        let newest_session: Option<String> = conn
            .query_row(
                "SELECT created_at FROM sessions ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        let db_size_bytes = conn
            .query_row("PRAGMA page_count", [], |row| row.get::<_, i64>(0))
            .ok()
            .zip(
                conn.query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
                    .ok(),
            )
            .map_or(0, |(pages, size)| (pages as u64) * (size as u64));

        Ok(DatabaseStats {
            session_count,
            event_count,
            api_call_count,
            hook_execution_count,
            checkpoint_count,
            db_size_bytes,
            oldest_session,
            newest_session,
        })
    }

    // -- Cleanup ------------------------------------------------------------------

    /// Remove sessions older than `max_age_days` and all related data.
    ///
    /// Tables with `ON DELETE CASCADE` (checkpoints, `rewind_snapshots`,
    /// `hook_executions`, `api_calls`) are cleaned automatically when their
    /// parent session is deleted. The `events` table lacks cascade, so
    /// it is cleaned explicitly.
    pub fn cleanup_old_sessions(&self, max_age_days: u64) -> Result<CleanupStats> {
        let max_days_i64 = i64::try_from(max_age_days).unwrap_or(i64::MAX);
        let cutoff = Utc::now()
            - chrono::Duration::try_days(max_days_i64)
                .unwrap_or_else(|| chrono::Duration::days(i64::MAX));

        let cutoff_str = cutoff.to_rfc3339();
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Find sessions to delete
        let old_session_ids: Vec<String> = {
            let mut stmt = conn.prepare("SELECT id FROM sessions WHERE created_at < ?1")?;
            let rows = stmt.query_map(params![cutoff_str], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        if old_session_ids.is_empty() {
            return Ok(CleanupStats::default());
        }

        let mut stats = CleanupStats::default();

        for sid in &old_session_ids {
            // Count related records for stats before deletion
            stats.events_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM events WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.api_calls_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM api_calls WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.hook_executions_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM hook_executions WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.checkpoints_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM checkpoints WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.rewind_snapshots_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM rewind_snapshots WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);
            stats.snapshots_removed += u64::try_from(
                conn.query_row(
                    "SELECT count(*) FROM session_snapshots WHERE session_id = ?1",
                    params![sid],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap_or(0),
            )
            .unwrap_or(0);

            // Delete events explicitly (no cascade)
            conn.execute("DELETE FROM events WHERE session_id = ?1", params![sid])?;

            // Delete FTS entries explicitly (may not exist, ignore error)
            let fts_removed = conn
                .execute(
                    "DELETE FROM conversation_fts WHERE session_id = ?1",
                    params![sid],
                )
                .unwrap_or(0);
            stats.fts_entries_removed += u64::try_from(fts_removed).unwrap_or(0);

            // Delete session snapshots explicitly (no FK cascade)
            conn.execute(
                "DELETE FROM session_snapshots WHERE session_id = ?1",
                params![sid],
            )?;

            // Delete session (cascades to checkpoints, rewind_snapshots, hook_executions, api_calls)
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![sid])?;
            stats.sessions_removed += 1;
        }

        Ok(stats)
    }

    /// Remove ALL sessions and related data.
    pub fn cleanup_all_sessions(&self) -> Result<CleanupStats> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let stats = CleanupStats {
            sessions_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM sessions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            events_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            api_calls_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM api_calls", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            hook_executions_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM hook_executions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            checkpoints_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM checkpoints", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            rewind_snapshots_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM rewind_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            snapshots_removed: u64::try_from(
                conn.query_row("SELECT count(*) FROM session_snapshots", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap_or(0),
            )
            .unwrap_or(0),
            fts_entries_removed: 0,
        };

        // Delete from all tables (order matters for FK constraints)
        conn.execute("DELETE FROM events", [])?;
        if let Err(e) = conn.execute("DELETE FROM conversation_fts", []) {
            tracing::warn!(error = %e, "failed to clear conversation_fts table");
        }
        conn.execute("DELETE FROM session_snapshots", [])?;
        conn.execute("DELETE FROM api_calls", [])?;
        conn.execute("DELETE FROM hook_executions", [])?;
        conn.execute("DELETE FROM rewind_snapshots", [])?;
        conn.execute("DELETE FROM checkpoints", [])?;
        conn.execute("DELETE FROM sessions", [])?;

        Ok(stats)
    }

    /// Run `VACUUM` to reclaim disk space and defragment the database.
    pub fn vacuum(&self) -> Result<()> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute_batch("VACUUM")
            .context("failed to vacuum database")?;
        Ok(())
    }

    /// Remove orphaned events that reference sessions that no longer exist.
    pub fn cleanup_orphaned_events(&self) -> Result<u64> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let removed = conn.execute(
            "DELETE FROM events WHERE session_id NOT IN (SELECT id FROM sessions)",
            [],
        )?;
        Ok(u64::try_from(removed).unwrap_or(0))
    }

    // -- FTS Index ----------------------------------------------------------------

    /// Create the FTS5 virtual table for conversation search indexing.
    ///
    /// This is called during `Storage::open` to enable fast full-text search
    /// across conversation content. If FTS5 is not available, search falls
    /// back to LIKE-based queries.
    pub(crate) fn ensure_fts_index(&self) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Create a standalone FTS5 virtual table (not content-synced).
        // Content-synced FTS tables require triggers and have complex lifecycle;
        // a standalone table is simpler and more reliable.
        conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS conversation_fts USING fts5(
                session_id,
                content,
                tokenize='porter unicode61'
            );",
        )
        .context("failed to create conversation_fts (FTS5 may not be compiled in)")?;

        // Populate FTS from existing snapshots that haven't been indexed yet
        conn.execute(
            "INSERT OR IGNORE INTO conversation_fts(session_id, content)
             SELECT s.session_id, s.snapshot_json
             FROM session_snapshots s
             WHERE NOT EXISTS (
                 SELECT 1 FROM conversation_fts c WHERE c.session_id = s.session_id
             )",
            [],
        )
        .context("failed to populate FTS index")?;

        Ok(())
    }
}
