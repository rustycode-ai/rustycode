//! Tool-call format adapter for cross-harness compatibility.
//!
//! AST prompts are harness-agnostic, but actual tool-call formats differ between
//! execution environments. This module provides a thin translation layer that
//! normalizes tool names and arguments between harnesses.
//!
//! v0.4 experimental result (§17.5, Implication 4): AST prompt generates
//! `Write(...)` but `RustyCode` expects `agent_tool_write(...)`. The adapter
//! bridges this gap.

use serde_json::Value;

/// Identifies which execution harness is being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolHarness {
    /// Claude Code CLI — standard tool format (identity mapping).
    ClaudeCode,
    /// `RustyCode` agent — uses `agent_tool_*` naming.
    RustyCode,
    /// Gemini CLI — sandbox-based execution with limited tools.
    GeminiCli,
    /// `OpenAI` Codex — different tool registry format.
    Codex,
}

impl std::fmt::Display for ToolHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaudeCode => write!(f, "claude-code"),
            Self::RustyCode => write!(f, "rustycode"),
            Self::GeminiCli => write!(f, "gemini-cli"),
            Self::Codex => write!(f, "codex"),
        }
    }
}

/// Translates tool calls between AST prompt format and harness-specific format.
pub trait ToolAdapter: Send + Sync {
    fn normalize_tool_name(&self, tool: &str) -> String;
    fn normalize_args(&self, tool: &str, args: &Value) -> Value;
    fn format_hint(&self) -> &'static str;
    fn harness(&self) -> ToolHarness;
}

/// Identity adapter — Claude Code uses the same format AST generates.
pub struct ClaudeCodeAdapter;

impl ToolAdapter for ClaudeCodeAdapter {
    fn normalize_tool_name(&self, tool: &str) -> String {
        tool.to_string()
    }

    fn normalize_args(&self, _tool: &str, args: &Value) -> Value {
        args.clone()
    }

    fn format_hint(&self) -> &'static str {
        "Use standard tool format: Write(file_path, content), Edit(file_path, old_string, new_string), Bash(command), Read(file_path)"
    }

    fn harness(&self) -> ToolHarness {
        ToolHarness::ClaudeCode
    }
}

/// `RustyCode` adapter — translates to `agent_tool_*` naming convention.
///
/// - `Write({file_path, content})` → `agent_tool_write({path, data})`
/// - `Edit({file_path, old_string, new_string})` → `agent_tool_edit({path, old, new})`
/// - `Bash({command})` → `agent_tool_bash({cmd})`
/// - `Read({file_path})` → `agent_tool_read({path})`
pub struct RustyCodeAdapter;

impl RustyCodeAdapter {
    const TOOL_MAP: &[(&str, &str)] = &[
        ("Write", "agent_tool_write"),
        ("Edit", "agent_tool_edit"),
        ("Read", "agent_tool_read"),
        ("Bash", "agent_tool_bash"),
        ("Grep", "agent_tool_grep"),
        ("Glob", "agent_tool_glob"),
    ];

    fn map_tool(tool: &str) -> String {
        Self::TOOL_MAP
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(tool))
            .map_or_else(
                || format!("agent_tool_{}", tool.to_lowercase()),
                |(_, v)| (*v).to_string(),
            )
    }

    fn map_write_args(args: &Value) -> Value {
        let path = args
            .get("file_path")
            .or_else(|| args.get("path"))
            .cloned()
            .unwrap_or(Value::Null);
        let data = args
            .get("content")
            .or_else(|| args.get("data"))
            .cloned()
            .unwrap_or(Value::Null);
        serde_json::json!({ "path": path, "data": data })
    }

    fn map_edit_args(args: &Value) -> Value {
        let path = args
            .get("file_path")
            .or_else(|| args.get("path"))
            .cloned()
            .unwrap_or(Value::Null);
        let old = args
            .get("old_string")
            .or_else(|| args.get("old"))
            .cloned()
            .unwrap_or(Value::Null);
        let new = args
            .get("new_string")
            .or_else(|| args.get("new"))
            .cloned()
            .unwrap_or(Value::Null);
        serde_json::json!({ "path": path, "old": old, "new": new })
    }

    fn map_bash_args(args: &Value) -> Value {
        let cmd = args
            .get("command")
            .or_else(|| args.get("cmd"))
            .cloned()
            .unwrap_or(Value::Null);
        serde_json::json!({ "cmd": cmd })
    }

    fn map_read_args(args: &Value) -> Value {
        let path = args
            .get("file_path")
            .or_else(|| args.get("path"))
            .cloned()
            .unwrap_or(Value::Null);
        serde_json::json!({ "path": path })
    }
}

impl ToolAdapter for RustyCodeAdapter {
    fn normalize_tool_name(&self, tool: &str) -> String {
        Self::map_tool(tool)
    }

    fn normalize_args(&self, tool: &str, args: &Value) -> Value {
        match tool {
            "Write" => Self::map_write_args(args),
            "Edit" => Self::map_edit_args(args),
            "Bash" => Self::map_bash_args(args),
            "Read" => Self::map_read_args(args),
            _ => args.clone(),
        }
    }

    fn format_hint(&self) -> &'static str {
        "RustyCode tool format: agent_tool_write(path, data), agent_tool_edit(path, old, new), agent_tool_bash(cmd), agent_tool_read(path)"
    }

    fn harness(&self) -> ToolHarness {
        ToolHarness::RustyCode
    }
}

/// Gemini CLI adapter — sandbox-based execution with limited file I/O.
pub struct GeminiAdapter;

impl ToolAdapter for GeminiAdapter {
    fn normalize_tool_name(&self, tool: &str) -> String {
        format!("gemini_{tool}")
    }

    fn normalize_args(&self, _tool: &str, args: &Value) -> Value {
        args.clone()
    }

    fn format_hint(&self) -> &'static str {
        "Gemini sandbox tools: gemini_Write, gemini_Edit, gemini_Bash, gemini_Read. Note: sandbox has limited file I/O capabilities"
    }

    fn harness(&self) -> ToolHarness {
        ToolHarness::GeminiCli
    }
}

/// `OpenAI` Codex adapter — different tool registry format.
pub struct CodexAdapter;

impl ToolAdapter for CodexAdapter {
    fn normalize_tool_name(&self, tool: &str) -> String {
        format!("codex_{tool}")
    }

    fn normalize_args(&self, _tool: &str, args: &Value) -> Value {
        args.clone()
    }

    fn format_hint(&self) -> &'static str {
        "Codex tool format: codex_Write, codex_Edit, codex_Bash, codex_Read"
    }

    fn harness(&self) -> ToolHarness {
        ToolHarness::Codex
    }
}

/// Get the appropriate adapter for a given harness.
pub fn adapter(harness: ToolHarness) -> Box<dyn ToolAdapter> {
    match harness {
        ToolHarness::ClaudeCode => Box::new(ClaudeCodeAdapter),
        ToolHarness::RustyCode => Box::new(RustyCodeAdapter),
        ToolHarness::GeminiCli => Box::new(GeminiAdapter),
        ToolHarness::Codex => Box::new(CodexAdapter),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_code_identity_tool_name() {
        let adapter = ClaudeCodeAdapter;
        assert_eq!(adapter.normalize_tool_name("Write"), "Write");
        assert_eq!(adapter.normalize_tool_name("Bash"), "Bash");
    }

    #[test]
    fn claude_code_identity_args() {
        let adapter = ClaudeCodeAdapter;
        let args = json!({"file_path": "test.rs", "content": "fn main() {}"});
        assert_eq!(adapter.normalize_args("Write", &args), args);
    }

    #[test]
    fn rustycode_tool_name_mapping() {
        let adapter = RustyCodeAdapter;
        assert_eq!(adapter.normalize_tool_name("Write"), "agent_tool_write");
        assert_eq!(adapter.normalize_tool_name("Edit"), "agent_tool_edit");
        assert_eq!(adapter.normalize_tool_name("Bash"), "agent_tool_bash");
        assert_eq!(adapter.normalize_tool_name("Read"), "agent_tool_read");
    }

    #[test]
    fn rustycode_unknown_tool_gets_prefix() {
        let adapter = RustyCodeAdapter;
        assert_eq!(
            adapter.normalize_tool_name("CustomTool"),
            "agent_tool_customtool"
        );
    }

    #[test]
    fn rustycode_write_args_mapping() {
        let adapter = RustyCodeAdapter;
        let ast_args = json!({"file_path": "src/main.rs", "content": "hello"});
        let result = adapter.normalize_args("Write", &ast_args);
        assert_eq!(result["path"], "src/main.rs");
        assert_eq!(result["data"], "hello");
        assert!(result.get("file_path").is_none());
        assert!(result.get("content").is_none());
    }

    #[test]
    fn rustycode_write_args_reverse_keys() {
        let adapter = RustyCodeAdapter;
        let ast_args = json!({"path": "src/lib.rs", "data": "world"});
        let result = adapter.normalize_args("Write", &ast_args);
        assert_eq!(result["path"], "src/lib.rs");
        assert_eq!(result["data"], "world");
    }

    #[test]
    fn rustycode_edit_args_mapping() {
        let adapter = RustyCodeAdapter;
        let ast_args = json!({
            "file_path": "src/lib.rs",
            "old_string": "old code",
            "new_string": "new code"
        });
        let result = adapter.normalize_args("Edit", &ast_args);
        assert_eq!(result["path"], "src/lib.rs");
        assert_eq!(result["old"], "old code");
        assert_eq!(result["new"], "new code");
    }

    #[test]
    fn rustycode_bash_args_mapping() {
        let adapter = RustyCodeAdapter;
        let ast_args = json!({"command": "cargo test"});
        let result = adapter.normalize_args("Bash", &ast_args);
        assert_eq!(result["cmd"], "cargo test");
    }

    #[test]
    fn rustycode_read_args_mapping() {
        let adapter = RustyCodeAdapter;
        let ast_args = json!({"file_path": "Cargo.toml"});
        let result = adapter.normalize_args("Read", &ast_args);
        assert_eq!(result["path"], "Cargo.toml");
    }

    #[test]
    fn gemini_tool_name_prefix() {
        let adapter = GeminiAdapter;
        assert_eq!(adapter.normalize_tool_name("Write"), "gemini_Write");
    }

    #[test]
    fn codex_tool_name_prefix() {
        let adapter = CodexAdapter;
        assert_eq!(adapter.normalize_tool_name("Write"), "codex_Write");
    }

    #[test]
    fn factory_returns_correct_adapter() {
        assert_eq!(
            adapter(ToolHarness::ClaudeCode).harness(),
            ToolHarness::ClaudeCode
        );
        assert_eq!(
            adapter(ToolHarness::RustyCode).harness(),
            ToolHarness::RustyCode
        );
        assert_eq!(
            adapter(ToolHarness::GeminiCli).harness(),
            ToolHarness::GeminiCli
        );
        assert_eq!(
            adapter(ToolHarness::Codex).harness(),
            ToolHarness::Codex
        );
    }

    #[test]
    fn harness_display() {
        assert_eq!(ToolHarness::ClaudeCode.to_string(), "claude-code");
        assert_eq!(ToolHarness::RustyCode.to_string(), "rustycode");
        assert_eq!(ToolHarness::GeminiCli.to_string(), "gemini-cli");
        assert_eq!(ToolHarness::Codex.to_string(), "codex");
    }

    #[test]
    fn all_adapters_have_format_hints() {
        let harnesses = [
            ToolHarness::ClaudeCode,
            ToolHarness::RustyCode,
            ToolHarness::GeminiCli,
            ToolHarness::Codex,
        ];
        for h in harnesses {
            let adapter = adapter(h);
            let hint = adapter.format_hint();
            assert!(!hint.is_empty(), "Format hint empty for {h}");
        }
    }

    #[test]
    fn tool_harness_serialization_roundtrip() {
        let harnesses = [
            ToolHarness::ClaudeCode,
            ToolHarness::RustyCode,
            ToolHarness::GeminiCli,
            ToolHarness::Codex,
        ];
        for h in harnesses {
            let json = serde_json::to_string(&h).unwrap();
            let back: ToolHarness = serde_json::from_str(&json).unwrap();
            assert_eq!(back, h);
        }
    }

    #[test]
    fn rustycode_end_to_end_write_translation() {
        let adapter = adapter(ToolHarness::RustyCode);

        let tool = "Write";
        let args = json!({"file_path": "src/parser.rs", "content": "fn parse() {}"});

        assert_eq!(adapter.normalize_tool_name(tool), "agent_tool_write");
        let normalized_args = adapter.normalize_args(tool, &args);
        assert_eq!(normalized_args["path"], "src/parser.rs");
        assert_eq!(normalized_args["data"], "fn parse() {}");
    }

    #[test]
    fn claude_code_end_to_end_passthrough() {
        let adapter = adapter(ToolHarness::ClaudeCode);

        let tool = "Write";
        let args = json!({"file_path": "src/parser.rs", "content": "fn parse() {}"});

        assert_eq!(adapter.normalize_tool_name(tool), "Write");
        assert_eq!(adapter.normalize_args(tool, &args), args);
    }
}
