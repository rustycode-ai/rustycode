use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use rustycode_storage::Storage;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

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

const fn to_storage_status(status: TodoStatus) -> rustycode_storage::task_store::TodoStatus {
    match status {
        TodoStatus::Pending => rustycode_storage::task_store::TodoStatus::Pending,
        TodoStatus::InProgress => rustycode_storage::task_store::TodoStatus::InProgress,
        TodoStatus::Completed => rustycode_storage::task_store::TodoStatus::Completed,
    }
}

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

pub struct TodoWriteTool {
    pub state: TodoState,
    storage: Option<Arc<Storage>>,
}

impl TodoWriteTool {
    pub const fn new(state: TodoState) -> Self {
        Self {
            state,
            storage: None,
        }
    }

    pub const fn with_storage(state: TodoState, storage: Arc<Storage>) -> Self {
        Self {
            state,
            storage: Some(storage),
        }
    }
}

impl Tool for TodoWriteTool {
    fn name(&self) -> &'static str {
        "todo_write"
    }

    fn description(&self) -> &'static str {
        r#"Use this tool to create and manage a structured task list for your current coding session. \
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
- Only mark a task completed when you have FULLY accomplished it — not when it's partially done
- If tests are failing, the implementation is partial, or you encountered unresolved errors, keep it in_progress
- After completing your current task, check the list to find the next available task
- Prefer working on tasks in ID order (lowest ID first) when multiple tasks are available

## activeForm
The activeForm field is a present continuous form of the task title, used to display \
a spinner or progress indicator. For example, if the title is "Fix build errors", \
the activeForm would be "Fixing build errors"."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title for this todo list"
                },
                "todos": {
                    "type": "array",
                    "description": "Array of todo items",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for this todo"
                            },
                            "title": {
                                "type": "string",
                                "description": "Task description"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Current status"
                            },
                            "activeForm": {
                                "type": "string",
                                "description": "Present continuous form for spinner (e.g., 'Fixing build errors')"
                            }
                        },
                        "required": ["id", "title", "status"]
                    }
                }
            },
            "required": ["title", "todos"]
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let title = params["title"]
            .as_str()
            .ok_or(anyhow::anyhow!("Missing title"))?;
        let todos_input = params["todos"]
            .as_array()
            .ok_or(anyhow::anyhow!("Missing todos"))?;

        let mut todos = Vec::new();
        for item in todos_input {
            let id = item["id"].as_str().ok_or(anyhow::anyhow!("Missing id"))?;
            let item_title = item["title"]
                .as_str()
                .ok_or(anyhow::anyhow!("Missing title"))?;
            let status_str = item["status"]
                .as_str()
                .ok_or(anyhow::anyhow!("Missing status"))?;

            let status = match status_str {
                "pending" => TodoStatus::Pending,
                "in_progress" => TodoStatus::InProgress,
                "completed" => TodoStatus::Completed,
                _ => return Err(anyhow::anyhow!("Invalid status: {status_str}")),
            };

            todos.push(TodoItem {
                id: id.to_string(),
                title: item_title.to_string(),
                status,
                active_form: item
                    .get("activeForm")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            });
        }

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *state = todos;

        let completed_count = state
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();
        let in_progress_count = state
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::InProgress))
            .count();

        if let (Some(storage), Some(sid), Some(pid)) =
            (&self.storage, &ctx.session_id, &ctx.project_id)
        {
            persist_todos(storage, &state, sid, pid);
        }

        let mut output = format!(
            "Todo list '{}' updated:\n- {} items total\n- {} completed\n- {} in progress",
            title,
            state.len(),
            completed_count,
            in_progress_count
        );

        // Verification nudge: when all tasks are completed (3+) and none involved verification
        if state.len() >= 3 && completed_count == state.len() {
            output.push_str(
                "\n\nNote: All tasks completed. Consider running tests or verifying the changes \
                 before concluding.",
            );
        }

        Ok(ToolOutput::text(output))
    }
}

pub struct TodoUpdateTool {
    pub state: TodoState,
    storage: Option<Arc<Storage>>,
}

impl TodoUpdateTool {
    pub const fn new(state: TodoState) -> Self {
        Self {
            state,
            storage: None,
        }
    }

    pub const fn with_storage(state: TodoState, storage: Arc<Storage>) -> Self {
        Self {
            state,
            storage: Some(storage),
        }
    }
}

impl Tool for TodoUpdateTool {
    fn name(&self) -> &'static str {
        "todo_update"
    }

    fn description(&self) -> &'static str {
        r#"Update a single todo item's status, title, or activeForm.

Use this to mark tasks in_progress before starting work, and completed when fully done. \
Only mark a task completed when you have FULLY accomplished it. If tests are failing, \
the implementation is partial, or you encountered unresolved errors, keep it in_progress.

Parameters:
- id (string, required): Todo item identifier
- status (string, optional): New status: pending, in_progress, or completed
- title (string, optional): Updated task description
- activeForm (string, optional): Updated present continuous form for spinner"#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Todo item identifier"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed"],
                    "description": "New status"
                },
                "title": {
                    "type": "string",
                    "description": "Updated task description"
                },
                "activeForm": {
                    "type": "string",
                    "description": "Updated present continuous form for spinner"
                }
            },
            "required": ["id"]
        })
    }

    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let id = params["id"].as_str().ok_or(anyhow::anyhow!("Missing id"))?;

        let new_status = params
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "pending" => Ok(TodoStatus::Pending),
                "in_progress" => Ok(TodoStatus::InProgress),
                "completed" => Ok(TodoStatus::Completed),
                _ => Err(anyhow::anyhow!("Invalid status: {s}")),
            })
            .transpose()?;

        let new_title = params
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);
        let new_active_form = params
            .get("activeForm")
            .and_then(|v| v.as_str())
            .map(String::from);

        if new_status.is_none() && new_title.is_none() && new_active_form.is_none() {
            return Err(anyhow::anyhow!(
                "Must provide at least one of: status, title, activeForm"
            ));
        }

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let item = state
            .iter_mut()
            .find(|t| t.id == id)
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

        if let (Some(storage), Some(sid), Some(pid)) =
            (&self.storage, &ctx.session_id, &ctx.project_id)
        {
            persist_todos(storage, &state, sid, pid);
        }

        Ok(ToolOutput::text(format!(
            "Todo '{}': {}",
            id,
            changes.join(", ")
        )))
    }
}

fn format_status(status: TodoStatus) -> String {
    match status {
        TodoStatus::Pending => "pending".to_string(),
        TodoStatus::InProgress => "in progress".to_string(),
        TodoStatus::Completed => "completed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_write() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state.clone());

        let params = json!({
            "title": "Test todos",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"},
                {"id": "2", "title": "Task 2", "status": "in_progress"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(params, &ctx).unwrap();

        assert!(result.text.contains("Test todos"));
        assert!(result.text.contains("2 items total"));

        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(state_guard.len(), 2);
        assert_eq!(state_guard[0].title, "Task 1");
    }

    #[test]
    fn test_todo_update() {
        let state = new_todo_state();
        let write_tool = TodoWriteTool::new(state.clone());

        let params = json!({
            "title": "Test",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        write_tool.execute(params, &ctx).unwrap();

        let update_tool = TodoUpdateTool::new(state.clone());
        let update_params = json!({
            "id": "1",
            "status": "completed"
        });

        let result = update_tool.execute(update_params, &ctx).unwrap();
        assert!(result.text.contains("completed"));

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
    fn write_missing_title_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"todos": []}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn write_missing_todos_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"title": "X"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn write_invalid_status_errors() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(
            json!({
                "title": "T",
                "todos": [{"id": "1", "title": "A", "status": "bogus"}]
            }),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn write_empty_todos_clears_state() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");

        tool.execute(
            json!({
                "title": "Fill",
                "todos": [{"id": "1", "title": "A", "status": "pending"}]
            }),
            &ctx,
        )
        .unwrap();
        assert_eq!(state.lock().unwrap_or_else(|e| e.into_inner()).len(), 1);

        tool.execute(json!({"title": "Clear", "todos": []}), &ctx)
            .unwrap();
        assert!(state.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    }

    #[test]
    fn update_nonexistent_id_errors() {
        let state = new_todo_state();
        let tool = TodoUpdateTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"id": "missing", "status": "completed"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn update_shows_transition() {
        let state = new_todo_state();
        let write = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");
        write
            .execute(
                json!({
                    "title": "T",
                    "todos": [{"id": "1", "title": "A", "status": "pending"}]
                }),
                &ctx,
            )
            .unwrap();

        let update = TodoUpdateTool::new(state);
        let result = update
            .execute(json!({"id": "1", "status": "in_progress"}), &ctx)
            .unwrap();
        assert!(result.text.contains("pending"));
        assert!(result.text.contains("in progress"));
    }

    #[test]
    fn update_title_and_active_form() {
        let state = new_todo_state();
        let write = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");
        write
            .execute(
                json!({
                    "title": "T",
                    "todos": [{"id": "1", "title": "Old title", "status": "pending"}]
                }),
                &ctx,
            )
            .unwrap();

        let update = TodoUpdateTool::new(state.clone());
        let result = update
            .execute(
                json!({
                    "id": "1",
                    "title": "New title",
                    "activeForm": "Working on new title"
                }),
                &ctx,
            )
            .unwrap();
        assert!(result.text.contains("title → New title"));
        assert!(result.text.contains("activeForm → Working on new title"));

        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(guard[0].title, "New title");
        assert_eq!(
            guard[0].active_form.as_deref(),
            Some("Working on new title")
        );
    }

    #[test]
    fn update_without_fields_errors() {
        let state = new_todo_state();
        let tool = TodoUpdateTool::new(state);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(json!({"id": "1"}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one"));
    }

    #[test]
    fn write_with_active_form() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state.clone());

        let params = json!({
            "title": "Test",
            "todos": [
                {"id": "1", "title": "Fix bug", "status": "in_progress", "activeForm": "Fixing bug"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        tool.execute(params, &ctx).unwrap();

        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(guard[0].active_form.as_deref(), Some("Fixing bug"));
    }

    #[test]
    fn verification_nudge_on_all_completed() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);

        let params = json!({
            "title": "Done",
            "todos": [
                {"id": "1", "title": "A", "status": "completed"},
                {"id": "2", "title": "B", "status": "completed"},
                {"id": "3", "title": "C", "status": "completed"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(params, &ctx).unwrap();
        assert!(result.text.contains("Consider running tests"));
    }

    #[test]
    fn no_nudge_under_three_completed() {
        let state = new_todo_state();
        let tool = TodoWriteTool::new(state);

        let params = json!({
            "title": "Small",
            "todos": [
                {"id": "1", "title": "A", "status": "completed"},
                {"id": "2", "title": "B", "status": "completed"}
            ]
        });

        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(params, &ctx).unwrap();
        assert!(!result.text.contains("Consider running tests"));
    }

    #[test]
    fn format_status_values() {
        assert_eq!(format_status(TodoStatus::Pending), "pending");
        assert_eq!(format_status(TodoStatus::InProgress), "in progress");
        assert_eq!(format_status(TodoStatus::Completed), "completed");
    }

    #[test]
    fn tool_names() {
        let state = new_todo_state();
        assert_eq!(TodoWriteTool::new(state.clone()).name(), "todo_write");
        assert_eq!(TodoUpdateTool::new(state).name(), "todo_update");
    }

    #[test]
    fn tool_permissions() {
        let state = new_todo_state();
        assert_eq!(
            TodoWriteTool::new(state.clone()).permission(),
            ToolPermission::None
        );
        assert_eq!(
            TodoUpdateTool::new(state).permission(),
            ToolPermission::None
        );
    }

    #[test]
    fn write_with_storage_persists_to_sqlite() {
        let dir =
            std::env::temp_dir().join(format!("rustycode-todo-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");
        let storage = Arc::new(Storage::open(&db_path).unwrap());
        storage
            .insert_project(&rustycode_storage::task_store::Project {
                id: "test-project".into(),
                path: "/tmp".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let state = new_todo_state();
        let tool = TodoWriteTool::with_storage(state.clone(), storage.clone());
        let ctx = ToolContext::new("/tmp")
            .with_session_id("sess-1")
            .with_project_id("test-project");

        tool.execute(
            json!({
                "title": "Persist test",
                "todos": [
                    {"id": "1", "title": "Task A", "status": "pending"},
                    {"id": "2", "title": "Task B", "status": "completed"}
                ]
            }),
            &ctx,
        )
        .unwrap();

        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(guard.len(), 2);
        drop(guard);

        let db_todos = storage.get_todos("sess-1").unwrap();
        assert_eq!(db_todos.len(), 2);
        assert_eq!(db_todos[0].content, "Task A");
        assert_eq!(db_todos[1].content, "Task B");
    }

    #[test]
    fn write_with_storage_replaces_todos() {
        let dir =
            std::env::temp_dir().join(format!("rustycode-todo-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");
        let storage = Arc::new(Storage::open(&db_path).unwrap());
        storage
            .insert_project(&rustycode_storage::task_store::Project {
                id: "test-project".into(),
                path: "/tmp".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let state = new_todo_state();
        let tool = TodoWriteTool::with_storage(state.clone(), storage.clone());
        let ctx = ToolContext::new("/tmp")
            .with_session_id("sess-1")
            .with_project_id("test-project");

        tool.execute(
            json!({
                "title": "First batch",
                "todos": [
                    {"id": "1", "title": "A", "status": "pending"},
                    {"id": "2", "title": "B", "status": "pending"},
                    {"id": "3", "title": "C", "status": "pending"}
                ]
            }),
            &ctx,
        )
        .unwrap();

        tool.execute(
            json!({
                "title": "Second batch",
                "todos": [
                    {"id": "4", "title": "D", "status": "in_progress"},
                    {"id": "5", "title": "E", "status": "completed"}
                ]
            }),
            &ctx,
        )
        .unwrap();

        let db_todos = storage.get_todos("sess-1").unwrap();
        assert_eq!(db_todos.len(), 2);
        assert_eq!(db_todos[0].content, "D");
        assert_eq!(db_todos[1].content, "E");
    }

    #[test]
    fn update_with_storage_persists_status_change() {
        let dir =
            std::env::temp_dir().join(format!("rustycode-todo-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).ok();
        let db_path = dir.join("test.db");
        let storage = Arc::new(Storage::open(&db_path).unwrap());
        storage
            .insert_project(&rustycode_storage::task_store::Project {
                id: "test-project".into(),
                path: "/tmp".into(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let state = new_todo_state();
        let write = TodoWriteTool::with_storage(state.clone(), storage.clone());
        let ctx = ToolContext::new("/tmp")
            .with_session_id("sess-1")
            .with_project_id("test-project");

        write
            .execute(
                json!({
                    "title": "Test",
                    "todos": [{"id": "1", "title": "Task A", "status": "pending"}]
                }),
                &ctx,
            )
            .unwrap();

        let update = TodoUpdateTool::with_storage(state, storage.clone());
        update
            .execute(json!({"id": "1", "status": "completed"}), &ctx)
            .unwrap();

        let db_todos = storage.get_todos("sess-1").unwrap();
        assert_eq!(db_todos.len(), 1);
        assert_eq!(
            db_todos[0].status,
            rustycode_storage::task_store::TodoStatus::Completed
        );
    }
}
