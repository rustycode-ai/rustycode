//! `TodoRead` tool — lets the LLM read the current todo list.
//!
//! Complements `TodoWriteTool` (create/update) and `TodoUpdateTool` (update single)
//! by providing a read-only view of the current todo state.

use crate::todo::{get_or_create_todo_state, TodoStatus};
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde_json::Value;

#[derive(serde::Deserialize, JsonSchema)]
pub struct TodoReadParams {}

// Zero-sized tool struct
#[derive(Debug, Clone, Copy)]
pub struct TodoReadTool;

impl Tool for TodoReadTool {
    fn name(&self) -> &'static str {
        "todo_read"
    }

    fn description(&self) -> &'static str {
        r#"Read the current todo list.

Returns the full list of uncompleted and completed tasks. Use this tool
to understand what tasks are pending, or to verify if a task was successfully
marked as completed."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Read
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn execute(&self, _params: Value, ctx: &ToolContext) -> anyhow::Result<ToolOutput> {
        // Retrieve state from global store, keyed by session_id
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");
        let state = get_or_create_todo_state(session_id);

        let state_guard = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state_guard.is_empty() {
            return Ok(ToolOutput::text(
                "No todos. Use todo_write to create a list.",
            ));
        }

        let mut lines = Vec::with_capacity(state_guard.len() + 1);
        lines.push(format!("Todo list ({} items):", state_guard.len()));

        for item in state_guard.iter() {
            let icon = match item.status {
                TodoStatus::Pending => "⏳",
                TodoStatus::InProgress => "🔄",
                TodoStatus::Completed => "✅",
            };
            lines.push(format!(
                "{} [{}] {} ({})",
                icon,
                item.id,
                item.title,
                match item.status {
                    TodoStatus::Pending => "pending",
                    TodoStatus::InProgress => "in_progress",
                    TodoStatus::Completed => "completed",
                }
            ));
        }

        let completed = state_guard
            .iter()
            .filter(|t| matches!(t.status, TodoStatus::Completed))
            .count();

        lines.push(format!(
            "\nProgress: {}/{} completed",
            completed,
            state_guard.len()
        ));

        Ok(ToolOutput::text(lines.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_read_empty() {
        let ctx = ToolContext::new("/tmp").with_session_id("read-test-empty");
        let tool = TodoReadTool;
        let result = tool
            .execute(serde_json::Value::Object(Default::default()), &ctx)
            .unwrap();
        assert!(result.text.contains("No todos"));
    }

    #[test]
    fn test_todo_read_with_items() {
        let ctx = ToolContext::new("/tmp").with_session_id("read-test-items");

        // Create some todos first
        let write_tool = crate::todo::TodoWriteTool;
        let _ = write_tool
            .execute(
                serde_json::json!({
                    "title": "Test",
                    "todos": [
                        {"id": "1", "title": "Task 1", "status": "pending"},
                        {"id": "2", "title": "Task 2", "status": "completed"}
                    ]
                }),
                &ctx,
            )
            .unwrap();

        let read_tool = TodoReadTool;
        let result = read_tool
            .execute(serde_json::Value::Object(Default::default()), &ctx)
            .unwrap();

        assert!(result.text.contains("Task 1"));
        assert!(result.text.contains("Task 2"));
        assert!(result.text.contains("2 items"));
        assert!(result.text.contains("1/2 completed"));
    }
}
