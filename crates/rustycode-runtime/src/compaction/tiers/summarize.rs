//! SummarizeTier -- LLM-backed compaction pass that generates structured summaries.
//!
//! This tier replaces older conversation turns with an LLM-generated summary,
//! preserving a configurable number of recent "tail" turns verbatim. The
//! summary template degrades from Full (9 sections) to Compact (5) to Minimal (2)
//! when tighter compaction is needed.

use rustycode_llm::provider::{ChatMessage, CompletionRequest, LLMProvider};
use rustycode_protocol::compaction::SummaryTemplate;
use rustycode_protocol::Message;

use super::TierResult;

// ---------------------------------------------------------------------------
// Prompt constants
// ---------------------------------------------------------------------------

/// Full summary prompt -- 9 sections.
const FULL_PROMPT: &str = "\
You are a conversation summarizer. Summarize the conversation below into a \
structured summary with these 9 sections. Be concise but thorough.

## Primary Request
What the user originally asked for.

## Key Concepts
Important concepts, terminology, and domain knowledge mentioned.

## Files/Code
Files read, written, or modified. Include paths and key changes.

## Errors/Fixes
Errors encountered and how they were resolved.

## Problem Solving
Approach taken, alternatives considered, and reasoning.

## User Messages
Key user messages or instructions that changed direction.

## Pending Tasks
Tasks mentioned but not yet completed.

## Current Work
What was being worked on most recently.

## Next Step
The logical next step based on where the conversation left off.

---

CONVERSATION:

";

/// Compact summary prompt -- 5 sections.
const COMPACT_PROMPT: &str = "\
You are a conversation summarizer. Summarize the conversation below into a \
compact summary with these 5 sections.

## Goal
What the user is trying to accomplish.

## Progress
What has been done so far.

## Decisions Made
Key decisions and their rationale.

## Active Files
Files currently being worked on.

## Next Step
The logical next step.

---

CONVERSATION:

";

/// Minimal summary prompt -- 2 sections.
const MINIMAL_PROMPT: &str = "\
You are a conversation summarizer. Summarize the conversation below into a \
minimal summary with these 2 sections.

## What the user wants
The user's goal.

## Where we are right now
Current state and what was last done.

---

CONVERSATION:

";

// ---------------------------------------------------------------------------
// SummarizeTier
// ---------------------------------------------------------------------------

/// LLM-backed compaction tier: generates a structured summary of older turns.
///
/// Splits the conversation at a turn boundary, asks the LLM to summarize the
/// head, and concatenates the summary with the preserved tail.
#[derive(Debug, Clone)]
pub struct SummarizeTier {
    template: SummaryTemplate,
    tail_turns: usize,
}

impl SummarizeTier {
    /// Create a new SummarizeTier with the given template granularity and number
    /// of tail turns to preserve verbatim.
    pub fn new(template: SummaryTemplate, tail_turns: usize) -> Self {
        Self {
            template,
            tail_turns,
        }
    }

    /// Build the LLM prompt for the given template level from the messages that
    /// need summarizing.
    pub fn build_prompt(&self, messages: &[Message]) -> String {
        let prompt_prefix = match self.template {
            SummaryTemplate::Full => FULL_PROMPT,
            SummaryTemplate::Compact => COMPACT_PROMPT,
            SummaryTemplate::Minimal => MINIMAL_PROMPT,
        };

        let conversation_text = messages
            .iter()
            .map(|msg| {
                let role = &msg.role;
                let text = msg.content.as_text();
                format!("[{role}]: {text}")
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        format!("{prompt_prefix}{conversation_text}")
    }

    /// Find the split index in `messages`.
    ///
    /// Messages at indices before the returned value will be summarized; messages
    /// from the returned index onward are preserved as tail turns.
    ///
    /// The split is placed at the Nth-to-last User-role message boundary, where
    /// N = `tail_turns`. If there are fewer user messages than `tail_turns`,
    /// returns 0 (summarize nothing, preserve all).
    pub fn find_summary_split(&self, messages: &[Message]) -> usize {
        if self.tail_turns == 0 {
            // Preserve nothing as tail -- summarize everything.
            return messages.len();
        }

        // Collect indices of User-role messages (turn boundaries).
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, m)| m.is_user())
            .map(|(i, _)| i)
            .collect();

        let num_turns = user_indices.len();
        if num_turns <= self.tail_turns {
            // Fewer turns than tail_turns -- preserve everything.
            return 0;
        }

        // Split at the (num_turns - tail_turns)-th user message so that the
        // last `tail_turns` user messages (and everything after each) are kept.
        user_indices[num_turns - self.tail_turns]
    }

    /// Run the summarize pass over `messages` using the given LLM provider.
    ///
    /// 1. Finds the split point.
    /// 2. Builds a prompt from the head portion.
    /// 3. Calls the LLM for a summary.
    /// 4. Returns `[summary as System] + preserved tail`.
    pub async fn compact(
        &self,
        messages: Vec<Message>,
        llm: &dyn LLMProvider,
    ) -> anyhow::Result<TierResult> {
        let original_chars: usize = messages.iter().map(|m| m.content.len()).sum();

        let split = self.find_summary_split(&messages);

        // If nothing to summarize, return unchanged.
        if split == 0 {
            return Ok(TierResult {
                messages,
                tokens_removed: 0,
            });
        }

        let (to_summarize, to_preserve) = messages.split_at(split);

        // If there is nothing to summarize (all preserved), return unchanged.
        if to_summarize.is_empty() {
            return Ok(TierResult {
                messages,
                tokens_removed: 0,
            });
        }

        let prompt = self.build_prompt(to_summarize);

        let request = CompletionRequest::new("summarize", vec![ChatMessage::user(prompt)])
            .with_max_tokens(3000);

        let response = llm.complete(request).await?;

        let summary_message = Message::system(response.content);
        let preserved: Vec<Message> = to_preserve.to_vec();

        let mut result_messages = vec![summary_message];
        result_messages.extend(preserved);

        let result_chars: usize = result_messages.iter().map(|m| m.content.len()).sum();
        let chars_removed = original_chars.saturating_sub(result_chars);

        Ok(TierResult {
            messages: result_messages,
            tokens_removed: chars_removed / 4,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::Message;

    /// Helper: create a user message.
    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    /// Helper: create an assistant message.
    fn assistant_msg(text: &str) -> Message {
        Message::assistant(text)
    }

    #[test]
    fn full_prompt_contains_sections() {
        let tier = SummarizeTier::new(SummaryTemplate::Full, 2);
        let prompt = tier.build_prompt(&[user_msg("hello")]);

        let sections = [
            "Primary Request",
            "Key Concepts",
            "Files/Code",
            "Errors/Fixes",
            "Problem Solving",
            "User Messages",
            "Pending Tasks",
            "Current Work",
            "Next Step",
        ];
        for section in &sections {
            assert!(
                prompt.contains(section),
                "FULL_PROMPT should contain section '{section}'"
            );
        }
    }

    #[test]
    fn compact_prompt_contains_sections() {
        let tier = SummarizeTier::new(SummaryTemplate::Compact, 2);
        let prompt = tier.build_prompt(&[user_msg("hello")]);

        let sections = [
            "Goal",
            "Progress",
            "Decisions Made",
            "Active Files",
            "Next Step",
        ];
        for section in &sections {
            assert!(
                prompt.contains(section),
                "COMPACT_PROMPT should contain section '{section}'"
            );
        }
    }

    #[test]
    fn minimal_prompt_contains_sections() {
        let tier = SummarizeTier::new(SummaryTemplate::Minimal, 2);
        let prompt = tier.build_prompt(&[user_msg("hello")]);

        assert!(
            prompt.contains("What the user wants"),
            "MINIMAL_PROMPT should contain 'What the user wants'"
        );
        assert!(
            prompt.contains("Where we are right now"),
            "MINIMAL_PROMPT should contain 'Where we are right now'"
        );
    }

    #[test]
    fn split_with_enough_turns() {
        // 4 user messages at indices 0, 2, 4, 6 with interleaved assistants.
        // tail_turns=2 means preserve the last 2 user-message turns.
        // The 3rd user message (0-indexed: 2nd) is at index 4.
        let messages = vec![
            user_msg("a"),      // 0 — turn 0
            assistant_msg("A"), // 1
            user_msg("b"),      // 2 — turn 1
            assistant_msg("B"), // 3
            user_msg("c"),      // 4 — turn 2
            assistant_msg("C"), // 5
            user_msg("d"),      // 6 — turn 3
            assistant_msg("D"), // 7
        ];

        let tier = SummarizeTier::new(SummaryTemplate::Full, 2);
        let split = tier.find_summary_split(&messages);

        // 4 user messages, tail_turns=2 => keep last 2 turns.
        // Split at user_indices[4 - 2] = user_indices[2] = index 4.
        assert_eq!(
            split, 4,
            "split should be at the 3rd user message (index 4), preserving turns 2 and 3"
        );

        // Verify: messages[0..4] are summarized, messages[4..] are preserved.
        assert_eq!(messages[split].content.as_text(), "c");
    }

    #[test]
    fn split_with_fewer_turns_than_tail() {
        // Only 1 user message, but tail_turns=2.
        let messages = vec![user_msg("only one"), assistant_msg("response")];

        let tier = SummarizeTier::new(SummaryTemplate::Full, 2);
        let split = tier.find_summary_split(&messages);

        assert_eq!(
            split, 0,
            "split should be 0 when fewer turns than tail_turns (preserve all)"
        );
    }
}
