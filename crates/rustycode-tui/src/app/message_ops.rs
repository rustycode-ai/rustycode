//! Message management operations
//!
//! Handles adding, modifying, and managing messages.

use super::event_loop::TUI;
use crate::ui::message::Message;

impl TUI {
    /// Add an AI assistant message
    pub fn add_ai_message(&mut self, content: String) {
        let message = Message::assistant(content);
        self.messages.push(message);
        self.dirty = true;

        // Mark session recovery dirty for auto-save
        if let Some(ref mut recovery) = self.session_recovery {
            recovery.mark_dirty();
        }

        // Auto-scroll to latest message
        if !self.messages.is_empty() {
            self.view.selected_message = self.messages.len() - 1;
            // Reset line-based scroll offset to show latest (auto-scroll to bottom)
            self.view.scroll_offset_line = 0;
            self.view.user_scrolled = false;
        }

        // Update context monitor and check for auto-compaction
        self.compaction.context_monitor.update(&self.messages);
        self.maybe_auto_compact();
    }

    /// Add a system message
    pub fn add_system_message(&mut self, content: String) {
        let message = Message::system(content);
        self.messages.push(message);
        self.dirty = true;

        // Mark session recovery dirty for auto-save
        if let Some(ref mut recovery) = self.session_recovery {
            recovery.mark_dirty();
        }

        // Only auto-scroll to the new message if the user hasn't scrolled up.
        // Background system messages (auto-approvals, workspace notifications)
        // should not yank the user away from what they're reading.
        if !self.view.user_scrolled && !self.messages.is_empty() {
            self.view.selected_message = self.messages.len() - 1;
            self.view.scroll_offset_line = 0;
        }

        // Update context monitor and check for auto-compaction
        self.compaction.context_monitor.update(&self.messages);
        self.maybe_auto_compact();
    }

    /// Show an error with the error manager
    ///
    /// Converts an anyhow::Error into a user-friendly ErrorDisplay and shows it.
    pub fn show_error(&mut self, error: anyhow::Error) {
        use crate::ui::error_messages::classify_error;
        use crate::ui::errors::{ErrorSeverity, ErrorSuggestion};

        let error_msg = error.to_string();
        let category = classify_error(&error);

        let (_severity, title, _description, suggestions) = match category {
            crate::ui::error_messages::ErrorCategory::Network => (
                ErrorSeverity::Warning,
                "Network Connection Failed",
                "Could not connect to the API server. This may be due to network issues.",
                vec![
                    ErrorSuggestion::new("Check internet connection"),
                    ErrorSuggestion::new("Try again in a moment"),
                    ErrorSuggestion::new("Check API service status"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::Authentication => (
                ErrorSeverity::Error,
                "Authentication Failed",
                "Your API key appears to be invalid or missing.",
                vec![
                    ErrorSuggestion::new("Check ANTHROPIC_API_KEY is set"),
                    ErrorSuggestion::new("Verify API key is correct"),
                    ErrorSuggestion::new("Run /wizard to configure"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::RateLimit => (
                ErrorSeverity::Warning,
                "Rate Limited",
                "API rate limit reached. Requests will auto-retry after the cooldown period.",
                vec![
                    ErrorSuggestion::new("Wait 30-60 seconds before retrying"),
                    ErrorSuggestion::new("Check your API usage dashboard"),
                    ErrorSuggestion::new("Consider upgrading your API plan for higher limits"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::Streaming => (
                ErrorSeverity::Warning,
                "Response Interrupted",
                "The connection was interrupted while receiving the response.",
                vec![
                    ErrorSuggestion::new("Check internet connection"),
                    ErrorSuggestion::new("Try resending"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::ToolExecution => (
                ErrorSeverity::Error,
                "Tool Execution Failed",
                "A tool command failed to execute successfully.",
                vec![
                    ErrorSuggestion::new("Check tool output above"),
                    ErrorSuggestion::new("Verify permissions"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::FileOperation => (
                ErrorSeverity::Error,
                "File Operation Failed",
                "Could not perform the file operation.",
                vec![
                    ErrorSuggestion::new("Check file exists and permissions"),
                    ErrorSuggestion::new("Verify file path"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::Configuration => (
                ErrorSeverity::Error,
                "Configuration Error",
                "There's an issue with the application configuration.",
                vec![
                    ErrorSuggestion::new("Check environment variables"),
                    ErrorSuggestion::new("Run /wizard to reconfigure"),
                ],
            ),
            crate::ui::error_messages::ErrorCategory::Other => (
                ErrorSeverity::Error,
                "An Error Occurred",
                "An unexpected error occurred.",
                vec![
                    ErrorSuggestion::new("Try again"),
                    ErrorSuggestion::new("Check logs for details"),
                ],
            ),
        };

        let inline = format!("{}: {}", title, error_msg);

        if suggestions.is_empty() {
            self.add_system_message(inline);
        } else {
            let hint: String = suggestions
                .iter()
                .map(|s| s.action.as_str())
                .collect::<Vec<_>>()
                .join(" · ");
            self.add_system_message(format!("{} — {}", inline, hint));
        }
    }

    /// Add tools to the last AI message
    pub fn add_tools_to_last_message(&mut self, tools: Vec<crate::ui::message::ToolExecution>) {
        if let Some(last_msg) = self.messages.last_mut() {
            if last_msg.role == crate::ui::message::MessageRole::Assistant {
                last_msg.tool_executions = Some(tools);
                self.dirty = true;
            }
        }
    }

    /// Add thinking to the last AI message
    pub fn add_thinking_to_last_message(&mut self, thinking: String) {
        if let Some(last_msg) = self.messages.last_mut() {
            if last_msg.role == crate::ui::message::MessageRole::Assistant {
                last_msg.thinking = Some(thinking);
                self.dirty = true;
            }
        }
    }

    /// Auto-scroll to latest message
    ///
    /// During streaming: if the user has scrolled up, never override their
    /// position — they're reading earlier content. The scroll-to-bottom
    /// indicator ("▼ N new below") provides a way to jump back.
    ///
    /// When not streaming: respects a 2-second debounce after user manual
    /// scroll, then snaps to the bottom on the next auto_scroll call.
    pub(crate) fn auto_scroll(&mut self) {
        // During active streaming, never fight user scroll position.
        // The user is reading earlier content and the overflow indicator
        // gives them a way to jump back down when ready.
        if self.streaming.is_streaming && self.view.user_scrolled {
            return;
        }

        // When not streaming, debounce to avoid snapping back immediately
        const SCROLL_DEBOUNCE: std::time::Duration = std::time::Duration::from_secs(2);
        if self.view.user_scrolled && self.view.last_user_scroll_time.elapsed() < SCROLL_DEBOUNCE {
            return;
        }

        self.view.user_scrolled = false;
        // Set scroll_offset_line to the actual bottom position so that
        // scroll_up_by works correctly from the bottom (instead of jumping to top).
        let max_scroll = self
            .view
            .last_total_lines
            .get()
            .saturating_sub(self.view.viewport_height.max(1));
        self.view.scroll_offset_line = max_scroll;
        if !self.messages.is_empty() {
            self.view.selected_message = self.messages.len() - 1;
        }
    }

    /// Update viewport height (called when terminal is resized)
    pub fn update_viewport_height(&mut self, height: usize) {
        self.view.viewport_height = height;
        // Line-based scrolling doesn't need adjustment - the render function
        // automatically calculates the visible range based on viewport height
    }

    /// Build conversation history from TUI messages for multi-turn LLM context.
    ///
    /// Converts the internal message list into ChatMessages suitable for
    /// the LLM provider. Only includes user and assistant messages (skips
    /// system messages since those are handled separately).
    ///
    /// For messages with tool executions, generates proper assistant+user
    /// message pairs with tool_use and tool_result content blocks, so the
    /// LLM gets the full tool context (not just summaries).
    ///
    /// Safety: Ensures alternating user/assistant roles — some providers
    /// (OpenAI) reject consecutive same-role messages.
    pub(crate) fn build_conversation_history(&self) -> Vec<rustycode_llm::provider::ChatMessage> {
        use rustycode_llm::provider::ChatMessage;
        use rustycode_llm::provider::MessageRole as LlmRole;
        use rustycode_protocol::{ContentBlock, MessageContent};

        // Cap history to prevent token overflow — keep last N turns.
        // Each turn is 1-3 messages (user, assistant, optional tool_result user).
        const MAX_HISTORY_MESSAGES: usize = 200;

        let mut messages: Vec<ChatMessage> = Vec::new();

        for msg in self.messages.iter() {
            match msg.role {
                crate::ui::message::MessageRole::User => {
                    messages.push(ChatMessage::user(msg.content.clone()));
                }
                crate::ui::message::MessageRole::Assistant => {
                    // Skip empty assistant messages (streaming placeholders)
                    if msg.content.trim().is_empty()
                        && msg.tool_executions.as_ref().is_none_or(|t| t.is_empty())
                    {
                        continue;
                    }

                    // Build assistant message with tool_use blocks
                    let has_tools = msg.tool_executions.as_ref().is_some_and(|t| !t.is_empty());

                    if has_tools {
                        let mut blocks = Vec::new();

                        // Add text content if present
                        if !msg.content.trim().is_empty() {
                            blocks.push(ContentBlock::text(&msg.content));
                        }

                        // Add tool_use blocks
                        if let Some(tools) = &msg.tool_executions {
                            for tool in tools {
                                blocks.push(ContentBlock::tool_use(
                                    &tool.tool_id,
                                    &tool.name,
                                    // Reconstruct input from stored parameters
                                    tool.input_json.clone().unwrap_or_default(),
                                ));
                            }
                        }

                        messages.push(ChatMessage::assistant(MessageContent::blocks(blocks)));

                        // Add tool_result messages with actual outputs
                        // Each tool result is a separate user message (matching response.rs pattern)
                        if let Some(tools) = &msg.tool_executions {
                            for tool in tools {
                                let output = tool
                                    .detailed_output
                                    .as_deref()
                                    .unwrap_or(&tool.result_summary);
                                // Truncate large outputs to avoid token overflow.
                                // Split at the last newline boundary before the limit
                                // so the LLM gets complete lines (better comprehension).
                                const MAX_TOOL_OUTPUT_CHARS: usize = 4000;
                                let truncated = if output.len() > MAX_TOOL_OUTPUT_CHARS {
                                    let byte_limit =
                                        output.floor_char_boundary(MAX_TOOL_OUTPUT_CHARS);
                                    let slice = &output[..byte_limit];
                                    // Find last newline for clean split
                                    let end = slice.rfind('\n').unwrap_or(byte_limit);
                                    format!(
                                        "{}\n\n[... truncated, {} chars omitted]",
                                        &output[..end],
                                        output.len() - end
                                    )
                                } else {
                                    output.to_string()
                                };
                                let is_error =
                                    matches!(tool.status, crate::ui::message::ToolStatus::Failed);
                                messages.push(ChatMessage::tool_result_with_error(
                                    truncated,
                                    tool.tool_id.clone(),
                                    is_error,
                                ));
                            }
                        }
                    } else {
                        // No tools — simple text message
                        messages.push(ChatMessage::assistant(msg.content.clone()));
                    }
                }
                // Skip system messages — handled separately by stream_llm_response
                crate::ui::message::MessageRole::System => {}
            }
        }

        // Cap to MAX_HISTORY_MESSAGES, keeping the most recent.
        // Carefully avoid splitting in the middle of a tool_use/tool_result pair —
        // Anthropic's API rejects orphaned tool_results without their parent tool_use.
        if messages.len() > MAX_HISTORY_MESSAGES {
            let start = messages.len() - MAX_HISTORY_MESSAGES;
            messages = messages.split_off(start);

            // Remove leading orphaned tool_result messages (user role with
            // tool_result JSON content) that lost their parent assistant+tool_use
            // in the split. These are encoded as Simple content containing
            // "\"type\":\"tool_result\"" JSON.
            while let Some(msg) = messages.first() {
                if msg.role != LlmRole::User {
                    break;
                }
                let is_tool_result = msg.text().contains("\"type\":\"tool_result\"");
                if is_tool_result {
                    tracing::debug!(
                        "Dropping orphaned tool_result from history cap (parent tool_use was cut)"
                    );
                    messages.remove(0);
                } else {
                    break;
                }
            }
        }

        // Ensure alternating roles.
        // fix_conversation_messages in response.rs handles this too,
        // but we do a basic cleanup here to avoid issues.
        let mut result: Vec<ChatMessage> = Vec::with_capacity(messages.len());
        // Ensure first message is from user (trim leading non-user messages)
        let start = messages
            .iter()
            .position(|m| m.role == LlmRole::User)
            .unwrap_or(0);
        let msgs = messages.into_iter().skip(start);
        for msg in msgs {
            if let Some(last) = result.last_mut() {
                if last.role == msg.role {
                    // Same role as previous — only merge simple text messages.
                    // Tool_use/tool_result blocks must not be merged (would lose structure).
                    let last_has_blocks = matches!(&last.content,
                        rustycode_llm::provider::MessageContent::Blocks(blocks) if !blocks.is_empty());
                    let msg_has_blocks = matches!(&msg.content,
                        rustycode_llm::provider::MessageContent::Blocks(blocks) if !blocks.is_empty());

                    if !last_has_blocks && !msg_has_blocks {
                        tracing::debug!("Merging consecutive {:?} messages in history", msg.role);
                        let existing = last.text();
                        let incoming = msg.text();
                        let merged = format!("{}\n\n{}", existing, incoming);
                        last.content = MessageContent::simple(&merged);
                        continue;
                    }
                    // If either has blocks, don't merge — append as-is.
                    // The API-side fix_conversation_messages will handle the ordering.
                }
            }
            result.push(msg);
        }
        result
    }

    /// Find the index of the last assistant message, searching from the end.
    pub(crate) fn last_assistant_index(&self) -> Option<usize> {
        self.messages
            .iter()
            .rposition(|m| m.role == crate::ui::message::MessageRole::Assistant)
    }

    /// Get an immutable reference to the last assistant message.
    pub(crate) fn last_assistant_message(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == crate::ui::message::MessageRole::Assistant)
    }

    /// Get a mutable reference to the last assistant message.
    pub(crate) fn last_assistant_message_mut(&mut self) -> Option<&mut Message> {
        self.messages
            .iter_mut()
            .rev()
            .find(|m| m.role == crate::ui::message::MessageRole::Assistant)
    }

    /// Check whether the last assistant message has any tool executions.
    pub(crate) fn last_assistant_has_tools(&self) -> bool {
        self.last_assistant_message()
            .is_some_and(|m| m.tool_executions.as_ref().is_some_and(|t| !t.is_empty()))
    }

    /// Check whether the last assistant message has no text content and no tools.
    pub(crate) fn last_assistant_is_empty(&self) -> bool {
        self.last_assistant_message().is_some_and(|m| {
            m.content.is_empty()
                && m.thinking.is_none()
                && m.tool_executions.as_ref().is_none_or(|t| t.is_empty())
        })
    }
}
