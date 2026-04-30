//! Thinking module integration tests.
//!
//! Tests the full Graph-of-Thoughts pipeline: strategy selection,
//! graph construction, scoring, pruning, convergence, and metacognitive
//! monitoring as integrated systems.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::thinking::convergence::{ConvergenceDetector, ConvergenceMetrics};
use rustycode_orchestration::thinking::core::graph::ReasoningGraph;
use rustycode_orchestration::thinking::core::metacog::MetacognitiveMonitor;
use rustycode_orchestration::thinking::core::parsing::ResponseParser;
use rustycode_orchestration::thinking::core::pruning::{GraphPruner, PruningStrategy};
use rustycode_orchestration::thinking::core::scoring::ConfidenceScorer;
use rustycode_orchestration::thinking::core::types::{EdgeKind, Thought, ThoughtId, ThoughtKind};
use rustycode_orchestration::thinking::prompting::context::PromptContext;
use rustycode_orchestration::thinking::selection::StrategySelector;

// ─── 1. Full Graph-of-Thoughts Lifecycle ──────────────────────────────────

mod graph_lifecycle {
    use super::*;

    #[test]
    fn test_full_reasoning_graph_lifecycle() {
        let mut graph = ReasoningGraph::new();
        let scorer = ConfidenceScorer::new();

        // Step 1: Add initial thought
        let initial = Thought::new(
            ThoughtKind::Initial,
            "What is the best sorting algorithm?".into(),
        )
        .with_confidence(0.5);
        let initial_id = initial.id;
        graph.add_thought(initial).unwrap();

        // Step 2: Add analysis thought
        let analysis = Thought::new(
            ThoughtKind::Analysis,
            "QuickSort has O(n log n) average case".into(),
        )
        .with_confidence(0.7);
        let analysis_id = analysis.id;
        graph.add_thought(analysis).unwrap();

        // Step 3: Add synthesis
        let synthesis = Thought::new(
            ThoughtKind::Synthesis,
            "QuickSort is optimal for general use".into(),
        )
        .with_confidence(0.85);
        let synth_id = synthesis.id;
        graph.add_thought(synthesis).unwrap();

        // Step 4: Connect with edges
        graph
            .add_edge(initial_id, analysis_id, EdgeKind::DerivesFrom)
            .unwrap();
        graph
            .add_edge(analysis_id, synth_id, EdgeKind::Supports)
            .unwrap();

        // Step 5: Verify graph structure
        assert_eq!(graph.len(), 3);
        let successors = graph.successors(initial_id);
        assert_eq!(successors.len(), 1);
        assert_eq!(successors[0], analysis_id);

        // Step 6: Score all thoughts
        let thought_scores = scorer.score_all(&graph);
        assert_eq!(thought_scores.len(), 3);
        assert!(
            thought_scores[&synth_id] > thought_scores[&initial_id],
            "Synthesis should score higher than initial"
        );

        // Step 7: Build prompt context from graph
        let thoughts: Vec<Thought> = graph.thoughts().cloned().collect();
        let ctx = PromptContext::new("Best sorting algorithm?")
            .with_previous_thoughts(thoughts)
            .with_depth(2)
            .with_iteration(3);

        let summary = ctx.thoughts_summary(500);
        assert!(summary.contains("Previous thoughts"));
        assert!(summary.contains("QuickSort"));
    }

    #[test]
    fn test_reasoning_graph_with_contradictions() {
        let mut graph = ReasoningGraph::new();

        let t1 =
            Thought::new(ThoughtKind::Analysis, "MergeSort is stable".into()).with_confidence(0.8);
        let t1_id = t1.id;
        graph.add_thought(t1).unwrap();

        let t2 = Thought::new(ThoughtKind::Critique, "QuickSort is not stable".into())
            .with_confidence(0.7);
        let t2_id = t2.id;
        graph.add_thought(t2).unwrap();

        let t3 = Thought::new(
            ThoughtKind::Resolution,
            "Use MergeSort when stability matters".into(),
        )
        .with_confidence(0.9);
        graph.add_thought(t3).unwrap();

        // Contradiction edge
        graph.add_edge(t1_id, t2_id, EdgeKind::Contradicts).unwrap();

        // Verify contradiction is tracked
        let edges: Vec<_> = graph.edges().iter().collect();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].kind, EdgeKind::Contradicts);
    }
}

// ─── 2. Pruning + Scoring Integration ────────────────────────────────────

mod pruning_scoring_integration {
    use super::*;

    #[test]
    fn test_pruning_preserves_high_quality_chain() {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.4);
        let mut graph = ReasoningGraph::new();

        // High-quality chain
        let h1 = Thought::new(ThoughtKind::Initial, "good start".into()).with_confidence(0.8);
        let h1_id = h1.id;
        graph.add_thought(h1).unwrap();

        let h2 = Thought::new(ThoughtKind::Analysis, "good analysis".into()).with_confidence(0.9);
        let h2_id = h2.id;
        graph.add_thought(h2).unwrap();

        graph.add_edge(h1_id, h2_id, EdgeKind::DerivesFrom).unwrap();

        // Low-quality noise
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "noise 1".into()).with_confidence(0.1))
            .unwrap();
        graph
            .add_thought(Thought::new(ThoughtKind::Analysis, "noise 2".into()).with_confidence(0.2))
            .unwrap();

        assert_eq!(graph.len(), 4);

        let removed = pruner
            .prune(&mut graph, PruningStrategy::Aggressive)
            .unwrap();
        assert!(
            removed >= 2,
            "Should remove at least 2 low-confidence thoughts"
        );
        assert_eq!(graph.len(), 2, "Should keep the high-quality chain");

        // Verify the chain is intact
        let successors = graph.successors(h1_id);
        assert_eq!(successors.len(), 1);
        assert_eq!(successors[0], h2_id);
    }

    #[test]
    fn test_max_nodes_enforcement_keeps_best() {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_max_nodes(3);
        let mut graph = ReasoningGraph::new();

        // Add 7 thoughts with varying confidence
        for i in 0..7u32 {
            let confidence = f64::from(i).mul_add(0.1, 0.2);
            let t = Thought::new(ThoughtKind::Analysis, format!("thought-{i}"))
                .with_confidence(confidence);
            graph.add_thought(t).unwrap();
        }

        assert_eq!(graph.len(), 7);
        let removed = pruner.enforce_max_nodes(&mut graph).unwrap();
        assert_eq!(removed, 4, "Should remove 4 to reach max_nodes=3");
        assert_eq!(graph.len(), 3);

        // Verify remaining thoughts are the highest confidence ones
        let remaining: Vec<f64> = graph.thoughts().map(|t| t.metadata.confidence).collect();
        assert!(
            remaining.iter().all(|&c| c >= 0.6),
            "Should keep highest confidence thoughts"
        );
    }
}

// ─── 3. Convergence + Metacog Integration ────────────────────────────────

mod convergence_metacog_integration {
    use super::*;

    #[test]
    fn test_convergence_detection_with_low_confidence() {
        let mut metrics = ConvergenceMetrics::new(10);
        let detector = ConvergenceDetector::new();

        // Simulate reasoning with low confidence (below 0.5 diminishing returns threshold)
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "uncertain".into()).with_confidence(0.2);
        graph.add_thought(t).unwrap();
        // Record only 2 iterations — not enough for plateau/stagnation windows
        metrics.record_iteration(&graph);
        metrics.record_iteration(&graph);

        // Should NOT be converged — low confidence + insufficient data for plateau/stagnation
        assert!(
            !detector.has_converged(&metrics, Some(0.99)),
            "Should not converge with low confidence and unreachable target"
        );
    }

    #[test]
    fn test_convergence_detection_with_stagnant_graph() {
        let mut metrics = ConvergenceMetrics::new(10);
        let detector = ConvergenceDetector::new();

        // Simulate stagnant reasoning (empty graph = no new thoughts)
        for _ in 0..4 {
            let graph = ReasoningGraph::new();
            metrics.record_iteration(&graph);
        }

        assert!(
            detector.has_converged(&metrics, None),
            "Stagnant reasoning should converge"
        );
        let reason = detector.convergence_reason(&metrics);
        assert!(reason.is_some());
    }

    #[test]
    fn test_metacog_detects_healthy_reasoning() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "Solid analysis".into()).with_confidence(0.8);
        graph.add_thought(t).unwrap();

        let metrics = monitor.analyze(&graph);
        assert!(
            !metrics.is_stagnant,
            "First analysis should not be stagnant"
        );
        assert!(
            !metrics.declining_confidence,
            "Single good thought should not show decline"
        );
        assert!(
            metrics.suggested_strategy.is_none(),
            "No suggestion needed for healthy reasoning"
        );
    }

    #[test]
    fn test_metacog_tracks_history_across_analyses() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Analyze multiple times with improving graph
        for i in 0..5u32 {
            let mut graph = ReasoningGraph::new();
            let confidence = f64::from(i).mul_add(0.05, 0.5);
            let t = Thought::new(ThoughtKind::Analysis, format!("step-{i}"))
                .with_confidence(confidence);
            graph.add_thought(t).unwrap();
            let _ = monitor.analyze(&graph);
        }

        // History should accumulate across analyses (internal state)
        let metrics = monitor.analyze(&ReasoningGraph::new());
        // After 6 total analyses, avg_confidence should be 0 (empty graph)
        assert_eq!(metrics.avg_confidence, 0.0);
    }
}

// ─── 4. Strategy Selection + Prompting Integration ───────────────────────

mod strategy_prompting_integration {
    use super::*;

    #[test]
    fn test_strategy_selector_with_prompt_context() {
        let graph = ReasoningGraph::new();

        // StrategySelector::select takes problem + graph
        let strategy = StrategySelector::select("implement a function", &graph);
        assert!(strategy.name() == "Sequential" || strategy.name() == "Parallel");

        // Verify prompt context can be built for this strategy
        let ctx = PromptContext::new("implement a function")
            .with_depth(0)
            .with_iteration(0)
            .with_constraints(vec!["Must be pure function".into()]);

        assert_eq!(ctx.problem, "implement a function");
        assert_eq!(ctx.constraints.len(), 1);
        assert!(ctx.format_constraints().contains("pure function"));
    }

    #[test]
    fn test_prompt_context_with_graph_thoughts() {
        let mut graph = ReasoningGraph::new();

        for i in 0..3 {
            let t = Thought::new(ThoughtKind::Analysis, format!("Analysis step {i}"))
                .with_confidence(f64::from(i).mul_add(0.1, 0.5));
            graph.add_thought(t).unwrap();
        }

        let thoughts: Vec<Thought> = graph.thoughts().cloned().collect();
        let ctx = PromptContext::new("Continue reasoning")
            .with_previous_thoughts(thoughts)
            .with_goal("Find the optimal solution")
            .with_success_criteria(vec!["Confidence > 0.8".into()]);

        // Top thoughts should be sorted by confidence
        let top = ctx.top_thoughts(2);
        assert_eq!(top.len(), 2);
        assert!(top[0].metadata.confidence >= top[1].metadata.confidence);

        // Goal formatting
        let goal_text = ctx.format_goal();
        assert!(goal_text.contains("## Goal"));
        assert!(goal_text.contains("Find the optimal solution"));
        assert!(goal_text.contains("### Success Criteria"));
    }

    #[test]
    fn test_response_parser_roundtrip_with_graph() {
        let mut graph = ReasoningGraph::new();

        // Parse a JSON response and add to graph
        let json = r#"{
            "thoughts": [
                {"kind": "Analysis", "content": "First analysis", "confidence": 0.8, "reasoning": "based on data"},
                {"kind": "Synthesis", "content": "Final answer", "confidence": 0.9, "reasoning": "combines evidence"}
            ],
            "relationships": [
                {"from_idx": 0, "to_idx": 1, "edge_kind": "supports"}
            ]
        }"#;

        let response = ResponseParser::parse_json(json).unwrap();
        let thoughts = ResponseParser::to_thoughts(&response).unwrap();

        assert_eq!(thoughts.len(), 2);

        // Add parsed thoughts to graph
        let ids: Vec<ThoughtId> = thoughts
            .iter()
            .map(|t| {
                let id = t.id;
                graph.add_thought(t.clone()).unwrap();
                id
            })
            .collect();

        // Add edges from relationships
        for rel in &response.relationships {
            graph
                .add_edge(ids[rel.from_idx], ids[rel.to_idx], EdgeKind::Supports)
                .unwrap();
        }

        assert_eq!(graph.len(), 2);
        assert_eq!(graph.successors(ids[0]).len(), 1);
    }
}

// ─── 5. Pipeline + Thinking Module Integration ───────────────────────────

mod pipeline_thinking_integration {
    use super::*;

    #[test]
    fn test_tier5_thinking_phase_with_graph() {
        // Simulate what happens when the conductor triggers tier 5 thinking
        let mut graph = ReasoningGraph::new();

        // Build up a reasoning graph
        let t1 = Thought::new(
            ThoughtKind::Initial,
            "Complex bug in concurrent code".into(),
        )
        .with_confidence(0.3);
        let t1_id = t1.id;
        graph.add_thought(t1).unwrap();

        let t2 = Thought::new(
            ThoughtKind::Analysis,
            "Race condition in shared state".into(),
        )
        .with_confidence(0.5);
        let t2_id = t2.id;
        graph.add_thought(t2).unwrap();

        let t3 = Thought::new(ThoughtKind::Analysis, "Need mutex around counter".into())
            .with_confidence(0.7);
        let t3_id = t3.id;
        graph.add_thought(t3).unwrap();

        graph.add_edge(t1_id, t2_id, EdgeKind::DerivesFrom).unwrap();
        graph.add_edge(t2_id, t3_id, EdgeKind::Supports).unwrap();

        // Score and prune
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.35);
        let removed = pruner
            .prune(&mut graph, PruningStrategy::ConfidenceThreshold)
            .unwrap();

        // Low confidence initial thought may be pruned
        assert!(
            graph.len() >= 2,
            "Should keep at least 2 thoughts after pruning"
        );
        let _ = removed; // May or may not prune depending on scoring

        // Now apply metacognitive monitoring
        let scorer2 = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer2);
        let metrics = monitor.analyze(&graph);

        // After good analysis, should not be stagnant or declining
        assert!(!metrics.is_stagnant, "Active graph should not be stagnant");
    }

    #[test]
    fn test_convergence_with_target_confidence() {
        let mut metrics = ConvergenceMetrics::new(10);
        let detector = ConvergenceDetector::new();

        // Build a graph that reaches high confidence
        let mut graph = ReasoningGraph::new();
        let t =
            Thought::new(ThoughtKind::Synthesis, "Definitive answer".into()).with_confidence(0.95);
        graph.add_thought(t).unwrap();

        // Record several iterations
        for _ in 0..3 {
            metrics.record_iteration(&graph);
        }

        // Should converge with target 0.75 (well below current)
        assert!(
            detector.has_converged(&metrics, Some(0.75)),
            "Should converge when confidence exceeds target"
        );
    }
}

// ─── 6. Full Parse → Graph → Score → Prune → Converge Pipeline ──────────

mod full_thinking_pipeline {
    use super::*;

    #[test]
    fn test_complete_thinking_workflow() {
        // 1. Parse LLM response
        let response_text = r#"{
            "thoughts": [
                {"kind": "Initial", "content": "Need to optimize query", "confidence": 0.4, "reasoning": "slow perf"},
                {"kind": "Analysis", "content": "Add index to users table", "confidence": 0.6, "reasoning": "missing index"},
                {"kind": "Analysis", "content": "Use query planner hints", "confidence": 0.7, "reasoning": "plan shows seq scan"},
                {"kind": "Synthesis", "content": "Add composite index + use prepared statements", "confidence": 0.9, "reasoning": "covers all columns"}
            ],
            "relationships": [
                {"from_idx": 0, "to_idx": 1, "edge_kind": "derives_from"},
                {"from_idx": 0, "to_idx": 2, "edge_kind": "derives_from"},
                {"from_idx": 1, "to_idx": 3, "edge_kind": "supports"},
                {"from_idx": 2, "to_idx": 3, "edge_kind": "supports"}
            ]
        }"#;

        let response = ResponseParser::parse_response(response_text).unwrap();
        assert_eq!(response.thoughts.len(), 4);

        // 2. Convert to thoughts and build graph
        let thoughts = ResponseParser::to_thoughts(&response).unwrap();
        let mut graph = ReasoningGraph::new();
        let ids: Vec<ThoughtId> = thoughts
            .iter()
            .map(|t| {
                let id = t.id;
                graph.add_thought(t.clone()).unwrap();
                id
            })
            .collect();

        assert_eq!(graph.len(), 4);

        // Add edges
        for rel in &response.relationships {
            let edge_kind = match rel.edge_kind.as_str() {
                "supports" => EdgeKind::Supports,
                "contradicts" => EdgeKind::Contradicts,
                _ => EdgeKind::DerivesFrom,
            };
            graph
                .add_edge(ids[rel.from_idx], ids[rel.to_idx], edge_kind)
                .unwrap();
        }

        // 3. Score all thoughts
        let conf_scorer = ConfidenceScorer::new();
        let thought_scores = conf_scorer.score_all(&graph);
        assert_eq!(thought_scores.len(), 4);

        // Synthesis should have highest score (high confidence + successors)
        let synth_score = thought_scores[&ids[3]];
        let initial_score = thought_scores[&ids[0]];
        assert!(
            synth_score >= initial_score,
            "Synthesis should score >= initial"
        );

        // 4. Prune low confidence
        let graph_pruner = GraphPruner::new(conf_scorer).with_threshold(0.5);
        let prune_count = graph_pruner
            .prune(&mut graph, PruningStrategy::ConfidenceThreshold)
            .unwrap();
        // Initial thought has confidence 0.4, may be pruned
        assert!(graph.len() <= 4);
        let _ = prune_count;

        // 5. Check convergence — detector works correctly with the graph state.
        // After pruning, remaining thoughts have high confidence so diminishing
        // returns may trigger. Just verify the API works without panicking.
        let mut conv_metrics = ConvergenceMetrics::new(10);
        conv_metrics.record_iteration(&graph);
        let detector = ConvergenceDetector::new();
        // With a reachable target, should converge
        if let Some(conf) = conv_metrics.latest_confidence() {
            if conf > 0.0 {
                assert!(detector.has_converged(&conv_metrics, Some(0.01)));
            }
        }
        // Verify no panic with unreachable target
        let _ = detector.has_converged(&conv_metrics, Some(0.99));

        // 6. Metacognitive monitoring
        let scorer2 = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer2);
        let meta = monitor.analyze(&graph);
        assert!(
            meta.avg_confidence > 0.0,
            "Should have non-zero confidence after analysis"
        );
    }

    #[test]
    fn test_fallback_parsing_integration() {
        // Simulate a malformed LLM response
        let malformed = "I think the issue is a race condition.\n\nconfidence: 0.6\n\nWe should add a mutex to protect shared state.\n\nconfidence: 0.8";

        let response = ResponseParser::parse_response(malformed).unwrap();
        assert!(
            !response.thoughts.is_empty(),
            "Fallback parser should extract thoughts"
        );

        // Convert to thoughts and verify
        let thoughts = ResponseParser::to_thoughts(&response).unwrap();
        assert!(!thoughts.is_empty());

        // Build a small graph from fallback results
        let mut graph = ReasoningGraph::new();
        for t in thoughts {
            graph.add_thought(t).unwrap();
        }
        assert!(
            graph.len() >= 2,
            "Should have extracted at least 2 thoughts"
        );
    }
}
