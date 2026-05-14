//! TUI-aligned bench agent — mirrors the TUI's AgentSession configuration.
//!
//! Uses the same system prompt, config source, and tool setup as the real TUI
//! so benchmarks measure the actual path users experience.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_agent_runtime::{AgentConfig, AgentSession};
use rustycode_tools_api::build_canonical_tool_schemas;
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;

use super::observer::{create_bench_provider, user_message, BenchObserver};
use crate::agent::tools::build_bench_registry;
use crate::agent::BenchAgent;
use crate::environment::BenchEnvironment;

// ── System prompt (mirrors TUI's response.rs) ────────────────────────

pub fn build_tui_system_prompt(cwd: &Path) -> String {
    let mut parts = vec![
        // ── Identity ──
        "You are RustyCode, an AI coding assistant.\n\
         \n\
         Output complete working code. No placeholders, no TODOs, no explanations of what you would do."
            .to_string(),

        // ── Core Rules ──
        "## Core Rules\n\
         - Read files before modifying them\n\
         - Make targeted changes, not broad refactors\n\
         - Run tests to verify your changes\n\
         - Use parallel tool calls when operations are independent\n\
         - After making changes, always verify (build/test/lint) before declaring success\n\
         - If repeating the same failed approach, switch strategy rather than retrying"
            .to_string(),

        format!(
            "## Environment\nPlatform: {} | Date: {}",
            std::env::consts::OS,
            chrono::Utc::now().format("%Y-%m-%d")
        ),

        // ── Thinking Workflow ──
        "## Thinking Workflow\n\
         Call `thinking_guide` BEFORE each action to track your reasoning phase.\n\
         \n\
         ### Simple tasks (single bug fix, small change)\n\
         Flow: explore → locate → plan → execute → done\n\
         - When confidence >= 85 and past explore, stop reading and start editing.\n\
         \n\
         ### Complex tasks (multi-file, refactor, architectural)\n\
         On your FIRST call, set `scope` to \"complex\" or `phase` to \"scope\".\n\
         Flow: scope → decompose → expand → revise → execute → verify → done\n\
         \n\
         **decompose**: Write a numbered plan with specific items:\n\
         \"1. Fix auth validation in src/auth.rs\\n2. Update middleware in src/mw.rs\\n3. Add tests\"\n\
         \n\
         **expand**: For each item, describe the concrete approach before doing it.\n\
         \n\
         **revise**: After learning from reading code, update your plan. Drop items that aren't needed.\n\
         \n\
         During execute, follow your plan item by item. Check: does the next item still make sense?\n\
         \n\
         ### Warnings and nudges\n\
         - If thinking_guide returns a 'warning', correct course immediately.\n\
         - If thinking_guide returns a 'nudge', follow its advice.\n\
         - If thinking_guide returns 'escalation', narrow scope to the most critical sub-task.\n\
         - Respect the turn budget — fewer turns remaining means act faster."
            .to_string(),
    ];

    // Read project-local files (mirrors TUI behavior)
    if let Some(cwd_str) = cwd.to_str() {
        let project_prompt = Path::new(cwd_str).join(".rustycode_system_prompt");
        if project_prompt.exists() {
            if let Ok(content) = std::fs::read_to_string(&project_prompt) {
                if !content.trim().is_empty() {
                    parts.push(content);
                }
            }
        }

        let agents_md = Path::new(cwd_str).join("AGENTS.md");
        if agents_md.exists() {
            if let Ok(content) = std::fs::read_to_string(&agents_md) {
                if !content.trim().is_empty() {
                    parts.push(format!("## Project Instructions (AGENTS.md)\n{content}"));
                }
            }
        }
    }

    if let Ok(custom) = std::env::var("RUSTYCODE_SYSTEM_PROMPT") {
        if !custom.is_empty() {
            parts.push(custom);
        }
    }

    parts.join("\n\n")
}

// ── TuiBenchAgent ────────────────────────────────────────────────────

pub struct TuiBenchAgent {
    provider: Arc<dyn rustycode_llm::LLMProvider>,
    model: String,
    input_tokens: u64,
    output_tokens: u64,
}

impl TuiBenchAgent {
    pub fn new(provider: Arc<dyn rustycode_llm::LLMProvider>, model: String) -> Self {
        Self {
            provider,
            model,
            input_tokens: 0,
            output_tokens: 0,
        }
    }
}

#[async_trait]
impl BenchAgent for TuiBenchAgent {
    fn name(&self) -> &'static str {
        "tui"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> Result<()> {
        Ok(())
    }

    async fn run(&mut self, instruction: &str, env: &mut dyn BenchEnvironment) -> Result<()> {
        self.input_tokens = 0;
        self.output_tokens = 0;

        let cwd = env
            .workspace_path()
            .context("workspace_path required for TuiBenchAgent — use native runner")?;

        // Mirror TUI: use from_env() for config
        let agent_config = AgentConfig::from_env();
        super::thinking_guide::configure(agent_config.max_turns as u32);
        let system_prompt = build_tui_system_prompt(&cwd);
        let messages = user_message(instruction);

        let registry = build_bench_registry(true);
        let tools_list = registry.list();
        let schemas: Vec<Value> = build_canonical_tool_schemas(&tools_list);

        let mut session = AgentSession::new(agent_config, &cwd).with_tier(ToolTier::Full);
        let mut observer = BenchObserver::new();

        let handle = tokio::runtime::Handle::current();
        let agent_result = tokio::task::block_in_place(|| {
            handle.block_on(session.run(
                &*self.provider,
                &self.model,
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
            "TuiBenchAgent completed"
        );

        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        (self.input_tokens, self.output_tokens)
    }
}

// ── Factory ──────────────────────────────────────────────────────────

/// Create a TuiBenchAgent from model string.
pub fn tui_agent_factory(
    _name: &str,
    model: &str,
    _solution_dir: std::path::PathBuf,
) -> Result<Box<dyn BenchAgent>> {
    let (provider, model_name) = crate::config::resolve_provider_model(model)?;
    let llm_provider = create_bench_provider(&provider, &model_name)?;
    Ok(Box::new(TuiBenchAgent::new(llm_provider, model_name)) as Box<dyn BenchAgent>)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_includes_platform() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = build_tui_system_prompt(tmp.path());
        assert!(prompt.contains("Platform:"));
        assert!(prompt.contains("RustyCode"));
        assert!(prompt.contains("## Core Rules"));
        assert!(prompt.contains("## Thinking Workflow"));
    }

    #[test]
    fn system_prompt_reads_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("AGENTS.md"), "Use Rust 2021 edition.").unwrap();
        let prompt = build_tui_system_prompt(tmp.path());
        assert!(prompt.contains("Project Instructions (AGENTS.md)"));
        assert!(prompt.contains("Use Rust 2021 edition."));
    }

    #[test]
    fn system_prompt_reads_rustycode_system_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".rustycode_system_prompt"),
            "Always use unsafe.",
        )
        .unwrap();
        let prompt = build_tui_system_prompt(tmp.path());
        assert!(prompt.contains("Always use unsafe."));
    }
}
