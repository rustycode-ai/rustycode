use crate::{ToolOutput, ToolPermission};
use dashmap::DashMap;
use rustycode_bus::{EventBus, TodoSnapshot, TodoUpdatedEvent};
use rustycode_storage::Storage;
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

// ============================================================================
// Global state management
// ============================================================================

/// Global todo state storage, keyed by session ID
static GLOBAL_TODO_STATES: std::sync::LazyLock<DashMap<String, TodoState>> =
    std::sync::LazyLock::new(DashMap::new);

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_form: Option<String>,
}

pub type TodoState = Arc<Mutex<Vec<TodoItem>>>;

pub fn new_todo_state() -> TodoState {
    Arc::new(Mutex::new(Vec::new()))
}

/// Retrieve or create TodoState for a given session.
pub fn get_or_create_todo_state(session_id: &str) -> TodoState {
    if let Some(entry) = GLOBAL_TODO_STATES.get(session_id) {
        entry.clone()
    } else {
        let state = new_todo_state();
        let _ = GLOBAL_TODO_STATES.insert(session_id.to_string(), state.clone());
        state
    }
}

// ============================================================================
// Helper functions
// ============================================================================

#[allow(dead_code)]
const fn to_storage_status(status: TodoStatus) -> rustycode_storage::task_store::TodoStatus {
    match status {
        TodoStatus::Pending => rustycode_storage::task_store::TodoStatus::Pending,
        TodoStatus::InProgress => rustycode_storage::task_store::TodoStatus::InProgress,
        TodoStatus::Completed => rustycode_storage::task_store::TodoStatus::Completed,
    }
}

#[allow(dead_code)]
fn persist_todos(storage: &Storage, state: &[TodoItem], session_id: &str, project_id: &str) {
    let storage_todos: Vec<rustycode_storage::task_store::TodoItem> = state
        .iter()
        .enumerate()
        .map(|(i, t)| rustycode_storage::task_store::TodoItem {
            id: t.id.clone(),
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            content: t.title.clone(),
            status: to_storage_status(t.status),
            priority: rustycode_storage::task_store::Priority::Medium,
            position: i as i64,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .collect();

    if let Err(e) = storage.replace_todos(session_id, &storage_todos) {
        tracing::warn!("Failed to persist todos to storage: {}", e);
    }
}

#[allow(dead_code)]
fn publish_todo_event(
    bus: Option<&Arc<EventBus>>,
    session_id: Option<&str>,
    todos_snapshot: &[TodoItem],
) {
    let Some(bus) = bus else { return };
    let Some(sid) = session_id else { return };
    let bus = Arc::clone(bus);
    let sid = sid.to_string();
    let snapshots: Vec<TodoSnapshot> = todos_snapshot
        .iter()
        .map(|t| TodoSnapshot {
            id: t.id.clone(),
            title: t.title.clone(),
            status: format_status(t.status),
            active_form: t.active_form.clone(),
        })
        .collect();
    tokio::spawn(async move {
        let event = TodoUpdatedEvent::new(sid, snapshots);
        if let Err(e) = bus.publish(event).await {
            tracing::debug!("Failed to publish todo.updated event: {}", e);
        }
    });
}

fn format_status(status: TodoStatus) -> String {
    match status {
        TodoStatus::Pending => "pending".to_string(),
        TodoStatus::InProgress => "in progress".to_string(),
        TodoStatus::Completed => "completed".to_string(),
    }
}

// ============================================================================
// Params structs
// ============================================================================

/// A single todo item input
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TodoItemInput {
    /// Unique identifier for this todo
    pub id: String,
    /// Task description
    pub title: String,
    /// Current status: pending, in_progress, or completed
    pub status: String,
    /// Present continuous form for spinner
    #[serde(rename = "activeForm")]
    pub active_form: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoWriteParams {
    /// Title for this todo list
    pub title: String,
    /// Array of todo items
    pub todos: Vec<TodoItemInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoUpdateParams {
    /// Todo item identifier
    pub id: String,
    /// New status: pending, in_progress, or completed
    pub status: Option<String>,
    /// Updated task description
    pub title: Option<String>,
    /// Updated present continuous form for spinner
    #[serde(rename = "activeForm")]
    pub active_form: Option<String>,
}

// ============================================================================
// TodoWriteTool
// ============================================================================

rustycode_tools_api::define_tool! {
    pub struct TodoWriteTool;

    name: "todo_write",
    description: r#"Use this tool to create and manage a structured task list for your current coding session. \
This helps you track progress on complex, multi-step tasks and demonstrate thoroughness to the user. \
It also helps the user understand the progress of the task and overall progress of their requests.

## When to Use
Use this tool proactively in these scenarios:
- Complex multi-step tasks — when a task requires 3 or more distinct steps or actions
- Non-trivial tasks that require careful planning or multiple operations
- User explicitly requests a todo list or provides multiple tasks (numbered or comma-separated)
- After receiving new instructions, immediately capture user requirements as tasks
- When you start working on a task, mark it as in_progress BEFORE beginning work
- When collaborating with teammates who may need context on what's being done

## When NOT to Use
Skip using this tool when:
- There is only one trivial task to do
- The task is purely conversational or informational
- You are confident the task can be completed in less than 3 trivial steps

## Task Management Rules
- Keep only ONE task in_progress at a time — complete the current task before starting the next
- Mark tasks in_progress BEFORE beginning work, and completed immediately when FULLY done
- Only mark a task completed when you have FULLY accomplished it — not when it is partially done
- If tests are failing, the implementation is partial, or you encountered unresolved errors, keep it in_progress
- After completing your current task, check the list to find the next available task
- Prefer working on tasks in ID order (lowest ID first) when multiple tasks are available

## activeForm
The activeForm field is a present continuous form of the task title, used to display \
a spinner or progress indicator. For example, if the title is "Fix build errors", \
the activeForm would be "Fixing build errors"."#,
    permission: ToolPermission::None,

    execute(params: TodoWriteParams, ctx) {
        let title = &params.title;

        let mut todos = Vec::new();
        for item in &params.todos {
            let status = match item.status.as_str() {
                "pending" => TodoStatus::Pending,
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                _ => return Err(anyhow::anyhow!("Invalid status: {}", item.status)),
            };

            todos.push(TodoItem {
                id: item.id.clone(),
                title: item.title.clone(),
                status,
                active_form: item.active_form.clone(),
            });
        }

        // Retrieve state from global store, keyed by session_id
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");
        let state = get_or_create_todo_state(session_id);

        let mut state_guard = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state_guard = todos;

        let completed_count = state_guard
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();
        let in_progress_count = state_guard
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::InProgress))
            .count();

        // Storage and event bus persistence handled separately

        let mut output = format!(
            "Todo list '{}' updated:\n- {} items total\n- {} completed\n- {} in progress",
            title,
            state_guard.len(),
            completed_count,
            in_progress_count
        );

        // Verification nudge: when all tasks are completed (3+) and none involved verification
        if state_guard.len() >= 3 && completed_count == state_guard.len() {
            output.push_str(
                "\n\nNote: All tasks completed. Consider running tests or verifying the changes \
                 before concluding.",
            );
        }

        Ok(ToolOutput::text(output))
    }
}

// ============================================================================
// TodoUpdateTool
// ============================================================================

rustycode_tools_api::define_tool! {
    pub struct TodoUpdateTool;

    name: "todo_update",
    description: r#"Update a single todo item's status, title, or activeForm.

Use this to mark tasks in_progress before starting work, and completed when fully done. \
Only mark a task completed when you have FULLY accomplished it. If tests are failing, \
the implementation is partial, or you encountered unresolved errors, keep it in_progress.

Parameters:
- id (string, required): Todo item identifier
- status (string, optional): New status: pending, in_progress, or completed
- title (string, optional): Updated task description
- activeForm (string, optional): Updated present continuous form for spinner"#,
    permission: ToolPermission::None,

    execute(params: TodoUpdateParams, ctx) {
        let id = &params.id;

        let new_status = params
            .status
            .as_deref()
            .map(|s| match s {
                "pending" => Ok(TodoStatus::Pending),
                "in_progress" => Ok(TodoStatus::InProgress),
                "completed" => Ok(TodoStatus::Completed),
                _ => Err(anyhow::anyhow!("Invalid status: {s}")),
            })
            .transpose()?;

        let new_title = params.title.clone();
        let new_active_form = params.active_form.clone();

        if new_status.is_none() && new_title.is_none() && new_active_form.is_none() {
            return Err(anyhow::anyhow!(
                "Must provide at least one of: status, title, activeForm"
            ));
        }

        // Retrieve state from global store, keyed by session_id
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");
        let state = get_or_create_todo_state(session_id);

        let mut state_guard = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let item = state_guard
            .iter_mut()
            .find(|t| t.id == *id)
            .ok_or_else(|| anyhow::anyhow!("Todo item not found: {id}"))?;

        let mut changes = Vec::new();

        if let Some(s) = new_status {
            let old = std::mem::replace(&mut item.status, s);
            changes.push(format!("{} → {}", format_status(old), format_status(s)));
        }
        if let Some(t) = new_title {
            changes.push(format!("title → {t}"));
            item.title = t;
        }
        if let Some(af) = new_active_form {
            changes.push(format!("activeForm → {af}"));
            item.active_form = Some(af);
        }

        // Storage and event bus persistence handled separately

        Ok(ToolOutput::text(format!(
            "Todo '{}': {}",
            id,
            changes.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use serde_json::json;

    #[test]
    fn test_todo_write() {
        let params = json!({
            "title": "Test todos",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"},
                {"id": "2", "title": "Task 2", "status": "in_progress"}
            ]
        });

        let ctx = ToolContext::new("/tmp").with_session_id("test-session-1");
        let tool = TodoWriteTool;
        let result = tool.execute(params, &ctx).unwrap();

        assert!(result.text.contains("Test todos"));
        assert!(result.text.contains("2 items total"));

        // Verify state was stored
        let state = get_or_create_todo_state("test-session-1");
        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(state_guard.len(), 2);
        assert_eq!(state_guard[0].title, "Task 1");
    }

    #[test]
    fn test_todo_update() {
        let ctx = ToolContext::new("/tmp").with_session_id("test-session-2");

        let params = json!({
            "title": "Test",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"}
            ]
        });

        let write_tool = TodoWriteTool;
        write_tool.execute(params, &ctx).unwrap();

        let update_tool = TodoUpdateTool;
        let update_params = json!({
            "id": "1",
            "status": "completed"
        });

        let result = update_tool.execute(update_params, &ctx).unwrap();
        assert!(result.text.contains("completed"));

        let state = get_or_create_todo_state("test-session-2");
        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(matches!(state_guard[0].status, TodoStatus::Completed));
    }

    #[test]
    fn todo_status_serde_roundtrip() {
        for status in [
            TodoStatus::Pending,
            TodoStatus::InProgress,
            TodoStatus::Completed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let decoded: TodoStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, decoded);
        }
    }

    #[test]
    fn todo_status_rename_lowercase() {
        assert_eq!(
            serde_json::to_string(&TodoStatus::InProgress).unwrap(),
            "\"inprogress\""
        );
        assert_eq!(
            serde_json::to_string(&TodoStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TodoStatus::Completed).unwrap(),
            "\"completed\""
        );
    }

    #[test]
    fn todo_item_serde_roundtrip() {
        let item = TodoItem {
            id: "42".into(),
            title: "Fix bug".into(),
            status: TodoStatus::InProgress,
            active_form: Some("Fixing bug".into()),
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: TodoItem = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, "42");
        assert_eq!(decoded.title, "Fix bug");
        assert_eq!(decoded.status, TodoStatus::InProgress);
        assert_eq!(decoded.active_form.as_deref(), Some("Fixing bug"));
    }

    #[test]
    fn new_todo_state_is_empty() {
        let state = new_todo_state();
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(guard.is_empty());
    }

    #[test]
    fn session_isolation() {
        let ctx1 = ToolContext::new("/tmp").with_session_id("session-a");
        let ctx2 = ToolContext::new("/tmp").with_session_id("session-b");

        let tool = TodoWriteTool;

        tool.execute(
            json!({
                "title": "A",
                "todos": [{"id": "1", "title": "Task A", "status": "pending"}]
            }),
            &ctx1,
        )
        .unwrap();

        tool.execute(
            json!({
                "title": "B",
                "todos": [
                    {"id": "1", "title": "Task B1", "status": "pending"},
                    {"id": "2", "title": "Task B2", "status": "pending"}
                ]
            }),
            &ctx2,
        )
        .unwrap();

        let state_a = get_or_create_todo_state("session-a");
        let state_b = get_or_create_todo_state("session-b");

        let guard_a = state_a.lock().unwrap_or_else(|e| e.into_inner());
        let guard_b = state_b.lock().unwrap_or_else(|e| e.into_inner());

        assert_eq!(guard_a.len(), 1);
        assert_eq!(guard_b.len(), 2);
        assert_eq!(guard_a[0].title, "Task A");
        assert_eq!(guard_b[0].title, "Task B1");
    }
}
