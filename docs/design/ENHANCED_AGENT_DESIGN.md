# Enhanced Agent Design — AutoAgent-Inspired Upgrades

**Date:** 2026-04-04
**Goal:** Transform rustycode's ensemble agents from "fixed roles + markdown memory" to "self-improving + capability-expanding" system

---

## Current Architecture (Baseline)

```
┌─────────────────────────────────────────────────────────────┐
│  Task → Architect → Plan → [Builder ↔ Skeptic ↔ Judge]     │
│                    ↑                                        │
│              reads TEAM_LEARNINGS.md                        │
│                    ↓                                        │
│              records "what worked/failed"                   │
└─────────────────────────────────────────────────────────────┘

Limitations:
- Learnings are text-only (no semantic search)
- Fixed agent roles (can't create new specialists)
- No few-shot examples in prompts
- Briefing is flat text (no structured retrieval)
```

---

## Proposed Enhancements

### 1. Vector Memory System (Replace/Augment TEAM_LEARNINGS.md)

**Inspiration:** AutoAgent's `RAGMemory` with ChromaDB

```rust
pub struct VectorMemory {
    project_path: PathBuf,
    collections: HashMap<MemoryType, CollectionId>,
    // Embedded learnings, task traces, code patterns
}

pub enum MemoryType {
    Learnings,      // What we currently store in TEAM_LEARNINGS.md
    TaskTraces,     // Full task histories with outcomes
    CodePatterns,   // Discovered patterns ("auth uses bcrypt")
    ToolUsage,      // How tools are used in this project
}
```

**Schema:**
```rust
pub struct MemoryEntry {
    pub id: String,           // UUID
    pub content: String,      // The actual memory
    pub metadata: MemoryMeta, // Type, confidence, source_task, timestamp
    pub embedding: Vec<f32>,  // 1536-dim (OpenAI) or 384-dim (BGE)
}

pub struct MemoryMeta {
    pub memory_type: MemoryType,
    pub confidence: f32,      // 0.0-1.0 (based on occurrences)
    pub source_task: String,  // Which task created this
    pub created_at: i64,      // Unix timestamp
    pub occurrence_count: u32, // How many times observed
}
```

**Query API:**
```rust
impl VectorMemory {
    pub fn search(&self, query: &str, memory_type: MemoryType, top_k: usize) -> Vec<MemoryEntry>;
    pub fn add(&mut self, content: String, memory_type: MemoryType, metadata: MemoryMeta);
    pub fn consolidate(&mut self); // Merge similar entries
}
```

**Benefits:**
- Semantic search: "What did we learn about auth?" returns relevant entries even if keywords don't match
- Cross-referencing: Task traces link to code patterns discovered
- Confidence decay: Old/low-confidence memories can be pruned

---

### 2. In-Context Learning Examples (Few-Shot Briefing)

**Inspiration:** AutoAgent's `add_in_context_learning_example`

**Current Briefing:**
```
Task: Fix auth token validation
Relevant Code: [src/auth.rs content]
Attempts: [...]
Insights: [...]
```

**Enhanced Briefing:**
```
Task: Fix auth token validation

[Similar Past Tasks] ← NEW
├─ Task #42: "JWT token expiration not checked"
│  └─ Solution: Added `exp` claim validation in src/auth.rs:145
└─ Task #38: "Token refresh endpoint missing"
   └─ Solution: Implemented /refresh endpoint with rate limiting

[Learnings from Memory]
├─ "Auth module uses bcrypt for password hashing" (confidence: 0.9)
└─ "Token validation must happen before /api routes" (confidence: 0.7)

[Code Patterns] ← NEW
└─ Pattern: "All auth handlers return Result<Token, AuthError>"

Current State: [...]
```

**Implementation:**
```rust
pub fn build_enhanced_briefing(task: &str, memory: &VectorMemory) -> Briefing {
    let similar_tasks = memory.search(task, MemoryType::TaskTraces, 3);
    let relevant_learnings = memory.search(task, MemoryType::Learnings, 5);
    let code_patterns = memory.search(task, MemoryType::CodePatterns, 3);
    
    Briefing {
        task: task.to_string(),
        few_shot_examples: format_examples(similar_tasks),
        retrieved_learnings: format_learnings(relevant_learnings),
        code_patterns: format_patterns(code_patterns),
        current_state: build_current_state(...),
    }
}
```

**Benefits:**
- Agents learn from *specific examples*, not just abstract learnings
- Faster convergence on solutions (no reinventing the wheel)
- Implicit knowledge transfer ("this is how we do things here")

---

### 3. Dynamic Agent Generation (Meta-Agent Pattern)

**Inspiration:** AutoAgent's `AgentCreatorAgent`

**Current:** Fixed roles (Architect, Builder, Skeptic, Judge, Scalpel)

**Enhanced:** On-demand specialist agents

```rust
// When Architect encounters novel task type:
pub fn create_specialist_agent(task_profile: &TaskProfile) -> Agent {
    match task_profile {
        TaskProfile::DatabaseMigration => Agent {
            name: "DatabaseMigrationAgent",
            role: "Execute safe database migrations with rollback capability",
            tools: vec![SchemaInspector, MigrationRunner, RollbackExecutor],
            instructions: "...",
        },
        TaskProfile::SecurityAudit => Agent {
            name: "SecurityAuditorAgent",
            role: "Review code for security vulnerabilities",
            tools: vec![CodeScanner, DependencyChecker, SecretDetector],
            instructions: "...",
        },
        _ => None, // Use standard ensemble
    }
}
```

**Agent Registry:**
```rust
pub struct AgentRegistry {
    built_in: HashMap<String, Agent>,         // Architect, Builder, etc.
    generated: HashMap<String, Agent>,        // Runtime-generated specialists
    task_history: Vec<(TaskProfile, AgentId)>, // Which agent solved what
}

impl AgentRegistry {
    pub fn get_agent_for_task(&mut self, task: &TaskProfile) -> AgentId {
        // Check if similar task was solved before
        if let Some(prev_agent) = self.find_similar_task(task) {
            return prev_agent;
        }
        // Create new specialist if needed
        if let Some(specialist) = create_specialist_agent(task) {
            let id = specialist.id.clone();
            self.generated.insert(id.clone(), specialist);
            return id;
        }
        // Fall back to standard ensemble
        AgentId::Architect
    }
}
```

**Benefits:**
- System *grows* capabilities over time
- Complex tasks get dedicated experts
- Knowledge accumulates in specialist agents

---

### 4. Event-Driven Agent Orchestration

**Inspiration:** AutoAgent's `EventEngine`

**Current:** Linear flow (Architect → Plan → Builder → Skeptic → Judge)

**Enhanced:** Event-driven coordination

```rust
pub enum EnsembleEvent {
    TaskStarted { task: String },
    CodeChanged { files: Vec<String> },
    TestFailed { test_name: String, error: String },
    CompilationError { file: String, line: u32 },
    SecurityIssueDetected { severity: Severity, location: CodeLocation },
    PatternDiscovered { pattern: String, confidence: f32 },
    // ... more events
}

pub struct AgentListener {
    agent_id: AgentId,
    subscribes_to: Vec<EnsembleEvent>,
    handler: fn(event: EnsembleEvent) -> AgentAction,
}

// Example: Skeptic automatically reviews when code changes
let skeptic_listener = AgentListener {
    agent_id: AgentId::Skeptic,
    subscribes_to: vec![EnsembleEvent::CodeChanged { .. }],
    handler: |event| {
        match event {
            EnsembleEvent::CodeChanged { files } => {
                AgentAction::ReviewCode(files)
            }
            _ => AgentAction::Noop,
        }
    },
};
```

**Benefits:**
- Agents react *proactively* (not just when scheduled)
- Parallel execution (multiple agents can handle same event)
- Easier to add new agents (just register listeners)

---

## Implementation Phases

### Phase 1: Vector Memory (Foundation) ✅ COMPLETE

**Status:** Implemented with BGE-Small embeddings via `fastembed` crate

- [x] Add `vector-memory` crate with pure Rust backend
  - Uses BGE-Small EN v1.5 embeddings (384-dim, semantic)
  - Cosine similarity search over stored memories
  - Persistence to `.rustycode/vector_memory/*.json`
- [x] Migrate `EnsembleLearnings` to use vector storage (keep markdown as fallback)
  - Hybrid approach: markdown for human readability, vector for semantic search
  - Dual-write: new learnings saved to both formats
- [x] Add `MemoryType::TaskTraces` for full task history
  - Four memory types: Learnings, TaskTraces, CodePatterns, ToolUsage
- [x] Wire into `BriefingBuilder::build()`
  - `load_project_learnings(task)` - semantic search for relevant learnings
  - `load_few_shot_examples(task)` - similar past tasks with outcomes

**Implementation Notes:**
- Using `fastembed` crate (BGE-Small EN v1.5) for real semantic embeddings
- Model downloads on first use (~70MB, cached in `~/.fastembed_cache/`)
- Mutex-wrapped embedder for thread-safe interior mutability
- Embeddings computed at init time for persisted entries

**Files:**
- `crates/rustycode-vector-memory/src/lib.rs` — Core vector memory with BGE embeddings
- `crates/rustycode-core/src/ensemble/briefing.rs` — Semantic search + few-shot examples
- `crates/rustycode-core/src/ensemble/orchestrator.rs` — Dual-write to markdown + vector

### Phase 2: In-Context Examples ✅ COMPLETE

**Status:** Implemented - few-shot examples now appear in briefings

- [x] Add `few_shot_examples` field to `Briefing` struct
- [x] Implement `load_few_shot_examples(task)` method
  - Searches `MemoryType::TaskTraces` for similar past tasks
  - Returns top 3 most similar tasks with relevance scores
- [x] Format examples in briefing for in-context learning

**Briefing Format:**
```
# Relevant Learnings

1. Tests live in /tests directory (relevance: 87%)
2. Auth module requires bcrypt hashing (relevance: 72%)

---
# Ensemble Learnings
[markdown content]

## Similar Past Tasks

### Task 1 (85% similar)
[SUCCESS] Task: Fix JWT token expiration validation

### Task 2 (73% similar)
[FAILED] Task: Implement rate limiting on auth endpoints

---
```

### Phase 3: Dynamic Agent Generation ✅ COMPLETE

**Status:** Implemented - AgentRegistry with 5 specialist types

- [x] Create `AgentRegistry` with built-in agents
  - Built-in: Architect, Builder, Skeptic, Judge, Scalpel, Coordinator
  - Generated: Specialist agents created on-demand
- [x] Implement `create_specialist_agent()` for 5 task types:
  - **DatabaseMigrationAgent** - Schema changes with rollback capability
  - **SecurityAuditorAgent** - OWASP Top 10 vulnerability scanning
  - **TestDebuggerAgent** - Flaky test investigation and fixes
  - **PerformanceOptimizerAgent** - Profiling and optimization
  - **ApiIntegrationAgent** - External service integration
- [x] Add task profile matching logic
  - `SpecialistType::from_task()` - Keyword-based task type detection
  - Agent selection reuses previously successful specialists
  - Task history tracks which agents succeeded for which task types
- [x] Domain-specific tools per specialist
  - Database: schema_inspector, migration_runner, rollback_executor
  - Security: code_scanner, dependency_checker, secret_detector
  - Test debugging: test_runner, flaky_test_detector, coverage_analyzer
  - Performance: profiler, benchmark_runner, memory_analyzer
  - API: http_client, oauth_handler, rate_limiter
- [x] Wire AgentRegistry into EnsembleOrchestrator
  - `EnsembleOrchestrator` now holds `Mutex<AgentRegistry>`
  - `execute()` calls `get_agent_for_task()` at startup
  - Task outcomes recorded via `record_task_outcome()` after completion
  - Events emitted for specialist creation/reuse

**Files:**
- `crates/rustycode-protocol/src/agent_registry.rs` — New module with AgentRegistry
- `crates/rustycode-protocol/src/lib.rs` — Re-exports for agent registry types
- `crates/rustycode-protocol/Cargo.toml` — uuid v7 feature added
- `crates/rustycode-core/src/ensemble/orchestrator.rs` — Integrated AgentRegistry into execute()

**Usage Example:**
```rust
let mut registry = AgentRegistry::new();
let selection = registry.get_agent_for_task(
    "Fix database migration rollback",
    &task_profile,
);

match selection {
    AgentSelection::NewSpecialist { agent_id, specialist_type, reason } => {
        // New DatabaseMigrationAgent created
    }
    AgentSelection::Reuse { agent_id, reason } => {
        // Reusing previously successful agent
    }
    AgentSelection::StandardEnsemble { reason } => {
        // Using built-in Architect/Builder/Skeptic/Judge
    }
}
```

### Phase 4: Event-Driven Orchestration ✅ COMPLETE

**Status:** Implemented - Event engine with pub/sub agent coordination

- [x] Extend `EnsembleEvent` enum with 11 new event types:
  - `CodeChanged` — Triggers proactive code review
  - `CompilationFailed` — Triggers Scalpel fix agent
  - `TestsFailed` — Triggers TestDebugger agent
  - `TrustChanged` — Tracks builder trust dynamics
  - `VerificationPassed` — Triggers performance/security scans
  - `PatternDiscovered` — Saves patterns to vector memory
  - `SecurityIssueDetected` — Triggers SecurityAuditor
  - `StructuralDeclarationSet` — Architect contract established
  - `PlanAdapted` — Plan modification event
  - `SpecialistCreated` — New specialist agent created
  - `ParallelExecutionRequested` — Multi-agent parallel execution

- [x] Create `EventEngine` module (`crates/rustycode-core/src/ensemble/event_engine.rs`)
  - `EnsembleEventType` — Event type enumeration for subscription matching
  - `AgentAction` — Actions agents can take in response to events
  - `AgentListener` — Subscribes to events, produces actions via handler function
  - `EventEngine` — Dispatches events to interested listeners, tracks history/stats

- [x] Register built-in listeners for standard ensemble:
  - **Skeptic** — Reviews on `CodeChanged`
  - **Scalpel** — Fixes on `CompilationFailed`, logs on `VerificationPassed`
  - **TestDebugger** — Debugs on `TestsFailed`
  - **SecurityAuditor** — Investigates on `SecurityIssueDetected`, scans on `CodeChanged`
  - **PerformanceOptimizer** — Runs on `VerificationPassed`

- [x] Integrate EventEngine into `EnsembleOrchestrator`:
  - Field: `event_engine: Mutex<EventEngine>`
  - Constructor auto-registers standard ensemble listeners
  - `emit_and_dispatch()` — Emits event and returns agent actions
  - Events emitted for: code changes, compilation failures, test failures

- [x] Wire events into execution flow (`execute_plan_step`):
  - After Builder modifies files → `CodeChanged` event
  - After compilation fails → `CompilationFailed` event
  - After tests fail → `TestsFailed` event
  - After verification passes → `VerificationPassed` event

**Files:**
- `crates/rustycode-core/src/ensemble/event_engine.rs` — NEW — Event engine implementation
- `crates/rustycode-core/src/ensemble/orchestrator.rs` — Integrated EventEngine, emits events
- `crates/rustycode-core/src/ensemble/mod.rs` — Re-exports event_engine types
- `crates/rustycode-core/src/ensemble/ensemble_status.rs` — Handles new events (no-op)
- `crates/rustycode-tui/src/ui/ensemble_panel.rs` — Handles new events (no-op)
- `docs/PHASE4_EVENT_DRIVEN_COMPLETE.md` — NEW — Phase 4 documentation

**Example:**
```rust
// Event engine automatically registered in EnsembleOrchestrator::new()
let orchestrator = EnsembleOrchestrator::new(project_root, provider, model);

// When code changes, Skeptic automatically reviews
let actions = orchestrator.emit_and_dispatch(EnsembleEvent::CodeChanged {
    files: vec!["src/auth.rs".to_string()],
    author: "Builder".to_string(),
    generation: 1,
});
// → actions contains {"Skeptic": AgentAction::ReviewCode { files: [...] }}

// When compilation fails, Scalpel automatically fixes
let actions = orchestrator.emit_and_dispatch(EnsembleEvent::CompilationFailed {
    errors: "error[E0308]: mismatched types".to_string(),
    files: vec!["src/lib.rs".to_string()],
    severity: "error".to_string(),
});
// → actions contains {"Scalpel": AgentAction::FixCompilation { errors, files }}
```

**Event Flow Diagram:**
```
Builder modifies code
       │
       ↓
  [CodeChanged event emitted]
       │
       ├─→ Skeptic listener → ReviewCode { files }
       ├─→ SecurityAuditor listener → SecurityScan { files }
       └─→ (other interested agents)

Compilation fails
       │
       ↓
  [CompilationFailed event emitted]
       │
       └─→ Scalpel listener → FixCompilation { errors, files }

Tests fail
       │
       ↓
  [TestsFailed event emitted]
       │
       └─→ TestDebugger listener → DebugTests { failed_tests }
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Vector search adds latency | Cache frequent queries, use local embeddings (BGE-small) |
| Generated agents are buggy | Validate agent definitions against schema before use |
| Memory bloat over time | Implement consolidation + TTL for low-confidence entries |
| Event system is complex | Start with simple pub/sub, add routing later |

---

## Success Metrics

1. **Task Completion Time:** Should decrease as memory grows
2. **Repeated Mistakes:** Should trend toward zero
3. **Novel Task Handling:** System should create specialists for new task types
4. **User Feedback:** "The ensemble feels smarter over time"

---

## Next Action

Start with **Phase 1: Vector Memory** — this is the foundation for all other enhancements. The current `EnsembleLearnings` module is well-designed and can be extended rather than replaced.

Key decision: Use embedded database (ChromaDB via Python, `chroma-py`) or pure Rust (pgvector with SQLite, or `hnsw` crate)?

Recommendation: Start with `hnsw` crate for pure-Rust, no-external-dependency approach. Add pgvector later for production deployments.
