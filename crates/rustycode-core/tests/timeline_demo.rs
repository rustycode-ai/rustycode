#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
//! Timeline visualization demo test.
//!
//! Run with: cargo test --package rustycode-core --test timeline_demo -- --nocapture

use rustycode_core::team::interaction_visualizer::generate_full_visualization;
use rustycode_core::team::{AgentState, AgentTimeline, TaskStatus};
use rustycode_protocol::agent_protocol::AgentRole;

#[test]
fn demo_complex_feature_timeline() {
    let mut timeline = AgentTimeline::new("complex-feature");

    // Simulate Architect phase for high-risk task
    timeline.activate_agent(AgentRole::Architect, "High risk task - needs planning");
    timeline.record_state_change(AgentRole::Architect, AgentState::Idle, AgentState::Reading);
    timeline.record_state_change(
        AgentRole::Architect,
        AgentState::Reading,
        AgentState::Analyzing,
    );
    timeline.record_state_change(
        AgentRole::Architect,
        AgentState::Analyzing,
        AgentState::Declaring,
    );
    timeline.deactivate_agent(AgentRole::Architect, "Declaration produced");

    timeline.next_turn();

    // Builder step 1
    timeline.activate_agent(AgentRole::Builder, "Step 1: Implement core logic");
    timeline.record_state_change(AgentRole::Builder, AgentState::Idle, AgentState::Reasoning);
    timeline.record_state_change(
        AgentRole::Builder,
        AgentState::Reasoning,
        AgentState::Implementing,
    );
    timeline.deactivate_agent(AgentRole::Builder, "Step complete");

    // Skeptic review
    timeline.activate_agent(AgentRole::Skeptic, "Review builder output");
    timeline.record_state_change(AgentRole::Skeptic, AgentState::Idle, AgentState::Reviewing);
    timeline.record_state_change(
        AgentRole::Skeptic,
        AgentState::Reviewing,
        AgentState::Verifying,
    );
    timeline.deactivate_agent(AgentRole::Skeptic, "Approved");

    // Judge verification
    timeline.activate_agent(AgentRole::Judge, "Run cargo check and tests");
    timeline.record_state_change(AgentRole::Judge, AgentState::Idle, AgentState::Compiling);
    timeline.record_insight(AgentRole::Judge, "Compilation successful");
    timeline.record_insight(AgentRole::Judge, "All tests passed");
    timeline.deactivate_agent(AgentRole::Judge, "All checks passed");

    timeline.next_turn();

    // Builder step 2
    timeline.activate_agent(AgentRole::Builder, "Step 2: Add tests");
    timeline.record_state_change(AgentRole::Builder, AgentState::Idle, AgentState::Reasoning);
    timeline.record_state_change(
        AgentRole::Builder,
        AgentState::Reasoning,
        AgentState::Implementing,
    );
    timeline.deactivate_agent(AgentRole::Builder, "Step complete");

    // Skeptic review
    timeline.activate_agent(AgentRole::Skeptic, "Review tests");
    timeline.record_state_change(AgentRole::Skeptic, AgentState::Idle, AgentState::Reviewing);
    timeline.record_state_change(
        AgentRole::Skeptic,
        AgentState::Reviewing,
        AgentState::Verifying,
    );
    timeline.deactivate_agent(AgentRole::Skeptic, "Approved");

    // Judge verification
    timeline.activate_agent(AgentRole::Judge, "Run cargo test");
    timeline.record_state_change(AgentRole::Judge, AgentState::Idle, AgentState::Testing);
    timeline.record_insight(AgentRole::Judge, "25 tests passed");
    timeline.deactivate_agent(AgentRole::Judge, "All checks passed");

    timeline.next_turn();
    timeline.set_status(TaskStatus::Success);

    println!("\n{}", "=".repeat(80));
    println!("  COMPLEX FEATURE TIMELINE VISUALIZATION");
    println!("{}", "=".repeat(80));
    println!("\n{}", generate_full_visualization(&timeline));
}

#[test]
fn demo_compile_error_fix() {
    let mut timeline = AgentTimeline::new("compile-error-fix");

    // Low risk - straight to Builder
    timeline.activate_agent(AgentRole::Builder, "Add missing import");
    timeline.record_state_change(
        AgentRole::Builder,
        AgentState::Idle,
        AgentState::Implementing,
    );
    timeline.deactivate_agent(AgentRole::Builder, "Fix applied");

    timeline.next_turn();

    // Skeptic review
    timeline.activate_agent(AgentRole::Skeptic, "Review fix");
    timeline.record_state_change(AgentRole::Skeptic, AgentState::Idle, AgentState::Reviewing);
    timeline.deactivate_agent(AgentRole::Skeptic, "Approved");

    // Judge finds compile error
    timeline.activate_agent(AgentRole::Judge, "Run cargo check");
    timeline.record_state_change(AgentRole::Judge, AgentState::Idle, AgentState::Compiling);
    timeline.record_insight(AgentRole::Judge, "error[E0432]: unresolved import");
    timeline.deactivate_agent(AgentRole::Judge, "Compile error detected");

    timeline.next_turn();

    // Scalpel fixes it
    timeline.activate_agent(AgentRole::Scalpel, "Fix import error");
    timeline.record_state_change(AgentRole::Scalpel, AgentState::Idle, AgentState::Diagnosing);
    timeline.record_state_change(
        AgentRole::Scalpel,
        AgentState::Diagnosing,
        AgentState::Fixing,
    );
    timeline.deactivate_agent(AgentRole::Scalpel, "Fix applied (3 lines)");

    timeline.next_turn();

    // Judge verifies
    timeline.activate_agent(AgentRole::Judge, "Verify fix");
    timeline.record_state_change(AgentRole::Judge, AgentState::Idle, AgentState::Compiling);
    timeline.record_insight(AgentRole::Judge, "Compilation successful");
    timeline.deactivate_agent(AgentRole::Judge, "All checks passed");

    timeline.set_status(TaskStatus::Success);

    println!("\n{}", "=".repeat(70));
    println!("  COMPILE ERROR FIX TIMELINE (with Scalpel)");
    println!("{}", "=".repeat(70));
    println!("\n{}", timeline.ascii_visualization());
}
