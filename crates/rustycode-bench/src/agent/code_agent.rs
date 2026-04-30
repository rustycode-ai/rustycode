//! Code agent — uses an LLM to solve benchmark tasks with tool access.
//!
//! Minimal implementation: system prompt → LLM call → parse tool_uses →
//! execute → feed results back → repeat. No steering, nudges, or
//! auto-corrections. Measures raw model capability.

use std::sync::Arc;

use rustycode_llm::provider::{ContentBlock, MessageContent, MessageRole};
use rustycode_protocol::intent::classify_intent;

use super::BenchAgent;
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
    /// System prompt for the agent.
    pub system_prompt: String,
    /// Timeout for each command execution in seconds.
    pub command_timeout_secs: u64,
    /// Approximate max characters of conversation context before pruning.
    pub max_context_chars: usize,
}

const DEFAULT_SYSTEM_PROMPT: &str = "Solve the task. Use the tools provided.";

impl Default for CodeAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            provider: "anthropic".to_string(),
            max_turns: 30,
            max_tokens: 8192,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            command_timeout_secs: 300,
            max_context_chars: 200_000,
        }
    }
}

/// Agent that uses an LLM to solve benchmark tasks with tool access.
pub struct CodeAgent {
    config: CodeAgentConfig,
    provider: Arc<dyn rustycode_llm::LLMProvider>,
    /// Tracks recent bash commands for repetition detection.
    recent_commands: Vec<String>,
}

/// Number of recent bash commands to track for repetition detection.
const REPETITION_WINDOW: usize = 3;

impl CodeAgent {
    #[must_use]
    pub fn new(config: CodeAgentConfig, provider: Arc<dyn rustycode_llm::LLMProvider>) -> Self {
        Self {
            config,
            provider,
            recent_commands: Vec::new(),
        }
    }

    /// Create using the default Anthropic provider.
    pub fn with_anthropic(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        let provider_config = rustycode_llm::ProviderConfig {
            api_key: Some(secrecy::SecretString::new(api_key.into())),
            base_url: std::env::var("ANTHROPIC_BASE_URL").ok(),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let provider =
            rustycode_llm::AnthropicProvider::new(provider_config, config.model.clone())?;

        Ok(Self {
            config,
            provider: Arc::new(provider),
            recent_commands: Vec::new(),
        })
    }

    /// Create using the OpenAI provider.
    pub fn with_openai(config: CodeAgentConfig) -> anyhow::Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;

        let provider_config = rustycode_llm::ProviderConfig {
            api_key: Some(secrecy::SecretString::new(api_key.into())),
            base_url: std::env::var("OPENAI_BASE_URL").ok(),
            timeout_seconds: Some(120),
            extra_headers: None,
            retry_config: None,
        };

        let provider = rustycode_llm::OpenAiProvider::new(provider_config, config.model.clone())?;

        Ok(Self {
            config,
            provider: Arc::new(provider),
            recent_commands: Vec::new(),
        })
    }

    /// Create auto-detected from the config's provider field.
    pub fn auto(config: CodeAgentConfig) -> anyhow::Result<Self> {
        match config.provider.as_str() {
            "anthropic" | "claude" => Self::with_anthropic(config),
            "openai" | "gpt" => Self::with_openai(config),
            other => {
                anyhow::bail!("Unsupported provider: '{other}'. Supported: anthropic, openai")
            }
        }
    }

    // ── Tool schemas ──────────────────────────────────────────────────

    fn bash_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "bash",
            "description": "Execute a bash command. Use for running scripts, installing packages, listing files, \
                checking test results, and any shell operations. Commands run in the workspace directory.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Optional timeout in seconds (default: 300, max: 600)"
                    }
                },
                "required": ["command"]
            }
        })
    }

    fn read_file_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "read_file",
            "description": "Read the contents of a file. Use offset and limit for large files. \
                Always read a file before editing it to ensure exact string matching.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Starting line number (1-based). Omit to read from beginning."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of lines to read. Omit to read entire file."
                    }
                },
                "required": ["path"]
            }
        })
    }

    fn edit_file_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "edit_file",
            "description": "Make a surgical edit to an existing file by replacing exact text. \
                The old_string must match exactly (including whitespace and indentation). \
                Prefer this over write_file for existing files.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Exact text to find and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Text to replace it with"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false, replaces first only)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            }
        })
    }

    fn write_file_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "write_file",
            "description": "Write content to a file, creating it if it doesn't exist. \
                For existing files, prefer edit_file to make surgical changes.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content to write"
                    }
                },
                "required": ["path", "content"]
            }
        })
    }

    fn grep_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "grep",
            "description": "Search for a pattern in files. Returns matching lines with file paths.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex or literal string)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory or file to search in (default: workspace root)"
                    },
                    "include": {
                        "type": "string",
                        "description": "File glob pattern to include (e.g. '*.py', '*.rs')"
                    },
                    "ignore_case": {
                        "type": "boolean",
                        "description": "Case-insensitive search (default: false)"
                    }
                },
                "required": ["pattern"]
            }
        })
    }

    fn glob_tool_schema() -> serde_json::Value {
        serde_json::json!({
            "name": "glob",
            "description": "Find files matching a glob pattern.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern (e.g. '**/*.py', 'src/**/*.rs')"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (default: workspace root)"
                    }
                },
                "required": ["pattern"]
            }
        })
    }

    fn all_tool_schemas() -> Vec<serde_json::Value> {
        vec![
            Self::bash_tool_schema(),
            Self::read_file_tool_schema(),
            Self::edit_file_tool_schema(),
            Self::write_file_tool_schema(),
            Self::grep_tool_schema(),
            Self::glob_tool_schema(),
        ]
    }

    // ── Parsing ───────────────────────────────────────────────────────

    /// Parse tool_use blocks from an LLM response.
    fn parse_tool_uses(content: &str) -> Vec<ToolUse> {
        let mut tool_uses = Vec::new();

        // Format 1: ```tool ... ``` code fences
        if let Some(tools) = Self::extract_tool_fences(content) {
            for tool in tools {
                tool_uses.push(tool);
            }
            return tool_uses;
        }

        // Format 2/3/4: direct JSON array
        if let Ok(blocks) = serde_json::from_str::<Vec<serde_json::Value>>(content) {
            for (i, block) in blocks.iter().enumerate() {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if block_type == "tool_use"
                    || block_type == "tool_call"
                    || block_type == "function_call"
                    || block_type == "function"
                    || (block.get("name").is_some() && block_type != "text")
                {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&format!("tool_{i}"))
                        .to_string();
                    let (name, input) = if let Some(func) = block.get("function") {
                        let n = func
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = func
                            .get("arguments")
                            .and_then(|v| v.as_str())
                            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                            .unwrap_or(serde_json::json!({}));
                        (n, args)
                    } else {
                        let n = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let inp = block
                            .get("input")
                            .or_else(|| block.get("arguments"))
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        (n, inp)
                    };

                    let has_valid_command = input
                        .get("command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| !c.is_empty());
                    if matches!(
                        name.as_str(),
                        "write_file" | "read_file" | "edit_file" | "grep" | "glob"
                    ) || (name == "bash" && has_valid_command)
                        || (!name.is_empty() && has_valid_command)
                    {
                        tool_uses.push(ToolUse { id, name, input });
                    }
                }
            }
        }

        // Fallback: scan for individual JSON tool objects in ```json blocks.
        if tool_uses.is_empty() {
            let mut search_from = 0;
            while let Some(start) = content[search_from..].find("```json") {
                let abs_start = search_from + start;
                let json_start = abs_start + "```json".len();
                let json_start = if content.as_bytes().get(json_start) == Some(&b'\n') {
                    json_start + 1
                } else {
                    json_start
                };
                if let Some(end) = content[json_start..].find("```") {
                    let json_str = &content[json_start..json_start + end];
                    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
                        let name = obj
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if matches!(
                            name.as_str(),
                            "write_file"
                                | "read_file"
                                | "edit_file"
                                | "grep"
                                | "glob"
                                | "bash"
                                | "Bash"
                                | "Read"
                                | "Edit"
                                | "Write"
                        ) {
                            let id = obj
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&format!("json_{abs_start}"))
                                .to_string();
                            let input = obj
                                .get("input")
                                .or_else(|| obj.get("arguments"))
                                .cloned()
                                .unwrap_or(serde_json::json!({}));
                            tool_uses.push(ToolUse { id, name, input });
                        }
                    }
                    search_from = json_start + end + 3;
                } else {
                    break;
                }
            }
        }

        tool_uses
    }

    fn extract_tool_fences(content: &str) -> Option<Vec<ToolUse>> {
        let mut tools = Vec::new();
        let mut search_from = 0;
        while let Some(start) = content[search_from..].find("```tool") {
            let abs_start = search_from + start;
            let json_start = abs_start + "```tool".len();
            let json_start = if content.as_bytes().get(json_start) == Some(&b'\n') {
                json_start + 1
            } else {
                json_start
            };

            if let Some(end) = content[json_start..].find("```") {
                let json_str = &content[json_start..json_start + end];
                if let Ok(calls) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                    for (i, call) in calls.iter().enumerate() {
                        let id = call
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&format!("tool_{i}_{abs_start}"))
                            .to_string();
                        // Handle OpenAI function wrapper: {"function": {"name": ..., "arguments": ...}}
                        let (name, input) = if let Some(func) = call.get("function") {
                            let n = func
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args = func
                                .get("arguments")
                                .and_then(|v| v.as_str())
                                .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                                .unwrap_or(serde_json::json!({}));
                            (n, args)
                        } else {
                            let n = call
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let inp = call
                                .get("arguments")
                                .or_else(|| call.get("input"))
                                .cloned()
                                .unwrap_or(serde_json::json!({}));
                            (n, inp)
                        };

                        let has_valid_command = input
                            .get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| !c.is_empty());
                        if matches!(
                            name.as_str(),
                            "write_file" | "read_file" | "edit_file" | "grep" | "glob"
                        ) || (name == "bash" && has_valid_command)
                            || (!name.is_empty() && has_valid_command)
                        {
                            tools.push(ToolUse { id, name, input });
                        }
                    }
                }
                search_from = json_start + end + 3;
            } else {
                break;
            }
        }

        if tools.is_empty() {
            None
        } else {
            Some(tools)
        }
    }

    /// Extract text content from response, stripping tool fences.
    fn extract_text(content: &str) -> String {
        if let Ok(blocks) = serde_json::from_str::<Vec<serde_json::Value>>(content) {
            let text_parts: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text_parts.is_empty() {
                return text_parts.join("\n");
            }
        }

        let mut result = content.to_string();
        while let Some(start) = result.find("```tool") {
            let end = result[start + 7..].find("```").map(|e| start + 7 + e + 3);
            if let Some(end) = end {
                result = format!("{}{}", &result[..start], &result[end..]);
            } else {
                break;
            }
        }
        result.trim().to_string()
    }

    // ── Path normalization ───────────────────────────────────────────

    fn normalize_path(path: &str) -> String {
        let p = path.trim();
        if p.starts_with("/app/") {
            p.strip_prefix("/app/").unwrap_or(p).to_string()
        } else if p == "/app" {
            ".".to_string()
        } else {
            p.to_string()
        }
    }

    // ── Tool execution ────────────────────────────────────────────────

    /// Read-only tools: safe to execute concurrently since they have no side effects.
    fn is_readonly_tool(name: &str) -> bool {
        matches!(name, "read_file" | "grep" | "glob")
    }

    async fn execute_tool(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let normalized_name = match tool_use.name.as_str() {
            "Edit" | "edit" => "edit_file",
            "Read" | "read" => "read_file",
            "Write" | "Create" => "write_file",
            "Bash" | "Shell" | "shell" => "bash",
            "Grep" | "Search" => "grep",
            "Glob" | "Find" | "ListFiles" => "glob",
            other => other,
        };
        match normalized_name {
            "write_file" => self.exec_write_file(tool_use, env).await,
            "read_file" => self.exec_read_file(tool_use, env).await,
            "edit_file" => self.exec_edit_file(tool_use, env).await,
            "grep" => self.exec_grep(tool_use, env).await,
            "glob" => self.exec_glob(tool_use, env).await,
            "bash" => self.exec_bash(tool_use, env).await,
            _ => self.exec_bash(tool_use, env).await,
        }
    }

    async fn exec_bash(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let raw_command = tool_use
            .input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if raw_command.is_empty() {
            return "ERROR: bash requires 'command' parameter".to_string();
        }

        // Normalize /app/ paths to relative
        let mut command = raw_command.to_string();
        if command.starts_with("/app/") {
            command = format!("./{}", &command[5..]);
        } else if command.starts_with("/app ") {
            command = format!(". {}", &command[5..]);
        }
        for boundary in [" ", "|", ";", "&", "(", "`", "$"] {
            command = command.replace(&format!("{boundary}/app/"), &format!("{boundary}./"));
            command = command.replace(&format!("{boundary}/app "), &format!("{boundary}. "));
        }
        command = command.replace("\n/app/", "\n./");

        let timeout = tool_use
            .input
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.config.command_timeout_secs)
            .min(600);

        tracing::info!("[code] Executing: {}", truncate(&command, 100));

        let result = env.exec_with_timeout(&command, timeout).await;

        match result {
            Ok(r) => {
                let stdout = r.stdout.trim();
                let stderr = r.stderr.trim();
                let mut out = String::new();
                if !stdout.is_empty() {
                    out.push_str(stdout);
                }
                if !stderr.is_empty() {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str("STDERR: ");
                    out.push_str(stderr);
                }
                if out.is_empty() {
                    out = "(no output)".to_string();
                }

                out = strip_ansi(&out);

                if !r.success() {
                    out.push_str(&format!("\n[exit code: {}]", r.exit_code));
                }

                // Truncate very long outputs
                if out.len() > 6_000 {
                    let head: String = out.chars().take(4_500).collect();
                    let tail_start = out.len().saturating_sub(1_500);
                    let tail: String = out.chars().skip(tail_start).collect();
                    out = format!(
                        "{head}\n\n... [{} chars truncated] ...\n\n{tail}",
                        out.len() - 6_000
                    );
                }

                out
            }
            Err(e) => format!("ERROR: {e}"),
        }
    }

    async fn exec_write_file(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let raw_path = tool_use
            .input
            .get("path")
            .or_else(|| tool_use.input.get("file_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = Self::normalize_path(raw_path);
        let content = tool_use
            .input
            .get("content")
            .or_else(|| tool_use.input.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if path.is_empty() {
            return "ERROR: write_file requires 'path' parameter".to_string();
        }

        tracing::info!("[code] Writing file: {} ({} bytes)", path, content.len());

        // Create parent directory if needed
        if let Some(slash) = path.rfind('/') {
            let dir = &path[..slash];
            if !dir.is_empty() {
                let esc_dir = dir.replace('\'', "'\\''");
                if let Err(e) = env
                    .exec_with_timeout(&format!("mkdir -p '{esc_dir}'"), 5)
                    .await
                {
                    return format!("ERROR: Could not create directory {dir}: {e}");
                }
            }
        }

        let escaped_path = path.replace('\'', "'\\''");
        let encoded = base64_encode(content);

        // Chunked write for large files to avoid ARG_MAX
        let write_result = if encoded.len() > 1_000_000 {
            let chunk_size = 500_000;
            let mut chunks = Vec::new();
            let mut pos = 0;
            while pos < encoded.len() {
                let end = (pos + chunk_size).min(encoded.len());
                chunks.push(&encoded[pos..end]);
                pos = end;
            }
            let mut success = true;
            for (i, chunk) in chunks.iter().enumerate() {
                let redirect = if i == 0 { ">" } else { ">>" };
                let cmd = format!("echo '{chunk}' | base64 -d {redirect} '{escaped_path}'");
                match env.exec_with_timeout(&cmd, 30).await {
                    Ok(r) if r.success() => {}
                    _ => {
                        success = false;
                        break;
                    }
                }
            }
            if success {
                env.exec_with_timeout("true", 1).await
            } else {
                env.exec_with_timeout("false", 1).await
            }
        } else {
            let cmd = format!("echo '{encoded}' | base64 -d > '{escaped_path}'");
            env.exec_with_timeout(&cmd, 30).await
        };

        match write_result {
            Ok(r) if r.success() => {
                let verify = env
                    .exec_with_timeout(&format!("wc -l < '{escaped_path}'"), 5)
                    .await;
                let size_info = match verify {
                    Ok(v) if v.success() => format!(" ({} lines)", v.stdout.trim()),
                    _ => String::new(),
                };
                format!("Successfully wrote to {path}{size_info}")
            }
            Ok(r) => {
                let err = r.stderr.trim();
                format!("ERROR writing file: {err}")
            }
            Err(e) => format!("ERROR: {e}"),
        }
    }

    async fn exec_read_file(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let raw_path = tool_use
            .input
            .get("path")
            .or_else(|| tool_use.input.get("file_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = Self::normalize_path(raw_path);
        let escaped_path = path.replace('\'', "'\\''");

        if path.is_empty() {
            return "ERROR: read_file requires 'path' parameter".to_string();
        }

        let offset = tool_use
            .input
            .get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let limit = tool_use
            .input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let cmd = if offset > 0 || limit > 0 {
            let start = if offset > 0 { offset } else { 1 };
            if limit > 0 {
                let end = start + limit - 1;
                format!(
                    "awk 'NR>={start}&&NR<={end} {{printf \"%6d  %s\\n\", NR, $0}}' '{escaped_path}'"
                )
            } else {
                format!("awk 'NR>={start} {{printf \"%6d  %s\\n\", NR, $0}}' '{escaped_path}'")
            }
        } else {
            format!("cat -n '{escaped_path}'")
        };

        tracing::info!("[code] Reading file: {path}");

        match env.exec_with_timeout(&cmd, 15).await {
            Ok(r) if r.success() => {
                let out = r.stdout.trim().to_string();
                if out.is_empty() {
                    format!("(file is empty: {path})")
                } else if out.len() > 6_000 {
                    let head: String = out.chars().take(5_000).collect();
                    let tail_start = out.len().saturating_sub(1_000);
                    let tail: String = out.chars().skip(tail_start).collect();
                    format!(
                        "{head}\n\n... [truncated, use offset/limit to read more] ...\n\n{tail}"
                    )
                } else {
                    out
                }
            }
            Ok(r) => {
                let err = if !r.stderr.trim().is_empty() {
                    r.stderr.trim().to_string()
                } else {
                    "file not found or unreadable".to_string()
                };
                format!("ERROR reading {path}: {err}")
            }
            Err(e) => format!("ERROR: {e}"),
        }
    }

    async fn exec_edit_file(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let raw_path = tool_use
            .input
            .get("path")
            .or_else(|| tool_use.input.get("file_path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = Self::normalize_path(raw_path);
        let escaped_path = path.replace('\'', "'\\''");
        let old_string = tool_use
            .input
            .get("old_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let new_string = tool_use
            .input
            .get("new_string")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let replace_all = tool_use
            .input
            .get("replace_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if path.is_empty() || old_string.is_empty() {
            return "ERROR: edit_file requires 'path' and 'old_string' parameters".to_string();
        }

        tracing::info!(
            "[code] Editing file: {path} (replacing {} bytes with {} bytes)",
            old_string.len(),
            new_string.len()
        );

        let path_b64 = base64_encode(&path);
        let old_b64 = base64_encode(old_string);
        let new_b64 = base64_encode(new_string);

        // Count occurrences via python3
        let count_cmd = format!(
            "python3 -c \"\
import base64, sys; \
p = base64.b64decode('{path_b64}').decode(); \
old = base64.b64decode('{old_b64}').decode(); \
data = open(p).read(); \
count = data.count(old); \
sys.stdout.write(str(count)); \
\""
        );

        let mut count: usize = 0;
        let mut python_available = true;

        match env.exec_with_timeout(&count_cmd, 10).await {
            Ok(r) if r.success() => {
                count = r.stdout.trim().parse().unwrap_or(0);
            }
            _ => {
                python_available = false;
            }
        }

        // Try CRLF→LF normalization if not found
        if count == 0 && python_available {
            let norm_old = base64_encode(&old_string.replace("\r\n", "\n"));
            let norm_cmd = format!(
                "python3 -c \"\
import base64, sys; \
p = base64.b64decode('{path_b64}').decode(); \
old = base64.b64decode('{norm_old}').decode(); \
data = open(p).read().replace(chr(13)+chr(10), chr(10)); \
count = data.count(old); \
sys.stdout.write(str(count)); \
\""
            );
            if let Ok(r) = env.exec_with_timeout(&norm_cmd, 10).await {
                if r.success() {
                    let norm_count: usize = r.stdout.trim().parse().unwrap_or(0);
                    if norm_count > 0 {
                        let normalize_cmd = format!(
                            "python3 -c \"\
p = base64.b64decode('{path_b64}').decode(); \
data = open(p).read(); \
open(p, 'w').write(data.replace(chr(13)+chr(10), chr(10))); \
print('ok')\""
                        );
                        let _ = env.exec_with_timeout(&normalize_cmd, 5).await;
                        if let Ok(r2) = env.exec_with_timeout(&count_cmd, 10).await {
                            if r2.success() {
                                count = r2.stdout.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                }
            }
        }

        // Fallback to sed for simple single-line edits when python3 unavailable
        if !python_available {
            let exists = env
                .exec_with_timeout(&format!("test -f '{escaped_path}' && echo exists"), 5)
                .await
                .map(|r| r.stdout.trim().to_string())
                .unwrap_or_default();
            if exists != "exists" {
                return format!("ERROR: File not found: {path}");
            }
            let escaped_old = old_string.replace('\'', "'\\''").replace('/', "\\/");
            let escaped_new = new_string.replace('\'', "'\\''").replace('/', "\\/");
            if !escaped_old.contains('\n') && !escaped_new.contains('\n') {
                let sed_flag = if replace_all { "g" } else { "" };
                let sed_cmd =
                    format!("sed -i 's/{escaped_old}/{escaped_new}/{sed_flag}' '{escaped_path}'");
                return match env.exec_with_timeout(&sed_cmd, 10).await {
                    Ok(r) if r.success() => "Replaced with sed".to_string(),
                    _ => format!("ERROR: Could not edit {path} (python3 and sed both failed)"),
                };
            }
            return format!(
                "ERROR: python3 unavailable and sed cannot handle multi-line edits for {path}"
            );
        }

        // Fuzzy match: compare lines with whitespace stripped
        if count == 0 && python_available {
            let norm_old: String = old_string
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
            let norm_old_b64 = base64_encode(&norm_old);
            let new_norm: String = new_string
                .lines()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join("\n");
            let new_norm_b64 = base64_encode(&new_norm);
            let fuzzy_replace = format!(
                "python3 -c \"\
import base64, sys; \
p = base64.b64decode('{path_b64}').decode(); \
old = base64.b64decode('{norm_old_b64}').decode(); \
new = base64.b64decode('{new_norm_b64}').decode(); \
lines = open(p).read().split(chr(10)); \
old_t = old.split(chr(10)); \
for i in range(len(lines) - len(old_t) + 1): \
    chunk = [l.strip() for l in lines[i:i+len(old_t)]]; \
    if chr(10).join(chunk) == old: \
        lines[i:i+len(old_t)] = new.split(chr(10)); \
        open(p, 'w').write(chr(10).join(lines)); \
        print('Replaced (whitespace-normalized match at line ' + str(i+1) + ')'); \
        sys.exit(0); \
sys.exit(1)\
\""
            );
            if let Ok(r) = env.exec_with_timeout(&fuzzy_replace, 10).await {
                if r.success() {
                    return r.stdout.trim().to_string();
                }
            }
        }

        if count == 0 {
            return format!(
                "ERROR: old_string not found in {path}. \
                 The exact text must match including whitespace and indentation. \
                 Use read_file to see current contents."
            );
        }

        // Perform the replacement
        let replace_py = if count > 1 && !replace_all {
            format!(
                "python3 -c \"\
import base64; \
p = base64.b64decode('{path_b64}').decode(); \
old = base64.b64decode('{old_b64}').decode(); \
new = base64.b64decode('{new_b64}').decode(); \
data = open(p).read(); \
data = data.replace(old, new, 1); \
open(p, 'w').write(data); \
print('Replaced 1 of {count} occurrences')\
\""
            )
        } else {
            format!(
                "python3 -c \"\
import base64; \
p = base64.b64decode('{path_b64}').decode(); \
old = base64.b64decode('{old_b64}').decode(); \
new = base64.b64decode('{new_b64}').decode(); \
data = open(p).read(); \
actual = data.count(old); \
data = data.replace(old, new); \
open(p, 'w').write(data); \
print('Replaced ' + str(actual) + ' occurrence(s)')\
\""
            )
        };

        match env.exec_with_timeout(&replace_py, 10).await {
            Ok(r) if r.success() => r.stdout.trim().to_string(),
            Ok(r) => format!("ERROR: {}", r.stderr.trim()),
            Err(e) => format!("ERROR: {e}"),
        }
    }

    async fn exec_grep(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let pattern = tool_use
            .input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = tool_use
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let include = tool_use.input.get("include").and_then(|v| v.as_str());
        let ignore_case = tool_use
            .input
            .get("ignore_case")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if pattern.is_empty() {
            return "ERROR: grep requires 'pattern' parameter".to_string();
        }

        let mut cmd = String::from("grep -rn --binary-files=without-match");
        if ignore_case {
            cmd.push_str(" -i");
        }
        if let Some(inc) = include {
            cmd.push_str(&format!(" --include='{inc}'"));
        }
        let escaped_pattern = pattern.replace('\'', "'\\''");
        let escaped_path = path.replace('\'', "'\\''");
        let quoted_path = if path == "." {
            ".".to_string()
        } else {
            format!("'{escaped_path}'")
        };
        cmd.push_str(&format!(" '{escaped_pattern}' {quoted_path}"));

        tracing::info!("[code] Grepping: {cmd}");

        match env.exec_with_timeout(&cmd, 15).await {
            Ok(r) if r.success() => {
                let out = r.stdout.trim().to_string();
                if out.len() > 6_000 {
                    let head: String = out.chars().take(5_000).collect();
                    format!("{head}\n\n... [truncated, use more specific pattern or path]")
                } else {
                    out
                }
            }
            Ok(_) => "No matches found.".to_string(),
            Err(e) => format!("ERROR: {e}"),
        }
    }

    async fn exec_glob(&self, tool_use: &ToolUse, env: &dyn BenchEnvironment) -> String {
        let pattern = tool_use
            .input
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = tool_use
            .input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        if pattern.is_empty() {
            return "ERROR: glob requires 'pattern' parameter".to_string();
        }

        let escaped_pattern = pattern.replace('\'', "'\\''");
        let escaped_path = path.replace('\'', "'\\''");
        let quoted_path = if path == "." {
            ".".to_string()
        } else {
            format!("'{escaped_path}'")
        };
        let cmd = if pattern.contains("**") {
            let clean_pattern = escaped_pattern.trim_start_matches("./");
            format!("find {quoted_path} -path '*/{clean_pattern}' -type f 2>/dev/null | head -50")
        } else {
            format!("find {quoted_path} -name '{escaped_pattern}' -type f 2>/dev/null | head -50")
        };

        tracing::info!("[code] Glob: {cmd}");

        match env.exec_with_timeout(&cmd, 10).await {
            Ok(r) => {
                let out = r.stdout.trim().to_string();
                if out.is_empty() {
                    "No files found matching pattern.".to_string()
                } else {
                    out
                }
            }
            Err(e) => format!("ERROR: {e}"),
        }
    }

    // ── Context management ────────────────────────────────────────────

    fn estimate_context_chars(messages: &[rustycode_llm::ChatMessage]) -> usize {
        messages
            .iter()
            .map(|m| match &m.content {
                MessageContent::Simple(s) => s.len(),
                MessageContent::Blocks(blocks) => blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text, .. } => text.len(),
                        ContentBlock::ToolUse { input, .. } => input.to_string().len(),
                        ContentBlock::ToolResult { content, .. } => content.len(),
                        _ => 0,
                    })
                    .sum(),
                _ => 0,
            })
            .sum()
    }

    /// Prune messages to stay within context budget.
    fn prune_messages(messages: &mut Vec<rustycode_llm::ChatMessage>, max_chars: usize) {
        let total = Self::estimate_context_chars(messages);
        if total <= max_chars {
            return;
        }

        let keep_tail = 8;
        if messages.len() <= keep_tail + 1 {
            return;
        }

        let task_recap = messages
            .first()
            .map(|m| {
                let text = match &m.content {
                    MessageContent::Simple(s) => s.clone(),
                    MessageContent::Blocks(blocks) => blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text, .. } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                truncate(&text, 500)
            })
            .unwrap_or_default();

        let remove_end = messages.len().saturating_sub(keep_tail);
        let removed_parts: Vec<String> = messages
            .drain(1..remove_end)
            .map(|m| match m.content {
                MessageContent::Simple(s) => s,
                MessageContent::Blocks(blocks) => blocks
                    .into_iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text, .. } => Some(text),
                        ContentBlock::ToolResult { content, .. } => Some(content),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            })
            .collect();
        let removed = removed_parts.join("\n");

        let summary = if removed.len() > 600 {
            let s: String = removed.chars().take(300).collect();
            let e: String = removed
                .chars()
                .rev()
                .take(300)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("{s}...\n...\n{e}")
        } else {
            removed
        };

        let recap_section = if task_recap.is_empty() {
            String::new()
        } else {
            format!("\n\n[ORIGINAL TASK (reminder)]:\n{task_recap}")
        };
        messages.insert(
            1,
            rustycode_llm::ChatMessage::user(format!(
                "[CONTEXT SUMMARY — earlier work was trimmed to save space]\n{summary}{recap_section}"
            )),
        );

        tracing::info!(
            "[code] Pruned context: {} chars → {} chars",
            total,
            Self::estimate_context_chars(messages)
        );
    }

    async fn write_trace(&self, trace: &str, env: &dyn BenchEnvironment) {
        if let Ok(r) = env.exec("pwd").await {
            if r.success() {
                let trace_path = format!("{}/conversation_trace.md", r.stdout.trim());
                let _ = tokio::fs::write(&trace_path, trace).await;
            }
        }
    }
}

// ── Free functions ────────────────────────────────────────────────────

fn base64_encode(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).map_or(0, |&b| u32::from(b));
        let b2 = chunk.get(2).map_or(0, |&b| u32::from(b));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(TABLE[((triple >> 18) & 0x3F) as usize]));
        out.push(char::from(TABLE[((triple >> 12) & 0x3F) as usize]));
        out.push(if chunk.len() > 1 {
            char::from(TABLE[((triple >> 6) & 0x3F) as usize])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(TABLE[(triple & 0x3F) as usize])
        } else {
            '='
        });
    }
    out
}

struct ToolUse {
    #[allow(dead_code)]
    id: String,
    name: String,
    input: serde_json::Value,
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && (bytes[i] >= 0x20 && bytes[i] <= 0x3f) {
                i += 1;
            }
            while i < bytes.len() && (bytes[i] >= 0x20 && bytes[i] <= 0x2f) {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                i += 1;
            }
        } else if bytes[i] == 0x1b {
            i += 2;
        } else {
            result.push(s.chars().nth(i).unwrap_or('\0'));
            i += 1;
        }
    }
    result
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}

// ── BenchAgent impl ──────────────────────────────────────────────────

#[async_trait::async_trait]
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
        let tools = Self::all_tool_schemas();

        let task_prompt = format!(
            "You have {} turns total. Use them wisely.\n\n{instruction}",
            self.config.max_turns
        );
        let mut messages = vec![rustycode_llm::ChatMessage::user(task_prompt)];

        // Classify intent to steer the conversation frame
        let intent = classify_intent(instruction);
        let system_prompt = format!(
            "Solve the task. Use the tools provided. Read workspace files first to understand \
             the task requirements and test expectations, then implement.\n\n{}",
            intent.prompt_suffix()
        );
        tracing::info!("[code] Intent: {:?} → frame applied", intent);

        let mut conversation_trace = format!(
            "# Conversation Trace\n\n## Instruction\n{instruction}\n## Intent: {:?}\n",
            intent
        );

        for turn in 0..self.config.max_turns {
            Self::prune_messages(&mut messages, self.config.max_context_chars);

            let request =
                rustycode_llm::CompletionRequest::new(&self.config.model, messages.clone())
                    .with_system_prompt(system_prompt.clone())
                    .with_max_tokens(self.config.max_tokens)
                    .with_temperature(0.2)
                    .with_tools(tools.clone())
                    .with_tool_choice(serde_json::json!("auto"));

            tracing::info!(
                "[code] Turn {}/{} (provider: {})",
                turn + 1,
                self.config.max_turns,
                self.provider.name()
            );

            let response = rustycode_llm::LLMProvider::complete(&*self.provider, request).await?;

            // Handle max_tokens truncation
            if response.stop_reason.as_deref() == Some("max_tokens")
                && turn < self.config.max_turns - 1
            {
                tracing::info!("[code] Model hit max_tokens, injecting continuation");
                let text = Self::extract_text(&response.content);
                if !text.is_empty() {
                    messages.push(rustycode_llm::ChatMessage {
                        role: MessageRole::Assistant,
                        content: MessageContent::Simple(text),
                    });
                }
                messages.push(rustycode_llm::ChatMessage::user("Continue.".to_string()));
                continue;
            }

            let text = Self::extract_text(&response.content);

            // Always log the full raw response content for debugging
            conversation_trace.push_str(&format!(
                "\n--- Turn {} — Raw Response ---\n{}\n",
                turn + 1,
                truncate(&response.content, 4000)
            ));

            if !text.is_empty() {
                tracing::info!("[code] LLM: {}", truncate(&text, 200));
            }

            let tool_uses = Self::parse_tool_uses(&response.content);
            if !tool_uses.is_empty() {
                conversation_trace.push_str(&format!(
                    "\n--- Turn {} — Tool Calls ({} total) ---",
                    turn + 1,
                    tool_uses.len()
                ));
                for (i, tu) in tool_uses.iter().enumerate() {
                    let input_preview = match tu.name.as_str() {
                        "bash" => tu
                            .input
                            .get("command")
                            .and_then(|v| v.as_str())
                            .map(|s| truncate(s, 500))
                            .unwrap_or_default()
                            .to_string(),
                        "write_file" => {
                            let p = tu
                                .input
                                .get("path")
                                .or(tu.input.get("file_path"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("path={p}")
                        }
                        "edit_file" => {
                            let p = tu
                                .input
                                .get("path")
                                .or(tu.input.get("file_path"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("path={p}")
                        }
                        "read_file" => {
                            let p = tu
                                .input
                                .get("path")
                                .or(tu.input.get("file_path"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            format!("path={p}")
                        }
                        _ => serde_json::to_string(&tu.input).unwrap_or_default(),
                    };
                    conversation_trace.push_str(&format!(
                        "\n  [{}] {} | {}",
                        i + 1,
                        tu.name,
                        input_preview
                    ));
                }
                conversation_trace.push('\n');
            }

            if tool_uses.is_empty() {
                tracing::info!("[code] No more tool calls — agent finished");
                break;
            }

            // Build assistant message with ContentBlocks
            let mut blocks: Vec<ContentBlock> = Vec::new();
            if !text.is_empty() {
                blocks.push(ContentBlock::text(text));
            }
            for tool_use in &tool_uses {
                blocks.push(ContentBlock::tool_use(
                    &tool_use.id,
                    &tool_use.name,
                    tool_use.input.clone(),
                ));
            }
            messages.push(rustycode_llm::ChatMessage {
                role: MessageRole::Assistant,
                content: MessageContent::Blocks(blocks),
            });

            // Execute tool calls in order. Consecutive read-only calls are
            // batched and run concurrently. Write calls act as barriers —
            // they flush any pending reads, execute, then the next batch starts.
            let mut raw_outputs: Vec<String> = Vec::with_capacity(tool_uses.len());
            let mut i = 0;
            while i < tool_uses.len() {
                // Collect a run of consecutive read-only calls.
                let read_start = i;
                while i < tool_uses.len() && Self::is_readonly_tool(&tool_uses[i].name) {
                    i += 1;
                }
                let read_end = i;

                if read_end > read_start {
                    // Execute consecutive read-only calls concurrently.
                    let batch_futures = tool_uses[read_start..read_end]
                        .iter()
                        .map(|t| self.execute_tool(t, env));
                    raw_outputs.extend(futures::future::join_all(batch_futures).await);
                }

                if i < tool_uses.len() && !Self::is_readonly_tool(&tool_uses[i].name) {
                    // Execute write call sequentially.
                    raw_outputs.push(self.execute_tool(&tool_uses[i], env).await);
                    i += 1;
                }
            }

            // Process results: repetition detection, truncation, error detection.
            let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
            for (i, mut output) in raw_outputs.into_iter().enumerate() {
                let tool_use = &tool_uses[i];

                // Repetition detection for bash commands.
                if tool_use.name == "bash" {
                    let normalized = tool_use
                        .input
                        .get("command")
                        .and_then(|v| v.as_str())
                        .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default();
                    if !normalized.is_empty() {
                        let is_repeat = self.recent_commands.iter().any(|c| c == &normalized);
                        if is_repeat {
                            output.push_str(
                                "\n\nNOTE: You already ran this exact command recently \
                                 with the same output. Consider a different approach.",
                            );
                            tracing::info!(
                                "[code] Repeated command detected: {}",
                                truncate(&normalized, 80)
                            );
                        }
                        self.recent_commands.push(normalized);
                        if self.recent_commands.len() > REPETITION_WINDOW {
                            self.recent_commands.remove(0);
                        }
                    }
                }

                conversation_trace.push_str(&format!(
                    "\n--- Turn {} — Tool Result ({}) ---\n{}\n",
                    turn + 1,
                    tool_use.name,
                    truncate(&output, 1000)
                ));

                const MAX_TOOL_RESULT_CHARS: usize = 4000;
                let context_output = if output.len() > MAX_TOOL_RESULT_CHARS {
                    let head: String = output.chars().take(3000).collect();
                    let tail_start = output.len().saturating_sub(1000);
                    let tail: String = output.chars().skip(tail_start).collect();
                    format!(
                        "{head}\n\n... [{} chars truncated] ...\n\n{tail}",
                        output.len() - MAX_TOOL_RESULT_CHARS
                    )
                } else {
                    output.clone()
                };

                let is_error = context_output.starts_with("Error ")
                    || context_output.starts_with("ERROR: ")
                    || context_output.starts_with("error: ")
                    || (context_output.contains("[exit code:")
                        && !context_output.contains("[exit code: 0]"))
                    || context_output.contains("command not found")
                    || context_output.contains("No such file or directory")
                    || context_output.contains("Permission denied");

                if is_error {
                    tool_result_blocks
                        .push(ContentBlock::tool_error(&tool_use.id, &context_output));
                } else {
                    tool_result_blocks
                        .push(ContentBlock::tool_result(&tool_use.id, &context_output));
                }
            }

            // All tool results in ONE user message with multiple content blocks.
            messages.push(rustycode_llm::ChatMessage {
                role: MessageRole::User,
                content: MessageContent::Blocks(tool_result_blocks),
            });

            // Write trace incrementally so it survives timeouts
            self.write_trace(&conversation_trace, env).await;
        }

        // Write conversation trace (final write)
        self.write_trace(&conversation_trace, env).await;

        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_fence_single_command() {
        let content = "I'll run a command.\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls -la\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "bash");
    }

    #[test]
    fn parse_tool_fence_multiple_commands() {
        let content = "```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"mkdir foo\"}}, {\"name\": \"bash\", \"arguments\": {\"command\": \"ls foo\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn parse_tool_fence_with_surrounding_text() {
        let content = "Let me check the files.\n\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"cat /app/regex.txt\"}}]\n```\n\nThat looks good.";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_direct_json_tool_use_blocks() {
        let content =
            r#"[{"type":"tool_use","id":"tu_1","name":"bash","input":{"command":"echo hello"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "tu_1");
    }

    #[test]
    fn parse_direct_json_with_arguments_key() {
        let content = r#"[{"name":"bash","arguments":{"command":"ls -la"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_empty_string_returns_nothing() {
        assert!(CodeAgent::parse_tool_uses("").is_empty());
        assert!(CodeAgent::parse_tool_uses("Just text, no tools.").is_empty());
    }

    #[test]
    fn parse_tool_fence_preserves_id_from_api() {
        let content = "```tool\n[{\"id\": \"call_abc123\", \"name\": \"bash\", \"arguments\": {\"command\": \"ls /app/\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].id, "call_abc123");
        assert_eq!(tools[0].name, "bash");
    }

    #[test]
    fn parse_tool_fence_generates_synthetic_id_when_missing() {
        let content = "```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert!(tools[0].id.starts_with("tool_"));
    }

    #[test]
    fn parse_mixed_text_and_tool_blocks() {
        let content = r#"[{"type":"text","text":"I'll run this."},{"type":"tool_use","id":"tu_0","name":"bash","input":{"command":"pwd"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_tool_fence_skips_empty_commands() {
        let content = "```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert!(tools.is_empty());
    }

    #[test]
    fn extract_text_from_plain_string() {
        let text = CodeAgent::extract_text("Hello, world!");
        assert_eq!(text, "Hello, world!");
    }

    #[test]
    fn extract_text_strips_tool_fences() {
        let content = "Let me check the files.\n```tool\n[{\"name\": \"bash\", \"arguments\": {\"command\": \"ls\"}}]\n```\nDone.";
        let text = CodeAgent::extract_text(content);
        assert!(!text.contains("```tool"));
        assert!(text.contains("Let me check"));
        assert!(text.contains("Done."));
    }

    #[test]
    fn extract_text_strips_multiple_fences() {
        let content = "Step 1\n```tool\n[{\"name\":\"bash\",\"arguments\":{\"command\":\"a\"}}]\n```\nStep 2\n```tool\n[{\"name\":\"bash\",\"arguments\":{\"command\":\"b\"}}]\n```\nDone";
        let text = CodeAgent::extract_text(content);
        assert!(!text.contains("```tool"));
        assert!(text.contains("Step 1"));
        assert!(text.contains("Step 2"));
    }

    #[test]
    fn extract_text_from_json_blocks() {
        let content =
            r#"[{"type":"text","text":"First part"},{"type":"text","text":"Second part"}]"#;
        let text = CodeAgent::extract_text(content);
        assert_eq!(text, "First part\nSecond part");
    }

    #[test]
    fn extract_tool_fences_returns_none_for_no_fences() {
        assert!(CodeAgent::extract_tool_fences("no fences here").is_none());
    }

    #[test]
    fn extract_tool_fences_ignores_non_tool_fences() {
        assert!(CodeAgent::extract_tool_fences("```bash\nls\n```").is_none());
    }

    #[test]
    fn parse_fence_with_input_key_raw_api_format() {
        let content =
            "```tool\n[{\"name\": \"bash\", \"input\": {\"command\": \"find . -name '*.rs'\"}}]\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn parse_write_file_tool() {
        let content = r#"[{"type":"tool_use","id":"wf_1","name":"write_file","input":{"path":"test.py","content":"print('hi')"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "write_file");
    }

    #[test]
    fn parse_read_file_tool() {
        let content =
            r#"[{"type":"tool_use","id":"rf_1","name":"read_file","input":{"path":"main.py"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "read_file");
    }

    #[test]
    fn parse_edit_file_tool() {
        let content = r#"[{"type":"tool_use","id":"ef_1","name":"edit_file","input":{"path":"a.py","old_string":"x=1","new_string":"x=2"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "edit_file");
    }

    #[test]
    fn parse_grep_tool() {
        let content =
            r#"[{"type":"tool_use","id":"gr_1","name":"grep","input":{"pattern":"TODO"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "grep");
    }

    #[test]
    fn parse_glob_tool() {
        let content =
            r#"[{"type":"tool_use","id":"gb_1","name":"glob","input":{"pattern":"**/*.py"}}]"#;
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "glob");
    }

    #[test]
    fn base64_encoding() {
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("f"), "Zg==");
        assert_eq!(base64_encode("fo"), "Zm8=");
        assert_eq!(base64_encode("foo"), "Zm9v");
        assert_eq!(base64_encode("Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
    }

    #[test]
    fn base64_encoding_special_chars() {
        let input = "line1\nline2\ttab's \"quote\"";
        let encoded = base64_encode(input);
        let decoded = String::from_utf8(base64_decode_for_test(&encoded)).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn strip_ansi_removes_color_codes() {
        assert_eq!(strip_ansi("\x1b[31mFAILED\x1b[0m"), "FAILED");
        assert_eq!(strip_ansi("\x1b[32mPASSED\x1b[0m test"), "PASSED test");
        assert_eq!(strip_ansi("no escape codes"), "no escape codes");
        assert_eq!(
            strip_ansi("\x1b[1;33mwarn\x1b[0m: \x1b[36mmsg\x1b[0m"),
            "warn: msg"
        );
        assert_eq!(strip_ansi("\x1b[2J\x1b[Hclear"), "clear");
    }

    #[test]
    fn strip_ansi_removes_cursor_and_color() {
        let with_ansi = "\x1B[31mRed text\x1B[0m and \x1B[2J\x1B[Hcursor stuff";
        let clean = strip_ansi(with_ansi);
        assert_eq!(clean, "Red text and cursor stuff");
    }

    #[test]
    fn truncate_multibyte_utf8_safe() {
        let s = "café résumé 数据";
        let truncated = truncate(s, 5);
        assert!(truncated.ends_with("..."));
        assert!(truncated.starts_with("café"));
    }

    #[test]
    fn prune_messages_noop_when_under_limit() {
        let msgs = vec![rustycode_llm::ChatMessage::user("Hello".to_string())];
        let mut msgs = msgs;
        CodeAgent::prune_messages(&mut msgs, 100_000);
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn normalize_path_strips_app_prefix() {
        assert_eq!(CodeAgent::normalize_path("/app/solution.py"), "solution.py");
        assert_eq!(CodeAgent::normalize_path("/app/src/main.py"), "src/main.py");
        assert_eq!(CodeAgent::normalize_path("/app"), ".");
        assert_eq!(CodeAgent::normalize_path("solution.py"), "solution.py");
        assert_eq!(CodeAgent::normalize_path("./solution.py"), "./solution.py");
        assert_eq!(CodeAgent::normalize_path("  /app/test.py  "), "test.py");
    }

    #[test]
    fn parse_json_fence_single_tool() {
        let content = "I'll run the tests.\n```json\n{\"name\": \"bash\", \"input\": {\"command\": \"pytest\"}}\n```\nLet me check.";
        let tools = CodeAgent::parse_tool_uses(content);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "bash");
    }

    #[test]
    fn parse_json_fence_no_tool_name_ignored() {
        let content = "```json\n{\"type\": \"text\", \"content\": \"hello\"}\n```";
        let tools = CodeAgent::parse_tool_uses(content);
        assert!(tools.is_empty());
    }

    fn base64_decode_for_test(input: &str) -> Vec<u8> {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut result = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let b0 = TABLE.iter().position(|&b| b == bytes[i]).unwrap_or(0) as u32;
            let b1 = if i + 1 < bytes.len() && bytes[i + 1] != b'=' {
                TABLE.iter().position(|&b| b == bytes[i + 1]).unwrap_or(0) as u32
            } else {
                0
            };
            let b2 = if i + 2 < bytes.len() && bytes[i + 2] != b'=' {
                TABLE.iter().position(|&b| b == bytes[i + 2]).unwrap_or(0) as u32
            } else {
                0
            };
            let b3 = if i + 3 < bytes.len() && bytes[i + 3] != b'=' {
                TABLE.iter().position(|&b| b == bytes[i + 3]).unwrap_or(0) as u32
            } else {
                0
            };
            let triple = (b0 << 18) | (b1 << 12) | (b2 << 6) | b3;
            result.push(((triple >> 16) & 0xFF) as u8);
            if i + 2 < bytes.len() && bytes[i + 2] != b'=' {
                result.push(((triple >> 8) & 0xFF) as u8);
            }
            if i + 3 < bytes.len() && bytes[i + 3] != b'=' {
                result.push((triple & 0xFF) as u8);
            }
            i += 4;
        }
        result
    }
}
