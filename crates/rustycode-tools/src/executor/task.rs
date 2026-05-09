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

use super::task_state;
use crate::{ToolOutput, ToolPermission};
use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::Path;

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
    fn run(&self, cwd: &Path, description: &str, prompt: &str) -> Result<String>;
}

/// Parameters for the task tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct TaskParams {
    /// Short description of the task (used for logging)
    pub description: Option<String>,
    /// Detailed instructions for the sub-agent. Be specific about what to do,
    /// which files to modify, and what the expected outcome is.
    pub prompt: String,
}

rustycode_tools_api::define_tool! {
    pub struct TaskTool;

    name: "task",
    description: r#"Launch a focused sub-agent to handle a specific task autonomously.
The sub-agent has access to all tools (read_file, write_file, bash, etc.)
and runs independently until completion. Use this for delegating focused work
like implementing a feature, fixing a bug, or analyzing code."#,
    permission: ToolPermission::Execute,

    execute(params: TaskParams, ctx) {
        let description = params
            .description
            .unwrap_or_else(|| "unnamed task".to_string());

        if params.prompt.trim().is_empty() {
            anyhow::bail!("Parameter 'prompt' must not be empty");
        }

        tracing::info!("TaskTool: Starting sub-agent for: {description}");

        // Retrieve state from global store keyed by session_id
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");

        match task_state::get_task_state(session_id) {
            Some(state) => {
                match &state.runner {
                    Some(runner) => {
                        let start = std::time::Instant::now();
                        match runner(&state.cwd, &description, &params.prompt) {
                            Ok(output) => {
                                let elapsed = start.elapsed();
                                tracing::info!(
                                    "TaskTool: Sub-agent completed '{}' in {:.1}s ({} chars)",
                                    description,
                                    elapsed.as_secs_f64(),
                                    output.len()
                                );
                                Ok(ToolOutput::text(output))
                            }
                            Err(e) => {
                                let elapsed = start.elapsed();
                                tracing::warn!(
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
            None => Ok(ToolOutput::text(format!(
                "Sub-agent task '{description}' could not run: no task state configured for session '{session_id}'. \
                 This tool requires the TUI layer to set up task state."
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn setup_task_tool(session_id: &str) -> TaskTool {
        // Set up default state for the session
        task_state::set_task_state(session_id, PathBuf::from("/tmp"), None);
        TaskTool
    }

    #[test]
    fn test_task_tool_zero_sized() {
        let tool = TaskTool;
        assert_eq!(std::mem::size_of_val(&tool), 0);
    }

    #[test]
    fn test_task_tool_metadata() {
        let tool = TaskTool;

        assert_eq!(tool.name(), "task");
        assert!(tool.description().contains("sub-agent"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn test_task_tool_schema() {
        let tool = TaskTool;
        let schema = tool.parameters_schema();

        // description is Option<String> so only prompt is required
        let required = schema["required"]
            .as_array()
            .expect("required should be array");
        assert!(required.contains(&serde_json::Value::String("prompt".to_string())));
        assert!(schema["properties"]["description"].is_object());
        assert!(schema["properties"]["prompt"].is_object());
    }

    #[test]
    fn test_task_tool_missing_prompt() {
        let tool = setup_task_tool("test-missing-prompt");
        let ctx = ToolContext::new("/tmp").with_session_id("test-missing-prompt");

        let result = tool.execute(serde_json::json!({"description": "test"}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_task_tool_empty_prompt() {
        let tool = setup_task_tool("test-empty-prompt");
        let ctx = ToolContext::new("/tmp").with_session_id("test-empty-prompt");

        let result = tool.execute(
            serde_json::json!({"description": "test", "prompt": "  "}),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_task_tool_no_runner_configured() {
        let tool = setup_task_tool("test-no-runner");
        let ctx = ToolContext::new("/tmp").with_session_id("test-no-runner");

        let result = tool
            .execute(
                serde_json::json!({"description": "test task", "prompt": "do something"}),
                &ctx,
            )
            .unwrap();

        assert!(result.text.contains("no sub-agent runner configured"));
    }

    #[test]
    fn test_task_tool_no_state_configured() {
        let tool = TaskTool;
        let ctx = ToolContext::new("/tmp").with_session_id("nonexistent-session");

        let result = tool
            .execute(
                serde_json::json!({"description": "test task", "prompt": "do something"}),
                &ctx,
            )
            .unwrap();

        assert!(result.text.contains("no task state configured"));
    }

    #[test]
    fn test_task_tool_with_custom_runner() {
        let session_id = "test-with-runner";
        let runner: Arc<task_state::RunnerFn> =
            Arc::new(|_cwd, _desc, prompt| Ok(format!("Sub-agent completed: {}", prompt)));

        let cwd = PathBuf::from("/tmp/project");
        task_state::set_task_state(session_id, cwd.clone(), Some(runner));

        let tool = TaskTool;
        let ctx = ToolContext::new("/tmp").with_session_id(session_id);

        let result = tool
            .execute(
                serde_json::json!({"description": "test task", "prompt": "say hello"}),
                &ctx,
            )
            .unwrap();

        assert_eq!(result.text, "Sub-agent completed: say hello");
    }

    #[test]
    fn test_task_tool_session_isolation() {
        let session_1 = "task-tool-isolation-1";
        let session_2 = "task-tool-isolation-2";

        let cwd_1 = PathBuf::from("/tmp/project1");
        let cwd_2 = PathBuf::from("/tmp/project2");

        task_state::set_task_state(session_1, cwd_1.clone(), None);
        task_state::set_task_state(session_2, cwd_2.clone(), None);

        let state_1 = task_state::get_task_state(session_1).unwrap();
        let state_2 = task_state::get_task_state(session_2).unwrap();

        assert_eq!(state_1.cwd, cwd_1);
        assert_eq!(state_2.cwd, cwd_2);
    }

    #[test]
    fn test_task_tool_runner_injection() {
        let session_id = "test-runner-injection";
        let cwd = PathBuf::from("/tmp/test");

        // Start with no runner
        task_state::set_task_state(session_id, cwd.clone(), None);
        let state = task_state::get_task_state(session_id).unwrap();
        assert!(state.runner.is_none());

        // Inject a runner
        let runner: Arc<task_state::RunnerFn> =
            Arc::new(|_, _, _| Ok("injected runner output".to_string()));
        task_state::set_task_runner(session_id, Arc::clone(&runner));

        // Verify runner is now available
        let updated_state = task_state::get_task_state(session_id).unwrap();
        assert!(updated_state.runner.is_some());
        assert_eq!(updated_state.cwd, cwd);
    }
}
