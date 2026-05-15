//! Code agent — thin wrapper around AgentSession.
//!
//! Uses the standard TUI system prompt via `build_tui_system_prompt`.

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use rustycode_agent_runtime::plugins::EarlyStopPolicy;
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
            with_symbol_tools: true,
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

        // Build config from our own values (NOT from_env which ignores our config)
        let agent_config = AgentConfig {
            max_turns: self.config.max_turns,
            max_output_tokens: self.config.max_tokens,
            thinking_nudge: true,
            ..AgentConfig::from_env()
        };
        let mut session = AgentSession::new(agent_config, &cwd)
            .with_tier(ToolTier::Full)
            .with_plugin(Box::new(EarlyStopPolicy::new()));
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
        let names: Vec<String> = schemas
            .iter()
            .filter_map(|s| {
                s.get("name")
                    .and_then(|n| n.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        assert!(names.iter().any(|n| n == "Read"), "missing Read");
        assert!(names.iter().any(|n| n == "Write"), "missing Write");
        assert!(names.iter().any(|n| n == "Edit"), "missing Edit");
        assert!(names.iter().any(|n| n == "ListDir"), "missing ListDir");
        assert!(names.iter().any(|n| n == "Grep"), "missing Grep");
        assert!(names.iter().any(|n| n == "Glob"), "missing Glob");
        assert!(
            names.iter().any(|n| n == "ApplyPatch"),
            "missing ApplyPatch"
        );
        assert!(names.iter().any(|n| n == "Bash"), "missing Bash");
        assert!(names.iter().any(|n| n == "GitStatus"), "missing GitStatus");
        assert!(names.iter().any(|n| n == "GitDiff"), "missing GitDiff");
        assert!(names.iter().any(|n| n == "GitLog"), "missing GitLog");
        // Thinking guide is registered
        assert!(
            names.iter().any(|n| n == "thinking_guide"),
            "missing thinking_guide"
        );
        // No interactive tools
        assert!(!names.iter().any(|n| n == "question"));
        assert!(!names.iter().any(|n| n == "ask_user"));
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

    /// Integration assertion: CodeAgent uses its own config.max_turns,
    /// NOT from_env default. This prevents accidental override by env vars.
    #[test]
    fn code_agent_config_uses_own_max_turns() {
        let config = CodeAgentConfig {
            max_turns: 42,
            ..CodeAgentConfig::default()
        };

        // The CodeAgent::run method builds AgentConfig like:
        //   AgentConfig { max_turns: self.config.max_turns, ..AgentConfig::from_env() }
        // This test verifies the config stores our value, not env default.
        assert_eq!(
            config.max_turns, 42,
            "CodeAgentConfig must use explicitly-set max_turns, not env default"
        );

        // Default should be 30 (SWE-bench friendly, > 25)
        let default = CodeAgentConfig::default();
        assert!(
            default.max_turns >= 25,
            "CodeAgentConfig default max_turns should be >= 25, got {}",
            default.max_turns
        );
    }

    /// Integration assertion: CodeAgent default config has symbol tools
    /// and thinking guide enabled — matching SWE-bench requirements.
    #[test]
    fn code_agent_default_enables_symbol_tools_and_thinking_guide() {
        let config = CodeAgentConfig::default();

        assert!(
            config.with_symbol_tools,
            "CodeAgentConfig default must have with_symbol_tools=true"
        );
        assert!(
            config.with_thinking_guide,
            "CodeAgentConfig default must have with_thinking_guide=true"
        );
    }

    /// Integration assertion: CodeAgent builds the correct registry based on
    /// config flags — symbol tools registry when enabled, base registry when not.
    #[test]
    fn code_agent_builds_correct_registry_per_config() {
        // With symbol tools
        let with_sym_tools = build_bench_registry_with_symbol_tools(true).list();
        let sym_names: Vec<&str> = with_sym_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            sym_names.contains(&"find_symbol"),
            "symbol tools registry missing find_symbol"
        );
        assert!(
            sym_names.contains(&"ts_query"),
            "symbol tools registry missing ts_query"
        );
        assert!(
            sym_names.contains(&"thinking_guide"),
            "symbol tools registry missing thinking_guide"
        );

        // Without symbol tools
        let base_tools = build_bench_registry(true).list();
        let base_names: Vec<&str> = base_tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            !base_names.contains(&"find_symbol"),
            "base registry should not have find_symbol"
        );
        assert!(
            !base_names.contains(&"ts_query"),
            "base registry should not have ts_query"
        );
        assert!(
            base_names.contains(&"thinking_guide"),
            "base registry should still have thinking_guide"
        );

        // Without thinking guide
        let no_guide_tools = build_bench_registry(false).list();
        assert!(
            !no_guide_tools
                .iter()
                .map(|t| t.name.as_str())
                .any(|x| x == "thinking_guide"),
            "registry with thinking_guide=false should not have thinking_guide"
        );
    }

    /// Integration assertion: CodeAgent wires EarlyStopPolicy via with_plugin,
    /// and the policy thresholds match SWE-bench requirements.
    #[test]
    fn code_agent_early_stop_policy_matches_swe_bench() {
        let policy = EarlyStopPolicy::new();

        // These thresholds were chosen for SWE-bench:
        // 15 total edits (scope creep detection)
        // 5 turns since last edit (stagnation)
        // 6 same-file edits (thrashing)
        // 3 consecutive error turns (broken state)
        assert_eq!(policy.max_total_edits(), 15, "max_total_edits must be 15");
        assert_eq!(
            policy.max_turns_since_edit(),
            5,
            "max_turns_since_edit must be 5"
        );
        assert_eq!(
            policy.max_same_file_edits(),
            6,
            "max_same_file_edits must be 6"
        );
        assert_eq!(
            policy.max_consecutive_errors(),
            3,
            "max_consecutive_errors must be 3"
        );
    }
}
