use super::*;
use crate::{ToolOutput, ToolPermission};
use anyhow::anyhow;

rustycode_tools_api::define_tool! {
    pub struct TodoUpdateTool;


    name: "TodoUpdate",
    namespace: "todo",
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
                _ => Err(anyhow!("Invalid status: {s}")),
            })
            .transpose()?;

        let new_title = params.title.clone();
        let new_active_form = params.active_form.clone();

        if new_status.is_none() && new_title.is_none() && new_active_form.is_none() {
            return Err(anyhow!(
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
            .ok_or_else(|| anyhow!("Todo item not found: {id}"))?;

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
    fn test_todo_update() {
        let ctx = ToolContext::new("/tmp").with_session_id("test-session-2");

        let params = serde_json::from_value(json!({
            "title": "Test",
            "todos": [
                {"id": "1", "title": "Task 1", "status": "pending"}
            ]
        }))
        .unwrap();

        let write_tool = TodoWriteTool;
        write_tool.execute(params, &ctx).unwrap();

        let update_tool = TodoUpdateTool;
        let update_params = serde_json::from_value(json!({
            "id": "1",
            "status": "completed"
        }))
        .unwrap();

        let result = update_tool.execute(update_params, &ctx).unwrap();
        assert!(result.text.contains("completed"));

        let state = get_or_create_todo_state("test-session-2");
        let state_guard = state.lock().unwrap_or_else(|e| e.into_inner());
        assert!(matches!(state_guard[0].status, TodoStatus::Completed));
    }
}
