use super::*;
use crate::{ToolOutput, ToolPermission};
use anyhow::anyhow;

rustycode_tools_api::define_tool! {
    pub struct TodoWriteTool;

    name: "TodoWrite",
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
                _ => return Err(anyhow!("Invalid status: {}", item.status)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use serde_json::json;

    #[test]
    fn test_todo_write() {
        let params = serde_json::from_value(json!({
            "title": "Test todos",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"},
                {"id": "2", "title": "Task 2", "status": "in_progress"}
            ]
        }))
        .unwrap();

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
}
