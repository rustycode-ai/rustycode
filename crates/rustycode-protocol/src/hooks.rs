//! Claude Code-compatible hook protocol types
//!
//! This module defines the data types for the RustyCode hook system, mirroring
//! the Claude Code hooks protocol exactly. See:
//! <https://docs.anthropic.com/en/docs/claude-code/hooks>
//!
//! # Hook Events
//!
//! Nine lifecycle events where hooks can fire:
//! - [`HookEvent::PreToolUse`] / [`HookEvent::PostToolUse`] — tool execution gating
//! - [`HookEvent::Notification`] — notification events
//! - [`HookEvent::UserPromptSubmit`] — user input preprocessing
//! - [`HookEvent::Stop`] / [`HookEvent::SubagentStop`] — session/subagent termination
//! - [`HookEvent::PreCompact`] — before context compaction
//! - [`HookEvent::SessionStart`] / [`HookEvent::SessionEnd`] — session lifecycle
//!
//! # Exit Code Semantics
//!
//! | Exit code | Meaning              | stdout             | stderr                 |
//! |-----------|----------------------|--------------------|------------------------|
//! | 0         | Success              | Parsed as JSON     | Logged                 |
//! | 2         | Blocking error       | Ignored            | Fed back to Claude     |
//! | Other     | Non-blocking error   | Ignored            | Logged                 |
//!
//! # Configuration
//!
//! Hooks are configured in `settings.json` under the `hooks` key.
//! Supports both Claude Code nested format and legacy flat format:
//!
//! ```json
//! // Nested format (Claude Code standard):
//! {
//!   "hooks": {
//!     "PreToolUse": [{
//!       "matcher": "Edit|Write",
//!       "hooks": [{ "type": "command", "command": "fmt.sh", "timeout": 30 }]
//!     }]
//!   }
//! }
//!
//! // Flat format (legacy, also accepted):
//! {
//!   "hooks": {
//!     "PreToolUse": [{ "matcher": "Edit|Write", "command": "fmt.sh", "timeout": 30 }]
//!   }
//! }
//! ```

use serde::{Deserialize, Serialize};

// HOOK EVENT ENUM

/// The nine hook lifecycle events, matching Claude Code's protocol exactly.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    Notification,
    UserPromptSubmit,
    Stop,
    SubagentStop,
    PreCompact,
    SessionStart,
    SessionEnd,
}

impl HookEvent {
    /// All hook events in canonical order.
    pub const ALL: [HookEvent; 9] = [
        HookEvent::PreToolUse,
        HookEvent::PostToolUse,
        HookEvent::Notification,
        HookEvent::UserPromptSubmit,
        HookEvent::Stop,
        HookEvent::SubagentStop,
        HookEvent::PreCompact,
        HookEvent::SessionStart,
        HookEvent::SessionEnd,
    ];

    /// Whether this event supports matcher patterns.
    ///
    /// PreToolUse/PostToolUse match tool names, PreCompact matches
    /// "manual"/"auto", SessionStart matches "startup"/"resume"/"clear"/"compact".
    pub fn supports_matcher(self) -> bool {
        matches!(
            self,
            HookEvent::PreToolUse
                | HookEvent::PostToolUse
                | HookEvent::PreCompact
                | HookEvent::SessionStart
        )
    }
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        // Remove quotes from JSON string
        write!(f, "{}", s.trim_matches('"'))
    }
}

// HOOK CONFIGURATION (settings.json format)

/// Top-level hooks configuration from `settings.json`.
///
/// Supports both Claude Code nested format and legacy flat format.
/// Use [`HooksConfig::entries_for`] to get flattened [`HookEntry`] items
/// for a given event.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub hooks: std::collections::HashMap<HookEvent, Vec<HookConfigEntry>>,
}

impl HooksConfig {
    /// Get flattened hook entries for a specific event.
    ///
    /// Normalizes both nested and flat config formats into a uniform
    /// list of [`HookEntry`] items ready for execution.
    pub fn entries_for(&self, event: HookEvent) -> Vec<HookEntry> {
        self.hooks
            .get(&event)
            .map(|entries| {
                entries
                    .iter()
                    .flat_map(HookConfigEntry::to_entries)
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A hook config entry that deserializes from either nested or flat format.
///
/// Nested format has `hooks` array, flat format has `command` directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HookConfigEntry {
    /// Claude Code nested format: `matcher` + `hooks` array.
    Group(MatcherGroup),
    /// Legacy flat format: `matcher` + `command` directly.
    Flat(FlatHookEntry),
}

impl HookConfigEntry {
    /// Convert to flattened hook entries for execution.
    pub fn to_entries(&self) -> Vec<HookEntry> {
        match self {
            Self::Group(group) => group
                .hooks
                .iter()
                .map(|h| HookEntry {
                    matcher: group.matcher.clone(),
                    command: h.command.clone(),
                    timeout: h.timeout,
                })
                .collect(),
            Self::Flat(flat) => vec![HookEntry {
                matcher: flat.matcher.clone(),
                command: flat.command.clone(),
                timeout: flat.timeout,
            }],
        }
    }
}

/// Nested format entry matching Claude Code's settings.json structure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatcherGroup {
    /// Regex matcher. Meaning depends on event:
    /// - PreToolUse/PostToolUse: tool name pattern
    /// - PreCompact: "manual" or "auto"
    /// - SessionStart: "startup", "resume", "clear", or "compact"
    #[serde(default)]
    pub matcher: Option<String>,

    /// Array of hook definitions to run when the matcher matches.
    pub hooks: Vec<HookDef>,
}

/// Legacy flat format entry (backward compatibility).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlatHookEntry {
    #[serde(default)]
    pub matcher: Option<String>,
    pub command: String,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

/// A single hook definition in nested format.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookDef {
    /// Hook type. Currently only "command" is supported.
    #[serde(default = "default_hook_type", rename = "type")]
    pub hook_type: HookType,

    /// Shell command to execute.
    pub command: String,

    /// Per-hook timeout in seconds. Defaults to 60.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

/// Hook type discriminator.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookType {
    #[default]
    Command,
}

const fn default_hook_type() -> HookType {
    HookType::Command
}

/// Flattened hook entry used for execution.
///
/// Produced by [`HookConfigEntry::to_entries`] regardless of whether
/// the config was parsed from nested or flat format.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HookEntry {
    /// Regex matcher (propagated from MatcherGroup or FlatHookEntry).
    #[serde(default)]
    pub matcher: Option<String>,

    /// Shell command to execute.
    pub command: String,

    /// Per-hook timeout in seconds. Defaults to 60.
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

pub const fn default_timeout() -> u64 {
    60
}

/// Default timeout for all hooks (seconds).
pub const DEFAULT_HOOK_TIMEOUT_SECS: u64 = 60;

// HOOK INPUT (JSON passed via stdin)

/// Input payload sent to hook scripts via stdin as JSON.
///
/// The exact fields vary by event. The `event` and `session_id` and `cwd`
/// fields are always present. Additional fields are populated per event type.
#[derive(Clone, Debug, Serialize)]
pub struct HookInput {
    /// Which event triggered this hook.
    pub event: HookEvent,

    /// Always-present fields
    pub session_id: String,
    pub cwd: String,

    // --- PreToolUse / PostToolUse fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_input: Option<serde_json::Value>,

    // --- PostToolUse-only fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<String>,

    // --- Notification fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    // --- UserPromptSubmit fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    // --- Stop / SubagentStop fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_hook_active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<String>,

    // --- PreCompact fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,

    // --- SessionEnd fields ---
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Builder for constructing event-specific hook input.
impl HookInput {
    /// Start building input for a given event.
    pub fn builder(
        event: HookEvent,
        session_id: impl Into<String>,
        cwd: impl Into<String>,
    ) -> HookInputBuilder {
        HookInputBuilder {
            inner: Self {
                event,
                session_id: session_id.into(),
                cwd: cwd.into(),
                tool_name: None,
                tool_input: None,
                tool_response: None,
                message: None,
                title: None,
                prompt: None,
                stop_hook_active: None,
                transcript_path: None,
                conversation: None,
                trigger: None,
                custom_instructions: None,
                source: None,
                reason: None,
            },
        }
    }
}

/// Builder for [`HookInput`].
pub struct HookInputBuilder {
    inner: HookInput,
}

impl HookInputBuilder {
    pub fn tool_name(mut self, name: impl Into<String>) -> Self {
        self.inner.tool_name = Some(name.into());
        self
    }

    pub fn tool_input(mut self, input: serde_json::Value) -> Self {
        self.inner.tool_input = Some(input);
        self
    }

    pub fn tool_response(mut self, response: impl Into<String>) -> Self {
        self.inner.tool_response = Some(response.into());
        self
    }

    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.inner.message = Some(msg.into());
        self
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.inner.title = Some(t.into());
        self
    }

    pub fn prompt(mut self, p: impl Into<String>) -> Self {
        self.inner.prompt = Some(p.into());
        self
    }

    pub fn stop_hook_active(mut self, active: bool) -> Self {
        self.inner.stop_hook_active = Some(active);
        self
    }

    pub fn transcript_path(mut self, path: impl Into<String>) -> Self {
        self.inner.transcript_path = Some(path.into());
        self
    }

    pub fn conversation(mut self, conv: impl Into<String>) -> Self {
        self.inner.conversation = Some(conv.into());
        self
    }

    pub fn trigger(mut self, trigger: impl Into<String>) -> Self {
        self.inner.trigger = Some(trigger.into());
        self
    }

    pub fn custom_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.inner.custom_instructions = Some(instructions.into());
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.inner.source = Some(source.into());
        self
    }

    pub fn reason(mut self, reason: impl Into<String>) -> Self {
        self.inner.reason = Some(reason.into());
        self
    }

    pub fn build(self) -> HookInput {
        self.inner
    }
}

// HOOK OUTPUT (parsed from stdout, exit code 0)

/// Parsed JSON output from a hook script (exit code 0).
///
/// All fields are optional — hooks can output `{}` or partial JSON.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOutput {
    /// Whether to continue execution. Defaults to `true` if absent.
    #[serde(default = "default_true")]
    pub r#continue: bool,

    /// Human-readable stop reason (used when `continue` is false).
    #[serde(default)]
    pub stop_reason: Option<String>,

    /// PreToolUse decision: `"allow"`, `"deny"`, or `"ask"`.
    /// - `allow`: bypass the permission check for this tool call
    /// - `deny`: block the tool, show reason to Claude
    /// - `ask`: show permission prompt to user
    #[serde(default)]
    pub decision: Option<HookDecision>,

    /// Alias for `decision` in PreToolUse context.
    #[serde(default)]
    pub permission_decision: Option<HookDecision>,

    /// Human-readable reason for the decision or block action.
    /// Used by deny/ask decisions and block decisions across all events.
    #[serde(default)]
    pub reason: Option<String>,

    /// Human-readable reason for the permission decision (legacy alias for `reason`).
    #[serde(default)]
    pub permission_decision_reason: Option<String>,

    /// Whether to suppress tool output from being shown to Claude.
    #[serde(default)]
    pub suppress_output: Option<bool>,

    /// System message injected into the conversation context.
    #[serde(default)]
    pub system_message: Option<String>,

    /// Event-specific structured output.
    #[serde(default)]
    pub hook_specific_output: Option<HookSpecificOutput>,
}

use crate::default_true;

/// Decision returned by hooks.
///
/// - PreToolUse: allow/deny/ask
/// - PostToolUse/UserPromptSubmit/Stop/SubagentStop: block (sets `continue: false`)
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    Allow,
    Deny,
    Ask,
    Block,
}

/// Event-specific output from hooks.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSpecificOutput {
    /// Additional context text injected into Claude's context window.
    #[serde(default)]
    pub additional_context: Option<String>,
}

// EXIT CODE SEMANTICS

/// Exit code interpretation per Claude Code protocol.
#[derive(Clone, Debug)]
pub enum HookExitCode {
    /// Exit code 0: success. Stdout is parsed as [`HookOutput`].
    Success(HookOutput),
    /// Exit code 2: blocking error. Stderr is fed back to Claude as a tool result.
    Block(String),
    /// Any other exit code: non-blocking error. Hook is skipped.
    Error(i32),
}

impl HookExitCode {
    /// Interpret a process exit code + stdout + stderr.
    pub fn from_process(exit_code: i32, stdout: &str, stderr: &str) -> Self {
        match exit_code {
            0 => {
                let output = if stdout.trim().is_empty() {
                    HookOutput::default()
                } else {
                    serde_json::from_str(stdout.trim()).unwrap_or_default()
                };
                Self::Success(output)
            }
            2 => Self::Block(stderr.to_string()),
            code => Self::Error(code),
        }
    }

    /// Whether the hook allows the operation to proceed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Success(_) | Self::Error(_))
    }
}

// HOOK EXECUTION RESULT

/// Result of executing all hooks for a single event.
#[derive(Clone, Debug, Default)]
pub struct HookExecutionResult {
    /// Individual hook results in execution order.
    pub results: Vec<SingleHookResult>,

    /// Whether the operation should be blocked.
    pub should_block: bool,

    /// Human-readable block reason (if blocked).
    pub block_reason: Option<String>,

    /// Combined additional context from all hooks.
    pub additional_context: Vec<String>,

    /// The permission decision, if any PreToolUse hook returned one.
    pub permission_decision: Option<HookDecision>,

    pub permission_decision_reason: Option<String>,
}

/// Result of executing a single hook command.
#[derive(Clone, Debug)]
pub struct SingleHookResult {
    /// The hook command that was executed.
    pub command: String,

    /// The matcher pattern (if any) that matched.
    pub matcher: Option<String>,

    /// Parsed exit code interpretation.
    pub outcome: HookExitCode,

    /// Execution duration in milliseconds.
    pub duration_ms: u128,
}

// MATCHER LOGIC

/// Check if a tool name matches a hook's matcher pattern.
///
/// The matcher is a regex applied against the tool name.
/// If the matcher is `None`, the hook matches all tools.
pub fn tool_matches_matcher(tool_name: &str, matcher: &Option<String>) -> bool {
    matcher.as_ref().is_none_or(|pattern| {
        regex::Regex::new(pattern)
            .map(|re| re.is_match(tool_name))
            .unwrap_or(false)
    })
}

// ENVIRONMENT VARIABLES

/// Environment variable set for all hook commands.
pub const ENV_CLAUDE_PROJECT_DIR: &str = "CLAUDE_PROJECT_DIR";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_serde_roundtrip() {
        for event in HookEvent::ALL {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: HookEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, event);
        }
    }

    #[test]
    fn hook_event_display() {
        assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
        assert_eq!(HookEvent::SessionStart.to_string(), "SessionStart");
        assert_eq!(HookEvent::SubagentStop.to_string(), "SubagentStop");
    }

    #[test]
    fn hook_event_supports_matcher() {
        assert!(HookEvent::PreToolUse.supports_matcher());
        assert!(HookEvent::PostToolUse.supports_matcher());
        assert!(HookEvent::PreCompact.supports_matcher());
        assert!(HookEvent::SessionStart.supports_matcher());
        assert!(!HookEvent::Stop.supports_matcher());
        assert!(!HookEvent::Notification.supports_matcher());
        assert!(!HookEvent::UserPromptSubmit.supports_matcher());
        assert!(!HookEvent::SubagentStop.supports_matcher());
        assert!(!HookEvent::SessionEnd.supports_matcher());
    }

    #[test]
    fn hooks_config_parse_flat_format() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Edit|Write", "command": "fmt.sh", "timeout": 30 }
                ],
                "Stop": [
                    { "command": "verify.sh" }
                ]
            }
        }"#;

        let config: HooksConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.hooks.len(), 2);

        // entries_for flattens both formats
        let pre_entries = config.entries_for(HookEvent::PreToolUse);
        assert_eq!(pre_entries.len(), 1);
        assert_eq!(pre_entries[0].matcher, Some("Edit|Write".to_string()));
        assert_eq!(pre_entries[0].command, "fmt.sh");
        assert_eq!(pre_entries[0].timeout, 30);

        let stop_entries = config.entries_for(HookEvent::Stop);
        assert_eq!(stop_entries.len(), 1);
        assert_eq!(stop_entries[0].matcher, None);
        assert_eq!(stop_entries[0].command, "verify.sh");
        assert_eq!(stop_entries[0].timeout, 60);
    }

    #[test]
    fn hooks_config_parse_nested_format() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Edit|Write",
                    "hooks": [
                        { "type": "command", "command": "validate.sh", "timeout": 15 },
                        { "type": "command", "command": "fmt.sh" }
                    ]
                }],
                "Stop": [{
                    "hooks": [{ "type": "command", "command": "verify.sh" }]
                }]
            }
        }"#;

        let config: HooksConfig = serde_json::from_str(json).unwrap();
        let pre_entries = config.entries_for(HookEvent::PreToolUse);
        assert_eq!(pre_entries.len(), 2);
        assert_eq!(pre_entries[0].matcher, Some("Edit|Write".to_string()));
        assert_eq!(pre_entries[0].command, "validate.sh");
        assert_eq!(pre_entries[0].timeout, 15);
        assert_eq!(pre_entries[1].matcher, Some("Edit|Write".to_string()));
        assert_eq!(pre_entries[1].command, "fmt.sh");
        assert_eq!(pre_entries[1].timeout, 60);

        let stop_entries = config.entries_for(HookEvent::Stop);
        assert_eq!(stop_entries.len(), 1);
        assert_eq!(stop_entries[0].matcher, None);
    }

    #[test]
    fn hooks_config_mixed_formats() {
        let json = r#"{
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "command": "check.sh" },
                    { "matcher": "Edit", "hooks": [{ "type": "command", "command": "fmt.sh" }] }
                ]
            }
        }"#;

        let config: HooksConfig = serde_json::from_str(json).unwrap();
        let entries = config.entries_for(HookEvent::PreToolUse);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "check.sh");
        assert_eq!(entries[1].command, "fmt.sh");
    }

    #[test]
    fn hooks_config_empty() {
        let json = r#"{}"#;
        let config: HooksConfig = serde_json::from_str(json).unwrap();
        assert!(config.hooks.is_empty());
    }

    #[test]
    fn hook_input_pre_tool_use() {
        let input = HookInput::builder(HookEvent::PreToolUse, "sess_123", "/project")
            .tool_name("Edit")
            .tool_input(serde_json::json!({"file": "main.rs"}))
            .build();

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"event\":\"PreToolUse\""));
        assert!(json.contains("\"tool_name\":\"Edit\""));
        assert!(json.contains("\"session_id\":\"sess_123\""));
        assert!(json.contains("\"cwd\":\"/project\""));
        // PostToolUse-only fields should be absent
        assert!(!json.contains("\"tool_response\""));
        assert!(!json.contains("\"stop_hook_active\""));
    }

    #[test]
    fn hook_input_post_tool_use() {
        let input = HookInput::builder(HookEvent::PostToolUse, "sess_123", "/project")
            .tool_name("Edit")
            .tool_input(serde_json::json!({"file": "main.rs"}))
            .tool_response("File written successfully")
            .build();

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"tool_response\":\"File written successfully\""));
    }

    #[test]
    fn hook_input_stop() {
        let input = HookInput::builder(HookEvent::Stop, "sess_123", "/project")
            .stop_hook_active(true)
            .transcript_path("/tmp/transcript.jsonl")
            .build();

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"stop_hook_active\":true"));
        assert!(json.contains("\"transcript_path\""));
    }

    #[test]
    fn hook_output_default_continue_true() {
        let output: HookOutput = serde_json::from_str("{}").unwrap();
        assert!(output.r#continue);
    }

    #[test]
    fn hook_output_with_decision() {
        let json = r#"{"decision":"allow","permissionDecisionReason":"trusted tool"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, Some(HookDecision::Allow));
        assert_eq!(
            output.permission_decision_reason,
            Some("trusted tool".to_string())
        );
    }

    #[test]
    fn hook_output_deny_decision() {
        let json = r#"{"continue":false,"decision":"deny","stopReason":"blocked by policy"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert!(!output.r#continue);
        assert_eq!(output.decision, Some(HookDecision::Deny));
        assert_eq!(output.stop_reason, Some("blocked by policy".to_string()));
    }

    #[test]
    fn hook_output_ask_decision() {
        let json = r#"{"decision":"ask","permissionDecisionReason":"sensitive file"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, Some(HookDecision::Ask));
    }

    #[test]
    fn hook_output_additional_context() {
        let json = r#"{"hookSpecificOutput":{"additionalContext":"line count: 42"}}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(
            output
                .hook_specific_output
                .and_then(|o| o.additional_context),
            Some("line count: 42".to_string())
        );
    }

    #[test]
    fn hook_exit_code_success() {
        let stdout = r#"{"decision":"allow"}"#;
        let result = HookExitCode::from_process(0, stdout, "");
        assert!(result.is_allowed());
        match result {
            HookExitCode::Success(output) => {
                assert_eq!(output.decision, Some(HookDecision::Allow));
            }
            _ => panic!("expected Success"),
        }
    }

    #[test]
    fn hook_exit_code_success_empty_stdout() {
        let result = HookExitCode::from_process(0, "", "");
        assert!(result.is_allowed());
        assert!(matches!(result, HookExitCode::Success(_)));
    }

    #[test]
    fn hook_exit_code_block() {
        let result = HookExitCode::from_process(2, "", "Blocked: dangerous tool");
        assert!(!result.is_allowed());
        match result {
            HookExitCode::Block(msg) => {
                assert_eq!(msg, "Blocked: dangerous tool");
            }
            _ => panic!("expected Block"),
        }
    }

    #[test]
    fn hook_exit_code_non_blocking_error() {
        let result = HookExitCode::from_process(1, "", "some error");
        assert!(result.is_allowed());
        assert!(matches!(result, HookExitCode::Error(1)));
    }

    #[test]
    fn tool_matches_matcher_no_matcher() {
        assert!(tool_matches_matcher("Edit", &None));
        assert!(tool_matches_matcher("Bash", &None));
    }

    #[test]
    fn tool_matches_matcher_regex() {
        let matcher = Some("Edit|Write".to_string());
        assert!(tool_matches_matcher("Edit", &matcher));
        assert!(tool_matches_matcher("Write", &matcher));
        assert!(!tool_matches_matcher("Bash", &matcher));
    }

    #[test]
    fn tool_matches_matcher_invalid_regex() {
        let matcher = Some("[invalid".to_string());
        assert!(!tool_matches_matcher("Edit", &matcher));
    }

    #[test]
    fn hook_decision_serde() {
        assert_eq!(
            serde_json::to_string(&HookDecision::Allow).unwrap(),
            "\"allow\""
        );
        assert_eq!(
            serde_json::to_string(&HookDecision::Deny).unwrap(),
            "\"deny\""
        );
        assert_eq!(
            serde_json::to_string(&HookDecision::Ask).unwrap(),
            "\"ask\""
        );
        assert_eq!(
            serde_json::to_string(&HookDecision::Block).unwrap(),
            "\"block\""
        );
        assert_eq!(
            serde_json::from_str::<HookDecision>("\"block\"").unwrap(),
            HookDecision::Block
        );
    }

    #[test]
    fn hook_output_block_decision() {
        let json = r#"{"continue":false,"decision":"block","reason":"unsafe operation"}"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert!(!output.r#continue);
        assert_eq!(output.decision, Some(HookDecision::Block));
        assert_eq!(output.reason, Some("unsafe operation".to_string()));
    }

    #[test]
    fn hook_output_new_fields() {
        let json = r#"{
            "reason": "policy violation",
            "suppressOutput": true,
            "systemMessage": "Hook blocked this action"
        }"#;
        let output: HookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.reason, Some("policy violation".to_string()));
        assert_eq!(output.suppress_output, Some(true));
        assert_eq!(
            output.system_message,
            Some("Hook blocked this action".to_string())
        );
    }

    #[test]
    fn hook_def_type_default() {
        let json = r#"{"command": "fmt.sh"}"#;
        let def: HookDef = serde_json::from_str(json).unwrap();
        assert_eq!(def.hook_type, HookType::Command);
        assert_eq!(def.timeout, 60);
    }

    #[test]
    fn matcher_group_multiple_hooks() {
        let json = r#"{
            "matcher": "Edit|Write",
            "hooks": [
                { "type": "command", "command": "lint.sh", "timeout": 10 },
                { "type": "command", "command": "fmt.sh", "timeout": 20 }
            ]
        }"#;
        let group: MatcherGroup = serde_json::from_str(json).unwrap();
        assert_eq!(group.matcher, Some("Edit|Write".to_string()));
        assert_eq!(group.hooks.len(), 2);
        assert_eq!(group.hooks[0].command, "lint.sh");
        assert_eq!(group.hooks[1].command, "fmt.sh");
    }

    #[test]
    fn entries_for_missing_event() {
        let config: HooksConfig = serde_json::from_str("{}").unwrap();
        assert!(config.entries_for(HookEvent::PreToolUse).is_empty());
    }

    #[test]
    fn all_hook_events_are_nine() {
        assert_eq!(HookEvent::ALL.len(), 9);
    }
}
