# Phase 2: Event-Sourced Rollouts — Architecture Design

**Status:** In Progress (15%)  
**Author:** RustyCode Architecture Team  
**Last Updated:** 2026-05-12  
**Related:** [Phase 1 EventMsg System](./PHASE1-UNIFY-EVENTS.md)

## Goal

Transform RustyCode's session persistence layer from snapshot-based serialization to **event-sourced rollouts** with JSONL as the source of truth and SQLite as a derived index. This enables session replay, thread forking, audit trails, and crash recovery while maintaining compatibility with the existing EventMsg protocol introduced in Phase 1.

## Current Progress (2026-05-12)

- **RolloutRecorder (MVP Implemented)**: `crates/rustycode-core/src/rollout.rs` provides async JSONL recording of LLM interactions, tool calls, and `EventMsg`/`Op` traffic.
- **JSONL Format (Established)**: Standardized on newline-delimited JSON with timestamps.
- **SQLite Derived Index (Pending)**: Migration of `rustycode-storage` from snapshots to derived event-sourced indexes is in design.
- **Session Replay (Pending)**: Initial logic for reading JSONL exists (`read_rollout`), but full state reconstruction is pending.

### Rollout File Format

Codex stores rollouts as JSONL files (`~/.codex/sessions/YYYY/MM/DD/rollout-TIMESTAMP-UUID.jsonl`) where each line is a JSON object with a `timestamp` and `item` payload. The 5 item types are:

1. **SessionMeta** — Session initialization data (task, mode, context)
2. **ResponseItem** — LLM responses with text/thinking deltas
3. **Compacted** — Summarized/compacted historical items
4. **TurnContext** — Turn-level metadata (token usage, tool calls)
5. **EventMsg** — Application events (tool execution, errors, lifecycle)

### Real-Time Synchronization

Codex's `RolloutRecorder` writes each item to the JSONL file and **immediately applies** it to an in-memory state structure, which is then persisted to SQLite (`state_5.sqlite`). The `threads` table stores:
- `id` — Thread UUID (primary key)
- `rollout_path` — Path to JSONL file
- `created_at` / `updated_at` — Timestamps
- `title` — Session title (derived from first user message)
- `tokens_used` — Total token count (derived from TurnContext items)

### Backfill System

Codex uses a watermark-based backfill process:
- **Watermark**: Tracks the last processed rollout file timestamp
- **Batching**: Processes 200 files at a time to avoid memory pressure
- **Lease**: Singleton worker acquires a 900-second lease via file lock
- **Crash Recovery**: If lease expires, another worker can take over

### Read Repair

On session access, Codex validates filesystem vs SQLite consistency:
- Count items in JSONL file
- Compare with SQLite `threads` row
- Trigger backfill if mismatch detected

### Session Replay

To replay a session:
1. Load all items from JSONL file
2. Apply each item sequentially to an empty state
3. Rebuild conversation history, token counts, and metadata

### Thread Forking

To create a forked thread:
1. Generate new `ThreadId` (UUID)
2. Set `forked_from_id = parent_thread_id`
3. Copy parent items as `InitialHistory::Forked` (compressed representation)
4. Start new rollout file with fork marker

### Compaction

When a rollout exceeds a threshold (e.g., 10,000 items):
1. Read all items from JSONL
2. Group consecutive items by type
3. Replace old items with `Compacted` item containing summary
4. Write compacted rollout to new file
5. Atomic rename

### Output Truncation

Codex truncates large tool outputs in `ExecCommandEnd` items:
- `aggregated_output` field truncated to 10KB
- Full output preserved in separate file if needed

## RolloutRecorder

### Overview

`RolloutRecorder` is the primary writer for rollout JSONL files. It:

1. Opens a new JSONL file on session start (`rollout-{timestamp}-{uuid}.jsonl`)
2. Writes each event as a JSON line with `{timestamp, item}` structure
3. Maintains an in-memory buffer of recent items for replay
4. Syncs to SQLite on each write (via `StateRuntime`)

### File Structure

```
~/.rustycode/sessions/
├── YYYY/
│   └── MM/
│       └── DD/
│           ├── rollout-{timestamp}-{uuid}.jsonl
│           ├── rollout-{timestamp}-{uuid}.jsonl.compacted  (post-compaction)
│           └── .watermark  (backfill cursor)
└── state_6.sqlite  (derived index, replaces state_5.sqlite)
```

### Type Definitions

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Rollout file path: ~/.rustycode/sessions/YYYY/MM/DD/rollout-{timestamp}-{uuid}.jsonl
#[derive(Debug, Clone)]
pub struct RolloutPath {
    pub base_dir: PathBuf,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub timestamp: i64,
    pub uuid: Uuid,
}

impl RolloutPath {
    pub fn new(base_dir: PathBuf, session_id: &SessionId) -> Self {
        let now = Utc::now();
        Self {
            base_dir,
            year: now.year(),
            month: now.month(),
            day: now.day(),
            timestamp: now.timestamp(),
            uuid: session_id.into_uuid(),
        }
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.base_dir
            .join(format!("{:04}", self.year))
            .join(format!("{:02}", self.month))
            .join(format!("{:02}", self.day))
            .join(format!("rollout-{}-{:013}.jsonl", self.timestamp, self.uuid))
    }
}

/// Rollout recorder — writes JSONL files and syncs to SQLite
pub struct RolloutRecorder {
    session_id: SessionId,
    rollout_path: RolloutPath,
    file: BufWriter<File>,
    item_count: u64,
    bytes_written: u64,
}

impl RolloutRecorder {
    /// Open a new rollout file for writing
    pub fn open(session_id: SessionId, base_dir: PathBuf) -> anyhow::Result<Self> {
        let rollout_path = RolloutPath::new(base_dir, &session_id);
        let full_path = rollout_path.to_path_buf();

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = BufWriter::new(File::create(&full_path)?);

        Ok(Self {
            session_id,
            rollout_path,
            file,
            item_count: 0,
            bytes_written: 0,
        })
    }

    /// Write a single rollout item
    pub fn write_item(&mut self, item: RolloutItem) -> anyhow::Result<()> {
        let line = RolloutLine {
            timestamp: Utc::now(),
            item,
        };

        let json = serde_json::to_string(&line)?;
        writeln!(self.file, "{}", json)?;
        
        self.item_count += 1;
        self.bytes_written += json.len() as u64 + 1; // +1 for newline

        self.file.flush()?;
        Ok(())
    }

    /// Close the rollout file
    pub fn close(mut self) -> anyhow::Result<()> {
        self.file.flush()?;
        Ok(())
    }
}
```

## RolloutItem Types

### Overview

`RolloutItem` is a comprehensive enum covering all session events. It extends Phase 1's `EventMsg` with additional metadata items for replay and compaction.

### Type Definition

```rust
/// A single item in a rollout JSONL file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "item_type", content = "item_data")]
pub enum RolloutItem {
    // ── Session Lifecycle ───────────────────────────────────────────────────────
    /// Session initialization metadata
    SessionMeta {
        session_id: SessionId,
        task: String,
        mode: SessionMode,
        tool_approval_mode: ToolApprovalMode,
        created_at: DateTime<Utc>,
        workspace_path: Option<PathBuf>,
        git_branch: Option<String>,
    },

    /// Session completed successfully
    SessionDone {
        completed_at: DateTime<Utc>,
        final_token_count: u64,
    },

    /// Session stopped with error
    SessionError {
        error: String,
        stopped_at: DateTime<Utc>,
    },

    // ── Turn Context ───────────────────────────────────────────────────────────
    /// Turn-level metadata (wraps EventMsg::TokenUsage with turn index)
    TurnContext {
        turn: usize,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_creation_tokens: u64,
    },

    /// Turn started marker
    TurnStarted {
        turn: usize,
        started_at: DateTime<Utc>,
    },

    /// Turn completed marker
    TurnCompleted {
        turn: usize,
        stop_reason: String,
        completed_at: DateTime<Utc>,
    },

    // ── LLM Responses ───────────────────────────────────────────────────────────
    /// Text delta from LLM (wraps EventMsg::TextDelta)
    TextDelta {
        delta: String,
    },

    /// Thinking delta from LLM (wraps EventMsg::ThinkingDelta)
    ThinkingDelta {
        delta: String,
    },

    /// Thinking block completed (wraps EventMsg::ThinkingBlockCompleted)
    ThinkingBlockCompleted {
        block_type: String,
        signature: String,
        data: String,
    },

    // ── Tool Execution ──────────────────────────────────────────────────────────
    /// Tool call started (wraps EventMsg::ToolCallStarted)
    ToolCallStarted {
        tool_name: String,
        tool_id: String,
        input: serde_json::Value,
    },

    /// Tool input delta (wraps EventMsg::ToolInputDelta)
    ToolInputDelta {
        tool_id: String,
        delta: String,
    },

    /// Tool execution started (wraps EventMsg::ToolExecStarted)
    ToolExecStarted {
        tool_name: String,
        tool_id: String,
    },

    /// Tool execution progress (wraps EventMsg::ToolExecProgress)
    ToolExecProgress {
        tool_id: String,
        stage: String,
        elapsed_ms: u64,
        preview: Option<String>,
    },

    /// Tool execution completed (wraps EventMsg::ToolExecCompleted)
    ToolExecCompleted {
        tool_id: String,
        tool_name: String,
        success: bool,
        output: String,
        output_size: usize,
        duration_ms: u64,
    },

    /// File snapshot before write (wraps EventMsg::FileSnapshot)
    FileSnapshot {
        batch: Vec<(String, String)>,
    },

    // ─── Tool Approval ──────────────────────────────────────────────────────────
    /// Approval required (wraps EventMsg::ApprovalRequired)
    ApprovalRequired {
        tool_name: String,
        tool_id: String,
        operation_class: OperationClass,
        description: String,
        diff: Option<String>,
    },

    /// Approval approved (wraps EventMsg::ApprovalApproved)
    ApprovalApproved {
        tool_id: String,
    },

    /// Approval rejected (wraps EventMsg::ApprovalRejected)
    ApprovalRejected {
        tool_id: String,
    },

    // ── User Questions ──────────────────────────────────────────────────────────
    /// Question required (wraps EventMsg::QuestionRequired)
    QuestionRequired {
        question_id: String,
        question_text: String,
        header: String,
        options: Vec<QuestionOption>,
        multi_select: bool,
    },

    /// Question answered (wraps EventMsg::QuestionAnswered)
    QuestionAnswered {
        question_id: String,
        answer: String,
    },

    // ── Plan Events ─────────────────────────────────────────────────────────────
    /// Plan created (wraps EventMsg::PlanCreated)
    PlanCreated {
        plan_id: PlanId,
        title: String,
        steps: Vec<PlanStepInfo>,
    },

    /// Plan step started (wraps EventMsg::PlanStepStarted)
    PlanStepStarted {
        plan_id: PlanId,
        step_index: usize,
    },

    /// Plan step completed (wraps EventMsg::PlanStepCompleted)
    PlanStepCompleted {
        plan_id: PlanId,
        step_index: usize,
        success: bool,
        message: String,
    },

    /// Plan completed (wraps EventMsg::PlanCompleted)
    PlanCompleted {
        plan_id: PlanId,
        success: bool,
        summary: String,
    },

    /// Plan approval requested (wraps EventMsg::PlanApprovalRequested)
    PlanApprovalRequested {
        plan_id: PlanId,
        title: String,
        steps: Vec<PlanStepInfo>,
    },

    // ── Workspace Events ────────────────────────────────────────────────────────
    /// Workspace update (wraps EventMsg::Workspace)
    Workspace(WorkspaceEvent),

    // ── Slash Commands ──────────────────────────────────────────────────────────
    /// Command event (wraps EventMsg::Command)
    Command(CommandEvent),

    // ── Milestone Progress ─────────────────────────────────────────────────────
    /// Milestone progress (wraps EventMsg::MilestoneProgress)
    MilestoneProgress(MilestoneProgress),

    // ── Compaction ──────────────────────────────────────────────────────────────
    /// Compacted historical items (replaces multiple items)
    Compacted {
        items_compacted: u64,
        summary: String,
        token_estimate: u64,
        earliest_timestamp: DateTime<Utc>,
        latest_timestamp: DateTime<Utc>,
    },

    // ── Thread Forking ──────────────────────────────────────────────────────────
    /// Thread fork marker (first item in forked rollout)
    ForkedFrom {
        parent_thread_id: ThreadId,
        parent_rollout_path: String,
        forked_at: DateTime<Utc>,
    },

    // ── System Messages ──────────────────────────────────────────────────────────
    /// System message (wraps EventMsg::SystemMessage)
    SystemMessage(String),

    /// Execution trace (wraps EventMsg::ExecutionTrace)
    ExecutionTrace(serde_json::Value),
}

/// A single line in a rollout JSONL file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutLine {
    pub timestamp: DateTime<Utc>,
    pub item: RolloutItem,
}
```

## StateRuntime

### Overview

`StateRuntime` is the SQLite state manager. It:

1. Maintains a `threads` table (derived index from rollouts)
2. Updates derived data on each `RolloutRecorder` write
3. Provides fast queries for session listing and metadata
4. Supports read repair and backfill operations

### SQLite Schema

```sql
-- Primary threads table (derived from rollout files)
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,                    -- Thread UUID (same as session_id)
    rollout_path TEXT NOT NULL UNIQUE,      -- Path to JSONL file
    created_at TEXT NOT NULL,               -- ISO8601 timestamp
    updated_at TEXT NOT NULL,               -- ISO8601 timestamp
    title TEXT,                             -- Session title (from first user message)
    task TEXT,                              -- Original task description
    mode TEXT,                              -- Session mode (executing, planning, etc.)
    status TEXT,                            -- Session status (executing, completed, error)
    tokens_used INTEGER DEFAULT 0,          -- Total tokens (sum of TurnContext)
    item_count INTEGER DEFAULT 0,           -- Number of items in rollout
    bytes_written INTEGER DEFAULT 0,        -- Total bytes in rollout file
    forked_from_id TEXT,                    -- Parent thread ID (if forked)
    forked_at TEXT,                         -- Fork timestamp (ISO8601)
    workspace_path TEXT,                    -- Workspace path (if available)
    git_branch TEXT,                        -- Git branch (if available)
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

CREATE TRIGGER IF NOT EXISTS threads_fts_update AFTER UPDATE ON threads BEGIN
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
    input_json TEXT NOT NULL,             -- JSON serialized input
    success BOOLEAN NOT NULL,
    output TEXT NOT NULL,                 -- Truncated output (10KB max)
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
```

### Rust API

```rust
use rusqlite::{Connection, params};
use anyhow::Result;

pub struct StateRuntime {
    conn: Connection,
}

impl StateRuntime {
    /// Open the state database
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let runtime = Self { conn };
        runtime.init_schema()?;
        Ok(runtime)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "BEGIN;
             CREATE TABLE IF NOT EXISTS threads (...);
             CREATE INDEX IF NOT EXISTS idx_threads_created_at ON threads(created_at DESC);
             ...
             COMMIT;"
        )?;
        Ok(())
    }

    /// Create a new thread row
    pub fn create_thread(
        &self,
        thread_id: &ThreadId,
        rollout_path: &str,
        task: &str,
        mode: &SessionMode,
        workspace_path: Option<&Path>,
        git_branch: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, task, mode, status, workspace_path, git_branch)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'executing', ?7, ?8)",
            params![
                thread_id.to_string(),
                rollout_path,
                now,
                now,
                task,
                mode.to_string(),
                workspace_path.map(|p| p.to_string()),
                git_branch,
            ],
        )?;
        Ok(())
    }

    /// Update thread metadata (called on each rollout write)
    pub fn update_thread(
        &self,
        thread_id: &ThreadId,
        item_count_delta: u64,
        bytes_delta: u64,
        tokens_delta: u64,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE threads 
             SET updated_at = ?1, 
                 item_count = item_count + ?2,
                 bytes_written = bytes_written + ?3,
                 tokens_used = tokens_used + ?4
             WHERE id = ?5",
            params![now, item_count_delta, bytes_delta, tokens_delta, thread_id.to_string()],
        )?;
        Ok(())
    }

    /// Mark thread as completed
    pub fn complete_thread(&self, thread_id: &ThreadId, final_token_count: u64) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE threads SET status = 'completed', updated_at = ?1, tokens_used = ?2 WHERE id = ?3",
            params![now, final_token_count, thread_id.to_string()],
        )?;
        Ok(())
    }

    /// Mark thread as errored
    pub fn error_thread(&self, thread_id: &ThreadId, error: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE threads SET status = 'error', updated_at = ?1 WHERE id = ?2",
            params![now, thread_id.to_string()],
        )?;
        Ok(())
    }

    /// Get recent threads
    pub fn recent_threads(&self, limit: usize) -> Result<Vec<ThreadMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, rollout_path, created_at, updated_at, title, task, mode, status, 
                    tokens_used, item_count, bytes_written, forked_from_id, workspace_path, git_branch
             FROM threads 
             ORDER BY created_at DESC 
             LIMIT ?1"
        )?;

        let threads = stmt.query_map(params![limit as i64], |row| {
            Ok(ThreadMetadata {
                id: row.get(0)?,
                rollout_path: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                title: row.get(4)?,
                task: row.get(5)?,
                mode: row.get(6)?,
                status: row.get(7)?,
                tokens_used: row.get(8)?,
                item_count: row.get(9)?,
                bytes_written: row.get(10)?,
                forked_from_id: row.get(11)?,
                workspace_path: row.get(12)?,
                git_branch: row.get(13)?,
            })
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }

    /// Search threads by title/task
    pub fn search_threads(&self, query: &str, limit: usize) -> Result<Vec<ThreadMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.id, t.rollout_path, t.created_at, t.updated_at, t.title, t.task, t.mode, t.status,
                    t.tokens_used, t.item_count, t.bytes_written, t.forked_from_id, t.workspace_path, t.git_branch
             FROM threads t
             JOIN threads_fts f ON t.rowid = f.rowid
             WHERE threads_fts MATCH ?1
             ORDER BY t.created_at DESC
             LIMIT ?2"
        )?;

        let threads = stmt.query_map(params![query, limit as i64], |row| {
            // Same mapping as recent_threads
            ...
        })?.collect::<Result<Vec<_>, _>>()?;

        Ok(threads)
    }
}

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
```

## Session Replay

### Overview

Session replay rebuilds complete session state from a rollout JSONL file. This is used for:

1. **Crash recovery** — Resume interrupted sessions
2. **Read repair** — Verify filesystem vs SQLite consistency
3. **Forking** — Copy parent state to new thread
4. **Debugging** — Inspect historical sessions

### Replay Algorithm

```rust
pub struct SessionReplayer;

impl SessionReplayer {
    /// Replay a rollout file into a complete session state
    pub fn replay(rollout_path: &Path) -> anyhow::Result<ReplayedSession> {
        let file = File::open(rollout_path)?;
        let reader = BufReader::new(file);

        let mut session = ReplayedSession::new();
        let mut current_turn = 0;
        let mut current_tokens = 0;

        for line in reader.lines() {
            let line = line?;
            let rollout_line: RolloutLine = serde_json::from_str(&line)?;

            match rollout_line.item {
                RolloutItem::SessionMeta { session_id, task, mode, .. } => {
                    session.session_id = session_id;
                    session.task = task;
                    session.mode = mode;
                }

                RolloutItem::TurnStarted { turn, .. } => {
                    current_turn = turn;
                }

                RolloutItem::TurnContext { input_tokens, output_tokens, .. } => {
                    current_tokens += input_tokens + output_tokens;
                }

                RolloutItem::TextDelta { delta } => {
                    session.accumulated_text.push_str(&delta);
                }

                RolloutItem::ToolExecCompleted { tool_name, output, .. } => {
                    session.tool_outputs.push((tool_name, output));
                }

                RolloutItem::SessionDone { .. } => {
                    session.status = SessionStatus::Completed;
                }

                RolloutItem::SessionError { error, .. } => {
                    session.status = SessionStatus::Error(error);
                }

                // ... handle all other item types
                _ => {}
            }
        }

        session.total_tokens = current_tokens;
        Ok(session)
    }
}

#[derive(Debug, Clone)]
pub struct ReplayedSession {
    pub session_id: SessionId,
    pub task: String,
    pub mode: SessionMode,
    pub status: SessionStatus,
    pub accumulated_text: String,
    pub tool_outputs: Vec<(String, String)>,
    pub total_tokens: u64,
}

impl ReplayedSession {
    pub fn new() -> Self {
        Self {
            session_id: SessionId::new(),
            task: String::new(),
            mode: SessionMode::Executing,
            status: SessionStatus::Executing,
            accumulated_text: String::new(),
            tool_outputs: Vec::new(),
            total_tokens: 0,
        }
    }
}
```

## Thread Forking

### Overview

Thread forking creates a new session that copies the parent's history. This enables:

1. **Exploration** — Try different approaches without losing the original
2. **A/B testing** — Compare solutions in parallel threads
3. **Rollback** — Fork from a checkpoint to undo changes

### Fork Algorithm

```rust
pub struct ThreadForker;

impl ThreadForker {
    /// Fork a new thread from a parent rollout
    pub fn fork(
        parent_rollout_path: &Path,
        new_session_id: SessionId,
        base_dir: PathBuf,
    ) -> anyhow::Result<(SessionId, RolloutRecorder)> {
        // 1. Replay parent rollout to get state
        let parent_session = SessionReplayer::replay(parent_rollout_path)?;

        // 2. Create new rollout file
        let mut recorder = RolloutRecorder::open(new_session_id.clone(), base_dir)?;

        // 3. Write fork marker as first item
        let parent_thread_id = ThreadId::from_session_id(&parent_session.session_id);
        recorder.write_item(RolloutItem::ForkedFrom {
            parent_thread_id,
            parent_rollout_path: parent_rollout_path.to_string_lossy().to_string(),
            forked_at: Utc::now(),
        })?;

        // 4. Write compacted parent history
        let compacted = RolloutItem::Compacted {
            items_compacted: parent_session.item_count,
            summary: format!("Forked from parent thread: {}", parent_session.task),
            token_estimate: parent_session.total_tokens,
            earliest_timestamp: parent_session.created_at,
            latest_timestamp: Utc::now(),
        };
        recorder.write_item(compacted)?;

        // 5. Write session metadata for new thread
        recorder.write_item(RolloutItem::SessionMeta {
            session_id: new_session_id.clone(),
            task: format!("[Fork] {}", parent_session.task),
            mode: parent_session.mode,
            tool_approval_mode: ToolApprovalMode::Auto,
            created_at: Utc::now(),
            workspace_path: None,
            git_branch: None,
        })?;

        Ok((new_session_id, recorder))
    }
}

#[derive(Debug, Clone)]
pub struct ThreadId(Uuid);

impl ThreadId {
    pub fn from_session_id(session_id: &SessionId) -> Self {
        Self(session_id.into_uuid())
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}
```

## Backfill System

### Overview

The backfill system processes existing rollout files and updates the SQLite index. It runs:

1. **On startup** — Catch up on any missed rollouts
2. **Periodically** — Every 5 minutes (configurable)
3. **On demand** — Triggered by CLI command

### Watermark Tracking

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillWatermark {
    pub last_processed_timestamp: i64,
    pub last_processed_path: Option<String>,
    pub items_processed: u64,
    pub threads_updated: u64,
    pub errors: Vec<String>,
}

impl BackfillWatermark {
    pub fn load(base_dir: &Path) -> anyhow::Result<Self> {
        let watermark_path = base_dir.join(".watermark");
        if watermark_path.exists() {
            let content = fs::read_to_string(&watermark_path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self {
                last_processed_timestamp: 0,
                last_processed_path: None,
                items_processed: 0,
                threads_updated: 0,
                errors: Vec::new(),
            })
        }
    }

    pub fn save(&self, base_dir: &Path) -> anyhow::Result<()> {
        let watermark_path = base_dir.join(".watermark");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&watermark_path, content)?;
        Ok(())
    }
}
```

### Backfill Worker

```rust
pub struct BackfillWorker {
    base_dir: PathBuf,
    state_runtime: Arc<Mutex<StateRuntime>>,
    batch_size: usize,
    lease_duration: Duration,
}

impl BackfillWorker {
    pub fn new(base_dir: PathBuf, state_runtime: Arc<Mutex<StateRuntime>>) -> Self {
        Self {
            base_dir,
            state_runtime,
            batch_size: 200,
            lease_duration: Duration::from_secs(900),
        }
    }

    /// Run a single backfill pass
    pub fn backfill(&mut self) -> anyhow::Result<BackfillReport> {
        // 1. Acquire lease
        let lease = self.acquire_lease()?;

        // 2. Load watermark
        let mut watermark = BackfillWatermark::load(&self.base_dir)?;

        // 3. Scan for rollout files newer than watermark
        let rollout_files = self.scan_rollouts_since(watermark.last_processed_timestamp)?;

        // 4. Process in batches
        let mut report = BackfillReport::default();
        for batch in rollout_files.chunks(self.batch_size) {
            // Check lease still valid
            if !lease.is_valid() {
                break;
            }

            for rollout_path in batch {
                match self.process_rollout(rollout_path) {
                    Ok(stats) => {
                        watermark.items_processed += stats.items_processed;
                        watermark.threads_updated += 1;
                        watermark.last_processed_timestamp = stats.timestamp;
                        watermark.last_processed_path = Some(rollout_path.clone());
                        report.processed += 1;
                    }
                    Err(e) => {
                        watermark.errors.push(format!("{}: {}", rollout_path.display(), e));
                        report.failed += 1;
                    }
                }
            }

            // Save watermark after each batch
            watermark.save(&self.base_dir)?;
        }

        // 5. Release lease
        lease.release()?;

        Ok(report)
    }

    /// Process a single rollout file
    fn process_rollout(&self, rollout_path: &Path) -> anyhow::Result<RolloutStats> {
        // 1. Replay rollout
        let session = SessionReplayer::replay(rollout_path)?;

        // 2. Update/create thread in SQLite
        let thread_id = ThreadId::from_session_id(&session.session_id);
        let runtime = self.state_runtime.lock().unwrap();

        // Check if thread exists
        let existing = runtime.get_thread(&thread_id)?;
        if existing.is_some() {
            // Update existing thread
            runtime.update_thread(
                &thread_id,
                session.item_count,
                session.bytes_written,
                session.total_tokens,
            )?;
        } else {
            // Create new thread
            runtime.create_thread(
                &thread_id,
                rollout_path.to_string_lossy().as_ref(),
                &session.task,
                &session.mode,
                None,
                None,
            )?;
        }

        Ok(RolloutStats {
            timestamp: session.created_at.timestamp(),
            items_processed: session.item_count,
        })
    }

    /// Acquire exclusive lease via file lock
    fn acquire_lease(&self) -> anyhow::Result<BackfillLease> {
        let lease_path = self.base_dir.join(".backfill_lease");
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&lease_path)?;

        // Try to acquire exclusive lock
        file.try_lock_exclusive()
            .map_err(|_| anyhow!("Another backfill worker is running"))?;

        // Write lease expiration
        let expires_at = Utc::now() + chrono::Duration::from_std(self.lease_duration)?;
        fs::write(&lease_path, expires_at.to_rfc3339())?;

        Ok(BackfillLease {
            _file: file,
            path: lease_path,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BackfillLease {
    _file: File,
    path: PathBuf,
}

impl BackfillLease {
    pub fn is_valid(&self) -> bool {
        // Check if lease file still exists and is not expired
        if let Ok(content) = fs::read_to_string(&self.path) {
            if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&content) {
                return Utc::now() < expires_at.with_timezone(&Utc);
            }
        }
        false
    }

    pub fn release(self) -> anyhow::Result<()> {
        // Release file lock and remove lease file
        drop(self._file);
        fs::remove_file(&self.path)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BackfillReport {
    pub processed: u64,
    pub failed: u64,
}

#[derive(Debug, Clone)]
pub struct RolloutStats {
    pub timestamp: i64,
    pub items_processed: u64,
}
```

## Compaction

### Overview

Compaction reduces rollout file size by replacing old items with summaries. It runs:

1. **When item count exceeds threshold** (e.g., 10,000 items)
2. **On demand** — Triggered by CLI command
3. **During backfill** — Compact old rollouts while scanning

### Compaction Algorithm

```rust
pub struct RolloutCompactor;

impl RolloutCompactor {
    /// Compact a rollout file
    pub fn compact(
        input_path: &Path,
        max_items: usize,
    ) -> anyhow::Result<CompactionReport> {
        // 1. Read all items from input
        let file = File::open(input_path)?;
        let reader = BufReader::new(file);
        let mut items: Vec<RolloutLine> = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let rollout_line: RolloutLine = serde_json::from_str(&line)?;
            items.push(rollout_line);
        }

        // 2. Check if compaction needed
        if items.len() < max_items {
            return Ok(CompactionReport {
                compacted: false,
                items_before: items.len(),
                items_after: items.len(),
            });
        }

        // 3. Group consecutive items by type
        let mut compacted_items = Vec::new();
        let mut current_group: Vec<RolloutLine> = Vec::new();
        let mut last_type = None;

        for item in items {
            let item_type = std::mem::discriminant(&item.item);
            
            if last_type == Some(item_type) {
                current_group.push(item);
            } else {
                // Flush previous group
                if !current_group.is_empty() {
                    if current_group.len() > 100 {
                        // Compact large groups
                        let summary = Self::summarize_group(&current_group);
                        compacted_items.push(RolloutLine {
                            timestamp: current_group.first().unwrap().timestamp,
                            item: RolloutItem::Compacted {
                                items_compacted: current_group.len() as u64,
                                summary,
                                token_estimate: Self::estimate_tokens(&current_group),
                                earliest_timestamp: current_group.first().unwrap().timestamp,
                                latest_timestamp: current_group.last().unwrap().timestamp,
                            },
                        });
                    } else {
                        // Keep small groups as-is
                        compacted_items.extend(current_group);
                    }
                }
                current_group = vec![item];
                last_type = Some(item_type);
            }
        }

        // Flush final group
        if !current_group.is_empty() {
            compacted_items.extend(current_group);
        }

        // 4. Write compacted rollout to new file
        let output_path = input_path.with_extension("jsonl.compacted");
        let output_file = File::create(&output_path)?;
        let mut writer = BufWriter::new(output_file);

        for item in compacted_items {
            let json = serde_json::to_string(&item)?;
            writeln!(writer, "{}", json)?;
        }
        writer.flush()?;

        // 5. Atomic rename
        fs::rename(&output_path, input_path)?;

        Ok(CompactionReport {
            compacted: true,
            items_before: items.len(),
            items_after: compacted_items.len(),
        })
    }

    fn summarize_group(items: &[RolloutLine]) -> String {
        // Generate a summary of the grouped items
        let first = items.first().unwrap();
        let last = items.last().unwrap();
        format!(
            "{:?} items from {} to {}",
            items.len(),
            first.timestamp.format("%H:%M:%S"),
            last.timestamp.format("%H:%M:%S")
        )
    }

    fn estimate_tokens(items: &[RolloutLine]) -> u64 {
        // Rough token estimation (4 chars per token)
        let total_chars: usize = items
            .iter()
            .map(|item| {
                serde_json::to_string(item)
                    .unwrap_or_default()
                    .len()
            })
            .sum();
        (total_chars / 4) as u64
    }
}

#[derive(Debug, Clone)]
pub struct CompactionReport {
    pub compacted: bool,
    pub items_before: usize,
    pub items_after: usize,
}
```

## Integration Points

### Phase 1 EventMsg Integration

`RolloutItem` extends Phase 1's `EventMsg` with additional metadata items:

| EventMsg Variant | RolloutItem Variant | Notes |
|-----------------|---------------------|-------|
| `TextDelta` | `TextDelta` | Direct mapping |
| `ThinkingDelta` | `ThinkingDelta` | Direct mapping |
| `ThinkingBlockCompleted` | `ThinkingBlockCompleted` | Direct mapping |
| `ToolCallStarted` | `ToolCallStarted` | Direct mapping |
| `ToolExecCompleted` | `ToolExecCompleted` | Direct mapping (with output truncation) |
| `ApprovalRequired` | `ApprovalRequired` | Direct mapping |
| `PlanCreated` | `PlanCreated` | Direct mapping |
| `Workspace` | `Workspace` | Direct mapping |
| `Command` | `Command` | Direct mapping |
| `MilestoneProgress` | `MilestoneProgress` | Direct mapping |
| — | `SessionMeta` | **NEW** — Session initialization |
| — | `TurnContext` | **NEW** — Turn-level metadata |
| — | `Compacted` | **NEW** — Compaction marker |
| — | `ForkedFrom` | **NEW** — Thread fork marker |

### Event Bus Integration

The existing `EventSubscriber` (from `rustycode-storage`) will be extended to write to `RolloutRecorder`:

```rust
pub struct RolloutEventSubscriber {
    recorder: Arc<Mutex<RolloutRecorder>>,
}

impl EventHandler for RolloutEventSubscriber {
    fn handle(&self, event: &dyn Any) -> anyhow::Result<()> {
        if let Some(event) = event.downcast_ref::<TextDeltaEvent>() {
            let item = RolloutItem::TextDelta {
                delta: event.delta.clone(),
            };
            self.recorder.lock().unwrap().write_item(item)?;
        }
        // ... handle other event types
        Ok(())
    }
}
```

## Crate Changes

### rustycode-session

**New types:**

```rust
// src/rollout.rs
pub mod rollout;

pub use rollout::{
    RolloutItem, RolloutLine, RolloutRecorder, RolloutPath,
    SessionReplayer, ReplayedSession, ThreadForker, ThreadId,
    RolloutCompactor, CompactionReport,
};

// src/thread.rs
pub mod thread;

pub use thread::{
    ThreadMetadata, ThreadStatus, BackfillWatermark, BackfillWorker,
    BackfillReport, BackfillLease,
};
```

**Modified types:**

```rust
// src/session.rs
pub struct Session {
    pub id: SessionId,
    pub rollout_path: Option<RolloutPath>,  // NEW
    // ... existing fields
}
```

### rustycode-storage

**New module:**

```rust
// src/state_runtime.rs
pub mod state_runtime;

pub use state_runtime::{
    StateRuntime, ThreadMetadata,
};

// src/backfill.rs
pub mod backfill;

pub use backfill::{
    BackfillWorker, BackfillReport, BackfillWatermark, BackfillLease,
};
```

**Modified modules:**

```rust
// src/lib.rs
pub use state_runtime::StateRuntime;
pub use backfill::BackfillWorker;
```

**SQLite schema version bump:**

```rust
// schema.rs
pub const SCHEMA_VERSION: i32 = 6;  // Was 5, now 6
```

## Migration Strategy

### Phase 1: Dual-Write (Week 1-2)

1. Add `RolloutRecorder` alongside existing `SessionSerializer`
2. Write to both JSONL and snapshot format
3. Verify data consistency between formats
4. Add feature flag to enable JSONL-only mode

### Phase 2: Read Migration (Week 3-4)

1. Update `SessionManager` to read from JSONL
2. Backfill existing sessions from snapshots
3. Add read repair on session load
4. Monitor for data inconsistencies

### Phase 3: Cleanup (Week 5-6)

1. Remove snapshot serialization code
2. Add migration script for legacy sessions
3. Update documentation
4. Release announcement

### Backwards Compatibility

- **Old snapshots** can be imported via migration script
- **SQLite schema** is additive (no breaking changes)
- **EventMsg protocol** remains unchanged
- **Feature flags** allow gradual rollout

## Success Criteria

### Functional

- [ ] All rollouts written to JSONL files
- [ ] SQLite threads table stays in sync with rollouts
- [ ] Session replay produces identical state
- [ ] Thread forking creates independent rollouts
- [ ] Backfill processes all historical rollouts
- [ ] Compaction reduces file size by >50%
- [ ] Read repair detects and fixes inconsistencies

### Performance

- [ ] Rollout write latency < 1ms (p99)
- [ ] SQLite update latency < 5ms (p99)
- [ ] Session replay < 100ms for 1000 items
- [ ] Backfill processes 200 files in < 30s
- [ ] Compaction completes in < 1s for 10k items

### Reliability

- [ ] Zero data loss in crash scenarios
- [ ] Lease system prevents concurrent backfills
- [ ] Watermark survives process restarts
- [ ] Filesystem errors are surfaced and retried

### Observability

- [ ] Metrics for rollout writes/reads
- [ ] Metrics for backfill progress
- [ ] Logs for compaction decisions
- [ ] Alerts for read repair failures

## Appendix

### File Size Estimates

| Session Type | Items | JSONL Size | Compacted Size |
|-------------|-------|-----------|----------------|
| Short (10 turns) | ~500 | ~50 KB | ~25 KB |
| Medium (100 turns) | ~5,000 | ~500 KB | ~100 KB |
| Long (1000 turns) | ~50,000 | ~5 MB | ~500 KB |

### SQLite Performance

| Operation | Latency (p99) | Throughput |
|-----------|---------------|------------|
| Create thread | 2ms | 500/s |
| Update thread | 1ms | 1000/s |
| Query recent (100) | 5ms | 200/s |
| Full-text search | 20ms | 50/s |

### Backfill Capacity

| Metric | Value |
|--------|-------|
| Batch size | 200 files |
| Files per hour | ~2,400 |
| Sessions per day | ~50,000 |
| Storage growth | ~25 GB/day |

---

**Next Steps:**

1. Review and approve this design document
2. Create implementation tasks in project tracker
3. Set up feature flags for gradual rollout
4. Begin Phase 1 implementation (dual-write)
