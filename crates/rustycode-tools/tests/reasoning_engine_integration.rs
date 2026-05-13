#![allow(clippy::expect_used)]
#![allow(clippy::unwrap_used)]

use rustycode_tools::providers::check_integration::CheckIntegrationTool;
use rustycode_tools::providers::decompose::DecomposeProblemTool;
use rustycode_tools::providers::guide_research::GuideResearchTool;
use rustycode_tools::providers::reasoning_types::{
    BudgetState, ReasoningPhase, MAX_EXPLORATION_CALLS, MAX_THINKING_NODES,
};
use rustycode_tools::providers::validate_requirements::ValidateRequirementsTool;
use rustycode_tools_api::{Tool, ToolContext};
use serde_json::json;

fn ctx() -> ToolContext {
    ToolContext::new(std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
        .with_structured_output(true)
}

#[test]
fn full_four_phase_workflow() {
    let decompose = DecomposeProblemTool;
    let research = GuideResearchTool;
    let validate = ValidateRequirementsTool;
    let check = CheckIntegrationTool;

    // Phase 1: Decompose
    let result = decompose
        .execute(
            json!({"goal": "Build a task queue system", "context": "Rust, async, tokio"}),
            &ctx(),
        )
        .expect("reasoning_decompose should succeed");
    let structured = result
        .structured
        .as_ref()
        .expect("decompose should have structured output");
    assert_eq!(structured["phase"], "decompose");
    assert_eq!(structured["goal"], "Build a task queue system");
    assert!(structured["instruction"]
        .as_str()
        .unwrap()
        .contains("submodules"));

    // Phase 2: Research
    let result = research
        .execute(
            json!({
                "module_name": "worker_pool",
                "open_question": "How to manage concurrent workers safely?",
                "known_constraints": "Must use tokio::sync primitives"
            }),
            &ctx(),
        )
        .expect("reasoning_research should succeed");
    let structured = result
        .structured
        .as_ref()
        .expect("research should have structured output");
    assert_eq!(structured["phase"], "research");
    assert_eq!(structured["module"], "worker_pool");
    assert!(structured["instruction"]
        .as_str()
        .unwrap()
        .contains("tokio::sync"));

    // Phase 3: Validate
    let result = validate
        .execute(
            json!({
                "requirements": "Workers must handle task retry with exponential backoff",
                "context": "Using tokio channel for task distribution"
            }),
            &ctx(),
        )
        .expect("reasoning_validate should succeed");
    let structured = result
        .structured
        .as_ref()
        .expect("validate should have structured output");
    assert_eq!(structured["phase"], "validate_requirements");
    assert!(structured["validation_checklist"].is_array());

    // Phase 4: Check integration
    let result = check
        .execute(
            json!({"changes": "Added worker_pool module with retry logic", "scope": "crate"}),
            &ctx(),
        )
        .expect("reasoning_integrate should succeed");
    let structured = result
        .structured
        .as_ref()
        .expect("check should have structured output");
    assert_eq!(structured["phase"], "check_integration");
    assert_eq!(structured["scope"], "crate");
    assert!(structured["integration_checklist"].is_array());
}

#[test]
fn budget_exhaustion_triggers_stop_and_code() {
    let mut budget = BudgetState::default();
    assert!(!budget.is_exhausted());

    for i in 0..MAX_EXPLORATION_CALLS {
        let triggered = budget.record_exploration();
        if i == MAX_EXPLORATION_CALLS - 1 {
            assert!(triggered);
            assert!(budget.stop_and_code_active);
            assert!(budget.force_stop);
        } else {
            assert!(!triggered);
        }
    }
    assert!(budget.is_exhausted());

    budget.record_code();
    assert!(!budget.force_stop);
    assert_eq!(budget.code_calls, 1);
}

#[test]
fn budget_warning_text_progression() {
    let mut budget = BudgetState::default();
    assert!(budget.warning_text().is_none());

    budget.record_exploration();
    let warning = budget.warning_text().unwrap();
    assert!(warning.contains("1/10"));
    assert!(warning.contains("Continue researching"));

    budget.exploration_calls = MAX_EXPLORATION_CALLS - 2;
    let warning = budget.warning_text().unwrap();
    assert!(warning.contains("Approaching limit"));
}

#[test]
fn phase_next_tool_chain() {
    assert_eq!(
        ReasoningPhase::Decompose.recommended_next_tool(),
        "ReasoningResearch"
    );
    assert_eq!(
        ReasoningPhase::Research.recommended_next_tool(),
        "ReasoningValidate"
    );
    assert_eq!(
        ReasoningPhase::Clarify.recommended_next_tool(),
        "ReasoningIntegrate"
    );
    assert_eq!(
        ReasoningPhase::Integrate.recommended_next_tool(),
        "implement_now"
    );
}

#[test]
fn node_limit_exhaustion_independent_of_code_calls() {
    let budget = BudgetState {
        exploration_calls: 0,
        code_calls: 5,
        nodes_created: MAX_THINKING_NODES,
        force_stop: false,
        stop_and_code_active: false,
    };
    assert!(budget.is_exhausted());
}
