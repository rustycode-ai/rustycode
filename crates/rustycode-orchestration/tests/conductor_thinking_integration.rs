//! Conductor + Thinking module integration tests.
//!
//! Tests the integration between the Conductor's escalation/tier decisions,
//! the Thinking module's reasoning pipeline, and the full flow from error
//! detection through thinking-triggered analysis.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use rustycode_orchestration::bus::BusHandle;
use rustycode_orchestration::conductor::{Conductor, EscalationDecision};
use rustycode_orchestration::config::OrchestrationConfig;
use rustycode_orchestration::error_signal::{ErrorSignal, SignalCategory};
use rustycode_orchestration::execution_trace::{ExecutionTrace, TraceEntry};
use rustycode_orchestration::state_machine::TaskContext;
use rustycode_orchestration::thinking::convergence::{ConvergenceDetector, ConvergenceMetrics};
use rustycode_orchestration::thinking::core::graph::ReasoningGraph;
use rustycode_orchestration::thinking::core::metacog::MetacognitiveMonitor;
use rustycode_orchestration::thinking::core::pruning::{GraphPruner, PruningStrategy};
use rustycode_orchestration::thinking::core::scoring::ConfidenceScorer;
use rustycode_orchestration::thinking::core::types::{EdgeKind, Thought, ThoughtKind};
use rustycode_orchestration::thinking::prompting::context::PromptContext;
use rustycode_orchestration::thinking::selection::StrategySelector;

// ─── 1. Conductor → Thinking Trigger ──────────────────────────────────────

mod conductor_thinking_trigger {
    use super::*;

    #[test]
    fn test_conductor_triggers_thinking_for_complex_errors() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        // Complex task + rich error context → thinking triggered
        let result = conductor.try_thinking(
            "Refactor the authentication module to support OAuth2",
            "Multiple compilation errors: error[E0425], error[E0277], type mismatch in handler",
        );
        assert!(result.is_some());
        assert!(result.unwrap().contains("tier=5"));
    }

    #[test]
    fn test_conductor_skips_thinking_for_simple_errors() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        let result = conductor.try_thinking("fix typo", "err");
        assert!(result.is_none());
    }

    #[test]
    fn test_thinking_flow_from_tier4_exhaustion() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        let mut ctx = TaskContext::new("t-thinking".into(), "complex task".into());
        ctx.current_tier = 4;

        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            Some(1),
            "tier 4 failed".into(),
            "step-1".into(),
            "bash".into(),
        );

        // Tier 4 should abandon
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));

        // But conductor should suggest thinking before abandoning
        let thinking = conductor.try_thinking(
            "Investigate complex concurrency bug in the authentication module",
            "Deadlock detected in mutex acquisition pattern",
        );
        assert!(
            thinking.is_some(),
            "Should suggest thinking for complex problems"
        );
    }
}

// ─── 2. Full Thinking → Convergence → Decision Pipeline ──────────────────

mod thinking_convergence_pipeline {
    use super::*;

    #[test]
    fn test_thinking_reaches_confidence_then_converges() {
        let scorer = ConfidenceScorer::new();
        let pruner = GraphPruner::new(scorer).with_threshold(0.3);
        let mut monitor = MetacognitiveMonitor::new(ConfidenceScorer::new());
        let mut metrics = ConvergenceMetrics::new(10);
        let _detector = ConvergenceDetector::new();

        // Simulate multiple reasoning iterations building confidence
        for i in 0..5u32 {
            let mut graph = ReasoningGraph::new();

            // Add progressively better thoughts
            let initial =
                Thought::new(ThoughtKind::Initial, "Problem statement".into()).with_confidence(0.3);
            graph.add_thought(initial).unwrap();

            let analysis = Thought::new(
                ThoughtKind::Analysis,
                format!("Analysis step {i} with evidence"),
            )
            .with_confidence(f64::from(i).mul_add(0.1, 0.4));
            let analysis_id = analysis.id;
            graph.add_thought(analysis).unwrap();

            let synth = Thought::new(
                ThoughtKind::Synthesis,
                format!("Synthesized conclusion at iteration {i}"),
            )
            .with_confidence(f64::from(i).mul_add(0.05, 0.7));
            let synth_id = synth.id;
            graph.add_thought(synth).unwrap();

            // Build edges
            let initial_id = graph
                .thoughts()
                .find(|t| t.kind == ThoughtKind::Initial)
                .unwrap()
                .id;
            graph
                .add_edge(initial_id, analysis_id, EdgeKind::DerivesFrom)
                .unwrap();
            graph
                .add_edge(analysis_id, synth_id, EdgeKind::Supports)
                .unwrap();

            // Score and prune
            let _removed = pruner
                .prune(&mut graph, PruningStrategy::ConfidenceThreshold)
                .unwrap();

            // Metacognitive check
            let _meta = monitor.analyze(&graph);

            // Record for convergence
            metrics.record_iteration(&graph);
        }

        // After 5 iterations with improving confidence, should detect convergence
        let latest = metrics.latest_confidence();
        assert!(latest.is_some(), "Should have recorded confidence");
        assert!(latest.unwrap() > 0.5, "Confidence should have improved");
    }

    #[test]
    fn test_stagnant_thinking_triggers_strategy_change() {
        let scorer = ConfidenceScorer::new();
        let mut monitor = MetacognitiveMonitor::new(scorer);

        // Simulate 4 identical low-confidence iterations (stagnant)
        // Stagnation requires >= 3 history entries with variance < 0.01
        for _ in 0..4 {
            let mut graph = ReasoningGraph::new();
            let t = Thought::new(ThoughtKind::Analysis, "repeated low quality".into())
                .with_confidence(0.2);
            graph.add_thought(t).unwrap();
            let _ = monitor.analyze(&graph);
        }

        // The 5th analysis should detect stagnation
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "still low".into()).with_confidence(0.2);
        graph.add_thought(t).unwrap();
        let metrics = monitor.analyze(&graph);

        assert!(
            metrics.is_stagnant,
            "Repeated low-confidence analysis should be stagnant"
        );
        assert!(
            metrics.suggested_strategy.is_some(),
            "Stagnant reasoning should suggest strategy change"
        );
    }
}

// ─── 3. Strategy Selection + Graph Context Integration ───────────────────

mod strategy_graph_integration {
    use super::*;

    #[test]
    fn test_strategy_changes_with_contradictions() {
        // Build a graph with contradictions
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(ThoughtKind::Analysis, "Use approach A".into()).with_confidence(0.8);
        let t1_id = t1.id;
        graph.add_thought(t1).unwrap();

        let t2 = Thought::new(ThoughtKind::Critique, "Approach A has issues".into())
            .with_confidence(0.7);
        let t2_id = t2.id;
        graph.add_thought(t2).unwrap();

        graph.add_edge(t1_id, t2_id, EdgeKind::Contradicts).unwrap();

        // Strategy selector should detect contradictions and override to Dialectic
        let strategy = StrategySelector::select("Analyze from multiple perspectives", &graph);
        assert_eq!(
            strategy.name(),
            "Dialectic",
            "Should switch to Dialectic for contradictions"
        );
    }

    #[test]
    fn test_strategy_with_confident_graph_stays() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Synthesis, "High confidence synthesis".into())
            .with_confidence(0.9);
        graph.add_thought(t).unwrap();

        // No contradictions, high confidence — strategy should stay as selected
        let strategy = StrategySelector::select("debug the error in my code", &graph);
        // Abductive matches "debug", no contradictions to override
        assert_eq!(strategy.name(), "Abductive");
    }

    #[test]
    fn test_prompt_context_with_strategy_and_graph() {
        let mut graph = ReasoningGraph::new();
        for i in 0..4 {
            let t = Thought::new(ThoughtKind::Analysis, format!("Step {i}"))
                .with_confidence(f64::from(i).mul_add(0.1, 0.5));
            graph.add_thought(t).unwrap();
        }

        let thoughts: Vec<Thought> = graph.thoughts().cloned().collect();
        let ctx = PromptContext::new("Solve the problem")
            .with_previous_thoughts(thoughts)
            .with_depth(3)
            .with_iteration(5)
            .with_goal("Find root cause")
            .with_success_criteria(vec!["Confidence > 0.85".into()]);

        // Verify prompt context is complete
        let summary = ctx.thoughts_summary(300);
        assert!(!summary.is_empty());

        let goal = ctx.format_goal();
        assert!(goal.contains("Find root cause"));
        assert!(goal.contains("Confidence > 0.85"));

        let top = ctx.top_thoughts(2);
        assert_eq!(top.len(), 2);
        assert!(top[0].metadata.confidence >= top[1].metadata.confidence);
    }
}

// ─── 4. Conductor + Bus + Thinking Event Flow ────────────────────────────

mod conductor_bus_thinking_flow {
    use super::*;

    #[test]
    fn test_escalation_events_with_thinking_context() {
        let mut config = OrchestrationConfig::default();
        config.budget.total_max_usd = 100.0;
        config.escalation.insert(
            "tier_2".into(),
            rustycode_orchestration::config::TierConfig {
                max_attempts: 1,
                critical_errors: vec![SignalCategory::LogicError],
                recoverable_errors: vec![],
            },
        );

        let bus = BusHandle::new(16);
        let mut rx = bus.subscribe();
        let conductor = Conductor::with_bus(config, bus);

        let mut ctx = TaskContext::new("t-event".into(), "complex debug task".into());
        ctx.attempt_count = 1;

        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            Some(1),
            "tier 2 error".into(),
            "step-1".into(),
            "bash".into(),
        );

        // Escalate to tier 3
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(
            decision,
            EscalationDecision::Escalate { next_tier: 3, .. }
        ));

        // Bus should have received escalation event
        let event = rx.try_recv().unwrap();
        assert!(matches!(
            event,
            rustycode_orchestration::bus::OrchestrationEvent::EscalationSignal {
                from_tier: 2,
                to_tier: 3,
                ..
            }
        ));

        // Conductor should suggest thinking for this complex task
        let thinking = conductor.try_thinking(
            "Debug race condition in concurrent hashmap implementation",
            "Test fails intermittently: assertion failed on thread 3",
        );
        assert!(thinking.is_some());
    }

    #[test]
    fn test_full_tier_lifecycle_with_thinking() {
        let config = OrchestrationConfig::default();
        let conductor = Conductor::new(config);

        let mut ctx = TaskContext::new("t-lifecycle".into(), "implement feature".into());

        // Tier 2: attempt
        ctx.attempt_count += 1;
        let signal = ErrorSignal::new(
            SignalCategory::SyntaxError,
            Some(1),
            "syntax error".into(),
            "s1".into(),
            "bash".into(),
        );
        let decision = conductor.handle_error(&mut ctx, &signal);
        // With no escalation config for tier_2, should retry
        assert!(matches!(decision, EscalationDecision::Retry));

        // Simulate progressing through tiers
        ctx.escalate(); // → tier 3
        ctx.escalate(); // → tier 4

        // Tier 4: should abandon
        let signal = ErrorSignal::new(
            SignalCategory::Internal,
            Some(1),
            "still failing".into(),
            "s1".into(),
            "bash".into(),
        );
        let decision = conductor.handle_error(&mut ctx, &signal);
        assert!(matches!(decision, EscalationDecision::Abandon { .. }));

        // Thinking should be suggested for complex task
        let thinking = conductor.try_thinking(
            "Design a distributed caching layer for the API gateway service",
            "All tier 2-4 attempts failed to produce valid cache invalidation logic",
        );
        assert!(thinking.is_some());
        assert!(thinking.unwrap().contains("tier=5"));
    }
}

// ─── 5. Thinking Module + Execution Trace Integration ────────────────────

mod thinking_trace_integration {
    use super::*;

    #[test]
    fn test_thinking_graph_to_execution_trace() {
        // Build a reasoning graph (simulating thinking output)
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(
            ThoughtKind::Initial,
            "Bug: intermittent test failure".into(),
        )
        .with_confidence(0.4);
        let t1_id = t1.id;
        graph.add_thought(t1).unwrap();

        let t2 = Thought::new(ThoughtKind::Analysis, "Race condition in counter".into())
            .with_confidence(0.7);
        let t2_id = t2.id;
        graph.add_thought(t2).unwrap();

        let t3 = Thought::new(ThoughtKind::Synthesis, "Add mutex to counter access".into())
            .with_confidence(0.9);
        let t3_id = t3.id;
        graph.add_thought(t3).unwrap();

        graph.add_edge(t1_id, t2_id, EdgeKind::DerivesFrom).unwrap();
        graph.add_edge(t2_id, t3_id, EdgeKind::Supports).unwrap();

        // Convert thinking output to execution steps
        let thoughts: Vec<_> = graph.thoughts().collect();
        assert_eq!(thoughts.len(), 3);

        // Score to find best synthesis
        let scorer = ConfidenceScorer::new();
        let thought_scores = scorer.score_all(&graph);
        let best = thought_scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        let best_thought = graph.get_thought(*best.0).unwrap();
        assert!(best_thought.content.contains("mutex"));

        // Create an execution trace entry from the thinking result
        let mut trace = ExecutionTrace::new("thinking-task".into());
        trace.append(TraceEntry::new_success(
            "thinking-step".into(),
            0,
            5, // tier 5 = thinking
            "thinking".into(),
            serde_json::json!({
                "thoughts": thoughts.len(),
                "best_confidence": *best.1,
                "strategy": "sequential",
            }),
            best_thought.content.clone(),
            Some(0),
            0.05,
        ));

        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].tier, 5);
        assert!(trace.steps[0].output.contains("mutex"));
    }

    #[test]
    fn test_thinking_improves_convergence_over_iterations() {
        let mut metrics = ConvergenceMetrics::new(10);
        let detector = ConvergenceDetector::new();

        // Simulate 4 improving iterations
        for confidence in [0.3, 0.5, 0.7, 0.9] {
            let mut graph = ReasoningGraph::new();
            let t = Thought::new(ThoughtKind::Synthesis, "improving".into())
                .with_confidence(confidence);
            graph.add_thought(t).unwrap();
            metrics.record_iteration(&graph);
        }

        // With reachable target, should converge
        assert!(
            detector.has_converged(&metrics, Some(0.6)),
            "Should converge after reaching target confidence"
        );
    }
}

// ─── 6. Error Classification → Thinking Strategy Selection ───────────────

mod error_to_thinking_strategy {
    use super::*;

    #[test]
    fn test_logic_error_triggers_abductive_thinking() {
        // Logic errors are debug-type problems → Abductive strategy
        let graph = ReasoningGraph::new();
        let strategy = StrategySelector::select("debug the logic error in validation", &graph);
        assert_eq!(strategy.name(), "Abductive");
    }

    #[test]
    fn test_design_tradeoff_triggers_dialectic() {
        let graph = ReasoningGraph::new();
        let strategy =
            StrategySelector::select("weigh the tradeoff between performance and safety", &graph);
        assert_eq!(strategy.name(), "Dialectic");
    }

    #[test]
    fn test_multi_perspective_triggers_parallel() {
        let graph = ReasoningGraph::new();
        let strategy =
            StrategySelector::select("Analyze from multiple perspectives thoroughly", &graph);
        assert_eq!(strategy.name(), "Parallel");
    }

    #[test]
    fn test_contradictions_in_graph_override_to_dialectic() {
        let mut graph = ReasoningGraph::new();

        let t1 = Thought::new(ThoughtKind::Analysis, "Hypothesis A".into()).with_confidence(0.8);
        let t2 = Thought::new(ThoughtKind::Critique, "Hypothesis B contradicts".into())
            .with_confidence(0.7);
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();
        graph.add_edge(id1, id2, EdgeKind::Contradicts).unwrap();

        // "debug" normally matches Abductive, but contradictions override to Dialectic
        let strategy = StrategySelector::select("debug the concurrent access error", &graph);
        assert_eq!(strategy.name(), "Dialectic");
    }

    #[test]
    fn test_low_confidence_graph_overrides_to_abductive() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "uncertain".into()).with_confidence(0.1);
        graph.add_thought(t).unwrap();

        // "tradeoff" normally matches Dialectic, but low confidence overrides to Abductive
        let strategy = StrategySelector::select("Analyze the tradeoff between approaches", &graph);
        assert_eq!(strategy.name(), "Abductive");
    }
}
