use super::error::{Error, Result};
use super::types::{Edge, EdgeKind, Thought, ThoughtId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Directed acyclic graph (DAG) of reasoning thoughts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningGraph {
    thoughts: HashMap<ThoughtId, Thought>,
    edges: Vec<Edge>,
    root_thoughts: HashSet<ThoughtId>, // Initial thoughts with no predecessors
}

impl ReasoningGraph {
    #[must_use]
    pub fn new() -> Self {
        Self {
            thoughts: HashMap::new(),
            edges: Vec::new(),
            root_thoughts: HashSet::new(),
        }
    }

    /// Add a thought to the graph.
    ///
    pub fn add_thought(&mut self, thought: Thought) -> Result<ThoughtId> {
        let id = thought.id;
        if self.thoughts.contains_key(&id) {
            return Err(Error::GraphError("Thought already exists".into()));
        }
        self.thoughts.insert(id, thought);
        self.root_thoughts.insert(id); // Initially a root until edges prove otherwise
        Ok(id)
    }

    /// Get a thought by ID.
    ///
    pub fn thought(&self, id: ThoughtId) -> Result<&Thought> {
        self.thoughts
            .get(&id)
            .ok_or_else(|| Error::ThoughtNotFound(id.to_string()))
    }

    /// Get a mutable thought by ID.
    ///
    pub fn thought_mut(&mut self, id: ThoughtId) -> Result<&mut Thought> {
        self.thoughts
            .get_mut(&id)
            .ok_or_else(|| Error::ThoughtNotFound(id.to_string()))
    }

    /// Add an edge between two thoughts.
    ///
    pub fn add_edge(&mut self, from: ThoughtId, to: ThoughtId, kind: EdgeKind) -> Result<()> {
        if !self.thoughts.contains_key(&from) {
            return Err(Error::ThoughtNotFound(format!("Source thought {from}")));
        }
        if !self.thoughts.contains_key(&to) {
            return Err(Error::ThoughtNotFound(format!("Target thought {to}")));
        }

        // Prevent cycles: check if adding this edge would create a cycle
        if self.would_create_cycle(from, to) {
            return Err(Error::GraphError("Adding edge would create a cycle".into()));
        }

        self.edges.push(Edge::new(from, to, kind));
        self.root_thoughts.remove(&to); // to is no longer a root
        Ok(())
    }

    /// Get all thoughts in the graph
    pub fn thoughts(&self) -> impl Iterator<Item = &Thought> {
        self.thoughts.values()
    }

    /// Get all edges in the graph
    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Get thoughts that derive from a given thought
    #[must_use]
    pub fn successors(&self, id: ThoughtId) -> Vec<ThoughtId> {
        self.edges
            .iter()
            .filter(|e| e.from == id)
            .map(|e| e.to)
            .collect()
    }

    /// Get thoughts that a given thought derives from
    #[must_use]
    pub fn predecessors(&self, id: ThoughtId) -> Vec<ThoughtId> {
        self.edges
            .iter()
            .filter(|e| e.to == id)
            .map(|e| e.from)
            .collect()
    }

    /// Find the nearest ancestor (including self) with confidence above threshold.
    #[must_use]
    pub fn find_nearest_anchor(&self, id: ThoughtId, threshold: f64) -> Option<ThoughtId> {
        let thought = self.thoughts.get(&id)?;
        if thought.metadata.confidence >= threshold {
            return Some(id);
        }

        let preds = self.predecessors(id);
        for pred in preds {
            if let Some(anchor) = self.find_nearest_anchor(pred, threshold) {
                return Some(anchor);
            }
        }
        None
    }

    /// Get all root thoughts (no predecessors)
    #[must_use]
    pub fn root_thoughts(&self) -> Vec<&Thought> {
        self.root_thoughts
            .iter()
            .filter_map(|id| self.thoughts.get(id))
            .collect()
    }

    /// Get depth of a thought (distance from nearest root)
    #[must_use]
    pub fn depth(&self, id: ThoughtId) -> usize {
        let mut visited = HashSet::new();
        self.depth_impl(id, &mut visited)
    }

    fn depth_impl(&self, id: ThoughtId, visited: &mut HashSet<ThoughtId>) -> usize {
        if visited.contains(&id) {
            return 0; // Cycle detected (shouldn't happen in valid DAG)
        }
        visited.insert(id);

        let predecessors = self.predecessors(id);
        if predecessors.is_empty() {
            0
        } else {
            predecessors
                .iter()
                .map(|&pred| self.depth_impl(pred, visited) + 1)
                .max()
                .unwrap_or(0)
        }
    }

    /// Get all descendants of a thought (closure)
    #[must_use]
    pub fn descendants(&self, id: ThoughtId) -> HashSet<ThoughtId> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(id);

        while let Some(current) = queue.pop_front() {
            for successor in self.successors(current) {
                if result.insert(successor) {
                    queue.push_back(successor);
                }
            }
        }

        result
    }

    /// Remove a thought and its edges.
    ///
    pub fn remove_thought(&mut self, id: ThoughtId) -> Result<Thought> {
        let thought = self
            .thoughts
            .remove(&id)
            .ok_or_else(|| Error::ThoughtNotFound(id.to_string()))?;

        self.edges.retain(|e| e.from != id && e.to != id);
        self.root_thoughts.remove(&id);
        Ok(thought)
    }

    /// Prune a thought and all its descendants from the graph.
    ///
    pub fn prune_branch(&mut self, id: ThoughtId) -> Result<HashSet<ThoughtId>> {
        if !self.thoughts.contains_key(&id) {
            return Err(Error::ThoughtNotFound(id.to_string()));
        }

        let mut to_remove = self.descendants(id);
        to_remove.insert(id);

        for id in &to_remove {
            self.thoughts.remove(id);
            self.root_thoughts.remove(id);
        }

        self.edges
            .retain(|e| !to_remove.contains(&e.from) && !to_remove.contains(&e.to));

        Ok(to_remove)
    }

    /// Number of thoughts in the graph
    #[must_use]
    pub fn len(&self) -> usize {
        self.thoughts.len()
    }

    /// Check if graph is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.thoughts.is_empty()
    }

    /// Topological sort of thoughts.
    ///
    pub fn topological_sort(&self) -> Result<Vec<ThoughtId>> {
        let mut in_degree = HashMap::new();
        for &id in self.thoughts.keys() {
            in_degree.entry(id).or_insert(0);
        }

        for edge in &self.edges {
            let count = in_degree.entry(edge.to).or_insert(0);
            *count += 1;
        }

        let mut queue: VecDeque<ThoughtId> = in_degree
            .iter()
            .filter(|&(_, &degree)| degree == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::new();
        while let Some(id) = queue.pop_front() {
            result.push(id);
            for successor in self.successors(id) {
                if let Some(degree) = in_degree.get_mut(&successor) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(successor);
                    }
                }
            }
        }

        if result.len() != self.thoughts.len() {
            return Err(Error::GraphError("Graph contains a cycle".into()));
        }

        Ok(result)
    }

    /// Check if adding an edge would create a cycle (prevents DAG violations)
    fn would_create_cycle(&self, from: ThoughtId, to: ThoughtId) -> bool {
        // Check if to can reach from (if so, from->to would create a cycle)
        self.can_reach(to, from)
    }

    /// Check if source can reach target through edges
    fn can_reach(&self, source: ThoughtId, target: ThoughtId) -> bool {
        if source == target {
            return true;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(source);

        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if !visited.insert(current) {
                continue;
            }

            for successor in self.successors(current) {
                queue.push_back(successor);
            }
        }

        false
    }
}

impl Default for ReasoningGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::thinking::core::types::ThoughtKind;

    #[test]
    fn test_add_thought() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "Test thought".to_string());
        let id = thought.id;

        assert!(graph.add_thought(thought).is_ok());
        assert_eq!(graph.len(), 1);
        assert!(graph.thought(id).is_ok());
    }

    #[test]
    fn test_add_edge() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "First".to_string());
        let t2 = Thought::new(ThoughtKind::Refinement, "Second".to_string());
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1 should succeed");
        graph.add_thought(t2).expect("add t2 should succeed");
        assert!(graph.add_edge(id1, id2, EdgeKind::DerivesFrom).is_ok());
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn test_cycle_prevention() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "First".to_string());
        let t2 = Thought::new(ThoughtKind::Refinement, "Second".to_string());
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1 should succeed");
        graph.add_thought(t2).expect("add t2 should succeed");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("add edge should succeed");

        // Try to add a reverse edge (would create cycle)
        assert!(graph.add_edge(id2, id1, EdgeKind::DerivesFrom).is_err());
    }

    #[test]
    fn test_descendants() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let t2 = Thought::new(ThoughtKind::Refinement, "Child1".to_string());
        let t3 = Thought::new(ThoughtKind::Analysis, "Child2".to_string());
        let id1 = t1.id;
        let id2 = t2.id;
        let id3 = t3.id;

        graph.add_thought(t1).expect("add t1 should succeed");
        graph.add_thought(t2).expect("add t2 should succeed");
        graph.add_thought(t3).expect("add t3 should succeed");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("add edge 1->2 should succeed");
        graph
            .add_edge(id1, id3, EdgeKind::DerivesFrom)
            .expect("add edge 1->3 should succeed");

        let descendants = graph.descendants(id1);
        assert_eq!(descendants.len(), 2);
        assert!(descendants.contains(&id2));
        assert!(descendants.contains(&id3));
    }

    #[test]
    fn test_find_nearest_anchor_self_matches() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Root".to_string()).with_confidence(0.9);
        let id = t.id;
        graph.add_thought(t).expect("add should succeed");

        assert_eq!(graph.find_nearest_anchor(id, 0.8), Some(id));
    }

    #[test]
    fn test_find_nearest_anchor_traverses_predecessors() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string()).with_confidence(0.9);
        let t2 = Thought::new(ThoughtKind::Refinement, "Child".to_string()).with_confidence(0.3);
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge");

        assert_eq!(graph.find_nearest_anchor(id2, 0.8), Some(id1));
    }

    #[test]
    fn test_find_nearest_anchor_none_when_all_below() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string()).with_confidence(0.3);
        let t2 = Thought::new(ThoughtKind::Refinement, "Child".to_string()).with_confidence(0.2);
        let id1 = t1.id;
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge");

        assert_eq!(graph.find_nearest_anchor(id2, 0.8), None);
    }

    #[test]
    fn test_prune_branch_removes_descendants() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let t2 = Thought::new(ThoughtKind::Refinement, "Child".to_string());
        let t3 = Thought::new(ThoughtKind::Analysis, "Grandchild".to_string());
        let id1 = t1.id;
        let id2 = t2.id;
        let id3 = t3.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph.add_thought(t3).expect("add t3");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge 1->2");
        graph
            .add_edge(id2, id3, EdgeKind::DerivesFrom)
            .expect("edge 2->3");

        let removed = graph.prune_branch(id2).expect("prune should succeed");
        assert_eq!(removed.len(), 2);
        assert!(removed.contains(&id2));
        assert!(removed.contains(&id3));
        assert_eq!(graph.len(), 1);
        assert!(graph.thought(id1).is_ok());
    }

    #[test]
    fn test_prune_branch_nonexistent_returns_error() {
        let mut graph = ReasoningGraph::new();
        let result = graph.prune_branch(uuid::Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_duplicate_thought_rejected() {
        let mut graph = ReasoningGraph::new();
        let thought = Thought::new(ThoughtKind::Initial, "Test".to_string());
        let id = thought.id;
        graph.add_thought(thought).expect("add should succeed");

        // Try to add same thought again (same ID)
        let dup = Thought {
            id,
            kind: ThoughtKind::Analysis,
            content: "Duplicate".to_string(),
            metadata: crate::thinking::core::types::ThoughtMetadata::default(),
            created_at: 0,
        };
        assert!(graph.add_thought(dup).is_err());
    }

    #[test]
    fn test_get_thought_nonexistent() {
        let graph = ReasoningGraph::new();
        let result = graph.thought(uuid::Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_thought_mut_nonexistent() {
        let mut graph = ReasoningGraph::new();
        let result = graph.thought_mut(uuid::Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_add_edge_nonexistent_source() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Only".to_string());
        let id = t.id;
        graph.add_thought(t).expect("add");

        let result = graph.add_edge(uuid::Uuid::new_v4(), id, EdgeKind::DerivesFrom);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_edge_nonexistent_target() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Only".to_string());
        let id = t.id;
        graph.add_thought(t).expect("add");

        let result = graph.add_edge(id, uuid::Uuid::new_v4(), EdgeKind::DerivesFrom);
        assert!(result.is_err());
    }

    #[test]
    fn test_successors_empty() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let id = t.id;
        graph.add_thought(t).expect("add");

        assert!(graph.successors(id).is_empty());
    }

    #[test]
    fn test_predecessors_empty() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let id = t.id;
        graph.add_thought(t).expect("add");

        assert!(graph.predecessors(id).is_empty());
    }

    #[test]
    fn test_root_thoughts_initially_all() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string());
        let t2 = Thought::new(ThoughtKind::Initial, "B".to_string());
        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");

        let roots = graph.root_thoughts();
        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn test_root_thoughts_after_edge() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Parent".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Child".to_string());
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge");

        let roots = graph.root_thoughts();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].id, id1);
    }

    #[test]
    fn test_remove_thought() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string());
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge");

        let removed = graph.remove_thought(id2).expect("remove");
        assert_eq!(removed.id, id2);
        assert_eq!(graph.len(), 1);
        assert!(graph.edges().is_empty());
    }

    #[test]
    fn test_remove_thought_nonexistent() {
        let mut graph = ReasoningGraph::new();
        let result = graph.remove_thought(uuid::Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_topological_sort_simple() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "A".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "B".to_string());
        let id2 = t2.id;
        let t3 = Thought::new(ThoughtKind::Synthesis, "C".to_string());
        let id3 = t3.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph.add_thought(t3).expect("add t3");
        graph.add_edge(id1, id2, EdgeKind::DerivesFrom).expect("e1");
        graph.add_edge(id2, id3, EdgeKind::DerivesFrom).expect("e2");

        let sorted = graph.topological_sort().expect("topo sort");
        assert_eq!(sorted.len(), 3);
        // id1 should come before id2, id2 before id3
        let pos1 = sorted.iter().position(|&x| x == id1).unwrap();
        let pos2 = sorted.iter().position(|&x| x == id2).unwrap();
        let pos3 = sorted.iter().position(|&x| x == id3).unwrap();
        assert!(pos1 < pos2);
        assert!(pos2 < pos3);
    }

    #[test]
    fn test_depth_root_is_zero() {
        let mut graph = ReasoningGraph::new();
        let t = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let id = t.id;
        graph.add_thought(t).expect("add");

        assert_eq!(graph.depth(id), 0);
    }

    #[test]
    fn test_depth_child_is_one() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let id1 = t1.id;
        let t2 = Thought::new(ThoughtKind::Analysis, "Child".to_string());
        let id2 = t2.id;

        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph
            .add_edge(id1, id2, EdgeKind::DerivesFrom)
            .expect("edge");

        assert_eq!(graph.depth(id2), 1);
    }

    #[test]
    fn test_default_graph_is_empty() {
        let graph = ReasoningGraph::default();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_graph_serialization_roundtrip() {
        let mut graph = ReasoningGraph::new();
        let t1 = Thought::new(ThoughtKind::Initial, "Root".to_string());
        let t2 = Thought::new(ThoughtKind::Analysis, "Child".to_string());
        let id1 = t1.id;
        let id2 = t2.id;
        graph.add_thought(t1).expect("add t1");
        graph.add_thought(t2).expect("add t2");
        graph.add_edge(id1, id2, EdgeKind::Supports).expect("edge");

        let json = serde_json::to_string(&graph).unwrap();
        let back: ReasoningGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back.edges().len(), 1);
    }

    #[test]
    fn test_depth_diamond_dag() {
        // Diamond: A → B, A → C, B → D, C → D
        // depth(D) should be 2 (A→B→D or A→C→D), not inflated by revisiting A
        let mut graph = ReasoningGraph::new();
        let ta = Thought::new(ThoughtKind::Initial, "A".to_string());
        let tb = Thought::new(ThoughtKind::Analysis, "B".to_string());
        let tc = Thought::new(ThoughtKind::Analysis, "C".to_string());
        let td = Thought::new(ThoughtKind::Synthesis, "D".to_string());
        let ida = ta.id;
        let idb = tb.id;
        let idc = tc.id;
        let idd = td.id;

        graph.add_thought(ta).expect("add a");
        graph.add_thought(tb).expect("add b");
        graph.add_thought(tc).expect("add c");
        graph.add_thought(td).expect("add d");

        graph
            .add_edge(ida, idb, EdgeKind::DerivesFrom)
            .expect("a->b");
        graph
            .add_edge(ida, idc, EdgeKind::DerivesFrom)
            .expect("a->c");
        graph
            .add_edge(idb, idd, EdgeKind::DerivesFrom)
            .expect("b->d");
        graph
            .add_edge(idc, idd, EdgeKind::DerivesFrom)
            .expect("c->d");

        assert_eq!(graph.depth(ida), 0, "Root A should have depth 0");
        assert_eq!(graph.depth(idb), 1, "B should have depth 1");
        assert_eq!(graph.depth(idc), 1, "C should have depth 1");
        assert_eq!(graph.depth(idd), 2, "D should have depth 2 in diamond DAG");
    }

    #[test]
    fn test_depth_deep_chain() {
        // Linear chain: A → B → C → D → E
        let mut graph = ReasoningGraph::new();
        let mut ids = Vec::new();
        for i in 0..5 {
            let t = Thought::new(ThoughtKind::Analysis, format!("t{i}"));
            ids.push(t.id);
            graph.add_thought(t).expect("add");
        }
        for i in 0..4 {
            graph
                .add_edge(ids[i], ids[i + 1], EdgeKind::DerivesFrom)
                .expect("edge");
        }

        assert_eq!(graph.depth(ids[0]), 0);
        assert_eq!(graph.depth(ids[1]), 1);
        assert_eq!(graph.depth(ids[4]), 4);
    }

    #[test]
    fn test_thoughts_iterator() {
        let mut graph = ReasoningGraph::new();
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "A".to_string()))
            .expect("add");
        graph
            .add_thought(Thought::new(ThoughtKind::Initial, "B".to_string()))
            .expect("add");

        let count = graph.thoughts().count();
        assert_eq!(count, 2);
    }
}
