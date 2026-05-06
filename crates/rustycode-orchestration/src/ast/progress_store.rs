//! SQLite-backed progress store for AST (Adaptive Structured Thinking).
//!
//! Provides machine-readable task state storage that complements the markdown
//! ledger. Enables efficient querying and supports multi-agent coordination.
//!
//! Schema follows spec section 10.2: tasks, milestones, dependencies, events,
//! artifacts, and subagent runs.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::error::OrchestrationError;

// Record types

/// A single tracked task in the progress store.
#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub complexity: String,
    pub goal: String,
    pub current_phase: String,
    pub status: String,
    pub ledger_path: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A milestone within a task.
#[derive(Debug, Clone)]
pub struct MilestoneRecord {
    pub id: String,
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub owner: Option<String>,
    pub deliverable: Option<String>,
    pub ordinal: i64,
}

/// An event logged during task execution.
#[derive(Debug, Clone)]
pub struct EventRecord {
    pub id: String,
    pub task_id: String,
    pub phase: String,
    pub actor: String,
    pub event_type: String,
    pub summary: String,
    pub artifact_id: Option<String>,
    pub created_at: String,
}

/// An artifact produced or consumed during execution.
#[derive(Debug, Clone)]
pub struct ArtifactRecord {
    pub id: String,
    pub task_id: String,
    pub kind: String,
    pub path: Option<String>,
    pub content_hash: Option<String>,
    pub summary: Option<String>,
    pub created_at: String,
}

/// A subagent run record for delegated work.
#[derive(Debug, Clone)]
pub struct SubagentRunRecord {
    pub id: String,
    pub task_id: String,
    pub role: String,
    pub input_artifact_id: Option<String>,
    pub output_artifact_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
}

// Legal state transitions

/// Linear phase progression for the AST pipeline.
const PHASE_ORDER: &[&str] = &[
    "CLASSIFY", "RESEARCH", "SKELETON", "EXPAND", "EXECUTE", "VERIFY", "DONE",
];

/// Validate that a phase transition is legal.
///
/// Legal transitions:
/// - Forward along the linear pipeline: CLASSIFY -> RESEARCH -> ... -> DONE
/// - Any phase to BLOCKED
/// - BLOCKED to EXPAND (foreman replan)
/// - Same phase (no-op)
pub fn validate_phase_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    if to == "BLOCKED" {
        return true;
    }
    if from == "BLOCKED" && to == "EXPAND" {
        return true;
    }
    let from_idx = PHASE_ORDER.iter().position(|&p| p == from);
    let to_idx = PHASE_ORDER.iter().position(|&p| p == to);
    match (from_idx, to_idx) {
        (Some(fi), Some(ti)) => ti > fi,
        _ => false,
    }
}

/// Validate that a milestone status transition is legal.
///
/// Legal transitions:
/// - pending -> active
/// - pending -> dropped
/// - active -> done
/// - active -> blocked
/// - Same status (no-op)
pub fn validate_milestone_transition(from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    match from {
        "pending" => matches!(to, "active" | "dropped"),
        "active" => matches!(to, "done" | "blocked"),
        _ => false,
    }
}

// Schema DDL

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  complexity TEXT NOT NULL,
  goal TEXT NOT NULL,
  current_phase TEXT NOT NULL,
  status TEXT NOT NULL,
  ledger_path TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS milestones (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  owner TEXT,
  deliverable TEXT,
  ordinal INTEGER NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);

CREATE TABLE IF NOT EXISTS milestone_dependencies (
  task_id TEXT NOT NULL,
  milestone_id TEXT NOT NULL,
  depends_on_milestone_id TEXT NOT NULL,
  PRIMARY KEY (milestone_id, depends_on_milestone_id),
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);

CREATE TABLE IF NOT EXISTS events (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  phase TEXT NOT NULL,
  actor TEXT NOT NULL,
  event_type TEXT NOT NULL,
  summary TEXT NOT NULL,
  artifact_id TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);

CREATE TABLE IF NOT EXISTS artifacts (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  path TEXT,
  content_hash TEXT,
  summary TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);

CREATE TABLE IF NOT EXISTS subagent_runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  role TEXT NOT NULL,
  input_artifact_id TEXT,
  output_artifact_id TEXT,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  FOREIGN KEY(task_id) REFERENCES tasks(id)
);

CREATE INDEX IF NOT EXISTS idx_milestones_task_status ON milestones(task_id, status);
CREATE INDEX IF NOT EXISTS idx_events_task_created ON events(task_id, created_at);
CREATE INDEX IF NOT EXISTS idx_artifacts_task_kind ON artifacts(task_id, kind);
";

// ProgressStore

/// SQLite-backed progress store for AST task state.
///
/// Complements the markdown ledger with machine-readable storage that supports
/// efficient querying and multi-agent coordination.
pub struct ProgressStore {
    conn: Connection,
}

impl ProgressStore {
    /// Open (or create) the progress store at the given path.
    ///
    /// Creates the database and runs schema migrations if the file does not
    /// exist yet.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open progress store at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .context("Failed to set SQLite pragmas")?;
        conn.execute_batch(SCHEMA_SQL)
            .context("Failed to initialize progress store schema")?;
        Ok(Self { conn })
    }

    /// Open an in-memory progress store (useful for testing).
    pub fn open_in_memory() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("Failed to create in-memory SQLite database")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .context("Failed to set SQLite pragmas")?;
        conn.execute_batch(SCHEMA_SQL)
            .context("Failed to initialize progress store schema")?;
        Ok(Self { conn })
    }

    // -- Task CRUD -----------------------------------------------------------

    /// Insert a new task record.
    pub fn create_task(&self, task: &TaskRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO tasks (id, title, complexity, goal, current_phase, status, ledger_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    task.id,
                    task.title,
                    task.complexity,
                    task.goal,
                    task.current_phase,
                    task.status,
                    task.ledger_path,
                    task.created_at,
                    task.updated_at,
                ],
            )
            .with_context(|| format!("Failed to create task {}", task.id))?;
        Ok(())
    }

    /// Retrieve a task by ID. Returns `None` if not found.
    pub fn task(&self, id: &str) -> Result<Option<TaskRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, title, complexity, goal, current_phase, status, ledger_path, created_at, updated_at FROM tasks WHERE id = ?1")
            .context("Failed to prepare task query")?;
        let mut rows = stmt
            .query(params![id])
            .context("Failed to execute task query")?;
        match rows.next()? {
            Some(row) => Ok(Some(TaskRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                complexity: row.get(2)?,
                goal: row.get(3)?,
                current_phase: row.get(4)?,
                status: row.get(5)?,
                ledger_path: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })),
            None => Ok(None),
        }
    }

    /// Update a task's current phase. Validates the transition first.
    pub fn update_task_phase(&self, id: &str, phase: &str) -> Result<()> {
        let task = self
            .task(id)?
            .with_context(|| format!("Task {id} not found for phase update"))?;
        if !validate_phase_transition(&task.current_phase, phase) {
            return Err(OrchestrationError::AstConfig {
                message: format!(
                    "Illegal phase transition: {} -> {} for task {id}",
                    task.current_phase, phase
                ),
            }
            .into());
        }
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE tasks SET current_phase = ?1, updated_at = ?2 WHERE id = ?3",
                params![phase, now, id],
            )
            .with_context(|| format!("Failed to update task {id} phase to {phase}"))?;
        Ok(())
    }

    /// List all tasks with a non-terminal status (not DONE, not CANCELLED).
    pub fn list_active_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, complexity, goal, current_phase, status, ledger_path, created_at, updated_at
                 FROM tasks
                 WHERE status NOT IN ('DONE', 'CANCELLED')
                 ORDER BY updated_at DESC",
            )
            .context("Failed to prepare active tasks query")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    complexity: row.get(2)?,
                    goal: row.get(3)?,
                    current_phase: row.get(4)?,
                    status: row.get(5)?,
                    ledger_path: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })
            .context("Failed to query active tasks")?;
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.context("Failed to read task row")?);
        }
        Ok(tasks)
    }

    // -- Milestone CRUD ------------------------------------------------------

    /// Insert a new milestone record.
    pub fn create_milestone(&self, m: &MilestoneRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO milestones (id, task_id, title, status, owner, deliverable, ordinal)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    m.id,
                    m.task_id,
                    m.title,
                    m.status,
                    m.owner,
                    m.deliverable,
                    m.ordinal,
                ],
            )
            .with_context(|| format!("Failed to create milestone {}", m.id))?;
        Ok(())
    }

    /// Retrieve all milestones for a task, ordered by ordinal.
    pub fn milestones_for_task(&self, task_id: &str) -> Result<Vec<MilestoneRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_id, title, status, owner, deliverable, ordinal
                 FROM milestones
                 WHERE task_id = ?1
                 ORDER BY ordinal",
            )
            .context("Failed to prepare milestones query")?;
        let rows = stmt
            .query_map(params![task_id], |row| {
                Ok(MilestoneRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    owner: row.get(4)?,
                    deliverable: row.get(5)?,
                    ordinal: row.get(6)?,
                })
            })
            .with_context(|| format!("Failed to query milestones for task {task_id}"))?;
        let mut milestones = Vec::new();
        for row in rows {
            milestones.push(row.context("Failed to read milestone row")?);
        }
        Ok(milestones)
    }

    /// Get the first milestone with status "active" for a given task.
    pub fn active_milestone(&self, task_id: &str) -> Result<Option<MilestoneRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_id, title, status, owner, deliverable, ordinal
                 FROM milestones
                 WHERE task_id = ?1 AND status = 'active'
                 ORDER BY ordinal
                 LIMIT 1",
            )
            .context("Failed to prepare active milestone query")?;
        let mut rows = stmt
            .query(params![task_id])
            .with_context(|| format!("Failed to query active milestone for task {task_id}"))?;
        match rows.next()? {
            Some(row) => Ok(Some(MilestoneRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                title: row.get(2)?,
                status: row.get(3)?,
                owner: row.get(4)?,
                deliverable: row.get(5)?,
                ordinal: row.get(6)?,
            })),
            None => Ok(None),
        }
    }

    /// Update a milestone's status. Validates the transition first.
    pub fn update_milestone_status(&self, id: &str, status: &str) -> Result<()> {
        let current: String = self
            .conn
            .query_row(
                "SELECT status FROM milestones WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .with_context(|| format!("Milestone {id} not found for status update"))?;
        if !validate_milestone_transition(&current, status) {
            return Err(OrchestrationError::AstConfig {
                message: format!(
                    "Illegal milestone transition: {current} -> {status} for milestone {id}"
                ),
            }
            .into());
        }
        self.conn
            .execute(
                "UPDATE milestones SET status = ?1 WHERE id = ?2",
                params![status, id],
            )
            .with_context(|| format!("Failed to update milestone {id} status to {status}"))?;
        Ok(())
    }

    // -- Dependency queries --------------------------------------------------

    /// Add a dependency edge: milestone `milestone_id` depends on `depends_on`.
    pub fn add_dependency(
        &self,
        task_id: &str,
        milestone_id: &str,
        depends_on: &str,
    ) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO milestone_dependencies (task_id, milestone_id, depends_on_milestone_id)
                 VALUES (?1, ?2, ?3)",
                params![task_id, milestone_id, depends_on],
            )
            .with_context(|| {
                format!(
                    "Failed to add dependency: {milestone_id} depends on {depends_on} (task {task_id})"
                )
            })?;
        Ok(())
    }

    /// Get all milestone IDs that `milestone_id` directly depends on.
    pub fn dependencies(&self, milestone_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT depends_on_milestone_id
                 FROM milestone_dependencies
                 WHERE milestone_id = ?1",
            )
            .context("Failed to prepare dependency query")?;
        let rows = stmt
            .query_map(params![milestone_id], |row| row.get(0))
            .with_context(|| {
                format!("Failed to query dependencies for milestone {milestone_id}")
            })?;
        let mut deps = Vec::new();
        for row in rows {
            deps.push(row.context("Failed to read dependency row")?);
        }
        Ok(deps)
    }

    /// Check the dependency graph for a task. Returns `true` if no cycles exist.
    ///
    /// Uses depth-first search to detect cycles in the milestone dependency
    /// graph. An empty dependency set or missing milestones are considered
    /// acyclic.
    pub fn check_dependency_graph(&self, task_id: &str) -> Result<bool> {
        // DFS cycle detection helper.
        fn visit(
            node: &str,
            adj: &HashMap<String, Vec<String>>,
            white: &mut HashSet<String>,
            gray: &mut HashSet<String>,
            black: &mut HashSet<String>,
        ) -> bool {
            white.remove(node);
            gray.insert(node.to_string());
            if let Some(neighbors) = adj.get(node) {
                for neighbor in neighbors {
                    if gray.contains(neighbor) {
                        return true; // Back edge found: cycle.
                    }
                    if !black.contains(neighbor)
                        && white.contains(neighbor)
                        && visit(neighbor, adj, white, gray, black)
                    {
                        return true;
                    }
                }
            }
            gray.remove(node);
            black.insert(node.to_string());
            false
        }

        // Collect all milestones for this task.
        let milestones = self.milestones_for_task(task_id)?;
        if milestones.is_empty() {
            return Ok(true);
        }

        let milestone_ids: HashSet<String> = milestones.iter().map(|m| m.id.clone()).collect();

        // Build adjacency list: node -> nodes it depends on.
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for mid in &milestone_ids {
            let deps = self.dependencies(mid)?;
            adj.insert(mid.clone(), deps);
        }

        let mut white: HashSet<String> = milestone_ids;
        let mut gray: HashSet<String> = HashSet::new();
        let mut black: HashSet<String> = HashSet::new();

        // Run DFS from every unvisited node.
        while let Some(start) = white.iter().next().cloned() {
            if visit(&start, &adj, &mut white, &mut gray, &mut black) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    // -- Event logging -------------------------------------------------------

    /// Append an event record.
    pub fn append_event(&self, event: &EventRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO events (id, task_id, phase, actor, event_type, summary, artifact_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.id,
                    event.task_id,
                    event.phase,
                    event.actor,
                    event.event_type,
                    event.summary,
                    event.artifact_id,
                    event.created_at,
                ],
            )
            .with_context(|| format!("Failed to append event {}", event.id))?;
        Ok(())
    }

    /// Retrieve the most recent events for a task, ordered by creation time
    /// descending (newest first).
    pub fn events(&self, task_id: &str, limit: usize) -> Result<Vec<EventRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_id, phase, actor, event_type, summary, artifact_id, created_at
                 FROM events
                 WHERE task_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .with_context(|| format!("Failed to prepare events query for task {task_id}"))?;
        let rows = stmt
            .query_map(
                params![task_id, i64::try_from(limit).unwrap_or(i64::MAX)],
                |row| {
                    Ok(EventRecord {
                        id: row.get(0)?,
                        task_id: row.get(1)?,
                        phase: row.get(2)?,
                        actor: row.get(3)?,
                        event_type: row.get(4)?,
                        summary: row.get(5)?,
                        artifact_id: row.get(6)?,
                        created_at: row.get(7)?,
                    })
                },
            )
            .with_context(|| format!("Failed to query events for task {task_id}"))?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row.context("Failed to read event row")?);
        }
        Ok(events)
    }

    // -- Artifact tracking ---------------------------------------------------

    /// Store an artifact record.
    pub fn store_artifact(&self, artifact: &ArtifactRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO artifacts (id, task_id, kind, path, content_hash, summary, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    artifact.id,
                    artifact.task_id,
                    artifact.kind,
                    artifact.path,
                    artifact.content_hash,
                    artifact.summary,
                    artifact.created_at,
                ],
            )
            .with_context(|| format!("Failed to store artifact {}", artifact.id))?;
        Ok(())
    }

    /// Retrieve all artifacts of a given kind for a task.
    pub fn artifacts_by_kind(&self, task_id: &str, kind: &str) -> Result<Vec<ArtifactRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_id, kind, path, content_hash, summary, created_at
                 FROM artifacts
                 WHERE task_id = ?1 AND kind = ?2
                 ORDER BY created_at",
            )
            .with_context(|| {
                format!("Failed to prepare artifacts query for task {task_id} kind {kind}")
            })?;
        let rows = stmt
            .query_map(params![task_id, kind], |row| {
                Ok(ArtifactRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    kind: row.get(2)?,
                    path: row.get(3)?,
                    content_hash: row.get(4)?,
                    summary: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })
            .with_context(|| format!("Failed to query artifacts for task {task_id}"))?;
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row.context("Failed to read artifact row")?);
        }
        Ok(artifacts)
    }

    // -- Subagent runs -------------------------------------------------------

    /// Record the start of a subagent run.
    pub fn start_subagent_run(&self, run: &SubagentRunRecord) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO subagent_runs (id, task_id, role, input_artifact_id, output_artifact_id, status, started_at, finished_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    run.id,
                    run.task_id,
                    run.role,
                    run.input_artifact_id,
                    run.output_artifact_id,
                    run.status,
                    run.started_at,
                    run.finished_at,
                ],
            )
            .with_context(|| format!("Failed to start subagent run {}", run.id))?;
        Ok(())
    }

    /// Mark a subagent run as finished with its output artifact.
    pub fn finish_subagent_run(&self, id: &str, output_artifact_id: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn
            .execute(
                "UPDATE subagent_runs SET status = 'done', output_artifact_id = ?1, finished_at = ?2 WHERE id = ?3",
                params![output_artifact_id, now, id],
            )
            .with_context(|| format!("Failed to finish subagent run {id}"))?;
        Ok(())
    }

    /// Retrieve the full subagent run history for a task, ordered by start time.
    pub fn subagent_history(&self, task_id: &str) -> Result<Vec<SubagentRunRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, task_id, role, input_artifact_id, output_artifact_id, status, started_at, finished_at
                 FROM subagent_runs
                 WHERE task_id = ?1
                 ORDER BY started_at",
            )
            .with_context(|| format!("Failed to prepare subagent history query for task {task_id}"))?;
        let rows = stmt
            .query_map(params![task_id], |row| {
                Ok(SubagentRunRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    role: row.get(2)?,
                    input_artifact_id: row.get(3)?,
                    output_artifact_id: row.get(4)?,
                    status: row.get(5)?,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                })
            })
            .with_context(|| format!("Failed to query subagent history for task {task_id}"))?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row.context("Failed to read subagent run row")?);
        }
        Ok(runs)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn now_rfc3339() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    fn new_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    fn sample_task() -> TaskRecord {
        TaskRecord {
            id: new_id(),
            title: "Implement feature X".into(),
            complexity: "Moderate".into(),
            goal: "Ship feature X with tests".into(),
            current_phase: "CLASSIFY".into(),
            status: "active".into(),
            ledger_path: "/tmp/ledger.md".into(),
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        }
    }

    // -- Schema creation tests -----------------------------------------------

    #[test]
    fn opens_and_creates_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("progress.db");
        let store = ProgressStore::open(&db_path).unwrap();
        drop(store);

        // Reopen should succeed without error (IF NOT EXISTS guards).
        let _store2 = ProgressStore::open(&db_path).unwrap();
        assert!(db_path.exists());
    }

    #[test]
    fn opens_in_memory() {
        let store = ProgressStore::open_in_memory().unwrap();
        drop(store);
    }

    // -- Task CRUD tests -----------------------------------------------------

    #[test]
    fn create_and_get_task() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let retrieved = store.task(&task.id).unwrap().unwrap();
        assert_eq!(retrieved.id, task.id);
        assert_eq!(retrieved.title, task.title);
        assert_eq!(retrieved.complexity, task.complexity);
        assert_eq!(retrieved.goal, task.goal);
        assert_eq!(retrieved.current_phase, task.current_phase);
        assert_eq!(retrieved.status, task.status);
        assert_eq!(retrieved.ledger_path, task.ledger_path);
    }

    #[test]
    fn get_task_returns_none_for_missing() {
        let store = ProgressStore::open_in_memory().unwrap();
        let result = store.task("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_task_phase_forward() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        store.update_task_phase(&task.id, "RESEARCH").unwrap();
        let updated = store.task(&task.id).unwrap().unwrap();
        assert_eq!(updated.current_phase, "RESEARCH");
    }

    #[test]
    fn update_task_phase_to_blocked() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        store.update_task_phase(&task.id, "BLOCKED").unwrap();
        let updated = store.task(&task.id).unwrap().unwrap();
        assert_eq!(updated.current_phase, "BLOCKED");
    }

    #[test]
    fn update_task_phase_blocked_to_expand() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mut task = sample_task();
        task.current_phase = "BLOCKED".into();
        store.create_task(&task).unwrap();

        store.update_task_phase(&task.id, "EXPAND").unwrap();
        let updated = store.task(&task.id).unwrap().unwrap();
        assert_eq!(updated.current_phase, "EXPAND");
    }

    #[test]
    fn update_task_phase_rejects_backward() {
        let store = ProgressStore::open_in_memory().unwrap();
        let mut task = sample_task();
        task.current_phase = "EXECUTE".into();
        store.create_task(&task).unwrap();

        let result = store.update_task_phase(&task.id, "RESEARCH");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Illegal phase transition"));
    }

    #[test]
    fn update_task_phase_rejects_nonexistent_task() {
        let store = ProgressStore::open_in_memory().unwrap();
        let result = store.update_task_phase("missing", "RESEARCH");
        assert!(result.is_err());
    }

    #[test]
    fn list_active_tasks_filters_done() {
        let store = ProgressStore::open_in_memory().unwrap();

        let mut task1 = sample_task();
        task1.status = "active".into();
        store.create_task(&task1).unwrap();

        let mut task2 = sample_task();
        task2.status = "DONE".into();
        store.create_task(&task2).unwrap();

        let mut task3 = sample_task();
        task3.status = "CANCELLED".into();
        store.create_task(&task3).unwrap();

        let active = store.list_active_tasks().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, task1.id);
    }

    // -- Milestone CRUD tests ------------------------------------------------

    #[test]
    fn create_and_get_milestones() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m1 = MilestoneRecord {
            id: new_id(),
            task_id: task.id.clone(),
            title: "Setup module".into(),
            status: "pending".into(),
            owner: Some("agent-1".into()),
            deliverable: Some("module stubs".into()),
            ordinal: 1,
        };
        let m2 = MilestoneRecord {
            id: new_id(),
            task_id: task.id.clone(),
            title: "Implement logic".into(),
            status: "pending".into(),
            owner: None,
            deliverable: Some("working code".into()),
            ordinal: 2,
        };
        store.create_milestone(&m1).unwrap();
        store.create_milestone(&m2).unwrap();

        let milestones = store.milestones_for_task(&task.id).unwrap();
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[0].title, "Setup module");
        assert_eq!(milestones[1].title, "Implement logic");
    }

    #[test]
    fn get_active_milestone_returns_first_active() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m1 = MilestoneRecord {
            id: new_id(),
            task_id: task.id.clone(),
            title: "Done milestone".into(),
            status: "done".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        let m2 = MilestoneRecord {
            id: new_id(),
            task_id: task.id.clone(),
            title: "Active milestone".into(),
            status: "active".into(),
            owner: None,
            deliverable: None,
            ordinal: 2,
        };
        store.create_milestone(&m1).unwrap();
        store.create_milestone(&m2).unwrap();

        let active = store.active_milestone(&task.id).unwrap().unwrap();
        assert_eq!(active.title, "Active milestone");
    }

    #[test]
    fn get_active_milestone_returns_none_when_no_active() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let result = store.active_milestone(&task.id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn update_milestone_status_legal_transitions() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m = MilestoneRecord {
            id: new_id(),
            task_id: task.id,
            title: "Work item".into(),
            status: "pending".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        store.create_milestone(&m).unwrap();

        // pending -> active
        store.update_milestone_status(&m.id, "active").unwrap();
        // active -> done
        store.update_milestone_status(&m.id, "done").unwrap();
    }

    #[test]
    fn update_milestone_status_pending_to_dropped() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m = MilestoneRecord {
            id: new_id(),
            task_id: task.id,
            title: "Optional work".into(),
            status: "pending".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        store.create_milestone(&m).unwrap();
        store.update_milestone_status(&m.id, "dropped").unwrap();
    }

    #[test]
    fn update_milestone_status_active_to_blocked() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m = MilestoneRecord {
            id: new_id(),
            task_id: task.id,
            title: "Blocked work".into(),
            status: "active".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        store.create_milestone(&m).unwrap();
        store.update_milestone_status(&m.id, "blocked").unwrap();
    }

    #[test]
    fn update_milestone_status_rejects_illegal() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m = MilestoneRecord {
            id: new_id(),
            task_id: task.id,
            title: "Work item".into(),
            status: "done".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        store.create_milestone(&m).unwrap();

        let result = store.update_milestone_status(&m.id, "pending");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Illegal milestone transition"));
    }

    // -- Dependency tests ----------------------------------------------------

    #[test]
    fn add_and_get_dependencies() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m1_id = new_id();
        let m2_id = new_id();

        let m1 = MilestoneRecord {
            id: m1_id.clone(),
            task_id: task.id.clone(),
            title: "First".into(),
            status: "pending".into(),
            owner: None,
            deliverable: None,
            ordinal: 1,
        };
        let m2 = MilestoneRecord {
            id: m2_id.clone(),
            task_id: task.id.clone(),
            title: "Second".into(),
            status: "pending".into(),
            owner: None,
            deliverable: None,
            ordinal: 2,
        };
        store.create_milestone(&m1).unwrap();
        store.create_milestone(&m2).unwrap();

        store.add_dependency(&task.id, &m2_id, &m1_id).unwrap();

        let deps = store.dependencies(&m2_id).unwrap();
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], m1_id);
    }

    #[allow(clippy::cast_possible_wrap)]
    #[test]
    fn check_dependency_graph_no_cycles() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m1_id = new_id();
        let m2_id = new_id();
        let m3_id = new_id();

        for (ordinal, (mid, title)) in [(&m1_id, "A"), (&m2_id, "B"), (&m3_id, "C")]
            .into_iter()
            .enumerate()
        {
            store
                .create_milestone(&MilestoneRecord {
                    id: mid.clone(),
                    task_id: task.id.clone(),
                    title: title.into(),
                    status: "pending".into(),
                    owner: None,
                    deliverable: None,
                    ordinal: ordinal as i64 + 1,
                })
                .unwrap();
        }

        // A -> B -> C (linear, no cycle)
        store.add_dependency(&task.id, &m2_id, &m1_id).unwrap();
        store.add_dependency(&task.id, &m3_id, &m2_id).unwrap();

        assert!(store.check_dependency_graph(&task.id).unwrap());
    }

    #[allow(clippy::cast_possible_wrap)]
    #[test]
    fn check_dependency_graph_detects_cycle() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let m1_id = new_id();
        let m2_id = new_id();
        let m3_id = new_id();

        for (ordinal, (mid, title)) in [(&m1_id, "A"), (&m2_id, "B"), (&m3_id, "C")]
            .into_iter()
            .enumerate()
        {
            store
                .create_milestone(&MilestoneRecord {
                    id: mid.clone(),
                    task_id: task.id.clone(),
                    title: title.into(),
                    status: "pending".into(),
                    owner: None,
                    deliverable: None,
                    ordinal: ordinal as i64 + 1,
                })
                .unwrap();
        }

        // A -> B -> C -> A (cycle)
        store.add_dependency(&task.id, &m2_id, &m1_id).unwrap();
        store.add_dependency(&task.id, &m3_id, &m2_id).unwrap();
        store.add_dependency(&task.id, &m1_id, &m3_id).unwrap();

        assert!(!store.check_dependency_graph(&task.id).unwrap());
    }

    #[test]
    fn check_dependency_graph_empty_is_acyclic() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        // No milestones at all.
        assert!(store.check_dependency_graph(&task.id).unwrap());
    }

    // -- Event tests ---------------------------------------------------------

    #[allow(clippy::collection_is_never_read)]
    #[test]
    fn append_and_get_events_ordered() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let mut events = Vec::new();
        for i in 0..5 {
            let evt = EventRecord {
                id: new_id(),
                task_id: task.id.clone(),
                phase: "EXECUTE".into(),
                actor: "agent-1".into(),
                event_type: "step_complete".into(),
                summary: format!("Completed step {i}"),
                artifact_id: None,
                created_at: now_rfc3339(),
            };
            store.append_event(&evt).unwrap();
            events.push(evt);
        }

        let retrieved = store.events(&task.id, 3).unwrap();
        // Should return the 3 most recent (last 3 inserted), newest first.
        assert_eq!(retrieved.len(), 3);
        // Most recent event should be "Completed step 4".
        assert_eq!(retrieved[0].summary, "Completed step 4");
        assert_eq!(retrieved[2].summary, "Completed step 2");
    }

    #[test]
    fn get_events_respects_limit() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let evt = EventRecord {
            id: new_id(),
            task_id: task.id.clone(),
            phase: "EXECUTE".into(),
            actor: "agent-1".into(),
            event_type: "step_complete".into(),
            summary: "Step done".into(),
            artifact_id: None,
            created_at: now_rfc3339(),
        };
        store.append_event(&evt).unwrap();

        let retrieved = store.events(&task.id, 10).unwrap();
        assert_eq!(retrieved.len(), 1);
    }

    // -- Artifact tests ------------------------------------------------------

    #[test]
    fn store_and_get_artifacts_by_kind() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let a1 = ArtifactRecord {
            id: new_id(),
            task_id: task.id.clone(),
            kind: "file".into(),
            path: Some("/src/main.rs".into()),
            content_hash: Some("abc123".into()),
            summary: Some("Entry point".into()),
            created_at: now_rfc3339(),
        };
        let a2 = ArtifactRecord {
            id: new_id(),
            task_id: task.id.clone(),
            kind: "test_result".into(),
            path: None,
            content_hash: None,
            summary: Some("All tests pass".into()),
            created_at: now_rfc3339(),
        };
        store.store_artifact(&a1).unwrap();
        store.store_artifact(&a2).unwrap();

        let files = store.artifacts_by_kind(&task.id, "file").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, Some("/src/main.rs".into()));

        let tests = store.artifacts_by_kind(&task.id, "test_result").unwrap();
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].summary, Some("All tests pass".into()));

        let empty = store.artifacts_by_kind(&task.id, "nonexistent").unwrap();
        assert!(empty.is_empty());
    }

    // -- Subagent run tests --------------------------------------------------

    #[test]
    fn start_and_finish_subagent_run() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        let run = SubagentRunRecord {
            id: new_id(),
            task_id: task.id.clone(),
            role: "coder".into(),
            input_artifact_id: Some(new_id()),
            output_artifact_id: None,
            status: "running".into(),
            started_at: now_rfc3339(),
            finished_at: None,
        };
        store.start_subagent_run(&run).unwrap();

        let output_id = new_id();
        store.finish_subagent_run(&run.id, &output_id).unwrap();

        let history = store.subagent_history(&task.id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, "done");
        assert_eq!(history[0].output_artifact_id, Some(output_id));
        assert!(history[0].finished_at.is_some());
    }

    #[test]
    fn get_subagent_history_multiple_runs() {
        let store = ProgressStore::open_in_memory().unwrap();
        let task = sample_task();
        store.create_task(&task).unwrap();

        for role in ["coder", "reviewer", "tester"] {
            let run = SubagentRunRecord {
                id: new_id(),
                task_id: task.id.clone(),
                role: role.into(),
                input_artifact_id: None,
                output_artifact_id: None,
                status: "running".into(),
                started_at: now_rfc3339(),
                finished_at: None,
            };
            store.start_subagent_run(&run).unwrap();
        }

        let history = store.subagent_history(&task.id).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].role, "coder");
        assert_eq!(history[1].role, "reviewer");
        assert_eq!(history[2].role, "tester");
    }

    // -- Phase transition validation tests -----------------------------------

    #[test]
    fn phase_transition_all_forward_legal() {
        assert!(validate_phase_transition("CLASSIFY", "RESEARCH"));
        assert!(validate_phase_transition("RESEARCH", "SKELETON"));
        assert!(validate_phase_transition("SKELETON", "EXPAND"));
        assert!(validate_phase_transition("EXPAND", "EXECUTE"));
        assert!(validate_phase_transition("EXECUTE", "VERIFY"));
        assert!(validate_phase_transition("VERIFY", "DONE"));
    }

    #[test]
    fn phase_transition_backward_illegal() {
        assert!(!validate_phase_transition("RESEARCH", "CLASSIFY"));
        assert!(!validate_phase_transition("EXECUTE", "SKELETON"));
        assert!(!validate_phase_transition("DONE", "EXPAND"));
    }

    #[test]
    fn phase_transition_to_blocked_from_any() {
        assert!(validate_phase_transition("CLASSIFY", "BLOCKED"));
        assert!(validate_phase_transition("EXECUTE", "BLOCKED"));
        assert!(validate_phase_transition("VERIFY", "BLOCKED"));
    }

    #[test]
    fn phase_transition_blocked_to_expand() {
        assert!(validate_phase_transition("BLOCKED", "EXPAND"));
    }

    #[test]
    fn phase_transition_blocked_to_others_illegal() {
        assert!(!validate_phase_transition("BLOCKED", "CLASSIFY"));
        assert!(!validate_phase_transition("BLOCKED", "RESEARCH"));
        assert!(!validate_phase_transition("BLOCKED", "EXECUTE"));
        assert!(!validate_phase_transition("BLOCKED", "DONE"));
    }

    #[test]
    fn phase_transition_same_phase_legal() {
        assert!(validate_phase_transition("CLASSIFY", "CLASSIFY"));
        assert!(validate_phase_transition("EXECUTE", "EXECUTE"));
    }

    #[test]
    fn phase_transition_unknown_phases_illegal() {
        assert!(!validate_phase_transition("UNKNOWN", "OTHER"));
    }

    // -- Milestone transition validation tests --------------------------------

    #[test]
    fn milestone_transition_pending_to_active() {
        assert!(validate_milestone_transition("pending", "active"));
    }

    #[test]
    fn milestone_transition_pending_to_dropped() {
        assert!(validate_milestone_transition("pending", "dropped"));
    }

    #[test]
    fn milestone_transition_active_to_done() {
        assert!(validate_milestone_transition("active", "done"));
    }

    #[test]
    fn milestone_transition_active_to_blocked() {
        assert!(validate_milestone_transition("active", "blocked"));
    }

    #[test]
    fn milestone_transition_illegal_ones() {
        assert!(!validate_milestone_transition("pending", "done"));
        assert!(!validate_milestone_transition("done", "active"));
        assert!(!validate_milestone_transition("dropped", "pending"));
        assert!(!validate_milestone_transition("blocked", "pending"));
        assert!(!validate_milestone_transition("done", "dropped"));
    }

    #[test]
    fn milestone_transition_same_status_legal() {
        assert!(validate_milestone_transition("pending", "pending"));
        assert!(validate_milestone_transition("active", "active"));
        assert!(validate_milestone_transition("done", "done"));
    }
}
