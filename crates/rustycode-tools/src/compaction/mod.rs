//! Conversation Compaction Service
//!
//! Manages context window compaction by summarizing older messages
//! when token/turn thresholds are exceeded.

pub mod types;

#[cfg(test)]
mod tests;

pub use types::{
    CompactionConfig, CompactionStage, CompactionStrategy, ConversationMessage, MessageRole,
};

use rustycode_protocol::Message;

/// Default batch size for tool pair summarization
pub const DEFAULT_BATCH_SIZE: usize = 10;

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactionResult {
    /// Number of messages removed
    pub messages_removed: usize,
    /// Number of tokens saved
    pub tokens_saved: usize,
    /// The generated summary of removed messages
    pub summary: String,
    /// Strategy used for compaction
    pub strategy: CompactionStrategy,
}

/// Tracks compaction state and performs compaction when needed
#[derive(Debug)]
pub struct Compactor {
    config: CompactionConfig,
    turn_count: usize,
    total_tokens: usize,
    messages: Vec<ConversationMessage>,
    compaction_count: usize,
}

impl Compactor {
    pub const fn new(config: CompactionConfig) -> Self {
        Self {
            config,
            turn_count: 0,
            total_tokens: 0,
            messages: Vec::new(),
            compaction_count: 0,
        }
    }

    /// Add a message and check if compaction is needed
    pub fn add_message(&mut self, message: ConversationMessage) -> CompactionAction {
        self.total_tokens = self.total_tokens.saturating_add(message.token_count);
        if message.role == MessageRole::User || message.role == MessageRole::Assistant {
            self.turn_count = self.turn_count.saturating_add(1);
        }
        self.messages.push(message);

        if self.should_compact() {
            CompactionAction::Compact
        } else {
            CompactionAction::None
        }
    }

    /// Add a protocol Message and check if compaction is needed
    pub fn add_message_from_protocol(&mut self, message: Message) -> CompactionAction {
        let conv_message = ConversationMessage::from(message);
        self.add_message(conv_message)
    }

    /// Check if compaction thresholds are exceeded
    pub const fn should_compact(&self) -> bool {
        self.total_tokens >= self.config.max_tokens
            || self.turn_count >= self.config.max_turns
            || self.messages.len() >= self.config.max_messages
    }

    /// Perform compaction: remove old messages and generate a summary
    pub fn compact(&mut self) -> Option<CompactionResult> {
        if self.messages.len() <= self.config.retention_window {
            return None;
        }

        // Try progressive compaction first if enabled
        if self.config.progressive_compaction {
            if let Some(result) = self.compact_progressive() {
                return Some(result);
            }
        }

        // Fall back to simple front-removal compaction
        self.compact_simple()
    }

    /// Progressive compaction (inspired by goose).
    ///
    /// Tries removing tool responses at increasing percentages:
    /// 0% (just summarize) -> 10% -> 20% -> 50% -> 100%.
    /// Tool responses are removed from the middle outward to preserve
    /// early context and recent messages.
    fn compact_progressive(&mut self) -> Option<CompactionResult> {
        let percentages = [0, 10, 20, 50, 100];
        let protected = self.config.retention_window;

        // Find tool response indices in the removable range
        let removable_end = self.messages.len().saturating_sub(protected);
        let tool_indices: Vec<usize> = self.messages[..removable_end]
            .iter()
            .enumerate()
            .filter(|(_, m)| m.role == MessageRole::Tool)
            .map(|(i, _)| i)
            .collect();

        // If no tool responses, fall back to simple compaction
        if tool_indices.is_empty() {
            return None;
        }

        for &remove_percent in &percentages {
            let indices_to_remove = Self::middle_out_indices(&tool_indices, remove_percent);
            if indices_to_remove.is_empty() && remove_percent == 0 {
                // At 0%, just generate a summary without removing messages
                continue;
            }

            // Remove selected indices (in reverse order to preserve positions)
            let mut removed_messages = Vec::new();
            let mut tokens_removed = 0usize;
            for &idx in indices_to_remove.iter().rev() {
                if idx < self.messages.len() {
                    let msg = self.messages.remove(idx);
                    tokens_removed += msg.token_count;
                    removed_messages.push(msg);
                }
            }

            if !removed_messages.is_empty() {
                let summary = self.generate_summary(&removed_messages);
                self.total_tokens = self.total_tokens.saturating_sub(tokens_removed);
                self.recount_turns();
                self.compaction_count = self.compaction_count.saturating_add(1);

                return Some(CompactionResult {
                    messages_removed: removed_messages.len(),
                    tokens_saved: tokens_removed,
                    summary,
                    strategy: CompactionStrategy::ProgressiveToolRemoval(remove_percent),
                });
            }
        }

        None
    }

    /// Simple front-removal compaction (original strategy).
    fn compact_simple(&mut self) -> Option<CompactionResult> {
        let split_point = self.messages.len() - self.config.retention_window;
        let removed: Vec<_> = self.messages.drain(..split_point).collect();

        let tokens_removed: usize = removed.iter().map(|m| m.token_count).sum();
        let messages_removed = removed.len();

        let summary = self.generate_summary(&removed);

        self.total_tokens = self.total_tokens.saturating_sub(tokens_removed);
        self.recount_turns();
        self.compaction_count = self.compaction_count.saturating_add(1);

        Some(CompactionResult {
            messages_removed,
            tokens_saved: tokens_removed,
            summary,
            strategy: CompactionStrategy::FrontRemoval,
        })
    }

    /// Check if token usage exceeds the auto-compact threshold.
    ///
    /// Returns true when current usage exceeds `auto_compact_threshold` of `max_tokens`.
    pub fn should_auto_compact(&self, context_limit: usize) -> bool {
        if context_limit == 0 {
            return false;
        }
        let ratio = self.total_tokens as f64 / context_limit as f64;
        let threshold = self.config.auto_compact_threshold;
        threshold > 0.0 && threshold <= 1.0 && ratio > threshold
    }

    /// Compute how many tool call responses to summarize for a given context limit.
    ///
    /// Returns the number of tool responses that can be summarized to save space.
    pub fn tool_summarization_cutoff(&self, context_limit: usize) -> usize {
        let threshold = self.config.auto_compact_threshold;
        let effective_limit = (context_limit as f64 * threshold) as usize;
        (3 * effective_limit / 20_000).clamp(10, 500)
    }

    /// Middle-out index selection.
    ///
    /// Given a list of indices and a removal percentage, returns indices
    /// to remove by expanding outward from the middle. This preserves
    /// early context (system messages, first user message) and recent
    /// context (last few messages).
    fn middle_out_indices(indices: &[usize], remove_percent: u32) -> Vec<usize> {
        if indices.is_empty() || remove_percent == 0 {
            return Vec::new();
        }

        let num_to_remove = (indices.len().saturating_mul(remove_percent as usize) / 100)
            .min(indices.len())
            .max(1);
        let middle = indices.len() / 2;
        let mut result = Vec::with_capacity(num_to_remove);

        for i in 0..num_to_remove {
            if i % 2 == 0 {
                let offset = i / 2;
                if middle > offset {
                    result.push(indices[middle - offset - 1]);
                }
            } else {
                let offset = i / 2;
                if middle + offset < indices.len() {
                    result.push(indices[middle + offset]);
                }
            }
        }

        result
    }

    /// Recount turns from current messages.
    fn recount_turns(&mut self) {
        self.turn_count = self
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User || m.role == MessageRole::Assistant)
            .count();
    }

    /// Get current token count
    pub const fn token_count(&self) -> usize {
        self.total_tokens
    }

    /// Get current turn count
    pub const fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// Get number of messages
    pub const fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get compaction count
    pub const fn compaction_count(&self) -> usize {
        self.compaction_count
    }

    /// Identify tool call IDs eligible for summarization.
    ///
    /// Returns tool call IDs that exceed the cutoff and should be summarized
    /// in batches. Protects the most recent `protect_last_n` tool calls from
    /// summarization to preserve current-turn context.
    ///
    /// Inspired by goose's `tool_ids_to_summarize` pattern.
    pub fn tool_ids_to_summarize(
        &self,
        cutoff: usize,
        protect_last_n: usize,
        batch_size: usize,
    ) -> Vec<String> {
        let mut tool_call_ids: Vec<String> = Vec::new();

        for (i, msg) in self.messages.iter().enumerate() {
            if msg.role != MessageRole::Tool {
                continue;
            }
            // Use index-based pseudo-ID for tracking
            tool_call_ids.push(format!("tool_{i}"));
        }

        // Never summarize the last N tool calls (current turn)
        let eligible = tool_call_ids.len().saturating_sub(protect_last_n);
        if eligible <= cutoff + batch_size {
            return Vec::new();
        }

        tool_call_ids.into_iter().take(batch_size).collect()
    }

    /// Get a reference to current messages
    pub fn messages(&self) -> &[ConversationMessage] {
        &self.messages
    }

    /// Reset state
    pub fn reset(&mut self) {
        self.turn_count = 0;
        self.total_tokens = 0;
        self.messages.clear();
        self.compaction_count = 0;
    }

    fn generate_summary(&self, removed: &[ConversationMessage]) -> String {
        let mut parts = Vec::new();
        let mut current_role = None;
        let mut current_content = String::new();

        for msg in removed {
            if current_role.as_ref() != Some(&msg.role) {
                if let Some(role) = current_role.take() {
                    if !current_content.is_empty() {
                        parts.push(format!("{role:?}"));
                        parts.push(current_content.clone());
                    }
                }
                current_role = Some(msg.role.clone());
                current_content = msg.content.clone();
            } else {
                current_content.push('\n');
                current_content.push_str(&msg.content);
            }
        }

        if let Some(role) = current_role {
            if !current_content.is_empty() {
                parts.push(format!("{role:?}"));
                parts.push(current_content);
            }
        }

        // Truncate summary if too long
        let summary = parts.join("\n");
        if summary.len() > 2000 {
            let truncated = if summary.is_char_boundary(2000) {
                &summary[..2000]
            } else {
                let mut b = 2000;
                while b > 0 && !summary.is_char_boundary(b) {
                    b -= 1;
                }
                &summary[..b]
            };
            format!(
                "[Compacted {} messages, {} tokens] {}",
                removed.len(),
                removed.iter().map(|m| m.token_count).sum::<usize>(),
                truncated
            )
        } else {
            summary
        }
    }
}

/// Action to take after adding a message
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompactionAction {
    /// No action needed
    None,
    /// Compaction should be performed
    Compact,
}

impl CompactionAction {
    /// Check if compaction should be performed
    pub const fn should_compact(&self) -> bool {
        matches!(self, Self::Compact)
    }
}
