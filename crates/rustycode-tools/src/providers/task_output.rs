use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

/// Retrieve output from a running or completed background task.
pub struct TaskOutputTool;

impl Tool for TaskOutputTool {
    fn name(&self) -> &'static str {
        "task_output"
    }

    fn description(&self) -> &'static str {
        r#"Retrieves output from a running or completed background task.

- For bash tasks: prefer using the Read tool on the output file path
- For local_agent tasks: use the Agent tool result directly
- For remote_agent tasks: prefer using the Read tool on the output file path

Takes a task_id parameter identifying the task. Returns the task output along with status information.
Use block=true (default) to wait for completion, block=false for non-blocking status check."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::None
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to get output from"
                },
                "block": {
                    "type": "boolean",
                    "default": true,
                    "description": "Whether to wait for completion (default: true)"
                },
                "timeout": {
                    "type": "number",
                    "description": "Max wait time in ms (default: 30000)",
                    "minimum": 0,
                    "maximum": 600000
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing task_id"))?;

        let block = params.get("block").and_then(Value::as_bool).unwrap_or(true);

        let timeout = params
            .get("timeout")
            .and_then(Value::as_u64)
            .unwrap_or(30000);

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

/// Stop a running background task by its ID.
pub struct TaskStopTool;

impl Tool for TaskStopTool {
    fn name(&self) -> &'static str {
        "task_stop"
    }

    fn description(&self) -> &'static str {
        r#"Stops a running background task by its ID.
Takes a task_id parameter identifying the task to stop.
Returns a success or failure status.
Use this tool when you need to terminate a long-running task."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["task_id"],
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to stop"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let task_id = params
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("missing task_id"))?;

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
