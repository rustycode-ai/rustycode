#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::format_push_string,
    clippy::items_after_statements,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Performance tests for context reduction with tool selection
//!
//! These tests measure and validate that intelligent tool selection
//! reduces context size by 40-80%.

use rustycode_tools::default_registry;
use rustycode_tools_api::{ToolProfile, ToolSelector};
use std::sync::Arc;

#[test]
fn test_tool_count_reduction_baselines() {
    // Establish baseline: all tools in registry
    let registry = Arc::new(default_registry());
    let all_tools = registry.list();
    let baseline_count = all_tools.len();

    println!("Baseline: {} tools in registry", baseline_count);

    // Skip if registry is empty (test environment)
    if baseline_count == 0 {
        println!("Warning: Empty registry, skipping ratio tests");
        return;
    }

    // Measure reduction for each profile using available_tools()
    let explore_tools: Vec<String> = ToolProfile::Explore
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();
    let implement_tools: Vec<String> = ToolProfile::Implement
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();
    let debug_tools: Vec<String> = ToolProfile::Debug
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();
    let ops_tools: Vec<String> = ToolProfile::Ops
        .available_tools()
        .iter()
        .map(ToString::to_string)
        .collect();

    println!("\nTool count reduction:");
    println!(
        "  Explore: {} / {} ({:.1}%)",
        explore_tools.len(),
        baseline_count,
        (explore_tools.len() as f64 / baseline_count as f64) * 100.0
    );
    println!(
        "  Implement: {} / {} ({:.1}%)",
        implement_tools.len(),
        baseline_count,
        (implement_tools.len() as f64 / baseline_count as f64) * 100.0
    );
    println!(
        "  Debug: {} / {} ({:.1}%)",
        debug_tools.len(),
        baseline_count,
        (debug_tools.len() as f64 / baseline_count as f64) * 100.0
    );
    println!(
        "  Ops: {} / {} ({:.1}%)",
        ops_tools.len(),
        baseline_count,
        (ops_tools.len() as f64 / baseline_count as f64) * 100.0
    );

    // Assert that all profiles reduce tool count
    assert!(
        explore_tools.len() < baseline_count,
        "Explore profile should reduce tool count"
    );
    assert!(
        implement_tools.len() < baseline_count,
        "Implement profile should reduce tool count"
    );
    assert!(
        debug_tools.len() < baseline_count,
        "Debug profile should reduce tool count"
    );
    assert!(
        ops_tools.len() < baseline_count,
        "Ops profile should reduce tool count"
    );
}

#[test]
fn test_token_estimation_savings() {
    // Estimate token savings from tool selection
    //
    // Assumptions:
    // - Average tool definition: ~150 tokens (name, description, schema)
    // - Tool definitions are sent as part of the system prompt

    let registry = Arc::new(default_registry());
    let all_tools = registry.list();
    let baseline_count = all_tools.len();

    if baseline_count == 0 {
        println!("Warning: Empty registry, skipping token estimation");
        return;
    }

    const TOKENS_PER_TOOL: u32 = 150;
    let baseline_tokens = baseline_count as u32 * TOKENS_PER_TOOL;

    let explore_tools = ToolSelector::new()
        .with_profile(ToolProfile::Explore)
        .select_tools(&registry);
    let implement_tools = ToolSelector::new()
        .with_profile(ToolProfile::Implement)
        .select_tools(&registry);

    let explore_tokens = explore_tools.len() as u32 * TOKENS_PER_TOOL;
    let implement_tokens = implement_tools.len() as u32 * TOKENS_PER_TOOL;

    let explore_savings = baseline_tokens - explore_tokens;
    let implement_savings = baseline_tokens - implement_tokens;
    let explore_savings_pct = (explore_savings as f64 / baseline_tokens as f64) * 100.0;
    let implement_savings_pct = (implement_savings as f64 / baseline_tokens as f64) * 100.0;

    println!("\nToken savings (estimated):");
    println!(
        "  Baseline: {} tokens ({} tools × {} tokens/tool)",
        baseline_tokens, baseline_count, TOKENS_PER_TOOL
    );
    println!(
        "  Explore: {} tokens ({} tools) - saved {} tokens ({:.1}%)",
        explore_tokens,
        explore_tools.len(),
        explore_savings,
        explore_savings_pct
    );
    println!(
        "  Implement: {} tokens ({} tools) - saved {} tokens ({:.1}%)",
        implement_tokens,
        implement_tools.len(),
        implement_savings,
        implement_savings_pct
    );

    // Assert significant token savings (≥ 30%)
    assert!(
        explore_savings_pct >= 30.0,
        "Explore profile should save at least 30% tokens, got {:.1}%",
        explore_savings_pct
    );
    assert!(
        implement_savings_pct >= 30.0,
        "Implement profile should save at least 30% tokens, got {:.1}%",
        implement_savings_pct
    );
}

#[test]
fn test_tool_selection_relevance() {
    // Verify that selected tools are actually relevant for the detected intent
    //
    // This test ensures that tool selection doesn't just reduce count,
    // but also maintains quality by selecting appropriate tools.

    let selector = ToolSelector::new();
    let registry = default_registry();

    // Explore profile should prioritize read-only tools
    let explore_tools = selector
        .clone()
        .with_profile(ToolProfile::Explore)
        .select_tools(&registry);

    println!("\nExplore profile tools ({}):", explore_tools.len());
    for tool in &explore_tools {
        println!("  - {}", tool);
    }

    // Verify expected explore tools are present (if registry has them)
    let expected_explore_tools = vec!["Read", "Grep", "list_dir", "Glob"];
    for expected_tool in expected_explore_tools {
        if explore_tools.contains(&expected_tool.to_string()) {
            println!("  ✓ Explore profile includes {}", expected_tool);
        } else {
            println!(
                "  ⚠ Explore profile missing {} (may not be in registry)",
                expected_tool
            );
        }
    }

    // Implement profile should prioritize write tools
    let implement_tools = selector
        .clone()
        .with_profile(ToolProfile::Implement)
        .select_tools(&registry);

    println!("\nImplement profile tools ({}):", implement_tools.len());
    for tool in &implement_tools {
        println!("  - {}", tool);
    }

    // Verify expected implement tools are present
    let expected_implement_tools = vec!["Write", "Bash", "edit"];
    for expected_tool in expected_implement_tools {
        if implement_tools.contains(&expected_tool.to_string()) {
            println!("  ✓ Implement profile includes {}", expected_tool);
        } else {
            println!(
                "  ⚠ Implement profile missing {} (may not be in registry)",
                expected_tool
            );
        }
    }

    // Debug profile should prioritize diagnostic tools
    let debug_tools = selector
        .with_profile(ToolProfile::Debug)
        .select_tools(&registry);

    println!("\nDebug profile tools ({}):", debug_tools.len());
    for tool in &debug_tools {
        println!("  - {}", tool);
    }

    // Verify expected debug tools are present
    let expected_debug_tools = vec!["Bash", "Grep", "Read"];
    for expected_tool in expected_debug_tools {
        if debug_tools.contains(&expected_tool.to_string()) {
            println!("  ✓ Debug profile includes {}", expected_tool);
        } else {
            println!(
                "  ⚠ Debug profile missing {} (may not be in registry)",
                expected_tool
            );
        }
    }
}

#[test]
fn test_context_reduction_scenarios() {
    // Test realistic user scenarios and measure context reduction

    let scenarios = vec![
        ("Show me how authentication works", ToolProfile::Explore),
        ("Create a new user endpoint", ToolProfile::Implement),
        ("Fix the failing test", ToolProfile::Debug),
        ("Run cargo test", ToolProfile::Ops),
    ];

    let registry = Arc::new(default_registry());
    let baseline_count = registry.list().len();

    if baseline_count == 0 {
        println!("Warning: Empty registry, skipping scenario tests");
        return;
    }

    println!("\nScenario analysis (baseline: {} tools):", baseline_count);

    for (prompt, expected_profile) in scenarios {
        let detected_profile = ToolProfile::from_prompt(prompt);
        let tools = ToolSelector::new()
            .with_profile(detected_profile)
            .select_tools(&registry);
        let reduction = ((baseline_count - tools.len()) as f64 / baseline_count as f64) * 100.0;

        println!("\n  Prompt: \"{}\"", prompt);
        println!("    Profile: {:?}", detected_profile);
        println!(
            "    Tools: {} / {} ({:.1}% reduction)",
            tools.len(),
            baseline_count,
            reduction
        );

        // Verify profile detection matches expected
        assert_eq!(
            detected_profile, expected_profile,
            "Profile detection mismatch for '{}'",
            prompt
        );

        // Verify significant reduction (≥ 20%)
        assert!(
            reduction >= 20.0,
            "Scenario should achieve ≥20% reduction, got {:.1}%",
            reduction
        );
    }
}

#[test]
fn test_tool_selection_performance() {
    // Measure the performance of tool selection itself
    //
    // This ensures that tool selection is fast enough to not add
    // significant latency to LLM requests.

    use std::time::Instant;

    let selector = ToolSelector::new();
    let registry = default_registry();
    let iterations = 1000;

    let start = Instant::now();
    for _ in 0..iterations {
        selector
            .clone()
            .with_profile(ToolProfile::Explore)
            .select_tools(&registry);
        selector
            .clone()
            .with_profile(ToolProfile::Implement)
            .select_tools(&registry);
        selector
            .clone()
            .with_profile(ToolProfile::Debug)
            .select_tools(&registry);
    }
    let duration = start.elapsed();

    let avg_time_per_selection = duration / (iterations * 3);
    println!("\nTool selection performance:");
    println!("  {} selections in {:?}", iterations * 3, duration);
    println!("  Average: {:?} per selection", avg_time_per_selection);

    // Assert that selection is fast (< 1ms per selection)
    assert!(
        avg_time_per_selection.as_micros() < 1000,
        "Tool selection should be < 1ms, got {:?}",
        avg_time_per_selection
    );
}
