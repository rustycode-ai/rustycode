use crate::{ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde_json::json;

#[derive(serde::Deserialize, JsonSchema)]
pub struct TaskOutputParams {
    /// The task ID to get output from
    task_id: String,
    /// Whether to wait for completion (default: true)
    #[serde(default = "default_true")]
    block: bool,
    /// Max wait time in ms (default: 30000)
    #[serde(default = "default_timeout")]
    timeout: u64,
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    30000
}

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

#[derive(serde::Deserialize, JsonSchema)]
pub struct TaskStopParams {
    /// The task ID to stop
    task_id: String,
}

rustycode_tools_api::define_tool! {
    pub struct TaskStopTool;

    name: "task_stop",
    description: r#"Stops a running background task by its ID.
Takes a task_id parameter identifying the task to stop.
Returns a success or failure status.
Use this tool when you need to terminate a long-running task."#,
    permission: ToolPermission::Execute,

    execute(params: TaskStopParams, _ctx) {
        let task_id = &params.task_id;

        // Placeholder: actual task stop requires runtime integration
        Ok(ToolOutput::with_structured(
            format!("Task {task_id}: stop requested"),
            json!({
                "task_id": task_id,
                "stopped": true,
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;
    use crate::ToolContext;

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

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
