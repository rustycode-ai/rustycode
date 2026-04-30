//! Helper functions for converting between `ReasoningGraph` and serialized representations

use crate::thinking::core::graph::ReasoningGraph;
use crate::thinking::core::types::{EdgeKind, Thought, ThoughtKind};
use crate::thinking::persistence::{
    GraphMetadata, SerializedEdge, SerializedGraph, SerializedThought,
};
use std::collections::HashMap;
use std::time::SystemTime;

/// Convert a `ReasoningGraph` to a `SerializedGraph` for persistence
///
/// # Panics
///
/// Will not panic in practice; system time is always after the Unix epoch.
#[must_use]
#[allow(clippy::cast_possible_wrap)]
pub fn serialize_graph(
    graph: &ReasoningGraph,
    problem: String,
    session_id: String,
    strategy: Option<String>,
    iterations: usize,
) -> SerializedGraph {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Convert thoughts
    let thoughts: Vec<SerializedThought> = graph
        .thoughts()
        .map(|thought| SerializedThought {
            id: thought.id.to_string(),
            kind: format!("{:?}", thought.kind),
            content: thought.content.clone(),
            confidence: thought.metadata.confidence,
            evidence: thought.metadata.evidence.clone(),
            created_at: thought.created_at,
        })
        .collect();

    // Convert edges
    let edges: Vec<SerializedEdge> = graph
        .edges()
        .iter()
        .map(|edge| SerializedEdge {
            from: edge.from.to_string(),
            to: edge.to.to_string(),
            kind: format!("{:?}", edge.kind),
        })
        .collect();

    // Get root thought IDs
    let root_ids: Vec<String> = graph
        .root_thoughts()
        .iter()
        .map(|t| t.id.to_string())
        .collect();

    SerializedGraph {
        thoughts,
        edges,
        root_ids,
        metadata: GraphMetadata {
            problem,
            created_at: now,
            modified_at: now,
            session_id,
            strategy,
            iterations,
            custom: HashMap::new(),
        },
    }
}

/// Convert a `SerializedGraph` back to a `ReasoningGraph`
///
/// # Errors
///
/// Returns an error if thoughts or edges cannot be added to the graph.
pub fn deserialize_graph(
    serialized: &SerializedGraph,
) -> crate::thinking::core::error::Result<ReasoningGraph> {
    let mut graph = ReasoningGraph::new();

    // Create a map of string IDs to parsed UUIDs
    let mut id_map: HashMap<String, uuid::Uuid> = HashMap::new();

    // Reconstruct thoughts
    for thought_data in &serialized.thoughts {
        let kind = parse_thought_kind(&thought_data.kind);
        let mut thought = Thought::new(kind, thought_data.content.clone())
            .with_confidence(thought_data.confidence);

        // Restore evidence
        for evidence in &thought_data.evidence {
            thought.metadata.evidence.push(evidence.clone());
        }

        let thought_id = thought.id;
        id_map.insert(thought_data.id.clone(), thought_id);
        graph.add_thought(thought)?;
    }

    // Reconstruct edges
    for edge_data in &serialized.edges {
        if let (Some(&from_id), Some(&to_id)) =
            (id_map.get(&edge_data.from), id_map.get(&edge_data.to))
        {
            let kind = parse_edge_kind(&edge_data.kind);
            graph.add_edge(from_id, to_id, kind)?;
        }
    }

    Ok(graph)
}

/// Parse `ThoughtKind` from string representation
fn parse_thought_kind(kind_str: &str) -> ThoughtKind {
    match kind_str {
        "Initial" => ThoughtKind::Initial,
        "Refinement" => ThoughtKind::Refinement,
        "Synthesis" => ThoughtKind::Synthesis,
        "Critique" => ThoughtKind::Critique,
        "Resolution" => ThoughtKind::Resolution,
        _ => ThoughtKind::Analysis, // Default (covers "Analysis" and unknown)
    }
}

/// Parse `EdgeKind` from string representation
fn parse_edge_kind(kind_str: &str) -> EdgeKind {
    match kind_str {
        "Supports" => EdgeKind::Supports,
        "Contradicts" => EdgeKind::Contradicts,
        "Refines" => EdgeKind::Refines,
        "RelatesTo" => EdgeKind::RelatesTo,
        "Aggregates" => EdgeKind::Aggregates,
        _ => EdgeKind::DerivesFrom, // Default (covers "DerivesFrom" and unknown)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_empty_graph() {
        let graph = ReasoningGraph::new();
        let serialized = serialize_graph(
            &graph,
            "Test problem".to_string(),
            "test-session".to_string(),
            Some("Sequential".to_string()),
            0,
        );

        assert_eq!(serialized.thoughts.len(), 0);
        assert_eq!(serialized.edges.len(), 0);
        assert_eq!(serialized.metadata.problem, "Test problem");
        assert_eq!(serialized.metadata.session_id, "test-session");
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let mut graph = ReasoningGraph::new();
        let thought1 =
            Thought::new(ThoughtKind::Initial, "First thought".to_string()).with_confidence(0.8);
        let id1 = thought1.id;
        graph.add_thought(thought1).unwrap();

        let thought2 =
            Thought::new(ThoughtKind::Analysis, "Second thought".to_string()).with_confidence(0.7);
        let id2 = thought2.id;
        graph.add_thought(thought2).unwrap();

        graph.add_edge(id1, id2, EdgeKind::Supports).unwrap();

        // Serialize
        let serialized = serialize_graph(
            &graph,
            "Test roundtrip".to_string(),
            "roundtrip-test".to_string(),
            Some("Dialectic".to_string()),
            2,
        );

        assert_eq!(serialized.thoughts.len(), 2);
        assert_eq!(serialized.edges.len(), 1);

        // Deserialize
        let restored = deserialize_graph(&serialized).unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.edges().len(), 1);

        // Verify thoughts were restored correctly (IDs change during roundtrip)
        let restored_thoughts: Vec<_> = restored.thoughts().collect();
        assert_eq!(restored_thoughts.len(), 2);

        let contents: Vec<&str> = restored_thoughts
            .iter()
            .map(|t| t.content.as_str())
            .collect();
        assert!(contents.contains(&"First thought"));
        assert!(contents.contains(&"Second thought"));

        // Verify edge structure preserved
        assert_eq!(restored.edges().len(), 1);
    }

    #[test]
    fn test_parse_thought_kinds() {
        assert_eq!(parse_thought_kind("Initial"), ThoughtKind::Initial);
        assert_eq!(parse_thought_kind("Analysis"), ThoughtKind::Analysis);
        assert_eq!(parse_thought_kind("Synthesis"), ThoughtKind::Synthesis);
        assert_eq!(parse_thought_kind("Unknown"), ThoughtKind::Analysis); // Default
    }

    #[test]
    fn test_parse_edge_kinds() {
        assert_eq!(parse_edge_kind("Supports"), EdgeKind::Supports);
        assert_eq!(parse_edge_kind("Contradicts"), EdgeKind::Contradicts);
        assert_eq!(parse_edge_kind("Refines"), EdgeKind::Refines);
        assert_eq!(parse_edge_kind("Unknown"), EdgeKind::DerivesFrom); // Default
    }

    #[test]
    fn test_serialize_graph_with_evidence() {
        let mut graph = ReasoningGraph::new();
        let mut thought = Thought::new(ThoughtKind::Analysis, "Test with evidence".to_string())
            .with_confidence(0.9);
        thought.metadata.evidence.push("source A".to_string());
        thought.metadata.evidence.push("source B".to_string());
        graph.add_thought(thought).unwrap();

        let serialized = serialize_graph(
            &graph,
            "Evidence test".to_string(),
            "evidence-session".to_string(),
            None,
            1,
        );

        assert_eq!(serialized.thoughts[0].evidence.len(), 2);
        assert_eq!(serialized.thoughts[0].evidence[0], "source A");
    }

    #[test]
    fn test_deserialize_preserves_evidence() {
        let mut graph = ReasoningGraph::new();
        let mut thought =
            Thought::new(ThoughtKind::Critique, "Critique".to_string()).with_confidence(0.6);
        thought.metadata.evidence.push("fact 1".to_string());
        graph.add_thought(thought).unwrap();

        let serialized = serialize_graph(&graph, "Test".to_string(), "test".to_string(), None, 0);
        let restored = deserialize_graph(&serialized).unwrap();
        let t = restored.thoughts().next().unwrap();
        assert_eq!(t.metadata.evidence.len(), 1);
        assert_eq!(t.metadata.evidence[0], "fact 1");
    }

    #[test]
    fn test_serialize_multiple_edges() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string());
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string());
        let t3 = Thought::new(ThoughtKind::Synthesis, "C".to_string());
        let id1 = t1.id;
        let id2 = t2.id;
        let id3 = t3.id;
        graph.add_thought(t1).unwrap();
        graph.add_thought(t2).unwrap();
        graph.add_thought(t3).unwrap();
        graph.add_edge(id1, id2, EdgeKind::Supports).unwrap();
        graph.add_edge(id2, id3, EdgeKind::Refines).unwrap();
        graph.add_edge(id1, id3, EdgeKind::RelatesTo).unwrap();

        let serialized = serialize_graph(
            &graph,
            "Multi-edge".to_string(),
            "multi".to_string(),
            Some("Parallel".to_string()),
            3,
        );
        assert_eq!(serialized.edges.len(), 3);
        assert_eq!(serialized.root_ids.len(), 1);

        let restored = deserialize_graph(&serialized).unwrap();
        assert_eq!(restored.edges().len(), 3);
    }

    #[test]
    fn test_parse_all_thought_kinds() {
        assert_eq!(parse_thought_kind("Initial"), ThoughtKind::Initial);
        assert_eq!(parse_thought_kind("Refinement"), ThoughtKind::Refinement);
        assert_eq!(parse_thought_kind("Synthesis"), ThoughtKind::Synthesis);
        assert_eq!(parse_thought_kind("Critique"), ThoughtKind::Critique);
        assert_eq!(parse_thought_kind("Resolution"), ThoughtKind::Resolution);
        assert_eq!(parse_thought_kind("Analysis"), ThoughtKind::Analysis);
    }

    #[test]
    fn test_parse_all_edge_kinds() {
        assert_eq!(parse_edge_kind("Supports"), EdgeKind::Supports);
        assert_eq!(parse_edge_kind("Contradicts"), EdgeKind::Contradicts);
        assert_eq!(parse_edge_kind("Refines"), EdgeKind::Refines);
        assert_eq!(parse_edge_kind("RelatesTo"), EdgeKind::RelatesTo);
        assert_eq!(parse_edge_kind("Aggregates"), EdgeKind::Aggregates);
        assert_eq!(parse_edge_kind("DerivesFrom"), EdgeKind::DerivesFrom);
    }

    #[test]
    fn test_serialize_metadata_fields() {
        let graph = ReasoningGraph::new();
        let serialized = serialize_graph(
            &graph,
            "Meta test".to_string(),
            "meta-id".to_string(),
            Some("Abductive".to_string()),
            42,
        );

        assert_eq!(serialized.metadata.problem, "Meta test");
        assert_eq!(serialized.metadata.session_id, "meta-id");
        assert_eq!(serialized.metadata.strategy, Some("Abductive".to_string()));
        assert_eq!(serialized.metadata.iterations, 42);
        assert!(serialized.metadata.created_at > 0);
        assert_eq!(
            serialized.metadata.created_at,
            serialized.metadata.modified_at
        );
    }

    #[test]
    fn test_deserialize_empty_thoughts_and_edges() {
        let serialized = SerializedGraph {
            thoughts: vec![],
            edges: vec![],
            root_ids: vec![],
            metadata: GraphMetadata {
                problem: "Empty".to_string(),
                created_at: 0,
                modified_at: 0,
                session_id: "empty".to_string(),
                strategy: None,
                iterations: 0,
                custom: std::collections::HashMap::new(),
            },
        };
        let graph = deserialize_graph(&serialized).unwrap();
        assert_eq!(graph.len(), 0);
        assert!(graph.edges().is_empty());
    }

    #[test]
    fn test_serialize_preserves_created_at() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "timed thought".to_string());
        let created_at = thought.created_at;
        graph.add_thought(thought).unwrap();

        let serialized = serialize_graph(
            &graph,
            "Timestamp test".to_string(),
            "ts-test".to_string(),
            None,
            0,
        );

        assert_eq!(serialized.thoughts[0].created_at, created_at);
        assert!(
            created_at > 0,
            "created_at should be a valid Unix timestamp"
        );
    }
}
