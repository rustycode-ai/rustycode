//! Integration tests for message hierarchy system
//!
//! This test file verifies the complete message hierarchy implementation:
//! - Tool executions are children of AI messages
//! - Three-level expansion: summary → tool list → detailed output
//! - Clean conversation flow without tool noise

#[cfg(test)]
mod integration_tests {
    use super::super::*;
    use crate::ui::message::*;

    #[test]
    fn test_clean_conversation_flow() {
        // Simulate a clean conversation: user → AI → user → AI
        let mut messages = vec![];

        // User message
        messages.push(Message::user("Help me implement a BST".to_string()));

        // AI response with tools
        let tool1 = ToolExecution::new("read_file".to_string(), "read_file: src/main.rs (23b)".to_string());
        let mut tool2 = ToolExecution::new("write_file".to_string(), "write_file: src/tree.rs (122b)".to_string());
        tool2.complete(Some("File written successfully".to_string()));
        let mut tool3 = ToolExecution::new("bash".to_string(), "bash: cargo check".to_string());
        tool3.complete(Some("Compiling... Done".to_string()));

        let ai_msg = Message::assistant("I'll help you implement a binary search tree. Let me create the file.".to_string())
            .with_tools(vec![tool1, tool2, tool3]);

        messages.push(ai_msg);

        // Verify clean conversation flow
        assert_eq!(messages.len(), 2); // Only 2 messages in main flow
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert!(messages[1].has_tools());
        assert_eq!(messages[1].tool_count(), 3);

        // Tools are metadata, not separate messages
        let tools = messages[1].tool_executions.as_ref().unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].status, ToolStatus::Running);
        assert_eq!(tools[1].status, ToolStatus::Complete);
        assert_eq!(tools[2].status, ToolStatus::Complete);
    }

    #[test]
    fn test_expansion_levels() {
        // Create message with tools
        let tool1 = ToolExecution::new("read".to_string(), "read: file.txt (145b)".to_string());
        let tool2 = ToolExecution::new("write".to_string(), "write: file.txt (122b)".to_string());

        let mut msg = Message::assistant("Test".to_string())
            .with_tools(vec![tool1, tool2]);

        // Test collapsed (default)
        assert_eq!(msg.tools_expansion, ExpansionLevel::Collapsed);
        assert_eq!(msg.focused_tool_index, None);

        // Test expanded
        msg.toggle_tools_expansion();
        assert_eq!(msg.tools_expansion, ExpansionLevel::Expanded);
        assert_eq!(msg.focused_tool_index, None);

        // Test deep expansion
        msg.set_deep_tool_expansion(0);
        assert_eq!(msg.tools_expansion, ExpansionLevel::Deep);
        assert_eq!(msg.focused_tool_index, Some(0));

        // Test toggle back
        msg.toggle_tools_expansion();
        assert_eq!(msg.tools_expansion, ExpansionLevel::Collapsed);
        assert_eq!(msg.focused_tool_index, None);
    }

    #[test]
    fn test_tool_lifecycle() {
        let mut tool = ToolExecution::new("bash".to_string(), "bash: cargo test".to_string());

        // Initial state
        assert_eq!(tool.status, ToolStatus::Running);
        assert_eq!(tool.duration_string(), "running");

        // Complete the tool
        tool.complete(Some("All tests passed".to_string()));

        // Check completed state
        assert_eq!(tool.status, ToolStatus::Complete);
        assert!(tool.end_time.is_some());
        assert!(tool.duration_ms.is_some());
        assert!(tool.detailed_output.is_some());
        assert_ne!(tool.duration_string(), "running");

        // Check size summary
        assert_eq!(tool.size_summary(), "15b"); // "All tests passed".len()
    }

    #[test]
    fn test_thinking_display() {
        let mut msg = Message::assistant("Let me think about this...".to_string())
            .with_thinking("I need to consider:\n1. Performance\n2. Maintainability\n3. Best approach: BST".to_string());

        // Check thinking exists
        assert!(msg.has_thinking());
        assert_eq!(msg.thinking_expansion, ExpansionLevel::Collapsed);

        // Toggle expansion
        msg.toggle_thinking_expansion();
        assert_eq!(msg.thinking_expansion, ExpansionLevel::Expanded);

        // Toggle back
        msg.toggle_thinking_expansion();
        assert_eq!(msg.thinking_expansion, ExpansionLevel::Collapsed);
    }

    #[test]
    fn test_message_rendering_theme() {
        let theme = MessageTheme::default();

        // Check theme colors
        assert_eq!(theme.user_color, Color::Cyan);
        assert_eq!(theme.ai_color, Color::Magenta);
        assert_eq!(theme.system_color, Color::Gray);
        assert_eq!(theme.tool_summary_color, Color::Yellow);
        assert_eq!(theme.thinking_color, Color::Blue);
    }

    #[test]
    fn test_pipe_styles() {
        let user_msg = Message::user("Test".to_string());
        let (pipe, color) = user_msg.pipe_style();
        assert_eq!(pipe, '▌');
        assert_eq!(color, Color::Cyan);

        let ai_msg = Message::assistant("Test".to_string());
        let (pipe, color) = ai_msg.pipe_style();
        assert_eq!(pipe, '▐');
        assert_eq!(color, Color::Magenta);

        let sys_msg = Message::system("Test".to_string());
        let (pipe, color) = sys_msg.pipe_style();
        assert_eq!(pipe, '│');
        assert_eq!(color, Color::Gray);
    }

    #[test]
    fn test_tool_status_icons() {
        assert_eq!(ToolStatus::Running.icon(), "⏳");
        assert_eq!(ToolStatus::Complete.icon(), "✅");
        assert_eq!(ToolStatus::Failed.icon(), "❌");

        assert_eq!(ToolStatus::Running.color(), Color::Yellow);
        assert_eq!(ToolStatus::Complete.color(), Color::Green);
        assert_eq!(ToolStatus::Failed.color(), Color::Red);
    }

    #[test]
    fn test_message_metadata() {
        let mut msg = Message::assistant("Test".to_string());

        // Default metadata
        assert!(msg.metadata.model.is_none());
        assert!(msg.metadata.input_tokens.is_none());
        assert!(msg.metadata.output_tokens.is_none());

        // Add metadata
        msg.metadata.model = Some("claude-3-opus".to_string());
        msg.metadata.input_tokens = Some(1000);
        msg.metadata.output_tokens = Some(500);

        assert_eq!(msg.metadata.model.as_ref().unwrap(), "claude-3-opus");
        assert_eq!(msg.metadata.input_tokens.unwrap(), 1000);
        assert_eq!(msg.metadata.output_tokens.unwrap(), 500);
    }

    #[test]
    fn test_tool_failure_handling() {
        let mut tool = ToolExecution::new("bash".to_string(), "bash: cargo build".to_string());

        // Fail the tool
        tool.fail("Compilation failed with errors".to_string());

        assert_eq!(tool.status, ToolStatus::Failed);
        assert!(tool.end_time.is_some());
        assert_eq!(tool.result_summary, "bash: Error");
        assert_eq!(tool.detailed_output.as_ref().unwrap(), "Compilation failed with errors");
    }

    #[test]
    fn test_message_serialization() {
        let tool = ToolExecution::new("read".to_string(), "read: file.txt (145b)".to_string());
        let msg = Message::assistant("Test message".to_string())
            .with_tools(vec![tool]);

        // Test JSON serialization
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Test message"));
        assert!(json.contains("read"));

        // Test deserialization
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.content, "Test message");
        assert!(deserialized.has_tools());
    }

    #[test]
    fn test_role_display_formatting() {
        assert_eq!(format!("{}", MessageRole::User), "you");
        assert_eq!(format!("{}", MessageRole::Assistant), "ai");
        assert_eq!(format!("{}", MessageRole::System), "system");
    }

    #[test]
    fn test_message_with_thinking_and_tools() {
        // Message with both thinking and tools
        let mut tool = ToolExecution::new("write".to_string(), "write: code.rs (100b)".to_string());
        tool.complete(Some("Done".to_string()));

        let msg = Message::assistant("I'll create the file".to_string())
            .with_thinking("Best approach is to use a struct".to_string())
            .with_tools(vec![tool]);

        assert!(msg.has_thinking());
        assert!(msg.has_tools());
        assert_eq!(msg.tool_count(), 1);
        assert_eq!(msg.completed_tool_count(), 1);
    }
}
