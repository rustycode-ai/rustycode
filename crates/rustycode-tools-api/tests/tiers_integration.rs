//! Integration tests for tool tier management.

use rustycode_tools_api::{
    tiers::{default_tool_set, extended_tool_set},
    ToolActivationManager,
};

#[test]
fn test_tool_tier_promotion_logic() {
    let mut manager = ToolActivationManager::new();

    assert_eq!(
        manager.current_tier(),
        rustycode_tools_api::tiers::ToolTier::Default
    );

    manager.promote(rustycode_tools_api::tiers::ToolTier::Extended);
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

    // Default tier should have basic tools
    assert!(manager.is_tool_allowed("read_file"));
    assert!(manager.is_tool_allowed("edit_file"));
    assert!(manager.is_tool_allowed("write_file"));
    assert!(manager.is_tool_allowed("bash"));
    assert!(manager.is_tool_allowed("grep"));
    assert!(manager.is_tool_allowed("glob"));

    // Default tier should NOT have extended tools
    assert!(!manager.is_tool_allowed("web_fetch"));
    assert!(!manager.is_tool_allowed("notebook_edit"));
    assert!(!manager.is_tool_allowed("lsp_hover"));

    // Promote to extended tier
    let mut manager = manager;
    manager.promote(rustycode_tools_api::tiers::ToolTier::Extended);

    assert!(manager.is_tool_allowed("read_file"));
    assert!(manager.is_tool_allowed("web_fetch"));
    assert!(manager.is_tool_allowed("notebook_edit"));
    assert!(manager.is_tool_allowed("lsp_hover"));

    // Promote to full tier
    manager.promote(rustycode_tools_api::tiers::ToolTier::Full);

    assert!(manager.is_tool_allowed("read_file"));
    assert!(manager.is_tool_allowed("web_fetch"));
    assert!(manager.is_tool_allowed("custom_tool_xyz"));
    assert!(manager.is_tool_allowed("another_unknown_tool"));
}

#[test]
fn test_skill_scoping_intersects_with_tier() {
    let mut manager = ToolActivationManager::new();

    assert!(manager.is_tool_allowed("read_file"));
    assert!(!manager.is_tool_allowed("web_fetch"));

    // Apply skill scope that allows both default and extended tools
    manager = manager.with_scope(vec![
        "read_file".to_string(),
        "web_fetch".to_string(),
        "bash".to_string(),
    ]);

    // Should still respect tier restrictions
    assert!(manager.is_tool_allowed("read_file"));
    assert!(!manager.is_tool_allowed("web_fetch")); // in scope but not in default tier

    // Promote to extended tier
    manager.promote(rustycode_tools_api::tiers::ToolTier::Extended);

    assert!(manager.is_tool_allowed("read_file"));
    assert!(manager.is_tool_allowed("web_fetch"));
    assert!(manager.is_tool_allowed("bash"));

    // But scope should still filter
    assert!(!manager.is_tool_allowed("edit_file")); // in tier but not in scope
    assert!(!manager.is_tool_allowed("grep")); // in tier but not in scope
}

#[test]
fn test_tool_sets_match_expectations() {
    let defaults = default_tool_set();
    let extended = extended_tool_set();

    // Default tools should be core six
    assert!(defaults.contains("read_file"));
    assert!(defaults.contains("edit_file"));
    assert!(defaults.contains("write_file"));
    assert!(defaults.contains("bash"));
    assert!(defaults.contains("grep"));
    assert!(defaults.contains("glob"));
    assert_eq!(defaults.len(), 6);

    // Extended tools should have expected additional tools
    assert!(extended.contains("web_fetch"));
    assert!(extended.contains("notebook_edit"));
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

    manager.record_use("read_file", true);
    manager.record_use("read_file", true);
    manager.record_use("bash", false);
    manager.record_use("web_fetch", true);

    assert_eq!(manager.usage().invocation_count("read_file"), 2);
    assert_eq!(manager.usage().invocation_count("bash"), 1);
    assert_eq!(manager.usage().invocation_count("web_fetch"), 1);
    assert_eq!(manager.usage().invocation_count("nonexistent"), 0);

    let read_rate = manager.usage().success_rate("read_file");
    assert!((read_rate - 1.0).abs() < 0.001);

    let bash_rate = manager.usage().success_rate("bash");
    assert!((bash_rate - 0.0).abs() < 0.001);
}
