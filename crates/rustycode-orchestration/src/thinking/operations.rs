//! Strategy-specific operations for thought generation and refinement

use crate::thinking::core::error::Result;
use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::scoring::ConfidenceScorer;
use crate::thinking::core::types::{
    AggregationMethod, Operation, SelectionStrategy, Thought, ThoughtId, ThoughtKind,
};
use crate::thinking::prompting::PromptContext;

/// Executes operations on the reasoning graph
pub struct OperationExecutor {
    scorer: ConfidenceScorer,
}

impl OperationExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            scorer: ConfidenceScorer::new(),
        }
    }

    /// Execute a specific operation on the graph
    ///
    /// # Errors
    /// Returns an error if the operation fails (e.g., thought not found).
    #[allow(clippy::unused_async)]
    pub async fn execute(
        &self,
        operation: &Operation,
        graph: &mut ReasoningGraph,
        context: &PromptContext,
    ) -> Result<()> {
        match operation {
            Operation::Generate {
                from,
                count,
                prompt_template: _,
            } => self.execute_generate(*from, *count, graph, context),
            Operation::Aggregate {
                from_ids,
                aggregation_method,
                prompt_template: _,
            } => self.execute_aggregate(from_ids, *aggregation_method, graph),
            Operation::Score {
                thought_id,
                criteria,
            } => self.execute_score(*thought_id, criteria, graph),
            Operation::Refine {
                thought_id,
                refinement_prompt: _,
            } => self.execute_refine(*thought_id, graph, context),
            Operation::Select {
                from_ids,
                count,
                strategy,
            } => self.execute_select_internal(from_ids, *count, *strategy, graph),
        }
    }

    /// Generate new thoughts from a source thought
    ///
    /// # Errors
    /// Returns an error if the source thought is not found.
    #[allow(clippy::unused_self)]
    fn execute_generate(
        &self,
        from_id: ThoughtId,
        count: usize,
        graph: &mut ReasoningGraph,
        context: &PromptContext,
    ) -> Result<()> {
        let (source_content_preview, source_confidence) = {
            let source = graph.get_thought(from_id)?;
            let end = source.content.len().min(50);
            let end = source.content.floor_char_boundary(end);
            let preview = source.content[..end].to_string();
            let conf = source.metadata.confidence;
            (preview, conf)
        };

        for i in 0..count {
            let kind = match i % 3 {
                0 => ThoughtKind::Analysis,
                1 => ThoughtKind::Refinement,
                _ => ThoughtKind::Synthesis,
            };

            let new_thought = Thought::new(
                kind,
                format!(
                    "Generated from: {}. Iteration depth: {}",
                    source_content_preview, context.current_depth
                ),
            )
            .with_confidence(source_confidence * 0.9);

            graph.add_thought(new_thought)?;
        }

        tracing::debug!("Generated {count} thoughts from {from_id}");
        Ok(())
    }

    /// Aggregate multiple thoughts into synthesis
    ///
    /// # Errors
    /// Returns an error if any source thought is not found.
    #[allow(clippy::cast_precision_loss)]
    #[allow(clippy::unused_self)]
    fn execute_aggregate(
        &self,
        from_ids: &[ThoughtId],
        method: AggregationMethod,
        graph: &mut ReasoningGraph,
    ) -> Result<()> {
        if from_ids.is_empty() {
            return Ok(());
        }

        let mut sources = Vec::new();
        for id in from_ids {
            sources.push(graph.get_thought(*id)?.clone());
        }

        let kind = match method {
            AggregationMethod::Combine => ThoughtKind::Analysis,
            AggregationMethod::Synthesize => ThoughtKind::Synthesis,
            AggregationMethod::Consensus => ThoughtKind::Resolution,
        };

        let combined_content = format!(
            "{} - Aggregated {} thoughts using {method:?}",
            sources[0].content,
            sources.len(),
        );

        let avg_confidence: f64 =
            sources.iter().map(|s| s.metadata.confidence).sum::<f64>() / sources.len() as f64;

        let aggregated =
            Thought::new(kind, combined_content).with_confidence(avg_confidence.min(0.95));

        graph.add_thought(aggregated)?;

        tracing::debug!("Aggregated {} thoughts using {method:?}", from_ids.len(),);
        Ok(())
    }

    /// Score and update confidence of a thought
    ///
    /// # Errors
    /// Returns an error if the thought is not found.
    #[allow(clippy::unused_self)]
    fn execute_score(
        &self,
        thought_id: ThoughtId,
        criteria: &[String],
        graph: &mut ReasoningGraph,
    ) -> Result<()> {
        let thought = graph.get_thought(thought_id)?;

        let mut total_score = thought.metadata.confidence;
        for criterion in criteria {
            let matches = criterion.split_whitespace().any(|word| {
                thought
                    .content
                    .to_lowercase()
                    .contains(&word.to_lowercase())
            });

            if matches {
                total_score += 0.1;
            }
        }

        total_score = total_score.clamp(0.0, 1.0);

        let updated = graph.get_thought_mut(thought_id)?;
        updated.metadata.confidence = total_score;

        tracing::debug!(
            "Scored thought {thought_id} with {} criteria, new confidence: {total_score:.2}",
            criteria.len(),
        );
        Ok(())
    }

    /// Refine a thought with deeper analysis
    ///
    /// # Errors
    /// Returns an error if the thought is not found.
    #[allow(clippy::unused_self)]
    fn execute_refine(
        &self,
        thought_id: ThoughtId,
        graph: &mut ReasoningGraph,
        context: &PromptContext,
    ) -> Result<()> {
        let source = graph.get_thought(thought_id)?;

        let refined = Thought::new(
            ThoughtKind::Refinement,
            format!(
                "Refined: {} [Depth: {}, Iteration: {}]",
                source.content, context.current_depth, context.iteration
            ),
        )
        .with_confidence((source.metadata.confidence + 0.1).min(1.0));

        graph.add_thought(refined)?;

        tracing::debug!("Refined thought {thought_id}");
        Ok(())
    }

    /// Select best thoughts according to strategy
    #[allow(clippy::unnecessary_wraps)]
    fn execute_select_internal(
        &self,
        from_ids: &[ThoughtId],
        count: usize,
        strategy: SelectionStrategy,
        graph: &ReasoningGraph,
    ) -> Result<()> {
        if from_ids.is_empty() {
            return Ok(());
        }

        let selected_count = count.min(from_ids.len());
        let all_scores = self.scorer.score_all(graph);

        let mut scored_ids: Vec<_> = from_ids
            .iter()
            .map(|id| (*id, all_scores.get(id).copied().unwrap_or(0.0)))
            .collect();

        match strategy {
            SelectionStrategy::TopConfidence => {
                scored_ids
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
            SelectionStrategy::Diversity => {
                scored_ids
                    .sort_by_key(|a| graph.get_thought(a.0).map_or(0, |t| t.content.len() % 100));
            }
            SelectionStrategy::Support => {
                scored_ids.sort_by(|a, b| {
                    let b_support = graph.edges().iter().filter(|e| e.to == b.0).count();
                    let a_support = graph.edges().iter().filter(|e| e.to == a.0).count();
                    b_support.cmp(&a_support)
                });
            }
            SelectionStrategy::Coverage => {
                scored_ids.sort_by(|a, b| {
                    let a_kind = graph
                        .get_thought(a.0)
                        .map(|t| format!("{:?}", t.kind))
                        .unwrap_or_default();
                    let b_kind = graph
                        .get_thought(b.0)
                        .map(|t| format!("{:?}", t.kind))
                        .unwrap_or_default();
                    a_kind.cmp(&b_kind)
                });
            }
        }

        let selected_count = selected_count.min(scored_ids.len());

        tracing::debug!(
            "Selected {selected_count} of {} thoughts using {strategy:?} strategy",
            from_ids.len(),
        );
        Ok(())
    }
}

impl Default for OperationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_operation() {
        let mut graph = ReasoningGraph::new();
        let source = Thought::new(ThoughtKind::Initial, "Original thought".to_string());
        let id = source.id;
        graph.add_thought(source).expect("add source");

        let executor = OperationExecutor::new();
        let context = PromptContext::new("Test");

        let op = Operation::Generate {
            from: id,
            count: 3,
            prompt_template: "template".to_string(),
        };

        let result = executor.execute(&op, &mut graph, &context).await;
        assert!(result.is_ok());
        assert_eq!(graph.len(), 4);
    }

    #[tokio::test]
    async fn test_aggregate_operation() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "Thought 1".to_string()).with_confidence(0.8);
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Thought 2".to_string()).with_confidence(0.7);
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        let executor = OperationExecutor::new();

        let op = Operation::Aggregate {
            from_ids: vec![id1, id2],
            aggregation_method: AggregationMethod::Synthesize,
            prompt_template: "template".to_string(),
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
        assert_eq!(graph.len(), 3);
    }

    #[tokio::test]
    async fn test_score_operation() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Analysis, "High quality thought".to_string())
            .with_confidence(0.5);
        let thought_id = thought.id;

        graph.add_thought(thought).expect("add thought");

        let executor = OperationExecutor::new();

        let op = Operation::Score {
            thought_id,
            criteria: vec!["high".to_string(), "quality".to_string()],
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());

        let scored = graph.get_thought(thought_id).expect("get scored");
        assert!(scored.metadata.confidence > 0.5);
    }

    #[tokio::test]
    async fn test_refine_operation() {
        let mut graph = ReasoningGraph::new();
        let thought =
            Thought::new(ThoughtKind::Initial, "Raw thought".to_string()).with_confidence(0.6);
        let thought_id = thought.id;

        graph.add_thought(thought).expect("add thought");

        let executor = OperationExecutor::new();
        let context = PromptContext::new("Test").with_depth(1).with_iteration(2);

        let op = Operation::Refine {
            thought_id,
            refinement_prompt: "Make it better".to_string(),
        };

        let result = executor.execute(&op, &mut graph, &context).await;
        assert!(result.is_ok());
        assert_eq!(graph.len(), 2);
    }

    #[tokio::test]
    async fn test_select_top_confidence() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "Good".to_string()).with_confidence(0.9);
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Bad".to_string()).with_confidence(0.3);
        let id2 = t2.id;
        let t3 = Thought::new(ThoughtKind::Analysis, "Medium".to_string()).with_confidence(0.6);
        let id3 = t3.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph.add_thought(t3).expect("add t3");

        let executor = OperationExecutor::new();

        let op = Operation::Select {
            from_ids: vec![id1, id2, id3],
            count: 2,
            strategy: SelectionStrategy::TopConfidence,
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_aggregate_empty_ids() {
        let mut graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        let op = Operation::Aggregate {
            from_ids: vec![],
            aggregation_method: AggregationMethod::Combine,
            prompt_template: "template".to_string(),
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
        assert!(graph.is_empty());
    }

    #[tokio::test]
    async fn test_aggregate_combine_kind() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "A".to_string()).with_confidence(0.8);
        let id1 = t1.id;
        graph.add_thought(t1).expect("add t1");

        let executor = OperationExecutor::new();

        let op = Operation::Aggregate {
            from_ids: vec![id1],
            aggregation_method: AggregationMethod::Combine,
            prompt_template: "t".to_string(),
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
        assert_eq!(graph.len(), 2);
    }

    #[tokio::test]
    async fn test_aggregate_consensus_kind() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "A".to_string()).with_confidence(0.7);
        let id1 = t1.id;
        graph.add_thought(t1).expect("add t1");

        let executor = OperationExecutor::new();

        let op = Operation::Aggregate {
            from_ids: vec![id1],
            aggregation_method: AggregationMethod::Consensus,
            prompt_template: "t".to_string(),
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
        assert_eq!(graph.len(), 2);
    }

    #[tokio::test]
    async fn test_select_diversity_strategy() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "A".to_string()).with_confidence(0.9);
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string()).with_confidence(0.5);
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        let executor = OperationExecutor::new();

        let op = Operation::Select {
            from_ids: vec![id1, id2],
            count: 1,
            strategy: SelectionStrategy::Diversity,
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_select_support_strategy() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Parent".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Child".to_string());
        let id2 = t2.id;
        let t3 = Thought::new(ThoughtKind::Analysis, "Solo".to_string());
        let id3 = t3.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph.add_thought(t3).expect("add t3");
        graph
            .add_edge(
                id1,
                id2,
                crate::thinking::core::types::EdgeKind::DerivesFrom,
            )
            .expect("edge");

        let executor = OperationExecutor::new();

        let op = Operation::Select {
            from_ids: vec![id2, id3],
            count: 1,
            strategy: SelectionStrategy::Support,
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_select_coverage_strategy() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Analysis, "A".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Synthesis, "B".to_string());
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        let executor = OperationExecutor::new();

        let op = Operation::Select {
            from_ids: vec![id1, id2],
            count: 2,
            strategy: SelectionStrategy::Coverage,
        };

        let result = executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_select_empty_ids() {
        let graph = ReasoningGraph::new();
        let executor = OperationExecutor::new();

        let op = Operation::Select {
            from_ids: vec![],
            count: 5,
            strategy: SelectionStrategy::TopConfidence,
        };

        let mut g2 = graph;
        let result = executor
            .execute(&op, &mut g2, &PromptContext::new("Test"))
            .await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_executor() {
        let executor = OperationExecutor::default();
        let graph = ReasoningGraph::new();
        assert!(graph.is_empty());
        let _ = executor;
    }

    #[tokio::test]
    async fn test_generate_cycles_through_kinds() {
        let mut graph = ReasoningGraph::new();
        let source = Thought::new(ThoughtKind::Initial, "Source".to_string()).with_confidence(0.8);
        let id = source.id;
        graph.add_thought(source).expect("add");

        let executor = OperationExecutor::new();
        let ctx = PromptContext::new("Test").with_depth(1);

        let op = Operation::Generate {
            from: id,
            count: 3,
            prompt_template: "t".to_string(),
        };

        executor.execute(&op, &mut graph, &ctx).await.expect("exec");

        // 3 generated + 1 source = 4
        assert_eq!(graph.len(), 4);
    }

    #[tokio::test]
    async fn test_score_with_no_matching_criteria() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Analysis, "Hello world".to_string()).with_confidence(0.5);
        let id = t.id;
        graph.add_thought(t).expect("add");

        let executor = OperationExecutor::new();

        let op = Operation::Score {
            thought_id: id,
            criteria: vec!["xyz".to_string(), "abc".to_string()],
        };

        executor
            .execute(&op, &mut graph, &PromptContext::new("Test"))
            .await
            .expect("exec");

        // No criteria match, so confidence should stay at 0.5
        let thought = graph.get_thought(id).expect("get");
        assert!((thought.metadata.confidence - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_refine_adds_thought_to_graph() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Raw".to_string()).with_confidence(0.5);
        let id = t.id;
        graph.add_thought(t).expect("add");

        let executor = OperationExecutor::new();
        let ctx = PromptContext::new("Test").with_depth(2).with_iteration(3);

        let op = Operation::Refine {
            thought_id: id,
            refinement_prompt: "improve".to_string(),
        };

        executor.execute(&op, &mut graph, &ctx).await.expect("exec");
        assert_eq!(graph.len(), 2);
    }

    #[tokio::test]
    async fn test_generate_with_multibyte_utf8_content() {
        let mut graph = ReasoningGraph::new();
        // Create content where byte 50 falls inside a multibyte character
        // Each Japanese char is 3 bytes; 17 chars = 51 bytes, so byte 50 is mid-character
        let content = "日本語".repeat(17);
        assert!(content.len() > 50, "Content should exceed 50 bytes");

        let source = Thought::new(ThoughtKind::Initial, content).with_confidence(0.8);
        let id = source.id;
        graph.add_thought(source).expect("add");

        let executor = OperationExecutor::new();
        let ctx = PromptContext::new("Test").with_depth(1);

        let op = Operation::Generate {
            from: id,
            count: 1,
            prompt_template: "t".to_string(),
        };

        // Should not panic on UTF-8 boundary
        executor.execute(&op, &mut graph, &ctx).await.expect("exec");
        assert_eq!(graph.len(), 2);
    }
}
