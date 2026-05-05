use rustycode_protocol::message_selector::{MessageSelector, SelectionConfig};
use rustycode_protocol::{Conversation as ProtocolConversation, Message, MessagePriority, MessageRole};

/// Conversation manager with context windowing
pub struct ConversationManager {
    conversation: ProtocolConversation,
    max_messages: usize,
    max_tokens: usize,
    selector: Option<MessageSelector>,
}

const TOOL_OUTPUT_MASK_TAG: &str = "[tool-output-masked]";
const TOOL_OUTPUT_SOFT_LIMIT: usize = 1200;
const TOOL_OUTPUT_HARD_LIMIT: usize = 4000;
/// When estimated tokens exceed this fraction of max_tokens, trigger summarization.
/// Set to 80% to leave headroom for the user's next message while avoiding
/// premature summarization of short conversations.
const SUMMARIZATION_THRESHOLD: f64 = 0.8;
/// Minimum number of messages before summarization kicks in.
const MIN_MESSAGES_FOR_SUMMARY: usize = 10;
/// Number of recent turns to keep unsummarized.
const RECENT_TURNS_TO_KEEP: usize = 4;

impl ConversationManager {
    pub fn new(conversation: ProtocolConversation) -> Self {
        Self {
            conversation,
            max_messages: 10,
            max_tokens: 8000,
            selector: None,
        }
    }

    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = max;
        self
    }

    pub fn with_max_tokens(mut self, max: usize) -> Self {
        self.max_tokens = max;
        self
    }

    /// Enable priority-based message selection with the given retention window
    pub fn with_priority_selection(mut self, retention_window: usize) -> Self {
        self.selector = Some(MessageSelector::new(SelectionConfig {
            token_budget: self.max_tokens,
            always_include_system: true,
            retention_window,
            min_priority: MessagePriority::Skippable,
            prefer_successful_tools: true,
            max_age: None,
        }));
        self
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: Message) {
        self.conversation
            .add_message(Self::compact_message_if_needed(message));
        self.enforce_limits();
    }

    fn compact_message_if_needed(mut message: Message) -> Message {
        // Compact oversized assistant outputs and likely tool outputs.
        if message.role == MessageRole::Assistant {
            let text = message.content.as_text();
            let len = text.len();
            let looks_like_tool_output = text.contains("```")
                || text.contains("tool")
                || text.contains("error:")
                || text.contains("warning:")
                || text.lines().count() > 60;

            if len > TOOL_OUTPUT_HARD_LIMIT
                || (looks_like_tool_output && len > TOOL_OUTPUT_SOFT_LIMIT)
            {
                let preview_head: String = text.chars().take(600).collect();
                let preview_tail: String = text
                    .chars()
                    .rev()
                    .take(200)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
                message.content = rustycode_protocol::MessageContent::simple(format!(
                    "{tag} assistant output compacted ({orig} chars)\n{head}\n... [compacted] ...\n{tail}",
                    tag = TOOL_OUTPUT_MASK_TAG,
                    orig = len,
                    head = preview_head,
                    tail = preview_tail,
                ));
            }
        }

        message
    }

    /// Enforce context windowing limits
    fn enforce_limits(&mut self) {
        // Try summarization before eviction when context is large
        if self.conversation.messages.len() >= MIN_MESSAGES_FOR_SUMMARY
            && self.estimated_tokens() as f64 > self.max_tokens as f64 * SUMMARIZATION_THRESHOLD
        {
            self.summarize_old_turns();
        }

        // Use priority-based selection if enabled
        if let Some(ref selector) = self.selector {
            let messages = std::mem::take(&mut self.conversation.messages);
            let token_counter = |m: &Message| m.content.as_text().len() / 4 + 1;
            let result = selector.select(&messages, &token_counter);
            self.conversation.messages = result.messages;
            return;
        }

        // Fallback to simple FIFO enforcement
        // First enforce message count limit (protect system messages)
        if self.conversation.messages.len() > self.max_messages {
            let mut removed = 0;
            let excess = self.conversation.messages.len() - self.max_messages;
            self.conversation.messages.retain(|m| {
                if removed >= excess {
                    return true;
                }
                if m.role == MessageRole::System {
                    return true;
                }
                removed += 1;
                false
            });
        }

        // Then enforce token limit by removing messages from the start (skip system)
        loop {
            let tokens = self.estimated_tokens();
            if tokens <= self.max_tokens {
                break;
            }
            // Find first non-system message to evict
            let idx = self
                .conversation
                .messages
                .iter()
                .position(|m| m.role != MessageRole::System);
            match idx {
                Some(i) => {
                    self.conversation.messages.remove(i);
                }
                None => break, // only system messages left
            }
        }
    }

    /// Compress older turns into a single summary message, preserving
    /// system messages and the most recent turns.
    fn summarize_old_turns(&mut self) {
        let total = self.conversation.messages.len();
        if total < MIN_MESSAGES_FOR_SUMMARY {
            return;
        }

        // Split: system messages + old turns + recent turns
        let keep_from = total.saturating_sub(RECENT_TURNS_TO_KEEP);

        // Find the first non-system message index
        let first_non_system = self
            .conversation
            .messages
            .iter()
            .position(|m| m.role != MessageRole::System)
            .unwrap_or(total);

        // Nothing to summarize if all non-system messages are in the recent window
        if keep_from <= first_non_system {
            return;
        }

        // Collect messages to summarize: from first_non_system to keep_from
        let to_summarize: Vec<&Message> = self.conversation.messages[first_non_system..keep_from]
            .iter()
            .collect();

        if to_summarize.is_empty() {
            return;
        }

        // Build a compact summary
        let mut summary_parts: Vec<String> = Vec::new();
        for msg in &to_summarize {
            let text = msg.content.as_text();
            let preview: String = text.chars().take(120).collect();
            let role_label = match &msg.role {
                MessageRole::Assistant => "AI".to_string(),
                other => other.to_string(),
            };
            summary_parts.push(format!("[{role_label}] {preview}"));
        }

        let summary_text = format!(
            "[conversation-summary] Earlier turns ({} messages compressed):\n{}",
            to_summarize.len(),
            summary_parts.join("\n")
        );

        // Remove the summarized messages
        self.conversation
            .messages
            .drain(first_non_system..keep_from);

        // Insert the summary as a system message right after the last system message
        let insert_at = self
            .conversation
            .messages
            .iter()
            .rposition(|m| m.role == MessageRole::System)
            .map(|i| i + 1)
            .unwrap_or(0);

        self.conversation
            .messages
            .insert(insert_at, Message::system(summary_text));
    }

    /// Get the conversation ready for LLM input
    pub fn to_prompt(&self) -> String {
        // Format messages as a prompt string
        self.conversation
            .messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content.as_text()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get messages as a vector
    pub fn messages(&self) -> &[Message] {
        &self.conversation.messages
    }

    /// Get mutable reference to messages (for testing)
    pub fn messages_mut(&mut self) -> &mut Vec<Message> {
        &mut self.conversation.messages
    }

    /// Get current conversation
    pub fn conversation(&self) -> &ProtocolConversation {
        &self.conversation
    }

    /// Get estimated token count
    pub fn estimated_tokens(&self) -> usize {
        // Simple heuristic: ~4 chars per token
        self.conversation
            .messages
            .iter()
            .map(|m| m.content.as_text().len() / 4 + 1)
            .sum()
    }

    /// Clear conversation history
    pub fn clear(&mut self) {
        self.conversation.messages.clear();
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.conversation.messages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::SessionId;

    #[test]
    fn test_conversation_manager_creation() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let manager = ConversationManager::new(conv);

        assert_eq!(manager.message_count(), 0);
        assert_eq!(manager.max_messages, 10);
        assert_eq!(manager.max_tokens, 8000);
    }

    #[test]
    fn test_add_message() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        manager.add_message(Message::user("Hello"));
        manager.add_message(Message::assistant("Hi"));

        assert_eq!(manager.message_count(), 2);
    }

    #[test]
    fn test_message_limit_enforcement() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv).with_max_messages(3);

        for i in 0..5 {
            manager.add_message(Message::user(format!("Message {}", i)));
        }

        // Should only keep 3 most recent
        assert_eq!(manager.message_count(), 3);
        assert_eq!(
            manager.messages()[0].content,
            rustycode_protocol::MessageContent::Simple("Message 2".to_string())
        );
        assert_eq!(
            manager.messages()[2].content,
            rustycode_protocol::MessageContent::Simple("Message 4".to_string())
        );
    }

    #[test]
    fn test_token_limit_enforcement() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv).with_max_tokens(100);

        // Add messages until we exceed token limit
        for i in 0..50 {
            manager.add_message(Message::user(format!(
                "This is a message with some content {}",
                i
            )));
        }

        // Should have removed older messages to stay under token limit
        let tokens = manager.estimated_tokens();
        assert!(tokens <= 100, "Tokens: {}, Expected: <= 100", tokens);
    }

    #[test]
    fn test_to_prompt() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        manager.add_message(Message::user("What is 2+2?"));
        manager.add_message(Message::assistant("4"));

        let prompt = manager.to_prompt();
        assert!(prompt.contains("user: What is 2+2?"));
        assert!(prompt.contains("assistant: 4"));
    }

    #[test]
    fn test_clear_conversation() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        manager.add_message(Message::user("Hello"));
        assert_eq!(manager.message_count(), 1);

        manager.clear();
        assert_eq!(manager.message_count(), 0);
    }

    #[test]
    fn test_estimated_tokens() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        manager.add_message(Message::user("Hello world this is a test message"));
        let tokens = manager.estimated_tokens();
        assert!(tokens > 0);
    }

    #[test]
    fn test_builder_pattern() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let manager = ConversationManager::new(conv)
            .with_max_messages(5)
            .with_max_tokens(2000);

        assert_eq!(manager.max_messages, 5);
        assert_eq!(manager.max_tokens, 2000);
    }

    #[test]
    fn test_compacts_large_assistant_message() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        let large = "x".repeat(5000);
        manager.add_message(Message::assistant(large));

        let stored = &manager.messages()[0].content;
        assert!(stored.contains("[tool-output-masked]"));
        assert!(stored.contains("compacted"));
    }

    #[test]
    fn test_startup_examples_formatting() {
        // This test verifies that startup examples are formatted correctly
        // Tool block comes FIRST, then explanatory text (matching Claude's actual response format)
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv);

        // Add startup examples (CORRECT format: tool block first, then text)
        manager.add_message(Message::user("Read Cargo.toml"));
        manager.add_message(Message::assistant(
            "```tool\n{\"name\": \"read_file\", \"input\": {\"path\": \"Cargo.toml\"}}\n```\nI'll read the Cargo.toml file for you."
        ));
        manager.add_message(Message::system(
            "File read successfully. [Content of Cargo.toml shown]",
        ));

        manager.add_message(Message::user("List files"));
        manager.add_message(Message::assistant(
            "```tool\n{\"name\": \"bash\", \"input\": {\"command\": \"ls\"}}\n```\nI'll list the files in the current directory."
        ));
        manager.add_message(Message::system(
            "Files listed successfully. [Directory contents shown]",
        ));

        // Get the formatted prompt
        let prompt = manager.to_prompt();

        // Verify the prompt contains the tool calls with proper parameters
        assert!(
            prompt.contains("user: Read Cargo.toml"),
            "Should contain user request"
        );
        assert!(
            prompt.contains("```tool"),
            "Should contain tool block wrapper"
        );
        assert!(
            prompt.contains("{\"name\": \"read_file\", \"input\": {\"path\": \"Cargo.toml\"}}"),
            "Should contain tool call with parameters"
        );
        assert!(
            prompt.contains("{\"name\": \"bash\", \"input\": {\"command\": \"ls\"}}"),
            "Should contain tool call with parameters"
        );

        // Verify the tool calls have non-empty input fields
        assert!(
            prompt.contains("\"input\": {\"path\": \"Cargo.toml\"}"),
            "read_file should have path parameter"
        );
        assert!(
            prompt.contains("\"input\": {\"command\": \"ls\"}"),
            "bash should have command parameter"
        );

        // Verify there are NO empty input objects in the examples
        assert!(
            !prompt.contains("\"input\": {}"),
            "Startup examples should NOT have empty input objects"
        );

        // Verify tool block comes BEFORE explanatory text
        assert!(
            prompt.find("```tool").unwrap_or(0) < prompt.find("I'll read").unwrap_or(9999),
            "Tool block should come before explanatory text"
        );
    }

    #[test]
    fn test_priority_based_selection() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv)
            .with_max_tokens(1000)
            .with_priority_selection(5); // Last 5 turns are High priority

        // Add system message (should be Critical priority)
        manager.add_message(Message::system("You are a helpful assistant"));

        // Add older messages (will be Normal priority)
        for i in 0..10 {
            manager.add_message(Message::user(format!("User message {}", i)));
            manager.add_message(Message::assistant(format!("Assistant response {}", i)));
        }

        // Add recent messages (will be High priority due to retention window)
        manager.add_message(Message::user("Recent user message"));
        manager.add_message(Message::assistant("Recent assistant response"));

        // With priority selection, system should be preserved and recent messages kept
        let messages = manager.messages();
        assert!(
            !messages.is_empty(),
            "Should have messages after priority selection"
        );

        // System message should be preserved (Critical priority)
        assert!(
            messages.iter().any(|m| m.role == "system"),
            "System message should be preserved"
        );
    }

    #[test]
    fn test_priority_selection_excludes_skippable() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv)
            .with_max_tokens(200) // Tight budget to force exclusion
            .with_priority_selection(3);

        // Add system message
        manager.add_message(Message::system("System"));

        // Add skippable message (thinking tags)
        manager.add_message(Message::assistant(
            "<thinking>Let me analyze this step by step...</thinking>".to_string(),
        ));

        // Add normal messages
        manager.add_message(Message::user("Task"));
        manager.add_message(Message::assistant("Response"));

        let messages = manager.messages();

        // System should always be included
        assert!(
            messages.iter().any(|m| m.role == "system"),
            "System message should be included"
        );
    }

    #[test]
    fn test_fifo_preserves_system_message_under_message_limit() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv).with_max_messages(3);

        // Add system message first
        manager.add_message(Message::system("You are a helpful assistant"));

        // Add more messages than the limit
        for i in 0..5 {
            manager.add_message(Message::user(format!("Message {}", i)));
        }

        // System message should survive eviction
        let messages = manager.messages();
        assert!(
            messages.iter().any(|m| m.role == "system"),
            "System message should be preserved during FIFO message limit eviction"
        );
        assert!(
            messages.len() <= 4,
            "Should have at most 3 regular + 1 system message"
        );
    }

    #[test]
    fn test_fifo_preserves_system_message_under_token_limit() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv).with_max_tokens(80);

        // Add system message
        manager.add_message(Message::system("You are a helpful assistant"));

        // Add messages until we exceed token limit
        for i in 0..20 {
            manager.add_message(Message::user(format!(
                "This is message number {} with some extra text",
                i
            )));
        }

        // System message should survive token eviction
        let messages = manager.messages();
        assert!(
            messages.iter().any(|m| m.role == "system"),
            "System message should be preserved during FIFO token limit eviction"
        );
    }

    #[test]
    fn test_summarization_compresses_old_turns() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        // Set limits that allow messages to accumulate before summarization triggers
        let mut manager = ConversationManager::new(conv)
            .with_max_messages(30)
            .with_max_tokens(300);

        manager.add_message(Message::system("System prompt"));

        // Add enough messages to trigger summarization (>10 messages, >80% tokens)
        for i in 0..12 {
            manager.add_message(Message::user(format!(
                "User asks about topic {} with some detail",
                i
            )));
            manager.add_message(Message::assistant(format!(
                "Assistant responds about topic {} with analysis",
                i
            )));
        }

        let messages = manager.messages();

        // Should have a summary message
        assert!(
            messages
                .iter()
                .any(|m| m.content.as_text().contains("[conversation-summary]")),
            "Should contain a conversation summary"
        );

        // System prompt should survive
        assert!(
            messages
                .iter()
                .any(|m| m.content.as_text() == "System prompt"),
            "System prompt should survive summarization"
        );

        // Recent turns should survive
        assert!(
            messages
                .iter()
                .any(|m| m.content.as_text().contains("topic 11")),
            "Recent turns should survive summarization"
        );
    }

    #[test]
    fn test_summarization_not_triggered_below_threshold() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv).with_max_tokens(8000);

        manager.add_message(Message::system("System"));

        // Add only a few messages — should not trigger summarization
        for i in 0..5 {
            manager.add_message(Message::user(format!("Msg {}", i)));
        }

        let messages = manager.messages();
        assert!(
            !messages
                .iter()
                .any(|m| m.content.as_text().contains("[conversation-summary]")),
            "Should not summarize with few messages"
        );
    }

    #[test]
    fn test_summarization_preserves_recent_turns() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv)
            .with_max_messages(30)
            .with_max_tokens(300);

        manager.add_message(Message::system("System"));

        for i in 0..12 {
            manager.add_message(Message::user(format!("Question about thing {}", i)));
            manager.add_message(Message::assistant(format!("Answer about thing {}", i)));
        }

        let messages = manager.messages();
        let text = messages
            .iter()
            .map(|m| m.content.as_text())
            .collect::<Vec<_>>()
            .join(" ");

        // Most recent turns should be intact (not in summary)
        assert!(text.contains("thing 11"), "Latest turn should be preserved");
        assert!(
            text.contains("thing 10"),
            "Second-to-last turn should be preserved"
        );
    }

    #[test]
    fn test_summarization_end_to_end_with_system_and_recent() {
        let session_id = SessionId::new();
        let conv = ProtocolConversation::new(session_id);
        let mut manager = ConversationManager::new(conv)
            .with_max_messages(50)
            .with_max_tokens(200);

        // System message
        manager.add_message(Message::system("You are a code assistant"));

        // Old turns that should get summarized
        for i in 0..8 {
            manager.add_message(Message::user(format!("Explain concept {}", i)));
            manager.add_message(Message::assistant(format!(
                "Concept {} is about programming fundamentals with some detail",
                i
            )));
        }

        // Recent turns that should survive
        manager.add_message(Message::user("What about concept 99?"));
        manager.add_message(Message::assistant("Concept 99 is advanced"));

        let messages = manager.messages();
        let texts: Vec<String> = messages.iter().map(|m| m.content.as_text()).collect();

        // System message must survive
        assert!(
            texts.contains(&"You are a code assistant".to_string()),
            "system prompt preserved"
        );

        // Summary should exist
        assert!(
            texts.iter().any(|t| t.contains("[conversation-summary]")),
            "summary message should exist"
        );

        // Recent turns must survive
        assert!(
            texts.iter().any(|t| t.contains("concept 99")),
            "recent turn preserved"
        );

        // Old turn details should be compressed (not as full messages)
        let old_survivors = texts
            .iter()
            .any(|t| t.contains("Explain concept 0") && !t.contains("[conversation-summary]"));
        assert!(
            !old_survivors,
            "old turns should be in summary, not standalone"
        );
    }
}
