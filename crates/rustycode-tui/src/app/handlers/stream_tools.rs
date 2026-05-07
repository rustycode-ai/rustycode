//! Tool execution stream handlers — start, progress, and completion.
//!
//! These handle the tool lifecycle during streaming: creating ToolExecution entries,
//! updating progress, recording results, doom loop detection, and hook execution.

use crate::app::TUI;
use crate::ui::message::{ToolExecution, ToolStatus};
use chrono;
use tracing;

use super::helpers::build_tool_summary_arg;

const REASONING_TOOLS: &[&str] = &[
    "reasoning_decompose",
    "reasoning_research",
    "reasoning_validate",
    "reasoning_integrate",
];

fn is_reasoning_tool(name: &str) -> bool {
    REASONING_TOOLS.contains(&name)
}

fn new_running_tool(
    tool_id: String,
    tool_name: String,
    input_json: Option<serde_json::Value>,
    result_summary: String,
) -> ToolExecution {
    ToolExecution {
        tool_id,
        name: tool_name,
        status: ToolStatus::Running,
        start_time: chrono::Utc::now(),
        end_time: None,
        duration_ms: None,
        result_summary,
        detailed_output: None,
        input_json,
        progress_current: None,
        progress_total: None,
        progress_description: None,
    }
}

fn spawn_hook(
    hooks_dir: std::path::PathBuf,
    trigger: rustycode_tools::hooks::HookTrigger,
    ctx: serde_json::Value,
) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create runtime for hook execution: {}", e);
                return;
            }
        };
        let hm = rustycode_tools::hooks::HookManager::new(
            hooks_dir,
            rustycode_tools::hooks::HookProfile::Standard,
            String::new(),
        );
        if let Err(e) = rt.block_on(hm.execute(trigger, ctx)) {
            tracing::warn!("{:?} hook execution failed: {}", trigger, e);
        }
    });
}

pub(super) fn handle_tool_start_chunk(
    tui: &mut TUI,
    tool_name: String,
    tool_id: String,
    input_json_str: String,
) {
    tracing::debug!(
        "ToolStart: name={} id={} input_len={}",
        tool_name,
        tool_id,
        input_json_str.len()
    );
    let input_json: Option<serde_json::Value> = if input_json_str.is_empty() {
        None
    } else {
        serde_json::from_str(&input_json_str).ok()
    };
    tracing::debug!(
        "ToolStart parsed: name={} input_json={}",
        tool_name,
        input_json.is_some()
    );

    let plan_blocked = match tui.plan_mode.is_tool_allowed(&tool_name) {
        Ok(()) => false,
        Err(reason) => {
            const DOC_EXTENSIONS: &[&str] = &[".md", ".txt", ".rst", ".adoc", ".doc", ".docx"];
            if tool_name == "write_file" {
                if let Some(path) = input_json
                    .as_ref()
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                {
                    let lower = path.to_lowercase();
                    if DOC_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
                        false
                    } else {
                        tracing::warn!("Plan mode blocked tool: {}", reason);
                        true
                    }
                } else {
                    tracing::warn!("Plan mode blocked tool: {}", reason);
                    true
                }
            } else {
                tracing::warn!("Plan mode blocked tool: {}", reason);
                true
            }
        }
    };

    if plan_blocked {
        let convoy_id = tui
            .plan_mode
            .current_plan()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "Manual".to_string());
        tui.show_approval_banner(&convoy_id, "Plan mode: tool not allowed");
        tui.add_system_message(format!("Plan mode blocked tool execution: {}", tool_name));
        return;
    }

    let hooks_dir = tui.hook_manager.hooks_dir().to_path_buf();
    let ctx = serde_json::json!({"tool_name": &tool_name, "tool_id": &tool_id});
    spawn_hook(
        hooks_dir,
        rustycode_tools::hooks::HookTrigger::PreToolUse,
        ctx,
    );

    let is_reasoning_start = is_reasoning_tool(&tool_name);
    if is_reasoning_start {
        let budget_exhausted = match tui.reasoning_budget.lock() {
            Ok(budget) => budget.stop_and_code_active,
            Err(_) => false,
        };
        if budget_exhausted {
            tui.add_system_message(
                "STOP_AND_CODE: Reasoning budget exhausted. \
                 Produce code or implementation output now."
                    .to_string(),
            );
            tracing::warn!(
                "Reasoning budget exhausted, blocking reasoning tool: {}",
                tool_name
            );
        }
    }

    tui.update_terminal_title();

    let initial_summary = match (
        &input_json,
        build_tool_summary_arg(
            &tool_name,
            input_json.as_ref().unwrap_or(&serde_json::Value::Null),
        ),
    ) {
        (Some(_), Some(ctx)) => format!("{} {}...", tool_name, ctx),
        _ => format!("{}...", tool_name),
    };

    let panel_summary = format!("{}...", tool_name);
    tui.tool_panel.tool_panel_history.push(new_running_tool(
        tool_id.clone(),
        tool_name.clone(),
        input_json.clone(),
        panel_summary,
    ));
    if tui.tool_panel.tool_panel_history.len() > 50 {
        tui.tool_panel.tool_panel_history.remove(0);
    }

    tui.active_tools.insert(
        tool_id.clone(),
        new_running_tool(
            tool_id.clone(),
            tool_name.clone(),
            input_json.clone(),
            initial_summary.clone(),
        ),
    );

    let assistant_msg = tui.last_assistant_message_mut();
    if let Some(last_msg) = assistant_msg {
        let tool_execution = new_running_tool(tool_id, tool_name, input_json, initial_summary);

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

    tui.dirty = true;
}

pub(super) fn handle_tool_progress_chunk(
    tui: &mut TUI,
    tool_id: Option<String>,
    tool_name: String,
    stage: String,
    elapsed_ms: u64,
    output_preview: Option<String>,
) {
    // Tool execution progress — update tool panel and active tools
    tracing::debug!(
        "Tool progress: {} - {} ({}ms)",
        tool_name,
        stage,
        elapsed_ms
    );
    tui.dirty = true;

    // Match by tool_id when available, fall back to tool_name
    let matches_tool = |entry: &ToolExecution| {
        if let Some(ref id) = tool_id {
            entry.tool_id == *id
        } else {
            entry.name == tool_name
        }
    };

    // Update tool panel history entries for this tool (show progress)
    for entry in tui.tool_panel.tool_panel_history.iter_mut().rev() {
        if matches_tool(entry) && entry.status == ToolStatus::Running {
            let preview = output_preview.as_deref().unwrap_or("");
            if !preview.is_empty() {
                entry.result_summary = if preview.len() > 100 {
                    format!("{}...", &preview[..preview.floor_char_boundary(97)])
                } else {
                    preview.to_string()
                };
            }
            // Update progress description from stage
            if !stage.is_empty() {
                entry.progress_description = Some(stage.clone());
            }
            break; // Only update the most recent matching tool
        }
    }

    // Also update the ToolExecution in the current message's tool_executions
    for msg in tui.messages.iter_mut().rev() {
        if let Some(tools) = &mut msg.tool_executions {
            for tool in tools.iter_mut().rev() {
                if matches_tool(tool) && tool.status == ToolStatus::Running {
                    // Update progress description from stage
                    if !stage.is_empty() {
                        tool.progress_description = Some(stage.clone());
                    }
                    break;
                }
            }
        }
        // Stop at the first message with matching running tool
        if msg.tool_executions.as_ref().is_some_and(|tools| {
            tools
                .iter()
                .any(|t| matches_tool(t) && t.status == ToolStatus::Running)
        }) {
            break;
        }
    }

    if let Some(preview) = output_preview {
        if preview.len() <= 100 {
            tracing::debug!("  Output preview: {}", preview);
        }
    }
}

pub(super) fn handle_tool_complete_chunk(
    tui: &mut TUI,
    tool_name: String,
    tool_id: String,
    duration_ms: u64,
    success: bool,
    output_size: usize,
    output: Option<String>,
) {
    let status = if success { "✓" } else { "✗" };
    let size_str = if output_size > 1024 {
        format!("{:.1}KB", output_size as f64 / 1024.0)
    } else {
        format!("{}b", output_size)
    };

    // Remove from active_tools map using tool_id for accurate matching
    tui.active_tools.remove(&tool_id);
    tui.update_terminal_title();

    tracing::info!(
        "Tool complete: {} {} ({}ms, {})",
        status,
        tool_name,
        duration_ms,
        size_str
    );

    let context_summary = tui
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            m.tool_executions
                .as_ref()?
                .iter()
                .rev()
                .find(|t| t.tool_id == tool_id)
                .and_then(|t| t.input_json.as_ref())
        })
        .and_then(|json| build_tool_summary_arg(&tool_name, json));

    let result_summary = match &context_summary {
        Some(ctx) => format!(
            "{} {} {} ({}ms, {})",
            status, tool_name, ctx, duration_ms, size_str
        ),
        None => format!("{} {} ({}ms, {})", status, tool_name, duration_ms, size_str),
    };

    let detailed_output = output.map(|raw| {
        let clean = crate::app::tool_output_format::strip_ansi_escapes(&raw);
        const MAX_INLINE_CHARS: usize = 4000;
        if clean.chars().count() <= MAX_INLINE_CHARS {
            clean
        } else {
            let truncated_lines: Vec<&str> = clean.lines().take(21).collect();
            let has_more = truncated_lines.len() > 20;
            let truncated_lines = &truncated_lines[..20.min(truncated_lines.len())];
            let mut truncated = truncated_lines.join("\n");
            if has_more {
                truncated.push_str(&format!(
                    "\n\n... (more lines truncated, {} chars total)",
                    clean.chars().count()
                ));
            }
            truncated
        }
    });

    let assistant_msg = tui.last_assistant_message_mut();
    if let Some(last_msg) = assistant_msg {
        if let Some(tools) = &mut last_msg.tool_executions {
            if let Some(tool) = tools.iter_mut().find(|t| t.tool_id == tool_id) {
                tool.status = if success {
                    ToolStatus::Complete
                } else {
                    ToolStatus::Failed
                };
                tool.end_time = Some(chrono::Utc::now());
                tool.duration_ms = Some(duration_ms);
                tool.result_summary = result_summary.clone();
                tool.detailed_output = detailed_output.clone();
            }
        }
    }

    for entry in tui.tool_panel.tool_panel_history.iter_mut().rev() {
        if entry.tool_id == tool_id {
            entry.status = if success {
                ToolStatus::Complete
            } else {
                ToolStatus::Failed
            };
            entry.end_time = Some(chrono::Utc::now());
            entry.duration_ms = Some(duration_ms);
            entry.result_summary = result_summary.clone();
            entry.detailed_output = detailed_output.clone();
            break;
        }
    }

    // Auto-scroll when a tool completes so the subsequent response
    // text is visible, but only if the user hasn't deliberately scrolled
    // up to read earlier messages.
    if !tui.view.user_scrolled {
        tui.view.scroll_offset_line = 0;
    }

    tui.dirty = true;

    // Toast notification for failed tools so the user notices even
    // when scrolled away from the tool output.
    if !success {
        tui.toast_manager.warning(format!("{} failed", tool_name));
    }

    // Doom loop detection: record tool result for pattern analysis.
    // Reasoning tools are exempt — they're designed to be called in
    // sequence (decompose → research → validate → check) and may
    // repeat with similar args by design.
    let reasoning = is_reasoning_tool(&tool_name);

    if success {
        if let Ok(mut budget) = tui.reasoning_budget.lock() {
            if reasoning {
                let triggered = budget.record_exploration();
                if triggered {
                    tui.toast_manager.warning(
                        "Reasoning budget exhausted — STOP_AND_CODE activated".to_string(),
                    );
                }
            } else if matches!(
                tool_name.as_str(),
                "write_file" | "edit_file" | "multiedit" | "apply_patch" | "bash"
            ) {
                budget.record_code();
            }
        }
    }

    // Extract a key argument (file path, command, etc.) for fingerprinting.
    let key_arg = tui
        .messages
        .iter()
        .rev()
        .find_map(|m| {
            m.tool_executions
                .as_ref()?
                .iter()
                .rev()
                .find(|t| t.tool_id == tool_id && t.status != ToolStatus::Running)
        })
        .and_then(|t| {
            t.input_json.as_ref().and_then(|json| {
                // Try common field names: file_path, path, command, query, pattern
                json.get("file_path")
                    .or_else(|| json.get("path"))
                    .or_else(|| json.get("command"))
                    .or_else(|| json.get("query"))
                    .or_else(|| json.get("pattern"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
        });
    if !reasoning {
        tui.doom_loop
            .record(&tool_name, key_arg.as_deref(), success);
    }

    if tui.doom_loop.is_doom_loop() {
        if let Some(reason) = tui.doom_loop.doom_loop_reason() {
            tui.toast_manager.warning(format!("Doom loop: {}", reason));
        }
    }

    let post_ctx = serde_json::json!({"tool_name": &tool_name, "success": success});
    let post_dir = tui.hook_manager.hooks_dir().to_path_buf();
    spawn_hook(
        post_dir,
        rustycode_tools::hooks::HookTrigger::PostToolUse,
        post_ctx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_reasoning_tool ---

    #[test]
    fn test_is_reasoning_tool_decompose() {
        assert!(is_reasoning_tool("reasoning_decompose"));
    }

    #[test]
    fn test_is_reasoning_tool_research() {
        assert!(is_reasoning_tool("reasoning_research"));
    }

    #[test]
    fn test_is_reasoning_tool_validate() {
        assert!(is_reasoning_tool("reasoning_validate"));
    }

    #[test]
    fn test_is_reasoning_tool_integrate() {
        assert!(is_reasoning_tool("reasoning_integrate"));
    }

    #[test]
    fn test_is_reasoning_tool_bash_is_not_reasoning() {
        assert!(!is_reasoning_tool("bash"));
    }

    #[test]
    fn test_is_reasoning_tool_empty_string() {
        assert!(!is_reasoning_tool(""));
    }

    #[test]
    fn test_is_reasoning_tool_partial_name_is_not_reasoning() {
        assert!(!is_reasoning_tool("reasoning"));
        assert!(!is_reasoning_tool("decompose"));
    }

    // --- new_running_tool ---

    #[test]
    fn test_new_running_tool_status_is_running() {
        let tool = new_running_tool(
            "id-1".to_string(),
            "bash".to_string(),
            None,
            "running...".to_string(),
        );
        assert_eq!(tool.status, ToolStatus::Running);
    }

    #[test]
    fn test_new_running_tool_preserves_id_and_name() {
        let tool = new_running_tool(
            "tool-abc".to_string(),
            "read_file".to_string(),
            None,
            "summary".to_string(),
        );
        assert_eq!(tool.tool_id, "tool-abc");
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.result_summary, "summary");
    }

    #[test]
    fn test_new_running_tool_no_end_time_or_duration() {
        let tool = new_running_tool(
            "id".to_string(),
            "bash".to_string(),
            None,
            "summary".to_string(),
        );
        assert!(tool.end_time.is_none());
        assert!(tool.duration_ms.is_none());
    }

    #[test]
    fn test_new_running_tool_no_detailed_output() {
        let tool = new_running_tool(
            "id".to_string(),
            "bash".to_string(),
            None,
            "summary".to_string(),
        );
        assert!(tool.detailed_output.is_none());
    }

    #[test]
    fn test_new_running_tool_with_input_json() {
        let json = serde_json::json!({"command": "ls -la"});
        let tool = new_running_tool(
            "id".to_string(),
            "bash".to_string(),
            Some(json.clone()),
            "bash ls -la...".to_string(),
        );
        assert_eq!(tool.input_json, Some(json));
    }

    #[test]
    fn test_new_running_tool_without_input_json() {
        let tool = new_running_tool(
            "id".to_string(),
            "bash".to_string(),
            None,
            "bash...".to_string(),
        );
        assert!(tool.input_json.is_none());
    }

    #[test]
    fn test_new_running_tool_no_progress_fields() {
        let tool = new_running_tool(
            "id".to_string(),
            "bash".to_string(),
            None,
            "summary".to_string(),
        );
        assert!(tool.progress_current.is_none());
        assert!(tool.progress_total.is_none());
        assert!(tool.progress_description.is_none());
    }

    // --- REASONING_TOOLS constant coverage ---

    #[test]
    fn test_reasoning_tools_constant_has_four_entries() {
        assert_eq!(REASONING_TOOLS.len(), 4);
    }
}
