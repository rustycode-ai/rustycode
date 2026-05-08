//! Rich per-event stdin/stdout protocol for hooks
//!
//! Compatible with Claude Code / Codex hook JSON schemas.
//! PostToolUse hooks receive `tool_response` and can return `decision: "block"`
//! to replace tool output before the LLM sees it (credential masking).

use serde::{Deserialize, Serialize};

/// Hook event names matching Claude Code / Codex convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    PreToolUse,
    PostToolUse,
    PermissionRequest,
    UserPromptSubmit,
    Stop,
    SessionStart,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_default();
        // Remove surrounding quotes from JSON string
        write!(f, "{}", s.trim_matches('"'))
    }
}

/// Input sent to hook via stdin as JSON.
///
/// Each variant carries event-specific fields. The `hook_event_name` tag
/// identifies the event type for hook scripts.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "hook_event_name")]
pub enum HookProtocolInput {
    PreToolUse {
        session_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        cwd: String,
    },
    PostToolUse {
        session_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        /// The tool's response — hooks can see and optionally replace this.
        tool_response: serde_json::Value,
        cwd: String,
    },
    PermissionRequest {
        session_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        cwd: String,
    },
    UserPromptSubmit {
        session_id: String,
        prompt: String,
        cwd: String,
    },
    Stop {
        session_id: String,
        last_assistant_message: Option<String>,
        cwd: String,
    },
    SessionStart {
        session_id: String,
        source: String,
        cwd: String,
    },
}

impl HookProtocolInput {
    pub fn event(&self) -> HookEvent {
        match self {
            Self::PreToolUse { .. } => HookEvent::PreToolUse,
            Self::PostToolUse { .. } => HookEvent::PostToolUse,
            Self::PermissionRequest { .. } => HookEvent::PermissionRequest,
            Self::UserPromptSubmit { .. } => HookEvent::UserPromptSubmit,
            Self::Stop { .. } => HookEvent::Stop,
            Self::SessionStart { .. } => HookEvent::SessionStart,
        }
    }

    pub fn session_id(&self) -> &str {
        match self {
            Self::PreToolUse { session_id, .. }
            | Self::PostToolUse { session_id, .. }
            | Self::PermissionRequest { session_id, .. }
            | Self::UserPromptSubmit { session_id, .. }
            | Self::Stop { session_id, .. }
            | Self::SessionStart { session_id, .. } => session_id,
        }
    }
}

/// Output received from hook via stdout (JSON).
///
/// Hooks return this to control execution flow. Compatible with both
/// Claude Code and Codex output schemas.
#[derive(Clone, Debug, Deserialize, Default)]
pub struct HookProtocolOutput {
    /// "block" prevents execution / replaces output. "allow" permits it.
    pub decision: Option<HookDecision>,
    /// Human-readable reason shown to the user / LLM.
    pub reason: Option<String>,
    /// Extra context appended to the tool response for the LLM.
    pub additional_context: Option<String>,
    /// Injected as a system message visible only to the LLM.
    pub system_message: Option<String>,
}

/// Hook decision returned in stdout output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookDecision {
    /// Block execution or replace tool output.
    Block,
    /// Allow execution to proceed.
    Allow,
    /// Legacy alias for Allow (Codex compat).
    Approve,
}

/// Result of PreToolUse hook execution.
#[derive(Clone, Debug, Default)]
pub struct PreToolUseResult {
    pub blocked: bool,
    pub reason: Option<String>,
    pub additional_context: Option<String>,
    pub system_message: Option<String>,
}

/// Result of PostToolUse hook execution.
#[derive(Clone, Debug, Default)]
pub struct PostToolUseResult {
    /// If true, the tool response was replaced by hook feedback.
    pub replaced: bool,
    pub replacement_text: Option<String>,
    pub additional_context: Option<String>,
    pub system_message: Option<String>,
}

/// Result of PermissionRequest hook execution.
#[derive(Clone, Debug, Default)]
pub struct PermissionResult {
    /// Some(true) = auto-approve, Some(false) = auto-deny, None = no opinion.
    pub decision: Option<bool>,
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_event_display() {
        assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
        assert_eq!(HookEvent::PostToolUse.to_string(), "PostToolUse");
        assert_eq!(HookEvent::SessionStart.to_string(), "SessionStart");
    }

    #[test]
    fn hook_event_serde_roundtrip() {
        let json = serde_json::to_string(&HookEvent::PreToolUse).unwrap();
        assert_eq!(json, "\"PreToolUse\"");
        let parsed: HookEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, HookEvent::PreToolUse);
    }

    #[test]
    fn pre_tool_use_input_serializes_with_tag() {
        let input = HookProtocolInput::PreToolUse {
            session_id: "s1".into(),
            tool_name: "Bash".into(),
            tool_input: serde_json::json!({"command": "ls"}),
            cwd: "/tmp".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"hook_event_name\":\"PreToolUse\""));
        assert!(json.contains("\"tool_name\":\"Bash\""));
    }

    #[test]
    fn post_tool_use_input_includes_response() {
        let input = HookProtocolInput::PostToolUse {
            session_id: "s1".into(),
            tool_name: "Read".into(),
            tool_input: serde_json::json!({"path": "/etc/passwd"}),
            tool_response: serde_json::json!("root:x:0:0"),
            cwd: "/tmp".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"tool_response\""));
        assert!(json.contains("root:x:0:0"));
    }

    #[test]
    fn hook_output_deserialize_block() {
        let json = r#"{"decision":"block","reason":"Secret detected"}"#;
        let output: HookProtocolOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, Some(HookDecision::Block));
        assert_eq!(output.reason, Some("Secret detected".into()));
    }

    #[test]
    fn hook_output_deserialize_allow() {
        let json = r#"{"decision":"allow"}"#;
        let output: HookProtocolOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, Some(HookDecision::Allow));
    }

    #[test]
    fn hook_output_deserialize_approve_alias() {
        let json = r#"{"decision":"approve"}"#;
        let output: HookProtocolOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.decision, Some(HookDecision::Approve));
    }

    #[test]
    fn hook_output_deserialize_additional_context() {
        let json = r#"{"additional_context":"Running cargo fmt"}"#;
        let output: HookProtocolOutput = serde_json::from_str(json).unwrap();
        assert!(output.decision.is_none());
        assert_eq!(output.additional_context, Some("Running cargo fmt".into()));
    }

    #[test]
    fn hook_output_default_empty() {
        let output = HookProtocolOutput::default();
        assert!(output.decision.is_none());
        assert!(output.reason.is_none());
        assert!(output.additional_context.is_none());
        assert!(output.system_message.is_none());
    }

    #[test]
    fn input_event_accessor() {
        let input = HookProtocolInput::PreToolUse {
            session_id: "s1".into(),
            tool_name: "Bash".into(),
            tool_input: serde_json::json!({}),
            cwd: "/tmp".into(),
        };
        assert_eq!(input.event(), HookEvent::PreToolUse);
        assert_eq!(input.session_id(), "s1");
    }

    #[test]
    fn permission_request_input_serializes() {
        let input = HookProtocolInput::PermissionRequest {
            session_id: "s1".into(),
            tool_name: "Bash".into(),
            tool_input: serde_json::json!({"command": "rm -rf /"}),
            cwd: "/tmp".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"hook_event_name\":\"PermissionRequest\""));
    }

    #[test]
    fn session_start_input_serializes() {
        let input = HookProtocolInput::SessionStart {
            session_id: "s1".into(),
            source: "startup".into(),
            cwd: "/project".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"source\":\"startup\""));
    }

    #[test]
    fn stop_input_serializes() {
        let input = HookProtocolInput::Stop {
            session_id: "s1".into(),
            last_assistant_message: Some("Done!".into()),
            cwd: "/project".into(),
        };
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"last_assistant_message\":\"Done!\""));
    }
}
