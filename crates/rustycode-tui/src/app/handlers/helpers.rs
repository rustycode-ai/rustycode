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
        tui.auto_continue.auto_continue_iterations = 0;
    }

    // Enforce iteration limit to prevent infinite loops
    if tui.auto_continue.auto_continue_iterations >= MAX_AUTO_CONTINUE_ITERATIONS {
        tracing::warn!(
            "Auto-continue stopped after {} iterations (task creation may be outpacing completion)",
            MAX_AUTO_CONTINUE_ITERATIONS
        );
        tui.add_system_message(format!(
            "Auto-continue stopped after {} iterations. Press Ctrl+Shift+A to resume if needed.",
            MAX_AUTO_CONTINUE_ITERATIONS
        ));
        tui.auto_continue.auto_continue_enabled = false;
        tui.auto_continue.auto_continue_iterations = 0;
        return;
    }

    // Stagnation check: if we've had multiple consecutive iterations with no
    // tool use, the agent is likely stuck in a text-only loop. Stop and inform.
    if !last_stream_had_tools
        && tui.auto_continue.auto_continue_iterations >= MAX_STAGNANT_ITERATIONS
    {
        tracing::warn!(
            "Auto-continue stopped: {} consecutive iterations with no tool use",
            MAX_STAGNANT_ITERATIONS
        );
        tui.add_system_message(
            "Agent appears stuck (no tool calls for several turns). \
             Press Enter to continue manually if needed."
                .to_string(),
        );
        tui.auto_continue.auto_continue_enabled = false;
        tui.auto_continue.auto_continue_iterations = 0;
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
        .filter(|t| !t.done)
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

    tui.auto_continue.auto_continue_pending = true;
    tui.auto_continue.auto_continue_iterations += 1;
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
        tui.auto_continue.auto_continue_pending = false;
        tui.auto_continue.auto_continue_enabled = false;
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
