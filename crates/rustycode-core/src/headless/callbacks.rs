use crate::headless::config::REPETITION_CHECK_THRESHOLD;
use crate::headless::helpers::summarize_tool_args;
use crate::runtime::monitor::detect_and_truncate_repeated_blocks;
use crate::streaming::{StreamingCallbacks, ToolAccumulator};
use tracing::info;

/// Headless streaming callbacks for unified SSE event processing
///
/// Bridges the shared SseEventProcessor with headless-specific logic:
/// - Streaming text to stderr for real-time visibility
/// - Repetition detection and truncation
/// - Token counting from usage deltas
/// - Tool argument summarization
pub(crate) struct HeadlessStreamCallbacks<'a> {
    pub(crate) assistant_text: &'a mut String,
    pub(crate) completed_tools: &'a mut Vec<(String, String, String)>, // (id, name, json)
    pub(crate) stop_reason: &'a mut Option<String>,
    pub(crate) total_input_tokens: &'a mut u64,
    pub(crate) total_output_tokens: &'a mut u64,
    pub(crate) total_cache_read_tokens: &'a mut u64,
    pub(crate) total_cache_creation_tokens: &'a mut u64,
    pub(crate) break_stream: &'a mut bool, // Signal to break stream due to repetition
    pub(crate) last_block_was_tool: &'a mut bool, // Track if last block was a tool to know when to print newline
}

impl<'a> StreamingCallbacks for HeadlessStreamCallbacks<'a> {
    fn on_text(&mut self, text: &str) {
        eprint!("{}", text);
        self.assistant_text.push_str(text);
        *self.last_block_was_tool = false;

        if self.assistant_text.lines().count() > REPETITION_CHECK_THRESHOLD
            && self.completed_tools.is_empty()
        {
            if let Some(truncated) = detect_and_truncate_repeated_blocks(self.assistant_text) {
                tracing::warn!(
                    "In-stream repetition detected ({} lines), truncating and breaking",
                    self.assistant_text.lines().count()
                );
                eprintln!("\n\n[Repetition loop detected, truncating]");
                *self.assistant_text = truncated;
                *self.break_stream = true;
            }
        }
    }

    fn on_thinking(&mut self, _thinking: &str) {}

    fn on_tool_start(&mut self, _id: &str, name: &str) {
        eprint!("  🔧 {}(", name);
        *self.last_block_was_tool = true;
    }

    fn on_tool_complete(&mut self, tool: ToolAccumulator) {
        let args_display = summarize_tool_args(&tool.name, &tool.partial_json);
        eprintln!("{})", args_display);
        info!("Tool call: {} ({})", tool.name, tool.id);
        self.completed_tools
            .push((tool.id, tool.name, tool.partial_json));
    }

    fn on_turn_completed(&mut self, stop_reason: &str) {
        *self.stop_reason = Some(stop_reason.to_string());
        if !*self.last_block_was_tool && !self.assistant_text.is_empty() {
            eprintln!();
        }
    }

    fn on_token_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        *self.total_input_tokens = self.total_input_tokens.saturating_add(input_tokens);
        *self.total_output_tokens = self.total_output_tokens.saturating_add(output_tokens);
    }

    fn on_error(&mut self, error_type: &str, message: &str) {
        tracing::error!("SSE error: {} - {}", error_type, message);
    }
}
