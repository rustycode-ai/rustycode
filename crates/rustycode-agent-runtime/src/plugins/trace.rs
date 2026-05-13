//! Conversation trace — writes a markdown trace of tool interactions to disk.

use async_trait::async_trait;
use serde_json::Value;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use super::{AgentPlugin, TurnContext};

/// Writes an incremental markdown trace of tool calls and results.
///
/// The trace is written to `{cwd}/conversation_trace.md` after each tool
/// result and finalized in `on_done`.
pub struct ConversationTrace {
    trace_path: PathBuf,
    content: String,
}

impl ConversationTrace {
    pub fn new(cwd: &Path) -> Self {
        Self {
            trace_path: cwd.join("conversation_trace.md"),
            content: String::new(),
        }
    }
}

#[async_trait]
impl AgentPlugin for ConversationTrace {
    async fn on_start(&mut self, _ctx: &TurnContext) {
        self.content.push_str("# Conversation Trace\n\n");
    }

    async fn on_tool_result(
        &mut self,
        tool_name: &str,
        _tool_id: &str,
        input: &Value,
        output: &mut String,
    ) {
        let input_preview =
            serde_json::to_string(input).unwrap_or_else(|_| "<invalid>".to_string());

        let input_preview = if input_preview.len() > 500 {
            let truncated: String = input_preview.chars().take(500).collect();
            format!("{truncated}...")
        } else {
            input_preview
        };

        let output_preview = if output.len() > 1000 {
            let truncated: String = output.chars().take(1000).collect();
            format!("{truncated}...")
        } else {
            output.clone()
        };

        let _ = write!(
            self.content,
            "## Tool: {tool_name}\n\n**Input:** `{input_preview}`\n\n\
             **Output:**\n```\n{output_preview}\n```\n\n"
        );

        let _ = tokio::fs::write(&self.trace_path, &self.content).await;
    }

    async fn on_done(&mut self, _ctx: &TurnContext) {
        let _ = write!(
            self.content,
            "---\n\n*Trace completed. {} bytes.*\n",
            self.content.len()
        );
        let _ = tokio::fs::write(&self.trace_path, &self.content).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn make_ctx(turn: usize) -> TurnContext {
        TurnContext {
            turn,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cwd: PathBuf::from("/tmp"),
        }
    }

    #[tokio::test]
    async fn on_start_adds_header() {
        let mut trace = ConversationTrace::new(Path::new("/tmp"));
        trace.on_start(&make_ctx(0)).await;
        assert!(trace.content.contains("# Conversation Trace"));
    }

    #[tokio::test]
    async fn on_tool_result_appends_entry() {
        let mut trace = ConversationTrace::new(Path::new("/tmp"));
        trace.on_start(&make_ctx(0)).await;

        let mut output = "file contents".to_string();
        trace
            .on_tool_result(
                "Read",
                "1",
                &serde_json::json!({"path": "foo.rs"}),
                &mut output,
            )
            .await;

        assert!(trace.content.contains("## Tool: Read"));
        assert!(trace.content.contains("foo.rs"));
        assert!(trace.content.contains("file contents"));
    }

    #[tokio::test]
    async fn on_done_adds_footer() {
        let mut trace = ConversationTrace::new(Path::new("/tmp"));
        trace.on_start(&make_ctx(0)).await;
        trace.on_done(&make_ctx(1)).await;
        assert!(trace.content.contains("Trace completed"));
    }

    #[tokio::test]
    async fn default_should_stop_is_false() {
        let mut trace = ConversationTrace::new(Path::new("/tmp"));
        assert!(!trace.should_stop(&make_ctx(0)).await);
    }
}
