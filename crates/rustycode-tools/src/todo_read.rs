//! `TodoRead` tool — lets the LLM read the current todo list.
//!
//! Complements `TodoWriteTool` (create/update) and `TodoUpdateTool` (update single)
//! by providing a read-only view of the current todo state.

use crate::todo::TodoState;
use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde_json::Value;

/// Read the current todo list
pub struct TodoReadTool {
    /// Shared todo state
    pub state: TodoState,
}

impl TodoReadTool {
    /// Create a new `TodoReadTool` with shared state
    pub const fn new(state: TodoState) -> Self {
        Self { state }
    }
}

impl Tool for TodoReadTool {
    fn name(&self) -> &'static str {
        "todo_read"
    }

    fn description(&self) -> &'static str {
        "Read the current todo list. Returns all items with their IDs, titles, and statuses (pending/in_progress/completed). Use this before calling todo_write or todo_update to understand the current state."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "description": "No parameters needed. Returns the full current todo list."
        })
    }

    fn execute(&self, _params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.is_empty() {
            return Ok(ToolOutput::text(
                "No todos. Use todo_write to create a list.",
            ));
        }

        let mut lines = Vec::with_capacity(state.len() + 1);
        lines.push(format!("Todo list ({} items):", state.len()));

        for item in state.iter() {
            let icon = match item.status {
                crate::todo::TodoStatus::Pending => "⏳",
                crate::todo::TodoStatus::InProgress => "🔄",
                crate::todo::TodoStatus::Completed => "✅",
            };
            lines.push(format!(
                "{} [{}] {} ({})",
                icon,
                item.id,
                item.title,
                match item.status {
                    crate::todo::TodoStatus::Pending => "pending",
                    crate::todo::TodoStatus::InProgress => "in_progress",
                    crate::todo::TodoStatus::Completed => "completed",
                }
            ));
        }

        let completed = state
            .iter()
            .filter(|t| matches!(t.status, crate::todo::TodoStatus::Completed))
            .count();

        lines.push(format!(
            "\nProgress: {}/{} completed",
            completed,
            state.len()
        ));

        Ok(ToolOutput::text(lines.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::todo::TodoWriteTool;
    use serde_json::json;

    #[test]
    fn test_todo_read_empty() {
        let state = crate::todo::new_todo_state();
        let tool = TodoReadTool::new(state);
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(json!({}), &ctx).unwrap();
        assert!(result.text.contains("No todos"));
    }

    #[test]
    fn test_todo_read_with_items() {
        let state = crate::todo::new_todo_state();

        // Write some todos first
        let write_tool = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");
        write_tool
            .execute(
                json!({
                    "title": "Test",
                    "todos": [
                        {"id": "1", "title": "Task A", "status": "completed"},
                        {"id": "2", "title": "Task B", "status": "in_progress"},
                        {"id": "3", "title": "Task C", "status": "pending"}
                    ]
                }),
                &ctx,
            )
            .unwrap();

        // Now read them
        let read_tool = TodoReadTool::new(state);
        let result = read_tool.execute(json!({}), &ctx).unwrap();

        assert!(result.text.contains("3 items"));
        assert!(result.text.contains("Task A"));
        assert!(result.text.contains("Task B"));
        assert!(result.text.contains("Task C"));
        assert!(result.text.contains("1/3 completed"));
        assert!(result.text.contains("✅"));
        assert!(result.text.contains("🔄"));
        assert!(result.text.contains("⏳"));
    }

    // --- Tool metadata ---

    #[test]
    fn tool_name() {
        let state = crate::todo::new_todo_state();
        let tool = TodoReadTool::new(state);
        assert_eq!(tool.name(), "todo_read");
    }

    #[test]
    fn tool_permission() {
        let state = crate::todo::new_todo_state();
        let tool = TodoReadTool::new(state);
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn tool_schema_is_valid() {
        let state = crate::todo::new_todo_state();
        let tool = TodoReadTool::new(state);
        let schema = tool.parameters_schema();
        assert!(schema.is_object());
    }

    #[test]
    fn read_all_completed() {
        let state = crate::todo::new_todo_state();
        let write = TodoWriteTool::new(state.clone());
        let ctx = ToolContext::new("/tmp");
        write
            .execute(
                json!({
                    "title": "Done",
                    "todos": [
                        {"id": "1", "title": "A", "status": "completed"},
                        {"id": "2", "title": "B", "status": "completed"}
                    ]
                }),
                &ctx,
            )
            .unwrap();

        let read = TodoReadTool::new(state);
        let result = read.execute(json!({}), &ctx).unwrap();
        assert!(result.text.contains("2/2 completed"));
    }
}
