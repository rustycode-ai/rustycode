//! Environment variable compatibility layer for hooks
//!
//! Exposes standard env vars to hook commands, compatible with
//! Claude Code and Codex hook conventions.

use std::collections::HashMap;
use std::path::Path;

use super::protocol::HookEvent;

/// Build environment variables for a hook command.
///
/// Includes both RustyCode-native vars and compatibility aliases
/// for Claude Code and Codex hook scripts.
pub fn hook_env(
    event: HookEvent,
    tool_name: &str,
    session_id: &str,
    cwd: &Path,
) -> HashMap<String, String> {
    let mut env = HashMap::new();

    // RustyCode-native vars
    env.insert("TOOL_NAME".into(), tool_name.into());
    env.insert("SESSION_ID".into(), session_id.into());
    env.insert("HOOK_EVENT".into(), event.to_string());
    env.insert("CWD".into(), cwd.to_string_lossy().into());

    // Claude Code compat
    env.insert("CLAUDE_CODE_SESSION_ID".into(), session_id.into());

    // Codex compat
    env.insert("CODEX_SESSION_ID".into(), session_id.into());

    env
}

/// Build environment variables for a non-tool hook event
/// (UserPromptSubmit, Stop, SessionStart).
pub fn hook_env_no_tool(event: HookEvent, session_id: &str, cwd: &Path) -> HashMap<String, String> {
    let mut env = HashMap::new();

    env.insert("SESSION_ID".into(), session_id.into());
    env.insert("HOOK_EVENT".into(), event.to_string());
    env.insert("CWD".into(), cwd.to_string_lossy().into());
    env.insert("CLAUDE_CODE_SESSION_ID".into(), session_id.into());
    env.insert("CODEX_SESSION_ID".into(), session_id.into());

    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_env_has_all_vars() {
        let env = hook_env(
            HookEvent::PreToolUse,
            "Bash",
            "sess-123",
            Path::new("/project"),
        );

        assert_eq!(env.get("TOOL_NAME").unwrap(), "Bash");
        assert_eq!(env.get("SESSION_ID").unwrap(), "sess-123");
        assert_eq!(env.get("HOOK_EVENT").unwrap(), "PreToolUse");
        assert_eq!(env.get("CWD").unwrap(), "/project");
        assert_eq!(env.get("CLAUDE_CODE_SESSION_ID").unwrap(), "sess-123");
        assert_eq!(env.get("CODEX_SESSION_ID").unwrap(), "sess-123");
    }

    #[test]
    fn no_tool_env_excludes_tool_name() {
        let env = hook_env_no_tool(HookEvent::SessionStart, "sess-456", Path::new("/home"));

        assert!(!env.contains_key("TOOL_NAME"));
        assert_eq!(env.get("SESSION_ID").unwrap(), "sess-456");
        assert_eq!(env.get("HOOK_EVENT").unwrap(), "SessionStart");
    }

    #[test]
    fn compat_aliases_match_session() {
        let env = hook_env(HookEvent::PostToolUse, "Edit", "abc", Path::new("/tmp"));

        let cc_id = env.get("CLAUDE_CODE_SESSION_ID").unwrap();
        let codex_id = env.get("CODEX_SESSION_ID").unwrap();
        let native_id = env.get("SESSION_ID").unwrap();
        assert_eq!(cc_id, native_id);
        assert_eq!(codex_id, native_id);
    }
}
