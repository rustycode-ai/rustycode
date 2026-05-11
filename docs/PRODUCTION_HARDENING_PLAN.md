# RustyCode Comprehensive Production Hardening Plan (Option C)

**Timeline:** 8-12 weeks  
**Goal:** Transform RustyCode from demo-grade to production-ready autonomous agent infrastructure  
**Baseline:** Twitter thread's 4 pillars (Building, Memory, Harness, Orchestration)

---

## Executive Summary

This plan addresses why AI agents fail in production (88% of projects never ship) by implementing engineering discipline across four critical pillars:

1. **Building** — Input validation, actionable error messages
2. **Memory** — Session isolation, no cross-contamination
3. **Harness** — Runtime safety (token limits, loop detection, privilege boundaries)
4. **Orchestration** — Multi-agent coordination, observability, typed contracts

**Output:** Production-ready RustyCode that can handle concurrent users, prevents silent failures, enforces privilege boundaries, and provides full observability.

---

## Phase 1: Audit & Assessment (Week 1-2)

### 1.1 Current State Analysis
**Deliverable:** Audit report identifying gaps in each pillar

#### Building Pillar Audit
- [ ] Tool validation coverage scan
  - Where are inputs validated vs. passed through?
  - Which tools have user input?
  - Error message quality check: are failures actionable?
  - Silent error swallowing test: simulate invalid inputs to all tools
  
  **Files to audit:**
  - `crates/rustycode-tools/src/bash.rs` — shell command validation
  - `crates/rustycode-tools/src/osv_check.rs` — input parsing
  - Tool registry: `crates/rustycode-tools/src/registry.rs`
  - All tool implementations in `crates/rustycode-tools/src/`

#### Memory Pillar Audit
- [ ] Session state isolation verification
  - Is user state properly namespaced?
  - Can concurrent requests from different users interfere?
  - Concurrent load test: 10+ simultaneous sessions
  - Check for class-level mutable defaults or shared state
  
  **Files to audit:**
  - `crates/rustycode-core/src/session.rs` — session boundaries
  - `crates/rustycode-storage/src/` — storage layer isolation
  - State management in `rustycode-orchestration`
  - Shared state in `Arc<RwLock<T>>` patterns — are they user-isolated?

#### Harness Pillar Audit
- [ ] Runtime orchestration robustness
  - Token accounting: is it comprehensive and accurate?
  - Error recovery: what prevents infinite loops?
  - Privilege boundaries: can one agent affect another?
  - Timeout handling: are runaway processes killed?
  
  **Files to audit:**
  - `crates/rustycode-orchestration/src/` — execution engine
  - `crates/rustycode-core/src/recovery/checkpoint.rs` — safety mechanism
  - Token tracking: `crates/rustycode-tools/src/cost_tracker.rs`
  - Async cancellation: is `.abort()` used? Graceful shutdown?

#### Orchestration Pillar Audit
- [ ] Multi-agent coordination
  - Are task contracts typed or loosely defined?
  - State isolation between sub-agents?
  - Distributed tracing: can we follow a request through all agents?
  - Data leakage prevention: do agents see other agents' state?
  
  **Files to audit:**
  - `crates/rustycode-team/src/` — agent coordination
  - `crates/rustycode-protocol/src/` — shared type contracts
  - Event bus: `crates/rustycode-bus/src/` — are messages tagged by user/session?
  - Tracing infrastructure (if any)

**Output:** 4 audit reports (one per pillar) with:
- Gaps identified
- Risk level (Critical/High/Medium)
- Current implementation notes
- Comparison to thread recommendations

### 1.2 God Object Dependency Mapping
- [ ] Create visual dependency graph for `rustycode-tools`, `rustycode-tui`, `rustycode-core`
- [ ] Identify cyclic dependencies (already know: llm ↔ tools)
- [ ] Count crate dependencies for each god object
- [ ] Plan extraction targets (e.g., which 5-10 modules should become separate crates?)

---

## Phase 2: Pillar 1 — Building (Input Validation & Error Clarity) (Week 3-4)

**Goal:** Every tool input is validated, errors are actionable, no silent failures

### 2.1 Input Validation Framework
**Scope:** All tools in `rustycode-tools/src/`

- [ ] Define validation schema for each tool type
  - Shell commands: allowed patterns, blocked patterns, max length
  - File operations: path whitelist/blacklist
  - API calls: rate limits, request size
  - Code parsing: max file size, timeout
  
- [ ] Create `InputValidator` trait
  ```rust
  pub trait InputValidator {
      type Input;
      type Error: std::error::Error;
      fn validate(&self, input: &Self::Input) -> Result<(), Self::Error>;
  }
  ```
  
- [ ] Implement validators for each tool
  - Bash: command pattern validation (already exists, extend it)
  - File I/O: path canonicalization + permission check
  - API: rate limiting decorator
  - Code: parser timeout + size limits

### 2.2 Error Message Quality
**Scope:** Every tool error must be actionable

- [ ] Audit all error types in `rustycode-tools`
  - Replace generic "failed" with specific, actionable messages
  - Include: what was attempted, why it failed, how to fix it
  
- [ ] Error response format (standardize)
  ```rust
  pub struct ToolError {
      code: String,           // e.g., "INVALID_PATH", "PERMISSION_DENIED"
      message: String,        // user-facing explanation
      details: String,        // technical details
      suggestion: String,     // how to fix
  }
  ```

- [ ] Test all error paths
  - Invalid inputs → clear error message
  - Permission denied → suggest how to grant permission
  - Timeout → suggest checking system load or timeout config

### 2.3 Tool Integration Tests
- [ ] Create `tests/input_validation/` with table-driven tests
  - Valid inputs → should succeed
  - Invalid inputs → should fail with correct error code
  - Edge cases (max length, special chars, null bytes)
  
- [ ] Coverage target: 95%+ of error paths exercised

**Output:** 
- Validated tool input schema (document)
- All tools pass validation tests
- Error message audit report

---

## Phase 3: Pillar 2 — Memory (State Isolation & Concurrency Safety) (Week 5-7)

**Goal:** Concurrent sessions from different users are completely isolated; no cross-contamination

### 3.1 Session State Isolation Audit
**Scope:** All state management layers

- [ ] Session boundaries verification
  - Every session has a UUID
  - All state is tagged with session ID
  - State lookup always filters by session ID
  - Test: verify request in session A cannot read state from session B
  
  **Files to verify:**
  - `crates/rustycode-core/src/session.rs`
  - `crates/rustycode-storage/src/session_*`
  - State in `rustycode-orchestration`

- [ ] Identify shared state risks
  - Static variables or class-level mutable defaults → refactor to per-session
  - Global registries → make session-aware or immutable
  - Cached data → tag with session/user ID
  
  **Test:** Run 10 concurrent sessions with overlapping tool calls
  ```
  Session A: read file X
  Session B: modify file X
  Session A: verify file X unchanged (in A's view)
  ```

### 3.2 Concurrent Load Testing
- [ ] Create load test harness
  - 10, 25, 50 concurrent sessions
  - Each session: random tool calls (file I/O, API, bash)
  - Monitor for race conditions, deadlocks, memory leaks
  
- [ ] Use tools:
  - `tokio::task::spawn` for concurrent sessions
  - `Arc<Mutex<>>` and `Arc<RwLock<>>` integrity checks
  - Memory profiling with valgrind or similar
  
- [ ] Pass criteria:
  - No panics
  - No data corruption
  - < 5% latency increase under 50× load
  - No memory leaks over 10 min run

### 3.3 Storage Layer Isolation
**Scope:** SQLite + JSON storage

- [ ] Verify storage queries filter by session/user
  - Every `.find()`, `.list()`, `.update()` must have session filter
  - Test: session A writes → session B cannot read it
  
- [ ] Add session_id to all storage schemas
  - Migrations to add session_id to existing tables
  - Default: sessions are isolated
  - Explicit `is_cross_session=true` for shared data only
  
- [ ] Transactional integrity
  - Concurrent writes to same resource → handled safely
  - Read-your-writes consistency
  - No partial updates visible to other sessions

### 3.4 State Contamination Prevention
- [ ] Identify mutable defaults in structs
  - Replace with per-session initialization
  - Test: create N instances → verify no shared state
  
- [ ] Input validation isolation
  - User input in session A cannot affect session B's state
  - Test: session A sends malformed input → session B unaffected

**Output:**
- State isolation verification report
- Concurrent load test harness
- Refactored isolation violations
- Migration scripts (if storage changes needed)

---

## Phase 4: Pillar 3 — Harness (Runtime Orchestration Safety) (Week 8-9)

**Goal:** The orchestration layer is bulletproof — no infinite loops, no runaway tokens, strong privilege boundaries

### 4.1 Token Accounting Consolidation
**Scope:** Token tracking across all LLM calls

- [ ] Inventory current token tracking
  - Where is it done? (orchestration? llm? cost tracker?)
  - What's the structure? (CostTracker? LLMProvider trait?)
  - Is it comprehensive (prompt + completion)?
  - Is it per-session or global?
  
  **Files to check:**
  - `crates/rustycode-tools/src/cost_tracker.rs`
  - `crates/rustycode-llm/src/` — provider implementations
  - `crates/rustycode-orchestration/src/` — execution tracking

- [ ] Consolidate token tracking
  - Central token counter: `TokenAccountant` trait
  - Every LLM call goes through it
  - Session-scoped: tokens per session quota
  - Provider-specific conversion (GPT-4 vs Claude vs others)
  
  ```rust
  pub trait TokenAccountant {
      fn track_prompt(&mut self, tokens: u32, session_id: &str) -> Result<()>;
      fn track_completion(&mut self, tokens: u32, session_id: &str) -> Result<()>;
      fn remaining_budget(&self, session_id: &str) -> u32;
      fn enforce_limit(&self, session_id: &str) -> Result<()>; // Error if over
  }
  ```

- [ ] Token budgets per session
  - Default: 100K tokens/session
  - Configurable per-session
  - Hard limit: stop execution if exceeded
  - Graceful degradation: warn at 80%, error at 100%

- [ ] Test: token counting accuracy
  - Send known-token prompts
  - Verify reported tokens match actual
  - Test all provider types

### 4.2 Infinite Loop & Runaway Process Prevention
**Scope:** Execution engine safeguards

- [ ] Identify loop detection mechanisms
  - What prevents an agent from retrying forever?
  - Current retry logic: where is it?
  - Backoff strategy: exponential? bounded?
  
  **Files to check:**
  - `crates/rustycode-orchestration/src/execute*.rs`
  - `crates/rustycode-core/src/recovery/`

- [ ] Implement execution limits
  - Max tool calls per session/step: configurable (default 20)
  - Max retries per tool: 3
  - Max total execution time: 30 min per session
  - Max model calls: 50 per session
  
  - [ ] Enforce via `ExecutionContext`:
    ```rust
    pub struct ExecutionContext {
        max_calls: u32,
        calls_made: u32,
        max_time: Duration,
        start_time: Instant,
        max_model_calls: u32,
        model_calls_made: u32,
    }
    impl ExecutionContext {
        fn assert_not_exceeded(&self) -> Result<()> { /* check limits */ }
    }
    ```

- [ ] Loop detection heuristics
  - Track last N tool calls: if same tool called 3× without state change → stop
  - Agent state tracking: if state hasn't changed in 5 iterations → escalate to human
  - Checkpoint comparison: if latest checkpoint == previous checkpoint → stop

- [ ] Timeout enforcement
  - Async task: wrap execution with `tokio::time::timeout()`
  - On timeout: checkpoint current state, cancel remaining tasks, return result

- [ ] Test: runaway prevention
  - Simulate infinite retry loop → verify stops at limit
  - Monitor: CPU, memory, time — all capped
  - Verify: checkpoint created before abort

### 4.3 Privilege Boundaries
**Scope:** Agent isolation and permission gating

- [ ] Current permission model
  - How are tool permissions defined? (files, commands, APIs)
  - Who enforces them? (rustycode-tools? rustycode-guard?)
  - Can one agent escalate or bypass?
  
  **Files to check:**
  - `crates/rustycode-guard/src/` (if exists)
  - `crates/rustycode-tools/src/bash.rs` — command validation
  - `crates/rustycode-tools/src/security.rs`
  - `crates/rustycode-team/src/` — agent isolation

- [ ] Privilege enforcement
  - Each agent has a capability set: read, write, execute, network
  - Runtime enforces before each tool call
  - Tool output is isolated: no cross-agent data access
  
  - [ ] Create `PrivilegeGate` trait:
    ```rust
    pub trait PrivilegeGate {
        fn can_read(&self, agent_id: &str, path: &Path) -> bool;
        fn can_write(&self, agent_id: &str, path: &Path) -> bool;
        fn can_execute(&self, agent_id: &str, cmd: &str) -> bool;
    }
    ```

- [ ] Dangerous operations whitelist
  - File deletion: explicit approval + confirmation
  - Environment variable modification: log + audit
  - Privilege escalation (sudo, etc.): denied
  - Network access: restricted to allowed domains

- [ ] Test: privilege bypass attempts
  - Agent A tries to read agent B's temp files → denied
  - Agent tries to escalate to root → denied
  - Symlink attacks on file operations → blocked
  - Command injection in shell tools → sanitized

### 4.4 Error Recovery & Graceful Degradation
**Scope:** What happens when things fail?

- [ ] Failure modes documentation
  - LLM provider down → fallback or queue?
  - Storage error → checkpoint and retry?
  - Tool timeout → skip and continue?
  - Permission denied → human intervention?
  
- [ ] Graceful degradation strategy
  - Tier 1 (Recoverable): Log, retry with backoff, continue
  - Tier 2 (Degraded): Warn user, skip feature, continue
  - Tier 3 (Fatal): Checkpoint state, pause execution, request human

- [ ] Error budget per session
  - Track error frequency
  - If > threshold: escalate instead of retry
  - Prevent thrashing

**Output:**
- Token accounting system refactor plan
- Execution limits implementation spec
- Loop detection strategy document
- Privilege boundary enforcement spec
- Error recovery matrix

---

## Phase 5: Pillar 4 — Orchestration (Multi-Agent Coordination & Observability) (Week 10-11)

**Goal:** Multi-agent systems are coordinated, observable, and safe from data leakage

### 5.1 Typed Task Contracts
**Scope:** Multi-agent communication

- [ ] Current task definition
  - How are tasks passed between agents?
  - What's the contract? (files in `rustycode-protocol`?)
  - Type safety: are inputs/outputs validated?
  
  **Files to check:**
  - `crates/rustycode-protocol/src/` — shared types
  - `crates/rustycode-team/src/` — task distribution

- [ ] Define Task trait
  ```rust
  pub trait Task: Send + Sync {
      type Input: Serialize + Deserialize;
      type Output: Serialize + Deserialize;
      
      fn validate_input(&self, input: &Self::Input) -> Result<()>;
      fn schema() -> JsonSchema; // For validation
  }
  ```

- [ ] Task registry
  - Central registry of all task types
  - Each task has: name, input schema, output schema, timeout, retry policy
  - Type-checked at compile time where possible
  - Runtime validation for dynamic tasks

- [ ] Contract enforcement
  - Before task handoff: validate input against schema
  - After task completion: validate output against schema
  - Mismatch → error, not silent failure

### 5.2 Distributed Tracing & Observability
**Scope:** Follow a request through all agents and systems

- [ ] Tracing infrastructure
  - Add `tracing` crate: structured logging + spans
  - Every public API call gets a span
  - Spans include: session_id, agent_id, task_id, user_id
  
  - [ ] Instrument key operations:
    ```rust
    let span = tracing::info_span!("execute_tool", 
        session_id = %session.id,
        tool = %tool_name,
        agent_id = %agent.id
    );
    let _guard = span.enter();
    // execution here
    ```

- [ ] Trace propagation
  - Every async task inherits parent span
  - Tool calls are child spans
  - LLM calls are child spans
  - Can reconstruct full call tree from logs

- [ ] Metrics collection
  - Duration per operation
  - Success/failure rates
  - Token usage per agent
  - Error frequency
  
  - [ ] Export to observability platform (Jaeger, Datadog, etc.)

- [ ] Log aggregation
  - Structured JSON logs with session/agent context
  - Searchable by: user, session, agent, timerange, error type
  - Retention: 30 days

### 5.3 Multi-Agent State Isolation
**Scope:** Agents don't interfere with each other

- [ ] Agent state partitioning
  - Each agent gets isolated workspace
  - Temp files, caches, state → agent-specific directories
  - Shared resources: explicit and read-only
  
- [ ] Task result containment
  - Task output is returned to parent, not broadcast
  - Sub-agent cannot read sibling's files or state
  - Parent agent controls what to share with siblings

- [ ] Communication channels
  - Explicit: parent→child, child→parent
  - Implicit: shared nothing
  - Test: verify agent isolation with concurrent agents

### 5.4 Deadlock & Circular Dependency Prevention
**Scope:** Agent coordination deadlock-free

- [ ] Dependency graph
  - Task dependencies must be acyclic (DAG)
  - Detect cycles at task submission time
  - Error if cycle detected
  
- [ ] Timeout enforcement
  - Parent task waits for children with timeout
  - If child hangs → parent escalates
  - No unbounded waits

- [ ] Deadlock detection
  - Monitor for: all agents blocked, none making progress
  - Heuristic: if no events for 60 sec and all blocked → escalate

### 5.5 Observability Tests
- [ ] Create test harness for multi-agent scenario
  - 3+ agents working on a task
  - Inter-agent communication
  - Verify tracing captures all steps
  - Verify isolation maintained
  - Measure overhead: <5% perf impact from tracing

**Output:**
- Task contract schema spec
- Tracing instrumentation plan
- Multi-agent isolation verification report
- Observability metrics dashboard setup

---

## Phase 6: God Object Refactoring (Week 12+, concurrent with Phase 5)

**Scope:** Break up large crates into focused, single-responsibility crates

### 6.1 `rustycode-tools` Decomposition (50+ modules → 5 crates)
- [ ] `rustycode-tools-api`: Traits only (ToolExecutor, InputValidator, PrivilegeGate)
- [ ] `rustycode-tools-core`: Core implementations (bash, file I/O, registry)
- [ ] `rustycode-tools-security`: Permission gating, privilege checks
- [ ] `rustycode-tools-config`: Tool configuration, schema
- [ ] `rustycode-tools-ext`: Extensions (OSV check, custom tools)

**Dependency:** api ← {core, security, config, ext}

### 6.2 `rustycode-tui` Decomposition (22 dependencies → 3 crates)
- [ ] `rustycode-ui-core`: UI components, layout, rendering
- [ ] `rustycode-ui-session`: Session-specific UI (REPL, chat, plan display)
- [ ] `rustycode-ui-debug`: Debug panels, inspector, log viewer

**Dependency:** session, debug ← core

### 6.3 `rustycode-core` Consolidation (18 modules → refactor)
- [ ] Separate concerns: session, recovery, execution, messaging
- [ ] Extract to new crates if > 3K LOC per concern
- [ ] Consider: `rustycode-execution`, `rustycode-recovery`, `rustycode-messaging`

### 6.4 Fix Circular Dependency (llm ↔ tools)
- [ ] Move to `rustycode-tool-integration`:
  - ToolProfile, ToolRegistry, ToolSelector
  - SearchStrategy, default_registry, route_query
- [ ] Update imports: llm and tools both depend on tool-integration
- [ ] Verify no cycles with `cargo tree --duplicates`

**Output:**
- Refactoring roadmap (crate split plan)
- Migration guide for consumers
- New dependency graph

---

## Phase 7: Comprehensive Testing & Verification (Week 13, concurrent)

**Goal:** 80%+ coverage, all critical paths exercised, production readiness verified

### 7.1 Test Matrix
- [ ] Unit tests: Input validation, state isolation, token accounting
- [ ] Integration tests: Multi-agent, concurrent sessions, error recovery
- [ ] Stress tests: 100 concurrent sessions, 1M+ tool calls, memory stability
- [ ] Chaos tests: Simulate failures (LLM down, storage error, network timeout)
- [ ] Security tests: Privilege escalation attempts, symlink attacks, injection

### 7.2 Coverage Report
- [ ] Generate: `cargo tarpaulin --workspace`
- [ ] Target: 80% overall, 95% for critical paths (tools, orchestration, security)
- [ ] Identify gaps: missing error paths, untested branches
- [ ] Document exceptions: hard-to-test code with rationale

### 7.3 Production Readiness Checklist
- [ ] All 4 pillars pass audit
- [ ] No panics in production code (unwrap/expect eliminated)
- [ ] All errors are typed, contextual, actionable
- [ ] Load test: 100 concurrent sessions, < 5% latency degradation
- [ ] Stress test: 24h run, zero memory leaks, zero panics
- [ ] Chaos test: 90%+ recovery from simulated failures
- [ ] Security test: all privilege escalation attempts blocked
- [ ] Code review: 2 reviewers approve all Phase 1-6 changes
- [ ] CI/CD: all tests pass, clippy passes, fmt passes

---

## Phase 8: Documentation & Knowledge Transfer (Week 14)

**Output:** Production operations handbook

- [ ] Architecture documentation (updated)
- [ ] Deployment guide
- [ ] Observability guide (how to read traces, metrics)
- [ ] Troubleshooting guide (common issues + recovery)
- [ ] Runbook: incident response procedures
- [ ] Migration guide: for existing users upgrading

---

## Summary & Metrics

| Phase | Duration | Deliverable | Pass Criteria |
|-------|----------|-------------|---------------|
| 1 | 2 weeks | Audit reports (4) | 0 Critical gaps, ≤5 High gaps |
| 2 | 2 weeks | Tool validation framework | 95%+ input coverage, all errors actionable |
| 3 | 3 weeks | State isolation verification | 100+ concurrent sessions, 0 cross-contamination |
| 4 | 2 weeks | Harness safety hardening | Token budget enforced, loops prevented, privileges gated |
| 5 | 2 weeks | Multi-agent observability | Full tracing, zero data leaks, <5% perf overhead |
| 6 | 2+ weeks | God object refactoring | 8 new crates, 0 circular deps, coverage maintained |
| 7 | 1 week | Testing & verification | 80%+ coverage, all stress/chaos tests pass |
| 8 | 1 week | Documentation | Complete handbook, incident runbook |

**Total:** 15 weeks (ideal), 12-20 weeks (realistic)

**Success Criteria:**
- All tests pass: `cargo test --workspace`
- Coverage: 80%+ overall, 95%+ critical paths
- Load: 100 concurrent sessions, <5% latency increase
- Stress: 24h run, 0 panics, 0 memory leaks
- Security: all privilege escalation attempts blocked
- Production: can be deployed to production with confidence

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------:|
| Refactoring introduces bugs | Comprehensive test suite, code review, staged rollout |
| Circular dependency fix breaks llm/tools | Feature flags, parallel implementations, migration guide |
| Performance regression | Benchmark before/after each phase, rollback plan |
| Team coordination on 8-week effort | Weekly syncs, clear phase boundaries, parallel work |
| Incomplete documentation | Enforce doc review, runbook drills, incident simulation |

---

## Dependency & Ordering

```
Phase 1 (Audit) 
  ↓
├→ Phase 2 (Building) → Phase 7 (Testing)
├→ Phase 3 (Memory) → Phase 7 (Testing)
├→ Phase 4 (Harness) → Phase 7 (Testing)
├→ Phase 5 (Orchestration) → Phase 7 (Testing)
├→ Phase 6 (Refactoring, parallel with Phase 5)
└→ Phase 8 (Documentation, after Phase 7)
```

All phases can work in parallel after Phase 1 audit (assuming 3+ engineers).

---

## Appendix: Key Traits & Code Examples

### InputValidator Trait
```rust
pub trait InputValidator {
    type Input;
    type Error: std::error::Error;
    fn validate(&self, input: &Self::Input) -> Result<(), Self::Error>;
}
```

### TokenAccountant Trait
```rust
pub trait TokenAccountant {
    fn track_prompt(&mut self, tokens: u32, session_id: &str) -> Result<()>;
    fn track_completion(&mut self, tokens: u32, session_id: &str) -> Result<()>;
    fn remaining_budget(&self, session_id: &str) -> u32;
    fn enforce_limit(&self, session_id: &str) -> Result<()>;
}
```

### PrivilegeGate Trait
```rust
pub trait PrivilegeGate {
    fn can_read(&self, agent_id: &str, path: &Path) -> bool;
    fn can_write(&self, agent_id: &str, path: &Path) -> bool;
    fn can_execute(&self, agent_id: &str, cmd: &str) -> bool;
}
```

### Task Trait
```rust
pub trait Task: Send + Sync {
    type Input: Serialize + Deserialize;
    type Output: Serialize + Deserialize;
    
    fn validate_input(&self, input: &Self::Input) -> Result<()>;
    fn schema() -> JsonSchema;
}
```

### ExecutionContext Struct
```rust
pub struct ExecutionContext {
    max_calls: u32,
    calls_made: u32,
    max_time: Duration,
    start_time: Instant,
    max_model_calls: u32,
    model_calls_made: u32,
}

impl ExecutionContext {
    fn assert_not_exceeded(&self) -> Result<()> {
        if self.calls_made >= self.max_calls {
            return Err(anyhow!("Max tool calls exceeded"));
        }
        if self.start_time.elapsed() >= self.max_time {
            return Err(anyhow!("Execution timeout"));
        }
        if self.model_calls_made >= self.max_model_calls {
            return Err(anyhow!("Max model calls exceeded"));
        }
        Ok(())
    }
}
```

### ToolError Struct
```rust
pub struct ToolError {
    code: String,
    message: String,
    details: String,
    suggestion: String,
}
```
