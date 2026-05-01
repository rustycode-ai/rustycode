use crate::{FrontendMessageKind, FrontendSession};
use rustycode_protocol::StreamEvent;

#[allow(clippy::match_same_arms)]
pub fn apply_event(session: &mut FrontendSession, event: &StreamEvent) {
    match event {
        StreamEvent::TextDelta { content } => {
            session.append_assistant_chunk(content);
        }

        StreamEvent::ThinkingDelta { content } => {
            session.append_assistant_chunk(content);
        }

        StreamEvent::ToolCallStarted { name, .. } => {
            session.tool_iteration_count = session.tool_iteration_count.saturating_add(1);
            session.append_assistant_chunk(&format!("\n[tool: {name}]\n"));
        }

        StreamEvent::ToolInputDelta { .. } => {}

        StreamEvent::ToolExecStarted { .. } => {}

        StreamEvent::ToolExecCompleted {
            name,
            output,
            is_error,
            ..
        } => {
            let msg = if *is_error {
                format!("[tool error: {name}] {output}")
            } else {
                format!("[tool: {name}] {output}")
            };
            let kind = if *is_error {
                FrontendMessageKind::Error
            } else {
                FrontendMessageKind::Tool
            };
            session.add_message(msg, kind);
        }

        StreamEvent::TurnStarted { .. } => {}

        StreamEvent::TokenUsage { .. } => {}

        StreamEvent::TurnCompleted { stop_reason } => {
            if stop_reason == "end_turn" {
                let content = session.current_response.clone();
                if !content.is_empty() {
                    session.finish_assistant_message(content);
                }
            }
        }

        StreamEvent::CacheUsage { .. } => {}

        StreamEvent::Done => {
            if !session.current_response.is_empty() {
                let content = session.current_response.clone();
                session.finish_assistant_message(content);
            }
            session.pending_request = false;
        }

        // Forward-compatible: ignore unknown variants from future protocol versions
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn start_response(session: &mut FrontendSession) {
        session.pending_request = false;
        session.current_response.clear();
        session.add_message("...", FrontendMessageKind::Assistant);
    }

    #[test]
    fn text_delta_appends_to_response() {
        let mut session = FrontendSession::default();
        start_response(&mut session);

        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "hel".to_string(),
        });
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "lo".to_string(),
        });

        assert_eq!(session.current_response, "hello");
        assert_eq!(session.messages.last().unwrap().content, "hello");
    }

    #[test]
    fn thinking_delta_appends() {
        let mut session = FrontendSession::default();
        start_response(&mut session);

        apply_event(&mut session, &StreamEvent::ThinkingDelta {
            content: "thinking...".to_string(),
        });

        assert_eq!(session.current_response, "thinking...");
    }

    #[test]
    fn tool_call_started_increments_count() {
        let mut session = FrontendSession::default();
        start_response(&mut session);

        apply_event(&mut session, &StreamEvent::ToolCallStarted {
            id: "t1".to_string(),
            name: "bash".to_string(),
        });

        assert_eq!(session.tool_iteration_count, 1);
        assert!(session.current_response.contains("[tool: bash]"));
    }

    #[test]
    fn tool_exec_completed_adds_tool_message() {
        let mut session = FrontendSession::default();
        start_response(&mut session);

        apply_event(&mut session, &StreamEvent::ToolExecCompleted {
            id: "t1".to_string(),
            name: "read".to_string(),
            output: "file contents".to_string(),
            is_error: false,
        });

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Tool);
        assert!(session.messages[1].content.contains("read"));
    }

    #[test]
    fn tool_exec_completed_error_adds_error_message() {
        let mut session = FrontendSession::default();
        start_response(&mut session);

        apply_event(&mut session, &StreamEvent::ToolExecCompleted {
            id: "t2".to_string(),
            name: "bash".to_string(),
            output: "command failed".to_string(),
            is_error: true,
        });

        assert_eq!(session.messages[1].kind, FrontendMessageKind::Error);
    }

    #[test]
    fn turn_completed_end_turn_finalizes_response() {
        let mut session = FrontendSession::default();
        start_response(&mut session);
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "answer".to_string(),
        });

        apply_event(&mut session, &StreamEvent::TurnCompleted {
            stop_reason: "end_turn".to_string(),
        });

        assert!(!session.pending_request);
        assert_eq!(session.messages[0].content, "answer");
    }

    #[test]
    fn turn_completed_tool_use_does_not_finalize() {
        let mut session = FrontendSession::default();
        start_response(&mut session);
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "partial".to_string(),
        });

        apply_event(&mut session, &StreamEvent::TurnCompleted {
            stop_reason: "tool_use".to_string(),
        });

        assert_eq!(session.current_response, "partial");
    }

    #[test]
    fn done_finalizes_and_clears_pending() {
        let mut session = FrontendSession::default();
        session.pending_request = true;
        start_response(&mut session);
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "final".to_string(),
        });

        apply_event(&mut session, &StreamEvent::Done);

        assert!(!session.pending_request);
        assert_eq!(session.messages[0].content, "final");
    }

    #[test]
    fn full_streaming_lifecycle() {
        let mut session = FrontendSession::default();

        // User sends message
        session.input = "what is 2+2?".to_string();
        let _ = session.submit_input();
        session.add_message("what is 2+2?", FrontendMessageKind::User);

        // Start assistant response
        start_response(&mut session);

        // Stream text
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "The ".to_string(),
        });
        apply_event(&mut session, &StreamEvent::TextDelta {
            content: "answer is 4.".to_string(),
        });

        // Turn completes
        apply_event(&mut session, &StreamEvent::TurnCompleted {
            stop_reason: "end_turn".to_string(),
        });

        // Done
        apply_event(&mut session, &StreamEvent::Done);

        assert_eq!(session.messages.len(), 2);
        assert_eq!(session.messages[0].kind, FrontendMessageKind::User);
        assert_eq!(session.messages[1].kind, FrontendMessageKind::Assistant);
        assert_eq!(session.messages[1].content, "The answer is 4.");
        assert!(!session.pending_request);
    }

    #[test]
    fn tool_input_delta_is_noop() {
        let mut session = FrontendSession::default();
        let before = session.clone();
        apply_event(&mut session, &StreamEvent::ToolInputDelta {
            id: "t1".to_string(),
            chunk: "data".to_string(),
        });
        assert_eq!(session.messages, before.messages);
        assert_eq!(session.current_response, before.current_response);
    }

    #[test]
    fn tool_exec_started_is_noop() {
        let mut session = FrontendSession::default();
        let before = session.clone();
        apply_event(&mut session, &StreamEvent::ToolExecStarted {
            id: "t1".to_string(),
            name: "bash".to_string(),
        });
        assert_eq!(session.messages, before.messages);
    }

    #[test]
    fn turn_started_is_noop() {
        let mut session = FrontendSession::default();
        let before = session.clone();
        apply_event(&mut session, &StreamEvent::TurnStarted { turn: 1 });
        assert_eq!(session.messages, before.messages);
    }

    #[test]
    fn token_usage_is_noop() {
        let mut session = FrontendSession::default();
        let before = session.clone();
        apply_event(&mut session, &StreamEvent::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        });
        assert_eq!(session.messages, before.messages);
    }

    #[test]
    fn cache_usage_is_noop() {
        let mut session = FrontendSession::default();
        let before = session.clone();
        apply_event(&mut session, &StreamEvent::CacheUsage {
            cache_read_tokens: 10,
            cache_creation_tokens: 5,
        });
        assert_eq!(session.messages, before.messages);
    }
}
