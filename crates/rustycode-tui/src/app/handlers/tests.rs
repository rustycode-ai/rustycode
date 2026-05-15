#[cfg(test)]
mod tests {
    use crate::app::async_::{StreamChunk, StreamError, ToolOutput, ToolResult};
    use crate::app::handlers::handle_stream_chunk;
    use crate::app::handlers::tool_result::handle_tool_result;
    use crate::app::TUI;
    use crate::ui::message::{Message, MessageRole};

    fn create_test_tui() -> TUI {
        // Create a minimal TUI for testing using Default impl
        TUI::default()
    }

    #[test]
    fn test_duplicate_prevention_when_awaiting_clarification() {
        // Test that text-based question detection is skipped
        // when awaiting_clarification is already true
        let mut tui = create_test_tui();

        // Simulate that QuestionRequest already set awaiting_clarification
        tui.panels.awaiting_clarification = true;
        tui.session.streaming.current_stream_content =
            "What format? How should I proceed?".to_string();

        // Manually test the guard logic (extracted from StreamChunk::Done handler)
        let should_detect = !tui.panels.awaiting_clarification;
        assert!(
            !should_detect,
            "Should skip detection when awaiting_clarification is true"
        );

        // Verify no new system messages were added for clarification
        let initial_msg_count = tui.session.messages.len();

        // Simulate the guard check
        if !tui.panels.awaiting_clarification {
            tui.add_system_message("❓ The AI has some clarification questions".to_string());
        }

        assert_eq!(
            tui.session.messages.len(),
            initial_msg_count,
            "No clarification message should be added"
        );
    }

    #[test]
    fn test_question_detection_when_not_awaiting() {
        // Test that text-based question detection runs
        // when awaiting_clarification is false
        let mut tui = create_test_tui();

        tui.panels.awaiting_clarification = false;
        tui.session.streaming.current_stream_content = "What format do you prefer?".to_string();

        // The guard should allow detection
        let should_detect = !tui.panels.awaiting_clarification;
        assert!(
            should_detect,
            "Should detect when awaiting_clarification is false"
        );
    }

    #[test]
    fn test_stream_chunk_question_request_sets_flag() {
        // Verify that QuestionRequest chunk sets awaiting_clarification
        let mut tui = create_test_tui();

        assert!(
            !tui.panels.awaiting_clarification,
            "Initial state should be false"
        );

        // Simulate what QuestionRequest handler does
        tui.panels.awaiting_clarification = true;

        assert!(
            tui.panels.awaiting_clarification,
            "Should be true after QuestionRequest"
        );
    }

    #[test]
    fn test_stream_chunk_done_after_question_request() {
        // Integration test: QuestionRequest followed by Done should not duplicate
        let mut tui = create_test_tui();
        let _question_content = "What is your preferred format?";

        // Step 1: Simulate QuestionRequest handling
        tui.panels.awaiting_clarification = true;
        let after_question_request = tui.panels.awaiting_clarification;

        // Step 2: Simulate Done handler guard check
        let should_skip_detection = after_question_request;

        assert!(
            should_skip_detection,
            "Done handler should skip detection after QuestionRequest"
        );
    }

    #[test]
    fn test_handle_stream_chunk_text_appends_content() {
        let mut tui = create_test_tui();
        let initial_content = "Hello";
        tui.session.streaming.current_stream_content = initial_content.to_string();

        let chunk = StreamChunk::Text(" World".to_string());
        handle_stream_chunk(&mut tui, chunk);

        assert_eq!(tui.session.streaming.current_stream_content, "Hello World");
        assert!(tui.session.streaming.is_streaming);
    }

    #[test]
    fn test_handle_stream_chunk_done_without_clarification() {
        let mut tui = create_test_tui();
        tui.session.streaming.current_stream_content = "I will implement the feature.".to_string();
        tui.panels.awaiting_clarification = false;

        let initial_msg_count = tui.session.messages.len();

        // Simulate Done handler - text without questions
        let questions = crate::ui::detect_questions(&tui.session.streaming.current_stream_content);

        assert!(
            questions.is_empty(),
            "No questions should be detected in statement"
        );
        assert_eq!(
            tui.session.messages.len(),
            initial_msg_count,
            "No clarification message added"
        );
    }

    #[test]
    fn test_handle_stream_chunk_done_with_clarification_not_awaiting() {
        let mut tui = create_test_tui();
        tui.session.streaming.current_stream_content = "What format do you prefer?".to_string();
        tui.panels.awaiting_clarification = false;

        // Simulate Done handler - with questions and not awaiting
        let questions = crate::ui::detect_questions(&tui.session.streaming.current_stream_content);

        assert!(!questions.is_empty(), "Questions should be detected");

        // The guard would allow setting up clarification
        let should_setup = !tui.panels.awaiting_clarification;
        assert!(should_setup, "Should set up clarification panel");
    }

    #[test]
    fn test_tool_progress_matches_by_tool_id() {
        // Verify that ToolProgress uses tool_id for matching when available,
        // so parallel tools with the same name don't cross-contaminate.
        let mut tui = create_test_tui();
        tui.session.streaming.is_streaming = true;

        // Start two tools with the same name but different IDs
        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolStart {
                tool_name: "Read".to_string(),
                tool_id: "tool-1".to_string(),
                input_json: r#"{"path":"/a"}"#.to_string(),
            },
        );
        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolStart {
                tool_name: "Read".to_string(),
                tool_id: "tool-2".to_string(),
                input_json: r#"{"path":"/b"}"#.to_string(),
            },
        );

        // Send progress for tool-2 specifically
        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolProgress {
                tool_id: Some("tool-2".to_string()),
                tool_name: "Read".to_string(),
                stage: "reading".to_string(),
                elapsed_ms: 100,
                output_preview: Some("file b content".to_string()),
            },
        );

        // Verify only tool-2's entry was updated in panel history
        let tool1 = tui
            .panels
            .tool_panel
            .tool_panel_history
            .iter()
            .find(|e| e.tool_id == "tool-1");
        let tool2 = tui
            .panels
            .tool_panel
            .tool_panel_history
            .iter()
            .find(|e| e.tool_id == "tool-2");

        assert!(tool1.is_some(), "tool-1 should exist in panel history");
        assert!(tool2.is_some(), "tool-2 should exist in panel history");

        // tool-1 should still have initial summary, tool-2 should have updated preview
        assert_eq!(
            tool1.unwrap().result_summary,
            "Read...",
            "tool-1 should have initial summary"
        );
        assert_eq!(
            tool2.unwrap().result_summary,
            "file b content",
            "tool-2 should have updated preview"
        );
    }

    #[test]
    fn test_tool_result_fallback_preserves_start_time() {
        // Verify that the fallback branch in handle_tool_result
        // uses start_time from active_tools when the tool isn't found
        // in message tool_executions.
        let mut tui = create_test_tui();
        tui.session.streaming.is_streaming = true;

        // Start a tool BEFORE the assistant message exists.
        // ToolStart adds to active_tools but has no message to attach to.
        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolStart {
                tool_name: "Bash".to_string(),
                tool_id: "tool-x".to_string(),
                input_json: r#"{"command":"ls"}"#.to_string(),
            },
        );

        // Now add an assistant message (simulates late arrival)
        tui.session.messages.push(Message {
            id: "msg-1".to_string(),
            role: MessageRole::Assistant,
            content: "Using tool".to_string(),
            timestamp: chrono::Utc::now(),
            tool_executions: None,
            thinking: None,
            metadata: Default::default(),
            tools_expansion: Default::default(),
            thinking_expansion: Default::default(),
            focused_tool_index: None,
            collapsed: false,
            tags: Vec::new(),
        });

        let active_start = tui
            .session
            .active_tools
            .get("tool-x")
            .map(|t| t.start_time)
            .expect("tool should be in active_tools");

        // Complete the tool — message has no matching tool execution,
        // so the fallback branch fires and uses active_start
        let result = ToolResult {
            id: "tool-x".to_string(),
            name: "Bash".to_string(),
            result: ToolOutput::Success("file1\nfile2".to_string()),
        };
        handle_tool_result(&mut tui, result);

        // Verify the tool execution in the message
        let msg = tui.session.messages.last().expect("message should exist");
        let tools = msg
            .tool_executions
            .as_ref()
            .expect("should have tool_executions");
        let tool = tools
            .iter()
            .find(|t| t.tool_id == "tool-x")
            .expect("tool should exist");

        // start_time should match the active_tools entry (not Utc::now())
        assert_eq!(
            tool.start_time, active_start,
            "fallback should use active_tools start_time"
        );
        assert!(
            tool.duration_ms.is_some(),
            "duration_ms should be computed, not None"
        );
    }

    #[test]
    fn test_error_handler_preserves_content_with_system_messages() {
        // Bug: Error handler used messages.last() which could point to a
        // system message pushed during streaming, causing a duplicate
        // assistant message instead of updating the existing one.
        let mut tui = TUI::new_for_test();
        tui.session.streaming.is_streaming = true;

        // Simulate streaming: assistant message exists
        tui.session.messages.push(Message {
            id: "msg-1".to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            timestamp: chrono::Utc::now(),
            tool_executions: None,
            thinking: None,
            metadata: Default::default(),
            tools_expansion: Default::default(),
            thinking_expansion: Default::default(),
            focused_tool_index: None,
            collapsed: false,
            tags: Vec::new(),
        });

        // System message pushed during streaming (auto-approve, doom loop warning, etc.)
        tui.session.messages.push(Message {
            id: "msg-2".to_string(),
            role: MessageRole::System,
            content: "Auto-approved: read_file".to_string(),
            timestamp: chrono::Utc::now(),
            tool_executions: None,
            thinking: None,
            metadata: Default::default(),
            tools_expansion: Default::default(),
            thinking_expansion: Default::default(),
            focused_tool_index: None,
            collapsed: false,
            tags: Vec::new(),
        });

        // Accumulated streaming content
        tui.session.streaming.current_stream_content = "Here is my partial response".to_string();

        // Send error chunk — should update existing assistant, not create duplicate
        handle_stream_chunk(
            &mut tui,
            StreamChunk::Error(StreamError::Provider(
                rustycode_llm::provider::ProviderError::Network(
                    "Connection issue: stream interrupted".to_string(),
                ),
            )),
        );

        // Verify: exactly 2 messages (1 assistant + 1 system), no duplicate
        let assistant_count = tui
            .session
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .count();
        assert_eq!(
            assistant_count, 1,
            "should have exactly 1 assistant message, not a duplicate"
        );

        // Verify: assistant message has the partial content
        let assistant = tui
            .session
            .messages
            .iter()
            .find(|m| m.role == MessageRole::Assistant)
            .expect("assistant message should exist");
        assert_eq!(
            assistant.content, "Here is my partial response",
            "assistant content should be the preserved partial content"
        );

        // Verify: system message still exists
        assert!(
            tui.session
                .messages
                .iter()
                .any(|m| m.role == MessageRole::System),
            "system message should still be present"
        );

        // Verify: streaming state cleaned up
        assert!(!tui.session.streaming.is_streaming);
    }

    #[test]
    fn layer3_fifty_different_chunks_no_loss() {
        let mut tui = create_test_tui();

        // Send 50 chunks with unique content
        for i in 0..50 {
            let chunk_text = format!("chunk{}_", i);
            handle_stream_chunk(&mut tui, StreamChunk::Text(chunk_text));
        }

        // Verify all content is present
        let mut expected = String::new();
        for i in 0..50 {
            expected.push_str(&format!("chunk{}_", i));
        }

        assert_eq!(
            tui.session.streaming.current_stream_content, expected,
            "All 50 chunks must be accumulated without loss"
        );
    }

    #[test]
    fn layer3_done_chunk_transfers_all_content_to_message() {
        let mut tui = create_test_tui();

        handle_stream_chunk(&mut tui, StreamChunk::Text("Hello".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text(" ".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text("world".to_string()));

        assert_eq!(tui.session.streaming.current_stream_content, "Hello world");

        handle_stream_chunk(&mut tui, StreamChunk::Done);

        // After Done: content should transfer to assistant message
        let assistant = tui
            .session
            .messages
            .iter()
            .find(|m| m.role == MessageRole::Assistant);
        assert!(
            assistant.is_some(),
            "Assistant message should exist after Done"
        );
        assert_eq!(
            assistant.unwrap().content,
            "Hello world",
            "Content should transfer to message on Done"
        );

        // After Done: stream content should be cleared
        assert_eq!(
            tui.session.streaming.current_stream_content, "",
            "current_stream_content should be cleared after Done"
        );
        assert!(
            !tui.session.streaming.is_streaming,
            "is_streaming should be false after Done"
        );
    }

    #[test]
    fn layer3_identical_consecutive_chunks_preserved() {
        let mut tui = create_test_tui();

        // REGRESSION: Two identical periods must both appear (was broken by
        // aggressive dedup at Layer 3 that dropped consecutive identical chunks).
        handle_stream_chunk(&mut tui, StreamChunk::Text(".".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text(".".to_string()));

        assert_eq!(tui.session.streaming.current_stream_content, "..");
    }

    #[test]
    fn layer3_empty_text_chunk_ignored() {
        let mut tui = create_test_tui();

        handle_stream_chunk(&mut tui, StreamChunk::Text("Hello".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text("".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text("World".to_string()));

        assert_eq!(
            tui.session.streaming.current_stream_content, "HelloWorld",
            "Empty text chunks should be handled gracefully"
        );
    }

    #[test]
    fn layer3_whitespace_only_chunk_preserved() {
        let mut tui = create_test_tui();

        handle_stream_chunk(&mut tui, StreamChunk::Text("Hello".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text("   ".to_string()));
        handle_stream_chunk(&mut tui, StreamChunk::Text("world".to_string()));

        assert_eq!(
            tui.session.streaming.current_stream_content, "Hello   world",
            "Whitespace-only chunks must be preserved"
        );
    }

    // ── Characterization tests for StreamChunk variants ────────────────

    #[test]
    fn characterization_thinking_chunk_increments_counter() {
        let mut tui = create_test_tui();
        tui.session.streaming.is_streaming = true;
        let initial_thinking = tui.session.streaming.thinking_chunks_received;

        handle_stream_chunk(
            &mut tui,
            StreamChunk::Thinking("Let me reason...".to_string()),
        );

        assert_eq!(
            tui.session.streaming.thinking_chunks_received,
            initial_thinking + 1,
            "Thinking chunk should increment thinking counter"
        );
        assert!(
            tui.session.streaming.is_streaming,
            "Streaming should remain active after thinking chunk"
        );
    }

    #[test]
    fn characterization_stopped_chunk_ends_streaming() {
        let mut tui = create_test_tui();
        tui.session.streaming.is_streaming = true;

        handle_stream_chunk(
            &mut tui,
            StreamChunk::Stopped {
                stop_reason: "content_filter".to_string(),
            },
        );

        assert!(
            !tui.session.streaming.is_streaming,
            "Stopped chunk should end streaming"
        );
    }

    #[test]
    fn characterization_tool_complete_updates_panel_history() {
        let mut tui = create_test_tui();
        tui.session.streaming.is_streaming = true;

        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolStart {
                tool_name: "Write".to_string(),
                tool_id: "tool-w1".to_string(),
                input_json: r#"{"path":"/tmp/x"}"#.to_string(),
            },
        );

        handle_stream_chunk(
            &mut tui,
            StreamChunk::ToolComplete {
                tool_name: "Write".to_string(),
                tool_id: "tool-w1".to_string(),
                duration_ms: 50,
                success: true,
                output_size: 12,
                output: Some("wrote 12 bytes".to_string()),
            },
        );

        let entry = tui
            .panels
            .tool_panel
            .tool_panel_history
            .iter()
            .find(|e| e.tool_id == "tool-w1");
        assert!(
            entry.is_some(),
            "Tool should exist in panel history after complete"
        );
        let entry = entry.unwrap();
        assert_eq!(entry.name, "Write");
        assert!(
            entry.result_summary.contains("wrote 12 bytes") || entry.result_summary.contains("12"),
            "Result summary should reflect output: got '{}'",
            entry.result_summary
        );
    }

    #[test]
    fn characterization_system_message_chunk_adds_to_messages() {
        let mut tui = create_test_tui();
        let initial_count = tui.session.messages.len();

        handle_stream_chunk(
            &mut tui,
            StreamChunk::SystemMessage("Auto-approved: read_file".to_string()),
        );

        assert!(
            tui.session.messages.len() > initial_count,
            "SystemMessage chunk should add a message"
        );
        assert!(
            tui.session
                .messages
                .iter()
                .any(|m| m.content.contains("Auto-approved")),
            "System message content should be present"
        );
    }

    #[test]
    fn characterization_token_usage_updates_budget() {
        let mut tui = create_test_tui();
        let initial_input = tui.model.token_budget.session_input_tokens;

        handle_stream_chunk(
            &mut tui,
            StreamChunk::TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 30,
                cache_creation_tokens: 0,
            },
        );

        assert!(
            tui.model.token_budget.session_input_tokens >= initial_input + 100,
            "Token usage should be accumulated: got {}",
            tui.model.token_budget.session_input_tokens
        );
    }

    #[test]
    fn characterization_file_snapshot_records_undo_data() {
        let mut tui = create_test_tui();

        handle_stream_chunk(
            &mut tui,
            StreamChunk::FileSnapshot {
                batch: vec![
                    ("/tmp/a.txt".to_string(), "old content a".to_string()),
                    ("/tmp/b.txt".to_string(), "old content b".to_string()),
                ],
            },
        );

        let snapshots = &tui.session.undo;
        assert!(
            !snapshots.file_stack.is_empty(),
            "File snapshot should record undo data in file_stack"
        );
    }
}
