//! RealBenchAgent — delegates to the real AgentSession loop.
//!
//! No heuristics. No nudges. The model drives behavior; we enforce hard limits.
//! Benchmark scores honestly reflect the real app's capabilities.

// This module requires the `real-agent` feature which is not enabled by default.
// The imports (rustycode_agent::AgentSession, rustycode_tools_api) are not
// available in the bench crate's dependency tree.

#![cfg(feature = "real-agent")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use rustycode_agent::{AgentConfig, AgentEvents, AgentResult, AgentSession};
use rustycode_llm::provider::{ChatMessage, MessageContent, MessageRole};
use rustycode_llm::tool_annotations::anthropic_annotations_for_tool_info;
use rustycode_protocol::stream_event::{ApprovalDecision, StreamEvent};
use rustycode_tools_api::{Tool, ToolContext, ToolOutput, ToolPermission};
use serde_json::{json, Value};

use crate::agent::BenchAgent;
use crate::environment::BenchEnvironment;

// ---------------------------------------------------------------------------
// Thread-local env bridge
// ---------------------------------------------------------------------------
// Tool::execute() is synchronous, but BenchEnvironment is async.
// We store the env pointer in thread-locals during AgentSession::run()
// and reconstruct it inside each tool's execute().

thread_local! {
    static ENV_DATA: RefCell<Option<usize>> = const { RefCell::new(None) };
    static ENV_VTABLE: RefCell<Option<usize>> = const { RefCell::new(None) };
}

/// Store `&mut dyn BenchEnvironment` as two `usize` values (fat pointer parts).
///
/// # Safety
/// Caller must ensure the environment outlives the agent run.
unsafe fn set_env(env: &mut dyn BenchEnvironment) {
    let raw: *mut dyn BenchEnvironment = env;
    let (data, vtable): (usize, usize) = std::mem::transmute(raw);
    ENV_DATA.with(|c| *c.borrow_mut() = Some(data));
    ENV_VTABLE.with(|c| *c.borrow_mut() = Some(vtable));
}

fn clear_env() {
    ENV_DATA.with(|c| *c.borrow_mut() = None);
    ENV_VTABLE.with(|c| *c.borrow_mut() = None);
}

/// Execute an async env operation from sync context.
///
/// # Safety
/// Must be called only while an env is set (between set_env/clear_env).
fn get_env_raw() -> Result<(usize, usize)> {
    let data = ENV_DATA.with(|c| *c.borrow()).context("no env data set")?;
    let vtable = ENV_VTABLE
        .with(|c| *c.borrow())
        .context("no env vtable set")?;
    Ok((data, vtable))
}

fn env_exec(command: &str, timeout_secs: Option<u64>) -> Result<crate::environment::ExecResult> {
    let raw = get_env_raw()?;

    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        // SAFETY: The env pointer was stored by set_env() and is valid for
        // the duration of AgentSession::run(). block_in_place keeps us on
        // the same thread, matching the thread-local storage.
        let ptr: *mut dyn BenchEnvironment = unsafe { std::mem::transmute(raw) };
        let env: &mut dyn BenchEnvironment = unsafe { &mut *ptr };
        match timeout_secs {
            Some(t) => handle.block_on(env.exec_with_timeout(command, t)),
            None => handle.block_on(env.exec(command)),
        }
    })
}

fn env_upload(src: &Path, dest: &str) -> Result<()> {
    let raw = get_env_raw()?;

    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        // SAFETY: same as env_exec
        let ptr: *mut dyn BenchEnvironment = unsafe { std::mem::transmute(raw) };
        let env: &mut dyn BenchEnvironment = unsafe { &mut *ptr };
        handle.block_on(env.upload_file(src, dest))
    })
}

fn env_download(src: &str, dest: &Path) -> Result<()> {
    let raw = get_env_raw()?;

    let handle = tokio::runtime::Handle::current();
    tokio::task::block_in_place(|| {
        let ptr: *mut dyn BenchEnvironment = unsafe { std::mem::transmute(raw) };
        let env: &mut dyn BenchEnvironment = unsafe { &mut *ptr };
        handle.block_on(env.download_file(src, dest))
    })
}

fn env_workspace_path() -> Option<PathBuf> {
    let raw = get_env_raw().ok()?;
    let (data, vtable) = raw;

    tokio::task::block_in_place(|| {
        let ptr: *mut dyn BenchEnvironment = unsafe { std::mem::transmute((data, vtable)) };
        let env: &dyn BenchEnvironment = unsafe { &*ptr };
        env.workspace_path()
    })
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

struct TlBash;

impl Tool for TlBash {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Execute a bash command in the workspace. Use for running scripts, installing packages, \
         listing files, and any shell operations."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Optional timeout in seconds (max 600)"
                }
            },
            "required": ["command"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let command = params["command"].as_str().context("missing 'command'")?;
        let timeout = params["timeout"].as_u64().map(|t| t.min(600));
        let result = env_exec(command, timeout)?;
        let mut text = result.stdout;
        if !result.stderr.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str("[stderr]\n");
            text.push_str(&result.stderr);
        }
        if result.exit_code != 0 {
            let label = match result.exit_code {
                -9 | 137 => "killed (timeout/OOM)",
                126 => "command not executable",
                127 => "command not found",
                _ => "",
            };
            if label.is_empty() {
                text.push_str(&format!("\n[exit code: {}]", result.exit_code));
            } else {
                text.push_str(&format!("\n[exit code: {} — {}]", result.exit_code, label));
            }
        }
        Ok(ToolOutput {
            text,
            structured: None,
        })
    }
}

struct TlReadFile;

impl Tool for TlReadFile {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read a file's contents. Supports offset and limit for reading portions of large files."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based, default: 1)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: all)"
                }
            },
            "required": ["path"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["path"]
            .as_str()
            .or_else(|| params["file_path"].as_str())
            .context("missing 'path'")?;
        let offset = params["offset"].as_u64().unwrap_or(0);
        let limit = params["limit"].as_u64();

        // Build command: use head/tail for offset/limit, or cat for full file
        let cmd = if offset > 0 || limit.is_some() {
            let start = if offset > 0 { offset } else { 1 };
            if let Some(lim) = limit {
                format!("sed -n '{start},{}p' '{}'", start + lim - 1, path)
            } else {
                format!("tail -n +{} '{}'", start, path)
            }
        } else {
            format!("cat '{}'", path)
        };
        let result = env_exec(&cmd, None)?;
        if result.exit_code != 0 {
            anyhow::bail!("Failed to read {}: {}", path, result.stderr);
        }
        let mut text = result.stdout;
        // Append line count info for large files
        let line_count = text.lines().count();
        if line_count > 100 {
            text = format!("({} lines)\n{}", line_count, text);
        }
        Ok(ToolOutput {
            text,
            structured: None,
        })
    }
}

struct TlWriteFile;

impl Tool for TlWriteFile {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write content to a file. Creates parent directories if needed."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to write to"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["path", "content"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["path"]
            .as_str()
            .or_else(|| params["file_path"].as_str())
            .context("missing 'path'")?;
        let content = params["content"].as_str().context("missing 'content'")?;

        // Use base64 to avoid shell escaping issues with special characters
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content);
        let cmd =
            format!("mkdir -p $(dirname '{path}') && echo '{encoded}' | base64 -d > '{path}'");
        let result = env_exec(&cmd, None)?;
        if result.exit_code != 0 {
            anyhow::bail!("Failed to write {}: {}", path, result.stderr);
        }
        Ok(ToolOutput {
            text: format!("Wrote {} bytes to {}", content.len(), path),
            structured: None,
        })
    }
}

struct TlEditFile;

impl Tool for TlEditFile {
    fn name(&self) -> &str {
        "edit_file"
    }
    fn description(&self) -> &str {
        "Replace a string in a file. The old_string must match exactly."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "old_string": {
                    "type": "string",
                    "description": "Exact text to find"
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["path"]
            .as_str()
            .or_else(|| params["file_path"].as_str())
            .context("missing 'path'")?;
        let old = params["old_string"]
            .as_str()
            .context("missing 'old_string'")?;
        let new = params["new_string"]
            .as_str()
            .context("missing 'new_string'")?;

        // Encode old/new as base64 to avoid all shell escaping issues
        let old_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, old);
        let new_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, new);
        let script = format!(
            "import base64,sys\n\
             p=sys.argv[1]\n\
             o=base64.b64decode('{old_b64}').decode()\n\
             n=base64.b64decode('{new_b64}').decode()\n\
             with open(p) as f: c=f.read()\n\
             if o not in c:\n\
             \\    print('ERROR: old_string not found',file=sys.stderr);sys.exit(1)\n\
             c=c.replace(o,n,1)\n\
             with open(p,'w') as f: f.write(c)\n\
             print('OK')\n"
        );
        let script_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, script);
        let cmd = format!("echo '{script_b64}' | base64 -d | python3 - '{path}'");
        let result = env_exec(&cmd, None)?;
        if result.exit_code != 0 {
            anyhow::bail!("Failed to edit {}: {}", path, result.stderr.trim());
        }
        Ok(ToolOutput {
            text: format!("Edited {}", path),
            structured: None,
        })
    }
}

struct TlGrep;

impl Tool for TlGrep {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search for patterns in files. Supports regex, case-insensitive mode, and context lines."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (supports regex)"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: current directory)"
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Case-insensitive search (default: false)"
                },
                "context": {
                    "type": "integer",
                    "description": "Number of context lines to show around matches (default: 0)"
                },
                "head_limit": {
                    "type": "integer",
                    "description": "Maximum number of matches to return (default: 100)"
                }
            },
            "required": ["pattern"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let pattern = params["pattern"].as_str().context("missing 'pattern'")?;
        let path = params["path"].as_str().unwrap_or(".");
        let ci = params["case_insensitive"].as_bool().unwrap_or(false);
        let ctx = params["context"].as_u64().unwrap_or(0);
        let head = params["head_limit"].as_u64().unwrap_or(100);
        let esc = pattern.replace('\'', "'\\''");

        let ci_flag = if ci { " -i" } else { "" };
        let ctx_flag = if ctx > 0 {
            format!(" -C {}", ctx)
        } else {
            String::new()
        };

        let cmd = format!(
            "rg --no-heading -n{ci_flag}{ctx_flag} '{esc}' '{path}' 2>/dev/null | head -{head} \
             || grep -rn{ci_flag}{ctx_flag} '{esc}' '{path}' 2>/dev/null | head -{head} \
             || true"
        );
        let result = env_exec(&cmd, None)?;
        Ok(ToolOutput {
            text: result.stdout,
            structured: None,
        })
    }
}

struct TlGlob;

impl Tool for TlGlob {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "Find files matching a pattern. Use for discovering project structure."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern (e.g. '**/*.py', 'src/**/*.rs')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory to search in"
                }
            },
            "required": ["pattern"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let pattern = params["pattern"].as_str().context("missing 'pattern'")?;
        let path = params["path"].as_str().unwrap_or(".");
        let cmd = format!(
            "find {} -name '{}' -type f 2>/dev/null | head -100",
            path,
            pattern.replace('\'', "'\\''")
        );
        let result = env_exec(&cmd, None)?;
        Ok(ToolOutput {
            text: result.stdout,
            structured: None,
        })
    }
}

struct TlListFiles;

impl Tool for TlListFiles {
    fn name(&self) -> &str {
        "list_files"
    }
    fn description(&self) -> &str {
        "List files and directories at a given path. Use for exploring project structure."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list (default: current directory)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively (default: false)"
                }
            }
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["path"].as_str().unwrap_or(".");
        let recursive = params["recursive"].as_bool().unwrap_or(false);
        let cmd = if recursive {
            format!("find '{}' -type f 2>/dev/null | head -200", path)
        } else {
            format!("ls -la '{}' 2>/dev/null", path)
        };
        let result = env_exec(&cmd, None)?;
        Ok(ToolOutput {
            text: result.stdout,
            structured: None,
        })
    }
}

struct TlAppendFile;

impl Tool for TlAppendFile {
    fn name(&self) -> &str {
        "append_file"
    }
    fn description(&self) -> &str {
        "Append content to the end of a file. Creates the file if it doesn't exist."
    }
    fn permission(&self) -> ToolPermission {
        ToolPermission::Network
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to append"
                }
            },
            "required": ["path", "content"]
        })
    }
    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["path"].as_str().context("missing 'path'")?;
        let content = params["content"].as_str().context("missing 'content'")?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content);
        let cmd =
            format!("mkdir -p $(dirname '{path}') && echo '{encoded}' | base64 -d >> '{path}'");
        let result = env_exec(&cmd, None)?;
        if result.exit_code != 0 {
            anyhow::bail!("Failed to append to {}: {}", path, result.stderr);
        }
        Ok(ToolOutput {
            text: format!("Appended {} bytes to {}", content.len(), path),
            structured: None,
        })
    }
}

// ---------------------------------------------------------------------------
// Build tool registry + schemas
// ---------------------------------------------------------------------------

fn build_tools() -> (rustycode_tools_api::ToolRegistry, Vec<Value>) {
    let mut reg = rustycode_tools_api::ToolRegistry::new();
    reg.register(TlBash);
    reg.register(TlReadFile);
    reg.register(TlWriteFile);
    reg.register(TlEditFile);
    reg.register(TlAppendFile);
    reg.register(TlGrep);
    reg.register(TlGlob);
    reg.register(TlListFiles);

    // Structured thinking tool for complex task decomposition
    reg.register(
        rustycode_orchestration::structured_thinking_tool_impl::StructuredThinkingTool::new(None),
    );

    let schemas: Vec<Value> = reg
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
                matches!(info.permission, rustycode_tools_api::ToolPermission::Read),
            ) {
                schema["annotations"] = annotations;
            }
            schema
        })
        .collect();

    (reg, schemas)
}

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
            .unwrap_or_else(|| PathBuf::from("/app"));

        let config = AgentConfig {
            max_turns: self.max_turns,
            timeout_secs: self.timeout_secs,
            max_tool_result_bytes: 32_000,
            temperature: 0.2,
        };
        let mut session = AgentSession::new(config, cwd);

        let (registry, schemas) = build_tools();
        let mut observer = BenchObserver::new();

        let messages = vec![ChatMessage {
            role: MessageRole::User,
            content: MessageContent::Simple(instruction.to_string()),
        }];

        // Store env in thread-local for tool execution
        // SAFETY: env is borrowed for the duration of the run call below.
        unsafe {
            set_env(env);
        }
        let result = {
            let handle = tokio::runtime::Handle::current();
            tokio::task::block_in_place(|| {
                handle.block_on(session.run(
                    &*self.provider,
                    &self.model,
                    REAL_SYSTEM_PROMPT,
                    messages,
                    &schemas,
                    &registry,
                    &mut observer,
                ))
            })
        };
        clear_env();

        let agent_result = result?;
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
        assert!(REAL_SYSTEM_PROMPT.len() < 1500);
        assert!(!REAL_SYSTEM_PROMPT.contains("CRITICAL"));
        assert!(!REAL_SYSTEM_PROMPT.contains("NEVER"));
    }

    #[test]
    fn build_tools_registers_eight_tools() {
        let (reg, schemas) = build_tools();
        let infos = reg.list();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"edit_file"));
        assert!(names.contains(&"append_file"));
        assert!(names.contains(&"grep"));
        assert!(names.contains(&"glob"));
        assert!(names.contains(&"list_files"));
        assert_eq!(schemas.len(), 8);
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

    #[test]
    fn env_clear_after_clear() {
        clear_env();
        ENV_DATA.with(|c| assert!(c.borrow().is_none()));
        ENV_VTABLE.with(|c| assert!(c.borrow().is_none()));
    }
}
