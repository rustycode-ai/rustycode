//! TruncateTier -- free compaction pass that keeps only the last N complete turns.
//!
//! A "turn" is one complete user-to-assistant round trip, including any
//! tool_use / tool_result pairs within it. Each turn starts at a User-role
//! message and extends up to (but not including) the next User-role message.

use rustycode_protocol::Message;

use super::TierResult;

/// Free compaction pass: hard cut to the tail N turns.
#[derive(Debug, Clone)]
pub struct TruncateTier {
    /// Number of complete turns to retain from the tail of the conversation.
    tail_turns: usize,
}

impl TruncateTier {
    pub fn new(tail_turns: usize) -> Self {
        Self { tail_turns }
    }

    /// Run the truncate pass over `messages`, returning the retained messages
    /// and an estimated count of tokens removed.
    pub fn compact(&self, messages: Vec<Message>) -> TierResult {
        // tail_turns == 0 means drop everything.
        if self.tail_turns == 0 {
            let tokens_removed: usize = messages
                .iter()
                .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
                .sum();
            return TierResult {
                messages: Vec::new(),
                tokens_removed,
            };
        }

        let total_len = messages.len();
        if total_len == 0 {
            return TierResult {
                messages,
                tokens_removed: 0,
            };
        }

        // Find indices of every User-role message (these are turn boundaries).
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_user())
            .map(|(i, _)| i)
            .collect();

        if user_indices.is_empty() {
            // No user messages at all -- keep the last N messages as a flat tail.
            return self.keep_flat_tail(messages);
        }

        // If there are fewer turns than tail_turns, keep everything.
        let num_turns = user_indices.len();
        if num_turns <= self.tail_turns {
            return TierResult {
                messages,
                tokens_removed: 0,
            };
        }

        // Keep from the (num_turns - tail_turns)-th user message onward.
        let keep_from = user_indices[num_turns - self.tail_turns];
        self.split_at(messages, keep_from)
    }

    /// When no user messages exist, keep the last `tail_turns` messages as a
    /// flat tail.
    fn keep_flat_tail(&self, messages: Vec<Message>) -> TierResult {
        let total = messages.len();
        if total <= self.tail_turns {
            return TierResult {
                messages,
                tokens_removed: 0,
            };
        }
        let keep_from = total - self.tail_turns;
        self.split_at(messages, keep_from)
    }

    /// Split `messages` at `keep_from`, returning the tail and estimating
    /// tokens removed from the discarded head.
    fn split_at(&self, messages: Vec<Message>, keep_from: usize) -> TierResult {
        let tokens_removed: usize = messages[..keep_from]
            .iter()
            .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
            .sum();
        let retained = messages[keep_from..].to_vec();
        TierResult {
            messages: retained,
            tokens_removed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{ContentBlock, MessageContent, MessageRole};

    /// Helper: user message with simple text.
    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    /// Helper: assistant message with simple text.
    fn assistant_msg(text: &str) -> Message {
        Message::assistant(text)
    }

    /// Helper: user message carrying a ToolResult block (simulates tool result
    /// returned to the conversation).
    fn tool_result_msg(tool_use_id: &str, content: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: false,
            }]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }
    }

    /// Helper: assistant message carrying a ToolUse block.
    fn tool_use_msg(id: &str, name: &str) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::Value::Object(serde_json::Map::new()),
            }]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }
    }

    // -- Test 1: zero_returns_empty --------------------------------------------------

    #[test]
    fn zero_returns_empty() {
        let tier = TruncateTier::new(0);
        let msgs = vec![
            user_msg("hello"),
            assistant_msg("hi"),
            user_msg("how are you"),
            assistant_msg("fine"),
        ];
        let result = tier.compact(msgs);
        assert!(
            result.messages.is_empty(),
            "tail_turns=0 should return empty"
        );
        assert!(
            result.tokens_removed > 0,
            "should report tokens removed when messages are discarded"
        );
    }

    // -- Test 2: keeps_last_two_turns -------------------------------------------------

    #[test]
    fn keeps_last_two_turns() {
        let tier = TruncateTier::new(2);
        let long = "x".repeat(40);
        // Three turns:
        //   turn 0: user(long) + assistant(long)
        //   turn 1: user(long) + assistant(long)
        //   turn 2: user("c") + assistant("C")
        let msgs = vec![
            user_msg(&long),
            assistant_msg(&long),
            user_msg(&long),
            assistant_msg(&long),
            user_msg("c"),
            assistant_msg("C"),
        ];
        let result = tier.compact(msgs);

        // Should keep turns 1 and 2 (from the 2nd-to-last user msg onward).
        assert_eq!(result.messages.len(), 4);
        assert_eq!(result.messages[0].content.as_text(), long);
        assert_eq!(result.messages[1].content.as_text(), long);
        assert_eq!(result.messages[2].content.as_text(), "c");
        assert_eq!(result.messages[3].content.as_text(), "C");
        assert!(
            result.tokens_removed > 0,
            "should report tokens removed when turns are discarded"
        );
    }

    // -- Test 3: keeps_turn_with_tool_results -----------------------------------------

    #[test]
    fn keeps_turn_with_tool_results() {
        let tier = TruncateTier::new(1);
        // Turn 0: user("read file") + assistant(tool_use) + user(tool_result) + assistant("done")
        // Turn 1: user("summarize") + assistant("summary text")
        let msgs = vec![
            user_msg("read file"),
            tool_use_msg("t1", "Read"),
            tool_result_msg("t1", "file contents here"),
            assistant_msg("done reading"),
            user_msg("summarize"),
            assistant_msg("summary text"),
        ];

        let result = tier.compact(msgs);

        // Should keep only turn 1 (the last complete turn).
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "summarize");
        assert_eq!(result.messages[1].content.as_text(), "summary text");
        assert!(result.tokens_removed > 0);
    }

    // -- Test 4: fewer_messages_than_tail_keeps_all -----------------------------------

    #[test]
    fn fewer_messages_than_tail_keeps_all() {
        let tier = TruncateTier::new(5);
        let msgs = vec![user_msg("hello"), assistant_msg("hi")];
        let result = tier.compact(msgs);

        assert_eq!(
            result.messages.len(),
            2,
            "should keep all when fewer turns than tail_turns"
        );
        assert_eq!(
            result.tokens_removed, 0,
            "no tokens removed when nothing is discarded"
        );
    }

    // -- Test 5: no_user_messages_keeps_tail ------------------------------------------

    #[test]
    fn no_user_messages_keeps_tail() {
        let tier = TruncateTier::new(2);
        // Only assistant/system messages, no user messages at all.
        let msgs = vec![
            assistant_msg("first"),
            assistant_msg("second"),
            assistant_msg("third"),
            assistant_msg("fourth"),
        ];
        let result = tier.compact(msgs);

        // Should keep last 2 messages (flat tail).
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "third");
        assert_eq!(result.messages[1].content.as_text(), "fourth");
        assert!(result.tokens_removed > 0);
    }

    // -- Test 6: tokens_removed_positive ----------------------------------------------

    #[test]
    fn tokens_removed_positive() {
        let tier = TruncateTier::new(1);
        // Each word counts as a token; use multi-word messages to measure removal.
        let long_text = (0..50)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let msgs = vec![
            user_msg("short question"),
            assistant_msg(&long_text),
            user_msg(&long_text),
            assistant_msg(&long_text),
            user_msg("keep me"),
            assistant_msg("response"),
        ];
        let result = tier.compact(msgs);

        assert_eq!(result.messages.len(), 2);
        assert!(
            result.tokens_removed > 0,
            "tokens_removed should be positive when messages are discarded"
        );
        assert!(
            result.tokens_removed >= 100,
            "expected substantial removal, got {}",
            result.tokens_removed
        );
    }

    // -- Extended edge-case tests -------------------------------------------

    #[test]
    fn single_turn_kept_when_tail_is_one() {
        let tier = TruncateTier::new(1);
        let msgs = vec![user_msg("hello"), assistant_msg("world")];
        let result = tier.compact(msgs);

        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "hello");
        assert_eq!(result.messages[1].content.as_text(), "world");
        assert_eq!(result.tokens_removed, 0, "nothing removed when only 1 turn");
    }

    #[test]
    fn user_without_assistant_at_end() {
        let tier = TruncateTier::new(1);
        // Last turn has user but no assistant response.
        let msgs = vec![
            user_msg("question 1"),
            assistant_msg("answer 1"),
            user_msg("question 2"),
        ];
        let result = tier.compact(msgs);

        // Should keep from the last user message onward (just "question 2").
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].content.as_text(), "question 2");
    }

    #[test]
    fn all_assistant_messages_uses_flat_tail() {
        let tier = TruncateTier::new(2);
        let msgs = vec![
            assistant_msg("first"),
            assistant_msg("second"),
            assistant_msg("third"),
            assistant_msg("fourth"),
            assistant_msg("fifth"),
        ];
        let result = tier.compact(msgs);

        // No user messages → flat tail of last 2.
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "fourth");
        assert_eq!(result.messages[1].content.as_text(), "fifth");
    }

    #[test]
    fn tool_results_kept_with_their_turn() {
        let tier = TruncateTier::new(1);
        // Turn: user("read") + assistant(tool_use) + user(tool_result) + assistant("done")
        let msgs = vec![
            user_msg("read"),
            tool_use_msg("t1", "Read"),
            tool_result_msg("t1", "file contents"),
            assistant_msg("done reading"),
            user_msg("summarize"),
            assistant_msg("summary"),
        ];
        let result = tier.compact(msgs);

        // Should keep only the last turn: user("summarize") + assistant("summary")
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].content.as_text(), "summarize");
        assert_eq!(result.messages[1].content.as_text(), "summary");
        assert!(
            result.tokens_removed > 0,
            "earlier turn with tool results should be removed"
        );
    }

    #[test]
    fn large_turn_count_keeps_only_tail() {
        let tier = TruncateTier::new(3);
        let mut msgs = Vec::new();
        // 10 turns, each user + assistant.
        for i in 0..10 {
            msgs.push(user_msg(&format!("q{i}")));
            msgs.push(assistant_msg(&format!("a{i}")));
        }
        let result = tier.compact(msgs);

        // Should keep last 3 turns = 6 messages.
        assert_eq!(result.messages.len(), 6);
        assert_eq!(result.messages[0].content.as_text(), "q7");
        assert_eq!(result.messages[1].content.as_text(), "a7");
        assert_eq!(result.messages[4].content.as_text(), "q9");
        assert_eq!(result.messages[5].content.as_text(), "a9");
    }

    #[test]
    fn tail_equals_total_turns_keeps_all() {
        let tier = TruncateTier::new(3);
        let msgs = vec![
            user_msg("q1"),
            assistant_msg("a1"),
            user_msg("q2"),
            assistant_msg("a2"),
            user_msg("q3"),
            assistant_msg("a3"),
        ];
        let result = tier.compact(msgs);

        assert_eq!(
            result.messages.len(),
            6,
            "should keep all when tail equals total turns"
        );
        assert_eq!(result.tokens_removed, 0);
    }

    #[test]
    fn empty_messages_returns_empty() {
        let tier = TruncateTier::new(5);
        let msgs: Vec<Message> = Vec::new();
        let result = tier.compact(msgs);
        assert!(result.messages.is_empty());
        assert_eq!(result.tokens_removed, 0);
    }

    #[test]
    fn single_message_no_user_keeps_one() {
        // Flat tail with 1 message requested.
        let tier = TruncateTier::new(1);
        let msgs = vec![assistant_msg("only one")];
        let result = tier.compact(msgs);
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].content.as_text(), "only one");
    }

    #[test]
    fn preserves_important_user_instructions_in_tail() {
        let tier = TruncateTier::new(2);
        let msgs = vec![
            user_msg("implement the auth module"),
            assistant_msg("I'll create the auth module with JWT tokens"),
            user_msg("also add rate limiting"),
            assistant_msg("adding rate limiting middleware"),
            user_msg("now write tests for the auth module"),
            assistant_msg("writing comprehensive auth tests"),
            user_msg("IMPORTANT: the token must never expire"),
            assistant_msg("setting token to never expire"),
        ];
        let result = tier.compact(msgs);

        // Last 2 turns should contain the IMPORTANT instruction.
        assert_eq!(result.messages.len(), 4);
        let texts: Vec<String> = result
            .messages
            .iter()
            .map(|m| m.content.as_text())
            .collect();
        let combined = texts.join(" ");
        assert!(
            combined.contains("IMPORTANT"),
            "tail should preserve the latest user instruction with IMPORTANT keyword"
        );
        // Earlier instructions should be gone.
        assert!(
            !combined.contains("implement the auth module"),
            "head should be removed"
        );
    }
}
