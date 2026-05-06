#![allow(clippy::doc_markdown, clippy::unwrap_used)]

//! Integration test for TeamPanel event flow with TUI event loop.
//!
//! Verifies:
//! - TeamEvent broadcasts are received by TUI team panel
//! - Agent states are updated correctly
//! - Task completion renders properly
//! - Trust bar renders
//! - File changes display
//! - Panel visibility toggle
//! - Reset clears state for new task
//! - Phase 4 events handled gracefully

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use rustycode_team::orchestrator::TeamEvent;
use rustycode_tui::ui::team_panel::{AgentState, TeamPanel};

// Test: Agent activation updates panel state
#[test]
fn agent_activation_updates_panel() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Implementing".into(),
    });
    assert!(panel.has_active_agents());
    let name = panel.active_agent_name();
    assert_eq!(name.unwrap(), "Builder");
    assert_eq!(panel.current_turn(), 1);
}

// Test: Agent deactivation clears state
#[test]
fn agent_deactivation_clears_state() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Implementing".into(),
    });
    panel.handle_event(&TeamEvent::AgentDeactivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Done".into(),
    });
    assert!(!panel.has_active_agents());
}

// Test: Deactivation with failure reason
#[test]
fn deactivation_with_failure() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Implementing".into(),
    });
    panel.handle_event(&TeamEvent::AgentDeactivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Parse failed".into(),
    });
    assert_eq!(panel.agent_state("Builder"), Some(AgentState::Failed));
}

// Test: Task completion shows success + files
#[test]
fn task_completion_shows_success() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::TaskCompleted {
        success: true,
        turns: 5,
        files_modified: vec!["src/lib.rs".into()],
        final_trust: 0.95,
    });
    assert!(panel.is_complete());
    assert!(panel.is_success());
    assert_eq!(panel.current_turn(), 5);
    assert!((panel.trust_value() - 0.95).abs() < 0.01);
    assert_eq!(panel.files_modified(), &["src/lib.rs".to_string()]);
}

// Test: Task failure shows failure state
#[test]
fn task_failure_shows_failure() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::TaskCompleted {
        success: false,
        turns: 3,
        files_modified: vec![],
        final_trust: 0.3,
    });
    assert!(panel.is_complete());
    assert!(!panel.is_success());
    assert!(panel.trust_value() < 0.5);
}

// Test: Panel visibility toggle
#[test]
fn panel_visibility_toggle() {
    let mut panel = TeamPanel::new();
    assert!(!panel.visible);
    panel.toggle();
    assert!(panel.visible);
    panel.toggle();
    assert!(!panel.visible);
}

// Test: Step completed resets completed agents
#[test]
fn step_completed_resets_agents() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Implementing".into(),
    });
    panel.handle_event(&TeamEvent::AgentDeactivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Done".into(),
    });
    assert_eq!(panel.agent_state("Builder"), Some(AgentState::Complete));

    panel.handle_event(&TeamEvent::StepCompleted {
        step: 1,
        success: true,
        files: vec![],
    });
    assert_eq!(panel.agent_state("Builder"), Some(AgentState::Waiting));
}

// Test: Reset clears all state
#[test]
fn reset_clears_state() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 3,
        reason: "Implementing".into(),
    });
    assert_eq!(panel.total_activations(), 1);
    panel.reset();
    assert_eq!(panel.current_turn(), 0);
    assert_eq!(panel.total_activations(), 0);
    assert!(!panel.has_active_agents());
    assert!(panel.files_modified().is_empty());
}

// Test: Esc key hides panel with active agents (cancellation scenario)
#[test]
fn esc_key_hides_panel() {
    let mut panel = TeamPanel::new();
    panel.visible = true;
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Starting".into(),
    });
    assert!(panel.has_active_agents());
    panel.toggle();
    assert!(!panel.visible);
}

// Test: Render doesn't crash with normal area
#[test]
fn render_does_not_crash() {
    let mut panel = TeamPanel::new();
    panel.set_task("test task");
    panel.visible = true;

    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 38, 18);
    terminal
        .draw(|f| {
            let block = Block::default().borders(Borders::ALL);
            let paragraph = Paragraph::new(panel.build_content())
                .block(block)
                .wrap(Wrap { trim: false });
            paragraph.render(area, f.buffer_mut());
        })
        .unwrap();
}

// Test: Render doesn't crash with small area
#[test]
fn render_too_small_does_not_crash() {
    let panel = TeamPanel::new();
    let backend = TestBackend::new(5, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 5, 2);
    terminal
        .draw(|f| {
            panel.clone().render(area, f.buffer_mut());
        })
        .unwrap();
}

// Test: Build content shows empty state
#[test]
fn build_content_shows_empty_state() {
    let panel = TeamPanel::new();
    let content = panel.build_content();
    let has_awaiting = content
        .iter()
        .any(|line| line.spans.iter().any(|s| s.content.contains("Awaiting")));
    assert!(has_awaiting);
}

// Test: Build content shows success on completion
#[test]
fn build_content_shows_success() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::TaskCompleted {
        success: true,
        turns: 3,
        files_modified: vec![],
        final_trust: 0.9,
    });
    let content = panel.build_content();
    let has_success = content
        .iter()
        .any(|line| line.spans.iter().any(|s| s.content.contains("SUCCESS")));
    assert!(has_success);
}

// Test: Phase 4 events handled gracefully
#[test]
fn phase4_events_handled() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::CodeChanged {
        files: vec!["src/lib.rs".into()],
        author: "Builder".to_string(),
        generation: 1,
    });
    panel.handle_event(&TeamEvent::CompilationFailed {
        errors: "error msg".into(),
        files: vec!["src/lib.rs".into()],
        severity: "error".to_string(),
    });
    panel.handle_event(&TeamEvent::TestsFailed {
        failed_tests: vec!["test_basic".into()],
        total_failed: 1,
        error_output: "1 failed".into(),
    });
    panel.handle_event(&TeamEvent::TrustChanged {
        old_value: 0.7,
        new_value: 0.5,
        reason: "Builder improved".into(),
    });
    panel.handle_event(&TeamEvent::VerificationPassed {
        check_type: "compilation".into(),
        details: "All good".into(),
    });
    assert!(!panel.has_active_agents());
}

// Test: TrustChanged event updates trust score
#[test]
fn trust_changed_updates_trust() {
    let mut panel = TeamPanel::new();
    assert!(
        (panel.trust_value() - 0.7).abs() < 0.01,
        "Default trust should be 0.7"
    );

    panel.handle_event(&TeamEvent::TrustChanged {
        old_value: 0.7,
        new_value: 0.85,
        reason: "Good progress".into(),
    });
    assert!(
        (panel.trust_value() - 0.85).abs() < 0.01,
        "Trust should update to 0.85"
    );
}

// Test: Insight event updates detail
#[test]
fn insight_event_updates_detail() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Skeptic".into(),
        turn: 1,
        reason: "Reviewing".into(),
    });
    panel.handle_event(&TeamEvent::Insight {
        role: "Skeptic".into(),
        message: "Found potential issue".into(),
    });
    assert_eq!(panel.agent_detail("Skeptic"), Some("Found potential issue"));
}

// Test: Agent state change updates detail
#[test]
fn agent_state_change_updates_detail() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::AgentActivated {
        role: "Builder".into(),
        turn: 1,
        reason: "Implementing".into(),
    });
    panel.handle_event(&TeamEvent::AgentStateChanged {
        role: "Builder".into(),
        from_state: "Idle".into(),
        to_state: "Verifying code".into(),
    });
    assert_eq!(panel.agent_detail("Builder"), Some("Verifying code"));
}

// Test: Trust bar rendering
#[test]
fn trust_bar_rendering() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::TaskCompleted {
        success: true,
        turns: 1,
        files_modified: vec![],
        final_trust: 0.85,
    });
    let content = panel.build_content();
    let has_bar = content
        .iter()
        .any(|line| line.spans.iter().any(|s| s.content.contains("━")));
    assert!(has_bar);
}

// Test: File changes display on completion
#[test]
fn file_changes_display() {
    let mut panel = TeamPanel::new();
    panel.handle_event(&TeamEvent::TaskCompleted {
        success: true,
        turns: 1,
        files_modified: vec!["src/lib.rs".into(), "src/main.rs".into()],
        final_trust: 0.9,
    });
    let content = panel.build_content();
    let has_file = content
        .iter()
        .any(|line| line.spans.iter().any(|s| s.content.contains("src/lib.rs")));
    assert!(has_file);
}

// Test: Max turns getter
#[test]
fn max_turns_default() {
    let panel = TeamPanel::new();
    assert_eq!(panel.max_turns(), 50);
}

// Test: Set max turns
#[test]
fn set_max_turns() {
    let mut panel = TeamPanel::new();
    panel.set_max_turns(100);
    assert_eq!(panel.max_turns(), 100);
}
