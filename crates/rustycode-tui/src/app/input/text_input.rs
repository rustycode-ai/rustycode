//! Text input and composition handling
//!
//! Handles text input, search box, command palette, and special input modes.

use crate::app::event_loop::TUI;
use crate::session::save_command_history;
use crate::ui::input::InputAction;
use crate::ui::message_search::SearchEngine;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

impl TUI {
    /// Handle search box input
    pub(crate) fn handle_search_input(&mut self, key_code: KeyCode) -> Result<bool> {
        if !self.search_state.visible {
            return Ok(false);
        }

        match key_code {
            KeyCode::Esc => {
                self.search_state.clear();
                self.dirty = true;
            }
            KeyCode::Enter => {
                // Navigate to next match on Enter
                self.search_state.next_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Up => {
                // Navigate to previous match with Up arrow
                self.search_state.prev_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Down => {
                // Navigate to next match with Down arrow
                self.search_state.next_match();
                self.scroll_to_current_search_match();
                self.dirty = true;
            }
            KeyCode::Char(c) => {
                SearchEngine::add_char(&mut self.search_state, c);
                // Perform search with updated query
                let case_sensitive = self.search_state.case_sensitive;
                let role_filter = self.search_state.role_filter.clone();
                self.search_state.matches = SearchEngine::search(
                    &self.search_state.query,
                    &self.messages,
                    case_sensitive,
                    &role_filter,
                );
                SearchEngine::reset_match_position(&mut self.search_state);
                self.dirty = true;
            }
            KeyCode::Backspace => {
                SearchEngine::backspace(&mut self.search_state);
                // Perform search with updated query
                let case_sensitive = self.search_state.case_sensitive;
                let role_filter = self.search_state.role_filter.clone();
                self.search_state.matches = SearchEngine::search(
                    &self.search_state.query,
                    &self.messages,
                    case_sensitive,
                    &role_filter,
                );
                SearchEngine::reset_match_position(&mut self.search_state);
                self.dirty = true;
            }
            _ => {
                // Ignore other keys
                return Ok(true);
            }
        }
        Ok(true)
    }

    /// Handle command palette navigation
    pub(crate) fn handle_command_palette_input(
        &mut self,
        key_code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Result<bool> {
        if !self.showing_command_palette {
            return Ok(false);
        }
        tracing::warn!("PALETTE INPUT: {:?} {:?}", key_code, modifiers);

        match (key_code, modifiers) {
            (KeyCode::Esc, _) => {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                // Only clear the '/' prefix, preserve any other text the user typed
                let current = self.input_handler.state.all_text();
                if let Some(rest) = current.strip_prefix('/') {
                    self.input_handler.state.clear();
                    for c in rest.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Enter, _) => {
                // Insert selected command into input and submit
                if let Some(command) = self.command_palette.state().selected_command() {
                    let cmd_name = command.name.clone();
                    let needs_args = !command.argument_hint.is_empty();
                    self.input_handler.state.clear();
                    for c in cmd_name.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    if needs_args {
                        self.input_handler.state.insert_char(' ');
                        self.command_palette.state_mut().clear_query();
                        self.dirty = true;
                        self.showing_command_palette = false;
                        self.command_palette.hide();
                        self.input_mode = self.input_handler.state.mode;
                        return Ok(true);
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                // Close palette silently — normal dispatch handles the typed command.
                // Avoids spurious "No matching command found" before actual dispatch.
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                // Return false to allow command submission
                Ok(false)
            }
            (KeyCode::Tab, m) if m.contains(KeyModifiers::CONTROL) => {
                self.command_palette.state_mut().next_tab();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::BackTab, _) => {
                self.command_palette.state_mut().prev_tab();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Tab, _) => {
                // Insert selected command into input, keep palette for args
                if let Some(command) = self.command_palette.state().selected_command() {
                    let cmd_name = command.name.clone();
                    let has_hint = !command.argument_hint.is_empty();
                    self.input_handler.state.clear();
                    for c in cmd_name.chars() {
                        self.input_handler.state.insert_char(c);
                    }
                    if has_hint {
                        // Add space after command for argument typing
                        self.input_handler.state.insert_char(' ');
                    }
                    self.input_mode = self.input_handler.state.mode;
                }
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::PageUp, _) => {
                self.command_palette.state_mut().page_up();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::PageDown, _) => {
                self.command_palette.state_mut().page_down();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Home, _) => {
                self.command_palette.state_mut().home();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::End, _) => {
                self.command_palette.state_mut().end();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Up, _) => {
                self.command_palette.state_mut().move_up();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Down, _) => {
                self.command_palette.state_mut().move_down();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Char(c), m)
                if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT =>
            {
                self.command_palette.state_mut().insert_char(c);
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Backspace, KeyModifiers::NONE) => {
                self.command_palette.state_mut().backspace();
                self.dirty = true;
                Ok(true)
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Handle skill palette input
    pub(crate) fn handle_skill_palette_input(
        &mut self,
        key_code: KeyCode,
        key: crossterm::event::KeyEvent,
    ) -> Result<bool> {
        if !self.showing_skill_palette {
            return Ok(false);
        }

        match key_code {
            KeyCode::Esc => {
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.dirty = true;
                Ok(true)
            }
            KeyCode::Enter => {
                if let Some(skill) = self.skill_palette.take_selected() {
                    self.insert_skill_mention(&skill.name);
                    self.add_system_message(format!("Selected skill: {}", skill.name));
                }
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.dirty = true;
                Ok(true)
            }
            _ => {
                if self.skill_palette.handle_key(key) {
                    self.dirty = true;
                }
                Ok(true)
            }
        }
    }

    /// Process send message action
    pub(crate) fn process_send_message(&mut self, lines: Vec<String>) -> Result<()> {
        let debug_enabled = crate::logging::is_debug_enabled();
        let send_start = std::time::Instant::now();

        // Queue message if already streaming (goose pattern)
        if self.is_streaming {
            let queue_start = std::time::Instant::now();
            let text = lines.join("\n");
            let join_elapsed = queue_start.elapsed();
            if !text.trim().is_empty() {
                let queue_store_start = std::time::Instant::now();
                let mut replaced = false;
                let mut clear_elapsed = std::time::Duration::from_micros(0);
                if self.queued_message.is_some() {
                    // Already have a queued message — offer to replace it
                    self.queued_message = Some(text);
                    self.add_system_message("Replaced queued message".to_string());
                    replaced = true;
                } else {
                    self.queued_message = Some(text);
                    self.add_system_message(
                        "Message queued - will send when generation completes".to_string(),
                    );
                    let clear_start = std::time::Instant::now();
                    self.input_handler.state.clear();
                    self.input_mode = self.input_handler.state.mode;
                    clear_elapsed = clear_start.elapsed();
                }
                if debug_enabled {
                    tracing::debug!(
                        "Queue branch timing: join_us={} store_us={} clear_us={} total_us={} queued_replaced={}",
                        join_elapsed.as_micros(),
                        queue_store_start.elapsed().as_micros(),
                        clear_elapsed.as_micros(),
                        queue_start.elapsed().as_micros(),
                        replaced
                    );
                }
                self.dirty = true;
            }
            return Ok(());
        }

        // Check if we're in rate limit state - if so, check if retry is allowed
        if self.rate_limit.until.is_some() {
            let can_retry = if let Some(until) = self.rate_limit.until {
                until
                    .saturating_duration_since(std::time::Instant::now())
                    .as_secs()
                    == 0
            } else {
                true
            };

            if can_retry {
                // Rate limit expired — clear it and send the new message normally
                self.rate_limit.clear();
            } else {
                let remaining = self
                    .rate_limit
                    .until
                    .map(|t| {
                        t.saturating_duration_since(std::time::Instant::now())
                            .as_secs()
                    })
                    .unwrap_or(0);
                self.add_system_message(format!("⚠️  Rate limit active - wait {}s", remaining));
                self.auto_scroll();
                return Ok(());
            }
        }

        let content = lines.join("\n");
        let has_images = !self.input_handler.state.images.is_empty();
        if content.trim().is_empty() && !has_images {
            return Ok(());
        }

        // Check if this is the first user message BEFORE pushing (for shell history injection)
        let is_first_user_message = !self
            .messages
            .iter()
            .any(|m| matches!(m.role, crate::ui::message::MessageRole::User));

        // Extract images from input state before clearing
        let attached_images: Vec<_> = self.input_handler.state.images.drain(..).collect();
        let is_image_only_submission = content.trim().is_empty() && !attached_images.is_empty();

        const MAX_MESSAGE_LENGTH: usize = 100_000;
        if content.len() > MAX_MESSAGE_LENGTH {
            self.add_system_message(format!(
                "⚠️  Message too long ({} chars). Maximum is {} chars.",
                content.len(),
                MAX_MESSAGE_LENGTH
            ));
            self.auto_scroll();
            return Ok(());
        }

        // Check if task might benefit from team mode
        let team_suggestion_start = std::time::Instant::now();
        let team_suggestion = TUI::evaluate_team_mode_suggestion(&content);
        let team_suggestion_elapsed = team_suggestion_start.elapsed();
        if let Some(suggestion) = team_suggestion {
            self.add_system_message(suggestion);
        }

        // Classify task complexity for orchestration routing
        let classifier_start = std::time::Instant::now();
        let classifier = rustycode_classification::UnifiedTaskClassifier::new();
        let classification = classifier.classify(&content);
        let classifier_elapsed = classifier_start.elapsed();
        tracing::info!(
            complexity_score = classification.complexity_score,
            tier = ?classification.tier,
            agent_role = %classification.agent_role,
            "TUI task classified"
        );

        let history_persist_start = std::time::Instant::now();
        self.input_handler.add_to_history(content.clone());
        let _ = save_command_history(self.input_handler.get_history());
        let history_persist_elapsed = history_persist_start.elapsed();

        let injection_summary_start = std::time::Instant::now();
        let injection_summary = self.get_injection_summary_display(&content);
        let injection_summary_elapsed = injection_summary_start.elapsed();
        if !injection_summary.is_empty() {
            self.add_system_message(injection_summary);
        }

        let prepare_start = std::time::Instant::now();
        let display_content = if is_image_only_submission {
            "[Image attached]".to_string()
        } else {
            content.clone()
        };

        let message = crate::ui::message::Message::user(display_content);
        self.messages.push(message);
        self.dirty = true;
        let prepare_elapsed = prepare_start.elapsed();

        // Show image attachment notification
        if !attached_images.is_empty() {
            let img_count = attached_images.len();
            self.add_system_message(format!(
                "Attached {} image{} to message",
                img_count,
                if img_count > 1 { "s" } else { "" }
            ));
        }

        if !self.messages.is_empty() {
            self.selected_message = self.messages.len() - 1;
            self.scroll_offset_line = 0;
            self.user_scrolled = false;
        }

        self.input_handler.state.clear();
        self.input_mode = self.input_handler.state.mode;

        // Clear search state when sending a message to prevent stale highlighting
        self.search_state.query.clear();
        self.search_state.visible = false;

        if let Some(rest) = content.strip_prefix('!') {
            // Bash mode: execute shell command
            let cmd = rest.trim();
            if !cmd.is_empty() {
                self.add_system_message(format!("$ {}", cmd));
                self.execute_bash_command(cmd);
            }
            self.dirty = true;
            self.auto_scroll();
        } else if content.starts_with('/') {
            if content == "/" {
                // If palette is showing, pick the highlighted command
                if self.showing_command_palette {
                    if let Some(command) = self.command_palette.state().selected_command() {
                        let cmd_name = command.name.clone();
                        self.showing_command_palette = false;
                        self.command_palette.hide();
                        self.command_palette.state_mut().clear_query();
                        // Execute the slash command directly (no need to re-enter)
                        self.input_handler.state.clear();
                        self.input_mode = self.input_handler.state.mode;
                        if let Err(e) = self.handle_slash_command(&cmd_name) {
                            self.add_system_message(format!("Command failed: {}", e));
                        }
                        self.dirty = true;
                        return Ok(());
                    }
                }
                // No palette or no selection — just show it
                self.showing_command_palette = true;
                self.command_palette.show();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
                return Ok(());
            }

            let content_clone = content.clone();
            if let Err(e) = self.handle_slash_command(&content_clone) {
                tracing::error!("Slash command failed: {}", e);
                let err_str = e.to_string();
                let user_msg =
                    if err_str.contains("not found") || err_str.contains("Unknown command") {
                        "Unknown command. Type /help for available commands."
                    } else {
                        // Show actual error so user can fix it
                        &*format!("Command failed: {}", err_str)
                    };
                self.add_system_message(user_msg.to_string());
            }
            self.dirty = true;
            self.auto_scroll();
        } else {
            let message_to_send = self.prepare_message_for_send(&content);
            let message_to_send = if is_first_user_message {
                self.inject_shell_history_if_first_message(&message_to_send)
            } else {
                message_to_send
            };
            let _workspace_context = self.workspace_context.clone();

            // Build conversation history from existing messages for multi-turn context
            let history_start = std::time::Instant::now();
            let mut history = self.build_conversation_history();
            let history_elapsed = history_start.elapsed();
            if debug_enabled {
                tracing::debug!(
                    "Conversation history built: first_user={} elapsed_ms={} history_len={} visible_messages={} user_turns={}",
                    is_first_user_message,
                    history_elapsed.as_millis(),
                    history.len(),
                    self.messages.len(),
                    self.messages
                        .iter()
                        .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                        .count()
                );
            }

            // If images were attached, replace the last user message (which has text-only content)
            // with a multi-content message that includes image blocks
            if !attached_images.is_empty() {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
                use rustycode_llm::provider::{ContentBlock, ImageSource};
                use rustycode_protocol::MessageContent;

                let mut blocks = vec![ContentBlock::text(&message_to_send)];

                for img in &attached_images {
                    match std::fs::read(&img.path) {
                        Ok(bytes) => {
                            let b64 = BASE64.encode(&bytes);
                            blocks.push(ContentBlock::image(ImageSource::base64(
                                &img.mime_type,
                                b64,
                            )));
                            tracing::info!(
                                "Attached image: {} ({} bytes, {})",
                                img.id,
                                bytes.len(),
                                img.mime_type
                            );
                        }
                        Err(e) => {
                            tracing::warn!("Failed to read image {}: {}", img.path.display(), e);
                            blocks.push(ContentBlock::text(format!(
                                "[Image {} could not be loaded: {}]",
                                img.id, e
                            )));
                        }
                    }
                }

                // Replace the last user message in history with the image-enriched version
                if let Some(last_msg) = history.last_mut() {
                    if last_msg.role == rustycode_llm::provider::MessageRole::User {
                        last_msg.content = MessageContent::blocks(blocks);
                    }
                }
            }

            self.rate_limit.last_message = Some(content.clone());

            // Set streaming flag BEFORE sending to prevent double-Enter races.
            // If send fails, we clear it below.
            self.is_streaming = true;
            self.chunks_received = 0;
            self.thinking_chunks_received = 0;
            self.stream_start_time = Some(std::time::Instant::now());
            self.current_stream_content.clear();
            self.streaming_render_buffer =
                crate::app::streaming_render_buffer::StreamingRenderBuffer::new();

            // Clear previous turn's tool history so sidebar doesn't show stale calls.
            self.tool_panel_history.clear();
            self.tool_panel_selected_index = None;
            self.showing_tool_result = false;
            self.active_tools.clear();

            let send_call_start = std::time::Instant::now();
            if let Err(e) = self
                .services
                .send_message_with_history(message_to_send, Some(history))
            {
                tracing::error!("Failed to send message: {}", e);
                self.reset_streaming_state();
                self.active_tools.clear();

                // Keep the user message visible so they see what they typed.
                // Just add an error system message below it.
                let user_msg = if e.to_string().contains("not started") {
                    "⚠️  Service initializing - please try again in a moment".to_string()
                } else {
                    format!("⚠️  Send failed: {} - press Enter to retry", e)
                };
                self.add_system_message(user_msg);
                self.dirty = true;
                self.auto_scroll();
            } else {
                let assistant_msg = crate::ui::message::Message::assistant(String::new());
                self.messages.push(assistant_msg);
                self.dirty = true;
                self.auto_scroll();

                self.rate_limit.clear();
            }

            if debug_enabled {
                crate::debug_log!(
                    "Submit path timing: first_user={} history_ms={} send_ms={} total_ms={} messages={} history_turns={} persist_us={} inject_summary_us={} prepare_push_us={} team_suggestion_us={} classifier_us={}",
                    is_first_user_message,
                    history_elapsed.as_millis(),
                    send_call_start.elapsed().as_millis(),
                    send_start.elapsed().as_millis(),
                    self.messages.len(),
                    self.messages
                        .iter()
                        .filter(|m| matches!(m.role, crate::ui::message::MessageRole::User))
                        .count(),
                    history_persist_elapsed.as_micros(),
                    injection_summary_elapsed.as_micros(),
                    prepare_elapsed.as_micros(),
                    team_suggestion_elapsed.as_micros(),
                    classifier_elapsed.as_micros()
                );
            }
        }

        Ok(())
    }

    /// Handle input action from input handler
    pub(crate) fn handle_input_action(
        &mut self,
        action: InputAction,
        key: crossterm::event::KeyEvent,
    ) -> Result<()> {
        match action {
            InputAction::OpenCommandPalette => {
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.showing_command_palette = true;
                self.command_palette.show();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
            }
            InputAction::OpenSkillPalette => {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.showing_skill_palette = true;
                self.skill_palette.open();
                self.dirty = true;
            }
            InputAction::SendMessage(lines) => {
                self.process_send_message(lines)?;
            }
            InputAction::Consumed => {
                self.input_mode = self.input_handler.state.mode;
                self.dirty = true;
            }
            InputAction::Ignored => {
                self.handle_global_shortcut(key.code, key.modifiers)?;
            }
            InputAction::HistoryPrevious | InputAction::HistoryNext => {
                // History navigation is handled via InputAction::Consumed
                // in the input handler (Up/Down in single-line mode).
                self.dirty = true;
            }
            InputAction::SearchReverse => {
                self.dirty = true;
            }
            InputAction::RemoveImage(_) => {
                self.dirty = true;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::input::ImageAttachment;
    use crate::ui::message::MessageRole;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_image_only_submission_is_not_dropped() {
        let mut tui = TUI::new_for_test();

        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after UNIX_EPOCH")
            .as_nanos();
        let file_path = temp_dir.join(format!("rustycode_image_only_{}.png", unique));
        fs::write(&file_path, b"not-a-real-png-but-fine-for-base64").unwrap();

        tui.input_handler.state.images.push(ImageAttachment {
            id: "img-1".to_string(),
            path: PathBuf::from(&file_path),
            preview: "preview".to_string(),
            mime_type: "image/png".to_string(),
        });

        tui.process_send_message(vec![String::new()]).unwrap();

        assert!(
            tui.messages.iter().any(|msg| msg.role == MessageRole::User),
            "image-only submission should create a user message"
        );
        assert!(
            tui.input_handler.state.images.is_empty(),
            "image attachments should be drained when sending"
        );
    }

    #[test]
    fn test_command_palette_launcher_shortcuts_open_palette() {
        let mut tui = TUI::new_for_test();

        tui.handle_global_shortcut(KeyCode::Char('k'), KeyModifiers::CONTROL)
            .unwrap();
        assert!(tui.showing_command_palette);
        assert!(tui.command_palette.is_visible());

        tui.dismiss_any_overlay();
        assert!(!tui.showing_command_palette);
        assert!(!tui.command_palette.is_visible());

        tui.handle_global_shortcut(
            KeyCode::Char('P'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )
        .unwrap();
        assert!(tui.showing_command_palette);
        assert!(tui.command_palette.is_visible());
    }
}
