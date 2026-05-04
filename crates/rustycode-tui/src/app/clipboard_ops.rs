//! Clipboard and export operations
//!
//! Handles copying messages and conversations to clipboard, and exporting to files.

use super::event_loop::TUI;
use crate::ui::message::MessageRole;
use anyhow::Result;

impl TUI {
    fn copy_text_with_feedback(&mut self, content: String, success_label: &str) -> Result<()> {
        use crate::clipboard::copy_text_to_clipboard_both;

        let chars = content.chars().count();
        match copy_text_to_clipboard_both(&content) {
            Ok(()) => {
                tracing::debug!("Copied clipboard payload ({} chars)", chars);
                self.add_system_message(format!("✓ {} ({} chars)", success_label, chars));
            }
            Err(e) => {
                tracing::error!("Failed to copy clipboard payload: {}", e);
                self.toast_manager.error(format!("Failed to copy: {}", e));
                self.add_system_message(format!("[X] Failed to copy: {}", e));
                return Err(e);
            }
        }

        self.dirty = true;
        Ok(())
    }

    /// Copy a transcript range to clipboard.
    pub(crate) fn copy_message_range(&mut self, start: usize, end: usize) -> Result<()> {
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let end = end.min(self.messages.len().saturating_sub(1));
        let start = start.min(end);

        let mut conversation = Vec::new();
        for msg in self.messages.iter().take(end + 1).skip(start) {
            match msg.role {
                MessageRole::User => {
                    conversation.push(format!("User: {}", msg.content));
                }
                MessageRole::Assistant => {
                    conversation.push(format!("Assistant: {}", msg.content));
                }
                MessageRole::System => {}
            }
        }

        if conversation.is_empty() {
            self.add_system_message("No conversation text in selection".to_string());
            self.dirty = true;
            return Ok(());
        }

        self.copy_text_with_feedback(conversation.join("\n\n"), "Copied selection")
    }

    /// Copy the sidebar contents to clipboard.
    pub(crate) fn copy_sidebar_text(&mut self) -> Result<()> {
        let content = self.session_sidebar.copyable_text();
        if content.trim().is_empty() {
            self.add_system_message("No sidebar text to copy".to_string());
            self.dirty = true;
            return Ok(());
        }

        self.copy_text_with_feedback(content, "Copied sidebar")
    }

    /// Copy selected message to clipboard
    pub(crate) fn copy_selected_message(&mut self) -> Result<()> {
        if self.selected_message < self.messages.len() {
            let msg = &self.messages[self.selected_message];

            // Get the message content (without the role prefix)
            let content = msg.content.clone();
            self.copy_text_with_feedback(content, "Copied message")?;
        }

        Ok(())
    }

    /// Copy the last AI assistant response to clipboard (Ctrl+Y)
    pub(crate) fn copy_last_ai_response(&mut self) -> Result<()> {
        // Find the last assistant message
        let last_ai_idx = self
            .messages
            .iter()
            .rposition(|msg| matches!(msg.role, crate::ui::message::MessageRole::Assistant));

        match last_ai_idx {
            Some(idx) => {
                let content = self.messages[idx].content.clone();
                self.copy_text_with_feedback(content, "Copied last AI response")?;
            }
            None => {
                self.add_system_message("No AI response to copy yet".to_string());
            }
        }

        self.dirty = true;
        Ok(())
    }

    /// Copy entire conversation to clipboard (excludes system messages and tool panel)
    pub(crate) fn copy_all_conversation(&mut self) -> Result<()> {
        let content = self
            .messages
            .iter()
            .filter_map(|msg| match msg.role {
                MessageRole::User => Some(format!("User: {}", msg.content)),
                MessageRole::Assistant => Some(format!("Assistant: {}", msg.content)),
                MessageRole::System => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        if content.is_empty() {
            self.add_system_message("No conversation text to copy".to_string());
            self.dirty = true;
            return Ok(());
        }

        self.copy_text_with_feedback(content, "Copied conversation")
    }

    /// Export current conversation to file (Ctrl+Shift+E)
    pub(crate) fn export_conversation(&mut self) -> Result<()> {
        use crate::ui::message_export::{ConversationExporter, ExportFormat, ExportOptions};
        use dirs::home_dir;

        // Determine export directory
        let export_dir = if let Some(home) = home_dir() {
            home.join(".rustycode").join("exports")
        } else {
            std::path::PathBuf::from("./exports")
        };

        // Create exporter
        let exporter = ConversationExporter::new(export_dir.clone())?;

        // Use default export options (include tools, exclude thinking/metadata/timestamps)
        let options = ExportOptions::default();

        // Export as markdown
        let path = exporter.export(&self.messages, ExportFormat::Markdown, options)?;

        let msg_count = self
            .messages
            .iter()
            .filter(|m| {
                matches!(
                    m.role,
                    crate::ui::message::MessageRole::User
                        | crate::ui::message::MessageRole::Assistant
                )
            })
            .count();

        tracing::debug!("Exported {} messages to {}", msg_count, path.display());

        self.toast_manager
            .success(format!("✓ Exported {} messages", msg_count));

        let success_msg = format!(
            "[OK] Exported {} messages to {}",
            msg_count,
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("conversation.md")
        );
        self.add_system_message(success_msg);

        self.dirty = true;
        Ok(())
    }

    /// Regenerate the last AI response
    pub(crate) fn regenerate_last_response(&mut self) -> Result<()> {
        // Don't regenerate if we're already streaming
        if self.is_streaming {
            self.add_system_message(
                "⚠️  Cannot regenerate while streaming. Please wait.".to_string(),
            );
            return Ok(());
        }

        // Find the last AI message (assistant role)
        let last_ai_msg_idx = self
            .messages
            .iter()
            .rposition(|msg| msg.role == crate::ui::message::MessageRole::Assistant);

        let last_ai_msg_idx = match last_ai_msg_idx {
            Some(idx) => idx,
            None => {
                self.add_system_message(
                    "⚠️  No AI response to regenerate. Send a message first.".to_string(),
                );
                return Ok(());
            }
        };

        // Get the message before the AI message (the user's prompt)
        let user_msg_idx = match last_ai_msg_idx.checked_sub(1) {
            Some(idx) => idx,
            None => {
                self.add_system_message(
                    "⚠️  Cannot find user prompt to regenerate from.".to_string(),
                );
                return Ok(());
            }
        };

        let user_prompt = self.messages[user_msg_idx].content.clone();

        // Show regeneration started message
        let regen_msg = "🔄 Regenerating response...".to_string();
        self.add_system_message(regen_msg);

        // Remove the old AI message
        self.messages.remove(last_ai_msg_idx);
        if last_ai_msg_idx < self.selected_message {
            self.selected_message = self.selected_message.saturating_sub(1);
        } else if last_ai_msg_idx == self.selected_message && !self.messages.is_empty() {
            self.selected_message = self.selected_message.min(self.messages.len() - 1);
        }

        // Update dirty flag
        self.dirty = true;

        // Send the user prompt again to get a new response
        let _workspace_context = self.workspace_context.clone();
        let history = self.build_conversation_history();

        // Set streaming state before send to prevent double-Enter races
        self.is_streaming = true;
        self.chunks_received = 0;
        self.thinking_chunks_received = 0;
        self.stream_start_time = Some(std::time::Instant::now());
        self.current_stream_content.clear();
        self.streaming_render_buffer =
            crate::app::streaming_render_buffer::StreamingRenderBuffer::new();
        self.tool_panel_history.clear();
        self.tool_panel_selected_index = None;
        self.showing_tool_result = false;
        self.active_tools.clear();

        if let Err(e) = self
            .services
            .send_message_with_history(user_prompt, Some(history), None)
        {
            tracing::error!("Failed to regenerate response: {}", e);
            self.reset_streaming_state();
            self.active_tools.clear();
            self.add_system_message(format!("Regeneration failed: {}", e));
        } else {
            // Create empty assistant message for streaming to fill
            let assistant_msg = crate::ui::message::Message::assistant(String::new());
            self.messages.push(assistant_msg);
            self.auto_scroll();
        }

        Ok(())
    }

    /// Undo the last task extraction
    pub(crate) fn undo_last_extraction(&mut self) -> Result<()> {
        if let Some((old_tasks, old_todos)) = self.last_extraction.take() {
            // Update workspace tasks
            self.workspace_tasks.tasks = old_tasks;
            self.workspace_tasks.todos = old_todos;

            // Save the reverted tasks
            if let Err(e) = crate::tasks::save_tasks(&self.workspace_tasks) {
                self.add_system_message(format!("❌ Failed to save reverted tasks: {}", e));
                return Err(e.into());
            }

            // Update analytics
            if let Err(e) = crate::extraction_analytics::record_undo() {
                tracing::warn!("Failed to record extraction undo: {}", e);
            }

            self.add_system_message(
                "✅ Successfully reverted the last task extraction.".to_string(),
            );
            self.dirty = true;
            Ok(())
        } else {
            self.add_system_message("⚠️  No recent task extraction to revert.".to_string());
            Ok(())
        }
    }
}
