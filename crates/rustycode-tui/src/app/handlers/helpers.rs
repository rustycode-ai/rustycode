//! Shared helper functions for stream handlers.

use crate::app::TUI;
use tracing;

/// Shared cleanup after a stream ends normally (done or stopped).
/// Captures duration, completes the query, resets rate limit, and clears active tools.
pub(super) fn complete_stream_cleanup(tui: &mut TUI) {
    tui.streaming.is_streaming = false;
    tui.streaming.stream_cancelled = false;
    tui.update_terminal_title();
    if let Some(start) = tui.streaming.stream_start_time.take() {
        tui.streaming.last_response_duration = Some(start.elapsed());
    }
    tui.services.complete_query();
    tui.rate_limit.retry_count = 0;
    tui.rate_limit.auto_retry_cancelled = false;
    tui.active_tools.clear();
}

/// Reset the streaming render buffer and chunk counters.
pub(super) fn reset_streaming_buffer(tui: &mut TUI) {
    tui.streaming.streaming_render_buffer =
        crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
    tui.streaming.chunks_received = 0;
    tui.streaming.thinking_chunks_received = 0;
}

/// Mark the TUI as dirty and auto-scroll if the user hasn't manually scrolled.
pub(super) fn mark_dirty_and_scroll(tui: &mut TUI) {
    tui.dirty = true;
    if !tui.view.user_scrolled {
        tui.auto_scroll();
    }
}

/// Check for pending tasks and trigger auto-continue if needed
///
/// This function is called after stream completion when auto-continue is enabled.
/// It checks if there are pending or in-progress tasks, and if so, automatically
/// sends a continuation message to keep the AI working.
///
/// Safety: capped at MAX_AUTO_CONTINUE_ITERATIONS (100) to prevent infinite loops
/// if the AI keeps creating new tasks faster than it completes them.
pub(super) fn check_and_trigger_auto_continue(tui: &mut TUI) {
    use crate::app::tasks::TaskStatus;

    const MAX_AUTO_CONTINUE_ITERATIONS: usize = 100;
    const MAX_STAGNANT_ITERATIONS: usize = 5;

    // Check if the last stream was productive (had tool executions).
    // Productive streams reset the stagnation counter, allowing effectively
    // unlimited continuation as long as the agent keeps doing useful work.
    let last_stream_had_tools = tui.last_assistant_has_tools();

    if last_stream_had_tools {
        // Productive turn — reset iteration counter so the agent can keep going
        tui.auto_continue.reset_iterations();
    }

    // Enforce iteration limit to prevent infinite loops
    if tui.auto_continue.iterations() >= MAX_AUTO_CONTINUE_ITERATIONS {
        tracing::warn!(
            "Auto-continue stopped after {} iterations (task creation may be outpacing completion)",
            MAX_AUTO_CONTINUE_ITERATIONS
        );
        tui.add_system_message(format!(
            "Auto-continue stopped after {} iterations. Press Ctrl+Shift+A to resume if needed.",
            MAX_AUTO_CONTINUE_ITERATIONS
        ));
        tui.auto_continue.disable();
        return;
    }

    // Stagnation check: if we've had multiple consecutive iterations with no
    // tool use, the agent is likely stuck in a text-only loop. Stop and inform.
    if !last_stream_had_tools && tui.auto_continue.iterations() >= MAX_STAGNANT_ITERATIONS {
        tracing::warn!(
            "Auto-continue stopped: {} consecutive iterations with no tool use",
            MAX_STAGNANT_ITERATIONS
        );
        tui.add_system_message(
            "Agent appears stuck (no tool calls for several turns). \
             Press Enter to continue manually if needed."
                .to_string(),
        );
        tui.auto_continue.disable();
        return;
    }

    // Check for pending or in-progress tasks
    let pending_tasks: Vec<_> = tui
        .workspace_tasks
        .tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Pending || t.status == TaskStatus::InProgress)
        .collect();

    // Check for incomplete todos
    let incomplete_todos: Vec<_> = tui
        .workspace_tasks
        .todos
        .iter()
        .filter(|t| {
            !matches!(
                t.status,
                crate::app::tasks::TodoStatus::Completed | crate::app::tasks::TodoStatus::Cancelled
            )
        })
        .collect();

    // Build the continuation context and send
    let context = if pending_tasks.is_empty() && incomplete_todos.is_empty() {
        tracing::debug!("Auto-continue: No pending tasks/todos, sending generic continue");
        "Continue where you left off. Keep working on the task.".to_string()
    } else {
        let total_pending = pending_tasks.len() + incomplete_todos.len();
        tracing::info!(
            "Auto-continue: {} tasks/todos remaining, continuing work",
            total_pending
        );
        let mut ctx = String::from("Continue working on the remaining tasks:\n\n");
        for task in &pending_tasks {
            ctx.push_str(&format!("- [ ] {}\n", task.description));
        }
        for todo in &incomplete_todos {
            ctx.push_str(&format!("- [ ] {}\n", todo.text));
        }
        ctx.push_str("\nPlease continue with the next task. Use tools to complete the work.");
        ctx
    };

    tui.auto_continue.mark_pending();
    tui.auto_continue.increment_iterations();
    let history = tui.build_conversation_history();

    // Set streaming state before send to prevent races
    tui.streaming.begin_streaming();

    if let Err(e) = tui
        .services
        .send_message_with_history(context, Some(history), None)
    {
        tracing::error!("Failed to send auto-continue message: {}", e);
        tui.add_system_message(format!(
            "Auto-continue failed: {}. Auto-continue disabled. Press Enter to continue manually.",
            e
        ));
        tui.reset_streaming_state();
        tui.active_tools.clear();
        tui.auto_continue.clear_pending();
        tui.auto_continue.disable();
    } else {
        tui.push_empty_assistant_message();
    }
}

const TOOL_SUMMARY_MAX_LEN: usize = 60;
const TOOL_SUMMARY_TRUNCATE_AT: usize = 57;

pub(super) fn build_tool_summary_arg(
    tool_name: &str,
    input_json: &serde_json::Value,
) -> Option<String> {
    let lower = tool_name.to_lowercase();
    if lower.contains("bash") || lower.contains("exec") || lower.contains("shell") {
        return input_json.get("command").and_then(|v| v.as_str()).map(|s| {
            if s.len() > TOOL_SUMMARY_MAX_LEN {
                format!("{}…", &s[..s.floor_char_boundary(TOOL_SUMMARY_TRUNCATE_AT)])
            } else {
                s.to_string()
            }
        });
    }
    if lower.contains("read")
        || lower.contains("cat")
        || lower.contains("view")
        || lower.contains("write")
        || lower.contains("create")
        || lower.contains("edit")
        || lower.contains("patch")
        || lower.contains("replace")
    {
        return input_json
            .get("path")
            .or_else(|| input_json.get("file_path"))
            .or_else(|| input_json.get("file"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    if lower.contains("grep") || lower.contains("search") {
        return input_json
            .get("pattern")
            .or_else(|| input_json.get("query"))
            .and_then(|v| v.as_str())
            .map(|s| {
                format!(
                    "\"{}\"",
                    if s.len() > 50 {
                        &s[..s.floor_char_boundary(47)]
                    } else {
                        s
                    }
                )
            });
    }
    if lower.contains("glob") || lower.contains("find") || lower.contains("list") {
        return input_json
            .get("pattern")
            .or_else(|| input_json.get("path"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }
    // Agent tool: show subagent_type and description/prompt excerpt
    if lower == "agent" {
        let agent_type = input_json
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let desc = input_json
            .get("description")
            .and_then(|v| v.as_str())
            .or_else(|| {
                input_json.get("prompt").and_then(|v| {
                    let s = v.as_str()?;
                    let first_line = s.split('\n').next().unwrap_or(s);
                    Some(if first_line.len() > TOOL_SUMMARY_MAX_LEN {
                        &first_line[..first_line.floor_char_boundary(TOOL_SUMMARY_TRUNCATE_AT)]
                    } else {
                        first_line
                    })
                })
            })
            .unwrap_or("no description");
        return Some(format!("{}: {}", agent_type, desc));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tool_summary_bash_command() {
        let json = serde_json::json!({"command": "cargo build --release"});
        let result = build_tool_summary_arg("Bash", &json);
        assert_eq!(result, Some("cargo build --release".to_string()));
    }

    #[test]
    fn test_build_tool_summary_bash_truncation() {
        let long_cmd = "x".repeat(80);
        let json = serde_json::json!({"command": long_cmd});
        let result = build_tool_summary_arg("Bash", &json);
        assert!(result.is_some());
        assert!(result.as_ref().unwrap().contains('…'));
        assert!(result.as_ref().unwrap().len() <= TOOL_SUMMARY_MAX_LEN + 1);
    }

    #[test]
    fn test_build_tool_summary_read_file() {
        let json = serde_json::json!({"path": "/src/main.rs"});
        let result = build_tool_summary_arg("Read", &json);
        assert_eq!(result, Some("/src/main.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_edit_file_path() {
        let json = serde_json::json!({"file_path": "/src/lib.rs"});
        let result = build_tool_summary_arg("Edit", &json);
        assert_eq!(result, Some("/src/lib.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_grep_pattern() {
        let json = serde_json::json!({"pattern": "fn main"});
        let result = build_tool_summary_arg("Grep", &json);
        assert_eq!(result, Some("\"fn main\"".to_string()));
    }

    #[test]
    fn test_build_tool_summary_search_query() {
        let json = serde_json::json!({"query": "TODO"});
        let result = build_tool_summary_arg("Search", &json);
        assert_eq!(result, Some("\"TODO\"".to_string()));
    }

    #[test]
    fn test_build_tool_summary_glob_pattern() {
        let json = serde_json::json!({"pattern": "**/*.rs"});
        let result = build_tool_summary_arg("Glob", &json);
        assert_eq!(result, Some("**/*.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_agent_tool() {
        let json = serde_json::json!({
            "subagent_type": "executor",
            "description": "Fix the build error"
        });
        let result = build_tool_summary_arg("agent", &json);
        assert_eq!(result, Some("executor: Fix the build error".to_string()));
    }

    #[test]
    fn test_build_tool_summary_agent_fallback_prompt() {
        let json = serde_json::json!({
            "subagent_type": "planner",
            "prompt": "Plan the feature implementation\nStep 1: Research\nStep 2: Design"
        });
        let result = build_tool_summary_arg("agent", &json);
        assert!(result.is_some());
        assert!(result.as_ref().unwrap().contains("planner:"));
        assert!(result.as_ref().unwrap().contains("Plan the feature"));
    }

    #[test]
    fn test_build_tool_summary_unknown_tool() {
        let json = serde_json::json!({"data": "value"});
        let result = build_tool_summary_arg("unknown_tool", &json);
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_tool_summary_case_insensitive() {
        let json = serde_json::json!({"command": "ls -la"});
        let result = build_tool_summary_arg("Bash", &json);
        assert_eq!(result, Some("ls -la".to_string()));
    }

    #[test]
    fn test_build_tool_summary_missing_field() {
        let json = serde_json::json!({"other": "value"});
        let result = build_tool_summary_arg("Bash", &json);
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_tool_summary_bash_exact_at_max_len() {
        // Command exactly at TOOL_SUMMARY_MAX_LEN (60 chars) should NOT be truncated
        let cmd = "x".repeat(60);
        let json = serde_json::json!({"command": cmd});
        let result = build_tool_summary_arg("Bash", &json);
        assert!(result.is_some());
        assert!(!result.as_ref().unwrap().contains('…'));
        assert_eq!(result.unwrap().len(), 60);
    }

    #[test]
    fn test_build_tool_summary_bash_one_over_max_len() {
        // Command one over TOOL_SUMMARY_MAX_LEN should be truncated
        let cmd = "x".repeat(61);
        let json = serde_json::json!({"command": cmd});
        let result = build_tool_summary_arg("Bash", &json);
        assert!(result.is_some());
        assert!(result.as_ref().unwrap().contains('…'));
    }

    #[test]
    fn test_build_tool_summary_grep_long_pattern_truncated() {
        let pattern = "a".repeat(60);
        let json = serde_json::json!({"pattern": pattern});
        let result = build_tool_summary_arg("Grep", &json);
        assert!(result.is_some());
        let inner = result.unwrap();
        assert!(inner.starts_with('"'));
        assert!(inner.ends_with('"'));
        // The quoted string should be shorter than the raw 60-char pattern
        assert!(inner.len() < 60 + 2); // 60 chars + 2 quotes
    }

    #[test]
    fn test_build_tool_summary_edit_prefers_path_over_file() {
        let json = serde_json::json!({"path": "/a.rs", "file": "/b.rs"});
        let result = build_tool_summary_arg("edit", &json);
        assert_eq!(result, Some("/a.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_edit_falls_back_to_file() {
        let json = serde_json::json!({"file": "/fallback.rs"});
        let result = build_tool_summary_arg("edit", &json);
        assert_eq!(result, Some("/fallback.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_agent_no_description_no_prompt() {
        let json = serde_json::json!({"subagent_type": "executor"});
        let result = build_tool_summary_arg("agent", &json);
        assert!(result.is_some());
        assert!(result.as_ref().unwrap().contains("executor:"));
        assert!(result.as_ref().unwrap().contains("no description"));
    }

    #[test]
    fn test_build_tool_summary_agent_long_description_not_truncated() {
        // Agent tool does NOT truncate the "description" field (only truncates "prompt")
        let desc = "d".repeat(70);
        let json = serde_json::json!({
            "subagent_type": "planner",
            "description": desc,
        });
        let result = build_tool_summary_arg("agent", &json);
        assert!(result.is_some());
        let inner = result.unwrap();
        assert!(inner.starts_with("planner:"));
        // Description is used as-is (not truncated)
        assert_eq!(inner, format!("planner: {}", "d".repeat(70)));
    }

    #[test]
    fn test_build_tool_summary_agent_long_prompt_truncated() {
        // Agent tool truncates the "prompt" field at TOOL_SUMMARY_TRUNCATE_AT
        let long_prompt = "p".repeat(70);
        let json = serde_json::json!({
            "subagent_type": "executor",
            "prompt": long_prompt,
        });
        let result = build_tool_summary_arg("agent", &json);
        assert!(result.is_some());
        let inner = result.unwrap();
        assert!(inner.starts_with("executor:"));
        // Truncated to 57 chars + prefix, should be shorter than untruncated
        assert!(inner.len() < "executor: ".len() + 70);
    }

    #[test]
    fn test_build_tool_summary_null_json() {
        let result = build_tool_summary_arg("Bash", &serde_json::Value::Null);
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_tool_summary_glob_prefers_pattern() {
        let json = serde_json::json!({"pattern": "*.rs", "path": "/src"});
        let result = build_tool_summary_arg("Glob", &json);
        assert_eq!(result, Some("*.rs".to_string()));
    }

    #[test]
    fn test_build_tool_summary_find_falls_back_to_path() {
        let json = serde_json::json!({"path": "/workspace"});
        let result = build_tool_summary_arg("Find", &json);
        assert_eq!(result, Some("/workspace".to_string()));
    }
}
