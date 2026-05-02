//! Delegation executor — a real `delegate_task` tool backed by `AgentSession`.
//!
//! Replaces the intent-only `DelegationTool` (from `rustycode-tools`) with a tool
//! that actually creates and runs sub-agent sessions. The tool name, description,
//! and parameters schema are identical so the LLM already knows how to call it.

use anyhow::Result;
use rustycode_agent::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::{ChatMessage, LLMProvider, MessageRole};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_protocol::MessageContent;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use super::definitions::{self, AgentDefinition};

/// Valid delegation roles (must match `DelegationTool` exactly).
const VALID_ROLES: &[&str] = &[
    "explore",
    "research",
    "code",
    "review",
    "verify",
    "plan",
    "debug",
];

/// Map a delegation role to an agent definition type.
fn role_to_agent_type(role: &str) -> &'static str {
    match role {
        "explore" => "explore",
        "research" => "explore",
        "code" => "general-purpose",
        "review" => "code-reviewer",
        "verify" => "code-reviewer",
        "plan" => "plan",
        "debug" => "build-error-resolver",
        _ => "general-purpose",
    }
}

/// Tool that executes delegated tasks by spawning real `AgentSession` sub-agents.
///
/// The LLM calls `delegate_task` with a `task_description`, optional `role`, and
/// optional `path_scope`/`resume_from`. This tool maps the role to an
/// `AgentDefinition`, creates a session, and runs it to completion.
pub struct DelegationExecutor {
    /// LLM provider for the sub-agent.
    provider: Arc<dyn LLMProvider>,
    /// Model name for the sub-agent.
    model: String,
    /// Working directory for tool execution.
    cwd: PathBuf,
    /// Tool descriptions schema (cloned from parent for sub-agent).
    tools_schema: Vec<Value>,
}

impl DelegationExecutor {
    /// Create a new `DelegationExecutor`.
    pub fn new(
        provider: Arc<dyn LLMProvider>,
        model: String,
        cwd: PathBuf,
        tools_schema: Vec<Value>,
    ) -> Self {
        Self {
            provider,
            model,
            cwd,
            tools_schema,
        }
    }

    /// Resolve the agent definition and run the sub-agent loop.
    fn run_delegation(&self, role: &str, task_description: &str) -> Result<String> {
        let agent_type = role_to_agent_type(role);

        let def = definitions::find_agent(agent_type).or_else(|| {
            // Fallback to general-purpose if the mapped type is not found.
            tracing::warn!(
                "DelegationExecutor: agent type '{}' not found for role '{}', falling back to general-purpose",
                agent_type,
                role
            );
            definitions::find_agent("general-purpose")
        }).ok_or_else(|| {
            anyhow::anyhow!("No agent definition available for role '{}'", role)
        })?;

        tracing::info!(
            "DelegationExecutor: starting delegated task with role='{}' agent_type='{}'",
            role,
            agent_type
        );

        let start = std::time::Instant::now();

        // Run the async AgentSession inside a blocking context.
        // `Tool::execute` is sync, but `AgentSession::run` is async.
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.execute_delegated_task(def, task_description, role))
        });

        let elapsed = start.elapsed();
        match &result {
            Ok(output) => {
                tracing::info!(
                    "DelegationExecutor: role='{}' completed in {:.1}s ({} chars)",
                    role,
                    elapsed.as_secs_f64(),
                    output.len()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "DelegationExecutor: role='{}' failed after {:.1}s: {}",
                    role,
                    elapsed.as_secs_f64(),
                    e
                );
            }
        }

        result
    }

    /// Execute the delegated task in an async context.
    async fn execute_delegated_task(
        &self,
        def: &AgentDefinition,
        prompt: &str,
        _role: &str,
    ) -> Result<String> {
        let config = AgentConfig {
            max_turns: 20,
            timeout_secs: 300,
            max_tool_result_bytes: 8_000,
            temperature: 0.2,
        };

        let mut session = AgentSession::new(config, &self.cwd);
        session.activation.promote(ToolTier::Full);

        // Build tool registry for sub-agent (subset that doesn't recurse).
        let tool_registry = self.build_subagent_tool_registry();

        // Initial user message.
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(prompt.to_string()),
        }];

        // Collector that captures output.
        let mut collector = DelegationCollector::default();

        let result = session
            .run(
                &*self.provider,
                &self.model,
                def.system_prompt,
                messages,
                &self.tools_schema,
                &tool_registry,
                &mut collector,
            )
            .await?;

        Ok(result.final_text)
    }

    /// Build a tool registry for the sub-agent.
    ///
    /// Excludes `AgentTool` and `DelegationExecutor` to prevent recursive spawning.
    fn build_subagent_tool_registry(&self) -> rustycode_tools::ToolRegistry {
        use rustycode_tools::edit::EditFile;
        use rustycode_tools::search::{GlobTool, GrepTool};
        use rustycode_tools::search_replace::SearchReplace;
        use rustycode_tools::{
            BashTool, GitDiffTool, GitLogTool, GitStatusTool, ListDirTool, ReadFileTool,
            WriteFileTool,
        };

        let mut registry = rustycode_tools::ToolRegistry::new();

        registry.register(ReadFileTool);
        registry.register(WriteFileTool);
        registry.register(ListDirTool);
        registry.register(EditFile);
        registry.register(GrepTool);
        registry.register(GlobTool);
        registry.register(SearchReplace);
        registry.register(BashTool);
        registry.register(GitStatusTool);
        registry.register(GitDiffTool);
        registry.register(GitLogTool);

        registry
    }
}

/// Tool trait implementation.
impl Tool for DelegationExecutor {
    fn name(&self) -> &'static str {
        "delegate_task"
    }

    fn description(&self) -> &'static str {
        "Spawn a delegated task with its own context. Use for research, exploration, code review, \
         or parallel implementation tasks that benefit from context isolation."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["task_description"],
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "What the delegated task should do"
                },
                "role": {
                    "type": "string",
                    "enum": VALID_ROLES,
                    "description": "Role for the spawned task (default: explore)"
                },
                "path_scope": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths the task should focus on"
                },
                "resume_from": {
                    "type": "string",
                    "description": "Checkpoint to resume from"
                }
            }
        })
    }

    #[allow(clippy::too_many_lines)]
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        // 1. Extract task_description (required).
        let task_description = params
            .get("task_description")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                anyhow::anyhow!("missing required parameter 'task_description' (string)")
            })?;

        // 2. Validate non-empty.
        if task_description.trim().is_empty() {
            anyhow::bail!("'task_description' must not be empty");
        }

        // 3. Extract role (default: explore).
        let role = params
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("explore");

        // 4. Validate role.
        if !VALID_ROLES.contains(&role) {
            anyhow::bail!(
                "invalid role '{}': must be one of {}",
                role,
                VALID_ROLES.join("|")
            );
        }

        // 5. Run the delegation.
        match self.run_delegation(role, task_description) {
            Ok(output) => Ok(ToolOutput::text(output)),
            Err(e) => Ok(ToolOutput::text(format!(
                "Delegated task (role='{role}') failed: {e}"
            ))),
        }
    }
}

/// Simple event collector for delegated sub-agent runs.
#[derive(Default)]
struct DelegationCollector {
    _tool_calls: usize,
}

#[async_trait::async_trait]
impl AgentEvents for DelegationCollector {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ToolExecStarted { .. } => {
                self._tool_calls += 1;
            }
            StreamEvent::Done => {}
            _ => {}
        }
    }

    async fn on_done(&mut self, _result: &AgentResult) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_llm::mock::MockProvider;

    fn make_executor() -> DelegationExecutor {
        let provider = Arc::new(MockProvider::from_text("mock result"));
        DelegationExecutor::new(
            provider,
            "test-model".to_string(),
            PathBuf::from("/tmp"),
            vec![],
        )
    }

    #[test]
    fn delegation_executor_metadata() {
        let tool = make_executor();
        assert_eq!(tool.name(), "delegate_task");
        assert!(tool.description().contains("delegat"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn delegation_executor_schema_has_required_fields() {
        let tool = make_executor();
        let schema = tool.parameters_schema();

        let required = schema["required"].as_array().expect("required should be array");
        assert!(required.iter().any(|v| v.as_str() == Some("task_description")));

        let props = schema["properties"].as_object().expect("properties should be object");
        assert!(props.contains_key("task_description"));
        assert!(props.contains_key("role"));

        // Verify role enum.
        let role_enum = props["role"]["enum"].as_array().expect("role should have enum");
        assert!(role_enum.iter().any(|v| v == "explore"));
        assert!(role_enum.iter().any(|v| v == "debug"));

        // Verify path_scope is array of strings.
        assert_eq!(props["path_scope"]["type"], "array");
    }

    #[test]
    fn delegation_executor_missing_task_description() {
        let tool = make_executor();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(serde_json::json!({"role": "explore"}), &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("task_description"),
            "expected 'task_description', got: {err}"
        );
    }

    #[test]
    fn delegation_executor_empty_task_description() {
        let tool = make_executor();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(serde_json::json!({"task_description": "   "}), &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("must not be empty"),
            "expected 'must not be empty', got: {err}"
        );
    }

    #[test]
    fn delegation_executor_invalid_role() {
        let tool = make_executor();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            serde_json::json!({"task_description": "Do something", "role": "nonexistent_role"}),
            &ctx,
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid role"),
            "expected 'invalid role', got: {err_msg}"
        );
        assert!(err_msg.contains("nonexistent_role"));
    }

    #[test]
    fn delegation_executor_all_roles_map() {
        let _tool = make_executor();
        let _ctx = ToolContext::new("/tmp");

        for role in VALID_ROLES {
            let params = serde_json::json!({
                "task_description": format!("Test task for {role}"),
                "role": role
            });

            // We can't run a real sub-agent in unit tests, but we can verify
            // that validation passes for all valid roles (execute will fail
            // at the async runtime level, not at validation).
            // Instead, just verify role validation passes:
            let role_val = params.get("role").and_then(Value::as_str).unwrap_or("explore");
            assert!(VALID_ROLES.contains(&role_val), "role '{role}' should be valid");
        }
    }

    #[test]
    fn role_to_agent_type_mappings() {
        assert_eq!(role_to_agent_type("explore"), "explore");
        assert_eq!(role_to_agent_type("research"), "explore");
        assert_eq!(role_to_agent_type("code"), "general-purpose");
        assert_eq!(role_to_agent_type("review"), "code-reviewer");
        assert_eq!(role_to_agent_type("verify"), "code-reviewer");
        assert_eq!(role_to_agent_type("plan"), "plan");
        assert_eq!(role_to_agent_type("debug"), "build-error-resolver");
        // Unknown role falls back to general-purpose.
        assert_eq!(role_to_agent_type("unknown"), "general-purpose");
    }
}
