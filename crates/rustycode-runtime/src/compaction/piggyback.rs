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
The conversation context is approaching capacity. You should call this tool \
ALONGSIDE your normal response — do NOT skip your answer to the user. The \
summary you produce will replace older messages on the next turn, freeing \
space so you can continue helping the user without losing important context.

When filling in the fields, distill the full conversation: what the user \
asked, what approach and reasoning you followed (including any internal \
thinking or analysis), which tools you called and what they returned, what \
code was read or changed, errors found and fixed, and the user's evolving \
intent. Preserve enough detail that a fresh reader could pick up exactly \
where you left off.";

/// Optional system-prompt suffix appended when compaction is triggered.
///
/// This reinforces the tool description from the system prompt level, which
/// some models respond to more reliably than tool-level descriptions alone.
/// Models that already call the tool from the description alone are unaffected.
pub const SYSTEM_PROMPT_SUFFIX: &str = "\
[Context Management] The conversation has grown long. A compact_context tool \
has been added to your available tools. Please call it alongside your response \
to this message so that older turns can be condensed. Your response to the \
user is the priority — the tool call is a secondary side-effect.";

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
                    "description": "What has been done so far. Include: files read/edited, tool calls made, errors encountered and fixed, reasoning steps taken, and any intermediate results."
                },
                "decisions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Key decisions made and why — design choices, approach changes, tradeoffs accepted, alternatives rejected."
                },
                "active_files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File paths currently being worked on or that contain unresolved changes."
                },
                "next_step": {
                    "type": "string",
                    "description": "The single most important next step to continue the work."
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
}
