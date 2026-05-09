//! Unified stream event processor for streaming LLM responses
//!
//! This module provides a canonical event dispatch implementation
//! that both headless and TUI can use, consuming provider-agnostic
//! StreamEvent instead of provider-specific wire formats.

use crate::streaming::tool_state::ToolAccumulator;
use rustycode_protocol::stream_event::StreamEvent;

/// Callbacks for stream event handling
///
/// Implemented by headless and TUI to handle semantic results of events.
/// This is the seam between shared dispatch logic and domain-specific handling.
pub trait StreamingCallbacks {
    /// Called when a text delta arrives (not inside a tool use block)
    fn on_text(&mut self, text: &str);

    /// Called when a thinking delta arrives (not inside a tool use block)
    ///
    /// Default implementation is a no-op, since headless ignores thinking.
    /// TUI overrides this to send thinking to the UI.
    fn on_thinking(&mut self, _thinking: &str) {}

    /// Called when a tool use block starts
    ///
    /// Default implementation is a no-op. Headless overrides to print tool name.
    fn on_tool_start(&mut self, _id: &str, _name: &str) {}

    /// Called when a tool accumulator is complete (all input received)
    fn on_tool_complete(&mut self, tool: ToolAccumulator);

    /// Called when the LLM turn ends (stop_reason and usage)
    fn on_turn_completed(&mut self, stop_reason: &str);

    /// Called on token usage report
    fn on_token_usage(&mut self, input_tokens: u64, output_tokens: u64);

    /// Called on error event
    fn on_error(&mut self, error_type: &str, message: &str);
}

/// Unified stream event processor
///
/// Maintains state (active_tool accumulator) and dispatches
/// StreamEvents to callbacks. Callers should create one instance per stream
/// and call process_event for each event.
pub struct StreamEventProcessor {
    active_tool: Option<ToolAccumulator>,
}

impl StreamEventProcessor {
    pub fn new() -> Self {
        Self { active_tool: None }
    }

    /// Process one stream event, calling appropriate callbacks
    ///
    pub fn process_event<C: StreamingCallbacks>(
        &mut self,
        event: StreamEvent,
        callbacks: &mut C,
    ) -> anyhow::Result<bool> {
        match event {
            StreamEvent::TextDelta { content } => {
                callbacks.on_text(&content);
            }

            StreamEvent::ThinkingDelta { content } => {
                callbacks.on_thinking(&content);
            }

            StreamEvent::ToolCallStarted { id, name } => {
                // Complete any previous tool before starting new one
                self.flush_active_tool(callbacks);
                callbacks.on_tool_start(&id, &name);
                self.active_tool = Some(ToolAccumulator::new(id, name, String::new()));
            }

            StreamEvent::ToolInputDelta { chunk, .. } => {
                if let Some(ref mut tool) = self.active_tool {
                    tool.push_json(&chunk);
                }
            }

            StreamEvent::TurnCompleted { stop_reason } => {
                self.flush_active_tool(callbacks);
                callbacks.on_turn_completed(&stop_reason);
                return Ok(false);
            }

            StreamEvent::TokenUsage {
                input_tokens,
                output_tokens,
            } => {
                callbacks.on_token_usage(input_tokens, output_tokens);
            }

            StreamEvent::CacheUsage { .. } => {
                // Cost accounting only; no callback needed
            }

            // Ignore tool execution lifecycle events in this processor
            StreamEvent::ToolExecStarted { .. } => {}
            StreamEvent::ToolExecCompleted { .. } => {}
            StreamEvent::TurnStarted { .. } => {}
            StreamEvent::Done => {
                self.flush_active_tool(callbacks);
                return Ok(false);
            }

            _ => {}
        }

        Ok(true)
    }

    fn flush_active_tool<C: StreamingCallbacks>(&mut self, callbacks: &mut C) {
        if let Some(tool) = self.active_tool.take() {
            callbacks.on_tool_complete(tool);
        }
    }
}

impl Default for StreamEventProcessor {
    fn default() -> Self {
        Self::new()
    }
}

// Type alias for backward compatibility
pub type SseEventProcessor = StreamEventProcessor;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCallbacks {
        texts: Vec<String>,
        thinkings: Vec<String>,
        tools_completed: Vec<(String, String)>,
        turn_completed: Vec<String>,
        token_usages: Vec<(u64, u64)>,
        errors: Vec<(String, String)>,
    }

    impl TestCallbacks {
        fn new() -> Self {
            Self {
                texts: vec![],
                thinkings: vec![],
                tools_completed: vec![],
                turn_completed: vec![],
                token_usages: vec![],
                errors: vec![],
            }
        }
    }

    impl StreamingCallbacks for TestCallbacks {
        fn on_text(&mut self, text: &str) {
            self.texts.push(text.to_string());
        }

        fn on_thinking(&mut self, thinking: &str) {
            self.thinkings.push(thinking.to_string());
        }

        fn on_tool_start(&mut self, _id: &str, _name: &str) {}

        fn on_tool_complete(&mut self, tool: ToolAccumulator) {
            self.tools_completed.push((tool.id, tool.name));
        }

        fn on_turn_completed(&mut self, stop_reason: &str) {
            self.turn_completed.push(stop_reason.to_string());
        }

        fn on_token_usage(&mut self, input_tokens: u64, output_tokens: u64) {
            self.token_usages.push((input_tokens, output_tokens));
        }

        fn on_error(&mut self, error_type: &str, message: &str) {
            self.errors
                .push((error_type.to_string(), message.to_string()));
        }
    }

    #[test]
    fn test_text_event() {
        let mut processor = StreamEventProcessor::new();
        let mut callbacks = TestCallbacks::new();

        let event = StreamEvent::TextDelta {
            content: "Hello".to_string(),
        };

        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(should_continue);
        assert_eq!(callbacks.texts, vec!["Hello"]);
    }

    #[test]
    fn test_thinking_event() {
        let mut processor = StreamEventProcessor::new();
        let mut callbacks = TestCallbacks::new();

        let event = StreamEvent::ThinkingDelta {
            content: "Reasoning...".to_string(),
        };

        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(should_continue);
        assert_eq!(callbacks.thinkings, vec!["Reasoning..."]);
    }

    #[test]
    fn test_tool_accumulation() {
        let mut processor = StreamEventProcessor::new();
        let mut callbacks = TestCallbacks::new();

        // Start tool
        let start = StreamEvent::ToolCallStarted {
            id: "call_1".to_string(),
            name: "Read".to_string(),
        };
        processor.process_event(start, &mut callbacks).unwrap();

        // Delta 1
        let delta1 = StreamEvent::ToolInputDelta {
            id: "call_1".to_string(),
            chunk: r#"{"path":""#.to_string(),
        };
        processor.process_event(delta1, &mut callbacks).unwrap();

        // Delta 2
        let delta2 = StreamEvent::ToolInputDelta {
            id: "call_1".to_string(),
            chunk: r#"/tmp/test.txt"}"#.to_string(),
        };
        processor.process_event(delta2, &mut callbacks).unwrap();

        // TurnCompleted flushes the tool
        let turn_end = StreamEvent::TurnCompleted {
            stop_reason: "tool_use".to_string(),
        };
        let should_continue = processor.process_event(turn_end, &mut callbacks).unwrap();
        assert!(!should_continue);

        assert_eq!(callbacks.tools_completed.len(), 1);
        assert_eq!(callbacks.tools_completed[0].0, "call_1");
        assert_eq!(callbacks.tools_completed[0].1, "Read");
    }

    #[test]
    fn test_turn_completed() {
        let mut processor = StreamEventProcessor::new();
        let mut callbacks = TestCallbacks::new();

        let event = StreamEvent::TurnCompleted {
            stop_reason: "end_turn".to_string(),
        };
        let should_continue = processor.process_event(event, &mut callbacks).unwrap();
        assert!(!should_continue);
        assert_eq!(callbacks.turn_completed, vec!["end_turn"]);
    }

    #[test]
    fn test_token_usage() {
        let mut processor = StreamEventProcessor::new();
        let mut callbacks = TestCallbacks::new();

        let event = StreamEvent::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        processor.process_event(event, &mut callbacks).unwrap();

        assert_eq!(callbacks.token_usages, vec![(100, 50)]);
    }
}
