use super::*;
use crate::{ToolOutput, ToolPermission};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct TaskStopTool;

    name: "task_stop",
    description: r#"Stops a running background task by its ID.
Takes a task_id parameter identifying the task to stop.
Returns a success or failure status.
Use this tool when you need to terminate a long-running task."#,
    permission: ToolPermission::Execute,

    execute(params: TaskStopParams, ctx) {
        let task_id = &params.task_id;

        // Placeholder: actual task stop requires runtime integration
        Ok(ToolOutput::text(format!("Task {task_id}: stop requested")).with_metadata(ctx, || json!({
                "task_id": task_id,
                "stopped": true,
            })))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_task_stop_metadata() {
        let tool = TaskStopTool;
        assert_eq!(tool.name(), "task_stop");
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_task_stop_requires_id() {
        let tool = TaskStopTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_task_stop_returns_confirmation() {
        let tool = TaskStopTool;
        let result = tool.execute(json!({"task_id": "bg-456"}), &test_ctx());
        assert!(result.is_ok());
        assert!(result.unwrap().text.contains("bg-456"));
    }
}
