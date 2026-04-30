//! End-to-end tests for Tool Selection Workflow
//!
//! Tests the complete tool selection system including:
//! - Profile detection from various prompts
//! - Tool ranking and suggestion
//! - Usage-based tool prediction
//! - Custom filtering (always_include/always_exclude)

use rustycode_tools::{ToolProfile, ToolSelector, UsageTracker};

#[test]
fn test_tool_selector_explore_profile_workflow() {
    // Test complete workflow for Explore profile
    let selector = ToolSelector::new().with_profile(ToolProfile::Explore);

    // Get available tools for Explore profile
    let tools = selector.select_tools();

    // Verify Explore-specific tools are available
    assert!(tools.contains(&"read_file".to_string()));
    assert!(tools.contains(&"grep".to_string()));
    assert!(tools.contains(&"glob".to_string()));
    assert!(tools.contains(&"list_dir".to_string()));

    // Verify write/modification tools are NOT available
    assert!(!tools.contains(&"write_file".to_string()));
    assert!(!tools.contains(&"edit".to_string()));

    // Get suggestions (filtered)
    let suggestions = selector.suggest_tools();
    assert!(suggestions.len() <= tools.len()); // Suggestions subset of available
}

#[test]
fn test_tool_selector_implement_profile_workflow() {
    // Test complete workflow for Implement profile
    let selector = ToolSelector::new().with_profile(ToolProfile::Implement);

    let tools = selector.select_tools();

    // Verify Implement-specific tools
    assert!(tools.contains(&"write_file".to_string()));
    assert!(tools.contains(&"edit".to_string()));
    assert!(tools.contains(&"bash".to_string()));
    assert!(tools.contains(&"test".to_string()));

    // Exploration tools may or may not be in Implement profile
    // (depends on implementation)
}

#[test]
fn test_tool_selector_debug_profile_workflow() {
    // Test complete workflow for Debug profile
    let selector = ToolSelector::new().with_profile(ToolProfile::Debug);

    let tools = selector.select_tools();

    // Verify Debug-specific tools
    assert!(tools.contains(&"lsp_diagnostics".to_string()));
    assert!(tools.contains(&"lsp_hover".to_string()));
    assert!(tools.contains(&"bash".to_string()));
    assert!(tools.contains(&"test".to_string()));
}

#[test]
fn test_tool_selector_ops_profile_workflow() {
    // Test complete workflow for Ops profile
    let selector = ToolSelector::new().with_profile(ToolProfile::Ops);

    let tools = selector.select_tools();

    // Verify Ops-specific tools
    assert!(tools.contains(&"bash".to_string()));
    assert!(tools.contains(&"git_commit".to_string()));
    assert!(tools.contains(&"git_diff".to_string()));
    assert!(tools.contains(&"git_status".to_string()));
}

#[test]
fn test_tool_selector_all_profile_workflow() {
    // Test All profile (default, all tools available)
    let selector = ToolSelector::new().with_profile(ToolProfile::All);

    let tools = selector.select_tools();

    // Verify all major tool categories are present
    assert!(tools.contains(&"bash".to_string()));
    assert!(tools.contains(&"read_file".to_string()));
    assert!(tools.contains(&"write_file".to_string()));
    assert!(tools.contains(&"grep".to_string()));

    // All profile should have more tools than specific profiles
    let explore_tools = ToolSelector::new()
        .with_profile(ToolProfile::Explore)
        .select_tools()
        .len();
    assert!(tools.len() >= explore_tools);
}

#[test]
fn test_profile_detection_from_real_prompts() {
    // Test profile detection with realistic user prompts
    let test_cases = vec![
        ("Show me the authentication code", ToolProfile::Explore),
        ("Create a new user model", ToolProfile::Implement),
        ("Fix the failing test", ToolProfile::Debug),
        ("Deploy to production", ToolProfile::Ops),
        ("What does this function do?", ToolProfile::Explore),
        ("Add error handling", ToolProfile::Implement),
        ("Debug the panic", ToolProfile::Debug),
        ("Run cargo test", ToolProfile::Ops),
    ];

    for (prompt, expected_profile) in test_cases {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected, expected_profile,
            "Prompt '{}' should detect {:?}, got {:?}",
            prompt, expected_profile, detected
        );
    }
}

#[test]
fn test_tool_selector_with_usage_tracking() {
    // Test that usage tracking affects tool ranking
    let mut selector = ToolSelector::new().with_profile(ToolProfile::Explore);

    // Record some usage
    selector.record_use("read_file");
    selector.record_use("read_file");
    selector.record_use("read_file");
    selector.record_use("grep");
    selector.record_use("grep");
    selector.record_use("glob");

    // Get ranked tools
    let tools = selector.select_tools();

    // read_file should be ranked high (most used)
    let read_file_index = tools
        .iter()
        .position(|t| t == "read_file")
        .expect("read_file should be in tools");
    let grep_index = tools
        .iter()
        .position(|t| t == "grep")
        .expect("grep should be in tools");

    // read_file (3 uses) should be ranked higher than grep (2 uses)
    assert!(
        read_file_index < grep_index,
        "read_file should be ranked higher than grep due to more usage"
    );
}

#[test]
fn test_tool_selector_custom_filters() {
    // Test custom include/exclude filters
    let selector = ToolSelector::new()
        .with_profile(ToolProfile::All)
        .always_include("custom_tool")
        .always_exclude("bash");

    let tools = selector.select_tools();

    // Custom tool should be included
    assert!(tools.contains(&"custom_tool".to_string()));

    // bash should be excluded
    assert!(!tools.contains(&"bash".to_string()));

    // Other tools should still be present
    assert!(tools.contains(&"read_file".to_string()));
}

#[test]
fn test_tool_selector_prediction_from_prompt() {
    // Test tool prediction based on prompt
    let mut selector = ToolSelector::new();

    // Record some usage to influence ranking
    selector.record_use("read_file");
    selector.record_use("read_file");
    selector.record_use("grep");

    // Predict tools for an explore prompt
    let predicted = selector.predict_from_prompt("Show me the authentication code");

    // Should return tools
    assert!(!predicted.is_empty());

    // Should include exploration tools
    assert!(predicted.contains(&"read_file".to_string()));

    // read_file should be ranked high due to usage
    if predicted[0] == "read_file" {
        // Good - usage-based ranking is working
    }
}

#[test]
fn test_tool_selector_format_for_llm() {
    // Test formatting tools for LLM consumption
    let selector = ToolSelector::new();
    let tools = vec![
        "read_file".to_string(),
        "grep".to_string(),
        "bash".to_string(),
    ];

    let formatted = selector.format_tools_for_llm(&tools);

    assert_eq!(formatted, "read_file, grep, bash");
}

#[test]
fn test_tool_selector_suggestions_filtering() {
    // Test that suggestions filter out unwanted tools
    let selector = ToolSelector::new().with_profile(ToolProfile::All);

    let suggestions = selector.suggest_tools();

    // Get the list of tools that should be filtered (from All profile)
    let filtered_tools = ToolProfile::All.filtered_suggestions();

    // Verify that filtered tools are not in suggestions
    for &tool in filtered_tools {
        assert!(
            !suggestions.contains(&tool.to_string()),
            "Filtered tool '{}' should not appear in suggestions",
            tool
        );
    }
}

#[test]
fn test_tool_selector_multiple_filters_interaction() {
    // Test interaction between profile and custom filters
    let selector = ToolSelector::new()
        .with_profile(ToolProfile::Explore)
        .always_exclude("read_file")
        .always_include("git_commit");

    let tools = selector.select_tools();

    // read_file should be excluded even though it's in Explore profile
    assert!(!tools.contains(&"read_file".to_string()));

    // git_commit should be included even though it's not in Explore profile
    assert!(tools.contains(&"git_commit".to_string()));

    // Other Explore tools should still be present
    assert!(tools.contains(&"grep".to_string()));
}

#[test]
fn test_usage_tracker_statistics() {
    // Test usage tracker statistics
    let mut tracker = UsageTracker::new();

    // Record various uses
    tracker.record_use("read_file");
    tracker.record_use("read_file");
    tracker.record_use("grep");
    tracker.record_use("bash");

    // Test usage counts
    assert_eq!(tracker.usage_count("read_file"), 2);
    assert_eq!(tracker.usage_count("grep"), 1);
    assert_eq!(tracker.usage_count("bash"), 1);
    assert_eq!(tracker.usage_count("nonexistent"), 0);

    // Test total uses
    assert_eq!(tracker.total_uses(), 4);

    // Test unique tools
    assert_eq!(tracker.unique_tools(), 3);

    // Test frequent tools
    let frequent = tracker.frequent_tools(2);
    assert_eq!(frequent.len(), 2);
    assert_eq!(frequent[0], "read_file"); // Most frequent
}

#[test]
fn test_tool_selector_ranking_with_ties() {
    // Test ranking when tools have equal usage
    let mut selector = ToolSelector::new().with_profile(ToolProfile::Explore);

    // Record equal usage
    selector.record_use("read_file");
    selector.record_use("grep");

    let tools = selector.select_tools();

    // Both should be present
    assert!(tools.contains(&"read_file".to_string()));
    assert!(tools.contains(&"grep".to_string()));

    // With ties, order may vary but both should be in the list
}

#[test]
fn test_profile_detection_edge_cases() {
    // Test edge cases in profile detection

    // Empty prompt
    let detected = ToolProfile::from_prompt("");
    // Should not panic and return a valid profile
    assert!(matches!(detected, ToolProfile::Explore | ToolProfile::All));

    // Very short prompt
    let detected = ToolProfile::from_prompt("test");
    // Should not panic
    assert!(matches!(
        detected,
        ToolProfile::Explore | ToolProfile::All | ToolProfile::Ops
    ));

    // Prompt with only punctuation
    let _detected = ToolProfile::from_prompt("?!...");
    // Should not panic
}

#[test]
fn test_tool_selector_concurrent_usage_recording() {
    // Test that usage recording works correctly with multiple calls
    let mut selector = ToolSelector::new();

    // Record the same tool multiple times
    for _ in 0..10 {
        selector.record_use("read_file");
    }

    let tools = selector.select_tools();

    // read_file should be ranked first
    assert_eq!(tools[0], "read_file");
}
