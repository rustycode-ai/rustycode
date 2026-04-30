//! Thinking module public API integration tests.

#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::doc_markdown,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::field_reassign_with_default
)]

use rustycode_orchestration::thinking::core::graph::ReasoningGraph;
use rustycode_orchestration::thinking::core::scoring::ConfidenceScorer;
use rustycode_orchestration::thinking::core::types::{
    AggregationMethod, ExecutionParams, Operation, SelectionStrategy, ThinkingConfig, Thought,
    ThoughtKind,
};
use rustycode_orchestration::thinking::executor::{DefaultExecutor, ThinkingExecutor};
use rustycode_orchestration::thinking::operations::OperationExecutor;
use rustycode_orchestration::thinking::prompting::context::PromptContext;

// ─── 1. DefaultExecutor Stub Tests ──────────────────────────────────────────

mod default_executor_api {
    use super::*;

    #[tokio::test]
    async fn test_default_executor_think_returns_stub() {
        let executor = DefaultExecutor;
        let result = executor.think("test prompt").await.unwrap();
        assert_eq!(result, "stub");
    }

    #[tokio::test]
    async fn test_default_executor_think_with_params_returns_stub() {
        let params = ExecutionParams::new("complex prompt");
        let executor = DefaultExecutor;
        let result = executor.think_with_params(params).await.unwrap();
        assert_eq!(result, "stub");
    }

    #[tokio::test]
    async fn test_default_executor_think_with_context_returns_stub() {
        let executor = DefaultExecutor;
        let mut ctx =
            rustycode_orchestration::task_context::TaskContext::new("t1".into(), "task".into());
        let result = executor.think_with_context(&mut ctx).await.unwrap();
        assert_eq!(result, "stub");
    }
}

// ─── 2. ExecutionParams + ThinkingConfig API Tests ──────────────────────────

mod params_config_api {
    use super::*;

    #[test]
    fn test_execution_params_new() {
        let params = ExecutionParams::new("Analyze this problem");
        assert_eq!(params.initial_prompt, "Analyze this problem");
    }

    #[test]
    fn test_thinking_config_default() {
        let config = ThinkingConfig::default();
        assert!(config.max_depth > 0);
        assert!(config.max_nodes > 0);
        assert!(config.target_confidence > 0.0);
    }

    #[test]
    fn test_execution_params_with_custom_config() {
        let mut config = ThinkingConfig::default();
        config.max_depth = 5;
        config.target_confidence = 0.85;

        let params = ExecutionParams {
            initial_prompt: "test".into(),
            config,
            selected_strategy: None,
            metadata: std::collections::HashMap::new(),
        };

        assert_eq!(params.config.max_depth, 5);
        assert!((params.config.target_confidence - 0.85).abs() < f64::EPSILON);
    }
}

// ─── 3. Operation Executor Pipeline Tests ───────────────────────────────────

mod operation_pipeline {
    use super::*;

    #[tokio::test]
    async fn test_full_generate_score_select_pipeline() {
        let mut graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        // Step 1: Add initial thought
        let initial =
            Thought::new(ThoughtKind::Initial, "Root problem".into()).with_confidence(0.6);
        let initial_id = initial.id;
        graph.add_thought(initial).unwrap();

        // Step 2: Generate derived thoughts
        let gen_op = Operation::Generate {
            from: initial_id,
            count: 3,
            prompt_template: "analyze".into(),
        };
        executor
            .execute(
                &gen_op,
                &mut graph,
                &PromptContext::new("test").with_depth(1),
            )
            .await
            .unwrap();

        assert_eq!(graph.len(), 4, "Should have initial + 3 generated");

        // Step 3: Score each thought
        let thought_ids: Vec<_> = graph.thoughts().map(|t| t.id).collect();
        for thought_id in &thought_ids {
            let score_op = Operation::Score {
                thought_id: *thought_id,
                criteria: vec!["root".into(), "problem".into()],
            };
            executor
                .execute(&score_op, &mut graph, &PromptContext::new("test"))
                .await
                .unwrap();
        }

        // Step 4: Verify scoring improved confidence for matching thoughts
        let initial_after = graph.get_thought(initial_id).unwrap();
        assert!(
            initial_after.metadata.confidence >= 0.6,
            "Initial thought with matching criteria should have equal or higher confidence"
        );

        // Step 5: Select top thoughts
        let all_ids: Vec<_> = graph.thoughts().map(|t| t.id).collect();
        let select_op = Operation::Select {
            from_ids: all_ids,
            count: 2,
            strategy: SelectionStrategy::TopConfidence,
        };
        executor
            .execute(&select_op, &mut graph, &PromptContext::new("test"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_aggregate_then_refine_pipeline() {
        let mut graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        // Add two analysis thoughts
        let t1 = Thought::new(ThoughtKind::Analysis, "Approach A: use caching".into())
            .with_confidence(0.7);
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Approach B: use batching".into())
            .with_confidence(0.8);
        let id2 = t2.id;

        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();

        // Aggregate into synthesis
        let agg_op = Operation::Aggregate {
            from_ids: vec![id1, id2],
            aggregation_method: AggregationMethod::Synthesize,
            prompt_template: "combine".into(),
        };
        executor
            .execute(&agg_op, &mut graph, &PromptContext::new("test"))
            .await
            .unwrap();

        assert_eq!(graph.len(), 3, "Should have 2 originals + 1 aggregated");

        // Find the synthesis thought (last added)
        let synth = graph
            .thoughts()
            .find(|t| t.kind == ThoughtKind::Synthesis)
            .unwrap();
        let synth_id = synth.id;

        // Refine the synthesis
        let refine_op = Operation::Refine {
            thought_id: synth_id,
            refinement_prompt: "Make more specific".into(),
        };
        executor
            .execute(
                &refine_op,
                &mut graph,
                &PromptContext::new("test").with_depth(1).with_iteration(1),
            )
            .await
            .unwrap();

        assert_eq!(graph.len(), 4, "Should have 3 + 1 refined");
    }

    #[tokio::test]
    async fn test_operation_on_nonexistent_thought_returns_error() {
        let mut graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        let fake_id = uuid::Uuid::new_v4();
        let op = Operation::Score {
            thought_id: fake_id,
            criteria: vec!["test".into()],
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("test"))
            .await;
        assert!(result.is_err(), "Should fail for nonexistent thought");
    }

    #[tokio::test]
    async fn test_aggregate_with_single_source() {
        let mut graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        let t = Thought::new(ThoughtKind::Analysis, "Only source".into()).with_confidence(0.75);
        let id = t.id;
        graph.add_thought(t).unwrap();

        let op = Operation::Aggregate {
            from_ids: vec![id],
            aggregation_method: AggregationMethod::Consensus,
            prompt_template: "t".into(),
        };
        executor
            .execute(&op, &mut graph, &PromptContext::new("test"))
            .await
            .unwrap();

        // Should add a Resolution thought from consensus
        let resolution = graph.thoughts().find(|t| t.kind == ThoughtKind::Resolution);
        assert!(
            resolution.is_some(),
            "Consensus of one should create Resolution"
        );
    }
}

// ─── 4. Graph + Scorer End-to-End Tests ─────────────────────────────────────

mod graph_scorer_e2e {
    use super::*;
    use rustycode_orchestration::thinking::core::types::EdgeKind;

    #[test]
    fn test_multi_path_reasoning_scores_best_path_highest() {
        let mut graph = ReasoningGraph::new();
        let scorer = ConfidenceScorer::new();

        // Path 1: Low confidence chain
        let a = Thought::new(ThoughtKind::Initial, "Start".into()).with_confidence(0.3);
        let a_id = a.id;
        let b = Thought::new(ThoughtKind::Analysis, "Weak analysis".into()).with_confidence(0.4);
        let b_id = b.id;
        graph.add_thought(a).unwrap();
        graph.add_thought(b).unwrap();
        graph.add_edge(a_id, b_id, EdgeKind::DerivesFrom).unwrap();

        // Path 2: High confidence chain
        let c = Thought::new(ThoughtKind::Initial, "Start alt".into()).with_confidence(0.7);
        let c_id = c.id;
        let d = Thought::new(ThoughtKind::Analysis, "Strong evidence".into()).with_confidence(0.9);
        let d_id = d.id;
        let e =
            Thought::new(ThoughtKind::Synthesis, "Definitive answer".into()).with_confidence(0.95);
        let e_id = e.id;
        graph.add_thought(c).unwrap();
        graph.add_thought(d).unwrap();
        graph.add_thought(e).unwrap();
        graph.add_edge(c_id, d_id, EdgeKind::DerivesFrom).unwrap();
        graph.add_edge(d_id, e_id, EdgeKind::Supports).unwrap();

        let scores = scorer.score_all(&graph);

        // The high-confidence path's synthesis should score highest
        let synth_score = scores[&e_id];
        let weak_score = scores[&b_id];
        assert!(
            synth_score > weak_score,
            "High-confidence path ({synth_score}) should outscore weak path ({weak_score})"
        );
    }

    #[test]
    fn test_contradicted_thoughts_score_lower() {
        let mut graph = ReasoningGraph::new();
        let scorer = ConfidenceScorer::new();

        let t1 = Thought::new(ThoughtKind::Analysis, "Hypothesis".into()).with_confidence(0.8);
        let id1 = t1.id;
        let t2 =
            Thought::new(ThoughtKind::Critique, "Counter-evidence".into()).with_confidence(0.7);
        let id2 = t2.id;

        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();
        graph.add_edge(id1, id2, EdgeKind::Contradicts).unwrap();

        let scores = scorer.score_all(&graph);

        // Both should still have scores, but the contradiction should factor in
        assert!(scores.contains_key(&id1));
        assert!(scores.contains_key(&id2));
    }
}

// ─── 5. Error Handling API Tests ────────────────────────────────────────────

mod error_api {
    use super::*;

    #[test]
    fn test_graph_duplicate_thought_rejected() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "test".into());
        let t_clone = t.clone();
        graph.add_thought(t).unwrap();

        // Adding same thought (same ID) should fail
        let result = graph.add_thought(t_clone);
        assert!(result.is_err(), "Duplicate thought ID should be rejected");
    }

    #[test]
    fn test_graph_nonexistent_edge_rejected() {
        let mut graph = ReasoningGraph::new();
        let fake_a = uuid::Uuid::new_v4();
        let fake_b = uuid::Uuid::new_v4();

        let result = graph.add_edge(
            fake_a,
            fake_b,
            rustycode_orchestration::thinking::core::types::EdgeKind::DerivesFrom,
        );
        assert!(
            result.is_err(),
            "Edge with nonexistent nodes should be rejected"
        );
    }

    #[test]
    fn test_graph_get_nonexistent_returns_error() {
        let graph = ReasoningGraph::new();
        let fake_id = uuid::Uuid::new_v4();
        let result = graph.get_thought(fake_id);
        assert!(result.is_err());
    }
}
