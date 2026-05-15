//! Input handling helpers for the event loop

use crate::app::event_loop::TUI;
use crate::ui::message::Message;
use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyModifiers};
use rustycode_protocol::Op;

/// State for scrolling operations
#[derive(Debug, Clone)]
pub struct ScrollState {
    pub scroll_offset: usize,
    pub scroll_offset_line: usize,
    pub selected_message: usize,
    pub user_scrolled: bool,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            scroll_offset_line: 0,
            selected_message: 0,
            user_scrolled: false,
        }
    }

    pub fn scroll_up(&mut self) {
        self.user_scrolled = true;
        self.scroll_offset_line = self.scroll_offset_line.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.user_scrolled = true;
        self.scroll_offset_line = self.scroll_offset_line.saturating_add(1);
    }

    pub fn page_up(&mut self, _message_count: usize, viewport_height: usize) {
        let scroll_amount = viewport_height.max(1);
        if self.selected_message >= scroll_amount {
            self.selected_message -= scroll_amount;
            self.scroll_offset = self.scroll_offset.saturating_sub(scroll_amount);
        } else {
            self.selected_message = 0;
            self.scroll_offset = 0;
        }
    }

    pub fn page_down(&mut self, message_count: usize, viewport_height: usize) {
        let scroll_amount = viewport_height.max(1);
        let new_selected =
            (self.selected_message + scroll_amount).min(message_count.saturating_sub(1));
        self.selected_message = new_selected;

        // Adjust scroll offset
        let viewport_bottom = self.scroll_offset + viewport_height;
        if self.selected_message >= viewport_bottom {
            self.scroll_offset = self.selected_message - viewport_height + 1;
        }
    }

    pub fn reset_user_scroll(&mut self) {
        self.user_scrolled = false;
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl TUI {
    /// Handle keyboard input - main entry point
    pub(crate) fn handle_input(&mut self) -> Result<()> {
        let input_start = std::time::Instant::now();
        let debug_enabled = crate::logging::is_debug_enabled();

        let event = event::read();
        let event_summary = event.as_ref().ok().map(|event| match event {
            CrosstermEvent::Key(key) => format!("key {:?} {:?}", key.code, key.modifiers),
            CrosstermEvent::Mouse(mouse) => format!("mouse {:?}", mouse.kind),
            CrosstermEvent::Resize(width, height) => format!("resize {}x{}", width, height),
            CrosstermEvent::Paste(content) => format!("paste {} chars", content.len()),
            CrosstermEvent::FocusGained => "focus_gained".to_string(),
            CrosstermEvent::FocusLost => "focus_lost".to_string(),
        });

        match event {
            Ok(CrosstermEvent::Key(key)) => {
                let is_cmd_palette_shortcut = (key.code == KeyCode::Char('k')
                    && key.modifiers.contains(KeyModifiers::CONTROL))
                    || (key.code == KeyCode::Char('P')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.modifiers.contains(KeyModifiers::SHIFT));
                if is_cmd_palette_shortcut && !self.is_any_overlay_open() {
                    self.overlays.showing_command_palette = true;
                    self.overlays.showing_skill_palette = false;
                    self.overlays.showing_marketplace_browser = false;
                    self.overlays.command_palette.show();
                    self.overlays.command_palette.state_mut().clear_query();
                    self.sys.dirty = true;
                    return Ok(());
                }
                if self.handle_wizard_input(key)? {
                    return Ok(());
                }
                if self.handle_approval_input(key)? {
                    return Ok(());
                }
                if self.handle_error_input(key)? {
                    return Ok(());
                }
                if self.handle_clarification_input(key)? {
                    return Ok(());
                }
                if self.sys.compaction.showing_preview {
                    match key.code {
                        KeyCode::Enter => {
                            self.execute_compaction();
                            return Ok(());
                        }
                        KeyCode::Esc => {
                            self.sys.compaction.showing_preview = false;
                            self.sys.compaction.pending = false;
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Char('c') | KeyCode::Char('q')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            // Allow Ctrl+C/Ctrl+Q to fall through to global shortcut handler
                        }
                        _ => {
                            self.sys.dirty = true;
                            return Ok(());
                        }
                    }
                }
                // Dismiss tool result overlay before global shortcuts can intercept
                if self.panels.tool_panel.showing_tool_result {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            // Close detail view and return to tool panel
                            self.panels.tool_panel.showing_tool_result = false;
                            self.panels.tool_panel.tool_result_show_full = false;
                            self.panels.tool_panel.tool_result_scroll_offset = 0;
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Char('f') | KeyCode::Char('F') => {
                            // Toggle between truncated and full output
                            self.panels.tool_panel.tool_result_show_full =
                                !self.panels.tool_panel.tool_result_show_full;
                            self.panels.tool_panel.tool_result_scroll_offset = 0;
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Up => {
                            // Scroll up in tool result overlay
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_sub(crate::app::TOOL_RESULT_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Down => {
                            // Scroll down in tool result overlay
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_add(crate::app::TOOL_RESULT_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::PageUp => {
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_sub(self.ui.view.viewport_height);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::PageDown => {
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_add(self.ui.view.viewport_height);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Char('j') => {
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_add(crate::app::TOOL_RESULT_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Char('k') => {
                            self.panels.tool_panel.tool_result_scroll_offset = self
                                .panels
                                .tool_panel
                                .tool_result_scroll_offset
                                .saturating_sub(crate::app::TOOL_RESULT_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Pass through Ctrl+C so user can cancel stream while viewing tool result
                            // Fall through to global shortcut handler below
                        }
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Pass through Ctrl+D for overlay dismiss/scroll
                            // Fall through to global shortcut handler below
                        }
                        _ => {
                            // Consume other keys while overlay is active
                            return Ok(());
                        }
                    }
                }

                // Handle help panel scroll (only when input is empty —
                // otherwise j/k should go to text input)
                let input_is_empty_for_help = self.ui.input_handler.state.lines.len() == 1
                    && self.ui.input_handler.state.lines[0].is_empty();
                if self.ui.help_state.visible && input_is_empty_for_help {
                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            self.ui.help_state.scroll_offset =
                                self.ui.help_state.scroll_offset.saturating_sub(1);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            self.ui.help_state.scroll_offset =
                                self.ui.help_state.scroll_offset.saturating_add(1);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::PageUp => {
                            self.ui.help_state.scroll_offset = self
                                .ui
                                .help_state
                                .scroll_offset
                                .saturating_sub(crate::app::HELP_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::PageDown => {
                            self.ui.help_state.scroll_offset = self
                                .ui
                                .help_state
                                .scroll_offset
                                .saturating_add(crate::app::HELP_SCROLL_STEP);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Home => {
                            self.ui.help_state.scroll_offset = 0;
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::End => {
                            // Will be clamped to actual content max_scroll during render
                            self.ui.help_state.scroll_offset = self
                                .ui
                                .help_state
                                .scroll_offset
                                .saturating_add(crate::app::HELP_SCROLL_STEP * 100);
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::Esc => {} // Esc handled by global shortcuts
                        _ => {
                            // Let unhandled keys fall through to input handler
                        }
                    }
                }

                // Handle sidebar toggle (Ctrl+B or Ctrl+Shift+B)
                // Ctrl+Shift+B works when running inside tmux (Ctrl+B is tmux prefix).
                if key.code == KeyCode::Char('b')
                    && (key.modifiers == KeyModifiers::CONTROL
                        || key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT)
                {
                    self.handle_sidebar_toggle();
                    return Ok(());
                }

                // Handle brutalist mode toggle (Alt+B)
                if key.code == KeyCode::Char('b') && key.modifiers == KeyModifiers::ALT {
                    self.handle_brutalist_toggle();
                    return Ok(());
                }

                // Handle session navigation (Ctrl+Shift+N/P/S only)
                if key.modifiers == KeyModifiers::CONTROL | KeyModifiers::SHIFT
                    && matches!(
                        key.code,
                        KeyCode::Char('N') | KeyCode::Char('P') | KeyCode::Char('S')
                    )
                {
                    self.handle_session_navigation(key.code);
                    return Ok(());
                }

                // Handle search toggle (Ctrl+F)
                if key.code == KeyCode::Char('f') && key.modifiers == KeyModifiers::CONTROL {
                    self.handle_search_toggle();
                    return Ok(());
                }

                // Handle tool panel toggle (Ctrl+P)
                if key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::CONTROL {
                    self.handle_tool_panel_toggle();
                    return Ok(());
                }

                // Handle file finder toggle (Ctrl+O)
                if key.code == KeyCode::Char('o') && key.modifiers == KeyModifiers::CONTROL {
                    self.handle_file_finder_toggle();
                    return Ok(());
                }

                // Handle team agent timeline toggle (Ctrl+G)
                if key.code == KeyCode::Char('g') && key.modifiers == KeyModifiers::CONTROL {
                    self.handle_team_panel_toggle();
                    return Ok(());
                }

                // Handle worker status panel toggle (Ctrl+W)
                // Only toggle when input is empty — otherwise let it fall through
                // to the input handler for the standard readline Ctrl+W (delete word)
                if key.code == KeyCode::Char('w')
                    && key.modifiers == KeyModifiers::CONTROL
                    && self.ui.input_handler.state.is_empty()
                {
                    self.handle_worker_panel_toggle();
                    return Ok(());
                }

                // Handle task dashboard toggle (Ctrl+T)
                if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::CONTROL {
                    self.handle_task_dashboard_toggle();
                    return Ok(());
                }

                // Handle theme preview toggle (Ctrl+Shift+T)
                if key.code == KeyCode::Char('T')
                    && key.modifiers == (KeyModifiers::CONTROL | KeyModifiers::SHIFT)
                {
                    self.theme.theme_preview.toggle();
                    self.sys.dirty = true;
                    return Ok(());
                }

                // Handle tool panel navigation
                if self.panels.tool_panel.showing_tool_panel && self.handle_tool_panel_input(key)? {
                    return Ok(());
                }

                // Handle file finder input (when visible)
                if self.handle_file_finder_input(key) {
                    return Ok(());
                }

                // Handle search box input
                if self.handle_search_input(key)? {
                    return Ok(());
                }

                // Handle marketplace browser input
                if self.handle_marketplace_browser_input(key) {
                    return Ok(());
                }

                // Handle command palette input
                let handled = self.handle_command_palette_input(key.code, key.modifiers)?;
                if handled {
                    return Ok(());
                }

                // Handle skill palette input
                if self.handle_skill_palette_input(key.code, key)? {
                    return Ok(());
                }

                // Handle theme preview input
                if self.handle_theme_preview_input(key) {
                    self.sys.dirty = true;
                    return Ok(());
                }

                // Handle model selector input
                if self.handle_model_selector_input(key) {
                    if let Some(selected) = self.overlays.model_selector.take_selected() {
                        self.apply_model_switch(&selected);
                    }
                    self.sys.dirty = true;
                    return Ok(());
                }

                // Handle F1 to open help (standard help key, no modifier conflict)
                if key.code == KeyCode::F(1) && !self.is_any_overlay_open() {
                    self.ui.help_state.visible = true;
                    self.ui.help_state.scroll_offset = 0;
                    self.theme.toast_manager.info("Press Esc to close");
                    self.sys.dirty = true;
                    return Ok(());
                }

                // Handle Space key to toggle message collapse/expand when input is empty
                if key.code == KeyCode::Char(' ') && key.modifiers == KeyModifiers::NONE {
                    let input_is_empty = self.ui.input_handler.state.lines.len() == 1
                        && self.ui.input_handler.state.lines[0].is_empty();
                    if input_is_empty && !self.session.messages.is_empty() {
                        self.toggle_message_collapse();
                        self.sys.dirty = true;
                        return Ok(());
                    }
                }

                // Handle Tab to toggle tool/thinking expansion when input is empty
                if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
                    let input_is_empty = self.ui.input_handler.state.lines.len() == 1
                        && self.ui.input_handler.state.lines[0].is_empty();
                    if input_is_empty && self.ui.view.selected_message < self.session.messages.len()
                    {
                        let msg = &mut self.session.messages[self.ui.view.selected_message];
                        let has_tools = msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty());
                        let has_thinking = msg.thinking.as_ref().is_some_and(|t| !t.is_empty());

                        if has_tools && has_thinking {
                            // Both: Tab cycles tools → thinking → both collapsed
                            match (msg.tools_expansion, msg.thinking_expansion) {
                                (crate::ui::message::ExpansionLevel::Collapsed, _) => {
                                    msg.tools_expansion =
                                        crate::ui::message::ExpansionLevel::Expanded;
                                    msg.thinking_expansion =
                                        crate::ui::message::ExpansionLevel::Collapsed;
                                }
                                (
                                    crate::ui::message::ExpansionLevel::Expanded,
                                    crate::ui::message::ExpansionLevel::Collapsed,
                                ) => {
                                    msg.tools_expansion =
                                        crate::ui::message::ExpansionLevel::Collapsed;
                                    msg.thinking_expansion =
                                        crate::ui::message::ExpansionLevel::Expanded;
                                }
                                _ => {
                                    msg.tools_expansion =
                                        crate::ui::message::ExpansionLevel::Collapsed;
                                    msg.thinking_expansion =
                                        crate::ui::message::ExpansionLevel::Collapsed;
                                }
                            }
                            self.sys.dirty = true;
                            return Ok(());
                        } else if has_tools {
                            msg.tools_expansion = match msg.tools_expansion {
                                crate::ui::message::ExpansionLevel::Collapsed => {
                                    crate::ui::message::ExpansionLevel::Expanded
                                }
                                _ => crate::ui::message::ExpansionLevel::Collapsed,
                            };
                            self.sys.dirty = true;
                            return Ok(());
                        } else if has_thinking {
                            msg.toggle_thinking_expansion();
                            self.sys.dirty = true;
                            return Ok(());
                        }
                    }
                }

                // Home/End always jump to top/bottom of messages (Ctrl+A/CtrlE handle cursor)
                if !self.session.messages.is_empty() && key.modifiers == KeyModifiers::NONE {
                    match key.code {
                        KeyCode::Home => {
                            self.push_undo_position();
                            self.ui.view.selected_message = 0;
                            self.ui.view.scroll_offset_line = 0;
                            self.ui.view.user_scrolled = true;
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        KeyCode::End => {
                            self.push_undo_position();
                            self.ui.view.selected_message =
                                self.session.messages.len().saturating_sub(1);
                            self.ui.view.user_scrolled = false;
                            self.auto_scroll();
                            self.sys.dirty = true;
                            return Ok(());
                        }
                        _ => {}
                    }
                }

                // Handle Vim navigation keys BEFORE text input (j/k/g/G when input empty)
                if self.ui.tui_config.behavior.vim_enabled
                    && (key.modifiers == KeyModifiers::NONE
                        || (key.modifiers == KeyModifiers::SHIFT
                            && matches!(
                                key.code,
                                KeyCode::Char('G') | KeyCode::Char('E') | KeyCode::Char('W')
                            )))
                {
                    let input_is_empty = self.ui.input_handler.state.lines.len() == 1
                        && self.ui.input_handler.state.lines[0].is_empty();
                    if input_is_empty {
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Char('k') | KeyCode::Char('g') => {
                                self.handle_global_shortcut(key.code, key.modifiers)?;
                                return Ok(());
                            }
                            KeyCode::Char('G') => {
                                self.handle_global_shortcut(key.code, key.modifiers)?;
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }

                // Normal input handling - delegate to input handler
                let action = self
                    .ui
                    .input_handler
                    .handle_key_event(key.code, key.modifiers);
                self.handle_input_action(action, key)?;
            }
            Ok(CrosstermEvent::Paste(content)) => {
                self.handle_bracketed_paste(&content)?;
            }
            Ok(CrosstermEvent::Mouse(mouse)) => match mouse.kind {
                event::MouseEventKind::ScrollUp | event::MouseEventKind::ScrollDown => {
                    self.handle_mouse_scroll(mouse);
                }
                event::MouseEventKind::Down(_)
                | event::MouseEventKind::Drag(_)
                | event::MouseEventKind::Up(_) => {
                    self.handle_mouse_input(mouse);
                }
                _ => {}
            },
            Ok(CrosstermEvent::Resize(width, height)) => {
                if width == 0 || height == 0 {
                    tracing::warn!(
                        "Terminal reported zero dimension ({width}x{height}), ignoring resize"
                    );
                    return Ok(());
                }
                self.ui.message_renderer.invalidate_cache();
                // Don't reset scroll_offset_line to 0, let it stay where it was
                // and it will be clamped during next render if necessary.
                self.dismiss_any_overlay();
                self.sys.dirty = true;
                tracing::debug!("Terminal resized to {}x{}", width, height);
            }
            Ok(_other) => {}
            Err(e) => {
                tracing::error!("Failed to read event: {}", e);
                return Err(e.into());
            }
        }

        let input_elapsed = input_start.elapsed();
        if debug_enabled {
            crate::debug_log!(
                "Processed input event: event={} elapsed_ms={} dirty={} streaming={} tool_panel={} command_palette={} skill_palette={} provider_selector={}",
                event_summary.as_deref().unwrap_or("unknown"),
                input_elapsed.as_millis(),
                self.sys.dirty,
                self.session.streaming.is_streaming,
                self.panels.tool_panel.showing_tool_panel,
                self.overlays.showing_command_palette,
                self.overlays.showing_skill_palette,
                self.overlays.showing_provider_selector
            );
        }

        Ok(())
    }

    /// Retry the last sent message
    pub(crate) fn retry_last_message(&mut self, message: String) {
        if self.session.streaming.is_streaming {
            tracing::warn!("Already streaming, skipping retry");
            return;
        }

        let message_to_send = self.prepare_message_for_send(&message);
        let _workspace_context = self.workspace.workspace_context.clone();
        let history = self.build_conversation_history();

        // Save message for potential subsequent auto-retries.
        // take_last_message() consumes it, so we must replenish before sending.
        self.integration.rate_limit.last_message = Some(message.clone());

        // Set streaming state before send to prevent double-Enter races
        self.session.streaming.is_streaming = true;
        self.session.streaming.chunks_received = 0;
        self.session.streaming.thinking_chunks_received = 0;
        self.session.streaming.stream_start_time = Some(std::time::Instant::now());
        self.session.streaming.current_stream_content.clear();
        self.session.streaming.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
        self.panels.tool_panel.reset();
        self.session.active_tools.clear();

        let send_result = self.integration.services.submit_op(Op::SendMessageFull {
            content: message_to_send,
            history: Some(serde_json::to_value(&history).unwrap_or(serde_json::Value::Null)),
            images: None,
        });
        if let Err(e) = send_result {
            tracing::error!("Failed to retry message: {}", e);
            self.reset_streaming_state();
            self.session.active_tools.clear();
            self.add_system_message("Retry failed - please try again".to_string());
            self.auto_scroll();
        } else {
            let assistant_msg = Message::assistant(String::new());
            self.session.messages.push(assistant_msg);
            self.sys.dirty = true;
            self.auto_scroll();
        }
    }

    /// Execute a bash command and display the result as a system message.
    ///
    /// Uses a bounded timeout (30s) to prevent blocking the TUI event loop
    /// indefinitely on long-running commands. For truly long commands,
    /// users should use the AI's bash tool instead.
    pub(crate) fn execute_bash_command(&mut self, cmd: &str) {
        use rustycode_tools::{BashTool, Tool, ToolContext, ToolOutput};
        use serde_json::json;

        let cwd = self.integration.services.cwd().to_path_buf();
        let tool = BashTool;

        let params = json!({"command": cmd});

        // Spawn thread and use channel with a short timeout to avoid freezing the TUI.
        // Fast commands (< 2s) complete inline; slow commands show a "still running" hint.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let ctx = ToolContext::new(&cwd);
            let result = tool.execute(params, &ctx);
            let _ = tx.send(result);
        });

        let result = match rx.recv_timeout(crate::app::TOOL_RESULT_INITIAL_TIMEOUT) {
            Ok(r) => r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.add_system_message(
                    "⏳ Command still running (output will appear shortly)...".to_string(),
                );
                self.auto_scroll();
                // Store result in shared state for polling by poll_services()
                let result_store = self.session.streaming.pending_bash_result.clone();
                std::thread::spawn(move || {
                    if let Ok(result) = rx.recv_timeout(crate::app::TOOL_RESULT_FALLBACK_TIMEOUT) {
                        let text = match result {
                            Ok(output) => {
                                let t = output.text.trim().to_string();
                                if t.is_empty() {
                                    "(no output)".to_string()
                                } else {
                                    t
                                }
                            }
                            Err(e) => format!("Error: {}", e),
                        };
                        if let Ok(mut store) = result_store.lock() {
                            *store = Some(text);
                        }
                    }
                });
                return;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Ok(ToolOutput::text("Bash command panicked".to_string()))
            }
        };

        match result {
            Ok(output) => {
                let text = output.text.trim();
                if text.is_empty() {
                    self.add_system_message("(no output)".to_string());
                } else {
                    // Truncate long output (char-safe: find a valid char boundary)
                    let display = if text.len() > crate::app::MAX_DISPLAY_CHARS {
                        let byte_limit = text.floor_char_boundary(crate::app::MAX_DISPLAY_CHARS);
                        let end = text[..byte_limit].rfind('\n').unwrap_or(byte_limit);
                        format!(
                            "{}\n... ({} chars truncated)",
                            &text[..end],
                            text.len() - end
                        )
                    } else {
                        text.to_string()
                    };
                    self.add_system_message(display);
                }
            }
            Err(e) => {
                self.add_system_message(format!("Error: {}", e));
            }
        }
        self.auto_scroll();
    }

    /// Edit the given text in $EDITOR and return the edited result
    pub(crate) fn edit_in_editor(&self, text: &str) -> Result<String> {
        use std::env;
        use std::fs;
        use std::process::Command;

        // Get the editor from environment variable
        let editor = env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());

        // Create a temporary file
        let temp_file = tempfile::NamedTempFile::new()?;
        let temp_path = temp_file.path().to_path_buf();

        // Write the current text to the temp file
        fs::write(&temp_path, text)?;

        // Suspend TUI terminal state so the editor can take over
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableBracketedPaste,
            crossterm::event::DisableMouseCapture,
            crossterm::cursor::Show,
        );
        let _ = std::io::Write::flush(&mut std::io::stdout());

        // Launch the editor
        let status = Command::new(&editor).arg(&temp_path).status();

        // Restore TUI terminal state regardless of editor outcome
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste,
            crossterm::event::EnableMouseCapture,
        );
        let _ = crossterm::terminal::enable_raw_mode();
        // Clear the screen to invalidate ratatui's stale back-buffer.
        // Without this, differential rendering skips cells that haven't
        // changed vs the old buffer, even though the alternate screen is blank.
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        );
        let _ = std::io::Write::flush(&mut std::io::stdout());

        let status = status?;
        if !status.success() {
            return Err(anyhow::anyhow!("Editor exited with error code: {}", status));
        }

        // Read the edited content back
        let edited_content = fs::read_to_string(&temp_path)?;

        Ok(edited_content)
    }

    pub(crate) fn insert_skill_mention(&mut self, skill_name: &str) {
        self.ui.input_handler.state.insert_char('@');
        for c in skill_name.chars() {
            self.ui.input_handler.state.insert_char(c);
        }
        self.ui.input_handler.state.insert_char(' ');
        self.sys.input_mode = self.ui.input_handler.state.mode;
        self.sys.dirty = true;
    }

    // ========================================================================
    // MESSAGE TAGGING
    // ========================================================================

    /// Tag the selected message with a tag type
    pub fn tag_selected_message(
        &mut self,
        tag_type: crate::ui::message_tags::TagType,
    ) -> Result<()> {
        if self.ui.view.selected_message < self.session.messages.len() {
            let tag = crate::ui::message_tags::Tag::new(tag_type.clone());
            if self.session.messages[self.ui.view.selected_message].add_tag(tag) {
                self.add_system_message(format!("Tagged message: {}", tag_type.display_name()));
            } else {
                self.add_system_message(format!(
                    "Message already has tag: {}",
                    tag_type.display_name()
                ));
            }
            self.sys.dirty = true;
        }
        Ok(())
    }

    /// Remove a tag from the selected message
    pub fn untag_selected_message(
        &mut self,
        tag_type: &crate::ui::message_tags::TagType,
    ) -> Result<()> {
        if self.ui.view.selected_message < self.session.messages.len() {
            if self.session.messages[self.ui.view.selected_message].remove_tag_type(tag_type) {
                self.add_system_message(format!("Removed tag: {}", tag_type.display_name()));
            } else {
                self.add_system_message(format!(
                    "Message does not have tag: {}",
                    tag_type.display_name()
                ));
            }
            self.sys.dirty = true;
        }
        Ok(())
    }

    pub fn set_tag_filter(&mut self, tag_type: Option<crate::ui::message_tags::TagType>) {
        self.search.tag_filter.set_active(tag_type.clone());
        if let Some(ref tag) = tag_type {
            let count = self
                .session
                .messages
                .iter()
                .filter(|m| m.has_tag(tag))
                .count();
            self.add_system_message(format!(
                "Filtering by tag: {} ({} messages)",
                tag.display_name(),
                count
            ));
        } else {
            self.add_system_message("Tag filter cleared - showing all messages".to_string());
        }
        self.sys.dirty = true;
    }

    /// Clear the tag filter
    pub fn clear_tag_filter(&mut self) {
        self.set_tag_filter(None);
    }

    /// Get the count of messages with a specific tag
    pub fn count_tagged_messages(&self, tag_type: &crate::ui::message_tags::TagType) -> usize {
        self.session
            .messages
            .iter()
            .filter(|m| m.has_tag(tag_type))
            .count()
    }

    /// Get all unique tag types in current messages
    pub fn all_tag_types(&self) -> Vec<crate::ui::message_tags::TagType> {
        let mut tag_types = std::collections::HashSet::new();
        for message in &self.session.messages {
            for tag in message.tags() {
                tag_types.insert(tag.tag_type.clone());
            }
        }
        let mut result: Vec<_> = tag_types.into_iter().collect();
        result.sort();
        result
    }
}
