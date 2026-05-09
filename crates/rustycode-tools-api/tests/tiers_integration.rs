//! Integration tests for tool tier management.

use rustycode_tools_api::{
    tiers::{default_tool_set, extended_tool_set},
    ToolActivationManager,
};

#[test]
fn test_tool_tier_promotion_logic() {
    let mut manager = ToolActivationManager::new();

    // Starts at Extended (LSP and advanced tools available from the start)
    assert_eq!(
        manager.current_tier(),
        rustycode_tools_api::tiers::ToolTier::Extended
    );

    manager.promote(rustycode_tools_api::tiers::ToolTier::Full);
    assert_eq!(
        manager.current_tier(),
        rustycode_tools_api::tiers::ToolTier::Full
    );

    // Should not demote
    manager.promote(rustycode_tools_api::tiers::ToolTier::Default);
    assert_eq!(
        manager.current_tier(),
        rustycode_tools_api::tiers::ToolTier::Full
    );
}

#[test]
fn test_tool_availability_by_tier() {
    let manager = ToolActivationManager::new();

    // Extended tier (default start) should have basic + extended tools
    assert!(manager.is_tool_allowed("Read"));
    assert!(manager.is_tool_allowed("Edit"));
    assert!(manager.is_tool_allowed("Write"));
    assert!(manager.is_tool_allowed("Bash"));
    assert!(manager.is_tool_allowed("Grep"));
    assert!(manager.is_tool_allowed("Glob"));
    assert!(manager.is_tool_allowed("WebFetch"));
    assert!(manager.is_tool_allowed("NotebookEdit"));
    assert!(manager.is_tool_allowed("lsp_hover"));

    // Extended tier should NOT allow arbitrary tools
    assert!(!manager.is_tool_allowed("custom_tool_xyz"));

    // Promote to full tier
    let mut manager = manager;
    manager.promote(rustycode_tools_api::tiers::ToolTier::Full);

    assert!(manager.is_tool_allowed("Read"));
    assert!(manager.is_tool_allowed("WebFetch"));
    assert!(manager.is_tool_allowed("custom_tool_xyz"));
    assert!(manager.is_tool_allowed("another_unknown_tool"));
}

#[test]
fn test_skill_scoping_intersects_with_tier() {
    let mut manager = ToolActivationManager::new();

    // Extended tier: basic + extended tools all available
    assert!(manager.is_tool_allowed("Read"));
    assert!(manager.is_tool_allowed("WebFetch"));

    // Apply skill scope that restricts to subset
    manager = manager.with_scope(vec![
        "Read".to_string(),
        "WebFetch".to_string(),
        "Bash".to_string(),
    ]);

    // Scope filters to intersection with tier
    assert!(manager.is_tool_allowed("Read"));
    assert!(manager.is_tool_allowed("WebFetch"));

    // But scope should still filter
    assert!(!manager.is_tool_allowed("Edit")); // in tier but not in scope
    assert!(!manager.is_tool_allowed("Grep")); // in tier but not in scope
}

#[test]
fn test_tool_sets_match_expectations() {
    let defaults = default_tool_set();
    let extended = extended_tool_set();

    // Default tools should be core six
    assert!(defaults.contains("Read"));
    assert!(defaults.contains("Edit"));
    assert!(defaults.contains("Write"));
    assert!(defaults.contains("Bash"));
    assert!(defaults.contains("Grep"));
    assert!(defaults.contains("Glob"));
    assert_eq!(defaults.len(), 6);

    // Extended tools should have expected additional tools
    assert!(extended.contains("WebFetch"));
    assert!(extended.contains("NotebookEdit"));
    assert!(extended.contains("lsp_diagnostics"));
    assert!(extended.contains("lsp_hover"));
    assert!(extended.contains("lsp_definition"));
    assert!(extended.contains("lsp_references"));
    assert!(extended.contains("lsp_completion"));
    assert!(extended.contains("todo_write"));
    assert!(extended.contains("memory_search"));
    assert!(extended.contains("memory_list"));
    assert!(extended.contains("list_dir"));
    assert!(extended.contains("git_status"));
    assert!(extended.contains("git_diff"));
    assert!(extended.contains("git_log"));

    // No overlap between default and extended (by design)
    assert!(
        defaults.intersection(&extended).copied().next().is_none(),
        "Default and extended tool sets should not overlap"
    );
}

#[test]
fn test_usage_tracking() {
    let mut manager = ToolActivationManager::new();

    manager.record_use("Read", true);
    manager.record_use("Read", true);
    manager.record_use("Bash", false);
    manager.record_use("WebFetch", true);

    assert_eq!(manager.usage().invocation_count("Read"), 2);
    assert_eq!(manager.usage().invocation_count("Bash"), 1);
    assert_eq!(manager.usage().invocation_count("WebFetch"), 1);
    assert_eq!(manager.usage().invocation_count("nonexistent"), 0);

    let read_rate = manager.usage().success_rate("Read");
    assert!((read_rate - 1.0).abs() < 0.001);

    let bash_rate = manager.usage().success_rate("Bash");
    assert!((bash_rate - 0.0).abs() < 0.001);
}
