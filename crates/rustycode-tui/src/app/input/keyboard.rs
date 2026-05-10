//! Keyboard input handling

use crate::app::event_loop::TUI;
use crate::ui::input::InputMode;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

impl TUI {
    /// Handle global keyboard shortcuts
    pub(crate) fn handle_global_shortcut(
        &mut self,
        key_code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Result<()> {
        let input_is_empty = self.input_handler.state.lines.len() == 1
            && self.input_handler.state.lines[0].is_empty();

        if (key_code == KeyCode::Char('k') && modifiers.contains(KeyModifiers::CONTROL))
            || (key_code == KeyCode::Char('P')
                && modifiers.contains(KeyModifiers::CONTROL)
                && modifiers.contains(KeyModifiers::SHIFT))
        {
            if !self.is_any_overlay_open() {
                self.showing_command_palette = true;
                self.showing_skill_palette = false;
                self.showing_plugin_manager = false;
                self.showing_marketplace_browser = false;
                self.command_palette.show();
                self.command_palette.state_mut().clear_query();
                self.dirty = true;
            }
            return Ok(());
        }

        if key_code == KeyCode::Char('M')
            && modifiers.contains(KeyModifiers::CONTROL)
            && modifiers.contains(KeyModifiers::SHIFT)
        {
            if !self.is_any_overlay_open() {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.showing_plugin_manager = true;
                self.plugin_manager_ui.show();
                {
                    let mut manager = self
                        .plugin_manager
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    let _ = manager.reload_from_disk();
                }
                self.dirty = true;
            }
            return Ok(());
        }

        if key_code == KeyCode::Char('M')
            && modifiers.contains(KeyModifiers::CONTROL)
            && modifiers.contains(KeyModifiers::ALT)
        {
            if !self.is_any_overlay_open() {
                self.showing_command_palette = false;
                self.command_palette.hide();
                self.showing_skill_palette = false;
                self.skill_palette.close();
                self.showing_plugin_manager = false;
                self.plugin_manager_ui.hide();
                self.showing_marketplace_browser = true;
                self.marketplace_browser.open();
                self.dirty = true;
            }
            return Ok(());
        }

        match (key_code, modifiers) {
            // Ctrl+U when input is empty: half-page scroll up (Vim Ctrl+U convention).
            // Must come before Ctrl+Shift+U (undo extraction) to match the simpler pattern.
            // When input has text, Ctrl+U falls through to InputHandler for "clear line".
            (KeyCode::Char('u'), KeyModifiers::CONTROL) if input_is_empty => {
                if !self.messages.is_empty() {
                    self.push_undo_position();
                    self.half_page_up();
                    self.dirty = true;
                }
                return Ok(());
            }
            // Ctrl+D when input is empty and user has scrolled: half-page down (Vim Ctrl+D).
            // Must come before the quit handler to intercept when scrolled.
            (KeyCode::Char('d'), KeyModifiers::CONTROL)
                if input_is_empty && !self.streaming.is_streaming =>
            {
                if self.view.user_scrolled && !self.messages.is_empty() {
                    self.push_undo_position();
                    self.half_page_down();
                    self.dirty = true;
                }
                return Ok(());
            }
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                // Stop any active stream before quitting
                if self.streaming.is_streaming {
                    if self.streaming.stream_cancelled {
                        // Second press during stream — force quit immediately
                        self.streaming.is_streaming = false;
                        self.running = false;
                        self.dirty = true;
                        return Ok(());
                    }
                    self.services.request_stop_stream();
                    self.streaming.stream_cancelled = true;
                    // Let Done handler clean up — then quit on next Ctrl+Q
                    self.add_system_message("Generation stopped - press again to quit".to_string());
                    self.dirty = true;
                    return Ok(());
                }
                self.running = false;
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL)
                if !self.streaming.is_streaming && !input_is_empty =>
            {
                // Ctrl+D with text in input: dismiss overlay if showing one, otherwise do nothing
                if self.tool_panel.showing_tool_result {
                    self.tool_panel.showing_tool_result = false;
                    self.dirty = true;
                }
                return Ok(());
            }
            // Ctrl+Shift+C: Copy selected message (moved from Ctrl+C to match industry convention)
            (KeyCode::Char('C'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                if let Err(e) = self.copy_selected_message() {
                    tracing::error!("Failed to copy message: {}", e);
                    self.add_system_message(format!("[X] Failed to copy: {}", e));
                }
                self.dirty = true;
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                // Cancel/Interrupt - matches Claude Code convention
                if self.streaming.is_streaming {
                    self.services.request_stop_stream();
                    // Don't set is_streaming=false here — let the StreamChunk::Done
                    // handler do it to avoid race with async stream task.
                    // Mark cancelled so Done handler skips auto-continue.
                    self.streaming.stream_cancelled = true;
                    // CLEAR THINKING STATE IMMEDIATELY
                    // Atomically capture and clear thinking so the spinner
                    // doesn't linger. Also clear stream content if it matches
                    // the thinking text (avoids showing stale content).
                    if let Some(thinking) = self.take_last_assistant_thinking() {
                        if !self.streaming.current_stream_content.is_empty()
                            && self.streaming.current_stream_content.trim() == thinking.trim()
                        {
                            self.streaming.current_stream_content.clear();
                        }
                    }

                    let preserved = self.streaming.current_stream_content.len();
                    if preserved > 0 {
                        self.add_system_message(format!(
                            "Generation stopped ({} chars preserved)",
                            preserved
                        ));
                    } else {
                        self.add_system_message("Generation stopped by user".to_string());
                    }
                    // Also clear any queued message — user explicitly stopped
                    if self.streaming.queued_message.take().is_some() {
                        self.add_system_message("Queued message cleared".to_string());
                    }
                } else {
                    // Not streaming: copy input text to clipboard if non-empty,
                    // then clear. If empty, dismiss overlays or show quit hint.

                    // Cancel rate limit auto-retry if active (users naturally press Ctrl+C)
                    if self.rate_limit.until.is_some() {
                        self.rate_limit.cancel_auto_retry();
                        self.add_system_message("Auto-retry cancelled".to_string());
                        self.dirty = true;
                        return Ok(());
                    }

                    // Dismiss any open overlay (same set as Esc handler)
                    if self.dismiss_any_overlay() {
                        // Overlay was dismissed
                    } else {
                        let input_text = self.input_handler.state.all_text();
                        if !input_text.is_empty() {
                            // Copy input to clipboard, then clear
                            if let Err(e) =
                                crate::services::clipboard::copy_text_to_clipboard_both(&input_text)
                            {
                                tracing::error!("Failed to copy input: {}", e);
                                self.add_system_message(format!("[X] Failed to copy: {}", e));
                            } else {
                                let chars = input_text.chars().count();
                                self.add_system_message(format!(
                                    "📋 Copied input ({} chars) to clipboard",
                                    chars
                                ));
                            }
                            self.input_handler.state.clear();
                            self.input_mode = self.input_handler.state.mode;
                        } else {
                            // No overlays and empty input — show quit hint
                            self.add_system_message("Press Ctrl+Q to quit".to_string());
                        }
                    }
                }
                self.dirty = true;
            }
            // Ctrl+Shift+S: Toggle skill palette
            #[allow(unreachable_patterns)]
            (KeyCode::Char('S'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                if !self.is_any_overlay_open() || self.showing_skill_palette {
                    self.showing_command_palette = false;
                    self.command_palette.hide();
                    self.showing_skill_palette = !self.showing_skill_palette;
                    if self.showing_skill_palette {
                        self.skill_palette.open();
                    } else {
                        self.skill_palette.close();
                    }
                    self.dirty = true;
                }
            }
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => {
                // Ctrl+Y: Quick copy last AI response
                if let Err(e) = self.copy_last_ai_response() {
                    tracing::error!("Failed to copy last response: {}", e);
                }
                self.dirty = true;
            }
            // Ctrl+Shift+K: Copy entire conversation
            (KeyCode::Char('K'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                if let Err(e) = self.copy_all_conversation() {
                    tracing::error!("Failed to copy conversation: {}", e);
                }
                self.dirty = true;
            }
            (KeyCode::Char('E'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                if let Err(e) = self.export_conversation() {
                    tracing::error!("Failed to export conversation: {}", e);
                    self.toast_manager.error(format!("Export failed: {}", e));
                } else {
                    self.toast_manager
                        .success("Exported conversation to file".to_string());
                }
                self.dirty = true;
            }
            // Note: Ctrl+R is handled by InputHandler for reverse search (readline standard)
            // Regenerate is on Ctrl+Shift+R to avoid conflict
            (KeyCode::Char('r'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                self.regenerate_last_response()?;
            }
            #[allow(unreachable_patterns)]
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                // Stash/unstash current input prompt
                let current_text = self.input_handler.state.all_text();
                if let Some(stashed) = self.stashed_prompt.take() {
                    // Restore stashed prompt (bulk set instead of char-by-char)
                    self.input_handler.state.set_text(&stashed);
                    self.input_mode = self.input_handler.state.mode;
                    self.toast_manager
                        .success("📝 Restored stashed prompt".to_string());
                } else if !current_text.trim().is_empty() {
                    // Stash current prompt
                    self.stashed_prompt = Some(current_text.clone());
                    self.input_handler.state.clear();
                    self.input_mode = self.input_handler.state.mode;
                    self.toast_manager
                        .success("📝 Prompt stashed - press Ctrl+S again to restore".to_string());
                }
                self.dirty = true;
            }
            (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                // Suspend process (Ctrl+Z) — must restore terminal state first
                // so the parent shell works normally, then re-enter TUI mode on resume.
                #[cfg(unix)]
                {
                    // Leave TUI terminal mode (alternate screen, raw mode)
                    let _ = crossterm::terminal::disable_raw_mode();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::LeaveAlternateScreen,
                        crossterm::event::DisableBracketedPaste,
                        crossterm::event::DisableMouseCapture,
                        crossterm::cursor::Show,
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());

                    // Send SIGTSTP to suspend ourselves
                    use nix::sys::signal::{kill, Signal};
                    use nix::unistd::Pid;
                    use std::process;
                    if let Ok(pid) = process::id().try_into() {
                        let _ = kill(Pid::from_raw(pid), Signal::SIGTSTP);
                    }

                    // After resume (SIGCONT), re-enter TUI terminal mode
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::EnterAlternateScreen,
                        crossterm::event::EnableBracketedPaste,
                        crossterm::event::EnableMouseCapture,
                    );
                    let _ = crossterm::terminal::enable_raw_mode();
                    self.dirty = true;
                }
                #[cfg(not(unix))]
                {
                    self.add_system_message(
                        "Process suspension not supported on this platform".to_string(),
                    );
                }
            }
            // Note: Ctrl+B is handled in event_loop_input.rs (sidebar toggle)
            // Note: Ctrl+F is handled in event_loop_input.rs (search toggle)
            // Line-by-line scroll with Up/Down when input empty (Claude Code pattern)
            (KeyCode::Up, KeyModifiers::NONE) if input_is_empty && !self.messages.is_empty() => {
                self.push_undo_position();
                self.scroll_up();
                self.dirty = true;
            }
            (KeyCode::Down, KeyModifiers::NONE) if input_is_empty && !self.messages.is_empty() => {
                self.push_undo_position();
                self.scroll_down();
                self.dirty = true;
            }
            // Turn-based navigation: Shift+Up/Down jumps between user messages
            (KeyCode::Up, KeyModifiers::SHIFT) if !self.messages.is_empty() => {
                self.push_undo_position();
                self.navigate_to_prev_turn();
                self.dirty = true;
            }
            (KeyCode::Down, KeyModifiers::SHIFT) if !self.messages.is_empty() => {
                self.push_undo_position();
                self.navigate_to_next_turn();
                self.dirty = true;
            }
            // Full-page scroll: PageUp/PageDown
            (KeyCode::PageUp, KeyModifiers::NONE) if input_is_empty => {
                self.push_undo_position();
                self.page_up();
            }
            (KeyCode::PageDown, KeyModifiers::NONE) if input_is_empty => {
                self.push_undo_position();
                self.page_down();
            }
            // Note: Home/End are handled in event_loop.rs pre-filter (always scroll, regardless of input state)
            // Note: Ctrl+G is handled in event_loop_input.rs (team panel toggle)
            // Note: Ctrl+O is handled in event_loop_input.rs (file finder toggle)
            (KeyCode::Char('u'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                // Undo last extraction
                self.undo_last_extraction()?;
                return Ok(());
            }
            (KeyCode::Char('a'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                // Toggle auto-continue mode
                self.auto_continue.toggle();
                if self.auto_continue.is_enabled() {
                    self.add_system_message(
                        "🔄 Auto-continue enabled - AI will work through tasks until complete"
                            .to_string(),
                    );
                    if !self.streaming.is_streaming {
                        self.auto_continue.mark_pending();
                    }
                } else {
                    self.add_system_message("⏸️  Auto-continue disabled".to_string());
                    self.auto_continue.clear_pending();
                }
                self.dirty = true;
                return Ok(());
            }
            (KeyCode::Char('m'), KeyModifiers::ALT) => {
                let new_mode = self.services.next_agent_mode();
                self.add_system_message(format!(
                    "🔧 Agent mode: {} - {}",
                    new_mode.display_name(),
                    new_mode.description()
                ));
                self.dirty = true;
                self.auto_scroll();
                return Ok(());
            }
            #[allow(unreachable_patterns)]
            (KeyCode::Char('M'), KeyModifiers::ALT | KeyModifiers::SHIFT) => {
                let new_mode = self.services.prev_agent_mode();
                self.add_system_message(format!(
                    "🔧 Agent mode: {} - {}",
                    new_mode.display_name(),
                    new_mode.description()
                ));
                self.dirty = true;
                self.auto_scroll();
                return Ok(());
            }
            // Ctrl+Shift+Z: Undo scroll position (jump back to previous position)
            #[allow(unreachable_patterns)]
            (KeyCode::Char('Z'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                if self.pop_undo_position() {
                    self.add_system_message("↩ Jumped back to previous position".to_string());
                } else {
                    self.add_system_message("No scroll position to undo".to_string());
                }
                self.dirty = true;
                return Ok(());
            }
            // Ctrl+Shift+H: Toggle UI section visibility (status bar / footer)
            (KeyCode::Char('h'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
                // Toggle both status bar and footer together for maximum screen real estate
                let new_state = !self.status_bar_collapsed;
                self.status_bar_collapsed = new_state;
                self.footer_collapsed = new_state;
                if new_state {
                    self.add_system_message(
                        "📐 UI sections collapsed - more space for messages".to_string(),
                    );
                } else {
                    self.add_system_message("📐 UI sections restored".to_string());
                }
                self.dirty = true;
                return Ok(());
            }
            // Ctrl+X: Open input in external editor
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => {
                if !self.streaming.is_streaming {
                    let current_text = self.input_handler.state.all_text();
                    match self.edit_in_editor(&current_text) {
                        Ok(edited) => {
                            let trimmed = edited.trim();
                            if !trimmed.is_empty() && trimmed != current_text.trim() {
                                self.input_handler.state.set_text(trimmed);
                                self.input_mode = self.input_handler.state.mode;
                                self.add_system_message(
                                    "📝 Loaded from editor - press Enter to send".to_string(),
                                );
                            } else if trimmed.is_empty() {
                                self.add_system_message(
                                    "Editor returned empty - input unchanged".to_string(),
                                );
                            }
                            self.dirty = true;
                            self.needs_full_redraw = true;
                        }
                        Err(e) => {
                            self.add_system_message(format!("⚠️ Editor error: {}", e));
                            self.dirty = true;
                            self.needs_full_redraw = true;
                        }
                    }
                }
                return Ok(());
            }
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => {
                self.input_handler.state.clear();
                self.dirty = true;
                return Ok(());
            }
            (KeyCode::Esc, KeyModifiers::NONE) => {
                // Esc is the universal dismiss key — close overlays in priority order.
                // Each dismiss consumes the Esc press (no fall-through to lower priority).
                // Only double-Esc (when nothing is open) clears input.

                // Priority 1: Dismiss overlays (most recent/important first)
                if self.dismiss_any_overlay() {
                    self.dirty = true;
                    return Ok(());
                }

                // Priority 2: Cancel active operations
                if self.streaming.is_streaming {
                    self.services.request_stop_stream();
                    // Don't set is_streaming=false here — let the StreamChunk::Done
                    // handler do it. Mark cancelled so Done handler skips auto-continue.
                    self.streaming.stream_cancelled = true;
                    let preserved = self.streaming.current_stream_content.len();
                    if preserved > 0 {
                        self.add_system_message(format!(
                            "Generation stopped ({} chars preserved)",
                            preserved
                        ));
                    } else {
                        self.add_system_message("Generation stopped by user".to_string());
                    }
                    // Also clear any queued message — user explicitly stopped
                    if self.streaming.queued_message.take().is_some() {
                        self.add_system_message("Queued message cleared".to_string());
                    }
                    self.dirty = true;
                    return Ok(());
                }
                // Cancel team orchestrator if running
                if self.team_handler.is_running() {
                    self.cancel_team();
                    return Ok(());
                }
                if self.rate_limit.until.is_some() && !self.rate_limit.auto_retry_cancelled {
                    self.rate_limit.auto_retry_cancelled = true;
                    if let Some(msg_idx) = self.rate_limit.message_index {
                        if let Some(msg) = self.messages.get_mut(msg_idx) {
                            msg.content = format!(
                                "{} (auto-retry cancelled - press Enter to retry)",
                                msg.content.replace("Auto-retrying", "Waiting")
                            );
                        }
                    }
                    self.add_system_message(
                        "⚠️  Auto-retry cancelled - press Enter when ready to retry".to_string(),
                    );
                    self.dirty = true;
                    return Ok(());
                }

                // Priority 3: Switch to single-line display mode (without destroying content)
                if self.input_mode == InputMode::MultiLine {
                    self.input_mode = InputMode::SingleLine;
                    self.input_handler.state.mode = InputMode::SingleLine;
                    self.dirty = true;
                    return Ok(());
                }

                // Priority 4: Double-Esc to clear input (only when nothing else is open)
                let now = std::time::Instant::now();
                if let Some(last_esc) = self.last_esc_press {
                    if now.duration_since(last_esc).as_millis()
                        < crate::app::KEYBOARD_CHORD_TIMEOUT.as_millis()
                    {
                        // Double-Esc: clear input
                        self.input_handler.state.clear();
                        self.input_mode = self.input_handler.state.mode;
                        self.last_esc_press = None;
                        self.dirty = true;
                        return Ok(());
                    }
                }
                self.last_esc_press = Some(now);
            }
            // REMOVED: '?' key handler moved to event_loop.rs (early intercept before InputHandler)
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                if !self.is_any_overlay_open() || self.theme_preview.is_visible() {
                    self.theme_preview.toggle();
                    self.dirty = true;
                }
            }
            (KeyCode::Char('t'), KeyModifiers::ALT) => {
                if let Some(theme) = self.theme_switcher.next_theme() {
                    self.toast_manager.success(format!("Theme: {}", theme.name));
                }
                self.dirty = true;
                self.auto_scroll();
            }
            (KeyCode::Char('T'), KeyModifiers::ALT | KeyModifiers::SHIFT) => {
                if let Some(theme) = self.theme_switcher.prev() {
                    self.toast_manager.success(format!("Theme: {}", theme.name));
                }
                self.dirty = true;
                self.auto_scroll();
            }
            // Vim keybindings (when enabled and input is not focused)
            (KeyCode::Char('j'), KeyModifiers::NONE)
                if self.tui_config.behavior.vim_enabled && input_is_empty =>
            {
                // Push current position to undo stack before moving
                self.push_undo_position();

                let action = self.keyboard_handler.handle_vim_key('j');
                if action == crate::app::keyboard_shortcuts::KeyboardAction::MoveDown {
                    self.scroll_down();
                    self.dirty = true;
                }
            }
            (KeyCode::Char('k'), KeyModifiers::NONE)
                if self.tui_config.behavior.vim_enabled && input_is_empty =>
            {
                // Push current position to undo stack before moving
                self.push_undo_position();

                let action = self.keyboard_handler.handle_vim_key('k');
                if action == crate::app::keyboard_shortcuts::KeyboardAction::MoveUp {
                    self.scroll_up();
                    self.dirty = true;
                }
            }
            (KeyCode::Char('g'), KeyModifiers::NONE)
                if self.tui_config.behavior.vim_enabled && input_is_empty =>
            {
                // Handle 'g' for gg chord detection
                let action = self.keyboard_handler.handle_vim_key('g');
                if action == crate::app::keyboard_shortcuts::KeyboardAction::JumpToStart {
                    // Push current position to undo stack before jumping
                    self.push_undo_position();

                    if !self.messages.is_empty() {
                        self.view.selected_message = 0;
                        self.view.scroll_offset_line = 0;
                        self.view.user_scrolled = true;
                        self.dirty = true;
                    }
                }
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT)
                if self.tui_config.behavior.vim_enabled && input_is_empty =>
            {
                // Jump to end (Vim: shift+G = capital G)
                // Push current position to undo stack before jumping
                self.push_undo_position();

                let action = self.keyboard_handler.handle_vim_key('G');
                if action == crate::app::keyboard_shortcuts::KeyboardAction::JumpToEnd
                    && !self.messages.is_empty()
                {
                    self.view.selected_message = self.messages.len().saturating_sub(1);
                    self.view.scroll_offset_line = 0;
                    self.view.user_scrolled = false;
                    self.auto_scroll();
                    self.dirty = true;
                }
            }
            (KeyCode::Char('p'), KeyModifiers::ALT) => {
                if !self.is_any_overlay_open() {
                    self.model_selector.show();
                    self.dirty = true;
                }
            }
            // Message collapse/expand shortcuts
            (KeyCode::Char('e'), KeyModifiers::ALT) => {
                // Alt+E: Expand all messages
                self.expand_all_messages();
                self.add_system_message("Expanded all messages".to_string());
            }
            (KeyCode::Char('f'), KeyModifiers::ALT) => {
                // Alt+F: Cycle effort level
                let new_effort = self.cycle_effort_level();
                self.add_system_message(format!("Effort: {}", new_effort));
            }
            (KeyCode::Char('w'), KeyModifiers::ALT) => {
                // Alt+W: Collapse all except user messages
                self.collapse_all_except_user();
                self.add_system_message("Collapsed non-user messages".to_string());
            }
            #[allow(unreachable_patterns)]
            (KeyCode::Char('E'), KeyModifiers::ALT | KeyModifiers::SHIFT) => {
                // Alt+Shift+E: Expand all tool blocks
                self.expand_all_tools();
                self.add_system_message("Expanded all tool blocks".to_string());
            }
            (KeyCode::Char('W'), KeyModifiers::ALT | KeyModifiers::SHIFT) => {
                // Alt+Shift+W: Collapse all tool blocks
                self.collapse_all_tools();
                self.add_system_message("Collapsed all tool blocks".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns true if any modal overlay is currently visible.
    /// Used to prevent opening new overlays on top of existing ones.
    /// Persistent panels (sidebar, worker panel, team panel, tool panel) are
    /// excluded since they don't block modal interactions.
    pub(crate) fn is_any_overlay_open(&self) -> bool {
        self.wizard.showing_wizard
            || !self.tool_approval.pending_requests.is_empty()
            || self.showing_error
            || self.awaiting_clarification
            || self.compaction.showing_preview
            || self.model_selector.is_visible()
            || self.showing_provider_selector
            || self.showing_command_palette
            || self.showing_skill_palette
            || self.showing_plugin_manager
            || self.showing_marketplace_browser
            || self.file_finder.is_visible()
            || self.search_state.visible
            || self.theme_preview.is_visible()
            || self.help_state.visible
    }

    /// Dismiss the topmost overlay (if any). Returns true if one was dismissed.
    ///
    /// Used by both Ctrl+C and Esc to ensure consistent overlay dismissal.
    /// Order matches the Esc handler priority.
    pub(crate) fn dismiss_any_overlay(&mut self) -> bool {
        if self.wizard.showing_wizard {
            self.wizard.showing_wizard = false;
            return true;
        }
        if self.tool_panel.showing_tool_result {
            self.tool_panel.showing_tool_result = false;
            self.tool_panel.tool_result_scroll_offset = 0;
            return true;
        }
        if self.awaiting_clarification && self.clarification_panel.visible {
            self.clarification_panel.visible = false;
            self.awaiting_clarification = false;
            self.dirty = true;
            return true;
        }
        if self.compaction.showing_preview {
            self.compaction.showing_preview = false;
            self.compaction.pending = false;
            self.dirty = true;
            return true;
        }
        if self.error_manager.is_showing() {
            self.error_manager.dismiss();
            self.showing_error = false;
            self.dirty = true;
            return true;
        }
        if self.model_selector.is_visible() {
            self.model_selector.hide();
            self.dirty = true;
            return true;
        }
        if self.showing_provider_selector {
            self.showing_provider_selector = false;
            self.dirty = true;
            return true;
        }
        if self.showing_command_palette {
            self.showing_command_palette = false;
            self.command_palette.hide();
            self.command_palette.state_mut().clear_query();
            // Clear input to prevent palette search text from leaking into main input
            self.input_handler.state.clear();
            self.input_mode = self.input_handler.state.mode;
            self.dirty = true;
            return true;
        }
        if self.showing_skill_palette {
            self.showing_skill_palette = false;
            self.skill_palette.close();
            self.dirty = true;
            return true;
        }
        if self.showing_plugin_manager {
            self.showing_plugin_manager = false;
            self.plugin_manager_ui.hide();
            self.dirty = true;
            return true;
        }
        if self.showing_marketplace_browser {
            self.showing_marketplace_browser = false;
            self.marketplace_browser.close();
            self.dirty = true;
            return true;
        }
        if self.file_finder.is_visible() {
            self.file_finder.hide();
            self.dirty = true;
            return true;
        }
        if self.search_state.visible {
            self.search_state.visible = false;
            self.search_state.query.clear();
            self.dirty = true;
            return true;
        }
        if self.tool_panel.showing_tool_panel {
            self.tool_panel.showing_tool_panel = false;
            self.dirty = true;
            return true;
        }
        if self.worker_panel.visible {
            self.worker_panel.visible = false;
            self.dirty = true;
            return true;
        }
        if self.team_panel.visible {
            self.team_panel.visible = false;
            self.dirty = true;
            return true;
        }
        if self.show_task_dashboard {
            self.show_task_dashboard = false;
            self.dirty = true;
            return true;
        }
        if self.session_sidebar.is_visible() {
            self.session_sidebar.hide();
            self.dirty = true;
            return true;
        }
        if self.theme_preview.is_visible() {
            self.theme_preview.hide();
            self.dirty = true;
            return true;
        }
        if self.help_state.visible {
            self.help_state.visible = false;
            self.dirty = true;
            return true;
        }
        false
    }
}
