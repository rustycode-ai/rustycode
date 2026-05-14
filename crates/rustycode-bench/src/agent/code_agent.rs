//! Code agent — thin wrapper around AgentSession, same as TuiBenchAgent.
//!
//! Uses the same system prompt and config source as the TUI so benchmarks
//! measure the actual path users experience. The only difference: optional
//! symbol tools and thinking guide for complex tasks.

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use rustycode_agent_runtime::{AgentConfig, AgentSession};
use rustycode_tools_api::build_canonical_tool_schemas;
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;

use super::observer::{create_bench_provider, user_message, BenchObserver};
use super::tui_agent::build_tui_system_prompt;
use super::BenchAgent;
use crate::agent::tools::{build_bench_registry, build_bench_registry_with_symbol_tools};
use crate::environment::BenchEnvironment;

/// Configuration for the code agent.
#[derive(Debug, Clone)]
pub struct CodeAgentConfig {
    /// Model to use (e.g. "claude-sonnet-4-6", "gpt-4o").
    pub model: String,
    /// LLM provider name: "anthropic", "openai", etc.
    pub provider: String,
    /// Maximum number of tool-use turns.
    pub max_turns: usize,
    /// Maximum tokens for LLM response.
    pub max_tokens: u32,
    /// Enable tree-sitter symbol tools (find_symbol, ts_query, outline_file, code_context).
    pub with_symbol_tools: bool,
    /// Enable thinking_guide workflow tool (set false for A/B baseline).
    pub with_thinking_guide: bool,
}

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            provider: "anthropic".to_string(),
            max_turns: 30,
            max_tokens: 8192,
            with_symbol_tools: false,
            with_thinking_guide: true,
        }
    }
}

/// Agent that uses an LLM to solve benchmark tasks with tool access.
///
/// Mirrors TuiBenchAgent — same system prompt, same config source.
/// Differentiator: optional symbol tools and thinking guide.
pub struct CodeAgent {
    config: CodeAgentConfig,
    provider: Arc<dyn rustycode_llm::LLMProvider>,
    input_tokens: u64,
    output_tokens: u64,
}

impl CodeAgent {
    #[must_use]
    pub fn new(config: CodeAgentConfig, provider: Arc<dyn rustycode_llm::LLMProvider>) -> Self {
        Self {
            config,
            provider,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    /// Create auto-detected from the config's provider field.
    pub fn auto(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let provider = create_bench_provider(&config.provider, &config.model)?;
        Ok(Self::new(config, provider))
    }
}

#[async_trait]
impl BenchAgent for CodeAgent {
    fn name(&self) -> &'static str {
        "code"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> anyhow::Result<()> {
        Ok(())
    }

    async fn run(
        &mut self,
        instruction: &str,
        env: &mut dyn BenchEnvironment,
    ) -> anyhow::Result<()> {
        self.input_tokens = 0;
        self.output_tokens = 0;

        let cwd = env
            .workspace_path()
            .context("workspace_path required for CodeAgent — use native runner")?;

        // Configure thinking guide with turn budget (only if enabled)
        if self.config.with_thinking_guide {
            super::thinking_guide::configure(self.config.max_turns as u32);
        }

        let system_prompt = build_tui_system_prompt(&cwd);
        let messages = user_message(instruction);

        // Build tool registry — optionally with symbol tools and thinking guide
        let registry = if self.config.with_symbol_tools {
            build_bench_registry_with_symbol_tools(self.config.with_thinking_guide)
        } else {
            build_bench_registry(self.config.with_thinking_guide)
        };
        let tools_list = registry.list();
        let schemas: Vec<Value> = build_canonical_tool_schemas(&tools_list);

        // Mirror TUI: use from_env() for config
        let agent_config = AgentConfig::from_env();
        let mut session = AgentSession::new(agent_config, &cwd).with_tier(ToolTier::Full);
        let mut observer = BenchObserver::new();

        let handle = tokio::runtime::Handle::current();
        let agent_result = tokio::task::block_in_place(|| {
            handle.block_on(session.run(
                &*self.provider,
                &self.config.model,
                &system_prompt,
                messages,
                &schemas,
                &registry,
                &mut observer,
            ))
        })?;

        self.input_tokens = agent_result.total_input_tokens;
        self.output_tokens = agent_result.total_output_tokens;

        tracing::info!(
            turns = observer.turns,
            tool_calls = observer.tool_calls,
            errors = observer.errors,
            input_tokens = self.input_tokens,
            output_tokens = self.output_tokens,
            ttft_ms = ?observer.time_to_first_text_ms(),
            elapsed_ms = observer.elapsed_ms(),
            stopped = ?agent_result.stopped_reason,
            "CodeAgent completed"
        );

        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        (self.input_tokens, self.output_tokens)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tool_schemas_has_core_tools() {
        let registry = build_bench_registry(true);
        let tools_list = registry.list();
        let schemas = build_canonical_tool_schemas(&tools_list);
        let names: Vec<&str> = schemas
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();
        assert!(names.contains(&"Read"), "missing Read");
        assert!(names.contains(&"Write"), "missing Write");
        assert!(names.contains(&"Edit"), "missing Edit");
        assert!(names.contains(&"ListDir"), "missing ListDir");
        assert!(names.contains(&"Grep"), "missing Grep");
        assert!(names.contains(&"Glob"), "missing Glob");
        assert!(names.contains(&"ApplyPatch"), "missing ApplyPatch");
        assert!(names.contains(&"Bash"), "missing Bash");
        assert!(names.contains(&"GitStatus"), "missing GitStatus");
        assert!(names.contains(&"GitDiff"), "missing GitDiff");
        assert!(names.contains(&"GitLog"), "missing GitLog");
        // Thinking guide is registered
        assert!(names.contains(&"thinking_guide"), "missing thinking_guide");
        // No interactive tools
        assert!(!names.contains(&"question"));
        assert!(!names.contains(&"ask_user"));
    }

    #[test]
    fn symbol_tools_registry_adds_extra_tools() {
        let base = build_bench_registry(true);
        let with_sym = build_bench_registry_with_symbol_tools(true);

        // Symbol tools registry should be a superset
        assert!(with_sym.list().len() > base.list().len());
    }

    #[test]
    fn build_tool_schemas_strips_metadata_and_simplifies_null_types() {
        let registry = build_bench_registry(true);
        let tools_list = registry.list();
        let schemas = build_canonical_tool_schemas(&tools_list);
        for schema in &schemas {
            let input = &schema["input_schema"];
            assert!(
                input.get("$schema").is_none(),
                "{} schema should not have $schema",
                schema["name"]
            );
            assert!(
                input.get("title").is_none(),
                "{} schema should not have title",
                schema["name"]
            );
            if let Some(props) = input.get("properties").and_then(|p| p.as_object()) {
                for (name, prop) in props {
                    if let Some(arr) = prop.get("type").and_then(|t| t.as_array()) {
                        assert!(
                            !arr.iter().any(|v| v.as_str() == Some("null")),
                            "{}.{name} should not have null in type array",
                            schema["name"]
                        );
                    }
                }
            }
        }
    }
}
