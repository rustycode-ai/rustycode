//! Functional Agent tool backed by `AgentSession`.

use anyhow::Result;
use rustycode_agent_runtime::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::{ChatMessage, LLMProvider, MessageRole};
use rustycode_protocol::stream_event::StreamEvent;
use rustycode_protocol::MessageContent;
use rustycode_tools::{Tool, ToolContext, ToolOutput, ToolPermission};
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use super::definitions::{self, AgentDefinition};

/// Tool for launching sub-agents that run real LLM↔tool loops.
///
/// The LLM reads the tool description (which includes `when_to_use` entries)
/// and calls this tool with `subagent_type` and `prompt`. The tool then
/// creates an `AgentSession` and runs it to completion.
pub struct AgentTool {
    /// LLM provider for the sub-agent.
    provider: Arc<dyn LLMProvider>,
    /// Model name for the sub-agent.
    model: String,
    /// Working directory for tool execution.
    cwd: PathBuf,
    /// Tool descriptions schema (cloned from parent for sub-agent).
    tools_schema: Vec<Value>,
    /// Static description (includes when_to_use entries).
    description: &'static str,
}

impl AgentTool {
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
            description: Box::leak(definitions::build_agent_tool_description().into_boxed_str()),
        }
    }

    /// Resolve the agent definition and run the sub-agent loop.
    fn run_subagent(&self, agent_type: &str, prompt: &str) -> Result<String> {
        let def = definitions::find_agent(agent_type).ok_or_else(|| {
            let available: Vec<&str> = definitions::built_in_agents()
                .iter()
                .map(|a| a.agent_type)
                .collect();
            anyhow::anyhow!(
                "Unknown agent type '{}'. Available: {}",
                agent_type,
                available.join(", ")
            )
        })?;

        tracing::info!("AgentTool: starting '{}' sub-agent", agent_type);

        let start = std::time::Instant::now();

        // Run the async AgentSession inside a blocking context.
        // `Tool::execute` is sync, but `AgentSession::run` is async.
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.execute_agent(def, prompt))
        });

        let elapsed = start.elapsed();
        match &result {
            Ok(output) => {
                tracing::info!(
                    "AgentTool: '{}' completed in {:.1}s ({} chars)",
                    agent_type,
                    elapsed.as_secs_f64(),
                    output.len()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "AgentTool: '{}' failed after {:.1}s: {}",
                    agent_type,
                    elapsed.as_secs_f64(),
                    e
                );
            }
        }

        result
    }

    /// Execute the agent loop.
    async fn execute_agent(&self, def: &AgentDefinition, prompt: &str) -> Result<String> {
        let mut config = AgentConfig::from_env();
        config.max_turns = 25;
        config.timeout_secs = 300;
        config.max_tool_result_bytes = 8_000;
        config.temperature = 0.2;

        let mut session = AgentSession::new(config, &self.cwd);
        session.activation.promote(ToolTier::Full);

        // Build tool registry for sub-agent (subset that doesn't recurse)
        let tool_registry = self.build_subagent_tool_registry();

        // Initial user message
        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(prompt.to_string()),
        }];

        // Collector that captures output
        let mut collector = SubAgentCollector::default();

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
    /// Excludes `AgentTool` itself to prevent recursive spawning.
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

/// Tool trait implementation.
impl Tool for AgentTool {
    fn name(&self) -> &'static str {
        "agent"
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Execute
    }

    fn parameters_schema(&self) -> Value {
        let agent_types: Vec<Value> = definitions::built_in_agents()
            .iter()
            .map(|a| Value::String(a.agent_type.to_string()))
            .collect();

        serde_json::json!({
            "type": "object",
            "required": ["subagent_type", "prompt"],
            "properties": {
                "subagent_type": {
                    "type": "string",
                    "enum": agent_types,
                    "description": "The type of agent to launch"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the subagent to perform. Be specific about what to do, which files to modify, and what the expected outcome is."
                },
                "description": {
                    "type": "string",
                    "description": "A short (3-5 word) description of the task"
                }
            }
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let agent_type = params
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: subagent_type"))?;

        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prompt"))?;

        if prompt.trim().is_empty() {
            anyhow::bail!("Parameter 'prompt' must not be empty");
        }

        match self.run_subagent(agent_type, prompt) {
            Ok(output) => Ok(ToolOutput::text(output)),
            Err(e) => Ok(ToolOutput::text(format!(
                "Sub-agent '{}' failed: {e}",
                agent_type
            ))),
        }
    }
}

/// Simple event collector for sub-agent runs.
#[derive(Default)]
struct SubAgentCollector {
    _tool_calls: usize,
}

#[async_trait::async_trait]
impl AgentEvents for SubAgentCollector {
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

    fn make_tool() -> AgentTool {
        let provider = Arc::new(MockProvider::from_text("mock result"));
        AgentTool::new(
            provider,
            "test-model".to_string(),
            PathBuf::from("/tmp"),
            vec![],
        )
    }

    #[test]
    fn agent_tool_metadata() {
        let tool = make_tool();
        assert_eq!(tool.name(), "agent");
        assert!(tool.description().contains("general-purpose"));
        assert_eq!(tool.permission(), ToolPermission::Execute);
    }

    #[test]
    fn agent_tool_schema_has_required_fields() {
        let tool = make_tool();
        let schema = tool.parameters_schema();

        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("subagent_type")));
        assert!(required.iter().any(|v| v.as_str() == Some("prompt")));

        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("subagent_type"));
        assert!(props.contains_key("prompt"));

        // Verify enum contains known types
        let enum_vals = props["subagent_type"]["enum"].as_array().unwrap();
        let types: Vec<&str> = enum_vals.iter().filter_map(|v| v.as_str()).collect();
        assert!(types.contains(&"general-purpose"));
        assert!(types.contains(&"explore"));
    }

    #[test]
    fn agent_tool_missing_subagent_type() {
        let tool = make_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(serde_json::json!({"prompt": "do something"}), &ctx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("subagent_type"));
    }

    #[test]
    fn agent_tool_empty_prompt() {
        let tool = make_tool();
        let ctx = ToolContext::new("/tmp");

        let result = tool.execute(
            serde_json::json!({"subagent_type": "explore", "prompt": "  "}),
            &ctx,
        );
        assert!(result.is_err());
    }
}
