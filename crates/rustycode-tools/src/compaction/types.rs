use rustycode_protocol::Message;
use std::time::Instant;

/// Configuration for when compaction should trigger
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum tokens before compaction triggers
    pub max_tokens: usize,
    /// Maximum turns before compaction triggers
    pub max_turns: usize,
    /// Maximum messages before compaction triggers
    pub max_messages: usize,
    /// Number of recent messages to always preserve
    pub retention_window: usize,
    /// Auto-compact threshold as fraction of context (0.0-1.0), e.g. 0.8 = compact at 80%
    pub auto_compact_threshold: f64,
    /// Whether to use progressive compaction (remove tool responses incrementally)
    pub progressive_compaction: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_tokens: 100_000,
            max_turns: 50,
            max_messages: 100,
            retention_window: 10,
            auto_compact_threshold: 0.8,
            progressive_compaction: true,
        }
    }
}

/// A conversation message with metadata for compaction
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub content: String,
    pub token_count: usize,
    pub timestamp: Instant,
}

impl From<Message> for ConversationMessage {
    fn from(msg: Message) -> Self {
        let content = msg.content.to_text();
        // Rough estimation: 1 token ≈ 4 characters
        let token_count = content.len().div_ceil(4);

        Self {
            role: MessageRole::from(msg.role.as_str()),
            content,
            token_count,
            timestamp: Instant::now(),
        }
    }
}

/// Role of a message in the conversation
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn parse_role(role: &str) -> Self {
        match role {
            "system" => Self::System,
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "tool" => Self::Tool,
            _ => Self::User, // Default to user for unknown roles
        }
    }
}

impl From<&str> for MessageRole {
    fn from(role: &str) -> Self {
        Self::parse_role(role)
    }
}

/// Strategy used for compaction
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompactionStrategy {
    /// Simple front-removal (original strategy)
    FrontRemoval,
    /// Progressive tool response removal with percentage
    ProgressiveToolRemoval(u32),
}

/// Age-aware compaction stages for conversation turns.
///
/// Determines how aggressively to compress messages based on their
/// position in the conversation timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompactionStage {
    /// No compression — full original content preserved.
    FullFidelity,
    /// Light compression — tool outputs replaced with 1-line summary.
    Microcompact,
    /// Aggressive compression — entire segment replaced with paragraph summary.
    ContextCollapse,
    /// Complete removal — oldest turns removed entirely.
    HistorySnip,
}

impl CompactionStage {
    /// Determine the compaction stage for a turn based on its position.
    ///
    /// Stage boundaries (from newest to oldest):
    /// - [total - `recent_window`, total) -> `FullFidelity`
    /// - [total * 0.5, total - `recent_window`) -> `Microcompact`
    /// - [total * 0.2, total * 0.5) -> `ContextCollapse`
    /// - [0, total * 0.2) -> `HistorySnip`
    pub fn stage_for_age(turn_index: usize, total_turns: usize, recent_window: usize) -> Self {
        if total_turns == 0 || turn_index >= total_turns.saturating_sub(recent_window) {
            return Self::FullFidelity;
        }

        let midpoint = total_turns / 2;
        let quarter = total_turns / 5;

        if turn_index >= midpoint {
            Self::Microcompact
        } else if turn_index >= quarter {
            Self::ContextCollapse
        } else {
            Self::HistorySnip
        }
    }

    /// Apply microcompact compression to a single message.
    ///
    /// For tool messages: replaces content with a one-line summary.
    /// For other messages: returns content unchanged.
    pub fn microcompact_message(msg: &ConversationMessage) -> String {
        if msg.role == MessageRole::Tool {
            let first_line = msg.content.lines().next().unwrap_or("");
            let truncated = if first_line.len() > 80 {
                let mut end = 80;
                while end > 0 && !first_line.is_char_boundary(end) {
                    end -= 1;
                }
                &first_line[..end]
            } else {
                first_line
            };
            format!("[tool output: {truncated}]")
        } else {
            msg.content.clone()
        }
    }

    /// Apply context collapse to a slice of messages.
    ///
    /// Replaces the entire segment with a single paragraph summary.
    pub fn context_collapse_messages(msgs: &[ConversationMessage]) -> String {
        if msgs.is_empty() {
            return String::new();
        }

        let user_count = msgs.iter().filter(|m| m.role == MessageRole::User).count();
        let tool_count = msgs.iter().filter(|m| m.role == MessageRole::Tool).count();
        let assistant_count = msgs
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();

        let topics: Vec<&str> = msgs
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .filter_map(|m| m.content.lines().next())
            .take(3)
            .collect();

        let topics_str = if topics.is_empty() {
            String::new()
        } else {
            format!(" Topics: {}.", topics.join("; "))
        };

        format!(
            "[context collapsed: {} messages ({} user, {} assistant, {} tool).{topics_str}]",
            msgs.len(),
            user_count,
            assistant_count,
            tool_count,
        )
    }
}
