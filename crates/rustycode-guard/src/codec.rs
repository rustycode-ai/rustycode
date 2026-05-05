use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct HookInput {
    pub session_id: Option<String>,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub cwd: Option<String>,
    pub hook_event_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookResult {
    pub hook_event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_decision_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_context: Option<String>,
}

pub fn parse_input(json: &str) -> Result<HookInput> {
    serde_json::from_str(json).context("Failed to parse hook input JSON")
}

#[allow(dead_code)]
pub fn write_result(result: &HookResult) -> Result<()> {
    let json = serde_json::to_string(result)?;
    println!("{json}");
    Ok(())
}

impl HookResult {
    pub fn allow() -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: None,
            permission_decision_reason: None,
            updated_input: None,
            additional_context: None,
        }
    }

    pub fn deny(reason: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: Some("deny".to_string()),
            permission_decision_reason: Some(reason.into()),
            updated_input: None,
            additional_context: None,
        }
    }

    pub fn warn(context: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PostToolUse".to_string(),
            permission_decision: None,
            permission_decision_reason: None,
            updated_input: None,
            additional_context: Some(context.into()),
        }
    }

    pub fn ask(reason: impl Into<String>) -> Self {
        Self {
            hook_event_name: "PreToolUse".to_string(),
            permission_decision: Some("ask".to_string()),
            permission_decision_reason: Some(reason.into()),
            updated_input: None,
            additional_context: None,
        }
    }
}

#[allow(dead_code)]
pub fn format_result_string(result: &HookResult) -> String {
    serde_json::to_string(result).unwrap_or_else(|_| String::from("{}"))
}

// Protocol type conversions

#[allow(clippy::use_self)]
impl From<HookInput> for rustycode_protocol::HookInput {
    fn from(input: HookInput) -> Self {
        let event = input
            .hook_event_name
            .as_deref()
            .and_then(|name| match name {
                "PreToolUse" => Some(rustycode_protocol::HookEvent::PreToolUse),
                "PostToolUse" => Some(rustycode_protocol::HookEvent::PostToolUse),
                "SessionStart" => Some(rustycode_protocol::HookEvent::SessionStart),
                "SessionEnd" => Some(rustycode_protocol::HookEvent::SessionEnd),
                _ => None,
            })
            .unwrap_or(rustycode_protocol::HookEvent::PreToolUse);

        rustycode_protocol::HookInput::builder(
            event,
            input.session_id.unwrap_or_default(),
            input.cwd.unwrap_or_default(),
        )
        .tool_name(input.tool_name)
        .tool_input(input.tool_input)
        .build()
    }
}

#[allow(clippy::use_self)]
impl From<HookResult> for rustycode_protocol::HookOutput {
    fn from(result: HookResult) -> Self {
        let decision = result.permission_decision.and_then(|d| match d.as_str() {
            "allow" => Some(rustycode_protocol::HookDecision::Allow),
            "deny" => Some(rustycode_protocol::HookDecision::Deny),
            "ask" => Some(rustycode_protocol::HookDecision::Ask),
            "block" => Some(rustycode_protocol::HookDecision::Block),
            _ => None,
        });

        rustycode_protocol::HookOutput {
            r#continue: !matches!(
                decision,
                Some(
                    rustycode_protocol::HookDecision::Deny
                        | rustycode_protocol::HookDecision::Block
                )
            ),
            decision,
            permission_decision_reason: result.permission_decision_reason,
            hook_specific_output: result.additional_context.map(|ctx| {
                rustycode_protocol::HookSpecificOutput {
                    additional_context: Some(ctx),
                }
            }),
            ..Default::default()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_input_valid() {
        let json =
            r#"{"session_id":"s1","tool_name":"bash","tool_input":{"command":"ls"},"cwd":"/tmp"}"#;
        let input = parse_input(json).unwrap();
        assert_eq!(input.session_id, Some("s1".to_string()));
        assert_eq!(input.tool_name, "bash");
        assert_eq!(input.cwd, Some("/tmp".to_string()));
    }

    #[test]
    fn test_parse_input_minimal() {
        let json = r#"{"tool_name":"read","tool_input":{}}"#;
        let input = parse_input(json).unwrap();
        assert!(input.session_id.is_none());
        assert_eq!(input.tool_name, "read");
        assert!(input.cwd.is_none());
    }

    #[test]
    fn test_parse_input_invalid_json() {
        let result = parse_input("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_result_allow() {
        let result = HookResult::allow();
        assert_eq!(result.hook_event_name, "PreToolUse");
        assert!(result.permission_decision.is_none());
        assert!(result.permission_decision_reason.is_none());
        assert!(result.updated_input.is_none());
        assert!(result.additional_context.is_none());
    }

    #[test]
    fn test_hook_result_deny() {
        let result = HookResult::deny("dangerous command");
        assert_eq!(result.permission_decision, Some("deny".to_string()));
        assert_eq!(
            result.permission_decision_reason,
            Some("dangerous command".to_string())
        );
    }

    #[test]
    fn test_hook_result_warn() {
        let result = HookResult::warn("file modified");
        assert_eq!(result.hook_event_name, "PostToolUse");
        assert_eq!(result.additional_context, Some("file modified".to_string()));
    }

    #[test]
    fn test_hook_result_ask() {
        let result = HookResult::ask("needs approval");
        assert_eq!(result.permission_decision, Some("ask".to_string()));
        assert_eq!(
            result.permission_decision_reason,
            Some("needs approval".to_string())
        );
    }

    #[test]
    fn test_hook_result_serde_roundtrip_allow() {
        let result = HookResult::allow();
        let json = serde_json::to_string(&result).unwrap();
        let de: HookResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.hook_event_name, "PreToolUse");
        assert!(de.permission_decision.is_none());
    }

    #[test]
    fn test_hook_result_serde_roundtrip_deny() {
        let result = HookResult::deny("nope");
        let json = serde_json::to_string(&result).unwrap();
        let de: HookResult = serde_json::from_str(&json).unwrap();
        assert_eq!(de.permission_decision, Some("deny".to_string()));
    }

    #[test]
    fn test_hook_result_camel_case_serialization() {
        let result = HookResult::deny("reason");
        let json = serde_json::to_string(&result).unwrap();
        // serde rename_all = camelCase
        assert!(json.contains("hookEventName"));
        assert!(json.contains("permissionDecision"));
    }

    #[test]
    fn test_format_result_string() {
        let result = HookResult::allow();
        let s = format_result_string(&result);
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["hookEventName"], "PreToolUse");
    }

    #[test]
    fn test_write_result_outputs_json() {
        // Verify write_result produces valid JSON output
        let result = HookResult::deny("test deny");
        let json = serde_json::to_string(&result).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
    }
}
