//! Integration test: Analysis → Strategy → Thinking pipeline
//!
//! Verifies the full pipeline from quality detection through strategy
//! selection to structured thinking dispatch works end-to-end.

#![allow(
    unknown_lints,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::option_if_let_else,
    clippy::similar_names,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::ptr_arg,
    clippy::format_in_format_args,
    clippy::let_and_return,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::let_unit_value,
    clippy::unused_async,
    clippy::doc_markdown,
    clippy::unnecessary_lazy_evaluations
)]

use rustycode_orchestration::{
    quality_detector::QualityDetector,
    reasoning_store::ReasoningStore,
    router::{RoutingRequest, TaskRouter},
    routing_metrics::{ExecutionResult, ModelChoice, RoutingMetrics},
    strategy_selector::StrategySelector,
    types::ReasoningStrategy,
};
use tempfile::TempDir;

/// Verify that a low-quality response selects a deeper strategy.
#[test]
fn low_quality_response_triggers_sequential_thinking() {
    let detector = QualityDetector::new();
    let selector = StrategySelector::new();
    let poor_response = "yes";
    let score = detector.evaluate(poor_response);

    assert!(
        score.total < 2.0,
        "poor response should score low, got {}",
        score.total
    );

    let strategy = selector.select(1.0, &score, 30);
    assert!(
        matches!(
            strategy,
            ReasoningStrategy::SequentialThinking | ReasoningStrategy::PhasedOrchestration
        ),
        "low quality should trigger structured thinking, got {:?}",
        strategy,
    );
}

/// Verify that a high-quality simple task gets direct execution.
#[test]
fn high_quality_simple_task_gets_direct_execution() {
    let detector = QualityDetector::new();
    let selector = StrategySelector::new();
    let good_response = "The algorithm uses a binary search tree because lookup is O(log n). \
        Edge cases include empty input and single-element arrays. Tests assert the invariant.";
    let score = detector.evaluate(good_response);

    assert!(
        score.total >= 4.0,
        "good response should score well, got {}",
        score.total
    );

    let strategy = selector.select(1.0, &score, 90);
    assert!(
        matches!(
            strategy,
            ReasoningStrategy::DirectExecution | ReasoningStrategy::QuickSelfEval
        ),
        "high quality with high confidence should get direct or quick strategy, got {:?}",
        strategy,
    );
}

/// Verify thoughts are stored and phase context is retrievable.
#[test]
fn thoughts_stored_and_phase_context_retrieved() {
    let temp_dir = TempDir::new().unwrap();
    let store = ReasoningStore::new(temp_dir.path().to_path_buf());

    let thought = rustycode_orchestration::StructuredThought::new(
        "Consider using HashMap for O(1) lookups".into(),
        1,
        rustycode_orchestration::ThoughtType::Decision,
    );
    store.store_thought("pipeline-test", 1, &thought).unwrap();

    let retrieved = store.phase_thoughts("pipeline-test", 1).unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(
        retrieved[0].thought,
        "Consider using HashMap for O(1) lookups"
    );

    let ctx = store.context_for_next_phase("pipeline-test", 2).unwrap();
    assert_eq!(ctx["phase"], 2);
    assert!(
        ctx["previous_summary"]["decisions_made"]
            .as_array()
            .unwrap()
            .len()
            == 1
    );
}

/// Verify router selects models based on complexity and budget.
#[test]
fn router_routes_based_on_complexity_and_budget() {
    let router = TaskRouter::new();

    let simple = RoutingRequest {
        description: "fix typo in readme".into(),
        estimated_tokens: 200,
        budget_tokens: 5_000,
    };
    let decision = router.route(&simple).unwrap();
    assert_eq!(decision.selected_model, ModelChoice::Haiku);

    let complex = RoutingRequest {
        description:
            "design the distributed systems architecture for the new microservices platform".into(),
        estimated_tokens: 8_000,
        budget_tokens: 50_000,
    };
    let decision = router.route(&complex).unwrap();
    assert_eq!(decision.selected_model, ModelChoice::Opus);
}

/// Verify routing metrics accumulate and recommend correctly.
#[test]
fn routing_metrics_accumulate_and_recommend() {
    let mut metrics = RoutingMetrics::new();

    // Haiku: 5 successes with small tokens
    for _ in 0..5 {
        metrics.record_execution(
            ModelChoice::Haiku,
            &ExecutionResult::Success { tokens_used: 200 },
        );
    }

    // Sonnet: 3 successes with medium tokens
    for _ in 0..3 {
        metrics.record_execution(
            ModelChoice::Sonnet,
            &ExecutionResult::Success { tokens_used: 2_000 },
        );
    }

    assert_eq!(metrics.total_executions(), 8);
    assert!((metrics.success_rate(ModelChoice::Haiku) - 1.0).abs() < f64::EPSILON);
    assert!((metrics.success_rate(ModelChoice::Sonnet) - 1.0).abs() < f64::EPSILON);

    let recommended = metrics.recommend_model();
    assert!(
        recommended == ModelChoice::Haiku || recommended == ModelChoice::Sonnet,
        "recommendation should pick a model with data",
    );
}

/// Verify quality-critical keywords force Opus regardless of complexity.
#[test]
fn security_keywords_force_opus_routing() {
    let router = TaskRouter::new();

    let security_task = RoutingRequest {
        description: "fix this security vulnerability".into(),
        estimated_tokens: 500,
        budget_tokens: 5_000,
    };
    let decision = router.route(&security_task).unwrap();
    assert_eq!(decision.selected_model, ModelChoice::Opus);
    assert!(decision.estimated_cost > 0.0);
    assert!((0.0..=1.0).contains(&decision.confidence));
}

/// Full pipeline: quality → strategy → thinking → store → context.
#[test]
fn full_pipeline_end_to_end() {
    let temp_dir = TempDir::new().unwrap();
    let store = ReasoningStore::new(temp_dir.path().to_path_buf());

    // Phase 1: Analyze a moderate-quality response
    let detector = QualityDetector::new();
    let response =
        "The solution uses a hash map for caching. Because cache hits avoid recomputation.";
    let score = detector.evaluate(response);
    assert!(score.total > 0.0, "response should score positively");

    // Phase 2: Select strategy
    let confidence = 75;
    let selector = StrategySelector::new();
    let strategy = selector.select(2.5, &score, confidence);
    assert!(
        !matches!(strategy, ReasoningStrategy::PhasedOrchestration),
        "moderate response should not require phased orchestration",
    );

    // Phase 3: Store a thought for the phase
    let thought = rustycode_orchestration::StructuredThought::new(
        format!(
            "Strategy {:?} selected with score {:.1}",
            strategy, score.total
        ),
        1,
        rustycode_orchestration::ThoughtType::Decision,
    );
    store.store_thought("full-pipeline", 1, &thought).unwrap();

    // Phase 4: Verify phase context is available for next phase
    let ctx = store.context_for_next_phase("full-pipeline", 2).unwrap();
    assert_eq!(ctx["phase"], 2);
    assert!(ctx["previous_summary"]["thought_count"].as_u64().unwrap() > 0);

    // Phase 5: Route the task
    let router = TaskRouter::new();
    let request = RoutingRequest {
        description: "implement caching middleware for the API".into(),
        estimated_tokens: 2_000,
        budget_tokens: 10_000,
    };
    let routing = router.route(&request).unwrap();
    assert!(!routing.rationale.is_empty());
}
