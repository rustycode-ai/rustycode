//! Selective Message Inclusion
//!
//! Provides priority-based message filtering to optimize context window usage.
//! Instead of sending all messages to the LLM, intelligently select based on:
//! - Message priority (Critical, High, Normal, Skippable)
//! - Recency (last N turns always included)
//! - Relevance to current task
//!
//! ## Usage
//!
//! ```ignore
//! use rustycode_protocol::message_selector::{MessageSelector, MessagePriority, SelectionConfig};
//!
//! let config = SelectionConfig {
//!     token_budget: 50_000,
//!     always_include_system: true,
//!     retention_window: 10, // Last 10 turns always included
//! };
//!
//! let selector = MessageSelector::new(config);
//! let selected = selector.select(messages, &token_counter);
//! ```

use crate::Message;
use std::time::Duration;

/// Priority level for message inclusion
///
/// Note: Order matters for comparison! Derived `PartialOrd` uses declaration order,
/// so variants declared later have higher values. This ensures `Critical > High > Normal > Skippable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MessagePriority {
    /// Skippable: Intermediate reasoning, failed attempts, redundant tool calls
    /// Excluded first when budget is tight
    Skippable,
    /// Normal: Older conversation context
    /// Included if space permits
    Normal,
    /// High: Recent turns, current task context
    /// Included unless budget is extremely constrained
    High,
    /// Critical: System prompts, task definitions
    /// Always included regardless of token budget
    Critical,
}

/// Configuration for message selection
#[derive(Debug, Clone)]
pub struct SelectionConfig {
    /// Maximum tokens to include
    pub token_budget: usize,
    /// Whether to always include system messages
    pub always_include_system: bool,
    /// Number of recent turns to always include (High priority)
    pub retention_window: usize,
    /// Minimum priority to include (messages below this are excluded)
    pub min_priority: MessagePriority,
    /// Prefer including tool results with successful execution
    pub prefer_successful_tools: bool,
    /// Skip messages older than this duration
    pub max_age: Option<Duration>,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            token_budget: 50_000,
            always_include_system: true,
            retention_window: 10,
            min_priority: MessagePriority::Skippable,
            prefer_successful_tools: true,
            max_age: None,
        }
    }
}

/// Result of message selection
#[derive(Debug, Clone)]
pub struct SelectionResult {
    /// Selected messages
    pub messages: Vec<Message>,
    /// Total tokens in selected messages
    pub total_tokens: usize,
    /// Number of messages excluded
    pub excluded_count: usize,
    /// Tokens excluded
    pub excluded_tokens: usize,
    /// Breakdown by priority
    pub by_priority: PriorityBreakdown,
}

/// Breakdown of selection by priority level
#[derive(Debug, Clone, Default)]
pub struct PriorityBreakdown {
    pub critical_count: usize,
    pub critical_tokens: usize,
    pub high_count: usize,
    pub high_tokens: usize,
    pub normal_count: usize,
    pub normal_tokens: usize,
    pub skippable_count: usize,
    pub skippable_tokens: usize,
}

/// Token counter function type
pub type TokenCounter = dyn Fn(&Message) -> usize;

/// Selective message inclusion engine
pub struct MessageSelector {
    config: SelectionConfig,
}

impl MessageSelector {
    pub fn new(config: SelectionConfig) -> Self {
        Self { config }
    }

    /// Select messages based on priority and token budget
    ///
    /// Algorithm:
    /// 1. Assign priority to each message
    /// 2. Sort by priority (Critical first) then recency
    /// 3. Include messages until token budget is exhausted
    pub fn select(&self, messages: &[Message], token_counter: &TokenCounter) -> SelectionResult {
        let mut result = SelectionResult {
            messages: Vec::new(),
            total_tokens: 0,
            excluded_count: 0,
            excluded_tokens: 0,
            by_priority: PriorityBreakdown::default(),
        };

        // Compute priority for each message
        let mut scored: Vec<(usize, MessagePriority, usize)> = messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                let priority = self.compute_priority(messages, i);
                let tokens = token_counter(msg);
                (i, priority, tokens)
            })
            .collect();

        // Sort by priority (Critical first) then by recency (recent first)
        scored.sort_by(|a, b| {
            // Primary: priority (Critical > High > Normal > Skippable)
            let priority_cmp = a.1.cmp(&b.1);
            if priority_cmp != std::cmp::Ordering::Equal {
                return priority_cmp;
            }
            // Secondary: recency (higher index = more recent)
            b.0.cmp(&a.0)
        });

        // Select messages within budget
        let mut running_total = 0;
        for (idx, priority, tokens) in scored {
            let msg = &messages[idx];

            // Check if we can include this message
            if running_total + tokens > self.config.token_budget {
                // Budget exceeded - exclude this message
                result.excluded_count += 1;
                result.excluded_tokens += tokens;
                continue;
            }

            // Check minimum priority threshold
            if priority < self.config.min_priority {
                result.excluded_count += 1;
                result.excluded_tokens += tokens;
                continue;
            }

            // Check max age
            if let Some(max_age) = self.config.max_age {
                let now = chrono::Utc::now();
                let age = now.signed_duration_since(msg.timestamp);
                if age.num_seconds() > max_age.as_secs().try_into().unwrap_or(i64::MAX) {
                    result.excluded_count += 1;
                    result.excluded_tokens += tokens;
                    continue;
                }
            }

            // Include this message
            result.messages.push(msg.clone());
            running_total += tokens;
            result.total_tokens += tokens;
            Self::track_included(&mut result.by_priority, priority, tokens);
        }

        // Sort selected messages by original order (preserve conversation flow)
        result.messages.sort_by_key(|m| m.timestamp);

        result
    }

    /// Compute priority for a message at the given index
    fn compute_priority(&self, messages: &[Message], idx: usize) -> MessagePriority {
        let msg = &messages[idx];

        // System messages are always Critical
        if msg.is_system() && self.config.always_include_system {
            return MessagePriority::Critical;
        }

        // Check if within retention window (last N turns)
        let turns_from_end = messages.len().saturating_sub(idx);
        if turns_from_end <= self.config.retention_window {
            return MessagePriority::High;
        }

        // Check if message is skippable
        if self.is_skippable(msg) {
            return MessagePriority::Skippable;
        }

        // Default to Normal for older messages
        MessagePriority::Normal
    }

    /// Check if a message should be marked as Skippable
    fn is_skippable(&self, msg: &Message) -> bool {
        let content = msg.content.to_text();

        // Skip intermediate reasoning markers
        if content.contains("<thinking>") || content.contains("</thinking>") {
            return true;
        }

        // Skip failed tool attempts (heuristic: contains error patterns)
        if content.contains("Error:") || content.contains("Tool execution failed") {
            return true;
        }

        // Skip redundant tool results (heuristic: same file read multiple times)
        // This would require more sophisticated deduplication

        false
    }

    /// Track included message in breakdown
    fn track_included(breakdown: &mut PriorityBreakdown, priority: MessagePriority, tokens: usize) {
        match priority {
            MessagePriority::Critical => {
                breakdown.critical_count += 1;
                breakdown.critical_tokens += tokens;
            }
            MessagePriority::High => {
                breakdown.high_count += 1;
                breakdown.high_tokens += tokens;
            }
            MessagePriority::Normal => {
                breakdown.normal_count += 1;
                breakdown.normal_tokens += tokens;
            }
            MessagePriority::Skippable => {
                breakdown.skippable_count += 1;
                breakdown.skippable_tokens += tokens;
            }
        }
    }

    /// Quick estimation of whether compaction/selection is needed
    pub fn should_select(&self, messages: &[Message], current_tokens: usize) -> bool {
        current_tokens > self.config.token_budget
            || messages.len() > self.config.retention_window * 2
    }
}

/// Helper function for quick message filtering
pub fn filter_messages(
    messages: &[Message],
    token_budget: usize,
    token_counter: impl Fn(&Message) -> usize + 'static,
) -> Vec<Message> {
    let selector = MessageSelector::new(SelectionConfig {
        token_budget,
        ..Default::default()
    });

    selector.select(messages, &token_counter).messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, MessageContent};

    fn make_message(role: &str, content: &str, offset_secs: i64) -> Message {
        let timestamp = chrono::Utc::now() + chrono::Duration::seconds(offset_secs);
        Message {
            role: crate::message::MessageRole::from(role),
            content: MessageContent::Simple(content.to_string()),
            timestamp,
            metadata: crate::MessageMetadata::default(),
        }
    }

    #[test]
    fn test_priority_assignment() {
        let messages = vec![
            make_message("system", "System prompt", -100),
            make_message("user", "User asks", -90),
            make_message("assistant", "Assistant responds", -80),
        ];

        let selector = MessageSelector::new(SelectionConfig {
            retention_window: 2,
            ..Default::default()
        });

        // Last 2 should be High priority
        assert_eq!(
            selector.compute_priority(&messages, 1),
            MessagePriority::High
        );
        assert_eq!(
            selector.compute_priority(&messages, 2),
            MessagePriority::High
        );

        // System should be Critical
        assert_eq!(
            selector.compute_priority(&messages, 0),
            MessagePriority::Critical
        );
    }

    #[test]
    fn test_skippable_detection() {
        let selector = MessageSelector::new(SelectionConfig::default());

        let thinking_msg = Message::assistant("<thinking>Let me analyze this...</thinking>");
        assert!(selector.is_skippable(&thinking_msg));

        let error_msg = Message::assistant("Error: Tool execution failed");
        assert!(selector.is_skippable(&error_msg));

        let normal_msg = Message::assistant("I've completed the task");
        assert!(!selector.is_skippable(&normal_msg));
    }

    #[test]
    fn test_message_selection_within_budget() {
        let messages = vec![
            make_message("system", "System", -100),
            make_message("user", "Hello", -90),
            make_message("assistant", "Hi there", -80),
        ];

        let selector = MessageSelector::new(SelectionConfig {
            token_budget: 1000,
            retention_window: 10,
            ..Default::default()
        });

        let counter = |m: &Message| m.content.to_text().len() / 4;
        let result = selector.select(&messages, &counter);

        // All messages should be included (well within budget)
        assert_eq!(result.messages.len(), 3);
        assert!(result.total_tokens > 0);
    }

    #[test]
    fn test_message_selection_exceeds_budget() {
        let messages = vec![
            make_message("system", "System prompt here", -100),
            make_message("user", "User message", -90),
            make_message("assistant", &"x".repeat(1000), -80), // Large message
        ];

        let selector = MessageSelector::new(SelectionConfig {
            token_budget: 50, // Very tight budget
            retention_window: 2,
            ..Default::default()
        });

        let counter = |m: &Message| m.content.to_text().len() / 4;
        let result = selector.select(&messages, &counter);

        // Should select some messages but not all
        assert!(result.excluded_count > 0);
        assert!(result.total_tokens <= 50);
    }

    #[test]
    fn test_filter_messages_helper() {
        let messages = vec![
            make_message("user", "Hello", -10),
            make_message("assistant", "Hi", -9),
        ];

        let filtered = filter_messages(&messages, 1000, |m| m.content.to_text().len() / 4);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_priority_breakdown() {
        let messages = vec![
            make_message("system", "System", -100),
            make_message("user", "User1", -90),
            make_message("assistant", "Assistant1", -80),
            make_message("user", "User2", -70),
            make_message("assistant", "Assistant2", -60),
        ];

        let selector = MessageSelector::new(SelectionConfig {
            token_budget: 1000,
            retention_window: 2, // Only last 2 are High
            always_include_system: true,
            ..Default::default()
        });

        let counter = |m: &Message| m.content.to_text().len() / 4;
        let result = selector.select(&messages, &counter);

        // System should be Critical, last 2 should be High, middle 2 should be Normal
        assert!(result.by_priority.critical_count >= 1); // System
        assert_eq!(result.by_priority.high_count, 2); // Last 2 turns
    }
}
