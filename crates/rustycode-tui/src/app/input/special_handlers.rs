//! Special input state handlers
//!
//! Handles wizard, approval dialogs, clarification panels, and other modal states.

use crate::app::event_loop::TUI;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl TUI {
    /// Handle wizard input
    pub(crate) fn handle_wizard_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.wizard.showing_wizard {
            return Ok(false);
        }

        if let Some(ref mut wizard) = self.wizard.wizard {
            match wizard.handle_key_event(key) {
                crate::ui::wizard::WizardAction::Continue => {
                    self.dirty = true;
                    // Check if wizard is complete
                    if wizard.step == crate::ui::wizard::WizardStep::Complete {
                        self.wizard.showing_wizard = false;
                    }
                }
                crate::ui::wizard::WizardAction::Finish => {
                    self.wizard.showing_wizard = false;
                    self.dirty = true;
                }
                crate::ui::wizard::WizardAction::Quit => {
                    self.wizard.showing_wizard = false;
                    self.running = false;
                    self.dirty = true;
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn handle_approval_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.awaiting_approval {
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
                    self.dirty = true;
                    Ok(true)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    crate::app::confirmation::deliver(&req_id, false);
                    self.add_system_message("✗ Cancelled action".to_string());
                    self.dirty = true;
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
                if let Some(req) = self.pending_approval_request.pop_front() {
                    self.add_system_message(format!("✓ Approved: {}", req.tool_name));
                    self.tool_approval.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::Approved,
                    );
                    self.services.send_approval_response(true);
                }
                self.awaiting_approval = !self.pending_approval_request.is_empty();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Char('n') => {
                if let Some(req) = self.pending_approval_request.pop_front() {
                    self.add_system_message(format!("✗ Rejected: {}", req.tool_name));
                    self.tool_approval.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::Rejected,
                    );
                    self.services.send_approval_response(false);
                }
                self.awaiting_approval = !self.pending_approval_request.is_empty();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Char('N') => {
                if let Some(req) = self.pending_approval_request.pop_front() {
                    self.add_system_message(format!(
                        "✗ Blocked for session: {} (won't ask again)",
                        req.tool_name
                    ));
                    self.tool_approval.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::RejectedAll,
                    );
                    self.services.send_approval_response(false);
                }
                self.awaiting_approval = !self.pending_approval_request.is_empty();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                if let Some(req) = self.pending_approval_request.pop_front() {
                    self.add_system_message(format!("✓ Always approved: {}", req.tool_name));
                    self.tool_approval.record_approval(
                        req.tool_name.clone(),
                        crate::tool_approval::ApprovalState::ApprovedAll,
                    );
                    self.services.send_approval_response(true);
                }
                self.awaiting_approval = !self.pending_approval_request.is_empty();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Esc => {
                self.pending_approval_request.pop_front();
                self.services.send_approval_response(false);
                self.add_system_message("⏸️  Approval cancelled".to_string());
                self.awaiting_approval = !self.pending_approval_request.is_empty();
                self.dirty = true;
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    /// Handle error display input
    pub(crate) fn handle_error_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.showing_error || !self.error_manager.is_showing() {
            return Ok(false);
        }

        match key.code {
            KeyCode::Enter => {
                // Dismiss error
                self.error_manager.dismiss();
                self.showing_error = false;
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Toggle details
                self.error_manager.toggle_details();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Esc => {
                // Also dismiss on Escape
                self.error_manager.dismiss();
                self.showing_error = false;
                self.dirty = true;
                Ok(true)
            }
            _ => {
                // Any other key dismisses the error (don't trap the user)
                self.error_manager.dismiss();
                self.showing_error = false;
                self.dirty = true;
                Ok(true)
            }
        }
    }

    /// Handle clarification question input
    pub(crate) fn handle_clarification_input(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.awaiting_clarification {
            return Ok(false);
        }

        match key.code {
            KeyCode::Enter => {
                // If current question has options, select highlighted option
                if self.clarification_panel.current_has_options()
                    && self.clarification_panel.current_answer().is_empty()
                {
                    self.clarification_panel.select_current_option();
                }

                // Submit all answers if all questions are answered
                if self.clarification_panel.all_answered() {
                    // Build the answer (for single question, just send the answer)
                    let answer = if self.clarification_panel.questions.len() == 1 {
                        // Single question - send just the answer
                        self.clarification_panel.current_answer().to_string()
                    } else {
                        // Multiple questions - build formatted response
                        let mut response = String::new();
                        for (i, question) in self.clarification_panel.questions.iter().enumerate() {
                            if let Some(answer) = self.clarification_panel.answers.get(i) {
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
                    self.services.send_question_response(answer);

                    // Reset clarification state
                    self.clarification_panel.reset();
                    self.awaiting_clarification = false;
                    self.add_system_message("✓ Answer submitted".to_string());
                } else {
                    self.add_system_message(format!(
                        "Please answer all {} questions first",
                        self.clarification_panel.questions.len()
                            - self.clarification_panel.answered_count()
                    ));
                }
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                // Navigate to previous question
                self.clarification_panel.select_previous();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                // Navigate to next question
                self.clarification_panel.select_next();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                // Navigate options left (for option-based questions)
                if self.clarification_panel.current_has_options() {
                    self.clarification_panel.select_previous_option();
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                // Navigate options right (for option-based questions)
                if self.clarification_panel.current_has_options() {
                    self.clarification_panel.select_next_option();
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Tab => {
                // Tab selects the currently highlighted option
                if self.clarification_panel.current_has_options() {
                    self.clarification_panel.select_current_option();
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyCode::Esc => {
                // Cancel clarification
                self.clarification_panel.reset();
                self.awaiting_clarification = false;
                self.add_system_message("⏸️  Clarification cancelled".to_string());
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Char(c) => {
                // Typing - only allow for free-text questions (no options)
                if !self.clarification_panel.current_has_options() {
                    let current_answer = self.clarification_panel.current_answer().to_string();
                    let new_answer = format!("{}{}", current_answer, c);
                    self.clarification_panel.set_current_answer(new_answer);
                }
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Backspace => {
                // Delete last character from current answer (free-text only)
                if !self.clarification_panel.current_has_options() {
                    let current_answer = self.clarification_panel.current_answer().to_string();
                    let new_answer: String = current_answer
                        .chars()
                        .take(current_answer.chars().count().saturating_sub(1))
                        .collect();
                    self.clarification_panel.set_current_answer(new_answer);
                }
                self.dirty = true;
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
        if !self.tool_panel.showing_tool_panel {
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers == crossterm::event::KeyModifiers::CONTROL => {
                // Cancel selected tool
                if let Some(idx) = self.tool_panel.tool_panel_selected_index {
                    if idx < self.tool_panel.tool_panel_history.len() {
                        // Only cancel running tools
                        if self.tool_panel.tool_panel_history[idx].status
                            == crate::ui::message::ToolStatus::Running
                        {
                            self.tool_panel.tool_panel_history[idx].cancel();
                            self.add_system_message(format!(
                                "⚠ Cancelled tool: {}",
                                self.tool_panel.tool_panel_history[idx].name
                            ));
                            self.dirty = true;
                            return Ok(true);
                        }
                    }
                }
                // If no running tool selected, show message
                self.add_system_message("⚠ No running tool selected to cancel".to_string());
                self.dirty = true;
                return Ok(true);
            }
            KeyCode::Esc => {
                if self.tool_panel.showing_tool_result {
                    // Close detailed result view
                    self.tool_panel.showing_tool_result = false;
                    self.tool_panel.tool_result_show_full = false;
                    self.tool_panel.tool_panel_selected_index = None;
                    self.dirty = true;
                } else {
                    // Close tool panel
                    self.tool_panel.showing_tool_panel = false;
                    self.tool_panel.tool_panel_selected_index = None;
                    self.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Up => {
                if !self.tool_panel.tool_panel_history.is_empty() {
                    let current = self.tool_panel.tool_panel_selected_index.unwrap_or(0);
                    self.tool_panel.tool_panel_selected_index = Some(current.saturating_sub(1));
                    self.tool_panel.showing_tool_result = false;
                    self.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Down => {
                if !self.tool_panel.tool_panel_history.is_empty() {
                    let current = self.tool_panel.tool_panel_selected_index.unwrap_or(0);
                    let max_idx = self.tool_panel.tool_panel_history.len().saturating_sub(1);
                    self.tool_panel.tool_panel_selected_index = Some((current + 1).min(max_idx));
                    self.tool_panel.showing_tool_result = false;
                    self.dirty = true;
                }
                return Ok(true);
            }
            KeyCode::Enter => {
                if let Some(idx) = self.tool_panel.tool_panel_selected_index {
                    if idx < self.tool_panel.tool_panel_history.len() {
                        let tool = &self.tool_panel.tool_panel_history[idx];
                        // Show detail view if there's detailed output OR a non-empty result summary
                        let has_content =
                            tool.detailed_output.is_some() || !tool.result_summary.is_empty();
                        if has_content {
                            self.tool_panel.showing_tool_result = true;
                            self.tool_panel.tool_result_show_full = false;
                            self.tool_panel.tool_result_scroll_offset = 0;
                            self.dirty = true;
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
        self.session_sidebar.toggle();
        self.dirty = true;
    }

    /// Handle brutalist mode toggle (Alt+B)
    pub(crate) fn handle_brutalist_toggle(&mut self) {
        self.renderer_mode = self.renderer_mode.toggled();
        let mode_name = self.renderer_mode.label();
        self.add_system_message(format!("✓ Switched to {} mode", mode_name));
        self.dirty = true;
    }

    /// Handle session navigation (Ctrl+Shift+N/P/S)
    pub(crate) fn handle_session_navigation(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('N') => {
                if self.session_sidebar.next_session() {
                    if let Some(session) = self.session_sidebar.selected_session() {
                        self.add_system_message(format!("📌 Selected session: {}", session.id));
                    }
                } else {
                    self.add_system_message("⚠ No more sessions".to_string());
                }
                self.dirty = true;
            }
            KeyCode::Char('P') => {
                if self.session_sidebar.prev_session() {
                    if let Some(session) = self.session_sidebar.selected_session() {
                        self.add_system_message(format!("📌 Selected session: {}", session.id));
                    }
                } else {
                    self.add_system_message("⚠ No previous sessions".to_string());
                }
                self.dirty = true;
            }
            KeyCode::Char('S') => {
                if let Some(session_id) = self
                    .session_sidebar
                    .selected_session()
                    .map(|s| s.id.clone())
                {
                    // Load the selected session directly (synchronous file I/O)
                    match crate::session::load_session(&session_id) {
                        Ok((name, serialized_messages, age)) => {
                            let msg_count = serialized_messages.len();
                            let messages: Vec<crate::ui::message::Message> = serialized_messages
                                .into_iter()
                                .map(|sm| {
                                    let role = match sm.role {
                                        crate::session::SerializedMessageType::User => {
                                            crate::ui::message::MessageRole::User
                                        }
                                        crate::session::SerializedMessageType::AI => {
                                            crate::ui::message::MessageRole::Assistant
                                        }
                                        crate::session::SerializedMessageType::System => {
                                            crate::ui::message::MessageRole::System
                                        }
                                        crate::session::SerializedMessageType::Tool => {
                                            crate::ui::message::MessageRole::System
                                        }
                                    };
                                    crate::ui::message::Message::new(role, sm.content)
                                })
                                .collect();

                            // Apply LoadSession effect inline (private method workaround)
                            self.reset_conversation_state();
                            // Stop background stream before resetting state (prevent stale chunks)
                            if self.streaming.is_streaming {
                                self.services.request_stop_stream();
                            }
                            self.file_undo_stack.clear();
                            self.undo_stack.clear();
                            self.messages = messages;
                            if !self.messages.is_empty() {
                                self.selected_message = self.messages.len() - 1;
                            }
                            self.add_system_message(format!(
                                "Loaded session '{}' — resumed from {} ({} messages)",
                                name, age, msg_count
                            ));
                            self.session_sidebar.hide();
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
                self.dirty = true;
            }
            _ => {}
        }
    }

    /// Handle search toggle (Ctrl+F)
    pub(crate) fn handle_search_toggle(&mut self) {
        if self.search_state.visible {
            // Already visible — close search
            self.search_state.clear();
        } else {
            self.search_state.visible = true;
            self.search_state.query.clear();
            self.search_state.matches.clear();
            self.search_state.current_match_index = 0;
        }
        self.dirty = true;
    }

    /// Handle tool panel toggle (Ctrl+P)
    pub(crate) fn handle_tool_panel_toggle(&mut self) {
        self.tool_panel.showing_tool_panel = !self.tool_panel.showing_tool_panel;
        self.dirty = true;
    }

    /// Handle team agent timeline toggle (Ctrl+G)
    pub(crate) fn handle_team_panel_toggle(&mut self) {
        self.team_panel.toggle();
        self.dirty = true;
    }

    /// Handle worker status panel toggle (Ctrl+W)
    pub(crate) fn handle_worker_panel_toggle(&mut self) {
        self.worker_panel.toggle();
        self.dirty = true;
    }

    /// Handle theme preview input
    pub(crate) fn handle_theme_preview_input(&mut self, key: KeyEvent) -> bool {
        if self.theme_preview.is_visible() {
            return self.theme_preview.handle_key(key);
        }
        false
    }

    /// Handle model selector input
    pub(crate) fn handle_model_selector_input(&mut self, key: KeyEvent) -> bool {
        if self.model_selector.is_visible() {
            return self.model_selector.handle_key(key);
        }
        false
    }

    /// Handle plugin manager input.
    pub(crate) fn handle_plugin_manager_input(&mut self, key: KeyEvent) -> bool {
        if !self.showing_plugin_manager {
            return false;
        }

        let handled = self.plugin_manager_ui.handle_key(key, &self.plugin_manager);
        if !self.plugin_manager_ui.is_visible() {
            self.showing_plugin_manager = false;
        }
        if handled {
            self.dirty = true;
        }
        handled
    }

    /// Handle marketplace browser input.
    pub(crate) fn handle_marketplace_browser_input(&mut self, key: KeyEvent) -> bool {
        if !self.showing_marketplace_browser {
            return false;
        }

        let handled = self.marketplace_browser.handle_key(key);
        if let Some(action) = self.marketplace_browser.take_pending_action() {
            self.handle_marketplace_browser_action(action);
        }
        if !self.marketplace_browser.is_visible() {
            self.showing_marketplace_browser = false;
        }
        if handled {
            self.dirty = true;
        }
        handled
    }

    fn handle_marketplace_browser_action(
        &mut self,
        action: crate::ui::marketplace_browser::MarketplaceBrowserAction,
    ) {
        let Some(command_tx) = self.services.command_sender() else {
            self.add_system_message("Marketplace action could not start".to_string());
            return;
        };

        match action {
            crate::ui::marketplace_browser::MarketplaceBrowserAction::Install(item_id) => {
                let Some(item) = self.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to create runtime: {}", e),
                            ));
                            return;
                        }
                    };
                    let result = rt.block_on(crate::marketplace::installer::install_item(&item));
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
                let Some(item) = self.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to create runtime: {}", e),
                            ));
                            return;
                        }
                    };
                    let result = rt.block_on(crate::marketplace::installer::uninstall_item(&item));
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
                let Some(item) = self.marketplace_browser.selected_item() else {
                    self.add_system_message(format!("Marketplace item not found: {}", item_id));
                    return;
                };
                let item = item.clone();
                std::thread::spawn(move || {
                    let rt = match tokio::runtime::Runtime::new() {
                        Ok(rt) => rt,
                        Err(e) => {
                            let _ = command_tx.send(crate::app::async_::SlashCommandResult::Error(
                                format!("Failed to create runtime: {}", e),
                            ));
                            return;
                        }
                    };
                    let result = rt.block_on(crate::marketplace::updates::update_item(
                        std::slice::from_ref(&item),
                        &item.id,
                    ));
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
        self.file_finder.toggle();
        self.dirty = true;
    }

    /// Handle file finder input when visible.
    /// Returns true if the key was consumed.
    pub(crate) fn handle_file_finder_input(&mut self, key: KeyEvent) -> bool {
        if !self.file_finder.is_visible() {
            return false;
        }

        // Let the file finder process the key
        let handled = self.file_finder.handle_key(key);

        // Check if a file was selected
        if let Some(file) = self.file_finder.take_selected() {
            // Insert the selected file path into the input
            let path_str = file.path.to_string_lossy();
            for c in path_str.chars() {
                self.input_handler.state.insert_char(c);
            }
            self.input_handler.state.insert_char(' ');
            self.input_mode = self.input_handler.state.mode;
            self.file_finder.hide();
            self.add_system_message(format!("Selected: {}", file.path.display()));
        }

        self.dirty = true;
        handled
    }
}
