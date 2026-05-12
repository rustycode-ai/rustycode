//! Special input state handlers
//!
//! Handles wizard, approval dialogs, clarification panels, and other modal states.

use crate::app::event_loop::TUI;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use rustycode_protocol::Op;

impl TUI {
    /// Handle wizard input
    pub(crate) fn handle_wizard_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.session.wizard.showing_wizard {
            return Ok(false);
        }

        if let Some(ref mut wizard) = self.session.wizard.wizard {
            match wizard.handle_key_event(key) {
                crate::ui::wizard::WizardAction::Continue => {
                    self.sys.dirty = true;
                    // Check if wizard is complete
                    if wizard.step == crate::ui::wizard::WizardStep::Complete {
                        self.session.wizard.showing_wizard = false;
                    }
                }
                crate::ui::wizard::WizardAction::Finish => {
                    self.session.wizard.showing_wizard = false;
                    self.sys.dirty = true;
                }
                crate::ui::wizard::WizardAction::Quit => {
                    self.session.wizard.showing_wizard = false;
                    self.sys.running = false;
                    self.sys.dirty = true;
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn handle_approval_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.panels.tool_approval.awaiting {
            return Ok(false);
        }

        // First handle any synchronous global confirmation requests (e.g., checkpoint restore)
        let pending = crate::app::confirmation::pending_list();
        if !pending.is_empty() {
            let req_id = pending[0].clone();
            return match key.code {
                KeyCode::Char('y')
                | KeyCode::Char('Y')
                | KeyCode::Char('a')
                | KeyCode::Char('A') => {
                    crate::app::confirmation::deliver(&req_id, true);
                    self.add_system_message("✓ Confirmed action".to_string());
                    self.sys.dirty = true;
                    Ok(true)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    crate::app::confirmation::deliver(&req_id, false);
                    self.add_system_message("✗ Cancelled action".to_string());
                    self.sys.dirty = true;
                    Ok(true)
                }
                _ => {
                    // Consume key while modal is up
                    Ok(true)
                }
            };
        }

        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(req) = self.panels.tool_approval.pop_next() {
                    self.add_system_message(format!("✓ Approved: {}", req.tool_name));
                    self.panels.tool_approval.manager.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::Approved,
                    );
                    self.integration
                        .services
                        .submit_op(Op::ApproveTool {
                            tool_id: req.tool_id.clone(),
                            approved: true,
                            modified_input: None,
                            timeout_override: None,
                        })
                        .ok();
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Char('n') => {
                if let Some(req) = self.panels.tool_approval.pop_next() {
                    self.add_system_message(format!("✗ Rejected: {}", req.tool_name));
                    self.panels.tool_approval.manager.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::Rejected,
                    );
                    self.integration
                        .services
                        .submit_op(Op::ApproveTool {
                            tool_id: req.tool_id.clone(),
                            approved: false,
                            modified_input: None,
                            timeout_override: None,
                        })
                        .ok();
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Char('N') => {
                if let Some(req) = self.panels.tool_approval.pop_next() {
                    self.add_system_message(format!(
                        "✗ Blocked for session: {} (won't ask again)",
                        req.tool_name
                    ));
                    self.panels.tool_approval.manager.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::RejectedAll,
                    );
                    self.integration
                        .services
                        .submit_op(Op::ApproveTool {
                            tool_id: req.tool_id.clone(),
                            approved: false,
                            modified_input: None,
                            timeout_override: None,
                        })
                        .ok();
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Some(req) = self.panels.tool_approval.pop_next() {
                    self.add_system_message(format!("✓ Always approved: {}", req.tool_name));
                    self.panels.tool_approval.manager.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::ApprovedAll,
                    );
                    self.integration
                        .services
                        .submit_op(Op::ApproveTool {
                            tool_id: req.tool_id.clone(),
                            approved: true,
                            modified_input: None,
                            timeout_override: None,
                        })
                        .ok();
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Esc => {
                let tool_id = self
                    .panels
                    .tool_approval
                    .pending_requests
                    .front()
                    .map(|req| req.tool_id.clone());
                self.panels.tool_approval.dismiss_current();
                self.integration
                    .services
                    .submit_op(Op::ApproveTool {
                        tool_id: tool_id.unwrap_or_default(),
                        approved: false,
                        modified_input: None,
                        timeout_override: None,
                    })
                    .ok();
                self.add_system_message("⏸️  Approval cancelled".to_string());
                self.sys.dirty = true;
                Ok(true)
            }
            _ => {
                // Scroll keys for diff preview
                if let Some(req) = self.panels.tool_approval.pending_requests.front_mut() {
                    if req.has_diff_content() {
                        match key.code {
                            KeyCode::Down | KeyCode::Char('j') => {
                                let visible = 10; // approximate visible diff lines
                                req.scroll_diff_down(visible);
                                self.sys.dirty = true;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                req.scroll_diff_up();
                                self.sys.dirty = true;
                            }
                            _ => {}
                        }
                    }
                }
                Ok(true)
            }
        }
    }

    /// Handle error display input
    pub(crate) fn handle_error_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.overlays.showing_error || !self.theme.error_manager.is_showing() {
            return Ok(false);
        }

        match key.code {
            KeyCode::Enter => {
                // Dismiss error
                self.theme.error_manager.dismiss();
                self.overlays.showing_error = false;
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Toggle details
                self.theme.error_manager.toggle_details();
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Esc => {
                // Also dismiss on Escape
                self.theme.error_manager.dismiss();
                self.overlays.showing_error = false;
                self.sys.dirty = true;
                Ok(true)
            }
            _ => {
                // Any other key dismisses the error (don't trap the user)
                self.theme.error_manager.dismiss();
                self.overlays.showing_error = false;
                self.sys.dirty = true;
                Ok(true)
            }
        }
    }

    /// Handle clarification question input
    pub(crate) fn handle_clarification_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.panels.awaiting_clarification {
            return Ok(false);
        }

        match key.code {
            KeyCode::Enter => {
                // If current question has options, select highlighted option
                if self.panels.clarification_panel.current_has_options()
                    && self.panels.clarification_panel.current_answer().is_empty()
                {
                    self.panels.clarification_panel.select_current_option();
                }

                // Submit all answers if all questions are answered
                if self.panels.clarification_panel.all_answered() {
                    // Build the answer (for single question, just send the answer)
                    let answer = if self.panels.clarification_panel.questions.len() == 1 {
                        // Single question - send just the answer
                        self.panels.clarification_panel.current_answer().to_string()
                    } else {
                        // Multiple questions - build formatted response
                        let mut response = String::new();
                        for (i, question) in
                            self.panels.clarification_panel.questions.iter().enumerate()
                        {
                            if let Some(answer) = self.panels.clarification_panel.answers.get(i) {
                                if !answer.is_empty() {
                                    if !response.is_empty() {
                                        response.push_str("\n\n");
                                    }
                                    response
                                        .push_str(&format!("Q: {}\nA: {}", question.text, answer));
                                }
                            }
                        }
                        response
                    };

                    // Send answer through the question channel (resumes streaming)
                    self.integration
                        .services
                        .submit_op(Op::AnswerQuestion { answer })
                        .ok();

                    // Reset clarification state
                    self.panels.clarification_panel.reset();
                    self.panels.awaiting_clarification = false;
                    self.add_system_message("✓ Answer submitted".to_string());
                } else {
                    self.add_system_message(format!(
                        "Please answer all {} questions first",
                        self.panels.clarification_panel.questions.len()
                            - self.panels.clarification_panel.answered_count()
                    ));
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                // Navigate to previous question
                self.panels.clarification_panel.select_previous();
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                // Navigate to next question
                self.panels.clarification_panel.select_next();
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                // Navigate options left (for option-based questions)
                if self.panels.clarification_panel.current_has_options() {
                    self.panels.clarification_panel.select_previous_option();
                    self.sys.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                // Navigate options right (for option-based questions)
                if self.panels.clarification_panel.current_has_options() {
                    self.panels.clarification_panel.select_next_option();
                    self.sys.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Tab => {
                // Tab selects the currently highlighted option
                if self.panels.clarification_panel.current_has_options() {
                    self.panels.clarification_panel.select_current_option();
                    self.sys.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Esc => {
                // Cancel clarification
                self.panels.clarification_panel.reset();
                self.panels.awaiting_clarification = false;
                self.add_system_message("⏸️  Clarification cancelled".to_string());
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Char(c) => {
                // Typing - only allow for free-text questions (no options)
                if !self.panels.clarification_panel.current_has_options() {
                    let current_answer =
                        self.panels.clarification_panel.current_answer().to_string();
                    let new_answer = format!("{}{}", current_answer, c);
                    self.panels
                        .clarification_panel
                        .set_current_answer(new_answer);
                }
                self.sys.dirty = true;
                Ok(true)
            }
            KeyCode::Backspace => {
                // Delete last character from current answer (free-text only)
                if !self.panels.clarification_panel.current_has_options() {
                    let current_answer =
                        self.panels.clarification_panel.current_answer().to_string();
                    let new_answer: String = current_answer
                        .chars()
                        .take(current_answer.chars().count().saturating_sub(1))
                        .collect();
                    self.panels
                        .clarification_panel
                        .set_current_answer(new_answer);
                }
                self.sys.dirty = true;
                Ok(true)
            }
            _ => {
                // Other keys are ignored while answering clarification
                Ok(true)
            }
        }
    }

    /// Handle tool panel navigation input
    pub(crate) fn handle_tool_panel_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.panels.tool_panel.showing_tool_panel {
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers == crossterm::event::KeyModifiers::CONTROL => {
                // Cancel selected tool
                if let Some(idx) = self.panels.tool_panel.tool_panel_selected_index {
                    if idx < self.panels.tool_panel.tool_panel_history.len() {
                        // Only cancel running tools
                        if self.panels.tool_panel.tool_panel_history[idx].status
                            == crate::ui::message::ToolStatus::Running
                        {
                            self.panels.tool_panel.tool_panel_history[idx].cancel();
                            self.add_system_message(format!(
                                "⚠ Cancelled tool: {}",
                                self.panels.tool_panel.tool_panel_history[idx].name
                            ));
                            self.sys.dirty = true;
                            return Ok(true);
                        }
                    }
                }
                // If no running tool selected, show message
                self.add_system_message("⚠ No running tool selected to cancel".to_string());
                self.sys.dirty = true;
                return Ok(true);
            }
            KeyCode::Esc => {
                if self.panels.tool_panel.showing_tool_result {
                    // Close detailed result view
                    self.panels.tool_panel.showing_tool_result = false;
                    self.panels.tool_panel.tool_result_show_full = false;
                    self.panels.tool_panel.tool_panel_selected_index = None;
                    self.sys.dirty = true;
                } else {
                    // Close tool panel
                    self.panels.tool_panel.showing_tool_panel = false;
                    self.panels.tool_panel.tool_panel_selected_index = None;
                    self.sys.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Up => {
                if !self.panels.tool_panel.tool_panel_history.is_empty() {
                    let current = self
                        .panels
                        .tool_panel
                        .tool_panel_selected_index
                        .unwrap_or(0);
                    self.panels.tool_panel.tool_panel_selected_index =
                        Some(current.saturating_sub(1));
                    self.panels.tool_panel.showing_tool_result = false;
                    self.sys.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Down => {
                if !self.panels.tool_panel.tool_panel_history.is_empty() {
                    let current = self
                        .panels
                        .tool_panel
                        .tool_panel_selected_index
                        .unwrap_or(0);
                    let max_idx = self
                        .panels
                        .tool_panel
                        .tool_panel_history
                        .len()
                        .saturating_sub(1);
                    self.panels.tool_panel.tool_panel_selected_index =
                        Some((current + 1).min(max_idx));
                    self.panels.tool_panel.showing_tool_result = false;
                    self.sys.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Enter => {
                if let Some(idx) = self.panels.tool_panel.tool_panel_selected_index {
                    if idx < self.panels.tool_panel.tool_panel_history.len() {
                        let tool = &self.panels.tool_panel.tool_panel_history[idx];
                        // Show detail view if there's detailed output OR a non-empty result summary
                        let has_content =
                            tool.detailed_output.is_some() || !tool.result_summary.is_empty();
                        if has_content {
                            self.panels.tool_panel.showing_tool_result = true;
                            self.panels.tool_panel.tool_result_show_full = false;
                            self.panels.tool_panel.tool_result_scroll_offset = 0;
                            self.sys.dirty = true;
                        }
                    }
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    /// Handle sidebar toggle (Ctrl+B)
    pub(crate) fn handle_sidebar_toggle(&mut self) {
        if self.session.session_sidebar.is_visible() || !self.is_any_overlay_open() {
            self.session.session_sidebar.toggle();
            self.sys.dirty = true;
        }
    }

    /// Handle brutalist mode toggle (Alt+B)
    pub(crate) fn handle_brutalist_toggle(&mut self) {
        self.sys.renderer_mode = self.sys.renderer_mode.toggled();
        let mode_name = self.sys.renderer_mode.label();
        self.add_system_message(format!("✓ Switched to {} mode", mode_name));
        self.sys.dirty = true;
    }

    /// Handle session navigation (Ctrl+Shift+N/P/S)
    pub(crate) fn handle_session_navigation(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('N') => {
                if self.session.session_sidebar.next_session() {
                    if let Some(session) = self.session.session_sidebar.selected_session() {
                        self.add_system_message(format!("📌 Selected session: {}", session.id));
                    }
                } else {
                    self.add_system_message("⚠ No more sessions".to_string());
                }
                self.sys.dirty = true;
            }
            KeyCode::Char('P') => {
                if self.session.session_sidebar.prev_session() {
                    if let Some(session) = self.session.session_sidebar.selected_session() {
                        self.add_system_message(format!("📌 Selected session: {}", session.id));
                    }
                } else {
                    self.add_system_message("⚠ No previous sessions".to_string());
                }
                self.sys.dirty = true;
            }
            KeyCode::Char('S') => {
                if let Some(session_id) = self
                    .session
                    .session_sidebar
                    .selected_session()
                    .map(|s| s.id.clone())
                {
                    // Load the selected session directly (synchronous file I/O)
                    match crate::services::session::load_session(&session_id) {
                        Ok((name, serialized_messages, age)) => {
                            let msg_count = serialized_messages.len();
                            let messages: Vec<crate::ui::message::Message> = serialized_messages
                                .into_iter()
                                .map(|sm| {
                                    let role = match sm.role {
                                        crate::services::session::SerializedMessageType::User => {
                                            crate::ui::message::MessageRole::User
                                        }
                                        crate::services::session::SerializedMessageType::AI => {
                                            crate::ui::message::MessageRole::Assistant
                                        }
                                        crate::services::session::SerializedMessageType::System => {
                                            crate::ui::message::MessageRole::System
                                        }
                                        crate::services::session::SerializedMessageType::Tool => {
                                            crate::ui::message::MessageRole::System
                                        }
                                    };
                                    crate::ui::message::Message::new(role, sm.content)
                                })
                                .collect();

                            // Stop background stream BEFORE resetting state.
                            // If we reset first, is_streaming gets cleared and the
                            // background task keeps sending stale chunks that corrupt
                            // the newly loaded session's messages.
                            let was_streaming = self.session.streaming.is_streaming;
                            if was_streaming {
                                self.integration.services.submit_op(Op::StopStream).ok();
                                self.session.streaming.stream_cancelled = true;
                            }
                            // Apply LoadSession effect inline (private method workaround)
                            self.reset_conversation_state();
                            self.session.undo.clear();
                            self.session.messages = messages;
                            if !self.session.messages.is_empty() {
                                self.ui.view.selected_message = self.session.messages.len() - 1;
                            }
                            self.add_system_message(format!(
                                "Loaded session '{}' — resumed from {} ({} messages)",
                                name, age, msg_count
                            ));
                            self.session.session_sidebar.hide();
                        }
                        Err(e) => {
                            self.add_system_message(format!(
                                "Failed to load session '{}': {}",
                                session_id, e
                            ));
                        }
                    }
                } else {
                    self.add_system_message("No session selected".to_string());
                }
                self.sys.dirty = true;
            }
            _ => {}
        }
    }

    /// Handle search toggle (Ctrl+F)
    pub(crate) fn handle_search_toggle(&mut self) {
        if self.search.search_state.visible {
            self.search.search_state.clear();
        } else if !self.is_any_overlay_open() {
            self.search.search_state.visible = true;
            self.search.search_state.query.clear();
            self.search.search_state.matches.clear();
            self.search.search_state.current_match_index = 0;
        }
        self.sys.dirty = true;
    }

    /// Handle tool panel toggle (Ctrl+P)
    pub(crate) fn handle_tool_panel_toggle(&mut self) {
        if self.panels.tool_panel.showing_tool_panel || !self.is_any_overlay_open() {
            self.panels.tool_panel.showing_tool_panel = !self.panels.tool_panel.showing_tool_panel;
            self.sys.dirty = true;
        }
    }

    /// Handle team agent timeline toggle (Ctrl+G)
    pub(crate) fn handle_team_panel_toggle(&mut self) {
        if self.team.team_panel.visible || !self.is_any_overlay_open() {
            self.team.team_panel.toggle();
            self.sys.dirty = true;
        }
    }

    /// Handle worker status panel toggle (Ctrl+W)
    pub(crate) fn handle_worker_panel_toggle(&mut self) {
        if self.team.worker_panel.visible || !self.is_any_overlay_open() {
            self.team.worker_panel.toggle();
            self.sys.dirty = true;
        }
    }

    pub(crate) fn handle_task_dashboard_toggle(&mut self) {
        if self.model.show_task_dashboard || !self.is_any_overlay_open() {
            self.model.show_task_dashboard = !self.model.show_task_dashboard;
            self.sys.dirty = true;
        }
    }

    /// Handle theme preview input
    pub(crate) fn handle_theme_preview_input(&mut self, key: KeyEvent) -> bool {
        if self.theme.theme_preview.is_visible() {
            return self.theme.theme_preview.handle_key(key);
        }
        false
    }

    /// Handle model selector input
    pub(crate) fn handle_model_selector_input(&mut self, key: KeyEvent) -> bool {
        if self.overlays.model_selector.is_visible() {
            return self.overlays.model_selector.handle_key(key);
        }
        false
    }

    /// Handle plugin manager input.
    pub(crate) fn handle_plugin_manager_input(&mut self, key: KeyEvent) -> bool {
        if !self.overlays.showing_plugin_manager {
            return false;
        }

        let handled = self
            .ui
            .plugin_manager_ui
            .handle_key(key, &self.sys.plugin_manager);
        if !self.ui.plugin_manager_ui.is_visible() {
            self.overlays.showing_plugin_manager = false;
        }
        if handled {
            self.sys.dirty = true;
        }
        handled
    }

    /// Handle marketplace browser input.
    pub(crate) fn handle_marketplace_browser_input(&mut self, key: KeyEvent) -> bool {
        if !self.overlays.showing_marketplace_browser {
            return false;
        }

        let handled = self.ui.marketplace_browser.handle_key(key);
        if let Some(action) = self.ui.marketplace_browser.take_pending_action() {
            self.handle_marketplace_browser_action(action);
        }
        if !self.ui.marketplace_browser.is_visible() {
            self.overlays.showing_marketplace_browser = false;
        }
        if handled {
            self.sys.dirty = true;
        }
        handled
    }

    fn handle_marketplace_browser_action(
        &mut self,
        action: crate::ui::marketplace_browser::MarketplaceBrowserAction,
    ) {
        let Some(command_tx) = self.integration.services.command_sender() else {
            self.add_system_message("Marketplace action could not start".to_string());
            return;
        };

        match action {
            crate::ui::marketplace_browser::MarketplaceBrowserAction::Install(item_id) => {
                let Some(item) = self.ui.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let result = rustycode_shared_runtime::block_on_shared(
                        crate::marketplace::installer::install_item(&item),
                    );
                    match result {
                        Ok(_) => {
                            let _ =
                                command_tx.send(crate::app::async_::SlashCommandResult::Success(
                                    format!("✓ Installed {}", item.name),
                                ));
                        }
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to install {}: {}", item.name, e),
                            ));
                        }
                    }
                });
            }
            crate::ui::marketplace_browser::MarketplaceBrowserAction::Uninstall(item_id) => {
                let Some(item) = self.ui.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let result = rustycode_shared_runtime::block_on_shared(
                        crate::marketplace::installer::uninstall_item(&item),
                    );
                    match result {
                        Ok(_) => {
                            let _ =
                                command_tx.send(crate::app::async_::SlashCommandResult::Success(
                                    format!("✓ Uninstalled {}", item.name),
                                ));
                        }
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to uninstall {}: {}", item.name, e),
                            ));
                        }
                    }
                });
            }
            crate::ui::marketplace_browser::MarketplaceBrowserAction::Update(item_id) => {
                let Some(item) = self.ui.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let result = rustycode_shared_runtime::block_on_shared(
                        crate::marketplace::updates::update_item(
                            std::slice::from_ref(&item),
                            &item.id,
                        ),
                    );
                    match result {
                        Ok(true) => {
                            let _ =
                                command_tx.send(crate::app::async_::SlashCommandResult::Success(
                                    format!("✓ Updated {}", item.name),
                                ));
                        }
                        Ok(false) => {
                            let _ =
                                command_tx.send(crate::app::async_::SlashCommandResult::Success(
                                    format!("{} is already up to date", item.name),
                                ));
                        }
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to update {}: {}", item.name, e),
                            ));
                        }
                    }
                });
            }
        }
    }

    /// Toggle file finder overlay (Ctrl+O)
    pub(crate) fn handle_file_finder_toggle(&mut self) {
        if self.search.file_finder.is_visible() || !self.is_any_overlay_open() {
            self.search.file_finder.toggle();
            self.sys.dirty = true;
        }
    }

    /// Handle file finder input when visible.
    /// Returns true if the key was consumed.
    pub(crate) fn handle_file_finder_input(&mut self, key: KeyEvent) -> bool {
        if !self.search.file_finder.is_visible() {
            return false;
        }

        // Let the file finder process the key
        let handled = self.search.file_finder.handle_key(key);

        // Check if a file was selected
        if let Some(file) = self.search.file_finder.take_selected() {
            // Insert the selected file path into the input
            let path_str = file.path.to_string_lossy();
            for c in path_str.chars() {
                self.ui.input_handler.state.insert_char(c);
            }
            self.ui.input_handler.state.insert_char(' ');
            self.sys.input_mode = self.ui.input_handler.state.mode;
            self.search.file_finder.hide();
            self.add_system_message(format!("Selected: {}", file.path.display()));
        }

        self.sys.dirty = true;
        handled
    }
}
