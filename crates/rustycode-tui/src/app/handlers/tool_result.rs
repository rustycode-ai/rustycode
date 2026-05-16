//! Tool execution result handler.
//!
//! Processes completed tool results, updates message tool executions,
//! manages AST phase state, and surfaces ask_user responses.

use crate::app::async_::{ToolOutput, ToolResult};
use crate::app::TUI;
use crate::ui::ast_progress::AST_PHASE_NAMES;
use crate::ui::message::{ToolExecution, ToolStatus};
use chrono;
use rustycode_protocol::EventMsg;
use std::time::SystemTime;
use tracing;

pub fn handle_tool_result(tui: &mut TUI, result: ToolResult) {
    tracing::debug!("Tool result: {} ({:?})", result.name, result.result);

    let result_status = match &result.result {
        ToolOutput::Success(_) => ToolStatus::Complete,
        ToolOutput::Error(_) => ToolStatus::Failed,
        ToolOutput::Timeout => ToolStatus::Failed,
    };
    let raw_output: Option<String> = match &result.result {
        ToolOutput::Success(s) => Some(s.clone()),
        ToolOutput::Error(e) => Some(e.to_string()),
        ToolOutput::Timeout => Some("Operation timed out".to_string()),
    };

    let detailed_output = raw_output.map(|raw| format_detailed_output(&result, raw));

    // Save copies for tool panel history before moving into message
    let panel_detailed_output = detailed_output.clone();
    let panel_status = result_status.clone();

    // Compute a smart summary using output_summary for better display
    let result_summary = compute_result_summary(&result);

    // Pre-extract input_json from messages (immutable borrow) BEFORE any
    // mutable borrow of tui.session.messages. ToolComplete may have already removed
    // the entry from active_tools, so we fall back to the message history.
    let fallback_input_json = tui
        .session
        .active_tools
        .get(&result.id)
        .and_then(|t| t.input_json.clone())
        .or_else(|| {
            tui.session.messages.iter().rev().find_map(|m| {
                m.tool_executions
                    .as_ref()?
                    .iter()
                    .rev()
                    .find(|t| t.tool_id == result.id)
                    .and_then(|t| t.input_json.clone())
            })
        });
    let fallback_start_time = tui
        .session
        .active_tools
        .get(&result.id)
        .map(|t| t.start_time)
        .unwrap_or_else(chrono::Utc::now);

    update_message_tool_execution(
        tui,
        &result,
        result_status,
        result_summary.clone(),
        detailed_output,
        fallback_input_json,
        fallback_start_time,
    );

    // Remove from active tools
    tui.session.active_tools.remove(&result.id);

    update_tool_panel_history(
        tui,
        &result,
        panel_status,
        result_summary.clone(),
        panel_detailed_output,
    );

    update_ast_phase_state(tui, &result);

    // Surface ask_user tool results to the user
    if result.name == "ask_user" {
        if let ToolOutput::Success(json_str) = &result.result {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                if parsed["status"].as_str() == Some("clarification_requested") {
                    let question = parsed["question"]
                        .as_str()
                        .unwrap_or("LLM needs clarification");
                    let urgency = parsed["urgency"].as_str().unwrap_or("medium");
                    tui.add_system_message(format!("[{urgency}] LLM asks: {question}"));
                }
            }
        }
    }

    tui.sys.dirty.set(crate::app::state_model::DirtyFlags::ALL);

    // Emit EventMsg for the unified event channel
    let (success, output) = match &result.result {
        ToolOutput::Success(s) => (true, s.clone()),
        ToolOutput::Error(e) => (false, e.to_string()),
        ToolOutput::Timeout => (false, "Operation timed out".to_string()),
    };
    let output_size = output.len();
    tui.integration
        .services
        .send_event(EventMsg::ToolExecCompleted {
            tool_id: result.id.clone(),
            tool_name: result.name.clone(),
            success,
            output,
            output_size,
            duration_ms: 0, // duration looked up from active_tools in handler above
            exit_code: None,
        });
}

/// Truncate large tool outputs with temp file fallback for inspection.
/// Strips ANSI escape codes so terminal colors don't render as garbage in the TUI.
fn format_detailed_output(result: &ToolResult, output: String) -> String {
    let output = crate::app::tool_output_format::strip_ansi_escapes(&output);
    const MAX_INLINE_CHARS: usize = 4000;
    if output.chars().count() <= MAX_INLINE_CHARS {
        return output;
    }

    let truncated_lines: Vec<&str> = output.lines().take(21).collect();
    let has_more = truncated_lines.len() > 20;
    let truncated_lines = &truncated_lines[..20.min(truncated_lines.len())];
    let mut truncated = truncated_lines.join("\n");

    // Save full output to temp file
    let filename = format!(
        "rustycode-tool-{}-{}.txt",
        result.name,
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );
    let path = std::env::temp_dir().join(&filename);
    if std::fs::write(&path, &output).is_ok() {
        if has_more {
            truncated.push_str(&format!(
                "\n\n... (more lines truncated. Full output: {})",
                path.display()
            ));
        }
    } else if has_more {
        truncated.push_str(&format!(
            "\n\n... (more lines truncated, {} chars total)",
            output.chars().count()
        ));
    }
    truncated
}

/// Compute a smart summary using output_summary for better display.
/// Strips ANSI from summary so status lines are clean.
fn compute_result_summary(result: &ToolResult) -> String {
    match &result.result {
        ToolOutput::Success(s) => {
            let clean = crate::app::tool_output_format::strip_ansi_escapes(s);
            crate::app::tool_output_format::output_summary(&clean)
        }
        ToolOutput::Error(e) => {
            let msg = e.display_message();
            let clean = crate::app::tool_output_format::strip_ansi_escapes(msg);
            format!("Error: {}", clean)
        }
        ToolOutput::Timeout => "Timeout".to_string(),
    }
}

/// Update or create the ToolExecution entry in the last assistant message.
fn update_message_tool_execution(
    tui: &mut TUI,
    result: &ToolResult,
    result_status: ToolStatus,
    result_summary: String,
    detailed_output: Option<String>,
    fallback_input_json: Option<serde_json::Value>,
    fallback_start_time: chrono::DateTime<chrono::Utc>,
) {
    let assistant_msg = tui.last_assistant_message_mut();
    let Some(last_msg) = assistant_msg else {
        return;
    };

    let updated_existing = if let Some(tools) = &mut last_msg.tool_executions {
        if let Some(tool) = tools.iter_mut().find(|t| t.tool_id == result.id) {
            tool.status = result_status.clone();
            let end_time = chrono::Utc::now();
            tool.end_time = Some(end_time);
            tool.duration_ms = Some(
                end_time
                    .signed_duration_since(tool.start_time)
                    .num_milliseconds()
                    .max(0) as u64,
            );
            tool.result_summary = result_summary.clone();
            tool.detailed_output = detailed_output.clone();
            true
        } else {
            false
        }
    } else {
        false
    };

    if !updated_existing {
        let start_time = fallback_start_time;
        let end_time = chrono::Utc::now();
        let tool_execution = ToolExecution {
            tool_id: result.id.clone(),
            name: result.name.clone(),
            start_time,
            end_time: Some(end_time),
            duration_ms: Some(
                end_time
                    .signed_duration_since(start_time)
                    .num_milliseconds()
                    .max(0) as u64,
            ),
            result_summary,
            status: result_status,
            detailed_output,
            input_json: fallback_input_json.clone(),
            input_json_pretty: fallback_input_json
                .as_ref()
                .and_then(|v| serde_json::to_string_pretty(v).ok()),
            progress_current: None,
            progress_total: None,
            progress_description: None,
        };

        if last_msg.tool_executions.is_none() {
            last_msg.tool_executions = Some(vec![]);
        }
        if let Some(tools) = &mut last_msg.tool_executions {
            tools.push(tool_execution);
            while tools.len() > 100 {
                tools.remove(0);
            }
        }
    }

    if !tui.ui.view.user_scrolled {
        tui.auto_scroll();
    }
}

/// Update the running entry in tool panel history (added by ToolStart).
fn update_tool_panel_history(
    tui: &mut TUI,
    result: &ToolResult,
    panel_status: ToolStatus,
    result_summary: String,
    panel_detailed_output: Option<String>,
) {
    // Look up duration from the message's tool execution that was just updated
    let panel_duration = tui
        .session
        .messages
        .last()
        .and_then(|m| m.tool_executions.as_ref())
        .and_then(|tools| tools.iter().rev().find(|t| t.tool_id == result.id))
        .and_then(|t| t.duration_ms);

    // Find the running entry for this tool and update it in-place
    let updated_existing = tui
        .panels
        .tool_panel
        .tool_panel_history
        .iter_mut()
        .rev()
        .find(|entry| entry.tool_id == result.id && entry.status == ToolStatus::Running)
        .map(|entry| {
            entry.status = panel_status.clone();
            entry.end_time = Some(chrono::Utc::now());
            entry.duration_ms = panel_duration;
            entry.result_summary = result_summary.clone();
            entry.detailed_output = panel_detailed_output.clone();
        })
        .is_some();

    // If no running entry was found (edge case), add a new one
    if !updated_existing {
        let tool_entry = ToolExecution {
            tool_id: result.id.clone(),
            name: result.name.clone(),
            start_time: chrono::Utc::now(),
            end_time: Some(chrono::Utc::now()),
            duration_ms: panel_duration,
            result_summary,
            status: panel_status,
            detailed_output: panel_detailed_output,
            input_json: None,
            input_json_pretty: None,
            progress_current: None,
            progress_total: None,
            progress_description: None,
        };
        tui.panels.tool_panel.tool_panel_history.push(tool_entry);
        if tui.panels.tool_panel.tool_panel_history.len() > 50 {
            tui.panels.tool_panel.tool_panel_history.remove(0);
        }
    }
}

/// Update AST phase state from structured_thinking tool results.
fn update_ast_phase_state(tui: &mut TUI, result: &ToolResult) {
    if result.name != "structured_thinking" {
        return;
    }

    match &result.result {
        ToolOutput::Success(json_str) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                let phase_num = parsed["phase"].as_u64().unwrap_or(1).saturating_sub(1) as usize;
                let phase_num = phase_num.min(AST_PHASE_NAMES.len() - 1);
                let phase_name = AST_PHASE_NAMES[phase_num];
                let next_needed = parsed["next_thought_needed"].as_bool().unwrap_or(true);
                let confidence = parsed["confidence"].as_u64().unwrap_or(0) as usize;

                if !tui.panels.ast_phase_state.is_active() {
                    tui.panels.ast_phase_state.activate(
                        phase_name,
                        phase_num,
                        "Structured thinking",
                    );
                    tui.terminal_progress.set_progress(0);
                } else {
                    tui.panels.ast_phase_state.phase = phase_name.to_string();
                    tui.panels.ast_phase_state.phase_index = phase_num;
                }
                // Use confidence as progress indicator (0-100 maps to phase completion)
                tui.panels
                    .ast_phase_state
                    .update_milestones(confidence, 100);

                if tui.terminal_progress.enabled {
                    let fraction = tui.panels.ast_phase_state.progress_fraction();
                    tui.terminal_progress.set_progress((fraction * 100.0) as u8);
                }

                if !next_needed {
                    tui.panels.ast_phase_state.complete();
                    tui.terminal_progress.set_progress(100);
                }

                // Surface loop warning as a system message
                if parsed
                    .get("loop_warning")
                    .and_then(|w| w.get("detected"))
                    .and_then(|d| d.as_bool())
                    .unwrap_or(false)
                {
                    let suggestion = parsed["loop_warning"]["suggestion"]
                        .as_str()
                        .unwrap_or("Thinking loop detected — consider using ask_user");
                    tui.add_system_message(suggestion.to_string());
                }
            }
        }
        ToolOutput::Error(_) | ToolOutput::Timeout => {
            tui.panels.ast_phase_state.deactivate();
            tui.terminal_progress.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::async_::{ToolExecutionError, ToolOutput, ToolResult};

    // --- compute_result_summary ---

    #[test]
    fn test_compute_result_summary_success_output() {
        let result = ToolResult {
            id: "id-1".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success("Build successful".to_string()),
        };
        let summary = compute_result_summary(&result);
        assert_eq!(summary, "Build successful");
    }

    #[test]
    fn test_compute_result_summary_success_empty() {
        let result = ToolResult {
            id: "id-2".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success(String::new()),
        };
        let summary = compute_result_summary(&result);
        assert_eq!(summary, "no output");
    }

    #[test]
    fn test_compute_result_summary_error() {
        let result = ToolResult {
            id: "id-3".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Error(ToolExecutionError::ExecutionFailed {
                tool: "Bash".to_string(),
                output: "command not found".to_string(),
            }),
        };
        let summary = compute_result_summary(&result);
        assert!(summary.starts_with("Error:"));
        assert!(summary.contains("command not found"));
    }

    #[test]
    fn test_compute_result_summary_timeout() {
        let result = ToolResult {
            id: "id-4".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Timeout,
        };
        let summary = compute_result_summary(&result);
        assert_eq!(summary, "Timeout");
    }

    #[test]
    fn test_compute_result_summary_permission_denied() {
        let result = ToolResult {
            id: "id-5".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Error(ToolExecutionError::PermissionDenied {
                tool: "Bash".to_string(),
                reason: "blocked by policy".to_string(),
            }),
        };
        let summary = compute_result_summary(&result);
        assert!(summary.contains("blocked by policy"));
    }

    #[test]
    fn test_compute_result_summary_strips_ansi_from_success() {
        let result = ToolResult {
            id: "id-6".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success("\x1b[32mOK\x1b[0m".to_string()),
        };
        let summary = compute_result_summary(&result);
        assert_eq!(summary, "OK");
    }

    // --- format_detailed_output ---

    #[test]
    fn test_format_detailed_output_short_output_unchanged() {
        let result = ToolResult {
            id: "id-10".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success("short output".to_string()),
        };
        let output = format_detailed_output(&result, "short output".to_string());
        assert_eq!(output, "short output");
    }

    #[test]
    fn test_format_detailed_output_strips_ansi() {
        let result = ToolResult {
            id: "id-11".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success("colored".to_string()),
        };
        let output = format_detailed_output(&result, "\x1b[31mcolored\x1b[0m text".to_string());
        assert!(!output.contains("\x1b"));
        assert!(output.contains("colored text"));
    }

    #[test]
    fn test_format_detailed_output_truncates_large_output() {
        let result = ToolResult {
            id: "id-12".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success(String::new()),
        };
        // Generate output exceeding MAX_INLINE_CHARS (4000 chars) AND more than 20 lines
        // so the truncation indicator is triggered
        let long_line = "x".repeat(300);
        let long_output: String = (0..25)
            .map(|i| format!("line {}: {}", i, long_line))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(long_output.chars().count() > 4000);

        let output = format_detailed_output(&result, long_output);
        // Should contain a truncation indicator (temp file path or char count)
        assert!(output.contains("truncated"));
        // The output should contain the first few lines (not completely empty)
        assert!(output.contains("line 0:"));
        // Should NOT contain lines beyond the first 20
        assert!(!output.contains("line 21:"));
    }

    #[test]
    fn test_format_detailed_output_under_limit_unchanged() {
        let result = ToolResult {
            id: "id-13".to_string(),
            name: "Read".to_string(),
            result: ToolOutput::Success(String::new()),
        };
        let input = "line 1\nline 2\nline 3".to_string();
        assert!(input.chars().count() <= 4000);
        let output = format_detailed_output(&result, input.clone());
        assert_eq!(output, input);
    }
}
