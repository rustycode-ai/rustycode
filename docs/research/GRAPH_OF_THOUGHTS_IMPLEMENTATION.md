# Graph of Thoughts (GoT) Implementation for RustyCode

**Research Date**: 2026-04-21  
**Status**: Detailed Implementation Guide  
**Based On**: [Official GoT Paper](https://arxiv.org/abs/2308.09687) and [Official Implementation](https://github.com/spcl/graph-of-thoughts)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [GoT Fundamentals](#got-fundamentals)
3. [Performance Metrics](#performance-metrics)
4. [RustyCode Design](#rustycode-design)
5. [Implementation Architecture](#implementation-architecture)
6. [Use Cases for Code Generation](#use-cases-for-code-generation)
7. [Code Examples](#code-examples)
8. [Integration Steps](#integration-steps)

---

## Executive Summary

Graph of Thoughts (GoT) is a framework that enables LLMs to solve complex problems by:

1. **Decomposing** large problems into independent subtasks
2. **Solving** each subtask in parallel with multiple reasoning paths
3. **Aggregating** results from multiple paths to synthesize final solutions

**Key Results from Research:**
- 62% quality improvement over Tree of Thoughts (ToT)
- 31% cost reduction compared to ToT
- Scales better with problem complexity
- Balances latency (log_k N) with volume (N)

**Why This Matters for RustyCode:**
- Code generation is a complex decomposable problem (architecture, implementation, testing)
- Multiple analyses can run in parallel (security, performance, maintainability)
- Final code benefits from synthesizing multiple perspectives
- Can reduce hallucinations through structured synthesis

---

## GoT Fundamentals

### 1. Core Concept: Directed Acyclic Graph

GoT represents reasoning as a **directed acyclic graph (DAG)** where:

```
Nodes = "Thoughts" (units of reasoning)
Edges = Dependencies (which thoughts depend on which)
```

**Key Difference from Trees:**
- **Tree**: Each node has one parent (one input dependency)
- **Graph**: Nodes can have multiple parents (multiple input dependencies)

```
Tree Structure:          Graph Structure:

    Root                    Thought A → Thought C
    /  \                         ↓       ↓
   B1   B2              Thought B → Thought D
   / \  / \                  ↓       ↓
  L1 L2 L3 L4           Aggregation Node E
                               ↓
                          Final Solution
```

### 2. Three Types of Operations

GoT supports three fundamental transformations:

#### Generation
```
Input Thought → Generate → Multiple Output Thoughts
(branching)

Example for code generation:
"Analyze module A" → generates:
  - "Security analysis of module A"
  - "Performance analysis of module A"
  - "Dependencies of module A"
```

#### Aggregation
```
Multiple Input Thoughts → Aggregate → Output Thought
(converging)

Example for code generation:
  - "Security issues in module A"
  - "Performance issues in module A"
  - "Dependency conflicts in module A"
  → Aggregate → "Fix priority list for module A"
```

#### Refinement
```
Input Thought → Refine → Improved Output Thought
(self-loop)

Example:
"Architecture plan v1" → Refine → "Architecture plan v2 (with issue #5 resolved)"
```

### 3. Graph of Operations (GoO)

The execution plan for solving a problem:

```
GoO = Sequence of operations applied to the graph

Example:
1. Generate: Create initial analysis nodes (security, performance, etc.)
2. Generate: For each analysis, create multiple perspectives
3. Score: Rate each perspective
4. Aggregation: Combine highest-rated perspectives
5. Refinement: Improve aggregated result
6. Ground Truth Check: Validate against requirements
```

### 4. Graph Reasoning State (GRS)

Maintains state throughout execution:

```
GRS = {
  nodes: HashMap<NodeId, Thought>,
  edges: Vec<(NodeId, NodeId)>,  // (parent, child)
  execution_log: Vec<Operation>,
  scores: HashMap<NodeId, Score>,
}
```

---

## Performance Metrics

### Quality Improvements

From the [official paper](https://arxiv.org/abs/2308.09687):

| Task | Improvement | Cost |
|------|-------------|------|
| Sorting (P=64) | 61% error reduction | -31% cost |
| Sorting (P=128) | 69% error reduction | -31% cost |
| Set Intersection | Consistent gains | Lower |
| Keyword Counting | Error reduction | Lower |
| Document Merging | Better synthesis | Comparable |

**Pattern**: Improvements increase with problem complexity.

### Why GoT Beats CoT and ToT

**Chain of Thought (CoT):**
- Single linear path
- No recovery from intermediate mistakes
- Cannot synthesize multiple perspectives

**Tree of Thoughts (ToT):**
- Multiple paths explored
- Can backtrack and explore alternatives
- BUT: Paths don't share insights
- Results not synthesized together

**Graph of Thoughts (GoT):**
- Multiple paths explored (like ToT)
- **PLUS** aggregation of results (unlike ToT)
- Independent analyses inform final decision
- Synergistic combination of insights

### Latency-Volume Tradeoff

GoT is unique in combining low latency with high volume:

```
Latency = How many "hops" (sequential LLM calls)
Volume = How many total thoughts contribute to solution

           Latency      Volume
CoT        N           N        (linear: both N)
ToT        log_k N     log_k N  (tree: both log)
GoT        log_k N     N        (best of both!)
```

**Why This Matters:**
- **Latency** determines response time (users wait for this)
- **Volume** determines information density in final answer
- GoT gets fast response time (log_k N) while incorporating all thoughts (N)

---

## RustyCode Design

### Architecture Overview

```
┌──────────────────────────────────────────────────────┐
│              Problem Specification                   │
│           (What code to generate/fix)                │
└────────────────────┬─────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────┐
│            Graph Builder                             │
│  - Defines decomposition strategy                    │
│  - Creates initial analysis nodes                    │
└────────────────────┬─────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────┐
│         Graph Executor (Main Loop)                   │
│                                                      │
│  ┌─────────────────────────────────────┐            │
│  │ 1. Select next operation from GoO    │            │
│  │ 2. Execute with LLM                  │            │
│  │ 3. Update GRS with results           │            │
│  │ 4. Repeat until GoO complete         │            │
│  └─────────────────────────────────────┘            │
│                                                      │
│  Maintains:                                          │
│  - Thought Graph: nodes + edges                      │
│  - Scores: quality of each thought                   │
│  - Execution Log: audit trail                        │
└────────────────────┬─────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────┐
│           Code Generator                             │
│  (Convert final thoughts to actual code)             │
└────────────────────┬─────────────────────────────────┘
                     │
┌────────────────────▼─────────────────────────────────┐
│            Generated Code                            │
└──────────────────────────────────────────────────────┘
```

### Graph Builder: Code Generation Decomposition

For a code generation task like "refactor module A", decompose into:

```rust
pub struct CodeGenerationDecomposition {
    pub analyses: Vec<Analysis>,          // What to understand
    pub perspectives: Vec<Perspective>,   // Different ways to solve
    pub synthesis: SynthesisStrategy,     // How to combine
}

pub enum Analysis {
    CurrentState,         // Understand existing code
    Requirements,         // What's required
    Security,            // Security implications
    Performance,         // Performance impact
    Dependencies,        // What it depends on
    Maintainability,     // Long-term quality
}

pub enum Perspective {
    Incremental,    // Minimal changes
    Refactor,       // Clean rewrite
    Modular,        // Split into modules
    Performance,    // Optimize for speed
}

pub struct SynthesisStrategy {
    aggregation_method: AggregationMethod,  // How to combine
    ranking_strategy: RankingStrategy,      // Which are best
    final_resolution: Resolution,           // How to pick final
}
```

### Example: Refactoring Legacy Code

**Decomposition:**

```
Start: "Refactor the authentication module"

Phase 1 - Analysis Generation:
├─ Analysis Node 1: "Current authentication code review"
├─ Analysis Node 2: "Security vulnerabilities analysis"
├─ Analysis Node 3: "Performance bottleneck analysis"
└─ Analysis Node 4: "Dependencies identification"

Phase 2 - Perspective Generation:
├─ Perspective Node 1.1: "Minimal fix approach" (based on Analysis 1)
├─ Perspective Node 1.2: "Incremental refactor" (based on Analysis 1)
├─ Perspective Node 2.1: "Security-first redesign" (based on Analysis 2)
├─ Perspective Node 3.1: "Performance optimization" (based on Analysis 3)
└─ Perspective Node 4.1: "Modular extraction" (based on Analysis 4)

Phase 3 - Evaluation:
├─ Score each perspective for:
│  ├─ Security improvement
│  ├─ Performance improvement
│  ├─ Code quality
│  ├─ Maintainability
│  └─ Risk level
└─ Rank by weighted scores

Phase 4 - Aggregation:
├─ Thought A: "Take security from Perspective 2.1"
├─ Thought B: "Add performance from Perspective 3.1"
├─ Thought C: "Ensure modularity from Perspective 4.1"
└─ Aggregation Node: "Synthesized architecture design"

Phase 5 - Implementation:
└─ "Generate code following synthesized design"
```

---

## Implementation Architecture

### Core Data Structures

```rust
// In crates/rustycode-reasoning/src/graph_of_thoughts/

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

/// A unit of reasoning - represents intermediate steps
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thought {
    pub id: ThoughtId,
    pub kind: ThoughtKind,
    pub content: String,
    pub metadata: ThoughtMetadata,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThoughtKind {
    Analysis,        // Understanding the problem
    Perspective,     // Potential solution approach
    Evaluation,      // Assessment of a perspective
    Aggregation,     // Combining multiple thoughts
    Implementation,  // Actual code generation
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThoughtMetadata {
    pub created_at: String,
    pub depth: u32,              // Distance from root
    pub score: Option<f32>,      // Quality score (0-1)
    pub parents: Vec<ThoughtId>, // Nodes this depends on
    pub children: Vec<ThoughtId>,// Nodes that depend on this
}

/// The reasoning graph - directed acyclic graph
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReasoningGraph {
    pub thoughts: HashMap<ThoughtId, Thought>,
    pub edges: Vec<(ThoughtId, ThoughtId)>, // (source, target)
}

impl ReasoningGraph {
    pub fn add_thought(&mut self, thought: Thought) {
        self.thoughts.insert(thought.id.clone(), thought);
    }

    pub fn add_edge(&mut self, from: ThoughtId, to: ThoughtId) {
        // Validate: no cycles
        if !self.would_create_cycle(&from, &to) {
            self.edges.push((from, to));
            // Update metadata
            if let Some(source) = self.thoughts.get_mut(&from) {
                source.metadata.children.push(to.clone());
            }
            if let Some(target) = self.thoughts.get_mut(&to) {
                target.metadata.parents.push(from);
            }
        }
    }

    fn would_create_cycle(&self, from: &ThoughtId, to: &ThoughtId) -> bool {
        // DFS to check if 'to' can reach 'from'
        // If yes, adding edge from->to creates cycle
        self.can_reach(to, from)
    }

    fn can_reach(&self, start: &ThoughtId, target: &ThoughtId) -> bool {
        let mut visited = HashSet::new();
        let mut queue = vec![start.clone()];

        while let Some(current) = queue.pop() {
            if current == *target {
                return true;
            }
            if visited.insert(current.clone()) {
                let children = self.thoughts[&current].metadata.children.clone();
                queue.extend(children);
            }
        }
        false
    }
}

/// Execution plan for solving a problem
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphOfOperations {
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Generate {
        from: ThoughtId,
        count: usize,
        prompt_template: String,
    },
    Aggregate {
        from_ids: Vec<ThoughtId>,
        aggregation_method: AggregationMethod,
        prompt_template: String,
    },
    Score {
        thought_id: ThoughtId,
        criteria: Vec<String>,
    },
    Refine {
        thought_id: ThoughtId,
        refinement_prompt: String,
    },
    Select {
        from_ids: Vec<ThoughtId>,
        count: usize,
        strategy: SelectionStrategy,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AggregationMethod {
    Consensus,    // Combine similar thoughts
    Synthesis,    // Create new thought from multiple
    Merge,        // Simple concatenation
    Voting,       // Weighted voting
    Structured,   // Template-based combination
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SelectionStrategy {
    TopK,
    Threshold,
    Diversity,
    Expert,
}

/// Runtime state tracking
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphReasoningState {
    pub graph: ReasoningGraph,
    pub scores: HashMap<ThoughtId, Score>,
    pub execution_log: Vec<ExecutionStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Score {
    pub criteria: HashMap<String, f32>, // criterion -> score (0-1)
    pub overall: f32,                    // weighted average
    pub reasoning: String,               // why this score
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionStep {
    pub operation: Operation,
    pub result: OperationResult,
    pub cost: TokenCost,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationResult {
    pub created_thoughts: Vec<ThoughtId>,
    pub status: OperationStatus,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OperationStatus {
    Success,
    PartialSuccess,
    Failed,
}
```

### Graph Executor

```rust
// In crates/rustycode-reasoning/src/graph_of_thoughts/executor.rs

use async_trait::async_trait;

pub struct GraphOfThoughtsExecutor {
    llm_provider: Arc<dyn LLMProvider>,
    prompter: Box<dyn Prompter>,
    parser: Box<dyn Parser>,
    scorer: Box<dyn Scorer>,
}

#[async_trait]
pub trait Prompter: Send + Sync {
    /// Convert a thought/operation into an LLM prompt
    async fn prompt_for_generation(
        &self,
        from: &Thought,
        count: usize,
    ) -> Result<String>;

    async fn prompt_for_aggregation(
        &self,
        thoughts: &[Thought],
        method: &AggregationMethod,
    ) -> Result<String>;

    async fn prompt_for_scoring(
        &self,
        thought: &Thought,
        criteria: &[String],
    ) -> Result<String>;
}

#[async_trait]
pub trait Parser: Send + Sync {
    /// Extract structured thoughts from LLM response
    async fn parse_generation(
        &self,
        response: &str,
        count: usize,
    ) -> Result<Vec<Thought>>;

    async fn parse_aggregation(
        &self,
        response: &str,
    ) -> Result<Thought>;

    async fn parse_score(
        &self,
        response: &str,
        criteria: &[String],
    ) -> Result<Score>;
}

#[async_trait]
pub trait Scorer: Send + Sync {
    /// Evaluate quality of thoughts
    async fn score_thought(
        &self,
        thought: &Thought,
        criteria: &[String],
    ) -> Result<Score>;
}

impl GraphOfThoughtsExecutor {
    pub async fn execute(
        &self,
        goo: &GraphOfOperations,
        initial_problem: &str,
    ) -> Result<GraphReasoningState> {
        let mut state = GraphReasoningState {
            graph: ReasoningGraph::default(),
            scores: HashMap::new(),
            execution_log: Vec::new(),
        };

        // Add initial thought
        let root = Thought {
            id: ThoughtId::new(),
            kind: ThoughtKind::Analysis,
            content: initial_problem.to_string(),
            metadata: ThoughtMetadata::default(),
        };
        state.graph.add_thought(root.clone());

        // Execute operations
        for operation in &goo.operations {
            let result = self.execute_operation(operation, &mut state).await?;
            state.execution_log.push(ExecutionStep {
                operation: operation.clone(),
                result,
                cost: TokenCost::default(),
                timestamp: chrono::Local::now().to_rfc3339(),
            });
        }

        Ok(state)
    }

    async fn execute_operation(
        &self,
        operation: &Operation,
        state: &mut GraphReasoningState,
    ) -> Result<OperationResult> {
        match operation {
            Operation::Generate { from, count, prompt_template } => {
                let source_thought = state.graph.thoughts[from].clone();
                let prompt = self.prompter
                    .prompt_for_generation(&source_thought, *count)
                    .await?;

                let response = self.llm_provider
                    .complete(CompletionRequest::new("gpt-4", vec![
                        ChatMessage::user(prompt),
                    ]))
                    .await?;

                let new_thoughts = self.parser
                    .parse_generation(&response.content, *count)
                    .await?;

                let mut created_ids = Vec::new();
                for thought in new_thoughts {
                    let id = thought.id.clone();
                    state.graph.add_thought(thought);
                    state.graph.add_edge(from.clone(), id.clone());
                    created_ids.push(id);
                }

                Ok(OperationResult {
                    created_thoughts: created_ids,
                    status: OperationStatus::Success,
                    error: None,
                })
            }

            Operation::Aggregate { from_ids, aggregation_method, .. } => {
                let source_thoughts: Vec<_> = from_ids
                    .iter()
                    .filter_map(|id| state.graph.thoughts.get(id).cloned())
                    .collect();

                let prompt = self.prompter
                    .prompt_for_aggregation(&source_thoughts, aggregation_method)
                    .await?;

                let response = self.llm_provider
                    .complete(CompletionRequest::new("gpt-4", vec![
                        ChatMessage::user(prompt),
                    ]))
                    .await?;

                let aggregated = self.parser
                    .parse_aggregation(&response.content)
                    .await?;

                let agg_id = aggregated.id.clone();
                state.graph.add_thought(aggregated);
                for from_id in from_ids {
                    state.graph.add_edge(from_id.clone(), agg_id.clone());
                }

                Ok(OperationResult {
                    created_thoughts: vec![agg_id],
                    status: OperationStatus::Success,
                    error: None,
                })
            }

            Operation::Score { thought_id, criteria } => {
                let thought = state.graph.thoughts[thought_id].clone();
                let score = self.scorer.score_thought(&thought, criteria).await?;
                state.scores.insert(thought_id.clone(), score);

                Ok(OperationResult {
                    created_thoughts: vec![],
                    status: OperationStatus::Success,
                    error: None,
                })
            }

            _ => todo!()
        }
    }
}
```

---

## Use Cases for Code Generation

### Use Case 1: Multi-File Refactoring

**Problem**: Refactor authentication module affecting 5 modules

**Decomposition**:
```
Analysis Phase:
├─ Analyze current auth module
├─ Identify security vulnerabilities
├─ Map dependencies
├─ Find performance bottlenecks
└─ Review for maintainability

Perspective Phase:
├─ Perspective A: Minimal OAuth2 upgrade
├─ Perspective B: Complete JWT migration
├─ Perspective C: Zero-trust architecture
└─ Perspective D: Microservice-based auth

Evaluation Phase:
├─ Score each on: security, performance, effort, risk
└─ Aggregate into ranked list

Synthesis Phase:
├─ Take security from B (JWT)
├─ Add performance optimization from D (separation)
├─ Reduce risk from A (incremental)
└─ Final: "Hybrid approach: JWT with separate service"

Implementation Phase:
└─ Generate refactored auth module code
```

**Benefit**: Synthesizes multiple valid approaches into optimal solution

### Use Case 2: Bug Fix with Root Cause Analysis

**Problem**: "Performance regression in search endpoint"

**Decomposition**:
```
Analysis Phase:
├─ Analyze search endpoint code
├─ Profile performance metrics
├─ Review recent changes
└─ Check database queries

Hypothesis Phase (Parallel):
├─ Hypothesis 1: Missing database index
├─ Hypothesis 2: N+1 query problem
├─ Hypothesis 3: Algorithm regression
└─ Hypothesis 4: Memory leak

Validation Phase:
├─ Design minimal tests for each hypothesis
├─ Execute tests in parallel
└─ Collect evidence

Root Cause Phase:
├─ Hypothesis 2 confirmed (N+1 queries)
├─ Identify new code causing it
└─ Propose fix

Implementation Phase:
└─ Generate optimized query code
```

**Benefit**: Systematically identifies root cause, not symptoms

### Use Case 3: Complex Feature Implementation

**Problem**: "Add real-time collaboration to document editor"

**Decomposition**:
```
Architecture Analysis:
├─ Current architecture review
├─ Real-time tech comparison (WebSocket, Server-Sent Events, polling)
├─ Scalability analysis
└─ Consistency model analysis

Design Perspective:
├─ Event-based design
├─ Operational transformation (OT)
├─ CRDT-based design
└─ Lock-based design

Evaluation:
├─ Score on: correctness, performance, complexity, maintainability
└─ Rank by weighted score

Aggregation:
├─ Take events from perspective A
├─ Use CRDT from perspective C (conflict-free)
├─ WebSocket from real-time tech (lowest latency)
└─ Final: "Event-driven CRDT over WebSocket"

Implementation:
└─ Generate collaboration module code
```

**Benefit**: Informed architecture decision based on multiple analyses

---

## Code Examples

### Example 1: Basic GoT Execution

```rust
use rustycode_reasoning::graph_of_thoughts::*;

async fn refactor_authentication() -> Result<()> {
    // 1. Create executor
    let executor = GraphOfThoughtsExecutor::new(
        Arc::new(anthropic_provider),
        Box::new(CodeGenPrompter),
        Box::new(CodeGenParser),
        Box::new(CodeGenScorer),
    );

    // 2. Define decomposition (Graph of Operations)
    let goo = GraphOfOperations {
        operations: vec![
            // Phase 1: Analysis
            Operation::Generate {
                from: root_id.clone(),
                count: 4,
                prompt_template: "Analyze [aspect] of authentication module".to_string(),
            },
            // Phase 2: Perspectives
            Operation::Generate {
                from: analysis_ids[0].clone(),  // From security analysis
                count: 3,
                prompt_template: "Design authentication based on [aspect]".to_string(),
            },
            // Phase 3: Evaluation
            Operation::Score {
                thought_id: perspective_id.clone(),
                criteria: vec![
                    "security".to_string(),
                    "performance".to_string(),
                    "maintainability".to_string(),
                    "implementation_effort".to_string(),
                ],
            },
            // Phase 4: Aggregation
            Operation::Aggregate {
                from_ids: top_perspective_ids,
                aggregation_method: AggregationMethod::Synthesis,
                prompt_template: "Synthesize best auth approach from:".to_string(),
            },
            // Phase 5: Implementation
            Operation::Generate {
                from: aggregated_id.clone(),
                count: 1,
                prompt_template: "Implement authentication following design:".to_string(),
            },
        ],
    };

    // 3. Execute
    let state = executor.execute(&goo, "Refactor authentication module").await?;

    // 4. Inspect results
    println!("Thoughts generated: {}", state.graph.thoughts.len());
    println!("Reasoning depth: {} levels", state.graph.max_depth());
    println!("Total cost: {} tokens", state.total_cost());

    // 5. Extract final code
    let final_thought = state.graph.leaf_nodes()
        .iter()
        .max_by_key(|id| state.scores[*id].overall)
        .unwrap();

    let final_code = &state.graph.thoughts[final_thought].content;
    println!("Generated code:\n{}", final_code);

    Ok(())
}
```

### Example 2: Custom Decomposition Strategy

```rust
pub struct CodeRefactoringDecomposition;

impl Decomposition for CodeRefactoringDecomposition {
    fn create_operations(&self, problem: &str) -> GraphOfOperations {
        let mut ops = vec![];

        // Step 1: Understand current state
        ops.push(Operation::Generate {
            from: root_id(),
            count: 1,
            prompt_template: format!(
                "Analyze current implementation of: {}",
                problem
            ),
        });

        // Step 2: Multiple perspectives
        ops.push(Operation::Generate {
            from: current_state_id(),
            count: 4,
            prompt_template: "Propose refactoring approach from perspective of: []".to_string(),
        });

        // Step 3: Score each perspective
        let perspectives = vec![/* ... */];
        for perspective_id in perspectives {
            ops.push(Operation::Score {
                thought_id: perspective_id,
                criteria: vec![
                    "Code quality".to_string(),
                    "Performance".to_string(),
                    "Maintainability".to_string(),
                    "Risk".to_string(),
                ],
            });
        }

        // Step 4: Aggregate top 3
        ops.push(Operation::Aggregate {
            from_ids: top_3_ids(),
            aggregation_method: AggregationMethod::Synthesis,
            prompt_template: "Combine best aspects of these approaches".to_string(),
        });

        // Step 5: Refine aggregation
        ops.push(Operation::Refine {
            thought_id: aggregated_id(),
            refinement_prompt: "Address potential issues in the combined approach".to_string(),
        });

        // Step 6: Generate implementation
        ops.push(Operation::Generate {
            from: refined_approach_id(),
            count: 1,
            prompt_template: "Implement the refactored code".to_string(),
        });

        GraphOfOperations { operations: ops }
    }
}
```

### Example 3: Real-Time Visualization

```rust
pub struct GoTVisualization {
    pub graph: ReasoningGraph,
    pub state: GraphReasoningState,
}

impl GoTVisualization {
    /// Render graph as mermaid diagram
    pub fn to_mermaid(&self) -> String {
        let mut mermaid = String::from("graph TD\n");

        for (id, thought) in &self.graph.thoughts {
            let label = format!("{}[{}]: {}", id, thought.kind, &thought.content[..50]);
            let score = self.state.scores.get(id)
                .map(|s| format!("({:.2})", s.overall))
                .unwrap_or_default();
            mermaid.push_str(&format!("    {}[\"{} {}\"]\n", id, label, score));
        }

        for (from, to) in &self.graph.edges {
            mermaid.push_str(&format!("    {} --> {}\n", from, to));
        }

        mermaid
    }

    /// Show full reasoning trace
    pub fn explain_reasoning(&self, thought_id: &ThoughtId) -> String {
        let thought = &self.graph.thoughts[thought_id];
        let score = self.state.scores.get(thought_id);
        let parents = &thought.metadata.parents;

        let mut explanation = String::new();
        explanation.push_str(&format!("## {}: {}\n\n", thought.kind, thought_id));
        explanation.push_str(&format!("Content: {}\n\n", thought.content));
        
        if let Some(score) = score {
            explanation.push_str(&format!("Score: {:.2}\n", score.overall));
            explanation.push_str("Criteria:\n");
            for (criterion, value) in &score.criteria {
                explanation.push_str(&format!("  - {}: {:.2}\n", criterion, value));
            }
            explanation.push_str(&format!("\nReasoning: {}\n\n", score.reasoning));
        }

        if !parents.is_empty() {
            explanation.push_str("Depends on:\n");
            for parent_id in parents {
                let parent = &self.graph.thoughts[parent_id];
                explanation.push_str(&format!("  - {}: {}\n", parent.kind, parent_id));
            }
        }

        explanation
    }
}
```

---

## Integration Steps

### Phase 1: Core Infrastructure (2 weeks)

**Step 1.1**: Create `crates/rustycode-reasoning/`
```
crates/rustycode-reasoning/
├── src/
│   ├── lib.rs
│   ├── graph_of_thoughts/
│   │   ├── mod.rs
│   │   ├── graph.rs          (ReasoningGraph, Thought, etc.)
│   │   ├── executor.rs       (GraphOfThoughtsExecutor)
│   │   ├── operations.rs     (GraphOfOperations)
│   │   └── visualization.rs  (Mermaid export, debugging)
│   └── prompting/
│       ├── code_prompter.rs
│       ├── code_parser.rs
│       └── code_scorer.rs
├── tests/
│   └── integration_tests.rs
└── Cargo.toml
```

**Step 1.2**: Implement core data structures
- `ReasoningGraph` with cycle detection
- `GraphOfOperations` with validation
- `GraphReasoningState` for runtime tracking

**Step 1.3**: Implement `GraphOfThoughtsExecutor`
- Generation, aggregation, refinement, scoring
- LLM integration
- Error handling and recovery

### Phase 2: Code Generation Patterns (2 weeks)

**Step 2.1**: Create decomposition strategies
- `RefactoringDecomposition`
- `BugFixDecomposition`
- `FeatureImplementationDecomposition`

**Step 2.2**: Implement prompter/parser for code
- Convert thoughts to code prompts
- Parse LLM responses into code

**Step 2.3**: Cost tracking
- Track tokens per operation
- Budget allocation per phase

### Phase 3: Integration with RustyCode (1 week)

**Step 3.1**: Integrate with orchestration
- Add GoT as alternative to direct generation
- Agent selection based on task complexity

**Step 3.2**: UI visualization
- Display reasoning graph in TUI
- Show thought hierarchy
- Explain decisions

### Phase 4: Optimization (1 week)

**Step 4.1**: Caching and optimization
- Cache intermediate thoughts
- Reuse analyses across similar tasks

**Step 4.2**: Tuning
- Branching factor optimization
- Budget allocation tuning

---

## File Structure for RustyCode

```
crates/rustycode-reasoning/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── graph_of_thoughts/
│   │   ├── mod.rs
│   │   ├── graph.rs
│   │   ├── executor.rs
│   │   ├── operations.rs
│   │   ├── visualization.rs
│   │   ├── decompositions/
│   │   │   ├── mod.rs
│   │   │   ├── refactoring.rs
│   │   │   ├── bug_fix.rs
│   │   │   └── feature_implementation.rs
│   │   └── prompting/
│   │       ├── mod.rs
│   │       ├── code_prompter.rs
│   │       ├── code_parser.rs
│   │       └── code_scorer.rs
│   └── metrics.rs
├── tests/
│   ├── graph_tests.rs
│   ├── executor_tests.rs
│   └── integration_tests.rs
└── examples/
    ├── refactoring.rs
    ├── bug_fixing.rs
    └── feature_implementation.rs
```

---

## Key Benefits for RustyCode

1. **Better Planning**: Multiple analyses inform architecture before coding
2. **Systematic Debugging**: Hypothesis-driven root cause analysis
3. **Cost Efficiency**: 31% cost reduction while improving quality
4. **Transparency**: Users see complete reasoning process
5. **Scalability**: Improves with problem complexity (exactly what code generation needs)
6. **Parallelization**: Multiple analyses run simultaneously
7. **Synthesis**: Final code informed by multiple perspectives

---

## References

- [Graph of Thoughts Paper](https://arxiv.org/abs/2308.09687)
- [Official GoT Implementation](https://github.com/spcl/graph-of-thoughts)
- [Demystifying Chains, Trees, Graphs](https://arxiv.org/html/2401.14295v6)
- [Original GoT Paper PDF](https://arxiv.org/pdf/2308.09687)

---

## Next Steps

1. **Create `rustycode-reasoning` crate** with GoT core
2. **Implement code-specific prompters** for various decompositions
3. **Integrate with orchestration** system for agent selection
4. **Add TUI visualization** for thought graphs
5. **Test on real refactoring tasks** and measure improvements
