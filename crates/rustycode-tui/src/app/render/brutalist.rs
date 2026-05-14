// BrutalistRenderer construction helper — single source of truth for all parameters.
//
// `BrutalistRendererState::snapshot()` extracts all backend-specific fields
// from live `TUI` state in one place, replacing the old 25-field builder
// chain that was buried inside the `TUI` impl.
//
// Usage:
//   let state = BrutalistRendererState::snapshot(&self, &input_text);
//   let renderer = BrutalistRenderer::from_state(state);
/// All fields required to construct a [`BrutalistRenderer`] for one frame.
///
/// Extracted from live `TUI` state via `BrutalistRendererState::snapshot`.
/// Keeping construction explicit here means the `TUI` struct no longer needs
/// to know about the renderer's internal fields.
pub struct BrutalistRendererState<'a> {
    // These are the same fields the old builder accepted, grouped for clarity.
    pub input_text: &'a str,
    pub agent_status: &'a str,
    pub auto_memory_status: &'a str,
    pub active_tool_count: usize,
    pub active_tool_display: String,
    pub input_line_count: usize,
    pub context_usage: crate::app::context_usage::ContextUsage,
}

impl TUI {
    /// Capture all state needed by the brutalist renderer for one frame.
    ///
    /// Returns a tuple of `(BrutalistRendererState, RendererState)` so callers
    /// can use the shared state for header/footer chrome without re-extracting.
    ///
    /// `input_text` must be passed in because the renderer borrows it;
    /// get it via `self.ui.input_handler.state.all_text()` before calling.
    pub(crate) fn snapshot_brutalist_state<'a>(
        &'a self,
        input_text: &'a str,
    ) -> BrutalistRendererState<'a> {
        let agent_status = if self.session.streaming.is_streaming {
            "thinking"
        } else if !self.session.active_tools.is_empty() {
            "tools"
        } else {
            "ready"
        };

        let auto_memory_status = if self.sys.auto_memory.is_some() { "on" } else { "off" };

        let active_tool_count = self.session.active_tools.len();
        let active_tool_names: String = self
            .session.active_tools
            .values()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let remaining = active_tool_count.saturating_sub(3);
        let active_tool_display = if remaining > 0 {
            format!("{}, +{} more", active_tool_names, remaining)
        } else {
            active_tool_names
        };

        let mut context_usage = crate::app::context_usage::ContextUsage::new();
        // Context length estimate = last prompt_tokens + last output_tokens.
        // prompt_tokens includes full history; output_tokens is the response that
        // will be appended to the next request's prompt.
        if self.model.token_budget.last_turn_input_tokens > 0 {
            context_usage.update(self.model.token_budget.last_turn_input_tokens, self.model.token_budget.last_turn_output_tokens);
        } else {
            context_usage.update(self.model.token_budget.session_input_tokens, 0);
        }
        context_usage.set_limit(self.sys.compaction.context_monitor.max_tokens);

        BrutalistRendererState {
            input_text,
            agent_status,
            auto_memory_status,
            active_tool_count,
            active_tool_display,
            input_line_count: if input_text.is_empty() {
                1
            } else if input_text.ends_with('\n') {
                input_text.lines().count() + 1
            } else {
                input_text.lines().count()
            }.max(1),
            context_usage,
        }
    }

    /// Create a [`BrutalistRenderer`] populated with current session data.
    ///
    /// Prefer this over calling `BrutalistRendererBuilder` directly — it
    /// ensures all fields are consistently populated from live TUI state.
    ///
    /// `input_text` must be passed in because the renderer borrows it.
    /// Get it via `self.ui.input_handler.state.all_text()` before calling.
    pub(crate) fn create_brutalist_renderer<'a>(
        &'a self,
        input_text: &'a str,
    ) -> crate::app::render::brutalist_renderer::BrutalistRenderer<'a> {
        let bs = self.snapshot_brutalist_state(input_text);

        // Compute stream elapsed time for live timing display
        let stream_elapsed = self.session.streaming.stream_start_time.map(|t| t.elapsed());

        // History/reverse search state for input bar display
        let (reverse_query, reverse_match, reverse_total) =
            self.ui.input_handler.reverse_search_info();
        let (hist_pos, hist_total) = self.ui.input_handler.history_position();

        crate::app::render::brutalist_renderer::BrutalistRendererBuilder::new(&self.session.messages, input_text)
            .stream_content(&self.session.streaming.current_stream_content)
            .cwd(self.integration.services.cwd().clone())
            .is_streaming(self.session.streaming.is_streaming)
            .scroll(self.ui.view.scroll_offset_line, self.ui.view.user_scrolled)
            .selection(self.ui.view.selected_message, self.ui.view.viewport_height)
            .theme(self.theme.theme_colors.clone())
            .statuses(bs.agent_status, bs.auto_memory_status)
            .input_mode(self.sys.input_mode)
            .rate_limit(self.integration.rate_limit.until)
            .streaming_state(
                self.session.streaming.chunks_received,
                self.session.streaming.thinking_chunks_received,
                self.ui.animator.current_frame().progress_frame,
            )
            .context_usage(bs.context_usage)
            .tool_status(bs.active_tool_count, bs.active_tool_display)
            .session_info(
                self.model.token_budget.session_cost_usd,
                self.model.token_budget.session_input_tokens,
                self.model.token_budget.session_output_tokens,
                self.model.token_budget.session_cache_read_tokens,
                self.model.token_budget.last_turn_input_tokens,
                &self.model.current_model,
            )
            .warnings(self.model.api_key_warning.clone())
            .collapsed(self.ui.status_bar_collapsed, self.ui.footer_collapsed)
            .input_state(
                bs.input_line_count,
                self.session.streaming.queued_message.is_some(),
                self.session.streaming.queued_message.as_deref().unwrap_or("").to_string(),
            )
            .timing(self.session.streaming.last_response_duration, stream_elapsed)
            .git_branch(self.workspace.git_branch.as_deref().unwrap_or(""))
            .reverse_search(reverse_query, reverse_match, reverse_total)
            .history_browsing(hist_pos, hist_total)
            .search(
                self.search.search_state.query.clone(),
                self.search.search_state.matches.clone(),
                self.search.search_state.current_match_index,
            )
            .session_start(Some(self.integration.start_time))
            .cursor_position(
                self.ui.input_handler.state.cursor_col,
                self.ui.input_handler.state.cursor_row,
            )
            .build()
    }
}
