//! `TaskTool` — lets the LLM delegate focused subtasks to a sub-agent.
//!
//! Inspired by Claude Code's `AgentTool`, Kilocode's `Task` tool, and `OpenCode`'s
//! sub-agent pattern. The LLM calls this tool to spawn a nested tool-use loop
//! with a fresh message history.
//!
//! # Architecture
//!
//! The tool definition lives here in `rustycode-tools`, but actual LLM interaction
//! is delegated via the `SubAgentRunner` trait. The TUI layer provides the
//! implementation that creates nested LLM conversations.
//!
//! # Key Design Decisions
//!
//! - **Fresh message history**: No context pollution from parent conversation
//! - **Same tool registry**: Sub-agent has full access to all tools
//! - **Bounded execution**: Max 10 turns per sub-agent to prevent infinite loops
//! - **Timeout**: 5min total per sub-agent task

use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::Result;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Maximum number of tool-use turns a sub-agent can take
pub const MAX_SUB_AGENT_TURNS: usize = 10;

/// Maximum total execution time for a sub-agent task
pub const MAX_SUB_AGENT_DURATION_SECS: u64 = 300;

/// Trait for running sub-agent tasks.
///
/// Implemented by the TUI layer where LLM provider access is available.
/// The tools crate only defines the interface.
pub trait SubAgentRunner: Send + Sync {
    /// Run a sub-agent task and return the final output.
    ///
    /// # Arguments
    /// * `cwd` - Working directory for tool execution
    /// * `description` - Short task description (for logging)
    /// * `prompt` - Detailed instructions for the sub-agent
    ///
    /// # Returns
    /// The sub-agent's final response text.
    fn run(&self, cwd: &Path, description: &str, prompt: &str) -> Result<String>;
}

/// Type-erased runner stored in the tool
type RunnerFn = dyn Fn(&Path, &str, &str) -> Result<String> + Send + Sync;

/// Tool for delegating focused subtasks to a sub-agent
///
/// The sub-agent runs a nested LLM conversation with tool access,
/// executing autonomously until the task is complete or limits are reached.
pub struct TaskTool {
    /// Working directory for tool execution
    cwd: PathBuf,
    /// Optional sub-agent runner (injected from TUI layer)
    runner: Option<Arc<RunnerFn>>,
}

impl TaskTool {
    /// Create a new `TaskTool` with default (no-op) runner.
    /// Without a runner, the tool returns a message asking the user to configure one.
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd, runner: None }
    }

    /// Create a `TaskTool` with a custom sub-agent runner function.
    ///
    /// Use this from the TUI layer to inject LLM-based sub-agent execution.
    pub fn with_runner<F>(cwd: PathBuf, runner: F) -> Self
    where
        F: Fn(&Path, &str, &str) -> Result<String> + Send + Sync + 'static,
    {
        Self {
            cwd,
            runner: Some(Arc::new(runner)),
        }
    }

    /// Create a `TaskTool` with a boxed `SubAgentRunner` trait object.
    pub fn with_sub_agent_runner(cwd: PathBuf, runner: Box<dyn SubAgentRunner>) -> Self {
        let runner_arc: Arc<RunnerFn> =
            Arc::new(move |cwd, desc, prompt| runner.run(cwd, desc, prompt));
        Self {
            cwd,
            runner: Some(runner_arc),
        }
    }
}

impl Tool for TaskTool {
    fn name(&self) -> &'static str {
        "task"
    }

    fn description(&self) -> &'static str {
        "Launch a focused sub-agent to handle a specific task autonomously. \
         The sub-agent has access to all tools (read_file, write_file, bash, etc.) \
         and runs independently until completion. Use this for delegating focused work \
         like implementing a feature, fixing a bug, or analyzing code."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["description", "prompt"],
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Short description of the task (used for logging)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Detailed instructions for the sub-agent. Be specific about what to do, which files to modify, and what the expected outcome is."
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let description = params["description"]
            .as_str()
            .unwrap_or("unnamed task")
            .to_string();

        let prompt = params["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prompt"))?
            .to_string();

        if prompt.trim().is_empty() {
            anyhow::bail!("Parameter 'prompt' must not be empty");
        }

        log::info!("TaskTool: Starting sub-agent for: {description}");

        match &self.runner {
            Some(runner) => {
                let start = std::time::Instant::now();
                match runner(&self.cwd, &description, &prompt) {
                    Ok(output) => {
                        let elapsed = start.elapsed();
                        log::info!(
                            "TaskTool: Sub-agent completed '{}' in {:.1}s ({} chars)",
                            description,
                            elapsed.as_secs_f64(),
                            output.len()
                        );
                        Ok(ToolOutput::text(output))
                    }
                    Err(e) => {
                        let elapsed = start.elapsed();
                        log::warn!(
                            "TaskTool: Sub-agent failed '{}' after {:.1}s: {}",
                            description,
                            elapsed.as_secs_f64(),
                            e
                        );
                        Ok(ToolOutput::text(format!(
                            "Sub-agent task '{description}' failed: {e}"
                        )))
                    }
                }
            }
            None => Ok(ToolOutput::text(format!(
                "Sub-agent task '{description}' could not run: no sub-agent runner configured. \
                 This tool requires the TUI layer to inject an LLM runner."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_tool_schema() {
        let tool = TaskTool::new(PathBuf::from("/tmp"));
        let schema = tool.parameters_schema();

        assert_eq!(schema["required"][0], "description");
        assert_eq!(schema["required"][1], "prompt");
        assert!(schema["properties"]["description"].is_object());
        assert!(schema["properties"]["prompt"].is_object());
    }

    #[test]
    fn test_task_tool_metadata() {
        let tool = TaskTool::new(PathBuf::from("/tmp"));

        assert_eq!(tool.name(), "task");
        assert!(tool.description().contains("sub-agent"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_task_tool_missing_prompt() {
        let tool = TaskTool::new(PathBuf::from("/tmp"));
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(serde_json::json!({"description": "test"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_task_tool_empty_prompt() {
        let tool = TaskTool::new(PathBuf::from("/tmp"));
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            serde_json::json!({"description": "test", "prompt": "  "}),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_task_tool_no_runner_returns_message() {
        let tool = TaskTool::new(PathBuf::from("/tmp"));
        let ctx = ToolContext::new("/tmp");

        let result = tool
            .execute(
                serde_json::json!({"description": "test task", "prompt": "do something"}),
                &ctx,
            )
            .unwrap();

        assert!(result.text.contains("no sub-agent runner configured"));
    }

    #[test]
    fn test_task_tool_with_custom_runner() {
        let runner =
            |_cwd: &Path, _desc: &str, prompt: &str| Ok(format!("Sub-agent completed: {}", prompt));

        let tool = TaskTool::with_runner(PathBuf::from("/tmp"), runner);
        let ctx = ToolContext::new("/tmp");

        let result = tool
            .execute(
                serde_json::json!({"description": "test task", "prompt": "say hello"}),
                &ctx,
            )
            .unwrap();

        assert_eq!(result.text, "Sub-agent completed: say hello");
    }
}
