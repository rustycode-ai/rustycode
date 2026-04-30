//! Integration tests for the status indicator system
//!
//! This module demonstrates how the status system integrates with the TUI
//! and tests the complete workflow.

#[cfg(test)]
mod integration_tests {

    use crate::ui::animator::Animator;
    use crate::ui::progress::{
        Progress, ProgressRenderer, ToolProgress, ToolStatus as ProgressToolStatus,
    };
    use crate::ui::status::{Status, StatusBar, ToolExecutions};

    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_complete_tool_execution_workflow() {
        // Create a tool executions tracker
        let mut tools = ToolExecutions::new();

        // Simulate tool execution lifecycle
        let mut tool = ToolProgress::new("read_file");
        tool.start(); // Mark as running
        tools.add(tool);
        tools.update_progress("read_file", 5, 10);

        // Verify it's running
        assert_eq!(tools.running_count(), 1);
        let tool = tools.current_tool().unwrap();
        assert_eq!(tool.status, ProgressToolStatus::Running);

        // Complete it
        tools.complete("read_file", "File contents".to_string());
        assert_eq!(tools.running_count(), 0);
        assert_eq!(tools.tools[0].status, ProgressToolStatus::Complete);
    }

    #[test]
    fn test_status_indicator_workflow() {
        // Test status progression
        let statuses = vec![
            Status::Ready,
            Status::Thinking { chunks_received: 0 },
            Status::Thinking { chunks_received: 5 },
            Status::ExecutingTools {
                remaining_tools: 1,
                total_tools: 3,
                current_tool: Some("bash".to_string()),
                progress_percentage: Some(33),
            },
            Status::Ready,
        ];

        for status in statuses {
            let indicator = status.indicator();
            let accessible = status.accessible_text();

            // Verify indicator has all required fields
            assert!(!indicator.icon.is_empty());
            assert!(!indicator.text.is_empty());
            assert!(!accessible.is_empty());

            // Verify color is set
            assert_ne!(indicator.color, ratatui::style::Color::Reset);
        }
    }

    #[test]
    fn test_animation_workflow() {
        let mut animator = Animator::new(10, false); // 10 FPS for faster testing

        // Test that animation updates
        let frame1 = animator.current_frame();
        thread::sleep(Duration::from_millis(110)); // Wait for >100ms
        animator.update();
        let frame2 = animator.current_frame();

        // Frames should be different after enough time has passed
        assert_ne!(frame1.cursor, frame2.cursor);
    }

    #[test]
    fn test_progress_rendering() {
        let renderer = ProgressRenderer::new(10);
        let mut tool = ToolProgress::new("test_tool");
        tool.start();
        tool.update_progress(5, 10);

        let rendered = renderer.render_with_timing(&tool);

        // Verify rendered output contains key information
        assert!(rendered.contains("test_tool"));
        assert!(rendered.contains("⏳")); // Running icon
        assert!(rendered.contains("%")); // Percentage
    }

    #[test]
    fn test_multiple_concurrent_tools() {
        let mut tools = ToolExecutions::new();

        // Add multiple tools
        for i in 1..=3 {
            let mut tool = ToolProgress::new(format!("tool_{}", i));
            tool.start();
            tools.add(tool);
        }

        assert_eq!(tools.running_count(), 3);
        assert_eq!(tools.tools.len(), 3);

        // Complete first tool
        tools.complete("tool_1", "Done".to_string());
        assert_eq!(tools.running_count(), 2);

        // Fail second tool
        tools.fail("tool_2", "Error".to_string());
        assert_eq!(tools.running_count(), 1);
    }

    #[test]
    fn test_status_bar_configuration() {
        // Test default config
        let bar_default = StatusBar::default();
        assert!(bar_default.config().animations_enabled);
        assert!(!bar_default.config().reduced_motion);

        // Test reduced motion config
        let bar_reduced = StatusBar::reduced_motion();
        assert!(!bar_reduced.config().animations_enabled);
        assert!(bar_reduced.config().reduced_motion);
    }

    #[test]
    fn test_eta_calculation() {
        let mut tool = ToolProgress::new("test");
        tool.start();
        tool.update_progress(2, 10);

        // Simulate some work
        thread::sleep(Duration::from_millis(50));

        tool.update_progress(4, 10);

        // Should have an ETA now
        let eta = tool.estimate_eta();
        assert!(eta.is_some());

        let formatted_eta = tool.format_eta();
        assert!(formatted_eta.is_some());
        assert!(formatted_eta.unwrap().contains("ETA"));
    }

    #[test]
    fn test_accessibility_features() {
        // Test reduced motion mode
        let animator = Animator::reduced_motion();
        assert!(animator.is_reduced_motion());

        let frame = animator.current_frame();
        assert_eq!(frame.cursor, '⏳');
        assert!(!frame.is_active);

        // Test accessible text for all statuses
        let statuses = vec![
            Status::Ready,
            Status::Thinking { chunks_received: 1 },
            Status::ExecutingTools {
                remaining_tools: 2,
                total_tools: 5,
                current_tool: Some("test".to_string()),
                progress_percentage: Some(60),
            },
            Status::Error {
                message: "Test error".to_string(),
                suggestions: vec!["Fix it".to_string()],
            },
        ];

        for status in statuses {
            let text = status.accessible_text();
            assert!(!text.is_empty());
            assert!(text.len() < 200); // Should be concise for screen readers
        }
    }

    #[test]
    fn test_tool_executions_trim() {
        let mut tools = ToolExecutions::new();

        // Add many completed tools
        for i in 0..10 {
            let mut tool = ToolProgress::new(format!("tool{}", i));
            tool.complete(format!("Result {}", i));
            tools.add(tool);
        }

        assert_eq!(tools.tools.len(), 10);

        // Trim to keep last 3
        tools.trim_completed(3);
        assert_eq!(tools.tools.len(), 3);
        assert_eq!(tools.tools[0].name, "tool7");
    }

    #[test]
    fn test_progress_percentage() {
        let progress = Progress::new(0, 100, "Test");
        assert_eq!(progress.percentage(), 0);

        let mut progress = progress;
        progress.update(50);
        assert_eq!(progress.percentage(), 50);

        progress.update(100);
        assert_eq!(progress.percentage(), 100);
        assert!(progress.is_complete());
    }

    #[test]
    fn test_elapsed_time_formatting() {
        let tool = ToolProgress::new("test");
        thread::sleep(Duration::from_millis(10));

        let elapsed = tool.format_elapsed();
        assert!(elapsed.contains("ms") || elapsed.contains("s"));
    }

    #[test]
    fn test_status_color_consistency() {
        let status_color_map = vec![
            (Status::Ready, ratatui::style::Color::Green),
            (
                Status::Thinking { chunks_received: 0 },
                ratatui::style::Color::Cyan,
            ),
            (
                Status::ExecutingTools {
                    remaining_tools: 1,
                    total_tools: 3,
                    current_tool: None,
                    progress_percentage: None,
                },
                ratatui::style::Color::Magenta,
            ),
            (
                Status::Error {
                    message: "Test".to_string(),
                    suggestions: vec![],
                },
                ratatui::style::Color::Red,
            ),
        ];

        for (status, expected_color) in status_color_map {
            assert_eq!(status.color(), expected_color);
        }
    }
}
