# Unified Multi-Agent Architecture for RustyCode

**Date:** 2026-05-06
**Status:** Architecture Design Document
**Scope:** Synthesize 5 existing paradigms into unified peer-to-peer agent team system

---

## Executive Summary

RustyCode has **five distinct multi-agent paradigms** across 8 crates, totaling 150+ source files and ~20,000 lines of agent/orchestration code. **None of them implement true peer-to-peer agent teams.** This document defines a unified architecture that:

1. **Combines** all 5 paradigms into a single coherent system
2. **Extends** existing infrastructure rather than replacing it
3. **Surpasses** OpenCode (event-driven P2P), Claude Code (declarative sub-agents), Gemini (function calling), and Codex (code-specific) by unifying their best features

**Core Insight:** RustyCode's orchestration crate already has 90% of the infrastructure needed. The gap is a **messaging layer** that connects the existing pieces into peer-to-peer teams.

---

## Part 1: The Five Existing Paradigms

### Paradigm 1: Quality Loop Team (`rustycode-team`)

**Location:** `crates/rustycode-team/` (21 modules, ~3,500 LOC)

**Pattern:** Builder → Skeptic → Judge hierarchical pipeline with trust tracking

```
User Request
    │
    ▼
┌─────────────────┐
│  Coordinator    │ ← Trust tracking (0.0-1.0 per agent)
│  (doom loop     │ ← Approach fingerprint detection
│   detection)    │ ← Builder rotation on failure
└────────┬────────┘
         │
    ┌────┴────┬────────┐
    ▼         ▼        ▼
┌──────┐ ┌──────┐ ┌──────┐
│Builder│ │Skeptic│ │ Judge │
│(writes)│ │(reviews)│ │(rates) │
└──────┘ └──────┘ └──────┘
```

**Key Components:**
- `coordinator.rs` — `TurnOutcome`, `TeamLoopState`, trust tracking, doom loop detection via approach fingerprints
- `plan_manager.rs` — Plan adaptation (reorder/add/remove steps), retry with exponential backoff, user escalation
- `event_engine.rs` — **Phase 4 event-driven reactions**: `AgentListener` subscriptions, `AgentAction` proactive dispatch
- `agent_timeline.rs` — 12 `AgentState` variants: `Idle`, `Planning`, `Executing`, `Reviewing`, `Blocked`, `Failed`, `Completed`, etc.
- `agent_registry.rs` — **Re-exported from orchestration crate**: built-in + generated specialist agents

**Integration Points:**
- Re-exported by `rustycode-orchestration/src/team_registry.rs` as `TeamRegistry`
- Consumed by TUI agents via `rustycode_tui::agents::delegation_executor::TaskDispatcher`
- Event bus integration via `rustycode_bus::EventBus`

**Limitation:** Hierarchical, not peer-to-peer. Builder cannot message Skeptic directly — all coordination goes through Coordinator.

---

### Paradigm 2: Sub-Agent Delegation (`rustycode-tui/agents`)

**Location:** `crates/rustycode-tui/src/agents/` (4 modules, ~800 LOC)

**Pattern:** Single-session specialist delegation with worktree isolation

```
Main Session
    │
    ├─► spawn AgentSession (25-turn max, 900s timeout)
    │       ├── worktree isolation (git worktree)
    │       ├── tool filtering (subset of parent tools)
    │       └── returns result to parent
    │
    ├─► spawn another AgentSession (parallel)
    │
    └─► collect results, synthesize
```

**Key Components:**
- `agents/mod.rs` — `AgentMetrics` with atomic counters (spawned/completed/failed/cancelled/timed_out/retries)
- `agents/definitions.rs` — `AgentDefinition` { agent_type, label, when_to_use, system_prompt }
- `agents/delegation_executor.rs` — `DelegationPlanner`, `TaskRole` enum (Code/Plan/Review/Debug/Test), `SpawnDecision` (Inline/Spawn/SpawnParallel/Ensemble)
- `agents/agent_tool.rs` — `AgentTool` that launches `AgentSession` sub-agents with configurable tool sets

**Integration Points:**
- Uses `rustycode_agent_runtime::session::AgentSession::run()` for execution
- Integrates with `rustycode_orchestration::task_dispatcher::{TaskDispatcher, TaskResult}`
- Worktree isolation via `rustycode_orchestration::isolation::TierIsolation`

**Limitation:** Parent-child only. Sub-agents cannot communicate with each other. No persistent state between sessions.

---

### Paradigm 3: In-Memory Ensemble (`rustycode-runtime/multi_agent.rs`)

**Location:** `crates/rustycode-runtime/src/multi_agent.rs` (~300 LOC)

**Pattern:** Parallel LLM analysis with consensus voting (no persistence)

```
Task
    │
    ├─► Agent A (analysis)
    ├─► Agent B (analysis)
    ├─► Agent C (analysis)
    │
    ▼
ConsensusAggregator
    │
    ▼
Best result or merged output
```

**Key Components:**
- `MultiAgentOrchestrator` — Spawns multiple agents in parallel
- `AgentCommunicationHub` — In-memory message passing (no disk/JSONL)
- `EnsembleCoordinator` — Consensus voting with configurable threshold

**Limitation:** Ephemeral — all state lost when process ends. No inter-agent messaging during execution.

---

### Paradigm 4: Tiered Orchestration (`rustycode-orchestration`)

**Location:** `crates/rustycode-orchestration/src/` (150+ files, ~8,000 LOC)

**Pattern:** 4-tier model escalation with ensembles, fork-join, and conductor governance

```
Task
    │
    ▼
┌─────────────┐    success? → done
│  Musician   │    (Tier 2, fast model)
│  (execute)  │
└──────┬──────┘
       │ failed
       ▼
┌─────────────┐    patch → retry
│   Editor    │    (Tier 3, capable model)
│  (review)   │
└──────┬──────┘
       │ failed
       ▼
┌─────────────┐    recompose → retry
│  Composer   │    (Tier 4, best model)
│  (rewrite)  │
└──────┬──────┘
       │ failed
       ▼
┌─────────────┐    abandon + store pattern
│  Conductor  │    (budget/loop detection)
│  (govern)   │
└─────────────┘
```

**Key Components:**

| Module | File(s) | Purpose |
|--------|---------|---------|
| `agent_registry.rs` | `src/agent_registry.rs` | Specialist matching, task history, tool injection |
| `conductor.rs` | `src/conductor.rs` | Tier escalation, budget enforcement, hallucination detection |
| `ensemble_strategy.rs` | `src/ensemble_strategy.rs` | 4 strategies: DecomposeAndDelegate, ParallelVote, SequentialReview, Adversarial |
| `fork_join.rs` | `src/fork_join.rs` | True parallel execution via `tokio::JoinSet`, semaphore-bound (max 4), context snapshots |
| `task_dispatcher.rs` | `src/task_dispatcher.rs` | Bridges TaskSpec → ForkJoinExecutor (V1 placeholder, V2 → AgentSession) |
| `bus.rs` | `src/bus.rs` | 40+ event types: ForkStarted, TaskSpawned, EnsembleStarted, TierHandoff, etc. |
| `pipeline.rs` | `src/pipeline.rs` | Main `OrchestrationPipeline::conduct()` — wires all tiers together |
| `shared_workspace.rs` | `src/shared_workspace.rs` | Cross-agent artifact store with `WorkspaceEntry` { key, value, written_by, timestamp } |

**Orchestration Event Types (40+):**
- Task lifecycle: `TaskSpawned`, `TaskDelegationCompleted`, `TaskDelegationFailed`
- Plan lifecycle: `PlanCreated`, `PlanStepStarted`, `PlanStepCompleted`, `PlanCompleted`
- Ensemble: `EnsembleStarted`, `EnsembleCompleted`, `PartialResult`, `Objection`
- Fork-join: `ForkStarted`, `ForkCompleted`
- Budget/escalation: `EscalationSignal`, `ContextBudgetExceeded`, `TierHandoff`
- Tool execution: `ToolCallStarted`, `ToolCallCompleted`, `ToolInputDelta`
- Resource management: `ResourceIntent`, `ResourceConflict`

**Limitation:** Orchestration is task-centric, not agent-centric. Events describe what happened to a task, not what one agent wants another agent to do.

---

### Paradigm 5: High-Level Agent Types (`rustycode-agents`)

**Location:** `crates/rustycode-agents/src/` (8 modules, ~600 LOC)

**Pattern:** Trait-based agent definitions with specializations

```rust
pub trait Agent: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    async fn execute(&self, prompt: &str, context: Option<&str>, config: &AgentConfig) -> Result<AgentResult>;
    fn can_handle(&self, task_description: &str) -> bool;
}
```

**Implementations:**
- `CodeAgent` — Feature implementation, TDD patterns
- `ReviewAgent` — Code review, security audit, style checks
- `TestAgent` — Test generation, coverage analysis
- `DebugAgent` — Error trace analysis, root cause identification
- `Subagent` — Wrapper for `AgentSession` delegation

**Limitation:** No inter-agent communication protocol. Each agent is an island that talks to the LLM and tools, not to other agents.

---

## Part 2: The Gap — What's Missing for True P2P Agent Teams

### Gap 1: Agent-to-Agent Messaging

**Current:** Events are broadcast on `BusHandle` (pub/sub). Any agent can listen, but there's no way for Agent A to send a directed message to Agent B and wait for a response.

**OpenCode Solution:** JSONL inboxes per agent with auto-wake on append.

**Claude Code Solution:** Parent spawns child, child returns result. No peer communication.

**What's Needed:** Directed message passing with request/response pattern on top of existing `BusHandle`.

### Gap 2: Agent Identity and Capability Registry

**Current:** `AgentRegistry` tracks built-in roles and generated specialists, but agents cannot advertise capabilities or discover peers dynamically.

**OpenCode Solution:** Agent definitions with `when_to_use` fields, capability-based matching.

**Claude Code Solution:** YAML frontmatter in `.claude/agents/*.md` with `tools`, `model`, `system_prompt`.

**What's Needed:** Extend `AgentRegistry` with capability advertisements and dynamic discovery.

### Gap 3: Persistent Agent State

**Current:** `AgentTimeline` tracks 12 states per task, but not per agent. `AgentSession` is ephemeral (25-turn max, then discarded).

**OpenCode Solution:** JSONL inboxes persist across sessions.

**Claude Code Solution:** Sub-agents are stateless; state lives in parent session.

**What's Needed:** Agent state persistence layer (filesystem or SQLite) with resume capability.

### Gap 4: Cross-Agent Plan Delegation

**Current:** `TaskDispatcher` V1 routes through `ForkJoinExecutor` (placeholder). `DelegationPlanner` decides whether to spawn, but doesn't support agent-initiated delegation.

**OpenCode Solution:** Any agent can delegate to any other agent via inbox.

**Claude Code Solution:** Parent controls all delegation decisions.

**What's Needed:** Agent-initiated delegation with capability matching.

### Gap 5: Two-Level State Machines

**Current:** Single-level state machines (`AgentState` in timeline, `TaskPhase` in orchestration).

**OpenCode Solution:** Agent-level state (Idle, Working, Waiting) + Protocol-level state (Listening, Processing, Responding).

**What's Needed:** Separate agent lifecycle state from communication protocol state.

---

## Part 3: Unified Architecture Design

### Design Principles

1. **Extend, don't replace** — All 5 paradigms remain functional; the unified layer sits above them
2. **Event-driven backbone** — Build on existing `BusHandle` pub/sub infrastructure
3. **Agent-centric** — Shift from task-centric to agent-centric event model
4. **Capability-based routing** — Agents discover and delegate based on advertised capabilities
5. **Optional P2P** — Hierarchical delegation (Paradigm 2) still works; P2P is an opt-in enhancement

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        UNIFIED AGENT TEAM LAYER                              │
│  (NEW: sits on top of all 5 paradigms)                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐ │
│  │ AgentMailbox │  │AgentDirectory│  │TeamFormation │  │CrossAgentPlanner│ │
│  │  (inboxes)   │  │(capabilities)│  │  (topology)  │  │  (delegation)   │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └────────┬────────┘ │
│         │                 │                 │                   │          │
│         └─────────────────┴─────────────────┴───────────────────┘          │
│                                   │                                        │
│                                   ▼                                        │
│                         ┌──────────────────┐                               │
│                         │  AgentProtocol   │  ← Message envelope format    │
│                         │   (envelope)     │    + routing logic            │
│                         └────────┬─────────┘                               │
└──────────────────────────────────┼─────────────────────────────────────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Paradigm 4:    │    │  Paradigm 1:    │    │  Paradigm 2:    │
│ Orchestration   │◄──►│  Quality Loop   │    │  Sub-Agent      │
│ (conductor,     │    │  (coordinator,  │    │  Delegation     │
│  ensembles,     │    │  builder,       │    │  (AgentSession, │
│  fork-join)     │    │  skeptic, judge)│    │  worktree)      │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │
         ▼
┌─────────────────┐    ┌─────────────────┐
│  Paradigm 5:    │    │  Paradigm 3:    │
│  Agent Types    │    │  In-Memory      │
│  (CodeAgent,    │    │  Ensemble       │
│  ReviewAgent)   │    │  (consensus)    │
└─────────────────┘    └─────────────────┘
```

---

### New Component: AgentMailbox

**Purpose:** Directed messaging between agents with request/response pattern.

**Location:** `crates/rustycode-orchestration/src/mailbox.rs` (NEW)

**Design:**
```rust
pub struct AgentMailbox {
    agent_id: String,
    inbox: Arc<Mutex<VecDeque<AgentMessage>>>,
    bus: BusHandle,
    wake_notify: Arc<Notify>,  // tokio::sync::Notify for auto-wake
}

pub struct AgentMessage {
    pub id: Uuid,
    pub from: String,
    pub to: String,
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub reply_to: Option<Uuid>,
}

pub enum MessageType {
    Request,      // Requires response
    Response,     // Reply to request
    Broadcast,    // Fire-and-forget to all
    Delegate,     // Task delegation with return address
    Query,        // Capability query
    Advertise,    // Capability advertisement
}
```

**Key Behaviors:**
- **Auto-wake:** `AgentMailbox::recv()` uses `tokio::sync::Notify` to block until message arrives (like OpenCode's auto-wake)
- **JSONL persistence:** Inbox backed by append-only JSONL file (like OpenCode)
- **Request/Response:** `send_request()` returns a future that resolves when response arrives
- **Timeout:** Configurable timeout per message type (default: 300s)

**Integration with Existing:**
- Uses `BusHandle` for broadcast messages (leverages existing 40+ event types)
- Uses `SharedWorkspace` for large payloads (files, code blocks)
- Stores metadata in `AgentTimeline` (message counts, last seen)

---

### New Component: AgentDirectory

**Purpose:** Dynamic capability registry and agent discovery.

**Location:** `crates/rustycode-orchestration/src/directory.rs` (NEW)

**Design:**
```rust
pub struct AgentDirectory {
    agents: DashMap<String, AgentRecord>,  // concurrent hash map
    capabilities: DashMap<String, Vec<String>>,  // capability -> agent_ids
    bus: BusHandle,
}

pub struct AgentRecord {
    pub agent_id: String,
    pub agent_type: AgentType,  // BuiltIn | Specialist | Dynamic
    pub capabilities: Vec<Capability>,
    pub status: AgentStatus,    // Idle | Busy | Offline
    pub mailbox: Weak<AgentMailbox>,
    pub trust_score: f64,       // from coordinator
    pub success_rate: f64,      // from task_history
}

pub struct Capability {
    pub name: String,           // e.g., "code_review", "rust_debugging"
    pub proficiency: f64,       // 0.0-1.0, learned from history
    pub tools: Vec<String>,     // Tools this agent has access to
}
```

**Key Behaviors:**
- **Advertise:** Agents register capabilities on startup
- **Discover:** Query for agents by capability: `directory.find_agents("security_audit")`
- **Match:** Rank agents by proficiency × trust_score × availability
- **Learn:** Update proficiency from task outcomes (success/failure)

**Integration with Existing:**
- Extends `AgentRegistry` (from `agent_registry.rs`) with dynamic registration
- Uses `AgentRegistry::task_history` to compute success rates
- Leverages `Coordinator::trust_score` for ranking

---

### New Component: TeamFormation

**Purpose:** Dynamically assemble agent teams based on task requirements.

**Location:** `crates/rustycode-orchestration/src/team_formation.rs` (NEW)

**Design:**
```rust
pub struct TeamFormation {
    directory: Arc<AgentDirectory>,
    strategies: HashMap<TeamTopology, Box<dyn TeamStrategy>>,
}

pub enum TeamTopology {
    Hierarchy,      // Manager → Workers (like Claude Code sub-agents)
    Ring,           // Circular messaging (like OpenCode ring)
    Mesh,           // All-to-all (consensus)
    Star,           // Central coordinator (like existing quality loop)
    Pipeline,       // Sequential handoff (builder→skeptic→judge)
}

pub trait TeamStrategy: Send + Sync {
    fn form_team(&self, task: &str, directory: &AgentDirectory) -> Team;
    fn routing_table(&self, team: &Team) -> HashMap<String, Vec<String>>;  // who can message whom
}
```

**Topology Selection Logic:**
| Task Characteristic | Topology | Rationale |
|---------------------|----------|-----------|
| Simple, well-defined | Hierarchy | Parent delegates to child, child returns result |
| Complex, multi-domain | Star | Coordinator (from quality loop) manages specialists |
| Requires consensus | Mesh | All agents vote, ensemble strategy aggregates |
| Sequential dependency | Pipeline | Builder → Reviewer → Tester handoff |
| Parallel independent | Ring | Each agent handles one file/module |

**Integration with Existing:**
- Uses `EnsembleStrategy::select_for_complexity()` for initial topology hint
- Uses `AgentRegistry::agent_for_task()` for specialist selection
- Uses `ForkJoinExecutor` for parallel agent execution

---

### New Component: CrossAgentPlanner

**Purpose:** Enable agents to delegate sub-tasks to other agents dynamically.

**Location:** `crates/rustycode-orchestration/src/cross_agent_planner.rs` (NEW)

**Design:**
```rust
pub struct CrossAgentPlanner {
    directory: Arc<AgentDirectory>,
    dispatcher: TaskDispatcher,  // existing from task_dispatcher.rs
    mailbox_system: Arc<AgentMailboxSystem>,
}

impl CrossAgentPlanner {
    /// Agent A delegates a sub-task to Agent B
    pub async fn delegate(
        &self,
        from_agent: &str,
        task: &str,
        required_capabilities: &[String],
    ) -> Result<DelegationResult> {
        // 1. Find best agent(s) for capability
        let candidates = self.directory.find_agents(required_capabilities).await;
        
        // 2. Send delegation request via mailbox
        let target = candidates.best_available();
        let request = AgentMessage::delegate(from_agent, &target.agent_id, task);
        
        // 3. Wait for response (with timeout)
        let response = self.mailbox_system.send_request(target.mailbox(), request).await?;
        
        // 4. Record outcome for learning
        self.directory.record_outcome(&target.agent_id, task, response.success).await;
        
        Ok(DelegationResult::from(response))
    }
}
```

**Integration with Existing:**
- Uses `TaskDispatcher::dispatch()` for actual execution
- Uses `AgentRegistry::record_task_outcome()` for learning
- Uses `SharedWorkspace` for passing large contexts between agents

---

### New Component: AgentProtocol (Envelope Format)

**Purpose:** Standard message envelope for all inter-agent communication.

**Location:** `crates/rustycode-protocol/src/agent_protocol.rs` (EXTEND EXISTING)

**Current State:** Already has `AgentAction` and `AgentRole` enums.

**Extension:**
```rust
pub struct AgentEnvelope {
    pub version: u8,           // Protocol version (1)
    pub message_id: Uuid,
    pub from: AgentAddress,
    pub to: AgentAddress,      // Can be broadcast: "*" or "team:<team_id>"
    pub message_type: AgentMessageType,
    pub payload: AgentPayload,
    pub timestamp: DateTime<Utc>,
    pub ttl_seconds: u32,      // Time-to-live for message
}

pub enum AgentPayload {
    TaskDelegation { task: String, context: serde_json::Value, return_address: AgentAddress },
    TaskResult { task_id: Uuid, success: bool, output: String, artifacts: Vec<String> },
    CapabilityQuery { capabilities: Vec<String> },
    CapabilityResponse { agent_id: String, capabilities: Vec<Capability> },
    ConsensusVote { proposal_id: Uuid, vote: Vote, reasoning: String },
    Objection { target_message_id: Uuid, reason: String },
    Heartbeat,                // Agent health check
    StateSync { state: AgentStateSnapshot },  // For crash recovery
}
```

**Integration with Existing:**
- `OrchestrationEvent` in `bus.rs` becomes a subset of `AgentPayload` (task lifecycle events)
- `TeamEvent` in `protocol/src/team.rs` (22 variants) maps to `AgentPayload` variants
- Serialization via `serde_json` (consistent with rest of codebase)

---

## Part 4: Integration Points — How the 5 Paradigms Connect

### Integration Matrix

| Unified Component | Paradigm 1 (Quality Loop) | Paradigm 2 (Sub-Agent) | Paradigm 3 (Ensemble) | Paradigm 4 (Orchestration) | Paradigm 5 (Agent Types) |
|-------------------|---------------------------|------------------------|-----------------------|---------------------------|-------------------------|
| **AgentMailbox** | Coordinator uses mailbox to message Builder/Skeptic/Judge instead of direct method calls | AgentSession gets mailbox for parent-child async comms | Ensemble agents use mailbox for vote collection | All tiers (Musician/Editor/Composer) get mailboxes for cross-tier messaging | Each agent implementation gets mailbox for P2P |
| **AgentDirectory** | AgentRegistry extended with dynamic registration | AgentDefinition auto-registers on spawn | Ensemble agents register temporarily | SpecialistAgent records synced to directory | Agent trait extended with `capabilities()` method |
| **TeamFormation** | Quality loop = Pipeline topology | Sub-agent = Hierarchy topology | Ensemble = Mesh topology | Fork-join = Ring topology | Agent types used for capability matching |
| **CrossAgentPlanner** | PlanManager uses planner for step delegation | DelegationPlanner uses planner with directory | EnsembleCoordinator uses planner for parallel spawn | TaskDispatcher V2 uses planner for agent routing | Agents can self-delegate via planner |

### Detailed Integration: Quality Loop + AgentMailbox

**Current:** `Coordinator` directly calls `Builder::execute()`, `Skeptic::review()`, `Judge::verify()` via method invocations.

**Unified:** Each role gets an `AgentMailbox`. Coordinator sends `TaskDelegation` messages:

```rust
// OLD: Direct method call
let builder_result = builder.execute(task).await?;

// NEW: Async message passing
let request = AgentMessage::delegate("coordinator", "builder-1", task);
let builder_result = mailbox_system.send_request(builder_mailbox, request).await?;
```

**Benefit:** Builder can now delegate sub-tasks to other agents without Coordinator involvement. For example, Builder delegates "write tests" to `TestAgent` via directory lookup.

### Detailed Integration: Sub-Agent + AgentDirectory

**Current:** `AgentDefinition` has `when_to_use` field as free text. `DelegationPlanner` parses this text to decide spawning.

**Unified:** `AgentDefinition` auto-registers in `AgentDirectory` on spawn with parsed capabilities:

```rust
// In AgentSession::run() startup
let capabilities = parse_capabilities_from_definition(&self.definition);
directory.register(self.agent_id, capabilities).await;

// Parent can now discover child by capability, not just by name
let testers = directory.find_agents(&["test_generation", "rust"]).await;
```

**Benefit:** Dynamic discovery replaces hardcoded agent names. New agent types work immediately without code changes.

### Detailed Integration: Ensemble + TeamFormation

**Current:** `EnsembleStrategy` selects strategy based on complexity score (0-100) and uses hardcoded participant roles.

**Unified:** `TeamFormation::form_team()` uses `AgentDirectory` to select real agents based on capabilities:

```rust
// OLD: Hardcoded roles
let strategy = EnsembleStrategy::parallel_vote();  // roles: worker-a, worker-b, skeptic, judge

// NEW: Dynamic team from directory
let team = team_formation.form_team(task, &directory).await;
// Team might contain: [CodeAgent-7, ReviewAgent-3, TestAgent-12]
```

**Benefit:** Ensembles use actual available agents with proven track records, not placeholders.

### Detailed Integration: Orchestration + CrossAgentPlanner

**Current:** `TaskDispatcher` V1 routes through `ForkJoinExecutor` placeholder. No real LLM tool-use in forks.

**Unified:** `TaskDispatcher` V2 uses `CrossAgentPlanner` to find agents and `AgentMailbox` to delegate:

```rust
// In TaskDispatcher::execute_parallel()
for spec in specs {
    let agent = directory.find_best_agent_for(&spec.prompt).await?;
    let result = planner.delegate("dispatcher", &spec.prompt, &agent.capabilities).await?;
    results.push(result);
}
```

**Benefit:** Fork-join executes real agent sessions with full LLM tool-use, not placeholder tasks.

---

## Part 5: Reuse vs. Build-New

### Reuse (Minimal Changes)

| Component | File | What to Reuse | How |
|-----------|------|---------------|-----|
| **BusHandle** | `orchestration/src/bus.rs` | 40+ event types, pub/sub mechanism | Add `AgentEnvelope` as new event variant; no breaking changes |
| **SharedWorkspace** | `orchestration/src/shared_workspace.rs` | Artifact storage with provenance | Use for large message payloads (reference from envelope) |
| **AgentRegistry** | `orchestration/src/agent_registry.rs` | Specialist matching, task history | Extend with `AgentRecord` struct; keep existing API |
| **Coordinator** | `team/src/coordinator.rs` | Trust tracking, doom loop detection | Add mailbox field; use for cross-team messaging |
| **ForkJoinExecutor** | `orchestration/src/fork_join.rs` | Parallel execution, context snapshots | Replace `TaskRunner` with `AgentMailbox` dispatch |
| **EnsembleStrategy** | `orchestration/src/ensemble_strategy.rs` | 4 strategy kinds, weighted voting | Replace hardcoded roles with `AgentDirectory` lookups |
| **AgentSession** | `agent-runtime/src/session.rs` | 25-turn limit, tool filtering | Add mailbox integration; keep existing hard limits |
| **AgentTimeline** | `team/src/agent_timeline.rs` | 12 AgentState variants | Extend with messaging states: `WaitingForResponse`, `ProcessingMessage` |

### Build New

| Component | File | Purpose | Lines (est.) |
|-----------|------|---------|--------------|
| **AgentMailbox** | `orchestration/src/mailbox.rs` | Directed messaging with auto-wake | ~400 |
| **AgentMailboxSystem** | `orchestration/src/mailbox_system.rs` | Manages all mailboxes, routing | ~300 |
| **AgentDirectory** | `orchestration/src/directory.rs` | Dynamic capability registry | ~350 |
| **TeamFormation** | `orchestration/src/team_formation.rs` | Topology selection, team assembly | ~300 |
| **CrossAgentPlanner** | `orchestration/src/cross_agent_planner.rs` | Dynamic delegation | ~250 |
| **AgentProtocol** | `protocol/src/agent_protocol.rs` | Message envelope format (extend) | ~200 |
| **AgentStateMachine** | `orchestration/src/agent_state_machine.rs` | Two-level state machine | ~200 |
| **MailboxPersistence** | `orchestration/src/mailbox_persist.rs` | JSONL inbox backup/restore | ~150 |

**Total New Code:** ~2,150 lines (vs. ~20,000 existing lines reused)

---

## Part 6: How This Surpasses Competitors

### vs. OpenCode Agent Teams

| Feature | OpenCode | RustyCode Unified |
|---------|----------|-------------------|
| Messaging | JSONL inboxes, auto-wake | ✅ Same + typed envelopes + request/response |
| State Machine | Two-level (Agent + Protocol) | ✅ Same + 12 lifecycle states + timeline |
| Topology | Ring, mesh, star (configurable) | ✅ Same + Pipeline + Hierarchy (5 total) |
| Capability Discovery | Static YAML definitions | ✅ Dynamic learning from task history |
| Trust System | None | ✅ Trust score (0.0-1.0) + success rate tracking |
| Quality Gates | Basic approval | ✅ 4-tier escalation + ensemble strategies |
| Isolation | Process-based | ✅ Git worktree + tier isolation + fork-join |

**Advantage:** OpenCode has P2P messaging but lacks quality gates, tier escalation, and dynamic learning. RustyCode adds all three on top of equivalent messaging.

### vs. Claude Code Sub-agents

| Feature | Claude Code | RustyCode Unified |
|---------|-------------|-------------------|
| Definition | YAML frontmatter | ✅ Same + dynamic registration + capability ads |
| Delegation | Parent → Child only | ✅ Parent → Child + Peer → Peer |
| Tool Filtering | Subset of parent tools | ✅ Same + dynamic tool injection per capability |
| Worktree Isolation | Yes | ✅ Same + fork-join parallel isolation |
| State Machine | Single-level | ✅ Two-level + timeline + crash recovery |
| Consensus | None | ✅ Parallel vote with weighted agents |

**Advantage:** Claude Code is elegant but limited to hierarchy. RustyCode adds peer-to-peer, consensus, and dynamic team formation.

### vs. Gemini Function Calling

| Feature | Gemini | RustyCode Unified |
|---------|--------|-------------------|
| Parallel Calls | Yes (function calling) | ✅ Same + cross-agent planning |
| Agent Types | Single model | ✅ Specialist agents with different models |
| Learning | None | ✅ Capability proficiency learning |
| Teams | No | ✅ Full team orchestration |

**Advantage:** Gemini is model-centric; RustyCode is agent-centric with persistent learning.

### vs. Codex

| Feature | Codex | RustyCode Unified |
|---------|-------|-------------------|
| Code Focus | Yes | ✅ Same + review + test + debug agents |
| Multi-file | Yes | ✅ Same + parallel fork-join |
| Quality Gates | Basic | ✅ Builder-Skeptic-Judge loop |
| Autonomy | High | ✅ Same + budget enforcement + hallucination detection |

**Advantage:** Codex is the closest competitor, but RustyCode adds ensemble strategies, tier escalation, and cross-agent delegation.

---

## Part 7: Implementation Roadmap

### Phase 1: Foundation (2 weeks)

**Goal:** AgentMailbox + AgentProtocol

**Tasks:**
1. Extend `protocol/src/agent_protocol.rs` with `AgentEnvelope` and `AgentPayload` variants
2. Implement `AgentMailbox` with `tokio::sync::Notify` auto-wake
3. Implement `AgentMailboxSystem` for routing
4. Add JSONL persistence for crash recovery
5. Integrate with existing `BusHandle` (add `AgentEnvelope` as new event variant)

**Verification:** Two agents can send request/response messages via mailbox.

### Phase 2: Discovery (1 week)

**Goal:** AgentDirectory + dynamic registration

**Tasks:**
1. Implement `AgentDirectory` with `DashMap` concurrent storage
2. Add capability advertisement to `AgentConfig`
3. Auto-register agents on `AgentSession` spawn
4. Implement capability query/response protocol
5. Integrate with `AgentRegistry::task_history` for success rate computation

**Verification:** Agent A can discover Agent B by capability and send a message.

### Phase 3: Teams (2 weeks)

**Goal:** TeamFormation + topologies

**Tasks:**
1. Implement `TeamFormation` with 5 topology strategies
2. Add topology selection logic based on task characteristics
3. Implement routing tables per topology
4. Integrate with `EnsembleStrategy` for complexity-based hints
5. Add team lifecycle: form → execute → dissolve

**Verification:** Dynamic team assembled for task, executes, reports results.

### Phase 4: Planning (2 weeks)

**Goal:** CrossAgentPlanner + self-delegation

**Tasks:**
1. Implement `CrossAgentPlanner::delegate()`
2. Add agent-initiated delegation (agent can call planner without parent)
3. Integrate with `TaskDispatcher` V2 for real AgentSession execution
4. Add delegation chain tracking (A → B → C)
5. Implement delegation timeout and retry logic

**Verification:** Agent delegates sub-task to another agent, receives result.

### Phase 5: Integration (2 weeks)

**Goal:** Connect all 5 paradigms

**Tasks:**
1. Refactor `Coordinator` to use `AgentMailbox` for Builder/Skeptic/Judge
2. Refactor `DelegationPlanner` to use `AgentDirectory`
3. Refactor `EnsembleStrategy` to use real agents from directory
4. Refactor `TaskDispatcher` V2 to use `CrossAgentPlanner`
5. Add unified event types to `BusHandle`

**Verification:** All existing tests pass + new integration tests for P2P scenarios.

### Phase 6: Polish (1 week)

**Goal:** Documentation, benchmarks, TUI integration

**Tasks:**
1. Add TUI visualization for agent teams (agent graph, message flow)
2. Implement team performance benchmarks
3. Write user documentation for defining agents
4. Add example: "Build a microservice" with multi-agent team

**Total Duration:** 10 weeks (single developer)
**Total New Code:** ~2,150 lines
**Tests:** ~300 new tests (unit + integration)

---

## Appendix A: File Locations

### Existing Files (Reused)

| File | Lines | Role |
|------|-------|------|
| `crates/rustycode-orchestration/src/agent_registry.rs` | 631 | Specialist matching, task history |
| `crates/rustycode-orchestration/src/conductor.rs` | 685 | Tier escalation, budget, hallucination detection |
| `crates/rustycode-orchestration/src/ensemble_strategy.rs` | 784 | 4 ensemble strategies with weighted voting |
| `crates/rustycode-orchestration/src/fork_join.rs` | 898 | Parallel execution with context snapshots |
| `crates/rustycode-orchestration/src/task_dispatcher.rs` | 462 | TaskSpec → execution bridge |
| `crates/rustycode-orchestration/src/bus.rs` | 817 | 40+ event types, pub/sub |
| `crates/rustycode-orchestration/src/pipeline.rs` | 1011 | Main orchestration pipeline |
| `crates/rustycode-orchestration/src/shared_workspace.rs` | 181 | Cross-agent artifact store |
| `crates/rustycode-team/src/coordinator.rs` | ~400 | Trust tracking, doom loop detection |
| `crates/rustycode-team/src/event_engine.rs` | ~300 | Phase 4 event-driven reactions |
| `crates/rustycode-team/src/plan_manager.rs` | ~350 | Plan adaptation, retry, escalation |
| `crates/rustycode-team/src/agent_timeline.rs` | ~250 | 12 AgentState variants |
| `crates/rustycode-tui/src/agents/delegation_executor.rs` | ~400 | DelegationPlanner, TaskRole, SpawnDecision |
| `crates/rustycode-tui/src/agents/definitions.rs` | ~150 | AgentDefinition with when_to_use |
| `crates/rustycode-agent-runtime/src/session.rs` | ~300 | AgentSession with 25-turn limit |
| `crates/rustycode-agents/src/agent.rs` | 88 | Base Agent trait |
| `crates/rustycode-protocol/src/team.rs` | ~200 | TaskProfile, TrustScore, TeamEvent (22 variants) |
| `crates/rustycode-bus/src/lib.rs` | ~150 | EventBus with wildcards |

### New Files (To Build)

| File | Est. Lines | Role |
|------|------------|------|
| `crates/rustycode-orchestration/src/mailbox.rs` | 400 | AgentMailbox with auto-wake |
| `crates/rustycode-orchestration/src/mailbox_system.rs` | 300 | Routing, management |
| `crates/rustycode-orchestration/src/directory.rs` | 350 | Dynamic capability registry |
| `crates/rustycode-orchestration/src/team_formation.rs` | 300 | Topology selection |
| `crates/rustycode-orchestration/src/cross_agent_planner.rs` | 250 | Dynamic delegation |
| `crates/rustycode-orchestration/src/agent_state_machine.rs` | 200 | Two-level state machine |
| `crates/rustycode-orchestration/src/mailbox_persist.rs` | 150 | JSONL persistence |

---

## Appendix B: Example — Multi-Agent Code Review

**Task:** "Review the authentication module for security issues"

**Unified Execution:**

```
User Request
    │
    ▼
TeamFormation::form_team("review auth module")
    │
    ├─► SecurityAuditorAgent (specialist, from AgentRegistry)
    ├─► ReviewAgent (built-in, from AgentDirectory)
    ├─► TestAgent (built-in, for regression tests)
    │
    ▼
Star Topology (coordinator-led)
    │
    ├─► Coordinator sends "review auth module" to all 3 agents
    │
    ├─► SecurityAuditorAgent finds 2 vulnerabilities
    │       └─► Writes to SharedWorkspace: "auth/vulnerabilities"
    │
    ├─► ReviewAgent finds code style issues
    │       └─► Writes to SharedWorkspace: "auth/style_issues"
    │
    ├─► TestAgent runs tests, finds 1 failing test
    │       └─► Writes to SharedWorkspace: "auth/failing_tests"
    │
    ▼
Coordinator aggregates results from SharedWorkspace
    │
    ▼
EnsembleStrategy::parallel_vote() on fix priority
    │
    ├─► SecurityAuditorAgent: "CRITICAL: fix SQL injection"
    ├─► ReviewAgent: "LOW: rename variable"
    ├─► TestAgent: "HIGH: fix failing test"
    │
    ▼
Coordinator presents ranked results to user
```

**Key Interactions:**
- Agents communicate via `AgentMailbox` (directed messages)
- Results shared via `SharedWorkspace` (artifact storage)
- Team formed via `AgentDirectory` (capability matching)
- Consensus via `EnsembleStrategy` (weighted voting)
- Coordinator trust tracking via existing `Coordinator` (trust scores updated)

---

## Appendix C: Event Flow — Agent A Delegates to Agent B

```
Agent A (User-facing)
    │
    ├─► CrossAgentPlanner::delegate("implement feature X")
    │       │
    │       ├─► AgentDirectory::find_agents(["code_generation", "rust"])
    │       │       └─► Returns: [CodeAgent-7 (proficiency: 0.92), CodeAgent-3 (0.85)]
    │       │
    │       ├─► AgentMailboxSystem::send_request(to: CodeAgent-7)
    │       │       └─► AgentEnvelope { from: "Agent-A", to: "CodeAgent-7",
    │       │                         type: TaskDelegation, payload: {...} }
    │       │
    │       ▼
    │   CodeAgent-7 receives message (auto-wake via Notify)
    │       │
    │       ├─► AgentState transitions: Idle → ProcessingMessage
    │       ├─► AgentTimeline records: Received delegation from Agent-A
    │       │
    │       ├─► Executes task via AgentSession (25-turn limit)
    │       │       ├─► Uses tools (filtered subset)
    │       │       └─► Writes artifacts to SharedWorkspace
    │       │
    │       ├─► AgentState transitions: ProcessingMessage → Idle
    │       │
    │       └─► AgentMailboxSystem::send_response(to: Agent-A)
    │               └─► AgentEnvelope { from: "CodeAgent-7", to: "Agent-A",
    │                                 type: TaskResult, payload: {...} }
    │
    ▼
Agent A receives response
    │
    ├─► AgentDirectory::record_outcome(CodeAgent-7, success=true)
    │       └─► Updates proficiency: 0.92 → 0.93
    │
    ├─► SharedWorkspace::read("artifact-key") for detailed output
    │
    └─► Presents result to user
```

**Events Published to BusHandle:**
1. `TaskSpawned { task_id, role: "CodeAgent", parent_task_id: "Agent-A" }`
2. `ForkStarted { task_id, fork_id: "CodeAgent-7", fork_count: 1 }`
3. `WorkspaceUpdated { task_id, key: "artifact", written_by: "CodeAgent-7" }`
4. `ForkCompleted { task_id, fork_id: "CodeAgent-7", success: true }`
5. `TaskDelegationCompleted { task_id, role: "CodeAgent", output_preview: "..." }`

---

*Document Version: 1.0*
*Generated: 2026-05-06*
*Total Existing Infrastructure: ~20,000 lines across 8 crates*
*Total New Code Required: ~2,150 lines (10% of existing)*
