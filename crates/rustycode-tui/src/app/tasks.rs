//! Task and Todo management for the TUI.
//!
//! This module provides data structures and persistence for:
//! - Tasks (with status tracking: Pending, InProgress, Completed, Blocked, Running, Failed, Killed)
//! - Todos (simple checklist items)
//! - Active agents (background agent processes)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

// ── Data Structures ───────────────────────────────────────────────────────

/// A task with status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub dependencies: Vec<String>,
    /// Agent ID that owns this task (set when an agent is spawned for it)
    #[serde(default)]
    pub owner: Option<String>,
}

/// Status of a task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
    Running,
    Failed,
    Killed,
}

/// A simple todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: String,
    pub text: String,
    pub done: bool,
    pub created_at: SystemTime,
}

/// An active agent process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveAgent {
    pub id: String,
    pub task: String,
    pub status: AgentStatus,
    pub created_at: SystemTime,
}

/// Status of an agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[non_exhaustive]
pub enum AgentStatus {
    Starting,
    Running,
    Completed,
    Failed,
    Killed,
}

/// Container for all workspace tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTasks {
    pub tasks: Vec<Task>,
    pub todos: Vec<Todo>,
    pub active_agents: Vec<ActiveAgent>,
}

// ── File Management ───────────────────────────────────────────────────────

// Thread-local override for tasks path (used by tests)
#[cfg(test)]
thread_local! {
    static TEST_TASKS_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Set a thread-local override for the tasks file path (tests only)
#[cfg(test)]
pub fn set_test_tasks_path(path: Option<PathBuf>) {
    TEST_TASKS_PATH.with(|p| *p.borrow_mut() = path);
}

/// Get the path to the tasks file
pub fn tasks_path() -> PathBuf {
    #[cfg(test)]
    {
        let override_path = TEST_TASKS_PATH.with(|p| p.borrow().clone());
        if let Some(path) = override_path {
            return path;
        }
    }
    PathBuf::from(".rustycode/tasks.json")
}

/// Load tasks from disk
pub fn load_tasks() -> WorkspaceTasks {
    let path = tasks_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            match serde_json::from_str::<WorkspaceTasks>(&content) {
                Ok(tasks) => {
                    tracing::debug!(
                        "Loaded {} tasks, {} todos, {} agents",
                        tasks.tasks.len(),
                        tasks.todos.len(),
                        tasks.active_agents.len()
                    );
                    return tasks;
                }
                Err(e) => {
                    tracing::warn!("Failed to deserialize tasks.json: {}", e);
                    tracing::warn!("Creating new empty tasks state");
                }
            }
        }
    }
    WorkspaceTasks {
        tasks: Vec::new(),
        todos: Vec::new(),
        active_agents: Vec::new(),
    }
}

/// Save tasks to disk atomically (temp file + rename).
///
/// Writes to a temporary file first, then renames it into place.
/// This prevents corruption if the app crashes mid-write.
pub fn save_tasks(tasks: &WorkspaceTasks) -> std::io::Result<()> {
    let path = tasks_path();
    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(tasks)?;

    // Atomic write: temp file in same directory, then rename
    let temp_path = path.with_extension("json.tmp");
    std::fs::write(&temp_path, &content)?;
    std::fs::rename(&temp_path, &path)?;

    Ok(())
}

// ── Task Operations ───────────────────────────────────────────────────────

/// Create a new task
pub fn create_task(description: String) -> Task {
    Task {
        id: ulid::Ulid::new().to_string(),
        description,
        status: TaskStatus::Pending,
        created_at: SystemTime::now(),
        dependencies: Vec::new(),
        owner: None,
    }
}

/// Update task status
pub fn update_task_status(
    tasks: &mut WorkspaceTasks,
    id: &str,
    status: TaskStatus,
) -> Result<(), String> {
    if let Some(task) = tasks.tasks.iter_mut().find(|t| t.id == id) {
        task.status = status;
        Ok(())
    } else {
        Err(format!("Task {} not found", id))
    }
}

/// Get status icon for a task
pub fn task_status_icon(status: &TaskStatus) -> &str {
    match status {
        TaskStatus::Pending => "⏳",
        TaskStatus::InProgress => "🔄",
        TaskStatus::Completed => "✅",
        TaskStatus::Blocked => "🚫",
        TaskStatus::Running => "🤖",
        TaskStatus::Failed => "💥",
        TaskStatus::Killed => "🗑️",
    }
}

// ── Todo Operations ───────────────────────────────────────────────────────

/// Create a new todo
pub fn create_todo(text: String) -> Todo {
    Todo {
        id: ulid::Ulid::new().to_string(),
        text,
        done: false,
        created_at: SystemTime::now(),
    }
}

/// Toggle todo completion
pub fn toggle_todo(tasks: &mut WorkspaceTasks, id: &str) -> Result<bool, String> {
    if let Some(todo) = tasks.todos.iter_mut().find(|t| t.id == id) {
        todo.done = !todo.done;
        Ok(todo.done)
    } else {
        Err(format!("Todo {} not found", id))
    }
}

/// Get checkbox icon for a todo
pub fn todo_checkbox(done: bool) -> &'static str {
    if done {
        "☑"
    } else {
        "☐"
    }
}

// ── Agent Operations ──────────────────────────────────────────────────────

/// Create a new agent
pub fn create_agent(task: String) -> ActiveAgent {
    ActiveAgent {
        id: ulid::Ulid::new().to_string(),
        task,
        status: AgentStatus::Starting,
        created_at: SystemTime::now(),
    }
}

/// Create an agent and a corresponding Task with `Running` status.
///
/// Returns `(task, agent)` where the task's `owner` is set to `agent.id`.
pub fn create_agent_from_task(description: String) -> (Task, ActiveAgent) {
    let agent = ActiveAgent {
        id: ulid::Ulid::new().to_string(),
        task: description.clone(),
        status: AgentStatus::Running,
        created_at: SystemTime::now(),
    };

    let task = Task {
        id: ulid::Ulid::new().to_string(),
        description,
        status: TaskStatus::Running,
        created_at: SystemTime::now(),
        dependencies: Vec::new(),
        owner: Some(agent.id.clone()),
    };

    (task, agent)
}

impl ActiveAgent {
    /// Convert a `Task` with an agent owner back into an `ActiveAgent`.
    ///
    /// Returns `None` if the task is not in an agent-like status or has no owner.
    pub fn from_task(task: &Task) -> Option<Self> {
        let status = match task.status {
            TaskStatus::Running => AgentStatus::Running,
            TaskStatus::Failed => AgentStatus::Failed,
            TaskStatus::Killed => AgentStatus::Killed,
            _ => return None,
        };

        task.owner.as_ref().map(|owner| ActiveAgent {
            id: owner.clone(),
            task: task.description.clone(),
            status,
            created_at: task.created_at,
        })
    }
}

impl WorkspaceTasks {
    /// Derive `ActiveAgent` list from tasks that have an owner and
    /// agent-like status (Running / Failed / Killed).
    pub fn active_agents_from_tasks(&self) -> Vec<ActiveAgent> {
        self.tasks
            .iter()
            .filter_map(ActiveAgent::from_task)
            .collect()
    }
}

const LLM_TODO_OWNER: &str = "llm-todo";

pub fn sync_from_todo_state(
    workspace: &mut WorkspaceTasks,
    todo_state: &rustycode_tools::todo::TodoState,
) -> bool {
    let items = match todo_state.lock() {
        Ok(guard) => guard,
        Err(poison) => {
            tracing::warn!("Mutex poisoned during todo sync, recovering");
            poison.into_inner()
        }
    };

    let old_llm_tasks: Vec<&Task> = workspace
        .tasks
        .iter()
        .filter(|t| t.owner.as_deref() == Some(LLM_TODO_OWNER))
        .collect();

    let new_tasks: Vec<Task> = items
        .iter()
        .map(|item| {
            let existing = old_llm_tasks
                .iter()
                .find(|t| t.id == item.id)
                .copied();
            llm_todo_to_task(item, existing)
        })
        .collect();

    let changed = old_llm_tasks.len() != new_tasks.len()
        || !std::iter::zip(&old_llm_tasks, &new_tasks)
            .all(|(old, new)| old.id == new.id && old.status == new.status && old.description == new.description);

    if changed {
        workspace
            .tasks
            .retain(|t| t.owner.as_deref() != Some(LLM_TODO_OWNER));
        workspace.tasks.extend(new_tasks);
    }

    changed
}

fn llm_todo_to_task(
    item: &rustycode_tools::todo::TodoItem,
    existing: Option<&Task>,
) -> Task {
    Task {
        id: item.id.clone(),
        description: item.title.clone(),
        status: match item.status {
            rustycode_tools::todo::TodoStatus::Pending => TaskStatus::Pending,
            rustycode_tools::todo::TodoStatus::InProgress => TaskStatus::InProgress,
            rustycode_tools::todo::TodoStatus::Completed => TaskStatus::Completed,
            _ => {
                tracing::warn!(
                    "Unknown TodoStatus for item {}: {:?}, treating as Pending",
                    item.id,
                    item.status
                );
                TaskStatus::Pending
            }
        },
        created_at: existing.map(|t| t.created_at).unwrap_or_else(SystemTime::now),
        dependencies: Vec::new(),
        owner: Some(LLM_TODO_OWNER.to_string()),
    }
}

/// Update agent status
pub fn update_agent_status(
    tasks: &mut WorkspaceTasks,
    id: &str,
    status: AgentStatus,
) -> Result<(), String> {
    if let Some(agent) = tasks.active_agents.iter_mut().find(|a| a.id == id) {
        agent.status = status.clone();

        // Mirror the status change onto the corresponding Task (matched by owner field)
        let task_status = match status {
            AgentStatus::Running => Some(TaskStatus::Running),
            AgentStatus::Completed => Some(TaskStatus::Completed),
            AgentStatus::Failed => Some(TaskStatus::Failed),
            AgentStatus::Killed => Some(TaskStatus::Killed),
            AgentStatus::Starting => None,
        };

        if let Some(ts) = task_status {
            if let Some(task) = tasks
                .tasks
                .iter_mut()
                .find(|t| t.owner.as_deref() == Some(id))
            {
                task.status = ts;
            }
        }

        Ok(())
    } else {
        Err(format!("Agent {} not found", id))
    }
}

/// Get status icon for an agent
pub fn agent_status_icon(status: &AgentStatus) -> &str {
    match status {
        AgentStatus::Starting => "⚡",
        AgentStatus::Running => "🤖",
        AgentStatus::Completed => "✨",
        AgentStatus::Failed => "💥",
        AgentStatus::Killed => "🗑️",
    }
}

// ── Formatting Helpers ────────────────────────────────────────────────────

/// Format timestamp for display
pub fn format_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local, Utc};

    let datetime: DateTime<Utc> = time.into();
    let datetime: DateTime<Local> = DateTime::from(datetime);
    datetime.format("%H:%M").to_string()
}

/// Format relative time for display
pub fn format_relative_time(time: SystemTime) -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(time).unwrap_or_default();

    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_task() {
        let task = create_task("Test task".to_string());
        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);
        assert!(!task.id.is_empty());
        assert!(task.owner.is_none());
    }

    #[test]
    fn test_create_todo() {
        let todo = create_todo("Test todo".to_string());
        assert_eq!(todo.text, "Test todo");
        assert!(!todo.done);
        assert!(!todo.id.is_empty());
    }

    #[test]
    fn test_create_agent() {
        let agent = create_agent("Test agent task".to_string());
        assert_eq!(agent.task, "Test agent task");
        assert_eq!(agent.status, AgentStatus::Starting);
        assert!(!agent.id.is_empty());
    }

    #[test]
    fn test_update_task_status() {
        let mut tasks = WorkspaceTasks {
            tasks: vec![create_task("Test".to_string())],
            todos: Vec::new(),
            active_agents: Vec::new(),
        };
        let id = tasks.tasks[0].id.clone();

        let result = update_task_status(&mut tasks, &id, TaskStatus::Completed);
        assert!(result.is_ok());
        assert_eq!(tasks.tasks[0].status, TaskStatus::Completed);
    }

    #[test]
    fn test_toggle_todo() {
        let mut tasks = WorkspaceTasks {
            tasks: Vec::new(),
            todos: vec![create_todo("Test".to_string())],
            active_agents: Vec::new(),
        };
        let id = tasks.todos[0].id.clone();

        let result = toggle_todo(&mut tasks, &id).unwrap();
        assert!(result);
        assert!(tasks.todos[0].done);
    }

    #[test]
    fn test_status_icons() {
        assert_eq!(task_status_icon(&TaskStatus::Pending), "⏳");
        assert_eq!(task_status_icon(&TaskStatus::InProgress), "🔄");
        assert_eq!(task_status_icon(&TaskStatus::Completed), "✅");
        assert_eq!(task_status_icon(&TaskStatus::Blocked), "🚫");
        assert_eq!(task_status_icon(&TaskStatus::Running), "🤖");
        assert_eq!(task_status_icon(&TaskStatus::Failed), "💥");
        assert_eq!(task_status_icon(&TaskStatus::Killed), "🗑️");

        assert_eq!(todo_checkbox(false), "☐");
        assert_eq!(todo_checkbox(true), "☑");

        assert_eq!(agent_status_icon(&AgentStatus::Starting), "⚡");
        assert_eq!(agent_status_icon(&AgentStatus::Running), "🤖");
        assert_eq!(agent_status_icon(&AgentStatus::Completed), "✨");
        assert_eq!(agent_status_icon(&AgentStatus::Failed), "💥");
        assert_eq!(agent_status_icon(&AgentStatus::Killed), "🗑️");
    }

    #[test]
    fn test_create_agent_from_task() {
        let (task, agent) = create_agent_from_task("Test agent task".to_string());
        assert_eq!(task.status, TaskStatus::Running);
        assert_eq!(task.owner.as_deref(), Some(agent.id.as_str()));
        assert_eq!(agent.status, AgentStatus::Running);
        assert_eq!(task.description, "Test agent task");
        assert_eq!(agent.task, "Test agent task");
    }

    #[test]
    fn test_active_agent_from_task() {
        let task = Task {
            id: "t1".to_string(),
            description: "agent work".to_string(),
            status: TaskStatus::Running,
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: Some("agent-42".to_string()),
        };

        let agent = ActiveAgent::from_task(&task).expect("should convert");
        assert_eq!(agent.id, "agent-42");
        assert_eq!(agent.status, AgentStatus::Running);
        assert_eq!(agent.task, "agent work");
    }

    #[test]
    fn test_active_agent_from_task_non_agent_status() {
        let task = Task {
            id: "t1".to_string(),
            description: "pending work".to_string(),
            status: TaskStatus::Pending,
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: Some("agent-42".to_string()),
        };
        assert!(ActiveAgent::from_task(&task).is_none());
    }

    #[test]
    fn test_active_agent_from_task_no_owner() {
        let task = Task {
            id: "t1".to_string(),
            description: "running without owner".to_string(),
            status: TaskStatus::Running,
            created_at: SystemTime::now(),
            dependencies: vec![],
            owner: None,
        };
        assert!(ActiveAgent::from_task(&task).is_none());
    }

    #[test]
    fn test_active_agents_from_tasks() {
        let tasks = WorkspaceTasks {
            tasks: vec![
                Task {
                    id: "t1".to_string(),
                    description: "running".to_string(),
                    status: TaskStatus::Running,
                    created_at: SystemTime::now(),
                    dependencies: vec![],
                    owner: Some("a1".to_string()),
                },
                Task {
                    id: "t2".to_string(),
                    description: "pending".to_string(),
                    status: TaskStatus::Pending,
                    created_at: SystemTime::now(),
                    dependencies: vec![],
                    owner: None,
                },
                Task {
                    id: "t3".to_string(),
                    description: "failed".to_string(),
                    status: TaskStatus::Failed,
                    created_at: SystemTime::now(),
                    dependencies: vec![],
                    owner: Some("a2".to_string()),
                },
            ],
            todos: vec![],
            active_agents: vec![],
        };

        let agents = tasks.active_agents_from_tasks();
        assert_eq!(agents.len(), 2);
        assert!(agents
            .iter()
            .any(|a| a.id == "a1" && a.status == AgentStatus::Running));
        assert!(agents
            .iter()
            .any(|a| a.id == "a2" && a.status == AgentStatus::Failed));
    }

    #[test]
    fn test_workspace_tasks_serialization() {
        // Create test data
        let tasks = WorkspaceTasks {
            tasks: vec![create_task("Test task".to_string())],
            todos: vec![create_todo("Test todo".to_string())],
            active_agents: vec![create_agent("Test agent".to_string())],
        };

        // Serialize and deserialize
        let json = serde_json::to_string(&tasks).unwrap();
        let loaded: WorkspaceTasks = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.todos.len(), 1);
        assert_eq!(loaded.active_agents.len(), 1);
        assert_eq!(loaded.tasks[0].description, "Test task");
        assert_eq!(loaded.tasks[0].owner, None);
        assert_eq!(loaded.todos[0].text, "Test todo");
        assert_eq!(loaded.active_agents[0].task, "Test agent");
    }

    #[test]
    fn test_deserialize_old_tasks_json_without_owner() {
        // Simulate loading an old tasks.json that doesn't have the owner field
        let json = r#"{
            "tasks": [{"id":"t1","description":"old task","status":"Pending","created_at":{"secs_since_epoch":1000,"nanos_since_epoch":0},"dependencies":[]}],
            "todos": [],
            "active_agents": []
        }"#;
        let loaded: WorkspaceTasks = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].owner, None);
    }

    #[test]
    fn test_sync_preserves_created_at() {
        // Verify that created_at timestamp is preserved when a task already exists
        let old_time = SystemTime::UNIX_EPOCH;

        let mut workspace = WorkspaceTasks {
            tasks: vec![Task {
                id: "todo-1".to_string(),
                description: "Existing task".to_string(),
                status: TaskStatus::Pending,
                created_at: old_time,
                dependencies: vec![],
                owner: Some(LLM_TODO_OWNER.to_string()),
            }],
            todos: vec![],
            active_agents: vec![],
        };

        let todo_state = std::sync::Arc::new(std::sync::Mutex::new(vec![
            rustycode_tools::todo::TodoItem {
                id: "todo-1".to_string(),
                title: "Existing task".to_string(),
                status: rustycode_tools::todo::TodoStatus::Pending,
                active_form: None,
            },
        ]));

        let changed = sync_from_todo_state(&mut workspace, &todo_state);

        // Task content didn't change, so sync should return false
        assert!(!changed);
        // But created_at should be preserved from the old task
        assert_eq!(workspace.tasks[0].created_at, old_time);
    }

    #[test]
    fn test_sync_sets_created_at_for_new_todos() {
        // Verify that created_at is set to now() for brand new todos
        let mut workspace = WorkspaceTasks {
            tasks: vec![],
            todos: vec![],
            active_agents: vec![],
        };

        let todo_state = std::sync::Arc::new(std::sync::Mutex::new(vec![
            rustycode_tools::todo::TodoItem {
                id: "new-todo".to_string(),
                title: "New task".to_string(),
                status: rustycode_tools::todo::TodoStatus::InProgress,
                active_form: None,
            },
        ]));

        let before_sync = SystemTime::now();
        let changed = sync_from_todo_state(&mut workspace, &todo_state);
        let after_sync = SystemTime::now();

        assert!(changed);
        assert_eq!(workspace.tasks.len(), 1);
        assert_eq!(workspace.tasks[0].id, "new-todo");
        assert_eq!(workspace.tasks[0].status, TaskStatus::InProgress);
        // created_at should be fresh (between before and after)
        assert!(workspace.tasks[0].created_at >= before_sync);
        assert!(workspace.tasks[0].created_at <= after_sync);
    }

    #[test]
    fn test_sync_detects_status_changes() {
        // Verify that change detection works correctly for status changes
        let mut workspace = WorkspaceTasks {
            tasks: vec![Task {
                id: "todo-1".to_string(),
                description: "Task".to_string(),
                status: TaskStatus::Pending,
                created_at: SystemTime::now(),
                dependencies: vec![],
                owner: Some(LLM_TODO_OWNER.to_string()),
            }],
            todos: vec![],
            active_agents: vec![],
        };

        let todo_state = std::sync::Arc::new(std::sync::Mutex::new(vec![
            rustycode_tools::todo::TodoItem {
                id: "todo-1".to_string(),
                title: "Task".to_string(),
                status: rustycode_tools::todo::TodoStatus::Completed,
                active_form: None,
            },
        ]));

        let changed = sync_from_todo_state(&mut workspace, &todo_state);

        assert!(changed);
        assert_eq!(workspace.tasks[0].status, TaskStatus::Completed);
    }

    #[test]
    fn test_sync_detects_description_changes() {
        // Verify that change detection works for description changes
        let mut workspace = WorkspaceTasks {
            tasks: vec![Task {
                id: "todo-1".to_string(),
                description: "Old description".to_string(),
                status: TaskStatus::Pending,
                created_at: SystemTime::now(),
                dependencies: vec![],
                owner: Some(LLM_TODO_OWNER.to_string()),
            }],
            todos: vec![],
            active_agents: vec![],
        };

        let todo_state = std::sync::Arc::new(std::sync::Mutex::new(vec![
            rustycode_tools::todo::TodoItem {
                id: "todo-1".to_string(),
                title: "New description".to_string(),
                status: rustycode_tools::todo::TodoStatus::Pending,
                active_form: None,
            },
        ]));

        let changed = sync_from_todo_state(&mut workspace, &todo_state);

        assert!(changed);
        assert_eq!(workspace.tasks[0].description, "New description");
    }

    #[test]
    fn test_sync_replaces_removed_todos() {
        // Verify that todos are properly removed when they disappear from the source
        let mut workspace = WorkspaceTasks {
            tasks: vec![
                Task {
                    id: "todo-1".to_string(),
                    description: "Task 1".to_string(),
                    status: TaskStatus::Pending,
                    created_at: SystemTime::now(),
                    dependencies: vec![],
                    owner: Some(LLM_TODO_OWNER.to_string()),
                },
                Task {
                    id: "todo-2".to_string(),
                    description: "Task 2".to_string(),
                    status: TaskStatus::Pending,
                    created_at: SystemTime::now(),
                    dependencies: vec![],
                    owner: Some(LLM_TODO_OWNER.to_string()),
                },
            ],
            todos: vec![],
            active_agents: vec![],
        };

        // Only task-1 remains in the source
        let todo_state = std::sync::Arc::new(std::sync::Mutex::new(vec![
            rustycode_tools::todo::TodoItem {
                id: "todo-1".to_string(),
                title: "Task 1".to_string(),
                status: rustycode_tools::todo::TodoStatus::Pending,
                active_form: None,
            },
        ]));

        let changed = sync_from_todo_state(&mut workspace, &todo_state);

        assert!(changed);
        assert_eq!(workspace.tasks.len(), 1);
        assert_eq!(workspace.tasks[0].id, "todo-1");
    }
}
