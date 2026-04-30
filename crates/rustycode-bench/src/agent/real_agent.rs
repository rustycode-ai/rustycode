//! RealBenchAgent — thin wrapper delegating to AgentSession with real tools.
//!
//! Uses the same tool implementations as the TUI (ReadFileTool, EditFile, etc.)
//! instead of hand-rolled docker-exec wrappers. For native mode, tools operate
//! directly on the host workspace. For Docker mode, the environment must provide
//! a workspace mount so tools can access container files.

#![cfg(feature = "real-agent")]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_agent::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::{ChatMessage, MessageContent, MessageRole};
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_tools_api::tiers::ToolTier;
use rustycode_tools_api::ToolPermission;
use serde_json::{json, Value};

use crate::agent::BenchAgent;
use crate::agent::tools::build_bench_registry;
use crate::environment::BenchEnvironment;

// ---------------------------------------------------------------------------
// BenchObserver — collects metrics from the agent loop
// ---------------------------------------------------------------------------

struct BenchObserver {
    turns: usize,
    tool_calls: usize,
    errors: usize,
    final_text: String,
}

impl BenchObserver {
    fn new() -> Self {
        Self {
            turns: 0,
            tool_calls: 0,
            errors: 0,
            final_text: String::new(),
        }
    }
}

#[async_trait]
impl AgentEvents for BenchObserver {
    async fn on_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ToolCallStarted { .. } => {
                self.tool_calls += 1;
            }
            StreamEvent::ToolExecCompleted { is_error, .. } => {
                if is_error {
                    self.errors += 1;
                }
            }
            StreamEvent::Done => {
                self.turns += 1;
            }
            _ => {}
        }
    }

    async fn on_approval_needed(&mut self, _tool_name: &str, _input: &Value) -> ApprovalDecision {
        ApprovalDecision::AutoApproved
    }

    async fn on_done(&mut self, result: &AgentResult) {
        self.final_text = result.final_text.clone();
    }
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

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
- grep, glob, search_replace: Code search and replacement
- bash: Run commands (tests, installs, scripts)
- git_status, git_diff, git_log: Inspect repository state
- structured_thinking: Decompose complex tasks into steps

## Rules
- Run tests AFTER every implementation. Do not batch changes without verifying.
- Read error messages carefully. Fix the root cause, not the symptom.
- Python: use `python3`, install deps with `pip install -r requirements.txt`.
- Web: check package.json for scripts (`npm test`, `npm run build`).
- If no test script exists, write one and run it.
- Use edit_file for targeted changes. Use write_file only for new files.
- Check exit codes: 0 = success, non-zero = failure.";

// ---------------------------------------------------------------------------
// RealBenchAgent
// ---------------------------------------------------------------------------

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

        let config = AgentConfig {
            max_turns: self.max_turns,
            timeout_secs: self.timeout_secs,
            max_tool_result_bytes: 32_000,
            temperature: 0.2,
        };
        let mut session = AgentSession::new(config, cwd).with_tier(ToolTier::Full);

        // Build registry with real production tools (includes structured thinking via feature gate)
        let registry = build_bench_registry();

        let schemas: Vec<Value> = registry
            .list()
            .into_iter()
            .map(|info| {
                let mut schema = json!({
                    "name": info.name,
                    "description": info.description,
                    "input_schema": info.parameters_schema,
                });
                if let Some(annotations) = anthropic_annotations_for_tool_info(
                    &info.name,
                    matches!(info.permission, ToolPermission::Read),
                ) {
                    schema["annotations"] = annotations;
                }
                schema
            })
            .collect();

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
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create a RealBenchAgent from model string.
pub fn real_agent_factory(
    _name: &str,
    model: &str,
    _solution_dir: PathBuf,
) -> Result<Box<dyn BenchAgent>> {
    let (provider, model_name) = crate::config::resolve_provider_model(model)?;
    let llm_provider = create_provider(&provider, &model_name)?;
    Ok(Box::new(RealBenchAgent {
        provider: llm_provider,
        model: model_name,
        max_turns: 30,
        timeout_secs: 900,
    }) as Box<dyn BenchAgent>)
}

fn create_provider(provider: &str, model: &str) -> Result<Arc<dyn rustycode_llm::LLMProvider>> {
    match provider {
        "anthropic" | "claude" => {
            let api_key =
                std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::AnthropicProvider::new(config, model.to_string())?;
            Ok(Arc::new(p) as Arc<dyn rustycode_llm::LLMProvider>)
        }
        "openai" | "gpt" => {
            let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: std::env::var("OPENAI_BASE_URL").ok(),
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::OpenAiProvider::new(config, model.to_string())?;
            Ok(Arc::new(p) as Arc<dyn rustycode_llm::LLMProvider>)
        }
        "gemini" => {
            let api_key = std::env::var("GOOGLE_API_KEY").context("GOOGLE_API_KEY not set")?;
            let config = rustycode_llm::ProviderConfig {
                api_key: Some(secrecy::SecretString::new(api_key.into())),
                base_url: None,
                timeout_seconds: Some(120),
                extra_headers: None,
                retry_config: None,
            };
            let p = rustycode_llm::GeminiProvider::new(config)?;
            Ok(Arc::new(p) as Arc<dyn rustycode_llm::LLMProvider>)
        }
        other => {
            anyhow::bail!("Unsupported provider: '{other}'. Supported: anthropic, openai, gemini")
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        assert!(names.contains(&"read_file"), "missing read_file");
        assert!(names.contains(&"write_file"), "missing write_file");
        assert!(names.contains(&"edit_file"), "missing edit_file");
        assert!(names.contains(&"list_dir"), "missing list_dir");
        // Search tools
        assert!(names.contains(&"grep"), "missing grep");
        assert!(names.contains(&"glob"), "missing glob");
        assert!(names.contains(&"search_replace"), "missing search_replace");
        // Bash
        assert!(names.contains(&"bash"), "missing bash");
        // Git (read-only)
        assert!(names.contains(&"git_status"), "missing git_status");
        assert!(names.contains(&"git_diff"), "missing git_diff");
        assert!(names.contains(&"git_log"), "missing git_log");
        // No interactive tools
        assert!(!names.contains(&"question"), "question should not be registered");
        assert!(!names.contains(&"ask_user"), "ask_user should not be registered");
        // No git commit
        assert!(!names.contains(&"git_commit"), "git_commit should not be registered");
    }

    #[test]
    fn observer_counts_events() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut obs = BenchObserver::new();
            obs.on_event(StreamEvent::ToolCallStarted {
                id: "c1".into(),
                name: "bash".into(),
            })
            .await;
            obs.on_event(StreamEvent::ToolExecCompleted {
                id: "c1".into(),
                name: "bash".into(),
                output: "file.txt".into(),
                is_error: false,
            })
            .await;
            obs.on_event(StreamEvent::ToolCallStarted {
                id: "c2".into(),
                name: "bash".into(),
            })
            .await;
            obs.on_event(StreamEvent::ToolExecCompleted {
                id: "c2".into(),
                name: "bash".into(),
                output: "not found".into(),
                is_error: true,
            })
            .await;
            obs.on_event(StreamEvent::Done).await;
            assert_eq!(obs.tool_calls, 2);
            assert_eq!(obs.errors, 1);
            assert_eq!(obs.turns, 1);
        });
    }

    #[test]
    fn resolve_provider_model_works() {
        let (p, m) = crate::config::resolve_provider_model("claude-sonnet-4-6").unwrap();
        assert_eq!(p, "anthropic");
        assert_eq!(m, "claude-sonnet-4-6");
    }
}
