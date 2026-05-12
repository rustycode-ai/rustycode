// event_loop_slash_commands.rs — Slash command handlers and post-command utilities
// Included via include!() at module level in event_loop.rs

impl TUI {

/// Apply a shared slash-command effect to the TUI state.
fn apply_slash_command_effect(&mut self, effect: CommandEffect) -> Result<()> {
    match effect {
        CommandEffect::AsyncStarted(message) | CommandEffect::SystemMessage(message) => {
            self.add_system_message(message);
        }
        CommandEffect::MultipleMessages(messages) => {
            for message in messages {
                self.add_system_message(message);
            }
        }
        CommandEffect::ShowHelp => {
            if !self.is_any_overlay_open() {
                self.ui.help_state.visible = true;
                self.ui.help_state.scroll_offset = 0;
            }
        }
        CommandEffect::ShowPluginManager => {
            if !self.is_any_overlay_open() {
                self.overlays.showing_plugin_manager = true;
                self.ui.plugin_manager_ui.show();
                {
                    let mut manager = self
                        .sys.plugin_manager
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    let _ = manager.reload_from_disk();
                }
                self.sys.dirty = true;
            }
        }
        CommandEffect::None => {}
        CommandEffect::ModelSwitch { model_id } => {
            self.model.current_model = model_id.clone();
            let short = model_id.rsplit('/').next().unwrap_or(&model_id);
            self.theme.toast_manager.success(format!("Model: {}", short));

            if let Err(e) = self.integration.services.submit_op(Op::SwitchModel { model_id }) {
                tracing::error!("Failed to switch model in services: {}", e);
                self.add_system_message(format!("⚠️ Failed to update orchestration model: {}", e));
            }
        }
        CommandEffect::ClearConversation => {
            // Signal background stream to stop BEFORE clearing state.
            // Without this, the stream thread keeps running and its Done
            // handler would trigger auto-continue or queued message on
            // the now-empty conversation.
            if self.session.streaming.is_streaming {
                self.integration.services.submit_op(Op::StopStream).ok();
                self.session.streaming.stream_cancelled = true;
            }
            self.reset_conversation_state();
            self.add_system_message("Conversation cleared".to_string());
        }
        CommandEffect::StartTeam { task } => {
            self.spawn_team_orchestrator(&task)?;
        }
        CommandEffect::CancelTeam => {
            self.cancel_team();
        }
        CommandEffect::LoadSession {
            name,
            messages,
            summary,
        } => {
            // Signal background stream to stop before loading new session
            if self.session.streaming.is_streaming {
                self.integration.services.submit_op(Op::StopStream).ok();
                self.session.streaming.stream_cancelled = true;
            }
            self.reset_conversation_state();
            self.session.messages = messages;
            self.sys.compaction.context_monitor.update(&self.session.messages);
            if !self.session.messages.is_empty() {
                self.ui.view.selected_message = self.session.messages.len() - 1;
            }
            self.add_system_message(format!("✓ Loaded session '{}' — {}", name, summary));
        }
        CommandEffect::SetPlanMode { planning } => {
            if planning {
                self.show_planning_banner("System");
                self.add_system_message("Plan mode enabled — tools are read-only".to_string());
            } else {
                self.clear_plan_mode_banner();
                self.add_system_message("Plan mode disabled — full tool access".to_string());
            }
        }
        CommandEffect::SetBudget { limit } => {
            if let Some(amount) = limit {
                self.add_system_message(format!("Budget limit set to ${:.2}", amount));
            } else {
                self.add_system_message("Budget limit removed".to_string());
            }
        }
        CommandEffect::RetryLastMessage => {
            // Find the last user message and re-send it
            if let Some(last_user_msg) = self
                .session.messages
                .iter()
                .rev()
                .find(|m| matches!(m.role, crate::ui::message::MessageRole::User))
            {
                if !last_user_msg.content.is_empty() {
                    self.retry_last_message(last_user_msg.content.clone());
                }
            }
        }
        CommandEffect::SwitchToMcpMode => {
            // Delegate to the existing MCP Mode entry point.
            // The async SlashCommandResult handler in the event loop
            // will receive the sentinel and switch the view.
            self.add_system_message(
                "MCP Mode: Press Esc to close. Type 'list' for servers, 'status' for connection info."
                    .to_string(),
            );
        }
    }

    Ok(())
}

/// Spawn a TeamOrchestrator on a background thread, subscribe to its
/// broadcast channel, and wire events into the team panel.
fn spawn_team_orchestrator(&mut self, task: &str) -> Result<()> {
    use rustycode_team::orchestrator::TeamOrchestrator;

    let cwd = std::env::current_dir().unwrap_or_default();

    // Load provider
    let (provider_type, model, v2_config) = rustycode_llm::load_provider_config_from_env()
        .context("Failed to load provider config for team mode")?;

    let provider =
        rustycode_llm::create_provider_with_config(&provider_type, &model, v2_config)
            .context("Failed to create provider for team mode")?;

    let orchestrator = TeamOrchestrator::new(&cwd, provider, model.to_string());
    let event_rx = orchestrator.subscribe();

    // Get cancel token for cooperative cancellation
    let cancel_token = orchestrator.cancel_token();
    self.team.team_handler.cancel_token = Some(cancel_token);

    // Show the team panel
    self.team.team_panel.set_task(task);
    self.team.team_panel.visible = true;
    self.team.team_panel.reset();
    self.sys.dirty = true;

    self.add_system_message(format!(
        "🤖 Team mode started: \"{}\"\n   Architect → Builder → Skeptic → Judge → Scalpel\n   Press Ctrl+G to toggle team panel | Esc to cancel",
        task
    ));

    // Store the receiver for polling in the event loop
    self.team.team_handler.event_rx = Some(event_rx);

    // Spawn the orchestrator on a background thread
    let task_owned = task.to_string();
    std::thread::spawn(move || {
        rustycode_shared_runtime::block_on_shared(async move {
            if let Err(e) = orchestrator.execute(&task_owned).await {
                tracing::error!("Team orchestrator failed: {}", e);
            }
        });
    });

    Ok(())
}

/// Cancel a running team orchestrator, Shows a summary and hides the panel.
pub(crate) fn cancel_team(&mut self) {
    if let Some(token) = &self.team.team_handler.cancel_token {
        token.store(true, std::sync::atomic::Ordering::SeqCst);
        self.add_system_message("⏹ Team task cancelled.".to_string());
        self.team.team_panel.visible = false;
        self.team.team_handler.event_rx = None;
        self.team.team_handler.cancel_token = None;
        self.sys.dirty = true;
    } else {
        self.add_system_message("⚠ No team task is running.".to_string());
    }
}

/// Show session cost and usage summary
fn handle_cost_command(&mut self) {
    let total_tokens = self.model.token_budget.session_input_tokens + self.model.token_budget.session_output_tokens;
    let turn_count = self
        .session.messages
        .iter()
        .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
        .count();

    let cost_str = if self.model.token_budget.session_cost_usd < 0.001 {
        "negligible".to_string()
    } else if self.model.token_budget.session_cost_usd < 0.01 {
        format!("${:.4}", self.model.token_budget.session_cost_usd)
    } else {
        format!("${:.2}", self.model.token_budget.session_cost_usd)
    };

    let token_str = if total_tokens >= 1_000_000 {
        format!("{:.1}M", total_tokens as f64 / 1_000_000.0)
    } else if total_tokens >= 1_000 {
        format!("{:.1}k", total_tokens as f64 / 1_000.0)
    } else {
        total_tokens.to_string()
    };

    let input_str = if self.model.token_budget.session_input_tokens >= 1_000 {
        format!("{:.1}k", self.model.token_budget.session_input_tokens as f64 / 1_000.0)
    } else {
        self.model.token_budget.session_input_tokens.to_string()
    };

    let output_str = if self.model.token_budget.session_output_tokens >= 1_000 {
        format!("{:.1}k", self.model.token_budget.session_output_tokens as f64 / 1_000.0)
    } else {
        self.model.token_budget.session_output_tokens.to_string()
    };

    let ctx_pct = if self.sys.compaction.context_monitor.max_tokens > 0 {
        format!("{:.0}%", self.sys.compaction.context_monitor.usage_percentage() * 100.0)
    } else {
        "N/A".to_string()
    };

    let model_display = self
        .model.current_model
        .rsplit('/')
        .next()
        .unwrap_or(&self.model.current_model)
        .to_string();

    let summary = format!(
        "Session Usage ({} turns, {}):\n  Tokens: {} total ({} in / {} out)\n  Context: {} used\n  Cost: {} ({})\n  API calls: {}",
        turn_count, model_display, token_str, input_str, output_str, ctx_pct, cost_str,
        if self.model.token_budget.session_cost_usd > 0.0 { "estimated" } else { "free/local model" },
        self.model.token_budget.cost_tracker.calls_count(),
    );

    let mut full_summary = summary;

    let by_tool = self.model.token_budget.cost_tracker.costs_by_tool();
    if !by_tool.is_empty() {
        let tool_breakdown: Vec<String> = by_tool
            .iter()
            .filter(|(_, c)| **c > 0.0)
            .map(|(tool, cost)| format!("    {}: ${:.4}", tool, cost))
            .collect();
        if !tool_breakdown.is_empty() {
            full_summary.push_str("\n  Cost by tool:\n");
            full_summary.push_str(&tool_breakdown.join("\n"));
        }
    }

    self.add_system_message(full_summary);
    // Ensure UI shows the latest summary by auto-scrolling to bottom
    self.auto_scroll();
}

/// Print session summary to stdout after TUI exits.
///
/// Print a one-line cost/duration summary in the terminal after the TUI exits,
/// making it easy to track spending across sessions.
fn print_session_summary(&self) {
    let turn_count = self
        .session.messages
        .iter()
        .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
        .count();
    let total_tokens = self.model.token_budget.session_input_tokens + self.model.token_budget.session_output_tokens;
    let model = self
        .model.current_model
        .rsplit('/')
        .next()
        .unwrap_or(&self.model.current_model);

    let fmt = |n: usize| -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}k", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    };

    let cost = if self.model.token_budget.session_cost_usd > 0.01 {
        format!("${:.2}", self.model.token_budget.session_cost_usd)
    } else if self.model.token_budget.session_cost_usd > 0.0 {
        format!("${:.4}", self.model.token_budget.session_cost_usd)
    } else {
        "free".to_string()
    };

    // Only print if there was actual activity
    if turn_count > 0 {
        println!(
            "\n  Session: {} turns, {} tokens ({} in / {} out), {}, model: {}",
            turn_count,
            fmt(total_tokens),
            fmt(self.model.token_budget.session_input_tokens),
            fmt(self.model.token_budget.session_output_tokens),
            cost,
            model
        );
    }
}

/// Update terminal window/tab title dynamically based on state.
///
/// Update terminal window/tab title so users with many tabs can see at a glance
/// whether the AI is idle, thinking, or running tools.
pub(crate) fn update_terminal_title(&self) {
    if let Some(dir_name) = self.integration.services.cwd().file_name().and_then(|n| n.to_str()) {
        let sanitized: String = dir_name.chars().filter(|c| !c.is_control()).collect();
        let state = if self.session.streaming.is_streaming {
            if self.session.active_tools.is_empty() {
                "thinking"
            } else {
                "tools"
            }
        } else {
            "ready"
        };
        print!("\x1b]0;rustycode: {} [{}]\x07", sanitized, state);
        let _ = std::io::stdout().flush();
    }
}

pub(crate) fn apply_model_switch(&mut self, model: &crate::ui::model_selector::ModelInfo) {
    let result = crate::services::provider_manager::compute_model_switch(model);
    std::env::set_var("RUSTYCODE_MODEL_OVERRIDE", &result.model_id);
    std::env::set_var("RUSTYCODE_PROVIDER_OVERRIDE", &result.provider);
    self.model.current_model = result.model_id.clone();
    self.sys.compaction.compaction_config.model_id = Some(result.model_id);
    self.sys.compaction.context_monitor.max_tokens = self.sys.compaction.compaction_config.effective_max_tokens();
    self.add_system_message(result.status_message);
    self.overlays.model_selector.hide();
    self.sys.dirty = true;
}

/// Update rate limit countdown message with auto-retry
fn update_rate_limit_countdown(&mut self) -> bool {
    // Capture message_index BEFORE update_countdown() clears it on expiry.
    let saved_msg_idx = self.integration.rate_limit.message_index;

    // Use the rate limit handler to update countdown
    if let Some(new_content) = self.integration.rate_limit.update_countdown() {
        // Update the countdown message in-place (if index is still valid)
        if let Some(msg_idx) = saved_msg_idx {
            if let Some(message) = self.session.messages.get_mut(msg_idx) {
                message.content = new_content;
                self.sys.dirty = true;
            }
        }

        // Check if we should auto-retry (countdown expired, not cancelled)
        if self.integration.rate_limit.should_auto_retry() {
            if let Some(last_msg) = self.integration.rate_limit.take_last_message() {
                self.retry_last_message(last_msg);
            }
        }
        return true;
    }
    false
}
}
