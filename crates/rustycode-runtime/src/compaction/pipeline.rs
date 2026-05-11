//! CompactPipeline -- main orchestration layer that ties all three compaction
//! tiers together with iterative tightening.
//!
//! The pipeline runs an iterative loop:
//!
//! 1. **Snip** (free) -- always applied first to trim tool output and strip
//!    thinking blocks.
//! 2. **Summarize** (LLM-backed) -- replaces older turns with a structured
//!    summary on every pass except the last.
//! 3. **Truncate** (destructive) -- hard cut to the tail on the final pass
//!    when summarization is not attempted or has already been exhausted.
//!
//! If the token estimate still exceeds the budget after all tightening passes,
//! an **emergency trim** keeps only the last user turn (or last two messages
//! when no user message exists).

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use rustycode_llm::provider::LLMProvider;
use rustycode_protocol::compaction::{
    CompactionError, CompactionResult, CompactionTierUsed, HybridCompactionConfig,
};
use rustycode_protocol::Message;

use super::budget::TokenBudget;
use super::context_block::SessionContextBlock;
use super::plan::CompactionPlan;
use super::tiers::{SnipTier, SummarizeTier, TruncateTier};

// CompactPipeline

/// Main orchestration layer for context compaction.
///
/// Runs the three compaction tiers in an iterative tightening loop, guarded
/// by an [`AtomicBool`] to prevent concurrent compaction.
pub struct CompactPipeline {
    config: HybridCompactionConfig,
    /// Lock-free guard: `true` while a compaction pass is running.
    compacting: AtomicBool,
}

impl CompactPipeline {
    pub fn new(config: HybridCompactionConfig) -> Self {
        Self {
            config,
            compacting: AtomicBool::new(false),
        }
    }

    /// Run the compaction pipeline.
    ///
    /// If another compaction is already in progress, returns
    /// [`CompactionError::AlreadyCompacting`] immediately.
    ///
    /// Otherwise, iterates through the tightening loop, applying Snip,
    /// Summarize (or Truncate on the last pass), and tightening the plan
    /// between passes until the token budget is satisfied or all passes are
    /// exhausted.
    pub async fn compact(
        &self,
        messages: Vec<Message>,
        budget: &TokenBudget,
        context_block: &mut SessionContextBlock,
        llm: &dyn LLMProvider,
    ) -> Result<CompactionResult> {
        // Concurrent guard -- compare_exchange to avoid races.
        self.compacting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow::anyhow!("{}", CompactionError::AlreadyCompacting))?;

        // Ensure the guard is cleared on all exit paths.
        let result = self
            .run_pipeline(messages, budget, context_block, llm)
            .await;
        self.compacting.store(false, Ordering::SeqCst);
        result
    }

    /// Whether a compaction pass is currently in progress.
    pub fn is_compacting(&self) -> bool {
        self.compacting.load(Ordering::SeqCst)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Core pipeline logic, extracted so the guard can be cleared in a single
    /// place regardless of success or failure.
    async fn run_pipeline(
        &self,
        messages: Vec<Message>,
        budget: &TokenBudget,
        context_block: &mut SessionContextBlock,
        llm: &dyn LLMProvider,
    ) -> Result<CompactionResult> {
        let tokens_before = estimate_tokens(&messages);
        let target = budget.target_size();

        let mut plan = CompactionPlan::from_config(&self.config);
        let mut current_messages = messages;
        let mut total_tokens_removed: usize = 0;
        let mut tiers_used: Vec<CompactionTierUsed> = Vec::new();
        let mut preserved_tail_turns: usize = 0;

        let max_passes = plan.max_passes;

        for pass in 0..max_passes {
            // -- Snip is always free -- apply before each pass ----------------
            let snip_tier = SnipTier::new(plan.max_tool_output_lines);
            let snipped = snip_tier.compact(current_messages);
            current_messages = snipped.messages;
            total_tokens_removed = total_tokens_removed.saturating_add(snipped.tokens_removed);

            // Measure after snip.
            let estimated = estimate_tokens(&current_messages);
            if estimated <= target {
                tiers_used.push(CompactionTierUsed::Snip {
                    tool_results_trimmed: 0, // SnipTier does not report per-block counts
                });
                return self.build_result(
                    current_messages,
                    context_block,
                    tokens_before,
                    total_tokens_removed,
                    tiers_used,
                    preserved_tail_turns,
                );
            }

            // -- Decide: Summarize (non-final) or Truncate (final pass) -------
            if pass < max_passes - 1 {
                // Try Summarize.
                let summarize_tier = SummarizeTier::new(plan.summary_template, plan.tail_turns);
                preserved_tail_turns = plan.tail_turns;

                // Clone before passing ownership to SummarizeTier::compact so we
                // can recover on failure. Compaction runs at most once per turn so
                // the clone cost is acceptable.
                let messages_for_summarize = current_messages.clone();
                let summarize_result = summarize_tier.compact(messages_for_summarize, llm).await;

                match summarize_result {
                    Ok(result) => {
                        tiers_used.push(CompactionTierUsed::Summarize {
                            template: plan.summary_template,
                            tail_preserved: plan.tail_turns,
                        });
                        current_messages = result.messages;
                        total_tokens_removed =
                            total_tokens_removed.saturating_add(result.tokens_removed);

                        let estimated = estimate_tokens(&current_messages);
                        if estimated <= target {
                            return self.build_result(
                                current_messages,
                                context_block,
                                tokens_before,
                                total_tokens_removed,
                                tiers_used,
                                preserved_tail_turns,
                            );
                        }
                    }
                    Err(_) => {
                        // Summarize failed -- keep current_messages unchanged
                        // and fall through to tighten for the next pass.
                    }
                }
            } else {
                // Last pass: use Truncate instead of Summarize.
                let truncate_tier = TruncateTier::new(plan.tail_turns);
                preserved_tail_turns = plan.tail_turns;
                let result = truncate_tier.compact(current_messages);
                let turns_kept = result.messages.len();
                tiers_used.push(CompactionTierUsed::Truncate { turns_kept });
                current_messages = result.messages;
                total_tokens_removed = total_tokens_removed.saturating_add(result.tokens_removed);

                let estimated = estimate_tokens(&current_messages);
                if estimated <= target {
                    return self.build_result(
                        current_messages,
                        context_block,
                        tokens_before,
                        total_tokens_removed,
                        tiers_used,
                        preserved_tail_turns,
                    );
                }
            }

            // Still over budget -- tighten for next pass.
            plan.tighten();
        }

        // All passes exhausted -- emergency trim.
        let trimmed = emergency_trim(current_messages);
        tiers_used.push(CompactionTierUsed::Emergency);
        self.build_result(
            trimmed,
            context_block,
            tokens_before,
            total_tokens_removed,
            tiers_used,
            0,
        )
    }

    /// Assemble the final [`CompactionResult`].
    fn build_result(
        &self,
        messages: Vec<Message>,
        context_block: &mut SessionContextBlock,
        tokens_before: usize,
        _total_tokens_removed: usize,
        tiers_used: Vec<CompactionTierUsed>,
        preserved_tail_turns: usize,
    ) -> Result<CompactionResult> {
        let tokens_after = estimate_tokens(&messages);

        // Split summary messages (system/summary) from preserved turns.
        let (summary_messages, preserved_turns) =
            split_summary_and_tail(messages, preserved_tail_turns);

        let context_block_render = context_block.render();

        Ok(CompactionResult {
            summary_messages,
            preserved_turns,
            context_block_render,
            tokens_before,
            tokens_after,
            tiers_used,
        })
    }
}

// Free helper functions

/// Rough token estimate using the canonical word-based heuristic.
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
        .sum()
}

/// Emergency trim: keep the last User message and the Assistant message that
/// follows it. If no User message is found, keep the last two messages.
pub fn emergency_trim(messages: Vec<Message>) -> Vec<Message> {
    if messages.is_empty() {
        return Vec::new();
    }

    // Walk backwards to find the last User message.
    let last_user_idx = messages.iter().rposition(|m| m.is_user());

    if let Some(idx) = last_user_idx {
        let mut kept = Vec::with_capacity(2);
        kept.push(messages[idx].clone());
        // Include the assistant message immediately after, if it exists.
        if let Some(next) = messages.get(idx + 1) {
            if next.is_assistant() {
                kept.push(next.clone());
            }
        }
        kept
    } else {
        // No user message -- keep last two.
        let start = messages.len().saturating_sub(2);
        messages[start..].to_vec()
    }
}

/// Split messages into summary (system/compaction-generated) and the
/// preserved tail turns.
///
/// The split point is determined by finding the last run of user/assistant
/// messages of length `tail_turns * 2` (each turn is user + assistant).
/// Everything before that is a summary message.
fn split_summary_and_tail(
    messages: Vec<Message>,
    preserved_tail_turns: usize,
) -> (Vec<Message>, Vec<Message>) {
    if preserved_tail_turns == 0 || messages.is_empty() {
        // All messages are the "summary" (possibly including emergency trim).
        return (messages, Vec::new());
    }

    // Find user-message boundaries from the tail.
    let user_positions: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.is_user())
        .map(|(i, _)| i)
        .collect();

    let num_user_msgs = user_positions.len();
    if num_user_msgs <= preserved_tail_turns {
        // Everything is tail -- no summary.
        return (Vec::new(), messages);
    }

    // Split at the user message that starts the tail.
    let split_idx = user_positions[num_user_msgs - preserved_tail_turns];
    let (summary, tail) = messages.split_at(split_idx);
    (summary.to_vec(), tail.to_vec())
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::Message;

    /// Helper: user message with simple text.
    fn user_msg(text: &str) -> Message {
        Message::user(text)
    }

    /// Helper: assistant message with simple text.
    fn assistant_msg(text: &str) -> Message {
        Message::assistant(text)
    }

    // -- Test 1: emergency_trim_keeps_last_user_turn ---------------------------

    #[test]
    fn emergency_trim_keeps_last_user_turn() {
        let messages = vec![
            user_msg("question 1"),
            assistant_msg("answer 1"),
            user_msg("question 2"),
            assistant_msg("answer 2"),
            user_msg("question 3"),
            assistant_msg("answer 3"),
        ];

        let trimmed = emergency_trim(messages);

        assert_eq!(trimmed.len(), 2, "should keep last user + next assistant");
        assert_eq!(trimmed[0].content.as_text(), "question 3");
        assert_eq!(trimmed[1].content.as_text(), "answer 3");
    }

    // -- Test 2: emergency_trim_no_user_keeps_last_two -------------------------

    #[test]
    fn emergency_trim_no_user_keeps_last_two() {
        let messages = vec![
            assistant_msg("first"),
            assistant_msg("second"),
            assistant_msg("third"),
            assistant_msg("fourth"),
        ];

        let trimmed = emergency_trim(messages);

        assert_eq!(trimmed.len(), 2, "should keep last 2 messages");
        assert_eq!(trimmed[0].content.as_text(), "third");
        assert_eq!(trimmed[1].content.as_text(), "fourth");
    }

    // -- Test 3: estimate_tokens_approximate ----------------------------------

    #[test]
    fn estimate_tokens_approximate() {
        // Each message has 10 words = 10 tokens, 4 messages = 40 tokens.
        let text_10_words = "one two three four five six seven eight nine ten";
        let messages = vec![
            user_msg(text_10_words),
            assistant_msg(text_10_words),
            user_msg(text_10_words),
            assistant_msg(text_10_words),
        ];

        let tokens = estimate_tokens(&messages);

        assert_eq!(
            tokens, 40,
            "4 messages of 10 words each should estimate 40 tokens"
        );
    }

    // -- Test 4: concurrent_guard_blocks --------------------------------------

    #[test]
    fn concurrent_guard_blocks() {
        let config = HybridCompactionConfig::default();
        let pipeline = CompactPipeline::new(config);

        // Manually set the guard.
        pipeline.compacting.store(true, Ordering::SeqCst);

        // Verify the guard check logic directly.
        assert!(
            pipeline.is_compacting(),
            "pipeline should report compacting=true"
        );

        // The actual blocking behavior is verified by checking that
        // compare_exchange fails when compacting is true.
        let guard_result =
            pipeline
                .compacting
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        assert!(
            guard_result.is_err(),
            "compare_exchange should fail when already compacting"
        );

        // Verify the error message matches CompactionError::AlreadyCompacting.
        let err = CompactionError::AlreadyCompacting;
        assert!(
            err.to_string().contains("compaction already in progress"),
            "error should mention already compacting"
        );

        // Clean up.
        pipeline.compacting.store(false, Ordering::SeqCst);
    }

    // -- Additional edge-case tests -------------------------------------------

    #[test]
    fn emergency_trim_single_user_no_following_assistant() {
        let messages = vec![user_msg("lonely")];
        let trimmed = emergency_trim(messages);
        assert_eq!(trimmed.len(), 1);
        assert_eq!(trimmed[0].content.as_text(), "lonely");
    }

    #[test]
    fn emergency_trim_empty_returns_empty() {
        let messages: Vec<Message> = Vec::new();
        let trimmed = emergency_trim(messages);
        assert!(trimmed.is_empty());
    }

    #[test]
    fn emergency_trim_single_message_no_user() {
        let messages = vec![assistant_msg("only one")];
        let trimmed = emergency_trim(messages);
        assert_eq!(trimmed.len(), 1);
        assert_eq!(trimmed[0].content.as_text(), "only one");
    }

    #[test]
    fn estimate_tokens_empty() {
        let messages: Vec<Message> = Vec::new();
        assert_eq!(estimate_tokens(&messages), 0);
    }

    #[test]
    fn pipeline_new_initializes_guard_to_false() {
        let config = HybridCompactionConfig::default();
        let pipeline = CompactPipeline::new(config);
        assert!(
            !pipeline.is_compacting(),
            "new pipeline should not be compacting"
        );
    }
}
