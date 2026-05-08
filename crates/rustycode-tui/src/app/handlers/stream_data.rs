//! Miscellaneous stream chunk handlers — task extraction, questions, snapshots, tokens, traces.

use crate::app::task_extraction::extract_action_items;
use crate::app::TUI;
use chrono;
use rustycode_protocol::MilestoneStatus;
use tracing;

pub(super) fn handle_extract_tasks_chunk(tui: &mut TUI, text: String) {
    // Save current state for undo
    tui.last_extraction = Some((
        tui.workspace_tasks.tasks.clone(),
        tui.workspace_tasks.todos.clone(),
    ));

    // Extract tasks/todos from the provided text
    let initial_todos = tui.workspace_tasks.todos.len();
    let initial_tasks = tui.workspace_tasks.tasks.len();

    extract_action_items(&text, &mut tui.workspace_tasks);

    let new_todos = tui
        .workspace_tasks
        .todos
        .len()
        .saturating_sub(initial_todos);
    let new_tasks = tui
        .workspace_tasks
        .tasks
        .len()
        .saturating_sub(initial_tasks);

    crate::app::tasks::save_tasks_with_storage(
        &tui.workspace_tasks,
        tui.storage.as_deref(),
        tui.services.cwd(),
        None,
    );

    // Provide feedback to user
    if new_todos > 0 || new_tasks > 0 {
        let mut feedback = Vec::new();
        if new_todos > 0 {
            feedback.push(format!(
                "☐ {} todo{}",
                new_todos,
                if new_todos == 1 { "" } else { "s" }
            ));
        }
        if new_tasks > 0 {
            feedback.push(format!(
                "🔄 {} task{}",
                new_tasks,
                if new_tasks == 1 { "" } else { "s" }
            ));
        }

        tui.add_system_message(format!("✓ Auto-created {}", feedback.join(" and ")));
        tui.add_system_message("💡 Tip: Press Ctrl+Shift+U to undo this extraction".to_string());
        tui.auto_scroll();
    }

    tracing::info!(
        "Auto-extracted {} todos and {} tasks from assistant response",
        new_todos,
        new_tasks
    );
}

pub(super) fn handle_tasks_extracted_chunk(_tui: &mut TUI, todos_count: usize, tasks_count: usize) {
    // Notification that extraction happened (for logging/debugging)
    tracing::info!(
        "Tasks extracted: {} todos, {} tasks",
        todos_count,
        tasks_count
    );
}

pub(super) fn handle_question_request_chunk(
    tui: &mut TUI,
    _question_id: String,
    question_text: String,
    header: String,
    options: Vec<crate::app::async_::QuestionOption>,
    _multi_select: bool,
) {
    // Show question to user - for now just log it
    // Full TUI integration would show a dialog here
    tracing::info!("Question from AI: {} - {}", header, question_text);
    let option_summary: Vec<_> = options.iter().map(|o| o.label.as_str()).collect();
    tracing::info!("Options: {:?}", option_summary);

    // Build structured options from the AI's question
    let ui_options: Vec<crate::ui::clarification::QuestionOption> = options
        .iter()
        .map(|o| crate::ui::clarification::QuestionOption {
            label: o.label.clone(),
            description: o.description.clone(),
        })
        .collect();

    let question = crate::ui::clarification::Question {
        text: format!("{}: {}", header, question_text),
        context: Some(format!("Options: {}", option_summary.join(", "))),
        options: ui_options,
    };
    tui.clarification_panel = crate::ui::clarification::ClarificationPanel::new(vec![question]);
    tui.awaiting_clarification = true;
    tui.add_system_message(format!("❓ AI asks: {}", question_text));
    tui.dirty = true;
}

pub(super) fn handle_question_answered_chunk(
    _tui: &mut TUI,
    _question_id: String,
    _answer: String,
) {
    // Question was answered - this is for logging
}

pub(super) fn handle_file_snapshot_chunk(tui: &mut TUI, batch: Vec<(String, String)>) {
    // Snapshot of file content before a write operation — push to undo stack
    if !batch.is_empty() {
        tui.undo.push_file_batch(batch);
    }
}

pub(super) fn handle_token_usage_chunk(
    tui: &mut TUI,
    input_tokens: usize,
    output_tokens: usize,
    cache_read_tokens: usize,
    cache_creation_tokens: usize,
) {
    tui.token_budget.session_input_tokens += input_tokens;
    tui.token_budget.session_output_tokens += output_tokens;
    tui.token_budget.session_cache_read_tokens += cache_read_tokens;
    tui.token_budget.session_cache_creation_tokens += cache_creation_tokens;
    tui.token_budget.last_turn_input_tokens = input_tokens;

    // Update context monitor with real API token counts
    tui.compaction
        .context_monitor
        .update_from_api(input_tokens, &tui.current_model);

    let model = &tui.current_model;
    let turn_cost = rustycode_llm::token_tracker::estimate_cost(model, input_tokens, output_tokens);
    tui.token_budget.session_cost_usd += turn_cost;

    let (input_cost_per_m, _) = rustycode_llm::token_tracker::cost_per_million_tokens_io(model);
    let cache_savings = (cache_read_tokens as f64 / 1_000_000.0) * input_cost_per_m * 0.9;

    if let Err(e) =
        tui.token_budget
            .cost_tracker
            .record_call(rustycode_llm::cost_tracker::LlmApiCall {
                model: model.clone(),
                input_tokens,
                output_tokens,
                cost_usd: turn_cost,
                timestamp: chrono::Utc::now(),
                tool_name: None,
                cache_read_tokens: cache_read_tokens as u32,
                cache_creation_tokens: cache_creation_tokens as u32,
                cache_savings_usd: cache_savings,
            })
    {
        tracing::debug!("Cost tracking failed: {}", e);
    }

    tui.dirty = true;
}

pub(super) fn handle_execution_trace_chunk(tui: &mut TUI, trace: serde_json::Value) {
    tracing::debug!("Received execution trace from orchestration pipeline");
    tui.execution_trace = Some(trace);
    tui.dirty = true;
    tui.mark_session_dirty();
}

pub(super) fn handle_system_message_chunk(tui: &mut TUI, msg: String) {
    tui.add_system_message(msg);
    tui.dirty = true;
}

pub(super) fn handle_milestone_progress_chunk(
    tui: &mut TUI,
    milestone_id: String,
    milestone_title: String,
    status: MilestoneStatus,
    plans_total: usize,
    plans_completed: usize,
    current_plan_summary: String,
    action_hint: String,
    plan_rows: Vec<rustycode_orchestration::bus::MilestonePlanProgress>,
) {
    tui.session_sidebar.update_milestone_progress(
        milestone_id.clone(),
        milestone_title.clone(),
        status,
        plans_total,
        plans_completed,
        current_plan_summary.clone(),
        action_hint.clone(),
        plan_rows,
    );
    tui.show_milestone_progress_banner(
        &milestone_title,
        status,
        plans_total,
        plans_completed,
        &current_plan_summary,
        &action_hint,
    );
    tui.dirty = true;
}
