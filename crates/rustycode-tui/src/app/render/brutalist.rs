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
#[non_exhaustive]
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
    /// get it via `self.input_handler.state.all_text()` before calling.
    pub(crate) fn snapshot_brutalist_state<'a>(
        &'a self,
        input_text: &'a str,
    ) -> BrutalistRendererState<'a> {
        let agent_status = if self.streaming.is_streaming {
            "thinking"
        } else if !self.active_tools.is_empty() {
            "tools"
        } else {
            "ready"
        };

        let auto_memory_status = if self.auto_memory.is_some() { "on" } else { "off" };

        let active_tool_count = self.active_tools.len();
        let active_tool_names: String = self
            .active_tools
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
        if self.token_budget.last_turn_input_tokens > 0 {
            context_usage.update(self.token_budget.last_turn_input_tokens, self.token_budget.session_output_tokens);
        } else {
            context_usage.update(self.token_budget.session_input_tokens, self.token_budget.session_output_tokens);
        }
        context_usage.set_limit(self.compaction.context_monitor.max_tokens);

        BrutalistRendererState {
            input_text,
            agent_status,
            auto_memory_status,
            active_tool_count,
            active_tool_display,
            input_line_count: input_text.lines().count().max(1),
            context_usage,
        }
    }

    /// Create a [`BrutalistRenderer`] populated with current session data.
    ///
    /// Prefer this over calling `BrutalistRendererBuilder` directly — it
    /// ensures all fields are consistently populated from live TUI state.
    ///
    /// `input_text` must be passed in because the renderer borrows it.
    /// Get it via `self.input_handler.state.all_text()` before calling.
    pub(crate) fn create_brutalist_renderer<'a>(
        &'a self,
        input_text: &'a str,
    ) -> crate::app::render::brutalist_renderer::BrutalistRenderer<'a> {
        let bs = self.snapshot_brutalist_state(input_text);

        // Compute stream elapsed time for live timing display (Goose pattern)
        let stream_elapsed = self.streaming.stream_start_time.map(|t| t.elapsed());

        // History/reverse search state for input bar display
        let (reverse_query, reverse_match, reverse_total) =
            self.input_handler.reverse_search_info();
        let (hist_pos, hist_total) = self.input_handler.history_position();

        crate::app::render::brutalist_renderer::BrutalistRendererBuilder::new(&self.messages, input_text)
            .stream_content(&self.streaming.current_stream_content)
            .cwd(self.services.cwd().clone())
            .is_streaming(self.streaming.is_streaming)
            .scroll(self.view.scroll_offset_line, self.view.user_scrolled)
            .selection(self.view.selected_message, self.view.viewport_height)
            .theme(self.theme_colors.clone())
            .statuses(bs.agent_status, bs.auto_memory_status)
            .input_mode(self.input_mode)
            .rate_limit(self.rate_limit.until)
            .streaming_state(
                self.streaming.chunks_received,
                self.streaming.thinking_chunks_received,
                self.animator.current_frame().progress_frame,
            )
            .context_usage(bs.context_usage)
            .tool_status(bs.active_tool_count, bs.active_tool_display)
            .session_info(
                self.token_budget.session_cost_usd,
                self.token_budget.session_input_tokens,
                self.token_budget.session_output_tokens,
                self.token_budget.session_cache_read_tokens,
                self.token_budget.last_turn_input_tokens,
                &self.current_model,
            )
            .warnings(self.api_key_warning.clone())
            .collapsed(self.status_bar_collapsed, self.footer_collapsed)
            .input_state(
                bs.input_line_count,
                self.streaming.queued_message.is_some(),
                self.streaming.queued_message.as_deref().unwrap_or("").to_string(),
            )
            .timing(self.streaming.last_response_duration, stream_elapsed)
            .git_branch(self.git_branch.as_deref().unwrap_or(""))
            .reverse_search(reverse_query, reverse_match, reverse_total)
            .history_browsing(hist_pos, hist_total)
            .search(
                self.search_state.query.clone(),
                self.search_state.matches.clone(),
                self.search_state.current_match_index,
            )
            .session_start(Some(self.start_time))
            .cursor_position(
                self.input_handler.state.cursor_col,
                self.input_handler.state.cursor_row,
            )
            .build()
    }
}
