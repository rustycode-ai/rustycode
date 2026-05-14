//! Delegation executor — a real `delegate_task` tool backed by `AgentSession`.

use anyhow::Result;
use rustycode_agent_runtime::{AgentConfig, AgentEvents, AgentResult, AgentSession, TaskBrief};
use rustycode_llm::provider::{ChatMessage, LLMProvider, MessageRole};
use rustycode_orchestration::cost_table::calculate_cost;
use rustycode_orchestration::delegation::{
    DelegationConfig, DelegationContext, DelegationPlanner, SpawnDecision, TaskRole,
};
use rustycode_orchestration::task_dispatcher::{TaskDispatcher, TaskResult};
use rustycode_orchestration::task_runner::{TaskRunResult, TaskRunner};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_protocol::MessageContent;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use super::definitions::{self, AgentDefinition};

// Role mapping helpers

/// Parse a string role (from LLM tool call) into a `TaskRole` enum.
fn parse_task_role(role: &str) -> TaskRole {
    match role {
        "explore" => TaskRole::Explore,
        "research" => TaskRole::Research,
        "code" => TaskRole::Code,
        "review" => TaskRole::Review,
        "verify" => TaskRole::Verify,
        "plan" => TaskRole::Plan,
        "debug" => TaskRole::Debug,
        _ => TaskRole::Explore,
    }
}

/// Map a `TaskRole` to the agent definition type used by `definitions::find_agent`.
fn task_role_to_agent_type(role: TaskRole) -> &'static str {
    match role {
        TaskRole::Explore | TaskRole::Research => "explore",
        TaskRole::Code => "general-purpose",
        TaskRole::Review | TaskRole::Verify => "code-reviewer",
        TaskRole::Plan => "plan",
        TaskRole::Debug => "build-error-resolver",
    }
}

/// Return all valid role strings for the tool schema.
const VALID_ROLE_STRINGS: [&str; 7] = [
    "explore", "research", "code", "review", "verify", "plan", "debug",
];

fn valid_role_strings() -> &'static [&'static str] {
    &VALID_ROLE_STRINGS
}

fn enrich_task_prompt(
    task_description: &str,
    path_scope: &[PathBuf],
    resume_from: Option<&str>,
) -> String {
    let mut prompt = task_description.to_string();

    if let Some(checkpoint) = resume_from {
        prompt = format!(
            "[Resuming from previous context: {checkpoint}]\n\n{prompt}\n\n\
             Note: You are continuing work from a previous session."
        );
    }

    if !path_scope.is_empty() {
        let paths_str = path_scope
            .iter()
            .map(|p| format!("- {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        prompt = format!("{prompt}\n\nFocus on these paths:\n{paths_str}");
    }

    prompt
}

/// Build a `TaskBrief` from the delegation inputs.
///
/// Mirrors `task_dispatcher::task_spec_to_task_brief()` so both execution
/// paths produce identical contracts.
fn build_delegation_brief(role: TaskRole, path_scope: &[PathBuf], prompt: &str) -> TaskBrief {
    use rustycode_protocol::agent_protocol::AgentRole;
    use std::convert::TryInto;

    let agent_role: AgentRole = role.try_into().unwrap_or_else(|e| {
        tracing::warn!("build_delegation_brief: {e}, falling back to Researcher");
        AgentRole::Researcher
    });
    let allowed_tools: Vec<String> = role
        .allowed_tools()
        .iter()
        .map(|s| (*s).to_string())
        .collect();

    TaskBrief {
        role: agent_role,
        brief: prompt.to_string(),
        path_scope: path_scope.to_vec(),
        allowed_tools,
    }
}

// DelegationExecutor

/// Tool that executes delegated tasks by spawning real `AgentSession` sub-agents.
///
/// The LLM calls `delegate_task` with a `task_description`, optional `role`, and
/// optional `path_scope`/`resume_from`. This tool maps the role to an
/// `AgentDefinition`, creates a session, and runs it to completion.
///
/// Implements `TaskRunner` so the orchestration crate can use it for
/// `ForkJoinExecutor` and `TaskDispatcher` parallel execution.
pub struct DelegationExecutor {
    /// LLM provider for the sub-agent.
    provider: Arc<dyn LLMProvider>,
    /// Model name for the sub-agent.
    model: String,
    /// Working directory for tool execution.
    cwd: PathBuf,
    /// Tool descriptions schema (cloned from parent for sub-agent).
    tools_schema: Vec<Value>,
    /// Delegation planner — gates simple tasks to inline, spawns complex ones.
    planner: DelegationPlanner,
    /// Planner configuration retained so runtime clones can share the same gate values.
    planner_config: DelegationConfig,
}

impl DelegationExecutor {
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
            planner_config: DelegationConfig::default(),
            planner: DelegationPlanner::new(DelegationConfig::default()),
        }
    }

    /// Create a runtime clone that can be used as a `TaskRunner`.
    fn runtime_clone(&self) -> Self {
        Self {
            provider: Arc::clone(&self.provider),
            model: self.model.clone(),
            cwd: self.cwd.clone(),
            tools_schema: self.tools_schema.clone(),
            planner_config: self.planner_config.clone(),
            planner: DelegationPlanner::new(self.planner_config.clone()),
        }
    }

    /// Build a `TaskDispatcher` backed by a fresh runtime clone.
    fn runtime_dispatcher(&self) -> TaskDispatcher {
        let runner: Arc<dyn TaskRunner> = Arc::new(self.runtime_clone());
        TaskDispatcher::with_runner(runner, rustycode_orchestration::bus::BusHandle::new(32))
    }

    async fn execute_delegated_task_inner(
        &self,
        def: &AgentDefinition,
        prompt: &str,
        cwd: &std::path::Path,
        task_brief: Option<TaskBrief>,
    ) -> Result<(AgentResult, usize)> {
        let mut config = AgentConfig::from_env();
        config.max_turns = 25;
        config.timeout_secs = 900;
        config.max_tool_result_bytes = 8_000;
        config.temperature = 0.2;
        config.max_output_tokens = 32_768;
        config.thinking_nudge = true;

        let mut session = AgentSession::new(config, cwd);

        // Apply delegated-agent contract: promote tier, set activation scope,
        // attach task brief — mirrors run_agent_session() in task_dispatcher.
        let allowed_tools_filter: Option<Vec<String>> =
            task_brief.as_ref().map(|b| b.allowed_tools.clone());

        if let Some(brief) = task_brief {
            session.activation.set_scope(brief.allowed_tools.clone());
            session.activation.promote(ToolTier::Full);
            session = session.with_task_brief(brief);
        }

        // Build tool registry for sub-agent (subset that doesn't recurse).
        let tool_registry = self.build_subagent_tool_registry();

        // Initial user message.
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(prompt.to_string()),
        }];

        let tools_schema: Vec<Value> = self
            .tools_schema
            .iter()
            .filter(|schema| {
                allowed_tools_filter.as_ref().is_none_or(|allowed| {
                    schema
                        .get("name")
                        .and_then(|n| n.as_str())
                        .is_none_or(|name| allowed.contains(&name.to_string()))
                })
            })
            .cloned()
            .collect();

        let mut collector = DelegationCollector::default();

        let result = session
            .run(
                &*self.provider,
                &self.model,
                def.system_prompt,
                messages,
                &tools_schema,
                &tool_registry,
                &mut collector,
            )
            .await?;

        Ok((result, collector.tool_calls()))
    }

    /// Build a tool registry for the sub-agent.
    ///
    /// Excludes `AgentTool` and `DelegationExecutor` to prevent recursive spawning.
    fn build_subagent_tool_registry(&self) -> rustycode_tools::ToolRegistry {
        use rustycode_tools::apply_patch::ApplyPatchTool;
        use rustycode_tools::edit::EditFile;
        use rustycode_tools::glob::GlobTool;
        use rustycode_tools::grep::GrepTool;
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
        registry.register(ApplyPatchTool);
        registry.register(BashTool);
        registry.register(GitStatusTool);
        registry.register(GitDiffTool);
        registry.register(GitLogTool);

        registry
    }
}

// TaskRunner implementation

impl TaskRunner for DelegationExecutor {
    fn run_task(
        &self,
        task_description: &str,
        role: TaskRole,
        path_scope: &[PathBuf],
        resume_from: Option<&str>,
    ) -> Result<TaskRunResult> {
        let start = std::time::Instant::now();

        // Map TaskRole to agent definition.
        let agent_type = task_role_to_agent_type(role);
        let def = definitions::find_agent(agent_type)
            .or_else(|| {
                tracing::warn!(
                    "DelegationExecutor: agent type '{agent_type}' not found for role {role:?}, falling back"
                );
                definitions::find_agent("general-purpose")
            })
            .ok_or_else(|| anyhow::anyhow!("No agent definition available for role {role:?}"))?;

        // Determine effective cwd from path_scope.
        let effective_cwd = path_scope
            .first()
            .cloned()
            .unwrap_or_else(|| self.cwd.clone());

        // Build enriched prompt once from the runner inputs.
        let prompt = enrich_task_prompt(task_description, path_scope, resume_from);

        // Build delegated-agent contract from role + path_scope.
        let task_brief = build_delegation_brief(role, path_scope, &prompt);

        tracing::info!(
            "DelegationExecutor::run_task: role={role:?} agent_type='{agent_type}' starting"
        );

        // Execute via AgentSession (async, need block_in_place).
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.execute_delegated_task_inner(
                def,
                &prompt,
                &effective_cwd,
                Some(task_brief),
            ))
        });

        let elapsed_ms = i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX);

        match result {
            Ok((agent_result, tool_calls)) => {
                let input_tokens =
                    usize::try_from(agent_result.total_input_tokens).unwrap_or(usize::MAX);
                let output_tokens =
                    usize::try_from(agent_result.total_output_tokens).unwrap_or(usize::MAX);
                let estimated_cost =
                    calculate_cost(&self.model, input_tokens, output_tokens).unwrap_or(0.0);

                tracing::info!(
                    "DelegationExecutor::run_task: role={role:?} completed in {:.1}s ({} chars, {} tool calls)",
                    start.elapsed().as_secs_f64(),
                    agent_result.final_text.len(),
                    tool_calls
                );
                Ok(TaskRunResult::success(
                    agent_result.final_text,
                    estimated_cost,
                    elapsed_ms,
                ))
            }
            Err(e) => {
                tracing::warn!(
                    "DelegationExecutor::run_task: role={role:?} failed after {:.1}s: {e}",
                    start.elapsed().as_secs_f64()
                );
                Ok(TaskRunResult::failure(e.to_string(), elapsed_ms))
            }
        }
    }
}

// Tool trait implementation

impl Tool for DelegationExecutor {
    fn name(&self) -> &'static str {
        "DelegateTask"
    }

    fn description(&self) -> &'static str {
        "Spawn a delegated task with its own context. Use for research, exploration, code review, \
         or parallel implementation tasks that benefit from context isolation."
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        let roles = valid_role_strings();
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
                    "enum": roles,
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

        // 3. Extract and validate role (default: explore).
        let roles = valid_role_strings();
        let role_str = params
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("explore");

        if !roles.contains(&role_str) {
            anyhow::bail!(
                "invalid role '{}': must be one of {}",
                role_str,
                roles.join("|")
            );
        }
        let task_role = parse_task_role(role_str);

        // 4. Extract path_scope and resume_from.
        let path_scope: Vec<PathBuf> = params
            .get("path_scope")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(PathBuf::from))
                    .collect()
            })
            .unwrap_or_default();

        let resume_from = params
            .get("resume_from")
            .and_then(Value::as_str)
            .map(String::from);

        // 5. Consult the delegation planner (three-gate model).
        let planner_cwd = path_scope
            .first()
            .cloned()
            .unwrap_or_else(|| self.cwd.clone());
        let mut context = DelegationContext::for_tool_call(&planner_cwd, "tui-parent");
        if !path_scope.is_empty() {
            context.affected_paths = path_scope.clone();
        }

        let dispatcher = self.runtime_dispatcher();

        let results_to_text = |results: Vec<TaskResult>| {
            results
                .into_iter()
                .enumerate()
                .map(|(i, r)| {
                    let status = if r.success { "success" } else { "failed" };
                    format!("## Task {} ({status})\n{}", i + 1, r.output)
                })
                .collect::<Vec<_>>()
                .join("\n\n---\n\n")
        };

        match self.planner.should_spawn(task_description, &context) {
            SpawnDecision::Inline => Ok(ToolOutput::text(
                "This task is simple enough to handle directly in the current context. \
                 Delegation is not necessary — consider using read_file, grep, or other \
                 tools inline instead of spawning a sub-agent.",
            )),
            SpawnDecision::Spawn(spec) => {
                let mut spec = spec;
                spec.role = task_role;
                if spec.path_scope.is_empty() && !path_scope.is_empty() {
                    spec.path_scope = path_scope.clone();
                }
                if spec.resume_from.is_none() {
                    spec.resume_from = resume_from.clone();
                }

                let results = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { dispatcher.dispatch(SpawnDecision::Spawn(spec)).await })
                });

                Ok(ToolOutput::text(results_to_text(results)))
            }
            SpawnDecision::SpawnParallel(specs) => {
                let mut specs = specs;
                for spec in &mut specs {
                    spec.role = task_role;
                    if spec.path_scope.is_empty() && !path_scope.is_empty() {
                        spec.path_scope = path_scope.clone();
                    }
                    if spec.resume_from.is_none() {
                        spec.resume_from = resume_from.clone();
                    }
                }

                let results = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        dispatcher
                            .dispatch(SpawnDecision::SpawnParallel(specs))
                            .await
                    })
                });

                Ok(ToolOutput::text(results_to_text(results)))
            }
            SpawnDecision::Ensemble(plan) => {
                let results = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        dispatcher.dispatch(SpawnDecision::Ensemble(plan)).await
                    })
                });

                Ok(ToolOutput::text(results_to_text(results)))
            }
        }
    }
}

// Event collector

/// Simple event collector for delegated sub-agent runs.
#[derive(Default)]
struct DelegationCollector {
    tool_calls: usize,
}

impl DelegationCollector {
    fn tool_calls(&self) -> usize {
        self.tool_calls
    }
}

#[async_trait::async_trait]
impl AgentEvents for DelegationCollector {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ToolExecStarted { .. } => {
                self.tool_calls += 1;
            }
            StreamEvent::Done => {}
            _ => {}
        }
    }

    async fn on_done(&mut self, _result: &AgentResult) {}
}

// Tests

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
        assert_eq!(tool.name(), "DelegateTask");
        assert!(tool.description().contains("delegat"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn delegation_executor_schema_has_required_fields() {
        let tool = make_executor();
        let schema = tool.parameters_schema();

        let required = schema["required"]
            .as_array()
            .expect("required should be array");
        assert!(required
            .iter()
            .any(|v| v.as_str() == Some("task_description")));

        let props = schema["properties"]
            .as_object()
            .expect("properties should be object");
        assert!(props.contains_key("task_description"));
        assert!(props.contains_key("role"));

        // Verify role enum.
        let role_enum = props["role"]["enum"]
            .as_array()
            .expect("role should have enum");
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

        for role in valid_role_strings() {
            let params = serde_json::json!({
                "task_description": format!("Test task for {role}"),
                "role": role
            });

            // Verify role validation passes:
            let role_val = params
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("explore");
            assert!(
                valid_role_strings().contains(&role_val),
                "role '{role}' should be valid"
            );
        }
    }

    #[test]
    fn task_role_to_agent_type_mapping() {
        assert_eq!(
            task_role_to_agent_type(parse_task_role("explore")),
            "explore"
        );
        assert_eq!(
            task_role_to_agent_type(parse_task_role("research")),
            "explore"
        );
        assert_eq!(
            task_role_to_agent_type(parse_task_role("code")),
            "general-purpose"
        );
        assert_eq!(
            task_role_to_agent_type(parse_task_role("review")),
            "code-reviewer"
        );
        assert_eq!(
            task_role_to_agent_type(parse_task_role("verify")),
            "code-reviewer"
        );
        assert_eq!(task_role_to_agent_type(parse_task_role("plan")), "plan");
        assert_eq!(
            task_role_to_agent_type(parse_task_role("debug")),
            "build-error-resolver"
        );
        // Unknown role falls back to explore.
        assert_eq!(
            task_role_to_agent_type(parse_task_role("unknown")),
            "explore"
        );
    }

    #[test]
    fn parse_task_role_known_roles() {
        assert_eq!(parse_task_role("explore"), TaskRole::Explore);
        assert_eq!(parse_task_role("research"), TaskRole::Research);
        assert_eq!(parse_task_role("code"), TaskRole::Code);
        assert_eq!(parse_task_role("review"), TaskRole::Review);
        assert_eq!(parse_task_role("verify"), TaskRole::Verify);
        assert_eq!(parse_task_role("plan"), TaskRole::Plan);
        assert_eq!(parse_task_role("debug"), TaskRole::Debug);
    }

    #[test]
    fn parse_task_role_unknown_defaults_to_explore() {
        assert_eq!(parse_task_role("unknown"), TaskRole::Explore);
        assert_eq!(parse_task_role(""), TaskRole::Explore);
    }

    #[test]
    fn valid_role_strings_contains_all_roles() {
        let roles = valid_role_strings();
        assert_eq!(roles.len(), 7);
        assert!(roles.contains(&"explore"));
        assert!(roles.contains(&"research"));
        assert!(roles.contains(&"code"));
        assert!(roles.contains(&"review"));
        assert!(roles.contains(&"verify"));
        assert!(roles.contains(&"plan"));
        assert!(roles.contains(&"debug"));
    }

    #[test]
    fn delegation_executor_has_planner() {
        let exec = make_executor();
        let ctx = DelegationContext::for_tool_call(&exec.cwd, "test");
        let decision = exec.planner.should_spawn("fix a typo in readme", &ctx);
        assert!(
            matches!(decision, SpawnDecision::Inline),
            "expected Inline for simple task, got {decision:?}"
        );
    }

    // --- Cross-path parity tests ---
    // These verify that build_delegation_brief() produces the same contract
    // that task_dispatcher::task_spec_to_task_brief() would produce.

    #[test]
    fn brief_parity_all_roles_map_to_correct_agent_role() {
        use rustycode_orchestration::delegation::TaskRole;
        use rustycode_protocol::agent_protocol::AgentRole;
        use std::convert::TryFrom;

        let cases: Vec<(TaskRole, AgentRole)> = vec![
            (TaskRole::Explore, AgentRole::Researcher),
            (TaskRole::Research, AgentRole::Researcher),
            (TaskRole::Code, AgentRole::Builder),
            (TaskRole::Review, AgentRole::Reviewer),
            (TaskRole::Verify, AgentRole::Judge),
            (TaskRole::Plan, AgentRole::Planner),
            (TaskRole::Debug, AgentRole::Scalpel),
        ];

        for (task_role, expected_agent_role) in cases {
            let brief = build_delegation_brief(task_role, &[], "test prompt");
            assert_eq!(
                brief.role, expected_agent_role,
                "TaskRole::{task_role:?} should map to {expected_agent_role:?}"
            );

            let protocol_converted: AgentRole = task_role.try_into().unwrap();
            assert_eq!(
                brief.role, protocol_converted,
                "TUI path should match protocol TryFrom for {task_role:?}"
            );
        }
    }

    #[test]
    fn brief_parity_allowed_tools_match_role_definition() {
        for role_str in valid_role_strings() {
            let task_role = parse_task_role(role_str);
            let brief = build_delegation_brief(task_role, &[], "test");

            let expected: Vec<String> = task_role
                .allowed_tools()
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            assert_eq!(
                brief.allowed_tools, expected,
                "allowed_tools mismatch for role {role_str}"
            );
        }
    }

    #[test]
    fn brief_parity_path_scope_propagated() {
        let scope = vec![PathBuf::from("src/auth"), PathBuf::from("src/middleware")];
        let brief = build_delegation_brief(TaskRole::Code, &scope, "fix auth bug");

        assert_eq!(brief.path_scope, scope);
        assert_eq!(brief.brief, "fix auth bug");
    }

    #[test]
    fn brief_parity_deny_by_default() {
        let brief = build_delegation_brief(TaskRole::Explore, &[], "investigate");

        assert!(!brief.allowed_tools.contains(&"Bash".to_string()));
        assert!(!brief.allowed_tools.contains(&"Write".to_string()));
        assert!(!brief.allowed_tools.contains(&"Edit".to_string()));

        assert!(brief.allowed_tools.contains(&"Read".to_string()));
        assert!(brief.allowed_tools.contains(&"Grep".to_string()));
    }

    #[test]
    fn code_role_has_write_and_bash() {
        let brief = build_delegation_brief(TaskRole::Code, &[], "implement feature");

        assert!(brief.allowed_tools.contains(&"Write".to_string()));
        assert!(brief.allowed_tools.contains(&"Edit".to_string()));
        assert!(brief.allowed_tools.contains(&"Bash".to_string()));
        assert!(brief.allowed_tools.contains(&"Read".to_string()));
    }

    #[test]
    fn verify_role_has_bash_but_not_write() {
        let brief = build_delegation_brief(TaskRole::Verify, &[], "run tests");

        assert!(brief.allowed_tools.contains(&"Bash".to_string()));
        assert!(brief.allowed_tools.contains(&"Read".to_string()));
        assert!(!brief.allowed_tools.contains(&"Write".to_string()));
        assert!(!brief.allowed_tools.contains(&"Edit".to_string()));
    }

    #[test]
    fn scope_check_matches_brief_path_scope() {
        let scope = vec![PathBuf::from("crates/rustycode-tools")];
        let brief =
            build_delegation_brief(TaskRole::Code, &scope, "fix the bug in rustycode-tools");

        assert!(brief.is_in_scope(std::path::Path::new("crates/rustycode-tools/src/lib.rs")));
        assert!(brief.is_in_scope(std::path::Path::new("crates/rustycode-tools")));
        assert!(!brief.is_in_scope(std::path::Path::new("crates/rustycode-llm/src/lib.rs")));
    }

    // --- Tool gating integration tests ---

    #[test]
    fn tool_activation_deny_by_default_after_set_scope() {
        use rustycode_tools_api::tiers::{ToolActivationManager, ToolTier};

        let mut mgr = ToolActivationManager::new();

        let allowed: Vec<String> = TaskRole::Explore
            .allowed_tools()
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        mgr.set_scope(allowed);
        mgr.promote(ToolTier::Full);

        assert!(mgr.is_tool_allowed("Read"));
        assert!(mgr.is_tool_allowed("Grep"));
        assert!(!mgr.is_tool_allowed("Bash"));
        assert!(!mgr.is_tool_allowed("Write"));
        assert!(!mgr.is_tool_allowed("Edit"));
    }

    #[test]
    fn tool_activation_code_role_allows_write_and_bash() {
        use rustycode_tools_api::tiers::{ToolActivationManager, ToolTier};

        let mut mgr = ToolActivationManager::new();

        let allowed: Vec<String> = TaskRole::Code
            .allowed_tools()
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        mgr.set_scope(allowed);
        mgr.promote(ToolTier::Full);

        assert!(mgr.is_tool_allowed("Read"));
        assert!(mgr.is_tool_allowed("Write"));
        assert!(mgr.is_tool_allowed("Edit"));
        assert!(mgr.is_tool_allowed("Bash"));
        assert!(mgr.is_tool_allowed("Grep"));
    }

    #[test]
    fn tool_activation_no_scope_means_no_restriction() {
        use rustycode_tools_api::tiers::{ToolActivationManager, ToolTier};

        let mut mgr = ToolActivationManager::new();
        mgr.promote(ToolTier::Full);

        assert!(mgr.is_tool_allowed("Read"));
        assert!(mgr.is_tool_allowed("Bash"));
        assert!(mgr.is_tool_allowed("Write"));
        assert!(mgr.is_tool_allowed("AnyTool"));
    }

    #[test]
    fn tool_schema_filtering_matches_activation() {
        let all_tools: Vec<Value> = vec![
            serde_json::json!({"name": "Read", "description": "Read file", "parameters": {}}),
            serde_json::json!({"name": "Write", "description": "Write file", "parameters": {}}),
            serde_json::json!({"name": "Bash", "description": "Run command", "parameters": {}}),
            serde_json::json!({"name": "Grep", "description": "Search", "parameters": {}}),
        ];

        let allowed: Vec<String> = TaskRole::Explore
            .allowed_tools()
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let filtered: Vec<Value> = all_tools
            .iter()
            .filter(|schema| {
                schema
                    .get("name")
                    .and_then(|n| n.as_str())
                    .is_none_or(|name| allowed.contains(&name.to_string()))
            })
            .cloned()
            .collect();

        let filtered_names: Vec<&str> = filtered
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();

        assert!(filtered_names.contains(&"Read"));
        assert!(filtered_names.contains(&"Grep"));
        assert!(!filtered_names.contains(&"Write"));
        assert!(!filtered_names.contains(&"Bash"));
        assert_eq!(filtered.len(), 2);
    }
}
