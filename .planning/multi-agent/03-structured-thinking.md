# 03 — Structured Thinking

*`crates/rustycode-orchestration/src/thinking/`*

The structured thinking module provides a **Graph-of-Thoughts** reasoning system that agents
use to decompose complex problems, track confidence, and carry reasoning across escalation
boundaries.

~2,600 LOC across 15 files.

---

## Core Types

### ReasoningGraph

*`crates/rustycode-orchestration/src/thinking/core/graph.rs`*

A directed acyclic graph (DAG) of reasoning thoughts. Each agent owns its own graph.

```rust
pub struct ReasoningGraph {
    thoughts: HashMap<ThoughtId, Thought>,
    edges: Vec<Edge>,
    root_thoughts: HashSet<ThoughtId>,
}
```

Key operations: `add_thought()`, `add_edge()`, `children()`, `ancestors()`,
`prune_low_confidence()`, `top_insights(n)`.

### Thought

*`crates/rustycode-orchestration/src/thinking/core/types.rs`*

```rust
pub struct Thought {
    pub id: ThoughtId,          // UUID
    pub kind: ThoughtKind,
    pub content: String,
    pub metadata: ThoughtMetadata,
    pub created_at: i64,        // Unix timestamp
}

pub struct ThoughtMetadata {
    pub confidence: f64,        // [0.0, 1.0]
    pub strategy: String,       // which strategy generated this
    pub depth: usize,           // distance from root
    pub pruned: bool,
    pub analysis_count: usize,
    pub evidence: Vec<String>,
}
```

### Edge

```rust
pub struct Edge {
    pub from: ThoughtId,
    pub to: ThoughtId,
    pub kind: EdgeKind,
    pub strength: f64,          // [0.0, 1.0]
}
```

### Operation

```rust
pub enum Operation {
    AddThought { kind, content, parent, confidence },
    RefineThought { id, new_content, new_confidence },
    Merge { source_ids, merged_content, confidence },
    Branch { parent_id, content, confidence },
    Prune { ids },
}
```

---

## Reasoning Strategies

*`crates/rustycode-orchestration/src/thinking/strategies/`*

Five adaptive strategies, each with deterministic problem matching:

```rust
#[async_trait]
pub trait ReasoningStrategy: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, prompt: &str) -> Result<Vec<String>>;
    fn matches_problem(&self, problem: &str) -> bool;
}
```

| Strategy | Match Signal | When to Use |
|----------|-------------|-------------|
| **Sequential** | "step", "first", "then" | Linear step-by-step reasoning |
| **Dialectic** | "debate", "pros cons", "argue" | Thesis/antithesis/synthesis |
| **Parallel** | "compare", "alternatives" | Explore multiple paths simultaneously |
| **Analogical** | "similar", "like", "analogy" | Transfer knowledge from known examples |
| **Abductive** | "why", "explain", "cause" | Best explanation for observations |

Strategy selection: first `matches_problem()` match wins in the decision tree.

---

## ThinkingExecutor

*`crates/rustycode-orchestration/src/thinking/executor.rs`* (~780 LOC)

Orchestrates the reasoning process: selects strategies, executes them, manages the graph.

Composed with:
- `ThinkingBudget` — token/cost ceiling for reasoning
- `ThinkingActivationPolicy` — when to activate thinking (conservative vs aggressive)
- `ConvergenceDetector` — stops when confidence stabilizes

---

## Activation

*`crates/rustycode-orchestration/src/thinking/activator.rs`*

```rust
pub trait ThinkingActivationPolicy: Send + Sync {
    fn should_activate(&self, signals: &ActivationSignals) -> bool;
}
```

Two built-in policies:
- `DefaultActivationPolicy` — activates on complex tasks
- `ConservativeActivationPolicy` — only activates on explicitly flagged tasks

`ActivationSignals` carries task complexity, past failure count, budget remaining.

---

## Persistence

*`crates/rustycode-orchestration/src/thinking/persistence.rs`*

Reasoning graphs survive across sessions via serialization:

```rust
pub struct SerializedGraph { /* serde-friendly graph representation */ }
pub struct SessionManager { /* save/load graphs to disk */ }
```

This enables cross-session reasoning carry-forward: a resumed session restores its graph
and continues reasoning from where it left off.

---

## Hybrid Model: Per-Agent Graphs + Team Convergence

Each agent owns its own `ReasoningGraph`. At team level, a lightweight `ConvergenceView`
aggregates without merging full graphs:

```
Agent A ─── ReasoningGraph (private, 50+ thoughts) ───┐
                                                        ├─→ ConvergenceView
Agent B ─── ReasoningGraph (private, 30+ thoughts) ───┘     • mean/max confidence
                                                             • top-5 insights
                                                             • dissenting opinions
                                                             • aggregate token usage
```

The `ConvergenceView` is what flows upward to ensembles. Individual graphs stay local.
See [04-context-forwarding.md](04-context-forwarding.md) for the full context model.
