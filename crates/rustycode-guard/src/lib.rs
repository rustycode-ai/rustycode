use anyhow::{anyhow, Result};
pub mod codec;
pub mod hooks_expanded;
pub mod permission;
pub mod post_tool;
pub mod pre_tool;
pub mod rules;

pub use hooks_expanded::{ExpandedHookDispatcher, LifecycleEvent, LifecycleHook};

pub fn process_hook(input_json: &str, hook_type: &str) -> Result<String> {
    let input = codec::parse_input(input_json)?;
    let result = match hook_type {
        "pre-tool" => pre_tool::evaluate(&input),
        "post-tool" => post_tool::evaluate(&input),
        "permission" => permission::evaluate(&input),
        _ => return Err(anyhow!("Unknown hook type")),
    };
    Ok(codec::format_result_string(&result))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::HookInput;
    use crate::rules::all_rules;

    fn bash_input(command: &str) -> HookInput {
        HookInput {
            session_id: Some("test-session".to_string()),
            tool_name: "Bash".to_string(),
            tool_input: serde_json::json!({ "command": command }),
            cwd: Some("/project".to_string()),
            hook_event_name: Some("PreToolUse".to_string()),
        }
    }

    fn write_input(path: &str, content: &str) -> HookInput {
        HookInput {
            session_id: Some("test-session".to_string()),
            tool_name: "Write".to_string(),
            tool_input: serde_json::json!({ "path": path, "content": content }),
            cwd: Some("/project".to_string()),
            hook_event_name: Some("PreToolUse".to_string()),
        }
    }

    fn edit_input(path: &str) -> HookInput {
        HookInput {
            session_id: Some("test-session".to_string()),
            tool_name: "Edit".to_string(),
            tool_input: serde_json::json!({ "path": path }),
            cwd: Some("/project".to_string()),
            hook_event_name: Some("PreToolUse".to_string()),
        }
    }

    // --- Rules coverage ---

    #[test]
    fn all_rules_returns_15_rules() {
        let rules = all_rules();
        assert_eq!(rules.len(), 15);
        // Each rule should have a unique ID
        let ids: Vec<&str> = rules.iter().map(|r| r.id).collect();
        let unique_ids: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique_ids.len(), "Rule IDs must be unique");
    }

    // --- R01: sudo ---

    #[test]
    fn r01_blocks_sudo() {
        let result = pre_tool::evaluate(&bash_input("sudo rm /etc/passwd"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
        assert!(result
            .permission_decision_reason
            .as_ref()
            .unwrap()
            .contains("R01"));
    }

    #[test]
    fn r01_allows_non_sudo() {
        let result = pre_tool::evaluate(&bash_input("ls -la"));
        assert!(result.permission_decision.is_none());
    }

    // --- R02: protected paths ---

    #[test]
    fn r02_blocks_write_to_env() {
        let result = pre_tool::evaluate(&write_input("/project/.env", "KEY=val"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r02_blocks_edit_to_git() {
        let result = pre_tool::evaluate(&edit_input("/project/.git/config"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r02_allows_normal_write() {
        let result = pre_tool::evaluate(&write_input("/project/src/main.rs", "fn main() {}"));
        assert!(result.permission_decision.is_none());
    }

    // --- R05: rm -rf ---

    #[test]
    fn r05_blocks_rm_rf() {
        let result = pre_tool::evaluate(&bash_input("rm -rf /tmp/test"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r05_allows_rm_without_rf() {
        let result = pre_tool::evaluate(&bash_input("rm /tmp/test.txt"));
        assert!(result.permission_decision.is_none());
    }

    // --- R06: git push --force ---

    #[test]
    fn r06_blocks_force_push() {
        let result = pre_tool::evaluate(&bash_input("git push --force origin main"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r06_blocks_force_push_short_flag() {
        let result = pre_tool::evaluate(&bash_input("git push -f origin feature"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r06_allows_normal_push() {
        let result = pre_tool::evaluate(&bash_input("git push origin feature"));
        assert!(result.permission_decision.is_none());
    }

    // --- R07: secrets in content ---

    #[test]
    fn r07_blocks_openai_key() {
        let result = pre_tool::evaluate(&write_input(
            "/project/config.rs",
            "sk-ant-1234567890abcdef",
        ));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r07_blocks_github_token() {
        let result = pre_tool::evaluate(&write_input("/project/config.rs", "ghp_abc123def456"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r07_blocks_aws_key() {
        let result = pre_tool::evaluate(&write_input("/project/config.rs", "AKIAIOSFODNN7EXAMPLE"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r07_blocks_private_key() {
        let result = pre_tool::evaluate(&write_input(
            "/project/config.rs",
            "-----BEGIN RSA PRIVATE KEY-----",
        ));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r07_allows_normal_content() {
        let result = pre_tool::evaluate(&write_input(
            "/project/src/main.rs",
            "fn main() { println!(\"hello\"); }",
        ));
        assert!(result.permission_decision.is_none());
    }

    // --- R09: path traversal ---

    #[test]
    fn r09_blocks_path_traversal() {
        let result = pre_tool::evaluate(&edit_input("/project/../../../etc/passwd"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r09_allows_normal_path() {
        let result = pre_tool::evaluate(&edit_input("/project/src/main.rs"));
        assert!(result.permission_decision.is_none());
    }

    // --- R10: no-verify ---

    #[test]
    fn r10_blocks_no_verify() {
        let result = pre_tool::evaluate(&bash_input("git commit --no-verify -m 'test'"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    // --- R11: git reset --hard main ---

    #[test]
    fn r11_blocks_hard_reset_main() {
        let result = pre_tool::evaluate(&bash_input("git reset --hard main"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r11_blocks_hard_reset_master() {
        let result = pre_tool::evaluate(&bash_input("git reset --hard master"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r11_allows_hard_reset_branch() {
        let result = pre_tool::evaluate(&bash_input("git reset --hard feature-branch"));
        assert!(result.permission_decision.is_none());
    }

    // --- R12: git push origin main ---

    #[test]
    fn r12_blocks_push_origin_main() {
        let result = pre_tool::evaluate(&bash_input("git push origin main"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r12_blocks_push_origin_master() {
        let result = pre_tool::evaluate(&bash_input("git push origin master"));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    // --- R15: content size ---

    #[test]
    fn r15_blocks_oversized_content() {
        let large_content = "x".repeat(10_000_001);
        let result = pre_tool::evaluate(&write_input("/project/big.txt", &large_content));
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
    }

    #[test]
    fn r15_allows_normal_content_size() {
        let content = "x".repeat(1000);
        let result = pre_tool::evaluate(&write_input("/project/small.txt", &content));
        assert!(result.permission_decision.is_none());
    }

    // --- process_hook integration ---

    #[test]
    fn process_hook_pre_tool_deny() {
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Bash",
            "tool_input": {"command": "sudo rm -rf /"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["permissionDecision"], "deny");
    }

    #[test]
    fn process_hook_pre_tool_allow() {
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("permission_decision").is_none());
    }

    #[test]
    fn process_hook_unknown_type_returns_error() {
        let input = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": {}
        });
        let result = process_hook(&input.to_string(), "unknown-hook");
        assert!(result.is_err());
    }

    // --- codec tests ---

    #[test]
    fn hook_result_allow_serializes() {
        let result = codec::HookResult::allow();
        assert!(result.permission_decision.is_none());
        assert!(result.permission_decision_reason.is_none());
    }

    #[test]
    fn hook_result_deny_serializes() {
        let result = codec::HookResult::deny("test reason");
        assert_eq!(result.permission_decision.as_deref(), Some("deny"));
        assert_eq!(
            result.permission_decision_reason.as_deref(),
            Some("test reason")
        );
    }

    #[test]
    fn hook_result_ask_serializes() {
        let result = codec::HookResult::ask("confirm?");
        assert_eq!(result.permission_decision.as_deref(), Some("ask"));
    }

    #[test]
    fn hook_result_warn_serializes() {
        let result = codec::HookResult::warn("careful!");
        assert!(result.additional_context.is_some());
        assert_eq!(result.additional_context.as_deref(), Some("careful!"));
    }

    #[test]
    fn parse_input_valid_json() {
        let json =
            r#"{"session_id":"s1","tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}"#;
        let input = codec::parse_input(json).unwrap();
        assert_eq!(input.tool_name, "Bash");
        assert_eq!(input.cwd.as_deref(), Some("/tmp"));
    }

    #[test]
    fn parse_input_invalid_json_returns_error() {
        let result = codec::parse_input("not json{{{");
        assert!(result.is_err());
    }

    #[test]
    fn format_result_string_produces_valid_json() {
        let result = codec::HookResult::deny("test");
        let s = codec::format_result_string(&result);
        let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(parsed["permissionDecision"], "deny");
    }

    // --- Read operations should NOT be blocked by write-only rules ---

    #[test]
    fn read_binary_file_allowed() {
        // R08 should only block writing .dll/.exe etc, not reading
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"path": "/project/lib/library.dll"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            parsed.get("permissionDecision").is_none(),
            "Reading a .dll should be allowed"
        );
    }

    #[test]
    fn read_sqlite_file_allowed() {
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"path": "/project/data.sqlite"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("permissionDecision").is_none());
    }

    #[test]
    fn write_binary_file_denied() {
        // R08 should still block writing binary extensions
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Write",
            "tool_input": {"path": "/project/malware.exe"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["permissionDecision"], "deny");
    }

    #[test]
    fn read_with_path_traversal_allowed() {
        // R09 should only block writes with .., not reads
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"path": "../sibling/file.txt"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(
            parsed.get("permissionDecision").is_none(),
            "Reading with .. should be allowed"
        );
    }

    #[test]
    fn read_settings_json_allowed() {
        // R13 should only block writes to config files, not reads
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"path": "/project/.claude/settings.json"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("permissionDecision").is_none());
    }

    #[test]
    fn read_outside_cwd_allowed() {
        // R04 should only block writes outside cwd, not reads
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"path": "/usr/local/config.yml"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("permissionDecision").is_none());
    }

    #[test]
    fn write_outside_cwd_denied() {
        // R04 should still block writes outside cwd
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Write",
            "tool_input": {"path": "/etc/hosts"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["permissionDecision"], "deny");
    }

    #[test]
    fn read_with_secrets_in_path_allowed() {
        // R07 (secrets) only applies to write content, not reads
        let input = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Grep",
            "tool_input": {"pattern": "test"},
            "cwd": "/project",
            "hook_event_name": "PreToolUse"
        });
        let result = process_hook(&input.to_string(), "pre-tool").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("permissionDecision").is_none());
    }
}
