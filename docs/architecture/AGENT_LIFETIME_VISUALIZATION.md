# Agent Lifetime Visualization

## Agent Lifecycle States

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         TASK LIFETIME TIMELINE                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  TaskProfiler   █████                                                         │
│  Coordinator    ████████████████████████████████████████████████████████      │
│  Architect      █████                         (High risk only)                │
│  Builder        ████████████████████████████████ (per step)                   │
│  Skeptic           █████   █████   █████        (after each Builder)          │
│  Judge               █████   █████   █████      (after each Skeptic)          │
│  Scalpel                      ████              (if compile errors)           │
│                                                                             │
│  Turn:          T0    T1    T2    T3    T4    T5    T6    T7                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

## State Machine per Agent

### Coordinator (Orchestrator)
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ Task received
            ▼
     ┌──────────────┐
     │  Profiling   │─────────────────────────┐
     └──────┬───────┘                         │
            │ Profile complete                 │
            ▼                                 │
     ┌──────────────┐     ┌───────────────┐   │
     │   Planning   │────►│   Executing   │───┤
     └──────────────┘     └───────┬───────┘   │
                                 │           │
                    ┌────────────┼───────────┘
                    │            │
         ┌──────────┼────────────┼──────────┐
         │          │            │          │
         ▼          ▼            ▼          ▼
    ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐
    │ Success│ │ Failure│ │ Doom   │ │ Budget │
    │        │ │        │ │ Loop   │ │ Exhaust│
    └────────┘ └────────┘ └────────┘ └────────┘
```

### Architect
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ High/Critical risk task
            │ OR Builder escalation
            ▼
     ┌──────────────┐
     │   Reading    │◄── Read-only tools only
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │  Analyzing   │
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │  Declaring   │──► StructuralDeclaration
     └──────────────┘
            │
            ▼
     ┌──────────────┐
     │   Complete   │ (never reactivated for same task)
     └──────────────┘
```

### Builder
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ Step assigned
            ▼
     ┌──────────────┐
     │  Reasoning   │◄── Full briefing + tools
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │ Implementing │◄── write_file, bash, etc.
     └──────┬───────┘
            │
     ┌──────┴──────┐
     │             │
     ▼             ▼
┌─────────┐   ┌─────────┐
│ Done    │   │Escalate │
│         │   │to Arch  │
└────┬────┘   └────┬────┘
     │             │
     ▼             ▼
┌─────────────────────────┐
│  Wait for Skeptic/Judge │
└─────────────────────────┘
```

### Skeptic
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ Builder turn complete
            ▼
     ┌──────────────┐
     │   Reviewing  │◄── Claims + diffs only
     └──────┬───────┘    (no builder reasoning)
            │
            ▼
     ┌──────────────┐
     │  Verifying   │◄── Read-only tools
     └──────┬───────┘
            │
     ┌──────┴──────┐
     │             │
     ▼             ▼
┌─────────┐   ┌─────────┐
│ Approve │   │  Veto   │
└────┬────┘   └────┬────┘
     │             │
     ▼             ▼
┌─────────────────────────┐
│  Coordinator processes  │
└─────────────────────────┘
```

### Judge
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ Skeptic approves
            ▼
     ┌──────────────┐
     │  Compiling   │◄── cargo check (local)
     └──────┬───────┘
            │
     ┌──────┴──────┐
     │             │
     ▼             ▼
┌─────────┐   ┌─────────┐
│ Success │   │ Errors  │
└────┬────┘   └────┬────┘
     │             │
     ▼             ▼
┌─────────┐   ┌─────────┐
│ Testing │   │Scalpel  │
└────┬────┘   └─────────┘
     │
     ▼
┌─────────────┐
│Test Results │
└─────────────┘
```

### Scalpel
```
     ┌──────────────┐
     │    Idle      │
     └──────┬───────┘
            │ Judge reports compile errors
            ▼
     ┌──────────────┐
     │  Diagnosing  │◄── Error messages only
     └──────┬───────┘
            │
            ▼
     ┌──────────────┐
     │   Fixing     │◄── Max 10 lines/file
     └──────┬───────┘    (surgical only)
            │
            ▼
     ┌──────────────┐
     │  Verifying   │◄── cargo check
     └──────┬───────┘
            │
     ┌──────┴──────┐
     │             │
     ▼             ▼
┌─────────┐   ┌─────────┐
│  Done   │   │Failed   │
└────┬────┘   └────┬────┘
     │             │
     ▼             ▼
┌─────────────────────────┐
│  Return to Judge        │
└─────────────────────────┘
```

## Sequence Diagram: Full Task Flow

```
┌─────┐  ┌──────────┐  ┌───────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌─────────┐
│Coord│  │Profiler  │  │Architect  │  │Builder │  │Skeptic │  │ Judge  │  │ Scalpel │
└──┬──┘  └────┬─────┘  └─────┬─────┘  └───┬────┘  └───┬────┘  └───┬────┘  └────┬────┘
   │          │              │            │           │           │            │
   │ Task     │              │            │           │           │            │
   ├─────────►│              │            │           │           │            │
   │          │              │            │           │           │            │
   │          │ Profile      │            │           │           │            │
   │◄─────────┤              │            │           │           │            │
   │          │              │            │           │           │            │
   │ [High Risk - Invoke Architect]       │           │           │            │
   │          │─────────────►│            │           │           │            │
   │          │              │            │           │           │            │
   │          │              │Declaration │           │           │            │
   │◄─────────┴──────────────┤            │           │           │            │
   │          │              │            │           │           │            │
   │ [Step 1: Understand]   │            │           │           │            │
   │          │              │            │           │           │            │
   │─────────────────────────┼───────────►│           │           │            │
   │          │              │            │           │           │            │
   │          │              │            │BuilderTurn│           │            │
   │◄─────────┴──────────────┴────────────┼───────────│           │            │
   │          │              │            │           │           │            │
   │ [Review] │              │            │           │           │            │
   │          │              │            │───────────┼──────────►│           │
   │          │              │            │           │           │            │
   │          │              │            │           │SkepticTurn│           │
   │◄─────────┴──────────────┴────────────┴───────────┼───────────│           │
   │          │              │            │           │           │            │
   │ [Verify] │              │            │           │           │            │
   │          │              │            │           │           │───────────►│
   │          │              │            │           │           │           │
   │          │              │            │           │           │◄──────────│
   │          │              │            │           │           │           │
   │          │              │            │           │           │JudgeTurn  │
   │◄─────────┴──────────────┴────────────┴───────────┴───────────┼───────────│
   │          │              │            │           │           │            │
   │ [... repeat per step ...]            │           │           │            │
   │          │              │            │           │           │            │
   │ Task Complete            │            │           │           │            │
   │          │              │            │           │           │            │
```

## Agent Activation Rules

| Agent | Activate When | Deactivate When |
|-------|---------------|-----------------|
| TaskProfiler | Task received | Profile produced |
| Coordinator | Task received | Task complete/failed |
| Architect | Risk=High/Critical OR Builder.escalation | Declaration produced |
| Builder | Each plan step | Step complete OR escalation |
| Skeptic | Builder complete, Risk>=Moderate | Verdict delivered |
| Judge | Skeptic approves | Verification complete |
| Scalpel | Judge reports compile errors | Errors fixed OR scope exceeded |

## Visualization Implementation

To visualize agent lifetime in real-time:

1. **Event Streaming**: Each agent emits events on state change
2. **Timeline View**: Horizontal bars showing active periods
3. **State Indicators**: Color-coded by state (Idle/Active/Complete)
4. **Interaction Log**: Click agent to see its turn history

Example implementation in `crates/rustycode-core/src/ensemble/agent_timeline.rs`:

```rust
pub struct AgentTimeline {
    task_id: String,
    start_time: Instant,
    agents: HashMap<AgentRole, AgentTrack>,
}

pub struct AgentTrack {
    role: AgentRole,
    events: Vec<TimelineEvent>,
}

pub enum TimelineEvent {
    Activated { turn: u32, reason: String },
    StateChange { from: AgentState, to: AgentState },
    Deactivated { turn: u32, reason: String },
}
```
