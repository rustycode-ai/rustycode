//! RealBenchAgent — thin wrapper delegating to `AgentSession` with real tool
//! implementations.
//!
//! Delegates to the shared `AgentSession` loop (same one the TUI uses) but
//! wires in the full production tool registry so the agent operates on real
//! files and shells. For Docker mode, the environment must provide a workspace
//! mount so tools can access container files.

#![cfg(feature = "real-agent")]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_agent_runtime::{AgentConfig, AgentSession};
use rustycode_llm::provider::{ChatMessage, MessageContent, MessageRole};
use rustycode_tools_api::build_canonical_tool_schemas;
use rustycode_tools_api::tiers::ToolTier;
use serde_json::Value;

use super::observer::{create_bench_provider, BenchObserver};
use crate::agent::tools::build_bench_registry;
use crate::agent::BenchAgent;
use crate::environment::BenchEnvironment;

// System prompt

const REAL_SYSTEM_PROMPT: &str = "\
You are an expert software engineer. Your job is to complete the given task correctly.

## Strategy
1. Understand: Read the task. Identify what must change and what the tests expect.
2. Explore: Read relevant files to understand existing code structure.
3. Plan: Decide which files to change and how.
4. Implement: Make changes using edit_file (small edits) or write_file (new/large files).
5. Verify: Run tests immediately after implementing. Use eval.py, test.sh, pytest, \
make test, or whatever test runner the project uses.
6. Fix: If tests fail, read the error, fix the issue, re-verify. Repeat until tests pass.

## Tools Available
- read_file, write_file, edit_file, list_dir: File operations
- grep, glob, apply_patch: Code search and patching
- bash: Run commands (tests, installs, scripts)
- git_status, git_diff, git_log: Inspect repository state
- thinking_guide: Track reasoning phase and get workflow guidance

## Rules
- Run tests AFTER every implementation. Do not batch changes without verifying.
- Read error messages carefully. Fix the root cause, not the symptom.
- Python: use `python3`, install deps with `pip install -r requirements.txt`.
- Web: check package.json for scripts (`npm test`, `npm run build`).
- If no test script exists, write one and run it.
- Use edit_file for targeted changes. Use write_file only for new files.
- Check exit codes: 0 = success, non-zero = failure.";

// RealBenchAgent

pub struct RealBenchAgent {
    provider: Arc<dyn rustycode_llm::LLMProvider>,
    model: String,
    max_turns: usize,
    timeout_secs: u64,
}

#[async_trait]
impl BenchAgent for RealBenchAgent {
    fn name(&self) -> &'static str {
        "real"
    }

    async fn setup(&mut self, _env: &mut dyn BenchEnvironment) -> Result<()> {
        Ok(())
    }

    async fn run(&mut self, instruction: &str, env: &mut dyn BenchEnvironment) -> Result<()> {
        let cwd = env
            .workspace_path()
            .context("workspace_path required for real-agent — use native runner or mount /app/")?;

        super::thinking_guide::configure(self.max_turns as u32);

        let config = AgentConfig {
            max_turns: self.max_turns,
            timeout_secs: self.timeout_secs,
            max_tool_result_bytes: 32_000,
            temperature: 0.2,
            effort: None,
            max_output_tokens: 32_768,
        };
        let mut session = AgentSession::new(config, cwd).with_tier(ToolTier::Full);

        // Build registry with real production tools (includes thinking_guide)
        let registry = build_bench_registry();
        let tools_list = registry.list();
        let schemas: Vec<Value> = build_canonical_tool_schemas(&tools_list);

        let mut observer = BenchObserver::new();

        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(instruction.to_string()),
        }];

        let handle = tokio::runtime::Handle::current();
        let agent_result = tokio::task::block_in_place(|| {
            handle.block_on(session.run(
                &*self.provider,
                &self.model,
                REAL_SYSTEM_PROMPT,
                messages,
                &schemas,
                &registry,
                &mut observer,
            ))
        })?;

        tracing::info!(
            turns = observer.turns,
            tool_calls = observer.tool_calls,
            errors = observer.errors,
            stopped = ?agent_result.stopped_reason,
            "RealBenchAgent completed"
        );

        Ok(())
    }

    fn token_usage(&self) -> (u64, u64) {
        // RealBenchAgent delegates to AgentSession which does not expose usage yet
        (0, 0)
    }
}

// Factory

/// Create a RealBenchAgent from model string.
pub fn real_agent_factory(
    _name: &str,
    model: &str,
    _solution_dir: PathBuf,
) -> Result<Box<dyn BenchAgent>> {
    let (provider, model_name) = crate::config::resolve_provider_model(model)?;
    let llm_provider = create_bench_provider(&provider, &model_name)?;
    Ok(Box::new(RealBenchAgent {
        provider: llm_provider,
        model: model_name,
        max_turns: 30,
        timeout_secs: 900,
    }) as Box<dyn BenchAgent>)
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_is_concise() {
        assert!(REAL_SYSTEM_PROMPT.len() < 2000);
        assert!(!REAL_SYSTEM_PROMPT.contains("CRITICAL"));
        assert!(!REAL_SYSTEM_PROMPT.contains("NEVER"));
    }

    #[test]
    fn bench_registry_has_core_tools() {
        let reg = build_bench_registry();
        let infos = reg.list();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        // File tools
        assert!(names.contains(&"Read"), "missing read_file");
        assert!(names.contains(&"Write"), "missing write_file");
        assert!(names.contains(&"Edit"), "missing edit_file");
        assert!(names.contains(&"ListDir"), "missing list_dir");
        // Search tools
        assert!(names.contains(&"Grep"), "missing grep");
        assert!(names.contains(&"Glob"), "missing glob");
        assert!(names.contains(&"ApplyPatch"), "missing apply_patch");
        // Bash
        assert!(names.contains(&"Bash"), "missing bash");
        // Git (read-only)
        assert!(names.contains(&"GitStatus"), "missing git_status");
        assert!(names.contains(&"GitDiff"), "missing git_diff");
        assert!(names.contains(&"GitLog"), "missing git_log");
        // No interactive tools
        assert!(
            !names.contains(&"question"),
            "question should not be registered"
        );
        assert!(
            !names.contains(&"ask_user"),
            "ask_user should not be registered"
        );
        // No git commit
        assert!(
            !names.contains(&"GitCommit"),
            "git_commit should not be registered"
        );
    }

    #[test]
    fn resolve_provider_model_works() {
        let (p, m) = crate::config::resolve_provider_model("claude-sonnet-4-6").unwrap();
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-sonnet-4-6");
    }
}
