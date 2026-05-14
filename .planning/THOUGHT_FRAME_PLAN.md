# Revised ThoughtFrame Implementation Plan

## 1. Architectural Shift: Reflection-Driven Orchestration
Shift from auto-heuristic "stuck counters" to explicit model reflection. The model is now responsible for declaring its `ThoughtFrame` via a mandatory (or highly-recommended) `reflect` tool.

### Core Components
- **`ThoughtFrame` (Protocol Layer):** Structured schema for Hypothesis, Findings, Confidence, and Strategy.
- **`ReflectionOrchestrator` (Orchestration Layer):** A new trait/service that manages `ThoughtFrame` lifecycle (persistence, nudge injection into system prompts).
- **Tool Integration:** Add a `reflect(frame: ThoughtFrame)` tool that acts as the heartbeat of the agent's reasoning process.

---

## 2. Updated Data Model (`rustycode-protocol`)
Move the schema to the protocol layer for consistency.

```rust
pub struct ThoughtFrame {
    pub hypothesis: String,
    pub findings: Vec<String>,
    pub confidence: f32, // 0.0 - 1.0
    pub pending_questions: Vec<String>,
    pub next_steps: Vec<String>,
    pub strategy: ReflectionStrategy, // enum: Researching, Coding, Verifying, Stuck
}
```

---

## 3. Implementation Tasks

### Wave 1: Core Protocol & Storage
- [ ] Define `ThoughtFrame` in `rustycode-protocol`.
- [ ] Implement `ReflectionStore` in `rustycode-storage` (keyed by `task_id`, session-scoped).

### Wave 2: Orchestration Layer Integration
- [ ] Implement `ReflectionManager` in `rustycode-orchestration`.
- [ ] Expose `reflect` tool to all agents (managed via the `ReflectionManager`).
- [ ] Wire `ReflectionManager` into `AgentSession::run` as an `AgentPlugin`.

### Wave 3: Nudge & Prompt Engineering
- [ ] Build the `ThoughtFramePromptInjector`: automatically injects current `ThoughtFrame` into the system prompt.
- [ ] Implement the `NudgeEngine`: uses the current frame to detect low confidence or strategy mismatches and injects an intervention nudge.

---

## 4. Key Improvements
- **Explicit Ownership:** The model owns its state; the system facilitates persistence and prompts.
- **Session Scoping:** Eliminates global `cwd` conflicts by keying stores to the specific `task_id`.
- **Modularity:** `rustycode-orchestration` drives the logic, `rustycode-protocol` holds the schema, `rustycode-agent-runtime` consumes it.
- **Memory Future:** The store can easily be upgraded to persist across sessions (cross-session memory).
