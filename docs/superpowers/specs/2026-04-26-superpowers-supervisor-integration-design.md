# Superpowers Supervisor Integration Design

**Date**: 2026-04-26
**Status**: Draft
**Owner**: RustyCode Orchestration

---

## Executive Summary

This document proposes a lightweight **supervisor layer** that sits above the normal implementation loop and consumes a local `superpowers` pack. The goal is to make complex tasks safer and more adaptive by letting orchestration:

- expand or narrow scope during implementation
- explore alternative approaches when the current path looks brittle
- re-plan when execution proves the initial shape is wrong
- pause when the task is drifting, thrashing, or burning budget

The supervisor should not write code itself. Its job is to observe progress and emit strategy directives that the existing orchestration stack can apply.

---

## 1. Why This Belongs In Orchestration

Complex work usually fails for one of four reasons:

1. The task was under-scoped and hidden dependencies appeared late.
2. The task was over-scoped and implementation wandered.
3. The chosen approach was viable, but not the best one.
4. The agent kept pushing on a bad plan instead of changing course.

The current orchestration stack already has most of the ingredients needed to respond:

- lifecycle hooks for tool and phase events
- tier promotion and scope control
- task context with phase and tier state
- quality heuristics
- escalation logic and deep-thinking fallback

What is missing is a dedicated supervisor that watches implementation in real time and decides when the task should be re-shaped.

---

## 2. Role Definition

The supervisor is a **strategy controller**, not an executor.

It may:

- request more context
- widen tool access
- narrow the task
- ask for alternative approaches
- trigger re-planning
- recommend a pause or handoff

It may not:

- edit files directly
- run tools on its own
- silently mutate the task goal
- override user intent without surfacing the tradeoff

This keeps the system honest. The supervisor changes the shape of the work, while the executor performs the work.

---

## 3. Supervisor Model

The supervisor watches a stream of events and converts them into directives.

### Inputs

- tool lifecycle events
- task phase changes
- tier promotions
- budget warnings
- repeated failures
- quality scores on recent outputs
- plan mismatch signals

### Outputs

- continue
- expand scope
- revise scope
- explore alternatives
- re-plan
- escalate tier
- pause for confirmation

The important distinction is that these outputs are **recommendations with authority**, not free-form advice. The orchestration engine should be able to apply them deterministically.

---

## 4. Directive Interface

The simplest useful interface is a small enum of strategy directives.

```rust
pub enum SupervisionDirective {
    Continue,
    ExpandScope {
        allowed_tools: Vec<String>,
        reason: String,
    },
    ReviseScope {
        reduced_goal: String,
        reason: String,
    },
    ExploreAlternatives {
        branches: u8,
        reason: String,
    },
    Replan {
        reason: String,
    },
    EscalateTier {
        to_tier: u8,
        reason: String,
    },
    PauseForReview {
        reason: String,
    },
}
```

The supervisor itself can be modeled as:

```rust
pub trait Supervisor {
    fn observe(&mut self, event: &SupervisionEvent) -> Option<SupervisionDirective>;
    fn reconcile(&mut self, ctx: &TaskSnapshot) -> SupervisionDirective;
}
```

This keeps the design flexible:

- `observe()` handles immediate reactions to events
- `reconcile()` handles slower periodic review of task state

That split is useful because some decisions should happen after a single failure, while others should wait until a pattern emerges.

---

## 5. Event Model

The supervisor should consume a small number of normalized event types.

```rust
pub enum SupervisionEvent {
    ToolStarted { tool: String },
    ToolFinished { tool: String, success: bool },
    ToolFailed { tool: String, error: String },
    PhaseChanged { from: String, to: String },
    TierChanged { from: u8, to: u8 },
    ScopeChanged { active_tools: Vec<String> },
    BudgetWarning { remaining_usd: f64 },
    QualitySignal { score: f64, details: String },
}
```

The event model should stay intentionally small. It is better to normalize a few strong signals than to expose the entire runtime state to the policy layer.

---

## 6. Decision Rules

The first version of the supervisor should be rule-based, not fully learned.

### Expand scope when:

- the task requires a tool that is currently unavailable
- the agent is repeatedly asking for missing context
- a recent step exposed a dependency that was not visible in the original plan

### Revise scope when:

- the implementation is growing beyond the user goal
- the current branch is adding optional work that is not necessary for completion
- the agent is solving a harder version of the problem than the user asked for

### Explore alternatives when:

- the current approach has failed more than once
- the failure looks architectural, not incidental
- there are multiple plausible implementation paths and the cost of choosing wrong is high

### Re-plan when:

- the plan and implementation have diverged
- the task has learned something that invalidates the original decomposition
- the system has enough evidence that continuing is wasteful

### Pause when:

- the task is thrashing
- the budget is close to exhausted
- the supervisor cannot explain why it wants to keep going

These rules are deliberately conservative. The supervisor should intervene only when it has a clear reason to change the shape of the work.

---

## 7. Integration Points

The supervisor should hook into existing orchestration seams rather than inventing new ones.

### Lifecycle hooks

Use tool and phase hooks as the main signal source. They are the natural place to observe progress without coupling the supervisor to every executor detail.

### Task context

Task context already carries phase, tier, cost, and trace data. That is enough to support periodic reconciliation.

### Tool scope

Scope changes should flow through the existing tool activation manager, not through ad hoc allowlists.

### Escalation

If the supervisor decides the current tier is insufficient, it should trigger the normal escalation path rather than bypassing it.

### Quality and failure memory

Quality detection and failure pattern storage should feed the supervisor’s decisions so it can recognize drift, repetition, and brittle plans.

---

## 8. How Babysitting Works In Practice

This is the main user-visible behavior.

### Example: hidden dependency appears

1. The implementation starts in a narrow scope.
2. A tool failure reveals an unanticipated dependency.
3. The supervisor emits `ExpandScope`.
4. Orchestration widens available tools or promotes the tier.
5. The executor continues with the updated scope.

### Example: current approach is wrong

1. The agent makes progress but keeps failing on the same boundary.
2. Quality scores stay low and retries keep repeating.
3. The supervisor emits `Replan` or `ExploreAlternatives`.
4. The executor pauses the branch and re-enters planning.

### Example: work is drifting

1. The output is technically working, but scope creep appears.
2. The supervisor emits `ReviseScope`.
3. The implementation is narrowed back to the user’s actual goal.

This is the babysitting effect you want: not a second author, but a steady hand that keeps the implementation on the rails.

---

## 9. Pack Layout

The local `superpowers` pack should be treated like a policy bundle.

Suggested structure:

```text
superpowers/
  README.md
  strategies/
  thresholds/
  prompts/
  rules/
  traces/
```

### What each part does

- `README.md`
  - describes the philosophy of the pack and default supervisor behavior
- `strategies/`
  - task-type playbooks such as debugging, refactoring, migration, and large feature work
- `thresholds/`
  - the signals that trigger scope expansion, re-planning, or pause
- `prompts/`
  - supervisor prompt fragments for explore, plan, and act phases
- `rules/`
  - hard constraints and safety rules
- `traces/`
  - optional examples of successful supervision decisions

The pack should be loadable by scope:

- user-global defaults
- repository-level policy
- task-type overlays
- branch or worktree-specific overrides

That gives the supervisor a way to behave like contextual instruction stacking instead of a single monolithic prompt.

---

## 10. Recommended Runtime Flow

```text
Task starts
  -> load superpowers pack
  -> classify task complexity
  -> select implementation scope
  -> enter implementation loop
       -> observe events
       -> score progress
       -> emit directive if needed
       -> apply scope/tier/phase change
       -> continue
```

The supervisor should run continuously, but cheaply. It does not need to inspect every token. It only needs enough signal to know when the task should change shape.

---

## 11. Rollout Plan

### Phase 1

- add the directive and event types
- wire a supervisor into the orchestration bus
- make it advisory only

### Phase 2

- allow scope expansion and revision
- persist supervisor decisions in trace data
- add simple thresholds for repeated failure and budget pressure

### Phase 3

- let the supervisor recommend alternative branches
- connect it to existing failure memory
- refine task-specific playbooks in the pack

The first release should be intentionally boring. A supervisor is only useful if it is predictable.

---

## 12. Open Questions

- Should the supervisor be one per task, one per session, or one per worktree?
- Should alternative branches run sequentially or in parallel?
- Which directives are automatic and which require user confirmation?
- Should scope expansion be allowed to widen tool access only, or also widen the problem definition?
- Should the pack be read from the filesystem only, or also from repository metadata?

The current recommendation is:

- one supervisor per task
- automatic scope changes only when confidence is high
- user confirmation required for true goal changes
- filesystem-based pack loading first

