use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableGraph};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Skill,
    Tool,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub name: String,
    pub kind: NodeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Uses,
    Requires,
    AssignedTo,
    RelatedTo,
    ConflictsWith,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedGraph {
    pub nodes: Vec<(usize, GraphNode)>,
    pub edges: Vec<(usize, usize, GraphEdge)>,
}

pub struct CapabilityGraph {
    graph: StableGraph<GraphNode, GraphEdge>,
    name_index: HashMap<String, NodeIndex>,
}

impl CapabilityGraph {
    #[allow(clippy::missing_const_for_fn)]
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            name_index: HashMap::new(),
        }
    }

    pub fn add_skill(&mut self, name: &str) -> NodeIndex {
        self.add_node(name, NodeType::Skill)
    }

    pub fn add_tool(&mut self, name: &str) -> NodeIndex {
        self.add_node(name, NodeType::Tool)
    }

    pub fn add_agent(&mut self, name: &str) -> NodeIndex {
        self.add_node(name, NodeType::Agent)
    }

    pub fn add_edge(&mut self, from: &str, to: &str, kind: EdgeKind) -> Option<EdgeIndex> {
        let from_idx = self.name_index.get(from)?;
        let to_idx = self.name_index.get(to)?;
        Some(
            self.graph
                .add_edge(*from_idx, *to_idx, GraphEdge { kind, weight: 1.0 }),
        )
    }

    pub fn node(&self, name: &str) -> Option<&GraphNode> {
        self.name_index.get(name).map(|idx| &self.graph[*idx])
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    pub fn walk_from(&self, name: &str, max_hops: usize) -> Vec<(String, f32)> {
        let start = match self.name_index.get(name) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };

        let mut visited: HashMap<NodeIndex, f32> = HashMap::new();
        visited.insert(start, 1.0);

        let mut frontier = vec![(start, 1.0f32, 0usize)];

        while let Some((node, score, hops)) = frontier.pop() {
            if hops >= max_hops {
                continue;
            }

            let neighbors: Vec<(NodeIndex, f32)> = self
                .graph
                .edges_directed(node, Direction::Outgoing)
                .map(|e| (e.target(), e.weight().weight))
                .collect();

            for (neighbor, weight) in neighbors {
                let new_score = score * weight * 0.7;
                if new_score < 0.01 {
                    continue;
                }

                #[allow(clippy::float_cmp)]
                let should_add = visited
                    .get(&neighbor)
                    .is_none_or(|existing| new_score > *existing);

                if should_add {
                    visited.insert(neighbor, new_score);
                    frontier.push((neighbor, new_score, hops + 1));
                }
            }
        }

        visited.remove(&start);

        let mut results: Vec<(String, f32)> = visited
            .into_iter()
            .filter_map(|(idx, score)| self.graph.node_weight(idx).map(|n| (n.name.clone(), score)))
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn centrality_score(&self, name: &str) -> f32 {
        let idx = match self.name_index.get(name) {
            Some(i) => *i,
            None => return 0.0,
        };

        let degree = self.graph.edges_directed(idx, Direction::Outgoing).count()
            + self.graph.edges_directed(idx, Direction::Incoming).count();

        let total_nodes = self.graph.node_count().max(1);
        degree as f32 / total_nodes as f32
    }

    pub fn serialize(&self) -> SerializedGraph {
        let nodes: Vec<(usize, GraphNode)> = self
            .graph
            .node_indices()
            .filter_map(|idx: NodeIndex| {
                self.graph
                    .node_weight(idx)
                    .map(|n: &GraphNode| (idx.index(), n.clone()))
            })
            .collect();

        let edges: Vec<(usize, usize, GraphEdge)> = self
            .graph
            .edge_indices()
            .filter_map(|idx: EdgeIndex| {
                let (src, dst) = self.graph.edge_endpoints(idx)?;
                let weight = self.graph.edge_weight(idx)?;
                Some((src.index(), dst.index(), weight.clone()))
            })
            .collect();

        SerializedGraph { nodes, edges }
    }

    pub fn deserialize(&mut self, data: &SerializedGraph) {
        self.graph.clear();
        self.name_index.clear();

        let mut idx_map: HashMap<usize, NodeIndex> = HashMap::new();
        for (raw_idx, node) in &data.nodes {
            let idx = self.graph.add_node(node.clone());
            self.name_index.insert(node.name.clone(), idx);
            idx_map.insert(*raw_idx, idx);
        }

        for (raw_src, raw_dst, edge) in &data.edges {
            if let (Some(src), Some(dst)) = (idx_map.get(raw_src), idx_map.get(raw_dst)) {
                self.graph.add_edge(*src, *dst, edge.clone());
            }
        }
    }

    pub fn clear(&mut self) {
        self.graph.clear();
        self.name_index.clear();
    }

    fn add_node(&mut self, name: &str, kind: NodeType) -> NodeIndex {
        if let Some(&idx) = self.name_index.get(name) {
            return idx;
        }
        let idx = self.graph.add_node(GraphNode {
            name: name.to_string(),
            kind,
        });
        self.name_index.insert(name.to_string(), idx);
        idx
    }
}

impl Default for CapabilityGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_graph_is_empty() {
        let g = CapabilityGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn default_graph_is_empty() {
        let g = CapabilityGraph::default();
        assert_eq!(g.node_count(), 0);
    }

    #[test]
    fn add_skill() {
        let mut g = CapabilityGraph::new();
        let _idx = g.add_skill("code-review");
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.node("code-review").unwrap().kind, NodeType::Skill);
    }

    #[test]
    fn add_tool() {
        let mut g = CapabilityGraph::new();
        g.add_tool("Bash");
        assert_eq!(g.node("Bash").unwrap().kind, NodeType::Tool);
    }

    #[test]
    fn add_agent() {
        let mut g = CapabilityGraph::new();
        g.add_agent("reviewer");
        assert_eq!(g.node("reviewer").unwrap().kind, NodeType::Agent);
    }

    #[test]
    fn add_duplicate_returns_same_index() {
        let mut g = CapabilityGraph::new();
        let a = g.add_skill("test");
        let b = g.add_skill("test");
        assert_eq!(a, b);
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn add_edge_between_nodes() {
        let mut g = CapabilityGraph::new();
        g.add_skill("code-review");
        g.add_tool("Read");
        let edge = g.add_edge("code-review", "Read", EdgeKind::Uses);
        assert!(edge.is_some());
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn add_edge_missing_node_returns_none() {
        let mut g = CapabilityGraph::new();
        g.add_skill("exists");
        assert!(g.add_edge("exists", "missing", EdgeKind::Uses).is_none());
    }

    #[test]
    fn walk_from_finds_related() {
        let mut g = CapabilityGraph::new();
        g.add_skill("code-review");
        g.add_tool("Read");
        g.add_skill("testing");
        g.add_edge("code-review", "Read", EdgeKind::Uses);
        g.add_edge("code-review", "testing", EdgeKind::RelatedTo);

        let related = g.walk_from("code-review", 3);
        assert_eq!(related.len(), 2);
        assert!(related.iter().any(|(n, _)| n == "testing"));
        assert!(related.iter().any(|(n, _)| n == "Read"));
    }

    #[test]
    fn walk_from_missing_returns_empty() {
        let g = CapabilityGraph::new();
        assert!(g.walk_from("missing", 3).is_empty());
    }

    #[test]
    fn walk_from_excludes_self() {
        let mut g = CapabilityGraph::new();
        g.add_skill("solo");
        let related = g.walk_from("solo", 3);
        assert!(related.is_empty());
    }

    #[test]
    fn centrality_score_connected() {
        let mut g = CapabilityGraph::new();
        g.add_skill("hub");
        g.add_skill("a");
        g.add_skill("b");
        g.add_edge("hub", "a", EdgeKind::RelatedTo);
        g.add_edge("b", "hub", EdgeKind::Requires);

        let score = g.centrality_score("hub");
        assert!(score > 0.0);
    }

    #[test]
    fn centrality_score_missing() {
        let g = CapabilityGraph::new();
        assert_eq!(g.centrality_score("missing"), 0.0);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let mut g = CapabilityGraph::new();
        g.add_skill("s1");
        g.add_tool("t1");
        g.add_edge("s1", "t1", EdgeKind::Uses);

        let serialized = g.serialize();
        let mut g2 = CapabilityGraph::new();
        g2.deserialize(&serialized);

        assert_eq!(g2.node_count(), 2);
        assert_eq!(g2.edge_count(), 1);
        assert!(g2.node("s1").is_some());
        assert!(g2.node("t1").is_some());
    }

    #[test]
    fn clear_empties_graph() {
        let mut g = CapabilityGraph::new();
        g.add_skill("test");
        g.add_tool("tool");
        assert_eq!(g.node_count(), 2);
        g.clear();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn walk_multi_hop_decays() {
        let mut g = CapabilityGraph::new();
        g.add_skill("a");
        g.add_skill("b");
        g.add_skill("c");
        g.add_edge("a", "b", EdgeKind::RelatedTo);
        g.add_edge("b", "c", EdgeKind::RelatedTo);

        let related = g.walk_from("a", 3);
        let b_score = related.iter().find(|(n, _)| n == "b").map(|(_, s)| *s);
        let c_score = related.iter().find(|(n, _)| n == "c").map(|(_, s)| *s);
        assert!(b_score.is_some());
        assert!(c_score.is_some());
        assert!(b_score.unwrap() > c_score.unwrap());
    }
}
