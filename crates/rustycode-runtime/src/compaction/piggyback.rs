//! Piggyback summarization — compact context as a side-effect of the next LLM call.
//!
//! Instead of making a dedicated summarization call (sending the full context a
//! second time), we inject a `compact_context` tool definition into the *next*
//! request. The LLM returns its normal answer **plus** a tool call that carries
//! a structured summary. On the following turn we replace older messages with
//! the summary, saving one full round of input tokens.
//!
//! ## Token cost comparison
//!
//! | Approach          | Input tokens              | Extra output |
//! |-------------------|---------------------------|--------------|
//! | Separate summary  | 2 × full_context          | summary      |
//! | Piggyback         | 1 × full_context + tool_def| summary      |
//!
//! Savings ≈ `full_context_tokens − tool_definition_tokens`.

use serde::{Deserialize, Serialize};

// Tool definition

/// The tool name used for piggyback compaction.
pub const TOOL_NAME: &str = "compact_context";

/// Tool description injected alongside other tool definitions when context is
/// approaching the compaction threshold.
///
/// The description is structured to maximize the chance the model actually calls
/// the tool. Key techniques:
///
/// 1. **Signals urgency** — "the conversation context is approaching capacity"
/// 2. **Explains the mechanism** — the summary replaces older messages on the
///    next turn, so the model understands *why* it helps
/// 3. **Explicit instruction to call alongside normal response** — prevents the
///    model from treating this as an either/or choice
/// 4. **Structured field guidance** — each field's description tells the model
///    exactly what to capture, including thinking/reasoning, tool calls, and
///    error fixes
pub const TOOL_DESCRIPTION: &str = "\
The conversation context is approaching capacity. Call this tool ALONGSIDE \
your normal response — your answer to the user is the priority, this is a \
side-effect. The summary will replace older messages on the next turn.

Capture: the user's goal, what has been done, what is still in-progress, \
active files, key decisions, and the immediate next step. Include reasoning \
ONLY when it explains a non-obvious decision or an incomplete approach that \
the next turn needs to continue. Exclude: system instructions, tool \
descriptions, obsolete reasoning, completed tasks that are no longer relevant.";

/// System-prompt suffix for the first piggyback attempt.
///
/// Clear instruction that the tool is available and should be called alongside
/// the normal response. This is the sweet spot between reliability and response
/// quality — strong enough for capable models (glm-5.1, deepseek-v4-flash) but
/// not so aggressive that it kills response content.
pub const SYSTEM_PROMPT_SUFFIX: &str = "\
[Context Management] The conversation has grown long. A compact_context tool \
has been added to your available tools. Please call it alongside your response \
to this message so that older turns can be condensed. Your response to the \
user is the priority — the tool call is a secondary side-effect.";

/// Escalated system-prompt suffix for the second piggyback attempt.
///
/// Used when the model didn't call compact_context on the first attempt.
/// Stronger language signals urgency without being so aggressive that it
/// suppresses the model's normal response entirely.
pub const STRONG_SYSTEM_PROMPT_SUFFIX: &str = "\
IMPORTANT: You MUST call the compact_context tool in your response alongside \
answering the user. This is required to free context space. Answer the user \
FIRST, then call compact_context.";

/// JSON Schema for the `compact_context` tool input.
///
/// Intentionally mirrors the "Compact" summary template (5 sections) so the
/// output is directly usable by [`CompactSummary::from_tool_input`].
pub fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": TOOL_NAME,
        "description": TOOL_DESCRIPTION,
        "input_schema": {
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "What the user is trying to accomplish overall — their high-level intent, not just the last message."
                },
                "progress": {
                    "type": "string",
                    "description": "What has been completed AND what is still in-progress. Include files read/edited, tool calls and results, errors found and fixed. Only include reasoning when it explains a non-obvious decision the next turn will need."
                },
                "decisions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Important decisions still in effect and why. Exclude decisions that have been superseded or are no longer relevant."
                },
                "active_files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths with unresolved changes or that are central to the current in-progress work."
                },
                "next_step": {
                    "type": "string",
                    "description": "The single most important next step to continue the current in-progress work."
                }
            },
            "required": ["goal", "progress", "next_step"]
        }
    })
}

/// Rough token cost of the tool definition above (word-based estimate).
pub fn tool_definition_token_cost() -> usize {
    let json = tool_definition().to_string();
    rustycode_protocol::estimate_tokens(&json)
}

// Summary extraction

/// Structured summary produced by the `compact_context` tool call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompactSummary {
    pub goal: String,
    pub progress: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub active_files: Vec<String>,
    pub next_step: String,
}

impl CompactSummary {
    /// Parse a summary from the `input` field of a `ContentBlock::ToolUse` that
    /// called the `compact_context` tool.
    pub fn from_tool_input(input: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(input.clone()).ok()
    }

    /// Render the summary as a single message string (for injection as a
    /// system message replacing older turns).
    pub fn render(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("## Goal\n{}", self.goal));
        parts.push(format!("## Progress\n{}", self.progress));
        if !self.decisions.is_empty() {
            parts.push(format!(
                "## Decisions\n{}",
                self.decisions
                    .iter()
                    .enumerate()
                    .map(|(i, d)| format!("{i}. {d}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if !self.active_files.is_empty() {
            parts.push(format!("## Active Files\n{}", self.active_files.join(", ")));
        }
        parts.push(format!("## Next Step\n{}", self.next_step));
        parts.join("\n\n")
    }

    /// Rough token count of the rendered summary.
    pub fn estimated_tokens(&self) -> usize {
        rustycode_protocol::estimate_tokens(&self.render())
    }
}

/// Scan a message's content blocks for a `compact_context` tool call and
/// extract the summary.
///
/// Returns `None` if the message doesn't contain a matching tool call.
pub fn extract_summary(message: &rustycode_protocol::Message) -> Option<CompactSummary> {
    use rustycode_protocol::ContentBlock;
    match &message.content {
        rustycode_protocol::MessageContent::Blocks(blocks) => {
            for block in blocks {
                if let ContentBlock::ToolUse { name, input, .. } = block {
                    if name == TOOL_NAME {
                        return CompactSummary::from_tool_input(input);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

// Cost analysis

/// Result of a token-cost comparison between separate and piggyback approaches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostComparison {
    /// Tokens in the full conversation context.
    pub context_tokens: usize,
    /// Tokens for the separate summary approach (2 × context + summary output).
    pub separate_input_tokens: usize,
    /// Tokens for the piggyback approach (1 × context + tool definition).
    pub piggyback_input_tokens: usize,
    /// Tokens saved by using piggyback instead of separate.
    pub tokens_saved: usize,
    /// Percentage reduction in input tokens.
    pub savings_pct: f64,
}

/// Compare the input-token cost of both approaches for a given context size.
pub fn compare_costs(context_tokens: usize) -> CostComparison {
    let tool_def_cost = tool_definition_token_cost();

    // Separate: send full context twice (once for summary call, once for actual).
    let separate_input = context_tokens * 2;

    // Piggyback: send full context once + the tool definition.
    let piggyback_input = context_tokens + tool_def_cost;

    let tokens_saved = separate_input.saturating_sub(piggyback_input);
    let savings_pct = if separate_input > 0 {
        (tokens_saved as f64 / separate_input as f64) * 100.0
    } else {
        0.0
    };

    CostComparison {
        context_tokens,
        separate_input_tokens: separate_input,
        piggyback_input_tokens: piggyback_input,
        tokens_saved,
        savings_pct,
    }
}

// Piggyback state machine

/// State machine for piggyback compaction across turns.
///
/// The lifecycle is:
///
/// 1. **Idle** — no compaction needed. Call [`PiggybackState::should_inject`]
///    each turn to check if context is approaching capacity.
///
/// 2. **Pending** — compaction triggered. The tool definition + system prompt
///    suffix are injected into the next LLM call. The caller sends the request
///    with the extra tool and (optionally) the appended system prompt.
///
/// 3. **Extracted** — after the LLM responds, call
///    [`PiggybackState::process_response`] to extract the summary from the
///    response. This returns a [`PiggybackResult`] with the summary and the
///    assistant's normal response (with the tool call stripped).
///
/// 4. **Compacted** — the caller replaces old messages with the summary
///    message for subsequent turns. Call [`PiggybackState::reset`] to return
///    to Idle.
///
/// If the LLM doesn't call `compact_context`, `process_response` returns
/// `PiggybackResult::NotCalled` and the state stays Pending for one more
/// attempt before falling back to the separate summarization pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum PiggybackState {
    /// No compaction in progress.
    Idle,
    /// Tool injected, waiting for LLM response.
    Pending {
        /// How many attempts have been made (max 2 before fallback).
        attempts: u8,
    },
    /// Summary extracted, waiting for caller to apply compaction.
    Compacted {
        summary: CompactSummary,
        /// The assistant's normal response text (tool call stripped).
        response_text: String,
    },
}

/// Result of processing an LLM response in piggyback mode.
#[derive(Debug)]
pub enum PiggybackResult {
    /// The LLM called compact_context and a summary was extracted.
    Compacted {
        summary: CompactSummary,
        /// The assistant's normal response (compact_context tool call removed).
        response_text: String,
    },
    /// The LLM did NOT call compact_context.
    NotCalled,
    /// Max attempts exhausted — should fall back to separate summarization.
    Fallback,
}

/// Maximum number of piggyback attempts before falling back.
const MAX_ATTEMPTS: u8 = 2;

impl PiggybackState {
    /// Create a new idle state.
    pub fn new() -> Self {
        Self::Idle
    }

    /// Check whether piggyback compaction should be triggered for the given
    /// token counts, and transition to Pending if so.
    ///
    /// Returns `true` if the tool definition should be injected into the next
    /// LLM call.
    pub fn should_inject(&mut self, current_tokens: usize, threshold: usize) -> bool {
        if current_tokens < threshold {
            return false;
        }

        match self {
            Self::Idle => {
                *self = Self::Pending { attempts: 0 };
                true
            }
            Self::Pending { .. } | Self::Compacted { .. } => {
                // Already pending or compacted — don't inject again.
                false
            }
        }
    }

    /// Whether the tool definition should be included in the current request.
    pub fn needs_tool_injection(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    /// Whether the system prompt suffix should be appended.
    pub fn needs_system_suffix(&self) -> bool {
        matches!(self, Self::Pending { .. })
    }

    /// Return the system prompt suffix appropriate for the current attempt.
    ///
    /// - First attempt (`attempts == 0`): [`SYSTEM_PROMPT_SUFFIX`] (clear but
    ///   not aggressive — preserves response quality).
    /// - Second attempt (`attempts == 1`): [`STRONG_SYSTEM_PROMPT_SUFFIX`]
    ///   (escalated urgency — higher call rate but may compress response).
    pub fn system_suffix(&self) -> Option<&'static str> {
        match self {
            Self::Pending { attempts } => {
                if *attempts == 0 {
                    Some(SYSTEM_PROMPT_SUFFIX)
                } else {
                    Some(STRONG_SYSTEM_PROMPT_SUFFIX)
                }
            }
            _ => None,
        }
    }

    /// Process an LLM response, extracting the compact_context summary if
    /// present.
    ///
    /// Call this after receiving the assistant's response when in the Pending
    /// state.
    pub fn process_response(
        &mut self,
        assistant_message: &rustycode_protocol::Message,
    ) -> PiggybackResult {
        let attempts = match self {
            Self::Pending { attempts } => *attempts,
            _ => return PiggybackResult::NotCalled,
        };

        // Try to extract the summary.
        if let Some(summary) = extract_summary(assistant_message) {
            // Strip the tool call from the response text.
            let response_text = strip_compact_tool_call(assistant_message);
            *self = Self::Compacted {
                summary: summary.clone(),
                response_text: response_text.clone(),
            };
            PiggybackResult::Compacted {
                summary,
                response_text,
            }
        } else if attempts + 1 >= MAX_ATTEMPTS {
            // Exhausted attempts — signal fallback.
            *self = Self::Idle;
            PiggybackResult::Fallback
        } else {
            // Retry next turn.
            *self = Self::Pending {
                attempts: attempts + 1,
            };
            PiggybackResult::NotCalled
        }
    }

    /// Build the compacted message list for the next turn.
    ///
    /// Replaces `old_messages` (everything before `tail_start_index`) with the
    /// rendered summary as a system message. Messages from `tail_start_index`
    /// onward are preserved.
    ///
    /// **Important:** The caller should NOT include the piggyback trigger
    /// message (user turn that triggered compaction) or the assistant response
    /// that contained the `compact_context` tool call in `old_messages`.
    /// Those two turns are consumed by the piggyback process and should be
    /// excluded from `tail_start_index` onward. Pass the messages *before*
    /// the piggyback trigger as `old_messages`, with `tail_start_index`
    /// pointing to where the preserved tail begins.
    ///
    /// Only call this when in the `Compacted` state.
    pub fn build_compacted_messages(
        &self,
        old_messages: Vec<rustycode_protocol::Message>,
        tail_start_index: usize,
    ) -> Option<Vec<rustycode_protocol::Message>> {
        match self {
            Self::Compacted { summary, .. } => {
                let summary_msg = rustycode_protocol::Message::system(summary.render());
                let mut result = vec![summary_msg];
                if tail_start_index < old_messages.len() {
                    result.extend_from_slice(&old_messages[tail_start_index..]);
                }
                Some(result)
            }
            _ => None,
        }
    }

    /// Reset to idle state. Call after compaction has been applied (or when
    /// falling back to the separate pipeline).
    pub fn reset(&mut self) {
        *self = Self::Idle;
    }
}

impl Default for PiggybackState {
    fn default() -> Self {
        Self::new()
    }
}

// Emergency compaction

/// Check whether an error from an LLM provider indicates the context window was
/// exceeded. Covers error patterns from Anthropic, OpenAI, Gemini, and common
/// proxy/gateway responses.
pub fn is_context_length_error(error_text: &str) -> bool {
    let lower = error_text.to_lowercase();
    CONTEXT_LENGTH_PATTERNS
        .iter()
        .any(|pat| lower.contains(pat))
}

/// Known substrings that signal a context-length-exceeded error.
const CONTEXT_LENGTH_PATTERNS: &[&str] = &[
    "context_length_exceeded",
    "context window",
    "maximum context length",
    "too many tokens",
    "token limit",
    "reduce the length",
    "request too large",
    "input is too long",
    "prompt is too long",
    "exceeds the maximum",
    "max_tokens",
];

/// Aggressively compact messages without any LLM calls.
///
/// This is the last-resort path when:
/// - The server returned a context-too-long error and we need to shrink
///   immediately before retrying.
/// - Piggyback compaction failed and we need a direct compaction fallback.
///
/// Applies, in order:
/// 1. **Snip** — trim tool output to `max_tool_lines` per block.
/// 2. **Truncate** — keep only `tail_turns` recent turns.
/// 3. **Emergency trim** — if still over budget, keep only the last user turn.
///
/// Returns the compacted messages and the number of messages removed.
pub fn emergency_compact(
    messages: Vec<rustycode_protocol::Message>,
    target_tokens: usize,
    tail_turns: usize,
    max_tool_lines: usize,
) -> EmergencyCompactResult {
    let tokens_before = estimate_message_tokens(&messages);
    let mut current = messages;
    let mut tiers_applied: Vec<String> = Vec::new();

    // Phase 1: Snip — trim tool output (free, no LLM).
    let snip = super::tiers::SnipTier::new(max_tool_lines);
    let snipped = snip.compact(current);
    current = snipped.messages;

    let tokens_after_snip = estimate_message_tokens(&current);
    if tokens_after_snip <= target_tokens {
        return EmergencyCompactResult {
            messages: current,
            tokens_before,
            tokens_after: tokens_after_snip,
            tiers_applied: vec!["snip".to_string()],
        };
    }
    tiers_applied.push("snip".to_string());

    // Phase 2: Truncate — hard cut to tail turns.
    let truncate = super::tiers::TruncateTier::new(tail_turns);
    let truncated = truncate.compact(current);
    current = truncated.messages;

    let tokens_after_truncate = estimate_message_tokens(&current);
    if tokens_after_truncate <= target_tokens {
        return EmergencyCompactResult {
            messages: current,
            tokens_before,
            tokens_after: tokens_after_truncate,
            tiers_applied,
        };
    }
    tiers_applied.push("truncate".to_string());

    // Phase 3: Emergency trim — keep only last user + assistant.
    let trimmed = super::pipeline::emergency_trim(current);
    let tokens_after_emergency = estimate_message_tokens(&trimmed);
    tiers_applied.push("emergency".to_string());

    EmergencyCompactResult {
        messages: trimmed,
        tokens_before,
        tokens_after: tokens_after_emergency,
        tiers_applied,
    }
}

/// Result of emergency compaction.
#[derive(Debug, Clone)]
pub struct EmergencyCompactResult {
    /// Compacted messages.
    pub messages: Vec<rustycode_protocol::Message>,
    /// Token count before compaction.
    pub tokens_before: usize,
    /// Token count after compaction.
    pub tokens_after: usize,
    /// Tiers applied, in order.
    pub tiers_applied: Vec<String>,
}

/// Estimate tokens for a slice of messages using the canonical heuristic.
fn estimate_message_tokens(messages: &[rustycode_protocol::Message]) -> usize {
    messages
        .iter()
        .map(|m| rustycode_protocol::estimate_tokens(&m.content.as_text()))
        .sum()
}

/// Extract the assistant's text response, stripping any compact_context tool
/// call blocks. Returns the concatenated text from all non-compact Text blocks.
fn strip_compact_tool_call(message: &rustycode_protocol::Message) -> String {
    use rustycode_protocol::{ContentBlock, MessageContent};

    match &message.content {
        MessageContent::Simple(text) => text.clone(),
        MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                ContentBlock::ToolUse { name, .. } if name == TOOL_NAME => None,
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use rustycode_protocol::{ContentBlock, Message, MessageContent, MessageRole};

    fn compact_tool_call(input: serde_json::Value) -> Message {
        Message {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "I'll continue working on that.".to_string(),
                    cache_control: None,
                },
                ContentBlock::ToolUse {
                    id: "comp_001".to_string(),
                    name: TOOL_NAME.to_string(),
                    input,
                },
            ]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        }
    }

    #[test]
    fn tool_definition_is_valid_json() {
        let def = tool_definition();
        assert_eq!(def["name"], TOOL_NAME);
        assert!(def["input_schema"]["properties"]["goal"].is_object());
        assert!(def["input_schema"]["properties"]["progress"].is_object());
        assert!(def["input_schema"]["properties"]["next_step"].is_object());
    }

    #[test]
    fn tool_definition_token_cost_reasonable() {
        let cost = tool_definition_token_cost();
        // The tool definition is ~150 words — should estimate ~100-200 tokens.
        assert!(
            (50..500).contains(&cost),
            "tool definition cost should be 50-500 tokens, got {cost}"
        );
    }

    #[test]
    fn extract_summary_from_tool_call() {
        let input = serde_json::json!({
            "goal": "Fix the authentication bug",
            "progress": "Read src/auth.rs, found the token expiry issue",
            "decisions": ["Use JWT refresh tokens", "Remove session cookies"],
            "active_files": ["src/auth.rs", "src/middleware.rs"],
            "next_step": "Write tests for the refresh token flow"
        });
        let msg = compact_tool_call(input);
        let summary = extract_summary(&msg).expect("should extract summary");

        assert_eq!(summary.goal, "Fix the authentication bug");
        assert_eq!(
            summary.progress,
            "Read src/auth.rs, found the token expiry issue"
        );
        assert_eq!(summary.decisions.len(), 2);
        assert_eq!(summary.active_files.len(), 2);
        assert!(summary.active_files.contains(&"src/auth.rs".to_string()));
        assert_eq!(summary.next_step, "Write tests for the refresh token flow");
    }

    #[test]
    fn extract_summary_returns_none_for_wrong_tool() {
        let msg = Message {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![ContentBlock::ToolUse {
                id: "t1".to_string(),
                name: "read_file".to_string(),
                input: serde_json::json!({"path": "/some/file"}),
            }]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        };
        assert!(extract_summary(&msg).is_none());
    }

    #[test]
    fn extract_summary_returns_none_for_text_only() {
        let msg = Message::assistant("just a regular response");
        assert!(extract_summary(&msg).is_none());
    }

    #[test]
    fn render_summary_includes_all_sections() {
        let summary = CompactSummary {
            goal: "Fix auth bug".to_string(),
            progress: "Found the issue".to_string(),
            decisions: vec!["Use JWT".to_string()],
            active_files: vec!["src/auth.rs".to_string()],
            next_step: "Write tests".to_string(),
        };
        let rendered = summary.render();

        assert!(rendered.contains("## Goal"));
        assert!(rendered.contains("Fix auth bug"));
        assert!(rendered.contains("## Progress"));
        assert!(rendered.contains("## Decisions"));
        assert!(rendered.contains("Use JWT"));
        assert!(rendered.contains("## Active Files"));
        assert!(rendered.contains("src/auth.rs"));
        assert!(rendered.contains("## Next Step"));
        assert!(rendered.contains("Write tests"));
    }

    #[test]
    fn render_summary_omits_empty_sections() {
        let summary = CompactSummary {
            goal: "Do something".to_string(),
            progress: "Started".to_string(),
            decisions: Vec::new(),
            active_files: Vec::new(),
            next_step: "Continue".to_string(),
        };
        let rendered = summary.render();

        assert!(
            !rendered.contains("## Decisions"),
            "empty decisions should be omitted"
        );
        assert!(
            !rendered.contains("## Active Files"),
            "empty files should be omitted"
        );
        assert!(rendered.contains("## Goal"));
    }

    #[test]
    fn summary_preserves_file_paths() {
        let summary = CompactSummary {
            goal: "Refactor".to_string(),
            progress: "Read files".to_string(),
            decisions: Vec::new(),
            active_files: vec![
                "src/auth/jwt.rs".to_string(),
                "src/middleware/rate_limit.rs".to_string(),
                "tests/auth_test.rs".to_string(),
            ],
            next_step: "Edit jwt.rs".to_string(),
        };
        let rendered = summary.render();

        assert!(
            rendered.contains("src/auth/jwt.rs"),
            "file paths should be preserved"
        );
        assert!(rendered.contains("src/middleware/rate_limit.rs"));
        assert!(rendered.contains("tests/auth_test.rs"));
    }

    // -- Cost comparison experiments --

    #[test]
    fn piggyback_saves_50_percent_at_large_context() {
        // 100K context — typical for a long coding session.
        let cmp = compare_costs(100_000);

        assert_eq!(
            cmp.separate_input_tokens, 200_000,
            "separate sends context twice"
        );
        assert!(
            cmp.piggyback_input_tokens < 101_000,
            "piggyback sends context once + small tool def, got {}",
            cmp.piggyback_input_tokens
        );
        assert!(
            cmp.savings_pct > 49.0,
            "should save ~50% input tokens, got {:.1}%",
            cmp.savings_pct
        );
        assert!(
            cmp.tokens_saved > 99_000,
            "should save ~100K tokens, got {}",
            cmp.tokens_saved
        );
    }

    #[test]
    fn piggyback_saves_at_small_context() {
        let cmp = compare_costs(10_000);
        assert_eq!(cmp.separate_input_tokens, 20_000);
        assert!(cmp.piggyback_input_tokens < 10_200);
        assert!(
            cmp.savings_pct > 45.0,
            "savings should be >45% even at small scale"
        );
    }

    #[test]
    fn piggyback_savings_scale_linearly() {
        let small = compare_costs(20_000);
        let large = compare_costs(200_000);

        // Savings should be roughly proportional to context size.
        let ratio = large.tokens_saved as f64 / small.tokens_saved as f64;
        let expected_ratio = 200_000_f64 / 20_000_f64;
        assert!(
            (ratio - expected_ratio).abs() < 1.0,
            "savings should scale linearly: ratio={ratio:.1}, expected={expected_ratio:.1}"
        );
    }

    #[test]
    fn piggyback_at_zero_context_no_panic() {
        let cmp = compare_costs(0);
        assert_eq!(cmp.tokens_saved, 0);
        assert_eq!(cmp.savings_pct, 0.0);
    }

    // -- Summary quality experiments --

    #[test]
    fn summary_is_much_smaller_than_original_context() {
        // Simulate: original context has 200 words across 5 file reads.
        let original_text = (0..5)
            .flat_map(|i| {
                let line = format!("line {i} ");
                std::iter::repeat_n(line, 40).collect::<Vec<_>>()
            })
            .collect::<String>();
        let original_tokens = rustycode_protocol::estimate_tokens(&original_text);

        let summary = CompactSummary {
            goal: "Fix auth bug in jwt.rs".to_string(),
            progress: "Read 5 files, found token expiry issue".to_string(),
            decisions: vec!["Use refresh tokens".to_string()],
            active_files: vec!["src/auth/jwt.rs".to_string()],
            next_step: "Write refresh token tests".to_string(),
        };
        let summary_tokens = summary.estimated_tokens();

        assert!(
            summary_tokens < original_tokens / 10,
            "summary ({summary_tokens} tokens) should be <10% of original ({original_tokens} tokens)"
        );
    }

    #[test]
    fn from_tool_input_handles_extra_fields_gracefully() {
        let input = serde_json::json!({
            "goal": "Do stuff",
            "progress": "Done some",
            "decisions": [],
            "active_files": [],
            "next_step": "Continue",
            "extra_field": "ignored"
        });
        let summary = CompactSummary::from_tool_input(&input);
        assert!(summary.is_some(), "extra fields should not break parsing");
    }

    #[test]
    fn from_tool_input_handles_missing_optional_fields() {
        let input = serde_json::json!({
            "goal": "Do stuff",
            "progress": "Done some",
            "next_step": "Continue"
        });
        let summary = CompactSummary::from_tool_input(&input).expect("should parse");
        assert!(summary.decisions.is_empty());
        assert!(summary.active_files.is_empty());
    }

    #[test]
    fn from_tool_input_rejects_missing_required_fields() {
        let input = serde_json::json!({
            "goal": "Do stuff",
            "progress": "Done some"
            // missing "next_step"
        });
        assert!(
            CompactSummary::from_tool_input(&input).is_none(),
            "should reject missing required field"
        );
    }

    // -- PiggybackState state machine tests --

    #[test]
    fn state_starts_idle() {
        let state = PiggybackState::new();
        assert_eq!(state, PiggybackState::Idle);
        assert!(!state.needs_tool_injection());
    }

    #[test]
    fn state_transitions_to_pending_when_threshold_exceeded() {
        let mut state = PiggybackState::new();
        assert!(state.should_inject(100_000, 80_000));
        assert_eq!(state, PiggybackState::Pending { attempts: 0 });
        assert!(state.needs_tool_injection());
        assert!(state.needs_system_suffix());
    }

    #[test]
    fn state_stays_idle_below_threshold() {
        let mut state = PiggybackState::new();
        assert!(!state.should_inject(50_000, 80_000));
        assert_eq!(state, PiggybackState::Idle);
    }

    #[test]
    fn state_does_not_re_trigger_while_pending() {
        let mut state = PiggybackState::new();
        state.should_inject(100_000, 80_000);
        assert!(!state.should_inject(120_000, 80_000));
        assert_eq!(state, PiggybackState::Pending { attempts: 0 });
    }

    #[test]
    fn process_response_extracts_summary() {
        let mut state = PiggybackState::new();
        state.should_inject(100_000, 80_000);

        let msg = Message {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Here are the tests.".to_string(),
                    cache_control: None,
                },
                ContentBlock::ToolUse {
                    id: "comp_001".to_string(),
                    name: TOOL_NAME.to_string(),
                    input: serde_json::json!({
                        "goal": "Fix auth",
                        "progress": "Found the bug",
                        "decisions": ["Use JWT"],
                        "active_files": ["src/auth.rs"],
                        "next_step": "Write tests"
                    }),
                },
            ]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        };

        let result = state.process_response(&msg);
        match result {
            PiggybackResult::Compacted {
                summary,
                response_text,
            } => {
                assert_eq!(summary.goal, "Fix auth");
                assert_eq!(response_text, "Here are the tests.");
            }
            other => panic!("expected Compacted, got {other:?}"),
        }
        assert!(matches!(state, PiggybackState::Compacted { .. }));
    }

    #[test]
    fn process_response_not_called_when_no_tool_use() {
        let mut state = PiggybackState::new();
        state.should_inject(100_000, 80_000);

        let msg = Message::assistant("just a normal response");
        let result = state.process_response(&msg);
        assert!(matches!(result, PiggybackResult::NotCalled));
        assert_eq!(state, PiggybackState::Pending { attempts: 1 });
    }

    #[test]
    fn process_response_fallback_after_max_attempts() {
        let mut state = PiggybackState::Pending { attempts: 1 };
        let msg = Message::assistant("still no tool call");
        let result = state.process_response(&msg);
        assert!(matches!(result, PiggybackResult::Fallback));
        assert_eq!(state, PiggybackState::Idle);
    }

    #[test]
    fn build_compacted_messages_replaces_head() {
        let summary = CompactSummary {
            goal: "Fix auth".to_string(),
            progress: "Found bug".to_string(),
            decisions: vec!["Use JWT".to_string()],
            active_files: vec!["src/auth.rs".to_string()],
            next_step: "Write tests".to_string(),
        };

        let state = PiggybackState::Compacted {
            summary,
            response_text: "Done.".to_string(),
        };

        let old_messages = vec![
            Message::user("old q1"),
            Message::assistant("old a1"),
            Message::user("old q2"),
            Message::assistant("old a2"),
            Message::user("keep this"),
            Message::assistant("keep this too"),
        ];

        // Preserve last 2 messages (index 4 and 5).
        let compacted = state
            .build_compacted_messages(old_messages, 4)
            .expect("should build");

        // Should be: [summary_system_msg, tail messages]
        assert_eq!(compacted.len(), 3);
        assert!(
            compacted[0].content.as_text().contains("## Goal"),
            "first message should be the summary"
        );
        assert!(compacted[0].is_system());
        assert_eq!(compacted[1].content.as_text(), "keep this");
        assert_eq!(compacted[2].content.as_text(), "keep this too");
    }

    #[test]
    fn build_compacted_returns_none_when_not_compacted() {
        let state = PiggybackState::Idle;
        let result = state.build_compacted_messages(vec![], 0);
        assert!(result.is_none());
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut state = PiggybackState::Compacted {
            summary: CompactSummary {
                goal: "X".to_string(),
                progress: "Y".to_string(),
                decisions: Vec::new(),
                active_files: Vec::new(),
                next_step: "Z".to_string(),
            },
            response_text: "done".to_string(),
        };
        state.reset();
        assert_eq!(state, PiggybackState::Idle);
    }

    #[test]
    fn system_suffix_returns_normal_on_first_attempt() {
        let mut state = PiggybackState::new();
        state.should_inject(100_000, 80_000);
        let suffix = state.system_suffix();
        assert_eq!(suffix, Some(SYSTEM_PROMPT_SUFFIX));
        assert!(
            suffix.unwrap().contains("[Context Management]"),
            "first attempt should use normal suffix"
        );
    }

    #[test]
    fn system_suffix_returns_strong_on_second_attempt() {
        let state = PiggybackState::Pending { attempts: 1 };
        let suffix = state.system_suffix();
        assert_eq!(suffix, Some(STRONG_SYSTEM_PROMPT_SUFFIX));
        assert!(
            suffix.unwrap().contains("MUST"),
            "second attempt should use strong suffix"
        );
    }

    #[test]
    fn system_suffix_returns_none_when_not_pending() {
        let state = PiggybackState::Idle;
        assert!(state.system_suffix().is_none());

        let state = PiggybackState::Compacted {
            summary: CompactSummary {
                goal: "X".to_string(),
                progress: "Y".to_string(),
                decisions: Vec::new(),
                active_files: Vec::new(),
                next_step: "Z".to_string(),
            },
            response_text: "done".to_string(),
        };
        assert!(state.system_suffix().is_none());
    }

    #[test]
    fn full_lifecycle_idle_to_compacted_to_idle() {
        let mut state = PiggybackState::new();

        // 1. Idle — below threshold
        assert!(!state.should_inject(30_000, 80_000));

        // 2. Threshold exceeded — transition to Pending
        assert!(state.should_inject(90_000, 80_000));
        assert!(state.needs_tool_injection());

        // 3. LLM responds with tool call
        let msg = Message {
            role: MessageRole::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Working on it.".to_string(),
                    cache_control: None,
                },
                ContentBlock::ToolUse {
                    id: "comp_001".to_string(),
                    name: TOOL_NAME.to_string(),
                    input: serde_json::json!({
                        "goal": "Fix bug",
                        "progress": "Reading code",
                        "next_step": "Edit file"
                    }),
                },
            ]),
            timestamp: chrono::Utc::now(),
            metadata: rustycode_protocol::MessageMetadata::default(),
        };

        let result = state.process_response(&msg);
        assert!(matches!(result, PiggybackResult::Compacted { .. }));

        // 4. Build compacted messages
        let old = vec![Message::user("q"), Message::assistant("a")];
        let compacted = state.build_compacted_messages(old, 1).unwrap();
        assert_eq!(compacted.len(), 2); // summary + tail

        // 5. Reset
        state.reset();
        assert_eq!(state, PiggybackState::Idle);
    }

    // -- Emergency compaction tests --

    #[test]
    fn detect_anthropic_context_length_error() {
        assert!(is_context_length_error(
            "Error: context_length_exceeded: your input is too long"
        ));
    }

    #[test]
    fn detect_openai_context_length_error() {
        assert!(is_context_length_error(
            "This model's maximum context length is 128000 tokens however you requested 150000"
        ));
    }

    #[test]
    fn detect_gemini_context_length_error() {
        assert!(is_context_length_error(
            "Request too large. Please reduce the length of messages."
        ));
    }

    #[test]
    fn detect_generic_token_limit_error() {
        assert!(is_context_length_error(
            "Token limit exceeded for this request"
        ));
        assert!(is_context_length_error("too many tokens in the prompt"));
    }

    #[test]
    fn reject_non_context_error() {
        assert!(!is_context_length_error("Network timeout"));
        assert!(!is_context_length_error("Rate limit exceeded"));
        assert!(!is_context_length_error("Internal server error"));
        assert!(!is_context_length_error("Invalid API key"));
    }

    #[test]
    fn context_error_detection_is_case_insensitive() {
        assert!(is_context_length_error("CONTEXT_LENGTH_EXCEEDED"));
        assert!(is_context_length_error("Context Window Is Full"));
    }

    #[test]
    fn emergency_compact_truncates_to_tail() {
        let messages = vec![
            Message::user("old question 1"),
            Message::assistant("old answer 1"),
            Message::user("old question 2"),
            Message::assistant("old answer 2"),
            Message::user("recent question"),
            Message::assistant("recent answer"),
        ];

        let result = emergency_compact(messages, 0, 1, 50);

        // With 1 tail turn, should keep last user+assistant pair.
        assert!(
            result.messages.len() <= 2,
            "emergency_compact with 1 tail_turn should keep at most 2 messages, got {}",
            result.messages.len()
        );
        assert!(result.tiers_applied.contains(&"truncate".to_string()));
        assert!(result.tokens_after <= result.tokens_before);
    }

    #[test]
    fn emergency_compact_snip_only_when_sufficient() {
        // Short messages — snip alone should bring it under target.
        let messages = vec![
            Message::user("short question"),
            Message::assistant("short answer"),
        ];

        let result = emergency_compact(messages.clone(), 1000, 2, 50);

        // Snip should be enough — no truncation needed.
        assert!(
            result.tiers_applied.len() == 1,
            "short messages should only need snip, got {:?}",
            result.tiers_applied
        );
        assert_eq!(result.tiers_applied[0], "snip");
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn emergency_compact_uses_emergency_trim_as_last_resort() {
        // Long messages that exceed even after truncation.
        let long_text: String = (0..500).fold(String::new(), |mut s, i| {
            use std::fmt::Write;
            let _ = write!(s, "word{i} ");
            s
        });

        let messages = vec![
            Message::user(long_text.as_str()),
            Message::assistant(long_text.as_str()),
            Message::user(long_text.as_str()),
            Message::assistant(long_text.as_str()),
            Message::user(long_text.as_str()),
            Message::assistant(long_text.as_str()),
        ];

        // Very small target — forces emergency trim.
        // Use tail_turns=1 so truncate keeps 2 messages that still exceed target=10.
        let result = emergency_compact(messages, 10, 1, 10);

        assert!(
            result.tiers_applied.contains(&"emergency".to_string()),
            "should reach emergency tier, got {:?}",
            result.tiers_applied
        );
        assert!(
            result.messages.len() <= 2,
            "emergency trim should keep at most 2 messages"
        );
    }

    #[test]
    fn emergency_compact_preserves_latest_content() {
        let messages = vec![
            Message::user("first question"),
            Message::assistant("first answer"),
            Message::user("latest question about jwt.rs"),
            Message::assistant("latest answer about jwt.rs"),
        ];

        let result = emergency_compact(messages, 0, 1, 50);
        let combined: String = result
            .messages
            .iter()
            .map(|m| m.content.as_text())
            .collect::<Vec<_>>()
            .join(" ");

        assert!(
            combined.contains("latest question"),
            "should preserve latest content"
        );
        assert!(
            combined.contains("jwt.rs"),
            "should preserve important keywords from latest turn"
        );
    }

    #[test]
    fn emergency_compact_handles_empty_messages() {
        let messages: Vec<Message> = Vec::new();
        let result = emergency_compact(messages, 100, 2, 50);
        assert!(result.messages.is_empty());
        assert_eq!(result.tokens_before, 0);
        assert_eq!(result.tokens_after, 0);
    }

    #[test]
    fn emergency_compact_result_tracks_tokens() {
        let messages = vec![
            Message::user("q1"),
            Message::assistant("a1"),
            Message::user("q2"),
            Message::assistant("a2"),
        ];

        let result = emergency_compact(messages, 2, 1, 50);

        assert!(
            result.tokens_after < result.tokens_before,
            "tokens_after ({}) should be less than tokens_before ({})",
            result.tokens_after,
            result.tokens_before
        );
    }
}
