# Revised Unified Agent Architecture for RustyCode

**Date:** 2026-05-06
**Status:** ✅ IMPLEMENTED — supersedes the greenfield parts of `UNIFIED_AGENT_ARCHITECTURE.md`
**Decision:** **Yes — the agent design should change.**

---

## Executive Summary

Our first unified-agent draft was too greenfield and too swarm-heavy.

Fresh evidence from:
- the RustyCode codebase,
- NotebookLM research,
- and an Oracle pressure test,

all point to the same conclusion:

> RustyCode already has most of the right primitives. We should **wire and narrow** them, not build a second overlapping multi-agent platform.

The right architecture is a **harness-first, bus-native, capability-scoped system** with a **thin directed-message routing layer** on top of existing primitives.

This reduces the plan from roughly **2,150 lines / ~10 weeks** of new infrastructure to roughly **400–500 lines / 2–3 days** of focused integration work.

---

## What Changed

### Original assumption
The main missing piece was a brand-new stack:
- `AgentMailbox`
- `AgentDirectory`
- `TeamFormation`
- `CrossAgentPlanner`
- JSONL mailbox persistence
- new topology system

### Revised conclusion
Those are mostly duplicates of systems RustyCode already has in partial or complete form.

What is actually missing is much smaller:

1. **Directed delivery** on top of existing messaging and bus types
2. **Capability advertisement** added to the existing agent registry
3. **Real routing** for the existing `send_message` tool
4. **TaskDispatcher V2 wiring** to real `AgentSession` execution
5. **A stricter harness boundary** around delegation, memory writes, and tool access

---

## Evidence That Forced the Change

### External research (NotebookLM)

The notebook strongly argues against the most speculative parts of the first draft:

- **Naive shared-state coordination is an anti-pattern**
- **Append-only JSONL persistence is risky** without strong validation and compaction
- **Dynamic swarms are often overbuilt**; multi-agent overhead is only justified for clearly heterogeneous roles
- **Harness-enforced controls** matter more than LLM-decided controls
- **Local event bus / SQLite** should be preferred for intra-system communication
- **A2A / HTTP boundaries** should be reserved for cross-trust or cross-system delegation
- **Constraint collapse** is real: agents should receive only the tools and context they need

### Internal codebase evidence

RustyCode already contains the following reusable primitives:

- **Typed agent envelopes**: `crates/rustycode-protocol/src/agent_protocol.rs`
  - `AgentMessage<T>`
  - `AgentRole`
  - request/reply shape already exists

- **Runtime communication model**: `crates/rustycode-runtime/src/multi_agent.rs`
  - `AgentCommunicationHub`
  - `AgentMessage::{Request, Response, Broadcast}`

- **Delegation lifecycle**: `crates/rustycode-orchestration/src/task_dispatcher.rs`, `bus.rs`, `delegation.rs`
  - `TaskSpec`
  - `TaskRole`
  - `SpawnDecision`
  - `TaskSpawned`, `TaskDelegationCompleted`, `TaskDelegationFailed`

- **Capability/scoping primitives**:
  - `AgentRegistry` in `agent_registry.rs`
  - `ToolCapability` in `isolation/tier.rs`
  - role-based tool shaping across the orchestration stack

- **Team and role systems** already exist, but overlap:
  - `AgentRole`
  - `TeamRole`
  - `TaskRole`
  - `CrewRole`

- **Trust and governance** already exist:
  - `TrustScore`
  - `Coordinator`
  - `Conductor`
  - `FailurePatternStore`
  - hallucination detection

- **Shared artifact storage** already exists:
  - `SharedWorkspace`
  - `SharedWorkingMemory`

- **Consensus / ensemble concepts** already exist:
  - `EnsembleStrategy`
  - `DecisionStrategy`
  - `Vote` / `VotingResult`

This means we do **not** need a second standalone system for mailboxes, registries, team formation, and coordination.

---

## Revised Architecture Direction

### Design principle

Build a **small integration layer** over the existing orchestration substrate.

### Core shape

```text
AgentSession / TeamOrchestrator / TaskDispatcher
                │
                ▼
         existing BusHandle + AgentMessage<T>
                │
                ▼
        MailboxRouter (NEW, thin directed router)
                │
      ┌─────────┴─────────┐
      ▼                   ▼
 existing AgentRegistry   existing SharedWorkspace
 (+ capability ads)       (+ artifact handoff only)
```

### What the revised design optimizes for

- **Directed intra-system messaging** without a new heavy mailbox subsystem
- **Capability-scoped delegation** without dynamic-swarm overreach
- **Harness-first enforcement** for permissions, tool scope, persistence, and retries
- **Minimal new state** and minimal new persistence
- **Use the current event/bus/delegation model as the backbone**

---

## The Minimum Viable Architecture

### 1. `MailboxRouter` (new, small)

Add a **thin directed routing layer** in orchestration.

**Purpose:** deliver `AgentMessage<AgentPayload>` to a specific local agent identity.

**Not a full mailbox system.**

It should:
- keep a map of `agent_id -> sender/receiver`
- support `send(to, msg)`
- support `broadcast(msg)`
- expose `recv(agent_id)`
- integrate with existing `BusHandle` for observability

This is enough to turn the existing message model into a real delivery path.

### 2. Extend `AgentMessage<T>`, don’t replace it

The protocol crate already has the correct envelope direction.

Add an `AgentPayload` enum for the actual routed messages:
- `TaskDelegation`
- `TaskResult`
- `CapabilityAdvertise`
- `CapabilityQuery`
- `ConsensusVote`
- `Objection`

Do **not** introduce a second envelope type.

### 3. Extend `AgentRegistry`, don’t create `AgentDirectory`

`AgentRegistry` already handles:
- built-in agents
- generated specialists
- task history
- specialist reuse

Add:
- `AgentRecord`
- structured capability descriptors
- liveness / availability
- optional tool-scope metadata

This gives us the useful part of a directory without duplicating the registry layer.

### 4. Wire `send_message` for real delivery

`crates/rustycode-tools/src/providers/send_message.rs` has the right interface but is currently only a stub.

It should route through `MailboxRouter` instead of just returning success text.

This turns an existing operator-facing abstraction into the real intra-agent communication mechanism.

### 5. Upgrade `TaskDispatcher` to V2

`TaskDispatcher` already exists and already converts spawn decisions into execution.

The real work is to replace the placeholder path with:
- real `AgentSession` execution
- capability-aware target selection from the extended registry
- optional directed handoff via routed messages

This is much smaller than introducing a separate `CrossAgentPlanner` subsystem.

### 6. Keep team topology constrained

Do **not** add a new 5-topology framework.

Use existing shapes only:
- **Hierarchy**: parent/sub-agent, already present
- **Coordinator-led star**: quality loop / specialist pattern, already present
- **Ensemble**: existing `EnsembleStrategy`, already present

If adaptive routing is added, it should choose among these **human-auditable existing modes**, not invent or learn new topologies dynamically.

---

## What to Cut From the Original Draft

### Cut entirely

- **`TeamFormation`**
  - redundant with `EnsembleStrategy`, `DelegationPlanner`, `Coordinator`

- **`CrossAgentPlanner`**
  - too much new orchestration logic when existing delegation primitives already exist

- **new topology strategy framework**
  - overbuilt relative to current needs and contrary to the research warning on swarm over-complexity

- **new mailbox persistence layer**
  - premature and risky

### Replace with smaller versions

- **`AgentMailbox` + `AgentMailboxSystem`**
  - replace with one thin `MailboxRouter`

- **`AgentDirectory`**
  - replace with a structured extension to `AgentRegistry`

- **new protocol envelope**
  - replace with extension of existing `AgentMessage<T>`

### Defer explicitly

These items are explicitly deferred — they are not wrong forever, but they are wrong **now**:

| Deferred Item | Reason | Re-evaluate When |
|---|---|---|
| A2A / HTTP agent boundaries | Local bus + router sufficient for intra-process; distributed adds complexity | Cross-service delegation is needed |
| JSONL inbox persistence | Premature without strong validation/compaction; mpsc channels sufficient | Durability guarantees are required across crashes |
| Generalized dynamic swarms | Research warns against swarm over-complexity; fixed topologies sufficient | Adaptive team sizing shows measurable benefit |
| Agent heartbeat / distributed state sync | Single-process model doesn't need it | Multi-process or distributed deployment is required |
| Broad cross-agent consensus protocol | Ensembles already handle voting; broader consensus is speculative | Heterogeneous consensus across agent types is needed |

---

## Revised Reuse-vs-Build Matrix

### Reuse as-is or with small extension

| Existing Component | File | Revised Role |
|---|---|---|
| `AgentMessage<T>` | `crates/rustycode-protocol/src/agent_protocol.rs` | canonical local envelope |
| `AgentRole` | same | canonical agent role base |
| `AgentRegistry` | `crates/rustycode-orchestration/src/agent_registry.rs` | base registry + capability extension |
| `BusHandle` / `OrchestrationEvent` | `crates/rustycode-orchestration/src/bus.rs` | observability + orchestration event surface |
| `TaskDispatcher` | `crates/rustycode-orchestration/src/task_dispatcher.rs` | dispatch bridge, upgraded to real execution |
| `DelegationPlanner` | `crates/rustycode-orchestration/src/delegation.rs` and TUI delegation logic | keep as the spawn decision engine |
| `ForkJoinExecutor` | `crates/rustycode-orchestration/src/fork_join.rs` | bounded parallel execution |
| `EnsembleStrategy` | `crates/rustycode-orchestration/src/ensemble_strategy.rs` | keep existing strategy family |
| `SharedWorkspace` | `crates/rustycode-orchestration/src/shared_workspace.rs` | artifact handoff, not coordination substrate |
| `SendMessageTool` | `crates/rustycode-tools/src/providers/send_message.rs` | real message delivery front-end |
| `AgentTimeline` | `crates/rustycode-team/src/agent_timeline.rs` | lifecycle + message-state extension |

### Build new (small)

| New Component | Purpose | Estimated Size |
|---|---|---|
| `MailboxRouter` | directed local message delivery | ~150 LOC |
| `AgentPayload` additions | actual routed message payloads | ~100 LOC |
| `AgentRegistry` extension | capability ads, availability, lookup refinement | ~80–120 LOC |
| `TaskDispatcher` V2 wiring | replace placeholder execution path | ~120 LOC |
| `DelegationToken` or equivalent ancestry tracking | safe agent-initiated delegation chain | ~50 LOC |

**Total new code:** roughly **400–500 LOC**

---

## Harness-Enforced Rules (Non-Negotiable)

Research and codebase reality both support stronger harness enforcement.

These should remain **harness-owned**, not LLM-owned:

1. **Tool scope**
   - grant only the tool subset needed for the immediate role/task

2. **Delegation boundaries**
   - who may delegate to whom
   - max delegation depth
   - allowed child roles

3. **Persistence writes**
   - validate and authorize writes to long-lived memory/state

4. **Budget / timeout / termination**
   - hard caps only, not advisory

5. **Error classification and retry policy**
   - the harness should classify transient vs structural failures before involving the LLM

6. **Context compaction**
   - the harness should own compaction; agents should not self-manage long-context persistence policies

---

## Shared State: Revised Rule

The first draft treated shared workspace and shared memory too generously.

### Revised rule

Use shared stores only for:
- artifacts
- evidence
- summaries
- immutable or validated coordination outputs

Do **not** use them as the primary live coordination mechanism.

Live coordination should happen through:
- routed messages
- bounded delegation
- explicit orchestration events

This keeps RustyCode out of the “naive shared-state swarm” failure mode.

---

## External Boundary Rule

For now:

- **Intra-process / intra-system agent communication:** local bus + router
- **Cross-trust / cross-service delegation:** future A2A / HTTP only if needed

That matches the research and avoids pulling distributed-systems complexity into local orchestration.

---

## Updated Recommendation

### Should we change the design?
**Yes.**

### How?
Move from:
- a new mailbox subsystem,
- a new directory subsystem,
- a new topology subsystem,
- and a new planning subsystem,

to:

- a **thin bus-native routing layer**,
- an **extended existing registry**,
- a **wired existing tool surface**,
- and **harness-enforced delegation over existing execution primitives**.

### Strategic posture

RustyCode should not try to “become OpenCode plus Claude Code plus A2A plus a dynamic swarm framework.”

It should become:

> **a disciplined local multi-agent harness** with strong internal governance, selective delegation, and minimal protocol additions.

That is both more defensible architecturally and much closer to the system RustyCode already is.

---

## Immediate Next Steps

1. ~~Add `MailboxRouter` in orchestration~~ ✅ `crates/rustycode-orchestration/src/mailbox_router.rs`
2. ~~Extend `AgentMessage<T>` with `AgentPayload`~~ ✅ `crates/rustycode-protocol/src/agent_protocol.rs` (fixed serde `rename_all = "snake_case"`)
3. ~~Extend `AgentRegistry` with structured capability advertisement~~ ✅ Already existed: `CapabilityDescriptor`, `find_by_capability`, `rank_by_success`, `mark_busy/mark_available`
4. ~~Wire `send_message` to real delivery~~ ✅ `MessageSender` trait in `rustycode-tools-api`, injected via `ToolContext::message_sender`
5. ~~Upgrade `TaskDispatcher` V2 to real `AgentSession` execution~~ ✅ `SessionExecutor` trait + V2 routing in `execute_single()`
6. ~~Add delegation ancestry / depth control~~ ✅ `DelegationToken` in `delegation.rs` with depth enforcement in `dispatch()`
7. ~~Role reconciliation~~ ✅ `From<TeamRole> for AgentRole` and `TryFrom<TaskRole> for AgentRole` already existed
8. Only after that, evaluate whether richer distributed features are still justified

### Files changed (summary)

| File | Change |
|------|--------|
| `crates/rustycode-protocol/src/agent_protocol.rs` | Fixed `AgentPayload` serde `rename_all` |
| `crates/rustycode-orchestration/src/mailbox_router.rs` | NEW — `MailboxRouter` with mpsc directed delivery (6 tests) |
| `crates/rustycode-orchestration/src/delegation.rs` | NEW — `DelegationToken` with depth enforcement (7 tests) |
| `crates/rustycode-orchestration/src/task_dispatcher.rs` | `SessionExecutor` trait, V2 routing, depth checks (4 new tests) |
| `crates/rustycode-tools-api/src/lib.rs` | `MessageSender` trait, `ToolContext::message_sender` field |
| `crates/rustycode-tools/src/providers/send_message.rs` | Wired to `MessageSender` with stub fallback (4 new tests) |

### Test results

2240 tests pass across affected crates. Clippy clean. Pre-existing integration test (`evaluate_orchestration_harness_on_terminal_bench`) fails due to missing CSV fixture — unrelated.

---

## Final Decision

The original draft was useful as an exploration artifact.

It is **not** the implementation plan we should follow.

The revised plan is smaller, safer, more aligned with the research, and much more faithful to the RustyCode codebase we actually have.
