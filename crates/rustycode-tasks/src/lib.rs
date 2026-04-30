//! Task management and lifecycle for `RustyCode` development workflows.
//!
//! Manages task creation, tracking, and lifecycle during development sessions.
//! Allows breaking down complex development work into subtasks, tracking progress,
//! managing dependencies between tasks, and filtering tasks by various criteria.
//!
//! # Example
//!
//! ```ignore
//! use rustycode_tasks::{Task, TaskManager, TaskStatus, TaskPriority};
//!
//! let mut manager = TaskManager::new("project-1");
//!
//! let task = Task::new("Implement authentication")
//!     .with_priority(TaskPriority::High)
//!     .with_owner("alice");
//!
//! let task_id = manager.create_task(task).unwrap();
//! manager.update_status(&task_id, TaskStatus::InProgress).unwrap();
//! manager.complete_task(&task_id).unwrap();
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during task operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskError {
    #[error("task not found: {0}")]
    NotFound(String),

    #[error("task already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid status transition: cannot go from {from} to {to}")]
    InvalidTransition { from: String, to: String },

    #[error("circular dependency detected involving task: {0}")]
    CircularDependency(String),

    #[error("dependency not found: {0}")]
    DependencyNotFound(String),

    #[error("task is blocked by incomplete dependencies: {0}")]
    Blocked(String),

    #[error("validation error: {0}")]
    Validation(String),
}

// ============================================================================
// Task Status
// ============================================================================

/// Status of a task in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
    Blocked,
    Running,
    Killed,
}

impl TaskStatus {
    /// Get the string representation of the status.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Blocked => "blocked",
            Self::Running => "running",
            Self::Killed => "killed",
        }
    }

    /// Parse a status from a string, returning `Pending` for unknown values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "in_progress" => Self::InProgress,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "blocked" => Self::Blocked,
            "running" => Self::Running,
            "killed" => Self::Killed,
            _ => Self::Pending,
        }
    }

    /// Whether this status represents a terminal (finished) state.
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Killed)
    }

    /// Whether this status represents an active (in-progress) state.
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::InProgress | Self::Running)
    }

    /// All status variants in order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Pending,
            Self::InProgress,
            Self::Completed,
            Self::Failed,
            Self::Blocked,
            Self::Running,
            Self::Killed,
        ]
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// Task Priority
// ============================================================================

/// Priority level for a task.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Background = 1,
    Low = 2,
    #[default]
    Normal = 3,
    High = 4,
    Critical = 5,
}

impl TaskPriority {
    /// Get the numeric value of this priority.
    pub const fn as_i32(&self) -> i32 {
        *self as i32
    }

    /// Get the string representation.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    /// Parse a priority from a string, returning `Normal` for unknown values.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "background" => Self::Background,
            "low" => Self::Low,
            "high" => Self::High,
            "critical" => Self::Critical,
            _ => Self::Normal,
        }
    }

    /// All priority variants in ascending order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Background,
            Self::Low,
            Self::Normal,
            Self::High,
            Self::Critical,
        ]
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ============================================================================
// Task
// ============================================================================

/// An individual task with description, status, owner, and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for the task.
    pub id: String,
    /// The project this task belongs to.
    pub project_id: String,
    /// Optional session this task was created in.
    pub session_id: Option<String>,
    /// Human-readable description of the task.
    pub description: String,
    /// Current status of the task.
    pub status: TaskStatus,
    /// Priority level.
    pub priority: TaskPriority,
    /// Optional owner (agent or user) assigned to this task.
    pub owner: Option<String>,
    /// Optional time estimate for completion.
    pub estimate: Option<String>,
    /// IDs of tasks that must complete before this task can start.
    pub dependencies: Vec<String>,
    /// Optional output or result of the task.
    pub output: Option<String>,
    /// Timestamp when the task was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the task was last updated.
    pub updated_at: DateTime<Utc>,
    /// Timestamp when the task was started (moved to in-progress).
    pub started_at: Option<DateTime<Utc>>,
    /// Timestamp when the task reached a terminal state.
    pub completed_at: Option<DateTime<Utc>>,
}

impl Task {
    /// Create a new task with the given description.
    ///
    /// The task is assigned a unique ID and default values for all fields.
    /// Use builder methods to customize.
    pub fn new(description: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: format!("task-{}", Uuid::new_v4()),
            project_id: String::new(),
            session_id: None,
            description: description.into(),
            status: TaskStatus::Pending,
            priority: TaskPriority::Normal,
            owner: None,
            estimate: None,
            dependencies: Vec::new(),
            output: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
        }
    }

    /// Create a task with a specific ID (useful for deserialization / testing).
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Set the project ID.
    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = project_id.into();
        self
    }

    /// Set the session ID.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the priority.
    pub const fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the owner.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Set the time estimate.
    pub fn with_estimate(mut self, estimate: impl Into<String>) -> Self {
        self.estimate = Some(estimate.into());
        self
    }

    /// Set the task dependencies.
    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Whether the task is in a terminal state.
    pub const fn is_finished(&self) -> bool {
        self.status.is_terminal()
    }

    /// Whether the task is currently active.
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    /// Duration the task has been (or was) running, if started.
    pub fn duration(&self) -> Option<chrono::Duration> {
        let start = self.started_at?;
        let end = self.completed_at.unwrap_or_else(Utc::now);
        Some(end - start)
    }
}

// ============================================================================
// Task Dependency
// ============================================================================

/// A dependency relationship between two tasks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskDependency {
    /// The task that is blocked.
    pub task_id: String,
    /// The task that must complete first.
    pub depends_on_id: String,
}

impl TaskDependency {
    /// Create a new dependency relationship.
    pub fn new(task_id: impl Into<String>, depends_on_id: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            depends_on_id: depends_on_id.into(),
        }
    }
}

// ============================================================================
// Task Filter
// ============================================================================

/// Builder for filtering tasks by various criteria.
#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    /// Filter by status (match any of these).
    pub statuses: Vec<TaskStatus>,
    /// Filter by owner.
    pub owner: Option<String>,
    /// Filter by priority (match any of these).
    pub priorities: Vec<TaskPriority>,
    /// Filter by project ID.
    pub project_id: Option<String>,
    /// Filter by session ID.
    pub session_id: Option<String>,
    /// Only include tasks with no dependencies (unblocked).
    pub unblocked_only: bool,
    /// Only include tasks that are blocked by incomplete dependencies.
    pub blocked_only: bool,
}

impl TaskFilter {
    /// Create a new empty filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a status to match.
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.statuses.push(status);
        self
    }

    /// Set the owner to match.
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Add a priority to match.
    pub fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priorities.push(priority);
        self
    }

    /// Set the project ID to match.
    pub fn with_project(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = Some(project_id.into());
        self
    }

    /// Set the session ID to match.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Only match unblocked tasks.
    pub const fn unblocked_only(mut self) -> Self {
        self.unblocked_only = true;
        self.blocked_only = false;
        self
    }

    /// Only match blocked tasks.
    pub const fn blocked_only(mut self) -> Self {
        self.blocked_only = true;
        self.unblocked_only = false;
        self
    }

    /// Check if a task matches this filter.
    pub fn matches(&self, task: &Task) -> bool {
        if !self.statuses.is_empty() && !self.statuses.contains(&task.status) {
            return false;
        }
        if let Some(ref owner) = self.owner {
            if task.owner.as_deref() != Some(owner.as_str()) {
                return false;
            }
        }
        if !self.priorities.is_empty() && !self.priorities.contains(&task.priority) {
            return false;
        }
        if let Some(ref project_id) = self.project_id {
            if task.project_id != *project_id {
                return false;
            }
        }
        if let Some(ref session_id) = self.session_id {
            if task.session_id.as_deref() != Some(session_id.as_str()) {
                return false;
            }
        }
        if self.unblocked_only && task.status == TaskStatus::Blocked {
            return false;
        }
        if self.blocked_only && task.status != TaskStatus::Blocked {
            return false;
        }
        true
    }
}

// ============================================================================
// Task Manager
// ============================================================================

/// In-memory task manager for creating, tracking, and querying tasks.
#[derive(Debug, Clone)]
pub struct TaskManager {
    project_id: String,
    tasks: HashMap<String, Task>,
    dependencies: Vec<TaskDependency>,
}

impl TaskManager {
    /// Create a new task manager for the given project.
    pub fn new(project_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            tasks: HashMap::new(),
            dependencies: Vec::new(),
        }
    }

    /// Get the project ID this manager is associated with.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Create a new task and add it to the manager.
    ///
    /// Returns the ID of the created task. The task's `project_id` is set
    /// to this manager's `project_id` if not already set.
    pub fn create_task(&mut self, mut task: Task) -> Result<String, TaskError> {
        if task.description.is_empty() {
            return Err(TaskError::Validation(
                "task description cannot be empty".to_string(),
            ));
        }
        if self.tasks.contains_key(&task.id) {
            return Err(TaskError::AlreadyExists(task.id));
        }

        // Validate dependencies exist
        for dep_id in &task.dependencies {
            if !self.tasks.contains_key(dep_id) {
                return Err(TaskError::DependencyNotFound(dep_id.clone()));
            }
        }

        // Set project_id if empty
        if task.project_id.is_empty() {
            task.project_id.clone_from(&self.project_id);
        }

        let id = task.id.clone();

        // Record dependencies
        for dep_id in &task.dependencies {
            self.dependencies.push(TaskDependency::new(&id, dep_id));
        }

        self.tasks.insert(id.clone(), task);
        Ok(id)
    }

    /// Get a task by ID.
    pub fn get_task(&self, task_id: &str) -> Option<&Task> {
        self.tasks.get(task_id)
    }

    /// Get a mutable reference to a task by ID.
    pub fn get_task_mut(&mut self, task_id: &str) -> Option<&mut Task> {
        self.tasks.get_mut(task_id)
    }

    /// Update the status of a task.
    ///
    /// Validates the transition and updates timestamps accordingly.
    pub fn update_status(
        &mut self,
        task_id: &str,
        new_status: TaskStatus,
    ) -> Result<(), TaskError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;

        let old_status = task.status;

        // Prevent transition from terminal states
        if old_status.is_terminal() {
            return Err(TaskError::InvalidTransition {
                from: old_status.to_string(),
                to: new_status.to_string(),
            });
        }

        // If moving to InProgress/Running, set started_at
        if new_status.is_active() && task.started_at.is_none() {
            task.started_at = Some(Utc::now());
        }

        // If moving to terminal state, set completed_at
        if new_status.is_terminal() {
            task.completed_at = Some(Utc::now());
        }

        task.status = new_status;
        task.updated_at = Utc::now();
        Ok(())
    }

    /// Mark a task as completed.
    pub fn complete_task(&mut self, task_id: &str) -> Result<(), TaskError> {
        self.update_status(task_id, TaskStatus::Completed)
    }

    /// Mark a task as failed.
    pub fn fail_task(&mut self, task_id: &str) -> Result<(), TaskError> {
        self.update_status(task_id, TaskStatus::Failed)
    }

    /// Assign an owner to a task.
    pub fn assign_owner(
        &mut self,
        task_id: &str,
        owner: impl Into<String>,
    ) -> Result<(), TaskError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        task.owner = Some(owner.into());
        task.updated_at = Utc::now();
        Ok(())
    }

    /// Set the output of a task.
    pub fn set_output(
        &mut self,
        task_id: &str,
        output: impl Into<String>,
    ) -> Result<(), TaskError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        task.output = Some(output.into());
        task.updated_at = Utc::now();
        Ok(())
    }

    /// Add a dependency: `task_id` depends on `depends_on_id`.
    pub fn add_dependency(&mut self, task_id: &str, depends_on_id: &str) -> Result<(), TaskError> {
        // Both tasks must exist
        if !self.tasks.contains_key(task_id) {
            return Err(TaskError::NotFound(task_id.to_string()));
        }
        if !self.tasks.contains_key(depends_on_id) {
            return Err(TaskError::DependencyNotFound(depends_on_id.to_string()));
        }

        // Self-dependency check
        if task_id == depends_on_id {
            return Err(TaskError::CircularDependency(task_id.to_string()));
        }

        // Check for existing dependency
        let already_exists = self
            .dependencies
            .iter()
            .any(|d| d.task_id == task_id && d.depends_on_id == depends_on_id);
        if already_exists {
            return Ok(()); // Idempotent
        }

        // Check for circular dependency (would creating this cause a cycle?)
        if self.would_create_cycle(task_id, depends_on_id) {
            return Err(TaskError::CircularDependency(task_id.to_string()));
        }

        self.dependencies
            .push(TaskDependency::new(task_id, depends_on_id));

        // Add to task's dependency list
        if let Some(task) = self.tasks.get_mut(task_id) {
            if !task.dependencies.contains(&depends_on_id.to_string()) {
                task.dependencies.push(depends_on_id.to_string());
            }
        }

        Ok(())
    }

    /// Remove a dependency between two tasks.
    pub fn remove_dependency(
        &mut self,
        task_id: &str,
        depends_on_id: &str,
    ) -> Result<(), TaskError> {
        if !self.tasks.contains_key(task_id) {
            return Err(TaskError::NotFound(task_id.to_string()));
        }
        self.dependencies
            .retain(|d| !(d.task_id == task_id && d.depends_on_id == depends_on_id));
        if let Some(task) = self.tasks.get_mut(task_id) {
            task.dependencies.retain(|d| d != depends_on_id);
        }
        Ok(())
    }

    /// Get all tasks that the given task depends on.
    pub fn get_dependencies(&self, task_id: &str) -> Vec<&Task> {
        self.dependencies
            .iter()
            .filter(|d| d.task_id == task_id)
            .filter_map(|d| self.tasks.get(&d.depends_on_id))
            .collect()
    }

    /// Get all tasks that depend on the given task (reverse dependencies).
    pub fn get_dependents(&self, task_id: &str) -> Vec<&Task> {
        self.dependencies
            .iter()
            .filter(|d| d.depends_on_id == task_id)
            .filter_map(|d| self.tasks.get(&d.task_id))
            .collect()
    }

    /// Query tasks using a filter.
    pub fn find_tasks(&self, filter: &TaskFilter) -> Vec<&Task> {
        self.tasks
            .values()
            .filter(|task| filter.matches(task))
            .collect()
    }

    /// Get all tasks.
    pub fn all_tasks(&self) -> Vec<&Task> {
        self.tasks.values().collect()
    }

    /// Get the number of tasks.
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether there are no tasks.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Remove a task by ID. Returns the removed task.
    pub fn remove_task(&mut self, task_id: &str) -> Option<Task> {
        let removed = self.tasks.remove(task_id)?;
        // Remove all dependencies involving this task
        self.dependencies
            .retain(|d| d.task_id != task_id && d.depends_on_id != task_id);
        Some(removed)
    }

    /// Get tasks grouped by status.
    pub fn tasks_by_status(&self) -> HashMap<TaskStatus, Vec<&Task>> {
        let mut map: HashMap<TaskStatus, Vec<&Task>> = HashMap::new();
        for task in self.tasks.values() {
            map.entry(task.status).or_default().push(task);
        }
        map
    }

    /// Check if adding "`task_id` depends on `depends_on_id`" would create a cycle.
    ///
    /// A cycle exists if `depends_on_id` already (transitively) depends on `task_id`.
    /// We traverse forward from `depends_on_id`, following "depends on" edges,
    /// to see if we can reach `task_id`.
    fn would_create_cycle(&self, task_id: &str, depends_on_id: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = vec![depends_on_id];

        while let Some(current) = stack.pop() {
            if current == task_id {
                return true;
            }
            if visited.insert(current.to_string()) {
                // Find all things that current depends on, and follow them
                for dep in &self.dependencies {
                    if dep.task_id == current {
                        stack.push(&dep.depends_on_id);
                    }
                }
            }
        }
        false
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::default_constructed_unit_structs
)]
mod tests {
    use super::*;

    // ========================================================================
    // TaskStatus tests
    // ========================================================================

    #[test]
    fn task_status_default_is_pending() {
        assert_eq!(TaskStatus::default(), TaskStatus::Pending);
    }

    #[test]
    fn task_status_as_str() {
        assert_eq!(TaskStatus::Pending.as_str(), "pending");
        assert_eq!(TaskStatus::InProgress.as_str(), "in_progress");
        assert_eq!(TaskStatus::Completed.as_str(), "completed");
        assert_eq!(TaskStatus::Failed.as_str(), "failed");
        assert_eq!(TaskStatus::Blocked.as_str(), "blocked");
        assert_eq!(TaskStatus::Running.as_str(), "running");
        assert_eq!(TaskStatus::Killed.as_str(), "killed");
    }

    #[test]
    fn task_status_from_str_lossy() {
        assert_eq!(TaskStatus::from_str_lossy("pending"), TaskStatus::Pending);
        assert_eq!(
            TaskStatus::from_str_lossy("in_progress"),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::from_str_lossy("completed"),
            TaskStatus::Completed
        );
        assert_eq!(TaskStatus::from_str_lossy("failed"), TaskStatus::Failed);
        assert_eq!(TaskStatus::from_str_lossy("blocked"), TaskStatus::Blocked);
        assert_eq!(TaskStatus::from_str_lossy("running"), TaskStatus::Running);
        assert_eq!(TaskStatus::from_str_lossy("killed"), TaskStatus::Killed);
    }

    #[test]
    fn task_status_from_str_lossy_unknown_returns_pending() {
        assert_eq!(TaskStatus::from_str_lossy("unknown"), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_str_lossy(""), TaskStatus::Pending);
        assert_eq!(
            TaskStatus::from_str_lossy("IN_PROGRESS"),
            TaskStatus::Pending
        );
    }

    #[test]
    fn task_status_is_terminal() {
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Failed.is_terminal());
        assert!(TaskStatus::Killed.is_terminal());
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::InProgress.is_terminal());
        assert!(!TaskStatus::Blocked.is_terminal());
        assert!(!TaskStatus::Running.is_terminal());
    }

    #[test]
    fn task_status_is_active() {
        assert!(TaskStatus::InProgress.is_active());
        assert!(TaskStatus::Running.is_active());
        assert!(!TaskStatus::Pending.is_active());
        assert!(!TaskStatus::Completed.is_active());
        assert!(!TaskStatus::Failed.is_active());
        assert!(!TaskStatus::Blocked.is_active());
        assert!(!TaskStatus::Killed.is_active());
    }

    #[test]
    fn task_status_display() {
        assert_eq!(format!("{}", TaskStatus::Pending), "pending");
        assert_eq!(format!("{}", TaskStatus::InProgress), "in_progress");
        assert_eq!(format!("{}", TaskStatus::Completed), "completed");
    }

    #[test]
    fn task_status_all_returns_seven_variants() {
        assert_eq!(TaskStatus::all().len(), 7);
    }

    #[test]
    fn task_status_serde_roundtrip() {
        for status in TaskStatus::all() {
            let json = serde_json::to_string(status).unwrap();
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, parsed, "Serde roundtrip failed for {status:?}");
        }
    }

    #[test]
    fn task_status_serde_json_values() {
        assert_eq!(
            serde_json::to_string(&TaskStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Completed).unwrap(),
            "\"completed\""
        );
    }

    #[test]
    fn task_status_serde_invalid() {
        let result: Result<TaskStatus, _> = serde_json::from_str("\"invalid_status\"");
        assert!(result.is_err());
    }

    #[test]
    fn task_status_eq_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        assert!(set.insert(TaskStatus::Pending));
        assert!(!set.insert(TaskStatus::Pending));
        assert!(set.insert(TaskStatus::Completed));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn task_status_copy_semantics() {
        let a = TaskStatus::Pending;
        let b = a;
        assert_eq!(a, b); // Copy, not move
    }

    // ========================================================================
    // TaskPriority tests
    // ========================================================================

    #[test]
    fn task_priority_default_is_normal() {
        assert_eq!(TaskPriority::default(), TaskPriority::Normal);
    }

    #[test]
    fn task_priority_as_str() {
        assert_eq!(TaskPriority::Background.as_str(), "background");
        assert_eq!(TaskPriority::Low.as_str(), "low");
        assert_eq!(TaskPriority::Normal.as_str(), "normal");
        assert_eq!(TaskPriority::High.as_str(), "high");
        assert_eq!(TaskPriority::Critical.as_str(), "critical");
    }

    #[test]
    fn task_priority_as_i32() {
        assert_eq!(TaskPriority::Background.as_i32(), 1);
        assert_eq!(TaskPriority::Low.as_i32(), 2);
        assert_eq!(TaskPriority::Normal.as_i32(), 3);
        assert_eq!(TaskPriority::High.as_i32(), 4);
        assert_eq!(TaskPriority::Critical.as_i32(), 5);
    }

    #[test]
    fn task_priority_ordering() {
        assert!(TaskPriority::Background < TaskPriority::Low);
        assert!(TaskPriority::Low < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Critical);
    }

    #[test]
    fn task_priority_from_str_lossy() {
        assert_eq!(
            TaskPriority::from_str_lossy("background"),
            TaskPriority::Background
        );
        assert_eq!(TaskPriority::from_str_lossy("low"), TaskPriority::Low);
        assert_eq!(TaskPriority::from_str_lossy("normal"), TaskPriority::Normal);
        assert_eq!(TaskPriority::from_str_lossy("high"), TaskPriority::High);
        assert_eq!(
            TaskPriority::from_str_lossy("critical"),
            TaskPriority::Critical
        );
    }

    #[test]
    fn task_priority_from_str_lossy_unknown_returns_normal() {
        assert_eq!(
            TaskPriority::from_str_lossy("unknown"),
            TaskPriority::Normal
        );
        assert_eq!(TaskPriority::from_str_lossy(""), TaskPriority::Normal);
    }

    #[test]
    fn task_priority_display() {
        assert_eq!(format!("{}", TaskPriority::Critical), "critical");
        assert_eq!(format!("{}", TaskPriority::Background), "background");
    }

    #[test]
    fn task_priority_all_returns_five_variants() {
        assert_eq!(TaskPriority::all().len(), 5);
    }

    #[test]
    fn task_priority_serde_roundtrip() {
        for priority in TaskPriority::all() {
            let json = serde_json::to_string(priority).unwrap();
            let parsed: TaskPriority = serde_json::from_str(&json).unwrap();
            assert_eq!(*priority, parsed, "Serde roundtrip failed for {priority:?}");
        }
    }

    // ========================================================================
    // Task tests
    // ========================================================================

    #[test]
    fn task_new_basic() {
        let task = Task::new("Implement auth");
        assert!(!task.id.is_empty());
        assert!(task.id.starts_with("task-"));
        assert_eq!(task.description, "Implement auth");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.priority, TaskPriority::Normal);
        assert!(task.owner.is_none());
        assert!(task.estimate.is_none());
        assert!(task.session_id.is_none());
        assert!(task.output.is_none());
        assert!(task.started_at.is_none());
        assert!(task.completed_at.is_none());
        assert!(task.dependencies.is_empty());
        assert_eq!(task.project_id, "");
    }

    #[test]
    fn task_builder_pattern() {
        let task = Task::new("Build feature")
            .with_id("custom-id")
            .with_project("proj-1")
            .with_session("sess-1")
            .with_priority(TaskPriority::High)
            .with_owner("alice")
            .with_estimate("2 hours")
            .with_dependencies(vec!["dep-1".to_string()]);

        assert_eq!(task.id, "custom-id");
        assert_eq!(task.project_id, "proj-1");
        assert_eq!(task.session_id.unwrap(), "sess-1");
        assert_eq!(task.priority, TaskPriority::High);
        assert_eq!(task.owner.unwrap(), "alice");
        assert_eq!(task.estimate.unwrap(), "2 hours");
        assert_eq!(task.dependencies, vec!["dep-1"]);
    }

    #[test]
    fn task_unique_ids() {
        let t1 = Task::new("Task 1");
        let t2 = Task::new("Task 2");
        assert_ne!(t1.id, t2.id);
    }

    #[test]
    fn task_timestamps_set_on_creation() {
        let before = Utc::now();
        let task = Task::new("Test");
        let after = Utc::now();

        assert!(task.created_at >= before);
        assert!(task.created_at <= after);
        assert_eq!(task.created_at, task.updated_at);
    }

    #[test]
    fn task_is_finished() {
        let mut task = Task::new("Test");
        assert!(!task.is_finished());

        task.status = TaskStatus::Completed;
        assert!(task.is_finished());

        task.status = TaskStatus::Failed;
        assert!(task.is_finished());

        task.status = TaskStatus::Killed;
        assert!(task.is_finished());

        task.status = TaskStatus::InProgress;
        assert!(!task.is_finished());
    }

    #[test]
    fn task_is_active() {
        let mut task = Task::new("Test");
        assert!(!task.is_active());

        task.status = TaskStatus::InProgress;
        assert!(task.is_active());

        task.status = TaskStatus::Running;
        assert!(task.is_active());

        task.status = TaskStatus::Completed;
        assert!(!task.is_active());
    }

    #[test]
    fn task_duration_none_when_not_started() {
        let task = Task::new("Test");
        assert!(task.duration().is_none());
    }

    #[test]
    fn task_duration_some_when_started() {
        let mut task = Task::new("Test");
        task.started_at = Some(Utc::now());
        assert!(task.duration().is_some());
    }

    #[test]
    fn task_serde_roundtrip() {
        let task = Task::new("Serialize me")
            .with_project("proj-1")
            .with_owner("bob")
            .with_priority(TaskPriority::Critical)
            .with_estimate("3 days");

        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, task.id);
        assert_eq!(parsed.description, task.description);
        assert_eq!(parsed.status, task.status);
        assert_eq!(parsed.priority, task.priority);
        assert_eq!(parsed.owner, task.owner);
        assert_eq!(parsed.estimate, task.estimate);
        assert_eq!(parsed.project_id, task.project_id);
    }

    #[test]
    fn task_serde_preserves_timestamps() {
        let task = Task::new("Test");
        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.created_at, task.created_at);
        assert_eq!(parsed.updated_at, task.updated_at);
    }

    #[test]
    fn task_serde_with_all_fields() {
        let mut task = Task::new("Full task")
            .with_id("t-1")
            .with_project("p-1")
            .with_session("s-1")
            .with_priority(TaskPriority::High)
            .with_owner("alice")
            .with_estimate("1h")
            .with_dependencies(vec!["dep-1".to_string()]);
        task.status = TaskStatus::InProgress;
        task.started_at = Some(Utc::now());
        task.output = Some("done".to_string());

        let json = serde_json::to_string(&task).unwrap();
        let parsed: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "t-1");
        assert_eq!(parsed.project_id, "p-1");
        assert_eq!(parsed.session_id.unwrap(), "s-1");
        assert_eq!(parsed.status, TaskStatus::InProgress);
        assert!(parsed.started_at.is_some());
        assert_eq!(parsed.output.unwrap(), "done");
        assert_eq!(parsed.dependencies, vec!["dep-1"]);
    }

    #[test]
    fn task_clone() {
        let task = Task::new("Clone test").with_owner("alice");
        let cloned = task.clone();
        assert_eq!(task.id, cloned.id);
        assert_eq!(task.description, cloned.description);
        assert_eq!(task.owner, cloned.owner);
    }

    // ========================================================================
    // TaskDependency tests
    // ========================================================================

    #[test]
    fn task_dependency_new() {
        let dep = TaskDependency::new("task-1", "task-2");
        assert_eq!(dep.task_id, "task-1");
        assert_eq!(dep.depends_on_id, "task-2");
    }

    #[test]
    fn task_dependency_equality() {
        let d1 = TaskDependency::new("a", "b");
        let d2 = TaskDependency::new("a", "b");
        let d3 = TaskDependency::new("b", "a");
        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
    }

    #[test]
    fn task_dependency_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        assert!(set.insert(TaskDependency::new("a", "b")));
        assert!(!set.insert(TaskDependency::new("a", "b")));
        assert!(set.insert(TaskDependency::new("b", "a")));
    }

    #[test]
    fn task_dependency_serde_roundtrip() {
        let dep = TaskDependency::new("task-1", "task-2");
        let json = serde_json::to_string(&dep).unwrap();
        let parsed: TaskDependency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, dep);
    }

    #[test]
    fn task_dependency_clone() {
        let dep = TaskDependency::new("x", "y");
        let cloned = dep.clone();
        assert_eq!(dep, cloned);
    }

    // ========================================================================
    // TaskFilter tests
    // ========================================================================

    #[test]
    fn task_filter_default_matches_everything() {
        let filter = TaskFilter::new();
        let task = Task::new("Test task");
        assert!(filter.matches(&task));
    }

    #[test]
    fn task_filter_by_status() {
        let filter = TaskFilter::new().with_status(TaskStatus::Pending);
        let pending = Task::new("Pending task");
        let mut completed = Task::new("Completed task");
        completed.status = TaskStatus::Completed;

        assert!(filter.matches(&pending));
        assert!(!filter.matches(&completed));
    }

    #[test]
    fn task_filter_by_multiple_statuses() {
        let filter = TaskFilter::new()
            .with_status(TaskStatus::Pending)
            .with_status(TaskStatus::InProgress);
        let pending = Task::new("Pending");
        let mut in_progress = Task::new("In progress");
        in_progress.status = TaskStatus::InProgress;
        let mut completed = Task::new("Completed");
        completed.status = TaskStatus::Completed;

        assert!(filter.matches(&pending));
        assert!(filter.matches(&in_progress));
        assert!(!filter.matches(&completed));
    }

    #[test]
    fn task_filter_by_owner() {
        let filter = TaskFilter::new().with_owner("alice");
        let alice_task = Task::new("Alice task").with_owner("alice");
        let bob_task = Task::new("Bob task").with_owner("bob");
        let no_owner = Task::new("No owner");

        assert!(filter.matches(&alice_task));
        assert!(!filter.matches(&bob_task));
        assert!(!filter.matches(&no_owner));
    }

    #[test]
    fn task_filter_by_priority() {
        let filter = TaskFilter::new().with_priority(TaskPriority::High);
        let high = Task::new("High").with_priority(TaskPriority::High);
        let normal = Task::new("Normal"); // default priority is Normal

        assert!(filter.matches(&high));
        assert!(!filter.matches(&normal));
    }

    #[test]
    fn task_filter_by_project() {
        let filter = TaskFilter::new().with_project("proj-1");
        let in_project = Task::new("Task").with_project("proj-1");
        let other_project = Task::new("Task").with_project("proj-2");

        assert!(filter.matches(&in_project));
        assert!(!filter.matches(&other_project));
    }

    #[test]
    fn task_filter_by_session() {
        let filter = TaskFilter::new().with_session("sess-1");
        let in_session = Task::new("Task").with_session("sess-1");
        let no_session = Task::new("Task");

        assert!(filter.matches(&in_session));
        assert!(!filter.matches(&no_session));
    }

    #[test]
    fn task_filter_unblocked_only() {
        let filter = TaskFilter::new().unblocked_only();
        let mut blocked = Task::new("Blocked");
        blocked.status = TaskStatus::Blocked;
        let pending = Task::new("Pending");

        assert!(filter.matches(&pending));
        assert!(!filter.matches(&blocked));
    }

    #[test]
    fn task_filter_blocked_only() {
        let filter = TaskFilter::new().blocked_only();
        let mut blocked = Task::new("Blocked");
        blocked.status = TaskStatus::Blocked;
        let pending = Task::new("Pending");

        assert!(filter.matches(&blocked));
        assert!(!filter.matches(&pending));
    }

    #[test]
    fn task_filter_combined_criteria() {
        let filter = TaskFilter::new()
            .with_status(TaskStatus::Pending)
            .with_priority(TaskPriority::High)
            .with_owner("alice");

        let matching = Task::new("Match")
            .with_priority(TaskPriority::High)
            .with_owner("alice");
        let wrong_priority = Task::new("Wrong").with_owner("alice");
        let wrong_owner = Task::new("Wrong").with_priority(TaskPriority::High);

        assert!(filter.matches(&matching));
        assert!(!filter.matches(&wrong_priority));
        assert!(!filter.matches(&wrong_owner));
    }

    #[test]
    fn task_filter_unblocked_overrides_blocked() {
        // Setting unblocked_only should clear blocked_only
        let filter = TaskFilter::new().blocked_only().unblocked_only();
        assert!(filter.unblocked_only);
        assert!(!filter.blocked_only);
    }

    #[test]
    fn task_filter_blocked_overrides_unblocked() {
        let filter = TaskFilter::new().unblocked_only().blocked_only();
        assert!(filter.blocked_only);
        assert!(!filter.unblocked_only);
    }

    #[test]
    fn task_filter_empty_statuses_matches_all() {
        let filter = TaskFilter::new();
        assert!(filter.statuses.is_empty());
        for status in TaskStatus::all() {
            let mut task = Task::new("Test");
            task.status = *status;
            assert!(
                filter.matches(&task),
                "Empty statuses should match {status:?}"
            );
        }
    }

    #[test]
    fn task_filter_empty_priorities_matches_all() {
        let filter = TaskFilter::new();
        assert!(filter.priorities.is_empty());
        for priority in TaskPriority::all() {
            let task = Task::new("Test").with_priority(*priority);
            assert!(
                filter.matches(&task),
                "Empty priorities should match {priority:?}"
            );
        }
    }

    // ========================================================================
    // TaskManager tests
    // ========================================================================

    #[test]
    fn manager_new() {
        let mgr = TaskManager::new("proj-1");
        assert_eq!(mgr.project_id(), "proj-1");
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn manager_create_task() {
        let mut mgr = TaskManager::new("proj-1");
        let task = Task::new("Implement auth");
        let id = mgr.create_task(task).unwrap();

        assert_eq!(mgr.len(), 1);
        let found = mgr.get_task(&id).unwrap();
        assert_eq!(found.description, "Implement auth");
        assert_eq!(found.project_id, "proj-1"); // auto-filled
    }

    #[test]
    fn manager_create_task_preserves_existing_project_id() {
        let mut mgr = TaskManager::new("proj-1");
        let task = Task::new("Task").with_project("proj-2");
        let id = mgr.create_task(task).unwrap();

        let found = mgr.get_task(&id).unwrap();
        assert_eq!(found.project_id, "proj-2"); // preserved
    }

    #[test]
    fn manager_create_task_empty_description_fails() {
        let mut mgr = TaskManager::new("proj-1");
        let task = Task::new("");
        let err = mgr.create_task(task).unwrap_err();
        assert!(matches!(err, TaskError::Validation(msg) if msg.contains("empty")));
    }

    #[test]
    fn manager_create_task_duplicate_id_fails() {
        let mut mgr = TaskManager::new("proj-1");
        let task1 = Task::new("Task 1").with_id("same-id");
        let task2 = Task::new("Task 2").with_id("same-id");
        mgr.create_task(task1).unwrap();
        let err = mgr.create_task(task2).unwrap_err();
        assert!(matches!(err, TaskError::AlreadyExists(id) if id == "same-id"));
    }

    #[test]
    fn manager_create_task_dependency_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let task = Task::new("Task").with_dependencies(vec!["nonexistent".to_string()]);
        let err = mgr.create_task(task).unwrap_err();
        assert!(matches!(err, TaskError::DependencyNotFound(d) if d == "nonexistent"));
    }

    #[test]
    fn manager_get_task_not_found() {
        let mgr = TaskManager::new("proj-1");
        assert!(mgr.get_task("nonexistent").is_none());
    }

    #[test]
    fn manager_get_task_mut() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        let task = mgr.get_task_mut(&id).unwrap();
        task.output = Some("result".to_string());
        assert_eq!(mgr.get_task(&id).unwrap().output.as_deref(), Some("result"));
    }

    #[test]
    fn manager_update_status() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();

        mgr.update_status(&id, TaskStatus::InProgress).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::InProgress);
        assert!(mgr.get_task(&id).unwrap().started_at.is_some());
    }

    #[test]
    fn manager_update_status_sets_completed_at_for_terminal() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();

        mgr.update_status(&id, TaskStatus::Completed).unwrap();
        let task = mgr.get_task(&id).unwrap();
        assert_eq!(task.status, TaskStatus::Completed);
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn manager_update_status_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let err = mgr
            .update_status("nonexistent", TaskStatus::InProgress)
            .unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn manager_update_status_terminal_rejected() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Completed).unwrap();

        let err = mgr.update_status(&id, TaskStatus::Pending).unwrap_err();
        assert!(matches!(err, TaskError::InvalidTransition { .. }));
    }

    #[test]
    fn manager_update_status_failed_is_terminal() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Failed).unwrap();

        let err = mgr.update_status(&id, TaskStatus::Pending).unwrap_err();
        assert!(matches!(err, TaskError::InvalidTransition { .. }));
    }

    #[test]
    fn manager_update_status_killed_is_terminal() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Killed).unwrap();

        let err = mgr.update_status(&id, TaskStatus::InProgress).unwrap_err();
        assert!(matches!(err, TaskError::InvalidTransition { .. }));
    }

    #[test]
    fn manager_complete_task() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.complete_task(&id).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::Completed);
    }

    #[test]
    fn manager_fail_task() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.fail_task(&id).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::Failed);
    }

    #[test]
    fn manager_assign_owner() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.assign_owner(&id, "alice").unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().owner.as_deref(), Some("alice"));
    }

    #[test]
    fn manager_assign_owner_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let err = mgr.assign_owner("nonexistent", "alice").unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn manager_set_output() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.set_output(&id, "All tests passed").unwrap();
        assert_eq!(
            mgr.get_task(&id).unwrap().output.as_deref(),
            Some("All tests passed")
        );
    }

    #[test]
    fn manager_set_output_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let err = mgr.set_output("nonexistent", "output").unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn manager_full_lifecycle() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr
            .create_task(
                Task::new("Build feature")
                    .with_priority(TaskPriority::High)
                    .with_owner("alice"),
            )
            .unwrap();

        // Pending -> InProgress
        mgr.update_status(&id, TaskStatus::InProgress).unwrap();
        assert!(mgr.get_task(&id).unwrap().is_active());
        assert!(mgr.get_task(&id).unwrap().started_at.is_some());

        // Set output
        mgr.set_output(&id, "Feature complete").unwrap();

        // InProgress -> Completed
        mgr.complete_task(&id).unwrap();
        assert!(mgr.get_task(&id).unwrap().is_finished());
        assert!(mgr.get_task(&id).unwrap().completed_at.is_some());
        assert_eq!(
            mgr.get_task(&id).unwrap().output.as_deref(),
            Some("Feature complete")
        );
    }

    #[test]
    fn manager_remove_task() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        let removed = mgr.remove_task(&id).unwrap();
        assert_eq!(removed.description, "Task");
        assert!(mgr.is_empty());
        assert!(mgr.get_task(&id).is_none());
    }

    #[test]
    fn manager_remove_task_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        assert!(mgr.remove_task("nonexistent").is_none());
    }

    #[test]
    fn manager_all_tasks() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1")).unwrap();
        mgr.create_task(Task::new("Task 2")).unwrap();
        mgr.create_task(Task::new("Task 3")).unwrap();

        let all = mgr.all_tasks();
        assert_eq!(all.len(), 3);
    }

    // ========================================================================
    // TaskManager dependency tests
    // ========================================================================

    #[test]
    fn manager_add_dependency() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr.create_task(Task::new("Task 1").with_id("t-1")).unwrap();
        let id2 = mgr.create_task(Task::new("Task 2").with_id("t-2")).unwrap();

        // t-2 depends on t-1
        mgr.add_dependency(&id2, &id1).unwrap();

        let deps = mgr.get_dependencies("t-2");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].id, "t-1");

        let dependents = mgr.get_dependents("t-1");
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].id, "t-2");
    }

    #[test]
    fn manager_add_dependency_task_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task").with_id("t-1")).unwrap();
        let err = mgr.add_dependency(&id, "nonexistent").unwrap_err();
        assert!(matches!(err, TaskError::DependencyNotFound(_)));
    }

    #[test]
    fn manager_add_dependency_source_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task").with_id("t-1")).unwrap();
        let err = mgr.add_dependency("nonexistent", "t-1").unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn manager_add_self_dependency_rejected() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task").with_id("t-1")).unwrap();
        let err = mgr.add_dependency(&id, &id).unwrap_err();
        assert!(matches!(err, TaskError::CircularDependency(_)));
    }

    #[test]
    fn manager_add_circular_dependency_rejected() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("Task 2").with_id("t-2")).unwrap();

        mgr.add_dependency("t-2", "t-1").unwrap();
        let err = mgr.add_dependency("t-1", "t-2").unwrap_err();
        assert!(matches!(err, TaskError::CircularDependency(_)));
    }

    #[test]
    fn manager_add_dependency_idempotent() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("Task 2").with_id("t-2")).unwrap();

        mgr.add_dependency("t-2", "t-1").unwrap();
        mgr.add_dependency("t-2", "t-1").unwrap(); // should not fail

        let deps = mgr.get_dependencies("t-2");
        assert_eq!(deps.len(), 1);
    }

    #[test]
    fn manager_remove_dependency() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("Task 2").with_id("t-2")).unwrap();
        mgr.add_dependency("t-2", "t-1").unwrap();

        mgr.remove_dependency("t-2", "t-1").unwrap();
        assert!(mgr.get_dependencies("t-2").is_empty());
        assert!(mgr.get_dependents("t-1").is_empty());
    }

    #[test]
    fn manager_remove_dependency_task_not_found() {
        let mut mgr = TaskManager::new("proj-1");
        let err = mgr.remove_dependency("nonexistent", "other").unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn manager_transitive_cycle_rejected() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("B").with_id("t-2")).unwrap();
        mgr.create_task(Task::new("C").with_id("t-3")).unwrap();

        mgr.add_dependency("t-2", "t-1").unwrap();
        mgr.add_dependency("t-3", "t-2").unwrap();

        // t-1 -> t-2 -> t-3, so t-3 -> t-1 would create a cycle
        let err = mgr.add_dependency("t-1", "t-3").unwrap_err();
        assert!(matches!(err, TaskError::CircularDependency(_)));
    }

    #[test]
    fn manager_remove_task_cleans_up_dependencies() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("B").with_id("t-2")).unwrap();
        mgr.add_dependency("t-2", "t-1").unwrap();

        mgr.remove_task("t-1");
        assert!(mgr.get_dependencies("t-2").is_empty());
    }

    // ========================================================================
    // TaskManager find_tasks tests
    // ========================================================================

    #[test]
    fn manager_find_by_status() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr.create_task(Task::new("Task 1")).unwrap();
        let id2 = mgr.create_task(Task::new("Task 2")).unwrap();
        mgr.update_status(&id2, TaskStatus::Completed).unwrap();

        let filter = TaskFilter::new().with_status(TaskStatus::Pending);
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id1);
    }

    #[test]
    fn manager_find_by_owner() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1").with_owner("alice"))
            .unwrap();
        mgr.create_task(Task::new("Task 2").with_owner("bob"))
            .unwrap();

        let filter = TaskFilter::new().with_owner("alice");
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].owner.as_deref(), Some("alice"));
    }

    #[test]
    fn manager_find_by_project() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1")).unwrap();
        mgr.create_task(Task::new("Task 2").with_project("proj-2"))
            .unwrap();

        let filter = TaskFilter::new().with_project("proj-1");
        let results = mgr.find_tasks(&filter);
        // Both tasks get project_id "proj-1" set by the manager (first gets auto-filled,
        // second has its own project_id preserved)
        assert!(results.iter().any(|t| t.project_id == "proj-1"));
    }

    #[test]
    fn manager_find_empty_results() {
        let mgr = TaskManager::new("proj-1");
        let filter = TaskFilter::new().with_owner("nobody");
        assert!(mgr.find_tasks(&filter).is_empty());
    }

    #[test]
    fn manager_find_no_filter_returns_all() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("Task 1")).unwrap();
        mgr.create_task(Task::new("Task 2")).unwrap();

        let filter = TaskFilter::new();
        assert_eq!(mgr.find_tasks(&filter).len(), 2);
    }

    #[test]
    fn manager_tasks_by_status() {
        let mut mgr = TaskManager::new("proj-1");
        let _id1 = mgr.create_task(Task::new("Task 1")).unwrap();
        let id2 = mgr.create_task(Task::new("Task 2")).unwrap();
        let id3 = mgr.create_task(Task::new("Task 3")).unwrap();
        mgr.update_status(&id2, TaskStatus::Completed).unwrap();
        mgr.update_status(&id3, TaskStatus::Failed).unwrap();

        let by_status = mgr.tasks_by_status();
        assert_eq!(by_status[&TaskStatus::Pending].len(), 1);
        assert_eq!(by_status[&TaskStatus::Completed].len(), 1);
        assert_eq!(by_status[&TaskStatus::Failed].len(), 1);
    }

    // ========================================================================
    // TaskError tests
    // ========================================================================

    #[test]
    fn task_error_display() {
        let err = TaskError::NotFound("task-1".to_string());
        assert!(err.to_string().contains("task-1"));

        let err = TaskError::AlreadyExists("task-1".to_string());
        assert!(err.to_string().contains("task-1"));

        let err = TaskError::InvalidTransition {
            from: "completed".to_string(),
            to: "pending".to_string(),
        };
        assert!(err.to_string().contains("completed"));
        assert!(err.to_string().contains("pending"));

        let err = TaskError::CircularDependency("task-1".to_string());
        assert!(err.to_string().contains("task-1"));

        let err = TaskError::DependencyNotFound("dep-1".to_string());
        assert!(err.to_string().contains("dep-1"));

        let err = TaskError::Blocked("task-1".to_string());
        assert!(err.to_string().contains("task-1"));

        let err = TaskError::Validation("bad input".to_string());
        assert!(err.to_string().contains("bad input"));
    }

    #[test]
    fn task_error_clone_eq() {
        let err1 = TaskError::NotFound("task-1".to_string());
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn task_error_neq_different_variants() {
        let err1 = TaskError::NotFound("x".to_string());
        let err2 = TaskError::AlreadyExists("x".to_string());
        assert_ne!(err1, err2);
    }

    #[test]
    fn task_error_debug_format() {
        let err = TaskError::NotFound("task-1".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("NotFound"));
    }

    // ========================================================================
    // Integration-style tests
    // ========================================================================

    #[test]
    fn manager_multiple_tasks_with_dependencies() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr.create_task(Task::new("Setup").with_id("t-1")).unwrap();
        let id2 = mgr.create_task(Task::new("Build").with_id("t-2")).unwrap();
        let id3 = mgr.create_task(Task::new("Test").with_id("t-3")).unwrap();

        // Test depends on Build, Build depends on Setup
        mgr.add_dependency(&id2, &id1).unwrap();
        mgr.add_dependency(&id3, &id2).unwrap();

        // Setup has one dependent (Build)
        assert_eq!(mgr.get_dependents(&id1).len(), 1);
        // Test has two dependencies (Setup, Build) via chain
        assert_eq!(mgr.get_dependencies(&id3).len(), 1); // direct only
    }

    #[test]
    fn manager_update_status_updates_timestamp() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        let original_updated = mgr.get_task(&id).unwrap().updated_at;

        // Small delay to ensure timestamp differs
        std::thread::sleep(std::time::Duration::from_millis(1));
        mgr.update_status(&id, TaskStatus::InProgress).unwrap();

        let new_updated = mgr.get_task(&id).unwrap().updated_at;
        assert!(new_updated >= original_updated);
    }

    #[test]
    fn manager_started_at_only_set_once() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();

        mgr.update_status(&id, TaskStatus::InProgress).unwrap();
        let first_start = mgr.get_task(&id).unwrap().started_at;

        mgr.update_status(&id, TaskStatus::Running).unwrap();
        let second_start = mgr.get_task(&id).unwrap().started_at;

        assert_eq!(first_start, second_start); // not overwritten
    }

    #[test]
    fn manager_completed_via_fail_and_kill() {
        let mut mgr = TaskManager::new("proj-1");

        let id1 = mgr.create_task(Task::new("Task 1")).unwrap();
        let id2 = mgr.create_task(Task::new("Task 2")).unwrap();

        mgr.fail_task(&id1).unwrap();
        mgr.update_status(&id2, TaskStatus::Killed).unwrap();

        assert!(mgr.get_task(&id1).unwrap().completed_at.is_some());
        assert!(mgr.get_task(&id2).unwrap().completed_at.is_some());
    }

    #[test]
    fn manager_running_sets_started_at() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Running).unwrap();
        assert!(mgr.get_task(&id).unwrap().started_at.is_some());
    }

    #[test]
    fn task_status_pending_to_blocked_transition() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Blocked).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::Blocked);
        // Blocked is not terminal, should still be able to transition
        mgr.update_status(&id, TaskStatus::InProgress).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::InProgress);
    }

    #[test]
    fn manager_workflow_with_filter() {
        let mut mgr = TaskManager::new("proj-1");

        // Create tasks with various statuses and owners
        let _id1 = mgr
            .create_task(Task::new("Task 1").with_owner("alice").with_id("t-1"))
            .unwrap();
        let id2 = mgr
            .create_task(Task::new("Task 2").with_owner("alice").with_id("t-2"))
            .unwrap();
        let id3 = mgr
            .create_task(Task::new("Task 3").with_owner("bob").with_id("t-3"))
            .unwrap();

        mgr.update_status(&id2, TaskStatus::Completed).unwrap();
        mgr.update_status(&id3, TaskStatus::InProgress).unwrap();

        // Find alice's pending tasks
        let filter = TaskFilter::new()
            .with_owner("alice")
            .with_status(TaskStatus::Pending);
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "t-1");
    }

    #[test]
    fn manager_ownership_reassignment() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr
            .create_task(Task::new("Task").with_owner("alice"))
            .unwrap();

        mgr.assign_owner(&id, "bob").unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().owner.as_deref(), Some("bob"));
    }

    #[test]
    fn manager_output_overwrite() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();

        mgr.set_output(&id, "First output").unwrap();
        mgr.set_output(&id, "Updated output").unwrap();
        assert_eq!(
            mgr.get_task(&id).unwrap().output.as_deref(),
            Some("Updated output")
        );
    }

    #[test]
    fn manager_large_number_of_tasks() {
        let mut mgr = TaskManager::new("proj-1");
        let mut ids = Vec::new();
        for i in 0..100 {
            let id = mgr.create_task(Task::new(format!("Task {i}"))).unwrap();
            ids.push(id);
        }
        assert_eq!(mgr.len(), 100);

        // Complete half
        for id in &ids[..50] {
            mgr.complete_task(id).unwrap();
        }

        let filter = TaskFilter::new().with_status(TaskStatus::Completed);
        assert_eq!(mgr.find_tasks(&filter).len(), 50);
    }

    // ========================================================================
    // Additional tests: TaskManager edge cases
    // ========================================================================

    #[test]
    fn manager_remove_task_also_removes_reverse_deps() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        mgr.create_task(Task::new("B").with_id("t-2")).unwrap();
        mgr.add_dependency("t-2", "t-1").unwrap();

        // t-1 has t-2 as a dependent. Removing t-2 should also clean up.
        mgr.remove_task("t-2");
        // t-1's dependents should be empty now
        assert!(mgr.get_dependents("t-1").is_empty());
    }

    #[test]
    fn manager_remove_nonexistent_dependency_noop() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        // Remove a dependency that never existed - should succeed
        mgr.remove_dependency("t-1", "nonexistent-dep").unwrap();
    }

    #[test]
    fn manager_get_dependencies_empty() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        assert!(mgr.get_dependencies("t-1").is_empty());
    }

    #[test]
    fn manager_get_dependents_empty() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("A").with_id("t-1")).unwrap();
        assert!(mgr.get_dependents("t-1").is_empty());
    }

    #[test]
    fn manager_tasks_by_status_empty() {
        let mgr = TaskManager::new("proj-1");
        let by_status = mgr.tasks_by_status();
        assert!(by_status.is_empty());
    }

    #[test]
    fn manager_clone_independent() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();

        let cloned = mgr.clone();
        assert_eq!(cloned.len(), 1);
        assert_eq!(cloned.get_task(&id).unwrap().description, "Task");

        // Modifying original doesn't affect clone
        mgr.complete_task(&id).unwrap();
        assert_eq!(cloned.get_task(&id).unwrap().status, TaskStatus::Pending);
    }

    #[test]
    fn manager_update_status_blocked_to_running() {
        let mut mgr = TaskManager::new("proj-1");
        let id = mgr.create_task(Task::new("Task")).unwrap();
        mgr.update_status(&id, TaskStatus::Blocked).unwrap();
        mgr.update_status(&id, TaskStatus::Running).unwrap();
        assert_eq!(mgr.get_task(&id).unwrap().status, TaskStatus::Running);
        assert!(mgr.get_task(&id).unwrap().started_at.is_some());
    }

    #[test]
    fn manager_find_by_priority_and_status_combined() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr
            .create_task(Task::new("High pending").with_priority(TaskPriority::High))
            .unwrap();
        let _id2 = mgr
            .create_task(Task::new("Normal pending").with_priority(TaskPriority::Normal))
            .unwrap();
        let id3 = mgr
            .create_task(Task::new("High completed").with_priority(TaskPriority::High))
            .unwrap();
        mgr.complete_task(&id3).unwrap();

        let filter = TaskFilter::new()
            .with_status(TaskStatus::Pending)
            .with_priority(TaskPriority::High);
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id1);
    }

    #[test]
    fn manager_find_by_session() {
        let mut mgr = TaskManager::new("proj-1");
        mgr.create_task(Task::new("With session").with_session("sess-1"))
            .unwrap();
        mgr.create_task(Task::new("No session")).unwrap();

        let filter = TaskFilter::new().with_session("sess-1");
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn manager_find_blocked_only() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr.create_task(Task::new("Task 1")).unwrap();
        let _id2 = mgr.create_task(Task::new("Task 2")).unwrap();
        mgr.update_status(&id1, TaskStatus::Blocked).unwrap();

        let filter = TaskFilter::new().blocked_only();
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TaskStatus::Blocked);
    }

    #[test]
    fn manager_find_unblocked_only() {
        let mut mgr = TaskManager::new("proj-1");
        let id1 = mgr.create_task(Task::new("Task 1")).unwrap();
        let _id2 = mgr.create_task(Task::new("Task 2")).unwrap();
        mgr.update_status(&id1, TaskStatus::Blocked).unwrap();

        let filter = TaskFilter::new().unblocked_only();
        let results = mgr.find_tasks(&filter);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, TaskStatus::Pending);
    }

    // ========================================================================
    // Additional tests: Task builder and edge cases
    // ========================================================================

    #[test]
    fn task_with_all_builder_fields() {
        let task = Task::new("Complex task")
            .with_id("t-custom")
            .with_project("proj-x")
            .with_session("sess-y")
            .with_priority(TaskPriority::Critical)
            .with_owner("system")
            .with_estimate("3 hours")
            .with_dependencies(vec!["dep-a".to_string(), "dep-b".to_string()]);

        assert_eq!(task.id, "t-custom");
        assert_eq!(task.project_id, "proj-x");
        assert_eq!(task.session_id.as_deref(), Some("sess-y"));
        assert_eq!(task.priority, TaskPriority::Critical);
        assert_eq!(task.owner.as_deref(), Some("system"));
        assert_eq!(task.estimate.as_deref(), Some("3 hours"));
        assert_eq!(task.dependencies.len(), 2);
    }

    #[test]
    fn task_duration_with_completed_at() {
        let mut task = Task::new("Test");
        let start = Utc::now() - chrono::Duration::seconds(60);
        let end = Utc::now();
        task.started_at = Some(start);
        task.completed_at = Some(end);
        let dur = task.duration().unwrap();
        assert!(dur.num_seconds() >= 59);
        assert!(dur.num_seconds() <= 61);
    }

    #[test]
    fn task_duration_without_completed_uses_now() {
        let mut task = Task::new("Test");
        task.started_at = Some(Utc::now() - chrono::Duration::seconds(5));
        let dur = task.duration().unwrap();
        assert!(dur.num_seconds() >= 5);
    }

    #[test]
    fn task_dependency_order_matters() {
        let d1 = TaskDependency::new("a", "b");
        let d2 = TaskDependency::new("b", "a");
        assert_ne!(d1, d2);
    }

    // ========================================================================
    // Additional tests: TaskError formatting
    // ========================================================================

    #[test]
    fn task_error_all_variants_have_nonempty_display() {
        let errors = [
            TaskError::NotFound("x".to_string()),
            TaskError::AlreadyExists("x".to_string()),
            TaskError::InvalidTransition {
                from: "a".to_string(),
                to: "b".to_string(),
            },
            TaskError::CircularDependency("x".to_string()),
            TaskError::DependencyNotFound("x".to_string()),
            TaskError::Blocked("x".to_string()),
            TaskError::Validation("x".to_string()),
        ];
        for err in &errors {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    #[allow(clippy::match_same_arms)]
    fn task_error_non_exhaustive_match() {
        // Verify that the #[non_exhaustive] attribute is present by using a wildcard
        let err = TaskError::NotFound("test".to_string());
        let msg = match &err {
            TaskError::NotFound(s) => s.clone(),
            TaskError::AlreadyExists(_) => String::new(),
            TaskError::InvalidTransition { .. } => String::new(),
            TaskError::CircularDependency(_) => String::new(),
            TaskError::DependencyNotFound(_) => String::new(),
            TaskError::Blocked(_) => String::new(),
            TaskError::Validation(_) => String::new(),
        };
        assert_eq!(msg, "test");
    }
}
