# 06 — Teams & Ensembles

Teams coordinate multiple named agents on a single task through structured turn-taking.
Ensembles coordinate multiple teams with consensus mechanisms.

---

## TeamOrchestrator ✅

*`crates/rustycode-team/src/orchestrator.rs`*

```rust
pub struct TeamOrchestrator {
    project_root: PathBuf,
    client: Arc<dyn TeamLLMClient>,
    config: OrchestratorConfig,
    event_tx: tokio::sync::broadcast::Sender<TeamEvent>,
    agent_registry: std::sync::Mutex<AgentRegistry>,
    event_engine: std::sync::Mutex<EventEngine>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    pattern_miner: std::sync::Mutex<PatternMiner>,
    prompt_optimizations: std::sync::Mutex<Vec<PromptOptimization>>,
    tool_executor: LLMToolExecutor,
    tool_loop_config: ToolLoopConfig,
}

pub struct OrchestratorConfig {
    pub max_total_turns: u32,       // default 50
    pub max_retries_per_step: u32,  // default 3
    pub max_adaptations: u32,       // default 5
    pub max_response_tokens: u32,   // default 16384
    pub use_local_judge: bool,      // default true
}
```

### Execution Flow ✅

```
Profile task
  → AgentRegistry.agent_for_task()
  → PlanManager builds plan
  → [optional] Architect reviews plan
  → loop until Complete or budget exhausted:
      Builder executes step
      Skeptic reviews output
      Judge approves / requests changes
  → Record learnings and patterns
```

### TeamLLMClient Trait ✅

```rust
pub trait TeamLLMClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
    async fn complete_with_tools(&self, ...) -> Result<ToolResponse>;
    async fn complete_stream_with_tools(&self, ...) -> Result<StreamResponse>;
}
```

Two implementations: `RealLLMClient` (production), mock client (testing).

### Event System ✅

`TeamEvent` variants (tagged serde enum with `#[serde(tag = "type")]`) emitted via
`tokio::sync::broadcast::Sender<TeamEvent>`. Agents and external consumers subscribe
to the channel. Key event types include `AgentActivated`, `AgentStateChanged`,
`AgentDeactivated`, `StepCompleted`, `TaskCompleted`, `Insight`, `CodeChanged`,
`CompilationFailed`, `TestsFailed`.

### Pattern Mining ✅

`PatternMiner` records successful and failed approaches across team runs via
`ExecutionTrace` and `TurnTrace`. Mined patterns feed `PromptOptimization` entries,
enabling automatic prompt tuning and trust-based agent selection.

---

## Coordinator ✅

*`crates/rustycode-team/src/coordinator.rs`*

Per-session state machine for the Builder→Skeptic→Judge loop.

```rust
pub struct Coordinator {
    project_root: PathBuf,
    state: TeamLoopState,
    attempt_log: Vec<AttemptSummary>,
    insights: Vec<String>,
    structural_declaration: Option<StructuralDeclaration>,
    plan: Option<ConvoyPlan>,
}
```

### TurnOutcome ✅

```rust
pub enum TurnOutcome {
    ChangeProposed { files_changed: Vec<String>, approach_fingerprint: ApproachFingerprint },
    Approved,
    Vetoed { reason: String, evidence: String },
    Verified(VerificationState),
    Complete,
    Stop(StopReason),
    Escalate(Escalation),
}
```

`process_turn(TurnInput)` dispatches Builder, Skeptic, or Judge roles. `TurnInput`
accepts optional `builder_action`, `skeptic_review`, and `judge_results`.

### Doom Loop Detection ✅

The 3rd occurrence of the same `approach_fingerprint` (same `ApproachCategory` with
overlapping target files, checked via `is_repeating_approach()`) triggers
`TurnOutcome::Stop(StopReason::DoomLoop)`. Trust exhaustion is a separate check that fires
before the loop count. On doom detection, `builder_generation` is incremented and trust
records a `RepeatedFailure` event.

### Trust-Based Escalation ✅

Builder rotation on repeated failures via `TrustEventKind::RepeatedFailure`. The
`TrustScore` starts at 0.7 and adjusts per event:

| Event | Delta |
|-------|-------|
| `ClaimVerified` | +0.05 |
| `TaskCompleted` | +0.10 |
| `FixVerified` | +0.08 |
| `RepeatedFailure` | -0.10 |
| `ClaimRefuted` | -0.15 |
| `RegressionsIntroduced` | -0.15 |
| `CompilationFailed` | -0.05 |
| `HallucinationCaught` | -0.20 |

Thresholds: autonomous if >= 0.5, supervised if 0.25..0.5, escalate if < 0.25.
Three consecutive failed attempts trigger `should_rotate_builder()`.

### Additional Features ✅

- **Scalpel agent** — targeted fixes for specific issues found by Skeptic
- **Architect phase** — plan review before execution begins
- **ConvoyPlan integration** — structured task plans with dependencies
- **Hallucination detection** — flags outputs that don't match code state
- **Progress delta tracking** — `ProgressDelta` tracks test count changes; 2+ consecutive negative deltas triggers `Degrading`
- **Builder generation tracking** — tracks which builder produced what
- **Agent rotation** — `should_rotate_builder()` after 3 consecutive failures

---

## TeamContext ✅

*`crates/rustycode-team/src/team_context.rs`*

Aggregated context from a completed team execution.

```rust
pub struct TeamContext {
    pub team_id: String,
    pub task_id: String,
    pub agent_outcomes: Vec<AgentOutcome>,
    pub convergence: ConvergenceView,
    pub combined_changes: Vec<FileChange>,
    pub total_usage: TokenUsage,
}
```

The ensemble layer collects `TeamContext` from each team, then resolves consensus
and builds an aggregated `ConvergenceView`.

---

## Ensemble Pattern ✅

*`crates/rustycode-team/src/ensemble.rs`*

Multiple teams working on related sub-tasks with shared coordination.

### EnsembleOrchestrator ✅

```rust
pub struct EnsembleOrchestrator {
    pub config: EnsembleConfig,
    pub convergence: ConvergenceView,
}

pub struct EnsembleConfig {
    pub team_count: usize,
    pub strategy: EnsembleStrategy,
    pub total_token_budget: u64,  // 0 = unlimited
}

pub enum EnsembleStrategy {
    Majority,
    Unanimous,
    WeightedConfidence,
}
```

### EnsembleResult ✅

```rust
pub struct EnsembleResult {
    pub consensus: ConsensusResult,
    pub convergence: ConvergenceView,
    pub team_results: Vec<TeamContext>,
    pub total_tokens_used: u64,
}

pub enum EnsembleError {
    BudgetExceeded { teams_completed: usize, total_teams: usize, budget_used: u64, budget_limit: u64 },
    TeamFailed { team_id: String, reason: String },
}
```

### Execution Paths ✅

Two ways to run an ensemble:

1. **Async `run(executor)`** — spawns `team_count` tokio tasks, each calling the
   executor closure with a team index. Collects `TeamContext` results, enforces
   per-team budget, resolves consensus, builds convergence.

2. **Sync `resolve(results)`** — takes pre-built `Vec<TeamContext>`. Checks budget,
   resolves consensus, builds convergence. Used when team results are already
   available (e.g., from prior runs or testing).

### ConvergenceView ✅

*`crates/rustycode-team/src/convergence.rs`*

```rust
pub struct ConvergenceView {
    pub team_count: usize,
    pub max_confidence: f64,
    pub mean_confidence: f64,
    pub top_insights: Vec<Insight>,
    pub dissenting_opinions: Vec<DissentingOpinion>,
    pub convergence_achieved: bool,
}
```

`ConvergenceView::empty()` returns a zero-team view with all defaults. The ensemble's
`build_convergence()` aggregates across teams: takes max and mean confidence,
deduplicates and rank-sorts insights, and sets `convergence_achieved` when no
dissenting opinions exist.

### Consensus Mechanisms ✅

*`crates/rustycode-team/src/consensus.rs`*

```rust
pub enum ConsensusResult {
    Agreed(ConvergenceView),
    Dissent(Vec<DissentingOpinion>),
}
```

Three resolution functions, each taking `&[TeamContext]`:

| Strategy | Function | Resolution Rule |
|----------|----------|-----------------|
| `Majority` | `resolve_simple_majority()` | Groups teams by top insight content. Winner needs >50% (floor(n/2)+1). Ties broken by first appearance. |
| `WeightedConfidence` | `resolve_weighted_confidence()` | Each team's vote weighted by `convergence.max_confidence`. Highest total weight wins. |
| `Unanimous` | `resolve_unanimous()` | Every team must share the same top insight. A single dissenting team blocks consensus. |

All strategies return `ConsensusResult::Agreed(ConvergenceView)` on success or
`ConsensusResult::Dissent(Vec<DissentingOpinion>)` on failure. Empty team lists
return `Agreed` with an empty view.

### ConsensusEngine ✅

A convenience evaluator that checks `convergence_achieved` on a `ConvergenceView`:

```rust
pub struct ConsensusEngine;
impl ConsensusEngine {
    pub fn evaluate(view: &ConvergenceView) -> ConsensusResult;
}
```

### Ensemble Flow ✅

```
EnsembleConfig { team_count, strategy, budget }
  → EnsembleOrchestrator::new(config)
  → run(|team_index| async { TeamContext })
     ├── Spawn N tokio tasks
     ├── Each task calls executor, collects TeamContext
     ├── Budget enforcement per-team (BudgetExceeded on overrun)
     └── Aggregate results
  → resolve_consensus(&results)
     ├── Majority:   resolve_simple_majority()
     ├── Unanimous:  resolve_unanimous()
     └── Weighted:   resolve_weighted_confidence()
  → build_convergence(&results)
  → EnsembleResult { consensus, convergence, team_results, total_tokens_used }
```

### DissentingOpinion ✅

*`crates/rustycode-team/src/convergence.rs`*

```rust
pub struct DissentingOpinion {
    pub agent_id: String,
    pub team_id: String,
    pub opinion: String,
    pub confidence: f64,
    pub evidence: Vec<String>,
}
```

When no pre-existing dissenting opinions exist on a team, the consensus layer
synthesizes them from minority-position or non-converging teams. The synthesized
opinion includes the team ID, confidence score, and top insight content as evidence.

### Budget Enforcement ✅

`total_token_budget` in `EnsembleConfig` sets a hard cap across all teams. Budget
is divided evenly (`budget / team_count`). Enforcement happens in two places:

1. **Async path** — each spawned task checks cumulative usage before pushing its
   result. Exceeding the cap returns `EnsembleError::BudgetExceeded`.
2. **Sync path** — `resolve()` checks total usage upfront. Over-budget returns an
   `EnsembleResult` with `ConsensusResult::Dissent(vec![])` and empty convergence.

---

## CLI Mode ✅

*`crates/rustycode-protocol/src/modes.rs`*

`WorkingMode::Ensemble` variant wires ensemble orchestration into the CLI/TUI:

```rust
WorkingMode::Ensemble => {
    temperature: 0.15,
    max_iterations: 60,
    use_streaming: false,    // batches across teams
}
```

Selected via `--mode ensemble` on the CLI. The system prompt instructs the agent
to decompose tasks, dispatch parallel teams, and resolve consensus with the
configured strategy.
