# RustyCode Multi-Agent Infrastructure Mapping

**Date:** 2026-05-06
**Focus:** Evaluate existing agent/team/orchestration infrastructure against OpenCode Agent Teams and Claude Code Sub-agents

---

## Executive Summary

RustyCode already has **extensive multi-agent infrastructure** that largely predates the user's request to port OpenCode's agent teams. The codebase contains **4 distinct multi-agent paradigms** that partially overlap with the target architecture:

1. **Builder-Skeptic-Judge Team Loop** (`rustycode-team`) — Structured quality-control team
2. **Sub-agent Delegation** (`rustycode-tui/agents`) — Single-session specialist delegation
3. **Parallel Multi-Agent Analysis** (`rustycode-runtime/multi_agent`) — Parallel LLM analysis with consensus
4. **Team Registry** (`rustycode-orchestration/team_registry` + `rustycode-tools/providers/team.rs`) — Persistent team filesystem storage

**Key Finding:** RustyCode's infrastructure is closer to Claude Code's sub-agent model than OpenCode's peer-to-peer agent teams, but has team coordination primitives too. The gap is primarily in **peer-to-peer messaging**, **auto-wake event-driven architecture**, and **declarative agent definitions**.

---

## Existing Infrastructure Deep Dive

### 1. Builder-Skeptic-Judge Team Loop

**Location:** `crates/rustycode-team/`

**What it is:** A structured quality-control loop where specialized agents cross-check each other's work. This is NOT peer-to-peer — it's a hierarchical pipeline.

**Architecture:**
```
Task → TaskProfiler → PlanManager → Coordinator
                                        │
                        ┌───────────────┼───────────────┐
                        │               │               │
                     Builder        Skeptic         Judge
                     (writes)      (reviews)    (verifies)
                        │               │               │
                        └───────────────┼───────────────┘
                                        │
                                   Coordinator
                                   (trust tracking)
```

**Key Components:**

| Component | File | Purpose |
|-----------|------|---------|
| `TeamOrchestrator` | `src/orchestrator.rs` | Wires LLM to team loop, manages execution |
| `Coordinator` | `src/coordinator.rs` | Trust tracking, progress, doom loop detection |
| `TeamExecutor` | `src/executor.rs` | Role-specific prompts and tool filtering |
| `EventEngine` | `src/event_engine.rs` | Proactive event-driven agent reactions |
| `PlanManager` | `src/plan_manager.rs` | Step execution, retry, adaptation |
| `AgentRegistry` | `src/agent_registry.rs` | Specialist agent creation/reuse |

**Agent Roles (10 defined):**
- `Builder` — Writes code, has `read_file`, `write_file`, `bash`, `grep`, `glob`
- `Skeptic` — Reviews code, has `read_file`, `grep`, `glob` (NO write)
- `Judge` — Runs `cargo check`/`cargo test`, has `bash`, `read_file`
- `Architect` — Produces StructuralDeclaration, read-only
- `Scalpel` — Targeted fixes, has `read_file`, `write_file`, `bash`
- `Coordinator` — Manages team, tracks trust
- `Planner` — Breaks work into phases
- `Worker` — Straightforward implementation
- `Reviewer` — Code review
- `Researcher` — Gathers context

**Tool Filtering:**
- Builder: `read_file`, `write_file`, `bash`, `grep`, `glob`
- Skeptic: `read_file`, `grep`, `glob` (NO write tools)
- Judge: `bash`, `read_file` (verification only)
- Architect: `read_file`, `grep`, `glob`, `lsp_references`, `lsp_hover`
- Scalpel: `read_file`, `write_file`, `bash` (targeted fixes only)

**Events (22 event types):**
- `AgentActivated`, `AgentStateChanged`, `AgentDeactivated`
- `StepCompleted`, `TaskCompleted`
- `CodeChanged`, `CompilationFailed`, `TestsFailed`
- `TrustChanged`, `VerificationPassed`
- `PatternDiscovered`, `SecurityIssueDetected`
- `StructuralDeclarationSet`, `PlanAdapted`
- `SpecialistCreated`, `ParallelExecutionRequested`
- `ToolStarted`, `ToolCompleted`, `ToolLoopIteration`
- `AdvisorGuidance`, `LLMTextChunk`, `LLMThinkingChunk`

**Key Capabilities:**
- ✅ Trust scoring (0.0-1.0, starts at 0.7)
- ✅ Doom loop detection (repeated approaches)
- ✅ Progress delta tracking (test count changes)
- ✅ Plan adaptation on failure
- ✅ Escalation to user (3 levels)
- ✅ Pattern mining from execution traces
- ✅ Vector memory for task traces (optional feature)
- ✅ Event-driven reactions (Phase 4)
- ✅ Streaming LLM responses
- ✅ Tool loop with validation (max 30 iterations)
- ✅ Local capabilities (cargo check, cargo test)

**Missing vs OpenCode:**
- ❌ Peer-to-peer messaging between agents
- ❌ JSONL append-only inboxes
- ❌ autoWake mechanism
- ❌ Persistent agent processes (agents are ephemeral LLM calls)
- ❌ Multi-provider coordination (all use same provider)
- ❌ Worktree isolation per agent
- ❌ Declarative agent YAML definitions

---

### 2. Sub-agent Delegation (Claude Code-style)

**Location:** `crates/rustycode-tui/src/agents/`

**What it is:** Single-session specialist delegation with custom prompts and tool filtering. Closest to Claude Code's sub-agents.

**Key Components:**

| Component | File | Purpose |
|-----------|------|---------|
| `AgentTool` | `src/agents/agent_tool.rs` | Launches sub-agents via AgentSession |
| `AgentSession` | `crates/rustycode-agent-runtime/src/lib.rs` | Real LLM↔tool loop for sub-agents |
| `AgentDefinition` | `src/agents/definitions.rs` | Agent type definitions |
| `AgentMetrics` | `src/agents/mod.rs` | Spawn/cancel/retry with metrics |

**How it works:**
```rust
// AgentTool launches sub-agents that run real LLM↔tool loops
let mut session = AgentSession::new(config, &self.cwd);
session.run(provider, model, system_prompt, messages, tools, registry, collector).await
```

**Agent Types (from `definitions.rs`):**
- `general-purpose` — Default agent
- `explore` — Codebase exploration
- `review` — Code review
- `test` — Test writing
- `debug` — Debugging
- `docs` — Documentation

**Sub-agent Tool Registry** (excludes `AgentTool` to prevent recursion):
- `read_file`, `write_file`, `list_dir`
- `edit`, `grep`, `glob`
- `apply_patch`
- `bash`
- `git_status`, `git_diff`, `git_log`

**Key Capabilities:**
- ✅ Real LLM↔tool loop (not simulated)
- ✅ 25 turns max, 5 min timeout
- ✅ Tool tier escalation (`ToolTier::Full`)
- ✅ Event collection (`AgentEvents` trait)
- ✅ Recursive prevention (no `AgentTool` in sub-agent registry)

**Missing vs Claude Code:**
- ❌ No worktree isolation (sub-agents run in same directory)
- ❌ No persistent memory between sub-agent runs
- ❌ No declarative YAML frontmatter for agent definitions
- ❌ No `allowed_tools`/`denied_tools` dynamic filtering (static registry)

---

### 3. Parallel Multi-Agent Analysis

**Location:** `crates/rustycode-runtime/src/multi_agent.rs`

**What it is:** Spawns multiple agents in parallel with different roles, then aggregates responses. More like an "ensemble" than a team.

**Key Components:**

| Component | Purpose |
|-----------|---------|
| `MultiAgentOrchestrator` | Spawns parallel agents with semaphore |
| `AgentCommunicationHub` | Routes messages between agents (in-memory only) |
| `SharedWorkingMemory` | Cross-agent memory sharing |
| `EnsembleCoordinator` | Hierarchical ensemble management |

**AgentMessage Types:**
- `Request { from, to, query, context, message_id }`
- `Response { from, to, answer, confidence, request_id }`
- `Broadcast { from, announcement, priority }`

**Key Capabilities:**
- ✅ Parallel agent spawning (semaphore-limited)
- ✅ Cross-agent messaging (Request/Response/Broadcast)
- ✅ Shared working memory
- ✅ Consensus building
- ✅ Service discovery registration
- ✅ Negotiation (AgentNegotiator)
- ✅ Provider-specific prompts (Anthropic/OpenAI/Gemini)

**Missing vs OpenCode:**
- ❌ Messaging is in-memory only (not persistent JSONL)
- ❌ No auto-wake on message arrival
- ❌ Agents are single-shot analysis, not long-running
- ❌ No peer-to-peer discovery

---

### 4. Team Registry & Persistent Storage

**Location:**
- `crates/rustycode-orchestration/src/team_registry.rs`
- `crates/rustycode-tools/src/providers/team.rs`

**What it is:** Filesystem-based team storage and tool-based team management.

**TeamRegistry:**
- Creates teams with IDs (`team_{timestamp}_{counter}`)
- Tracks status: `Created` → `Running` → `Completed` → `Deleted`
- Global registry via `OnceLock`

**Team Tools:**
- `team_create` — Creates `~/.claude/teams/{name}/config.json` + `~/.claude/tasks/{name}/`
- `team_delete` — Removes team directories
- `team_spawn` — (not yet implemented in team.rs)
- `team_message` — (not yet implemented)
- `team_broadcast` — (not yet implemented)
- `team_status` — (not yet implemented)

**Current State:**
- `TeamCreateTool` and `TeamDeleteTool` are implemented
- Other team tools (spawn, message, broadcast, status) are NOT implemented in `team.rs`
- Storage is filesystem-based JSON, not JSONL append-only

---

### 5. Event Bus

**Location:** `crates/rustycode-bus/src/lib.rs`

**What it is:** Type-safe pub/sub event bus for decoupled communication.

**Key Capabilities:**
- ✅ Wildcard subscriptions (`session.*`, `*.error`)
- ✅ Callback, Broadcast, and Hybrid subscriber types
- ✅ Pre/Post publish hooks
- ✅ Metrics tracking
- ✅ Auto-cleanup on drop (`SubscriptionHandle`)
- ✅ Thread-safe (`Arc<RwLock<HashMap<Uuid, Subscriber>>>`)

**Potential for Agent Teams:**
- Could power the auto-wake mechanism (subscribe to `agent.message.*`)
- Could replace filesystem polling for inbox monitoring
- Event types would need to be added for agent-specific events

---

## Gap Analysis: Existing vs Target Architecture

### OpenCode Agent Teams Features

| Feature | OpenCode | RustyCode Status | Gap |
|---------|----------|------------------|-----|
| **JSONL append-only inboxes** | Core mechanism | ❌ Filesystem JSON only | High |
| **autoWake** | Auto-restart on message | ❌ No auto-wake | High |
| **Peer-to-peer messaging** | Agents message directly | ⚠️ In-memory only (multi_agent.rs) | Medium |
| **Two-level state machines** | Per-message + per-task | ⚠️ Single-level (coordinator.rs) | Medium |
| **Multi-provider support** | Different providers per agent | ❌ Single provider per orchestrator | High |
| **Persistent agent processes** | Long-running agents | ❌ Ephemeral LLM calls | High |
| **Declarative agent definitions** | YAML frontmatter | ⚠️ AgentDefinition in code | Medium |
| **Tool allow/deny lists** | Per-agent tool filtering | ✅ Implemented (executor.rs) | None |
| **Worktree isolation** | Per-agent git worktrees | ❌ No isolation | High |
| **Event-driven architecture** | React to code changes | ✅ EventEngine (orchestrator.rs) | None |
| **Trust tracking** | Dynamic trust scores | ✅ TrustScore (team.rs) | None |
| **Streaming responses** | Real-time output | ✅ TeamEvent streaming | None |

### Claude Code Sub-agent Features

| Feature | Claude Code | RustyCode Status | Gap |
|---------|-------------|------------------|-----|
| **Single-session delegation** | Core mechanism | ✅ AgentTool | None |
| **Custom prompts** | Per-sub-agent | ✅ AgentDefinition | None |
| **Tool filtering** | allowed_tools/denied_tools | ⚠️ Static registry only | Low |
| **Worktree isolation** | Per-sub-agent worktrees | ❌ No isolation | High |
| **Skills preloading** | Load skills before spawn | ❌ Not implemented | Medium |
| **Persistent memory** | Memory between sub-agents | ❌ No persistence | Medium |
| **Declarative YAML** | YAML frontmatter | ❌ Not implemented | Medium |
| **Cancellation** | Cancel sub-agents | ✅ AgentMetrics cancellation | None |

---

## What Already Exists (Ready to Reuse)

### 1. Role-Based Agent System
The `AgentRole` enum and role-specific prompts are fully implemented. Adding new roles is straightforward.

### 2. Tool Filtering
`tools_for_role()` in `executor.rs` already implements per-role tool allowlists.

### 3. Event Broadcasting
`TeamOrchestrator::emit_and_dispatch()` already broadcasts events and returns agent actions.

### 4. Trust Tracking
`TrustScore` with `TrustEvent` history is fully implemented.

### 5. Local Verification
`local_capabilities::check_compilation()` and `run_tests()` provide Judge functionality.

### 6. Execution Traces
`ExecutionTrace` + `PatternMiner` capture and learn from task outcomes.

### 7. Event Bus
`rustycode_bus::EventBus` can power inter-agent communication.

### 8. Agent Registry
`AgentRegistry` creates/reuses specialist agents dynamically.

---

## What's Missing (Implementation Required)

### High Priority

1. **JSONL Append-Only Inboxes**
   - Replace filesystem JSON with append-only JSONL
   - Implement per-agent inbox directories
   - Add message delivery guarantees

2. **autoWake Mechanism**
   - Use `rustycode_bus` to subscribe to inbox events
   - Spawn agent tasks when messages arrive
   - Implement backoff for failed agents

3. **Multi-Provider Coordination**
   - Allow different LLM providers per agent role
   - Provider selection based on task type
   - Cost optimization across providers

4. **Persistent Agent Processes**
   - Long-running agent tasks (not single-shot LLM calls)
   - Agent lifecycle management (start/pause/resume/stop)
   - Resource limits per agent

5. **Worktree Isolation**
   - Git worktree per agent for parallel development
   - Merge strategies for agent work
   - Conflict detection and resolution

### Medium Priority

6. **Declarative Agent Definitions**
   - YAML frontmatter for agent types
   - Hot-reload of agent definitions
   - Agent composition (inheritance)

7. **Skills Preloading**
   - Load skills into agent context before spawn
   - Skill discovery and matching
   - Skill cache management

8. **Peer-to-Peer Messaging**
   - Direct agent-to-agent messages (not through coordinator)
   - Message routing and delivery
   - Dead letter queue for failed deliveries

### Low Priority

9. **TUI Integration**
   - Team panel visualization (`TeamPanel` already exists)
   - Agent status dashboard
   - Real-time team event display

10. **Metrics and Observability**
    - Team performance metrics
    - Agent utilization tracking
    - Cost attribution per agent

---

## Recommended Architecture

### Option A: Extend Existing Team Loop (Recommended)

Build on the existing `rustycode-team` infrastructure:

```
┌─────────────────────────────────────────┐
│         TeamOrchestrator                │
│  (extend with P2P messaging + autoWake) │
└─────────────────────────────────────────┘
                    │
        ┌───────────┼───────────┐
        │           │           │
    ┌───▼───┐   ┌───▼───┐   ┌───▼───┐
    │Agent A│   │Agent B│   │Agent C│
    │(JSONL │   │(JSONL │   │(JSONL │
    │ inbox)│   │ inbox)│   │ inbox)│
    └───┬───┘   └───┬───┘   └───┬───┘
        │           │           │
        └───────────┼───────────┘
                    │
            ┌───────▼───────┐
            │  EventBus     │
            │ (auto-wake)   │
            └───────────────┘
```

**Pros:**
- Leverages existing trust tracking, event engine, tool filtering
- Minimal disruption to existing codebase
- Can reuse `TeamEvent` types

**Cons:**
- Team loop is hierarchical, not P2P
- May need significant refactoring for true P2P

### Option B: New Agent Teams Crate

Create `rustycode-agent-teams` as a peer-to-peer layer:

```
┌─────────────────────────────────────────┐
│      rustycode-agent-teams              │
│  (P2P messaging, autoWake, inboxes)     │
└─────────────────────────────────────────┘
                    │
    ┌───────────────┼───────────────┐
    │               │               │
┌───▼───┐      ┌───▼───┐      ┌───▼───┐
│rusty- │      │rusty- │      │rusty- │
│code-  │      │code-  │      │code-  │
│team   │      │runtime│      │tui    │
│(BSJ)  │      │(multi)│      │(sub)  │
└───────┘      └───────┘      └───────┘
```

**Pros:**
- Clean separation of concerns
- Can implement true P2P without legacy constraints
- Easier to map to OpenCode's architecture

**Cons:**
- Duplicates some existing functionality
- More integration work
- Larger codebase

---

## Implementation Phases

### Phase 1: Foundation (2-3 weeks)
1. Implement JSONL append-only inboxes in `rustycode-team`
2. Add `autoWake` using `rustycode_bus`
3. Create persistent agent task runner
4. Add worktree isolation for agents

### Phase 2: P2P Messaging (2 weeks)
1. Implement peer-to-peer message routing
2. Add message delivery guarantees
3. Create agent discovery mechanism
4. Implement dead letter queue

### Phase 3: Multi-Provider (1-2 weeks)
1. Allow per-agent LLM provider selection
2. Implement provider-specific prompts
3. Add cost tracking per provider
4. Create provider fallback logic

### Phase 4: Declarative Agents (1 week)
1. YAML frontmatter parser for agent definitions
2. Hot-reload of agent definitions
3. Agent composition/inheritance
4. Skill preloading

### Phase 5: Integration (1 week)
1. TUI team panel enhancements
2. Metrics dashboard
3. Documentation and examples
4. End-to-end testing

---

## Key Files Reference

| Purpose | File Path |
|---------|-----------|
| Team orchestration | `crates/rustycode-team/src/orchestrator.rs` |
| Coordinator/Trust | `crates/rustycode-team/src/coordinator.rs` |
| Role prompts/tools | `crates/rustycode-team/src/executor.rs` |
| Team protocol types | `crates/rustycode-protocol/src/team.rs` |
| Multi-agent analysis | `crates/rustycode-runtime/src/multi_agent.rs` |
| Sub-agent tool | `crates/rustycode-tui/src/agents/agent_tool.rs` |
| Agent definitions | `crates/rustycode-tui/src/agents/definitions.rs` |
| Team registry | `crates/rustycode-orchestration/src/team_registry.rs` |
| Team tools | `crates/rustycode-tools/src/providers/team.rs` |
| Event bus | `crates/rustycode-bus/src/lib.rs` |
| Agent registry | `crates/rustycode-orchestration/src/agent_registry.rs` |
| Agent runtime | `crates/rustycode-agent-runtime/src/lib.rs` |

---

## Conclusion

RustyCode is **not starting from scratch**. The existing infrastructure provides:

- ✅ 70% of Claude Code's sub-agent features
- ✅ 60% of OpenCode's agent team features
- ✅ Strong foundation in role-based agents, tool filtering, trust tracking
- ✅ Event-driven architecture ready for auto-wake

The main gaps are:
1. **Persistent inboxes + autoWake** (highest impact)
2. **Worktree isolation** (critical for parallel agents)
3. **Multi-provider coordination** (enables cost optimization)
4. **Declarative agent definitions** (improves UX)

**Recommendation:** Extend the existing `rustycode-team` crate with P2P messaging and autoWake, rather than creating a new crate. This leverages the existing trust tracking, event engine, and tool filtering while adding the missing OpenCode-style features.
