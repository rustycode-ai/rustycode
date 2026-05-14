# 05 — Orchestration

Orchestration wraps single agents with tiered escalation, task state management, and
model routing. It sits between the raw agent engine and the team coordination layer.

---

## AgentSessionExecutor

*`crates/rustycode-orchestration/src/agent_executor.rs`*

Wraps `AgentSession` to implement the `ToolExecutor` trait used by the orchestration pipeline.
Bridges the agent runtime to the event bus and the approval UI.

```rust
pub struct AgentSessionExecutor {
    provider: Arc<dyn LLMProvider>,
    tool_registry: Arc<ToolRegistry>,
    system_prompt: String,
    model: String,
    config: AgentConfig,
    cwd: PathBuf,
    bus: BusHandle,
    interaction: Arc<Mutex<Option<Arc<dyn PipelineInteraction>>>>,
}
```

### Event Sink Modes

Three modes selected at construction time:

| Mode | Type | Behaviour |
|------|------|-----------|
| Silent | `BusAgentEvents` | Auto-approve, routes to `OrchestrationEvent` bus |
| Interactive | `BridgeEvents` | Approval gates via `PipelineInteraction` |
| Forwarding | `EventForwarder` | Streams all events to bus for external consumers |

---

## AgentRegistry

*`crates/rustycode-orchestration/src/agent_registry.rs`*

Catalog of available agent roles and dynamically-created specialists.

```rust
pub struct AgentRegistry {
    pub built_in: HashMap<String, AgentRole>,      // Architect, Builder, Skeptic, Judge, Scalpel
    pub generated: HashMap<String, SpecialistAgent>,
    pub task_history: Vec<TaskAgentMatch>,
}
```

### Agent Selection

`agent_for_task(&task, &profile) -> AgentSelection` returns one of:

| Selection | Meaning |
|-----------|---------|
| `StandardTeam { reason }` | Use default built-in roles |
| `Reuse { agent_id, reason }` | Reuse existing specialist (proven track record) |
| `NewSpecialist { agent_id, specialist_type, reason }` | Create specialist on the fly |

### Specialist Types

DatabaseMigration, SecurityAudit, TestDebugging, PerformanceOptimization, ApiIntegration

Exposed as global singleton via `global_agent_registry()` (`OnceLock`).

### Key Methods

| Method | Purpose |
|--------|---------|
| `agent_for_task()` | Select agent based on task and profile |
| `record_task_outcome()` | Record success/failure for learning |
| `find_by_capability()` | Find agents with a specific capability |
| `rank_by_success()` | Rank specialists by historical success rate |
| `mark_busy()` / `mark_available()` | Track agent availability |

### Planned: Lifecycle Orchestration

The registry will mediate lifecycle hooks integrated via `AgentPlugin`:
1. **Onboarding**: Upon agent registration, trigger `plugin.on_boarding()` to sync historical state from `Session`.
2. **Execution**: Standard tiered execution via `StepOrchestrator`.
3. **Offboarding**: Upon agent removal, trigger `plugin.on_offboarding()` to persist state before cleanup.

This is tracked in the implementation plan (Phase 2, Wave 5, tasks 2.16-2.18).

---

## StepOrchestrator

*`crates/rustycode-orchestration/src/orchestrator.rs`*

Coordinates five execution tiers. Each tier is a named role with increasing capability and cost.

```rust
pub struct StepOrchestrator {
    conductor: Arc<Conductor>,
    musician: Arc<Musician>,
    editor: Arc<Editor>,
    composer: Arc<Composer>,
    verification_gate: Arc<VerificationGateRegistry>,
    isolation: Arc<RwLock<TierIsolation>>,
    activation: Arc<RwLock<ToolActivationManager>>,
    budget_enforcer: Arc<RwLock<BudgetEnforcer>>,
    bus: BusHandle,
    ast_pipeline: RwLock<AstPipeline>,
    hooks: Option<Arc<RwLock<HookRegistry>>>,
    delegation_planner: DelegationPlanner,
}
```

### Tier System

| Tier | Name | Role | When Used |
|------|------|------|-----------|
| 1 | Conductor | Ultra-fast intent classification | Route initial request |
| 2 | Musician | Standard agentic execution | Default execution |
| 3 | Editor | Patch-level review and correction | Musician output needs fixes |
| 4 | Composer | Recompose from scratch | Patches fail, need fresh approach |
| 5 | Thinking | Extended reasoning with structured thinking | Last resort, complex problems |

### Lock Acquisition Order (Deadlock Prevention)

Always acquire in this order: `isolation` → `activation` → `budget_enforcer`.

### Escalation Path

```
Musician (tier 2)
  → Editor patches (tier 3)
    → Composer recomposes (tier 4)
      → Thinking with ReasoningGraph (tier 5)
```

### Orchestration Flow

1. `ModelRouter` determines tier and model requirements
2. `StepOrchestrator` dispatches to the appropriate tier
3. Tiered loop manages turn execution
4. On failure, escalation via `HandoffPackage`

---

## TaskContext

*`crates/rustycode-orchestration/src/task_context.rs`*

Transient execution state for one task. Intentionally does not fully round-trip through disk —
non-serialisable fields are `#[serde(skip)]`.

```rust
pub struct TaskContext {
    pub task_id: String,
    pub original_request: String,
    pub current_phase: TaskPhase,
    pub current_tier: u8,
    pub attempt_count: u8,
    pub cost_used: f64,
    pub budget_limit: f64,
    pub token_count: u64,
    pub execution_trace: ExecutionTrace,
    pub constraints: TaskConstraints,
    pub agent_role: AgentRole,
    pub classification_tier: ExecutionTier,
    pub execution_phase: ExecutionPhase,
    pub phase_skip: PhaseSkipConfig,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,

    // Ephemeral (not serialized):
    #[serde(skip)]
    pub workspace: Option<Arc<SharedWorkspace>>,
    #[serde(skip)]
    pub reasoning_graph: Option<ReasoningGraph>,
    pub conversation_history: Vec<Message>,
    #[serde(skip)]
    pub execution_limits: Option<ExecutionLimits>,
    #[serde(skip)]
    pub doom_loop_detector: Option<DoomLoopDetector>,
}
```

### TaskPhase

```rust
pub enum TaskPhase {
    Planning,
    Tier2Execution,
    Tier3Review,
    Tier4Recomposition,
    Tier5Thinking,
    Refining,
    Completed,
    Failed,
    Cancelled,
    Killed,
}
```

`TaskPhase::tier()` returns the tier number (0 for non-tier phases).
`TaskPhase::is_terminal()` returns true for Completed/Failed/Cancelled/Killed.

Key methods: `escalate()`, `advance_phase()`, `check_tool_limit()`, `check_doom_loop()`,
`check_before_tool_call()`.

---

## ModelRouter

*`crates/rustycode-orchestration/src/routing/model_router.rs`*

Routes tasks to execution tiers based on complexity classification.

```rust
pub struct RoutingPolicy {
    pub simple_tier: ExecutionTier,    // default: Musician
    pub moderate_tier: ExecutionTier,  // default: Editor
    pub complex_tier: ExecutionTier,   // default: Composer
}

pub struct ModelRouter {
    classifier: ComplexityClassifier,
    policy: RoutingPolicy,
}
```

`route(&TaskDescriptor) -> ExecutionTier` classifies complexity then maps to tier via policy.

### Model Catalog

*`crates/rustycode-providers/src/registry.rs`*

Predefined catalogs for 13 providers: Anthropic, OpenAI, OpenRouter, Gemini, Groq, GitHub
Copilot, ZhipuAI (GLM-4/5), Ollama, Kimi CN/Global, Alibaba CN/Global, Vertex, and LiteRT.

---

## AST Pipeline

*`crates/rustycode-orchestration/src/ast/`*

Composable AST analysis pipeline wired into `StepOrchestrator`. Used for code understanding
during execution.

## Hook Registry

*`crates/rustycode-orchestration/src/hook_points/`*

Optional `HookRegistry` wired into `StepOrchestrator` for pre/post execution hooks at the
orchestration level (distinct from agent-level `ExpandedHookDispatcher`).

## Delegation Planner

*`crates/rustycode-orchestration/src/`*

Plans which tasks to delegate to sub-agents vs handle in the current tier. Used when
`StepOrchestrator` determines that a task should be decomposed rather than escalated.
