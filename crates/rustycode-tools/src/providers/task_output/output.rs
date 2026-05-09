use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use serde_json::json;

rustycode_tools_api::define_tool! {
    pub struct TaskOutputTool;

    name: "task_output",
    description: r#"Retrieves output from a running or completed background task.

- For bash tasks: prefer using the Read tool on the output file path
- For local_agent tasks: use the Agent tool result directly
- For remote_agent tasks: prefer using the Read tool on the output file path

Takes a task_id parameter identifying the task. Returns the task output along with status information.
Use block=true (default) to wait for completion, block=false for non-blocking status check."#,
    permission: ToolPermission::None,
    tags: [ToolTag::Ops],

    execute(params: TaskOutputParams, _ctx) {
        let task_id = &params.task_id;
        let block = params.block;
        let timeout = params.timeout;

        // Placeholder: actual task lookup requires runtime integration
        Ok(ToolOutput::with_structured(
            format!("Task {task_id}: status query (block={block}, timeout={timeout}ms)"),
            json!({
                "task_id": task_id,
                "block": block,
                "timeout": timeout,
                "status": "pending_integration",
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_common::*;
    use super::*;
    use crate::Tool;

    #[test]
    fn test_task_output_metadata() {
        let tool = TaskOutputTool;
        assert_eq!(tool.name(), "task_output");
        assert_eq!(tool.permission(), ToolPermission::None);
    }

    #[test]
    fn test_task_output_requires_id() {
        let tool = TaskOutputTool;
        let result = tool.execute(json!({}), &test_ctx());
        assert!(result.is_err());
    }

    #[test]
    fn test_task_output_returns_status() {
        let tool = TaskOutputTool;
        let result = tool.execute(json!({"task_id": "bg-123", "block": false}), &test_ctx());
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.text.contains("bg-123"));
    }
}
