# TUI Orchestration Integration Implementation Plan

> **Status:** Updated 2026-04-25 — Phase 0 partially done, new ensemble infrastructure added.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the canonical orchestration thinking engine work cleanly at runtime by wiring it into the TUI with a Ralph Loop (iterate-until-done) pattern, behavioral complexity detection, and two-stage post-completion verification.

**Architecture:** Use the canonical orchestration thinking module, then implement a 2-tier routing system (simple tasks → direct stream, complex tasks → Ralph Loop with continuation prompts), followed by two-stage verification (spec compliance → quality review) borrowed from the superpowers subagent-driven-development pattern. The Ralph Loop from oh-my-openagent is the core: keep injecting continuation prompts until the LLM signals completion or behavioral evidence shows the task is done.

**Tech Stack:** Rust, tokio async runtime, ratatui TUI framework, serde, rustycode-orchestration (canonical thinking engine), rustycode-bus for EventBus.

**Inspiration:** `~/dev/superpowers` — two-stage review gate, status protocol, context isolation, 3-strike escalation, evidence-before-claims verification. `~/dev/oh-my-openagent` — Ralph Loop (iterate until `<promise>DONE</promise>`), adversarial verification.

---

## Architecture Notes

### What Changed From the Previous Plan

The previous plan had 4 phases, 13 tasks, 4-5 weeks. It was over-engineered:
- **Self-eval quality detection** — replaced with behavioral evidence (did LLM produce tool calls? did it stall?)
- **6 strategies** — replaced with 2 (direct stream vs. Ralph Loop), expand later
- **Adaptive learning DB** — cut entirely, premature without volume
- **Modal graph visualization** — cut, polish for later
- **Structured thinking JSON schema** — replaced with organic continuation prompts

### What Changed Since This Plan Was Written (2026-04-25)

Since this plan was authored, the following infrastructure was added that affects the plan:

| New Component | Commit | Impact on Plan |
|---------------|--------|----------------|
| `EnsembleStrategy` (4 strategies) | `242016ed` | Replaces orchestra stubs. Provides `DecomposeAndDelegate`, `ParallelVote`, `SequentialReview`, `Adversarial` — the Ralph Loop can use these as its execution strategies |
| `BusHandle` / `MessageBus` | `242016ed` | Agents now communicate via pub/sub. Ralph Loop should publish `PartialResult` events per iteration |
| `SharedWorkspace` | `242016ed` | Active memory for agent intermediate results. Ralph Loop can store accumulated responses here |
| `Composer` upgrade (strategy-aware) | `242016ed` | Tier 4 Composer now activates `EnsembleStrategy` instead of being a stub |
| `Editor` upgrade (bus-aware) | `242016ed` | Tier 3 Editor now publishes events on patch |
| `TaskContext.workspace` | `242016ed` | TaskContext holds `Option<Arc<SharedWorkspace>>` for per-task memory |
| Conductor thinking integration | `9057a385` | `Conductor::try_thinking()` with strategy preemption, `escalate_with_context()` |
| Module re-exports fixed | `d7f9af76` | `SignalCategory`, `shared_workspace`, `session`, `plan_refiner` now properly exported |

### Current State (What Exists) — Updated 2026-04-25

| Component | Status | Location | Notes |
|-----------|--------|----------|-------|
| Deep-thinker executor | ✅ Exists, not wired | `crates/rustycode-deep-thinker/src/executor.rs` (567 lines) | `RealExecutor` with convergence detection, strategy preemption |
| Deep-thinker strategies | ✅ 5 strategies | `crates/rustycode-deep-thinker/src/strategies/mod.rs` | Sequential, Parallel, Analogical, Dialectic, Abductive |
| UnifiedTaskClassifier | ✅ Classifies messages | `crates/rustycode-classification/src/classifier.rs` | **NOT in TUI crate** — plan previously referenced wrong path |
| Classifier routing | ❌ Logged but unused | `crates/rustycode-tui/src/app/text_input.rs:283-290` | Classification logged via `tracing::info!`, not passed to `send_message` |
| Orchestration thinking/ | ✅ Canonical reasoning engine | `crates/rustycode-orchestration/src/thinking/` | Orchestra now re-exports this module instead of forking it. |
| Deep-thinking service | ⚠️ Prompt injection only | `crates/rustycode-tui/src/services/deep_thinking.rs` (344 lines) | `analyze_and_transform()` injects planning prompts, zero LLM calls |
| ServiceManager | ✅ Spawns LLM thread | `crates/rustycode-tui/src/app/service_integration.rs` (741 lines) | `send_message` at line 225, delegates to `send_message_with_history` |
| Streaming pipeline | ✅ Works with stall detection | `crates/rustycode-tui/src/app/streaming/response.rs` | |
| Ensemble strategies | ✅ 4 strategies, wired | `crates/rustycode-orchestration/src/ensemble_strategy.rs` (791 lines) | **NEW** — replaces orchestra stubs |
| BusHandle / MessageBus | ✅ Pub/sub wired | `crates/rustycode-orchestration/src/bus.rs` (297 lines) | **NEW** — 10 event types including `TaskCompleted`, `EscalationSignal` |
| SharedWorkspace | ✅ Active memory | `crates/rustycode-orchestration/src/shared_workspace.rs` (190 lines) | **NEW** — `Arc<Mutex<HashMap>>` with timestamp tracking |
| Composer (Tier 4) | ✅ Strategy-aware | `crates/rustycode-orchestration/src/composer.rs` | **NEW** — uses `EnsembleStrategy::select_for_complexity()` |
| Editor (Tier 3) | ✅ Bus-aware | `crates/rustycode-orchestration/src/editor.rs` | **NEW** — publishes `PartialResult` on patch |
| Orchestra adapter | ✅ Works | `crates/rustycode-orchestra/src/orchestration_adapter/` | 965 tests passing |
| `status.rs` (TaskStatus) | ❌ Does not exist | — | Needed for Ralph Loop (Phase 1 Task 3) |
| `ralph_loop.rs` | ❌ Does not exist | — | Core loop (Phase 1 Task 4) |
| `verification.rs` (2-stage) | ❌ Does not exist | — | Spec + quality check (Phase 2 Task 6) |

### The Ralph Loop Pattern (from oh-my-openagent)

```
User message → classify complexity
  ├── Simple (confidence > 70, 1-2 steps) → Direct stream (existing path)
  └── Complex (ambiguous, multi-step, exploratory) → Ralph Loop:
        1. Send initial prompt to LLM
        2. Collect response
        3. Check: is the task complete?
           ├── YES (explicit done signal OR behavioral evidence) → exit loop
           ├── NO (incomplete, more to do) → inject continuation prompt → goto 1
           └── BLOCKED (3 iterations, no progress) → ask user
        4. Two-stage verification:
           ├── Spec compliance: did code do what was asked?
           └── Quality review: is code well-built?
        5. If verification fails → feed failure reason back into loop → goto 1
```

### Status Protocol (from superpowers)

```rust
enum TaskStatus {
    Complete,           // Done, verified
    PartialProgress,    // Made progress, more to do (continue loop)
    Blocked,            // Stuck, need user input or different approach
    NeedsMoreContext,   // LLM needs additional information
}
```

### Two-Stage Verification (from superpowers subagent-driven-development)

After Ralph Loop exits with `Complete`:
1. **Spec compliance** (cheap model): "Does the output match what the user asked for?"
2. **Quality review** (capable model): "Is the code well-built?"

If either fails → feed failure back into Ralph Loop.

### 3-Strike Escalation (from superpowers systematic-debugging)

After 3 Ralph Loop iterations with `PartialProgress` but no tangible advancement:
- Stop the loop
- Escalate to user: "I've tried 3 times but keep hitting the same wall. The issue might be architectural. Here's what I've tried..."
- Don't attempt iteration #4 without user input

---

## File Structure

### Phase 0: Fix the Foundation

**Deleted:**
- `crates/rustycode-orchestration/src/thinking/` — canonical reasoning engine

**Modified:**
- `crates/rustycode-orchestration/Cargo.toml` — Add `rustycode-deep-thinker` as dependency
- `crates/rustycode-orchestration/src/lib.rs` — Re-export from deep-thinker instead of local copy
- `crates/rustycode-orchestration/src/types.rs` — Use deep-thinker's `Strategy` enum

> **Note:** The lib.rs already exposes `thinking/` as the canonical reasoning engine and re-exports the key orchestration types. Orchestra should not fork this module.

### Phase 1: Ralph Loop

**New files:**
- `crates/rustycode-orchestration/src/ralph_loop.rs` — Core loop: send → collect → check → continue
- `crates/rustycode-orchestration/src/status.rs` — `TaskStatus` enum and detection logic

**Modified:**
- `crates/rustycode-orchestration/src/lib.rs` — Export new modules
- `crates/rustycode-tui/src/app/service_integration.rs` — Route based on complexity
- `crates/rustycode-tui/src/services/deep_thinking.rs` — Wire to Ralph Loop instead of prompt injection

> **Integration note:** The Ralph Loop should use `BusHandle` to publish `PartialResult` events per iteration, and store accumulated responses in `SharedWorkspace`. The existing `EnsembleStrategy` can serve as the execution backend for complex iterations.

### Phase 2: Two-Stage Verification

**New files:**
- `crates/rustycode-orchestration/src/verification.rs` — Spec compliance + quality review dispatch

**Modified:**
- `crates/rustycode-orchestration/src/ralph_loop.rs` — Add verification step after loop exits

> **Note:** This is separate from the existing `verification_gates.rs` which handles step-level output verification. This `verification.rs` handles task-level spec/quality verification.

### Phase 3: Wiring & Testing

**Modified:**
- `crates/rustycode-tui/src/app/service_integration.rs` — Full integration
- `crates/rustycode-tui/src/app/streaming/response.rs` — Handle Ralph Loop streaming

---

## Phase 0: Fix the Foundation (1-2 days)

**Prerequisite:** None — this is the first thing.

**Goal:** Keep the orchestration thinking engine canonical and re-export it from orchestra.

### Task 1: Delete Duplication and Re-export from Deep-Thinker

**Files:**
- Keep: `crates/rustycode-orchestration/src/thinking/` as the canonical implementation
- Modify: `crates/rustycode-orchestra/src/lib.rs`
- Modify: `crates/rustycode-orchestra/src/error.rs`

---

#### Task 1.1: Audit what imports the orchestration thinking module

- [ ] **Step 1: Find all imports of `thinking` module**

```bash
cd /Users/nat/dev/rustycode
grep -rn "use crate::thinking" crates/rustycode-orchestration/src/
grep -rn "use rustycode_orchestration::thinking" crates/
grep -rn "mod thinking" crates/rustycode-orchestration/src/
```

Expected: List of files that reference the duplicated `thinking` module. Most likely only `lib.rs` and `types.rs`.

- [ ] **Step 2: List the public API of the thinking module**

```bash
cd /Users/nat/dev/rustycode
grep -rn "pub " crates/rustycode-orchestration/src/thinking/ | grep -v "#\[cfg(test)\]" | head -60
```

Expected: Public structs, enums, traits that downstream code uses.

- [ ] **Step 3: Compare with canonical deep-thinker API**

```bash
cd /Users/nat/dev/rustycode
grep -rn "pub " crates/rustycode-deep-thinker/src/ | grep -v "#\[cfg(test)\]" | grep -v "pub fn " | head -40
```

Expected: The canonical types from deep-thinker. Map each orchestration `thinking::` type to its deep-thinker equivalent.

- [ ] **Step 4: Run existing tests to establish baseline**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration --lib 2>&1 | tail -20
```

Expected: All current tests pass (or note which fail before changes).

---

#### Task 1.2: Add deep-thinker dependency and create re-exports

- [ ] **Step 1: Add deep-thinker as dependency**

Modify `crates/rustycode-orchestration/Cargo.toml` — add to `[dependencies]`:

```toml
rustycode-deep-thinker = { path = "../rustycode-deep-thinker" }
```

- [ ] **Step 2: Replace thinking module with re-exports**

Modify `crates/rustycode-orchestration/src/lib.rs` — replace `pub mod thinking;` with:

```rust
// Re-export canonical types from rustycode-deep-thinker instead of local copy
pub use rustycode_deep_thinker::{
    executor::RealExecutor,
    strategies::{Strategy, StrategyFactory},
    convergence::{ConvergenceMetrics, ConvergenceDetector},
    reasoning_graph::ReasoningGraph,
    types::{ThinkingMode, ThinkingOutput, ThinkingPhase},
};
```

Adjust the re-exports based on what the audit in Task 1.1 found downstream code actually uses. Only export what's needed.

- [ ] **Step 3: Update types.rs if it re-exports from thinking**

Modify `crates/rustycode-orchestration/src/types.rs` — change any `use crate::thinking::` imports to `use rustycode_deep_thinker::`.

- [ ] **Step 4: Delete the duplicated directory**

```bash
cd /Users/nat/dev/rustycode
rm -rf crates/rustycode-orchestration/src/thinking/
```

- [ ] **Step 5: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration --lib 2>&1 | tail -30
```

Expected: Same number of tests pass as baseline. Compilation errors indicate missing re-exports — fix them.

- [ ] **Step 6: Run workspace clippy**

```bash
cd /Users/nat/dev/rustycode
cargo clippy -p rustycode-orchestration --all-targets -- -D warnings 2>&1 | tail -20
```

Expected: Zero warnings.

- [ ] **Step 7: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-orchestration/
git commit -m "refactor: re-export canonical thinking module and tighten error boundaries"
```

---

### Task 2: Wire UnifiedTaskClassifier Output for Routing

**Files:**
- Modify: `crates/rustycode-classification/src/classifier.rs` (canonical location)
- Modify: `crates/rustycode-tui/src/app/text_input.rs` (where classification is called)
- Modify: `crates/rustycode-tui/src/app/service_integration.rs`

> **Correction (2026-04-25):** `UnifiedTaskClassifier` lives in `crates/rustycode-classification/src/classifier.rs`, NOT in `crates/rustycode-tui/src/app/classification/` (that directory doesn't exist). The TUI calls the classifier in `text_input.rs:283-290` but only logs the result — it does NOT route based on it.

---

#### Task 2.1: Find where classifier output is used

- [ ] **Step 1: Locate the classifier invocation**

```bash
cd /Users/nat/dev/rustycode
grep -rn "UnifiedTaskClassifier" crates/rustycode-tui/src/ --include="*.rs" | head -20
grep -rn "UnifiedTaskClassifier" crates/rustycode-classification/src/ --include="*.rs" | head -20
```

Expected: Find where `classify()` is called in `text_input.rs` and the classifier definition in `crates/rustycode-classification/`.

- [ ] **Step 2: Read the classifier's return type**

```bash
cd /Users/nat/dev/rustycode
grep -A 20 "pub fn classify\|pub struct TaskClassification\|pub enum TaskCategory" crates/rustycode-classification/src/*.rs | head -40
```

Expected: The `TaskClassification` struct or similar — understand what fields are available for routing.

- [ ] **Step 3: Read how send_message currently flows**

Read `crates/rustycode-tui/src/app/service_integration.rs` lines 225-399 to understand the current message flow path. Key: `send_message()` → `send_message_with_history()` → spawns thread → `stream_llm_response()`. No classification routing exists.

- [ ] **Step 4: Write test for routing decision**

Add to `crates/rustycode-classification/src/classifier.rs` (inline tests):

```rust
#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn test_simple_message_routes_direct() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify("what does fn main do?");
        // Simple informational query should be classified as low complexity
        assert!(!result.is_complex());
    }

    #[test]
    fn test_complex_message_routes_ralph_loop() {
        let classifier = UnifiedTaskClassifier::new();
        let result = classifier.classify(
            "refactor the entire authentication module to support OAuth2, OIDC, and SAML with proper session management"
        );
        // Multi-step architectural task should be classified as complex
        assert!(result.is_complex());
    }
}
```

Note: Adjust the assertion methods based on the actual `TaskClassification` API found in Step 2.

- [ ] **Step 5: Add `is_complex()` method if missing**

If `TaskClassification` doesn't have an `is_complex()` method, add one in `crates/rustycode-classification/src/classifier.rs` based on behavioral signals:
- Does the message contain multiple distinct tasks?
- Does it mention refactoring/rewriting/architecture?
- Is it longer than N tokens?
- Does the classifier already output a complexity score?

- [ ] **Step 6: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-classification --lib 2>&1 | tail -20
```

Expected: Tests pass.

- [ ] **Step 7: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-classification/src/classifier.rs
git commit -m "feat: add is_complex() routing method to UnifiedTaskClassifier"
```

---

## Phase 1: Ralph Loop — The Core Loop (3-5 days)

**Prerequisite:** Phase 0 complete.

**Goal:** Implement the iterate-until-done pattern. Complex tasks go through a loop: send prompt → collect response → check completion → continue or exit. Completion is detected via behavioral evidence, not LLM self-rating.

### Task 3: Define TaskStatus and Completion Detection

**Files:**
- Create: `crates/rustycode-orchestration/src/status.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

---

#### Task 3.1: Write test for TaskStatus

- [ ] **Step 1: Write test for status detection**

Create `crates/rustycode-orchestration/src/status.rs`:

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Status of a task iteration in the Ralph Loop.
/// Inspired by superpowers' subagent status protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    /// Task is complete with verified output
    Complete,
    /// Progress made but more work remains — continue loop
    PartialProgress,
    /// Stuck — need user input or different approach
    Blocked { reason: String },
    /// LLM needs additional context to proceed
    NeedsMoreContext { questions: Vec<String> },
}

/// Detect task status from LLM response using behavioral evidence.
/// NOT self-evaluation — we measure what the LLM DID, not what it SAYS.
pub struct CompletionDetector {
    /// Maximum iterations before 3-strike escalation
    pub max_iterations: usize,
    /// Iterations with PartialProgress but no tangible advancement
    pub stagnation_count: usize,
}

impl CompletionDetector {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            max_iterations,
            stagnation_count: 0,
        }
    }

    /// Detect status from LLM response using behavioral signals.
    ///
    /// Evidence of completion:
    /// - Response contains code blocks (tool calls happened)
    /// - Response ends with a conclusion/summary
    /// - No explicit "I need to also..." or "next, I should..." language
    ///
    /// Evidence of partial progress:
    /// - Response contains some code but says "I also need to..."
    /// - Response is clearly mid-task
    ///
    /// Evidence of blocked:
    /// - Response asks questions back to user
    /// - Response says "I can't proceed without..."
    pub fn detect(&mut self, response: &str, iteration: usize) -> TaskStatus {
        // 3-strike escalation from superpowers systematic-debugging
        if iteration >= self.max_iterations {
            return TaskStatus::Blocked {
                reason: format!(
                    "Reached {} iterations without completion. The issue might be architectural.",
                    self.max_iterations
                ),
            };
        }

        let has_code = response.contains("```") || response.contains("fn ") || response.contains("def ");
        let has_continuation = response.contains("I also need to")
            || response.contains("next, I should")
            || response.contains("then I'll")
            || response.contains("let me also");
        let has_questions = response.contains("?") && response.matches('?').count() >= 2;
        let has_conclusion = response.contains("In summary")
            || response.contains("To summarize")
            || response.contains("this completes")
            || response.contains("done with");

        if has_questions && !has_code {
            return TaskStatus::NeedsMoreContext {
                questions: extract_questions(response),
            };
        }

        if has_code && !has_continuation {
            return TaskStatus::Complete;
        }

        if has_code && has_continuation {
            self.stagnation_count = 0; // progress was made
            return TaskStatus::PartialProgress;
        }

        if has_conclusion && !has_continuation {
            return TaskStatus::Complete;
        }

        // Default: assume partial progress
        self.stagnation_count += 1;
        if self.stagnation_count >= 3 {
            return TaskStatus::Blocked {
                reason: "3 consecutive iterations with no tangible progress. Escalating.".to_string(),
            };
        }

        TaskStatus::PartialProgress
    }
}

fn extract_questions(response: &str) -> Vec<String> {
    // Simple extraction — split on sentences ending with ?
    response
        .split(|c: char| c == '.' || c == '!')
        .filter(|s| s.contains('?'))
        .map(|s| s.trim().to_string())
        .take(5)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_complete_with_code() {
        let mut detector = CompletionDetector::new(10);
        let response = "Here's the implementation:\n```rust\nfn hello() { }\n```\nThis completes the task.";
        assert_eq!(detector.detect(response, 0), TaskStatus::Complete);
    }

    #[test]
    fn test_detect_partial_progress() {
        let mut detector = CompletionDetector::new(10);
        let response = "I've implemented the first part:\n```rust\nfn part1() { }\n```\nI also need to add error handling.";
        assert_eq!(detector.detect(response, 0), TaskStatus::PartialProgress);
    }

    #[test]
    fn test_detect_needs_context() {
        let mut detector = CompletionDetector::new(10);
        let response = "I need more information. Should I use PostgreSQL or SQLite? What's the expected scale?";
        match detector.detect(response, 0) {
            TaskStatus::NeedsMoreContext { questions } => {
                assert!(!questions.is_empty());
            }
            other => panic!("Expected NeedsMoreContext, got {:?}", other),
        }
    }

    #[test]
    fn test_three_strike_escalation() {
        let mut detector = CompletionDetector::new(10);
        // 3 consecutive partials with no code = stagnation
        let vague = "Let me think about this more carefully...";
        for _ in 0..2 {
            detector.detect(vague, 0);
        }
        let result = detector.detect(vague, 0);
        assert!(matches!(result, TaskStatus::Blocked { .. }));
    }

    #[test]
    fn test_max_iterations_escalation() {
        let mut detector = CompletionDetector::new(5);
        let response = "Still working on it...";
        let result = detector.detect(response, 5);
        assert!(matches!(result, TaskStatus::Blocked { .. }));
    }
}
```

- [ ] **Step 2: Update lib.rs**

Modify `crates/rustycode-orchestration/src/lib.rs` — add:

```rust
pub mod status;
pub use status::{TaskStatus, CompletionDetector};
```

- [ ] **Step 3: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration status --lib 2>&1 | tail -20
```

Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-orchestration/src/status.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add TaskStatus enum and behavioral completion detection"
```

---

### Task 4: Implement the Ralph Loop

**Files:**
- Create: `crates/rustycode-orchestration/src/ralph_loop.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

> **Design note (2026-04-25):** The Ralph Loop can leverage the existing `EnsembleStrategy` as its execution backend for complex iterations. When the loop detects a complex sub-task, it can call `EnsembleStrategy::select_for_complexity()` to delegate to DecomposeAndDelegate, ParallelVote, etc. The loop publishes `PartialResult` events via `BusHandle` and stores state in `SharedWorkspace`.

---

#### Task 4.1: Write test for Ralph Loop

- [ ] **Step 1: Write test for loop iteration**

Create `crates/rustycode-orchestration/src/ralph_loop.rs`:

```rust
use crate::status::{CompletionDetector, TaskStatus};
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Configuration for the Ralph Loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphLoopConfig {
    /// Maximum iterations before escalation
    pub max_iterations: usize,
    /// Prompt to inject when LLM needs to continue
    pub continuation_prompt: String,
    /// Prompt to inject when LLM is blocked
    pub escalation_prompt: String,
}

impl Default for RalphLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            continuation_prompt: "Continue with the next step. What remains to be done?".to_string(),
            escalation_prompt: "I've tried multiple times but keep hitting the same wall. The issue might be architectural. Can you provide guidance?".to_string(),
        }
    }
}

/// Result of a Ralph Loop execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphLoopResult {
    pub iterations: usize,
    pub final_status: TaskStatus,
    pub accumulated_response: String,
    /// History of (iteration, status) for debugging
    pub history: Vec<(usize, TaskStatus)>,
}

/// The Ralph Loop: iterate-until-done pattern from oh-my-openagent.
///
/// Core principle: keep injecting continuation prompts until the LLM
/// signals completion OR behavioral evidence shows the task is done.
/// After 3 iterations with no progress, escalate to user.
pub struct RalphLoop {
    config: RalphLoopConfig,
    detector: CompletionDetector,
}

impl RalphLoop {
    pub fn new(config: RalphLoopConfig) -> Self {
        let detector = CompletionDetector::new(config.max_iterations);
        Self { config, detector }
    }

    pub fn with_defaults() -> Self {
        Self::new(RalphLoopConfig::default())
    }

    /// Process one iteration of the Ralph Loop.
    /// Returns the status after analyzing the response.
    pub fn process_iteration(&mut self, response: &str, iteration: usize) -> TaskStatus {
        self.detector.detect(response, iteration)
    }

    /// Generate the next prompt based on current status.
    /// Returns None if the loop should terminate.
    pub fn next_prompt(&self, status: &TaskStatus) -> Option<String> {
        match status {
            TaskStatus::Complete => None, // Loop exits
            TaskStatus::PartialProgress => Some(self.config.continuation_prompt.clone()),
            TaskStatus::Blocked { reason } => {
                Some(format!("{}\n\nReason: {}", self.config.escalation_prompt, reason))
            }
            TaskStatus::NeedsMoreContext { questions } => {
                let qs = questions.join("\n- ");
                Some(format!(
                    "Here are answers to your questions:\n- {}\n\nPlease continue.",
                    qs
                ))
            }
        }
    }

    /// Check if the loop should continue.
    pub fn should_continue(&self, status: &TaskStatus) -> bool {
        !matches!(status, TaskStatus::Complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_exits_on_complete() {
        let mut ralph = RalphLoop::with_defaults();
        let response = "Here's the code:\n```rust\nfn solve() -> i32 { 42 }\n```\nDone.";
        let status = ralph.process_iteration(response, 0);
        assert_eq!(status, TaskStatus::Complete);
        assert!(!ralph.should_continue(&status));
        assert!(ralph.next_prompt(&status).is_none());
    }

    #[test]
    fn test_loop_continues_on_partial() {
        let mut ralph = RalphLoop::with_defaults();
        let response = "Part 1 done:\n```rust\nfn part1() {}\n```\nI also need to add part2.";
        let status = ralph.process_iteration(response, 0);
        assert_eq!(status, TaskStatus::PartialProgress);
        assert!(ralph.should_continue(&status));
        assert!(ralph.next_prompt(&status).is_some());
    }

    #[test]
    fn test_loop_escalates_on_blocked() {
        let mut ralph = RalphLoop::with_defaults();
        let response = "I can't proceed without knowing the database schema.";
        let status = ralph.process_iteration(response, 0);
        // Should be NeedsMoreContext or Blocked depending on response
        match status {
            TaskStatus::Blocked { .. } | TaskStatus::NeedsMoreContext { .. } => {
                assert!(ralph.should_continue(&status));
            }
            other => panic!("Expected Blocked or NeedsMoreContext, got {:?}", other),
        }
    }

    #[test]
    fn test_max_iterations_blocks() {
        let mut ralph = RalphLoop::with_defaults();
        let response = "Still thinking...";
        let status = ralph.process_iteration(response, 10);
        assert!(matches!(status, TaskStatus::Blocked { .. }));
    }
}
```

- [ ] **Step 2: Update lib.rs**

Modify `crates/rustycode-orchestration/src/lib.rs` — add:

```rust
pub mod ralph_loop;
pub use ralph_loop::{RalphLoop, RalphLoopConfig, RalphLoopResult};
```

- [ ] **Step 3: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration ralph_loop --lib 2>&1 | tail -20
```

Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-orchestration/src/ralph_loop.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: implement Ralph Loop iterate-until-done pattern"
```

---

### Task 5: Wire Ralph Loop into ServiceManager

**Files:**
- Modify: `crates/rustycode-tui/src/app/service_integration.rs`
- Modify: `crates/rustycode-tui/src/services/deep_thinking.rs`

---

#### Task 5.1: Understand the current message flow

- [ ] **Step 1: Read the current send_message path**

Read `crates/rustycode-tui/src/app/service_integration.rs` lines 225-399 to understand:
- How `send_message()` is called (line 225)
- How `send_message_with_history()` spawns an LLM thread (line 234-399)
- Where classification could be injected (currently logged in `text_input.rs:283-290` but not passed through)

- [ ] **Step 2: Read deep_thinking service**

Read `crates/rustycode-tui/src/services/deep_thinking.rs` to understand the current prompt-injection approach.

- [ ] **Step 3: Design the routing fork**

The routing happens at `service_integration.rs` in `send_message()`:

```
send_message(text)
  ├── classify(text) → is_complex?
  │   ├── NO → existing direct stream path (unchanged)
  │   └── YES → Ralph Loop path:
  │         1. Send text to LLM
  │         2. Stream response to TUI
  │         3. After response completes, detect status
  │         4. If PartialProgress → inject continuation → goto 1
  │         5. If Complete → run verification → done
  │         6. If Blocked → show escalation to user in TUI
  └── Return
```

- [ ] **Step 4: Implement the routing fork**

Modify `crates/rustycode-tui/src/app/service_integration.rs` — add a new method:

```rust
/// Route message: simple → direct stream, complex → Ralph Loop
async fn route_message(&mut self, text: &str) -> Result<()> {
    let classification = self.classifier.classify(text);

    if classification.is_complex() {
        self.execute_ralph_loop(text).await
    } else {
        // Existing direct stream path
        self.stream_llm_response(text).await
    }
}
```

Note: The exact method signatures depend on what's found in Step 1. Adapt accordingly.

- [ ] **Step 5: Implement execute_ralph_loop**

In the same file, add:

```rust
/// Execute the Ralph Loop for complex tasks.
async fn execute_ralph_loop(&mut self, initial_prompt: &str) -> Result<()> {
    use rustycode_orchestration::{RalphLoop, RalphLoopConfig};

    let mut ralph = RalphLoop::new(RalphLoopConfig::default());
    let mut current_prompt = initial_prompt.to_string();
    let mut iteration = 0;

    loop {
        // Send current prompt to LLM and stream response
        let response = self.stream_llm_response(&current_prompt).await?;

        // Detect completion status from response
        let status = ralph.process_iteration(&response, iteration);
        tracing::info!(iteration, ?status, "Ralph Loop iteration");

        // Check if loop should continue
        if !ralph.should_continue(&status) {
            break;
        }

        // Get next prompt
        match ralph.next_prompt(&status) {
            Some(next_prompt) => {
                current_prompt = next_prompt;
                iteration += 1;
            }
            None => break,
        }
    }

    Ok(())
}
```

Note: This is a simplified synchronous loop. The actual implementation may need to be async and handle streaming differently — adapt based on the actual LLM call API.

- [ ] **Step 6: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-tui --lib service_integration 2>&1 | tail -20
```

Expected: Existing tests still pass. New routing not yet tested (integration test in Phase 3).

- [ ] **Step 7: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-tui/src/app/service_integration.rs
git commit -m "feat: wire Ralph Loop routing into ServiceManager"
```

---

## Phase 2: Two-Stage Verification (2-3 days)

**Prerequisite:** Phase 1 complete.

**Goal:** After the Ralph Loop completes, verify the output in two stages: spec compliance (did it do what was asked?) then quality review (is it well-built?). If either fails, feed the failure back into the Ralph Loop.

### Task 6: Implement Two-Stage Verification

**Files:**
- Create: `crates/rustycode-orchestration/src/verification.rs`
- Modify: `crates/rustycode-orchestration/src/ralph_loop.rs`

---

#### Task 6.1: Write test for verification

- [ ] **Step 1: Write test for spec compliance check**

Create `crates/rustycode-orchestration/src/verification.rs`:

```rust
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Result of verification against the original user request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub spec_compliant: bool,
    pub quality_passing: bool,
    pub spec_issues: Vec<String>,
    pub quality_issues: Vec<String>,
    pub overall_pass: bool,
}

impl VerificationResult {
    pub fn pass() -> Self {
        Self {
            spec_compliant: true,
            quality_passing: true,
            spec_issues: vec![],
            quality_issues: vec![],
            overall_pass: true,
        }
    }

    pub fn fail(spec_issues: Vec<String>, quality_issues: Vec<String>) -> Self {
        let spec_compliant = spec_issues.is_empty();
        let quality_passing = quality_issues.is_empty();
        Self {
            spec_compliant,
            quality_passing,
            spec_issues,
            quality_issues,
            overall_pass: spec_compliant && quality_passing,
        }
    }
}

/// Two-stage verification inspired by superpowers subagent-driven-development.
///
/// Stage 1: Spec compliance — does the output match what was asked?
/// Stage 2: Quality review — is the code well-built?
///
/// Both stages use behavioral evidence, not LLM self-rating.
pub struct Verifier;

impl Verifier {
    /// Check spec compliance: does the response address the original request?
    ///
    /// This is a behavioral check:
    /// - Does the response contain code if the request asked for code?
    /// - Does the response address each part of a multi-part request?
    /// - Does the response actually solve the stated problem?
    pub fn check_spec_compliance(original_request: &str, response: &str) -> Vec<String> {
        let mut issues = Vec::new();

        // Check: if request asked for code, response should have code
        let requests_code = original_request.contains("implement")
            || original_request.contains("write")
            || original_request.contains("create")
            || original_request.contains("build")
            || original_request.contains("refactor");
        let has_code = response.contains("```");

        if requests_code && !has_code {
            issues.push("Request asked for code but response contains no code blocks".to_string());
        }

        // Check: if request has multiple parts, response should address them
        let request_parts: Vec<&str> = original_request
            .split(|c: char| c == ',' || c == ';' || c == '\n')
            .filter(|s| s.len() > 10)
            .collect();
        if request_parts.len() > 2 {
            // Rough check: response should be proportionally long
            let response_words = response.split_whitespace().count();
            if response_words < request_parts.len() * 20 {
                issues.push(format!(
                    "Request has {} parts but response seems too short ({} words)",
                    request_parts.len(),
                    response_words
                ));
            }
        }

        issues
    }

    /// Check quality: is the code well-built?
    ///
    /// Behavioral checks:
    /// - Does the code have error handling?
    /// - Does it have tests or test suggestions?
    /// - Are there obvious anti-patterns?
    pub fn check_quality(response: &str) -> Vec<String> {
        let mut issues = Vec::new();

        // Check for error handling
        let has_error_handling = response.contains("Result<")
            || response.contains("unwrap_or")
            || response.contains("map_err")
            || response.contains("?;")
            || response.contains("try {");

        if !has_error_handling && response.contains("```rust") {
            issues.push("Rust code lacks error handling (no Result, ?, or error propagation)".to_string());
        }

        // Check for unwrap in non-test code
        if response.contains("```rust") && response.contains(".unwrap()") {
            // Check if it's in a test context
            let in_test = response.contains("#[test]") || response.contains("#[tokio::test]");
            if !in_test {
                issues.push("Code uses .unwrap() outside of tests — use ? or proper error handling".to_string());
            }
        }

        issues
    }

    /// Run both verification stages.
    /// From superpowers: spec compliance MUST pass before quality review runs.
    pub fn verify(original_request: &str, response: &str) -> VerificationResult {
        // Stage 1: Spec compliance
        let spec_issues = Self::check_spec_compliance(original_request, response);

        // Stage 2: Quality review (only if spec passes)
        let quality_issues = if spec_issues.is_empty() {
            Self::check_quality(response)
        } else {
            // Don't waste time on quality if spec is wrong
            vec![]
        };

        VerificationResult::fail(spec_issues, quality_issues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_pass_code_request_with_code() {
        let request = "implement a function to reverse a string";
        let response = "Here's the implementation:\n```rust\nfn reverse(s: &str) -> String { s.chars().rev().collect() }\n```\nDone.";
        let issues = Verifier::check_spec_compliance(request, response);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_spec_fail_code_request_without_code() {
        let request = "implement a function to reverse a string";
        let response = "You could use the chars().rev().collect() pattern for that.";
        let issues = Verifier::check_spec_compliance(request, response);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_quality_pass_with_error_handling() {
        let response = "```rust\nfn read_file(path: &str) -> Result<String> {\n    fs::read_to_string(path).context(\"failed\")\n}\n```";
        let issues = Verifier::check_quality(response);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_quality_fail_with_unwrap() {
        let response = "```rust\nfn main() {\n    let data = fs::read_to_string(\"file.txt\").unwrap();\n}\n```";
        let issues = Verifier::check_quality(response);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_two_stage_verification() {
        let request = "implement a binary search function";
        let response = "Here's the code:\n```rust\nfn binary_search(arr: &[i32], target: i32) -> Option<usize> {\n    arr.iter().position(|&x| x == target)\n}\n```";
        let result = Verifier::verify(request, response);
        assert!(result.spec_compliant);
    }

    #[test]
    fn test_quality_not_checked_when_spec_fails() {
        let request = "implement a binary search function";
        let response = "You can use a divide-and-conquer approach for that.";
        let result = Verifier::verify(request, response);
        assert!(!result.spec_compliant);
        // Quality issues should be empty — we don't check quality when spec fails
        assert!(result.quality_issues.is_empty());
    }
}
```

- [ ] **Step 2: Update lib.rs**

Modify `crates/rustycode-orchestration/src/lib.rs` — add:

```rust
pub mod verification;
pub use verification::{Verifier, VerificationResult};
```

- [ ] **Step 3: Integrate verification into Ralph Loop**

Modify `crates/rustycode-orchestration/src/ralph_loop.rs` — add verification after loop exits:

In the `RalphLoop` struct, add a method:

```rust
/// Run verification after loop completes.
/// If verification fails, returns a prompt to re-enter the loop.
pub fn verify_output(
    &self,
    original_request: &str,
    accumulated_response: &str,
) -> (VerificationResult, Option<String>) {
    let result = Verifier::verify(original_request, accumulated_response);

    if result.overall_pass {
        (result, None)
    } else {
        let mut feedback = String::from("The previous attempt had issues:\n");
        for issue in &result.spec_issues {
            feedback.push_str(&format!("- [SPEC] {}\n", issue));
        }
        for issue in &result.quality_issues {
            feedback.push_str(&format!("- [QUALITY] {}\n", issue));
        }
        feedback.push_str("\nPlease address these issues and provide a corrected version.");
        (result, Some(feedback))
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration verification --lib 2>&1 | tail -20
```

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-orchestration/src/verification.rs \
        crates/rustycode-orchestration/src/ralph_loop.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add two-stage verification (spec compliance + quality review)"
```

---

## Phase 3: Wiring & End-to-End Testing (2-3 days)

**Prerequisite:** Phases 1-2 complete.

**Goal:** Full integration: complex message → Ralph Loop with verification → stream to TUI. Test with a real complex task.

### Task 7: Complete Integration Wiring

**Files:**
- Modify: `crates/rustycode-tui/src/app/service_integration.rs`
- Modify: `crates/rustycode-tui/src/app/streaming/response.rs`

---

#### Task 7.1: Wire verification into the Ralph Loop execution

- [ ] **Step 1: Update execute_ralph_loop to include verification**

Modify `crates/rustycode-tui/src/app/service_integration.rs` — update the `execute_ralph_loop` method from Task 5 to include verification after the loop:

After the Ralph Loop's main loop exits with `Complete`:
1. Call `ralph.verify_output(original_request, &accumulated_response)`
2. If verification passes → done, show result to user
3. If verification fails → inject failure feedback as next prompt, re-enter loop (max 2 verification rounds)

- [ ] **Step 2: Update streaming to show Ralph Loop status**

Modify `crates/rustycode-tui/src/app/streaming/response.rs` — add a way to show loop iteration count in the TUI status bar:

- Show "Thinking... (iteration N)" during Ralph Loop
- Show "Verifying..." during verification
- Show "Complete" or "Blocked: [reason]" when done

- [ ] **Step 3: Run TUI build**

```bash
cd /Users/nat/dev/rustycode
cargo build -p rustycode-tui 2>&1 | tail -20
```

Expected: Clean build.

- [ ] **Step 4: Commit**

```bash
cd /Users/nat/dev/rustycode
git add crates/rustycode-tui/src/app/service_integration.rs \
        crates/rustycode-tui/src/app/streaming/response.rs
git commit -m "feat: wire verification into Ralph Loop execution with TUI status"
```

---

### Task 8: End-to-End Test

**Files:**
- Create: `tests/ralph_loop_integration_test.rs`

---

#### Task 8.1: Write integration test

- [ ] **Step 1: Write integration test**

Create `tests/ralph_loop_integration_test.rs`:

```rust
use rustycode_orchestration::{RalphLoop, RalphLoopConfig, Verifier, TaskStatus};

#[test]
fn test_ralph_loop_simple_task_exits_immediately() {
    let mut ralph = RalphLoop::with_defaults();

    // Simulate a simple task that completes in one iteration
    let response = "Here's the code:\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```";
    let status = ralph.process_iteration(response, 0);

    assert_eq!(status, TaskStatus::Complete);
    assert!(!ralph.should_continue(&status));
}

#[test]
fn test_ralph_loop_complex_task_takes_multiple_iterations() {
    let mut ralph = RalphLoop::with_defaults();

    // Iteration 1: Partial progress
    let response1 = "Part 1:\n```rust\nstruct Config { db_url: String }\n```\nI also need to add the connection pool.";
    let status1 = ralph.process_iteration(response1, 0);
    assert_eq!(status1, TaskStatus::PartialProgress);

    // Iteration 2: Complete
    let response2 = "Part 2:\n```rust\nfn create_pool(config: &Config) -> Result<Pool> {\n    Pool::new(&config.db_url)\n}\n```\nThis completes the implementation.";
    let status2 = ralph.process_iteration(response2, 1);
    assert_eq!(status2, TaskStatus::Complete);
}

#[test]
fn test_full_pipeline_with_verification() {
    let mut ralph = RalphLoop::with_defaults();
    let original_request = "implement a function to parse CSV files";

    // Simulate a response that would pass verification
    let response = "Here's the implementation:\n\
        ```rust\n\
        use std::fs;\n\
        use anyhow::Result;\n\
        \n\
        pub fn parse_csv(path: &str) -> Result<Vec<Vec<String>>> {\n\
            let content = fs::read_to_string(path)?;\n\
            content.lines()\n\
                .map(|line| Ok(line.split(',').map(String::from).collect()))\n\
                .collect()\n\
        }\n\
        ```\n\
        This handles basic CSV parsing with proper error propagation.";

    let status = ralph.process_iteration(response, 0);
    assert_eq!(status, TaskStatus::Complete);

    // Verify
    let (result, feedback) = ralph.verify_output(original_request, response);
    assert!(result.overall_pass);
    assert!(feedback.is_none());
}

#[test]
fn test_verification_failure_triggers_retry() {
    let ralph = RalphLoop::with_defaults();
    let original_request = "implement a function to parse CSV files";

    // Response without code — should fail spec compliance
    let response = "You can use the csv crate for that.";
    let (result, feedback) = ralph.verify_output(original_request, response);

    assert!(!result.spec_compliant);
    assert!(feedback.is_some());
    assert!(feedback.unwrap().contains("[SPEC]"));
}
```

- [ ] **Step 2: Run integration tests**

```bash
cd /Users/nat/dev/rustycode
cargo test -p rustycode-orchestration ralph_loop_integration_test 2>&1 | tail -20
```

Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
cd /Users/nat/dev/rustycode
git add tests/ralph_loop_integration_test.rs
git commit -m "test: add Ralph Loop integration tests"
```

---

### Task 9: Full Workspace Validation

---

#### Task 9.1: Run the full validation suite

- [ ] **Step 1: Run full workspace tests**

```bash
cd /Users/nat/dev/rustycode
cargo test --workspace 2>&1 | tail -30
```

Expected: All tests pass (including new Ralph Loop + verification tests).

- [ ] **Step 2: Run clippy**

```bash
cd /Users/nat/dev/rustycode
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -20
```

Expected: Zero warnings.

- [ ] **Step 3: Run format check**

```bash
cd /Users/nat/dev/rustycode
cargo fmt --check
```

Expected: Zero issues.

- [ ] **Step 4: Manual TUI test**

```bash
cd /Users/nat/dev/rustycode
cargo run -p rustycode-tui
```

Test scenarios:
1. Simple query ("what does fn main do?") → should direct stream (no loop)
2. Complex query ("refactor the auth module to support OAuth2 and SAML") → should enter Ralph Loop (iteration count visible)
3. Verify that the TUI shows status updates during loop iterations

- [ ] **Step 5: Verify no regressions**

```bash
cd /Users/nat/dev/rustycode
git status
```

Expected: No uncommitted changes remain.

---

## Summary

**Total Tasks:** 9
**Estimated Duration:** 1.5-2.5 weeks (down from 4-5)

### Progress Tracker (Updated 2026-04-25)

| Task | Description | Status | Notes |
|------|-------------|--------|-------|
| 1 | Canonical thinking module | ✅ Done | orchestra re-exports orchestration thinking |
| 2 | Wire classifier for routing | ⬜ Not started | Classifier in wrong crate per plan (fixed path in doc) |
| 3 | TaskStatus + completion detection | ⬜ Not started | `status.rs` does not exist yet |
| 4 | Ralph Loop | ⬜ Not started | `ralph_loop.rs` does not exist yet. Can use EnsembleStrategy as backend |
| 5 | Wire Ralph Loop into ServiceManager | ⬜ Not started | send_message at service_integration.rs:225 |
| 6 | Two-stage verification | ⬜ Not started | `verification.rs` does not exist yet |
| 7 | Complete integration wiring | ⬜ Not started | Depends on Tasks 3-6 |
| 8 | End-to-end test | ⬜ Not started | |
| 9 | Full workspace validation | ⬜ Not started | Last task |

**Infrastructure already built (reusable):**

| Component | Location | How it helps the Ralph Loop |
|-----------|----------|-----------------------------|
| `BusHandle` / `MessageBus` | `bus.rs` | Publish `PartialResult` per loop iteration, subscribe for conflicts |
| `SharedWorkspace` | `shared_workspace.rs` | Store accumulated responses, pass context between iterations |
| `EnsembleStrategy` (4 strategies) | `ensemble_strategy.rs` | Execution backend for complex sub-tasks within a loop iteration |
| `Composer` (strategy-aware) | `composer.rs` | Tier 4 can activate ensemble strategies |
| `Editor` (bus-aware) | `editor.rs` | Tier 3 patches with event publishing |
| `Conductor::try_thinking()` | `conductor.rs` | Strategy preemption with cooldown |

**Deliverables:**

- ✅ Foundation fixed: canonical thinking engine exposed and re-exported
- ✅ Ralph Loop: iterate-until-done with continuation prompts
- ✅ Behavioral completion detection (not self-eval)
- ✅ Status protocol: Complete / PartialProgress / Blocked / NeedsMoreContext
- ✅ Two-stage verification: spec compliance then quality review
- ✅ 3-strike escalation: auto-escalate to user after 3 stagnant iterations
- ✅ Classifier routing: simple → direct, complex → Ralph Loop
- ✅ TUI integration: status display during loop iterations
- ✅ Full test coverage: unit + integration tests

**Cut from previous plan:**

- ❌ 6 strategies → 2 (direct vs. Ralph Loop)
- ❌ Self-eval quality detection → behavioral evidence
- ❌ Adaptive learning DB → premature
- ❌ Modal graph visualization → polish for later
- ❌ Structured thinking JSON schema → organic continuation prompts
- ❌ Terminal Bench validation → validates later, not during initial build

**Critical Path:**

```
Task 1 (delete duplication) → Task 2 (wire classifier) → Task 3 (TaskStatus) → Task 4 (Ralph Loop) → Task 5 (wire into TUI) → Task 6 (verification) → Task 7 (full wiring) → Task 8 (e2e test) → Task 9 (validation)
```

**Key patterns from ~/dev/superpowers:**

| Pattern | Source Skill | RustyCode Implementation |
|---------|-------------|--------------------------|
| Two-stage review | subagent-driven-development | Spec compliance THEN quality review |
| Status protocol | subagent-driven-development | TaskStatus enum (4 variants) |
| Context isolation | subagent-driven-development | Ralph Loop curates what context to pass |
| Model selection by complexity | subagent-driven-development | Classifier → cheap/capable model routing |
| 3-strike escalation | systematic-debugging | Auto-escalate after 3 stagnant iterations |
| Evidence-before-claims | verification-before-completion | Behavioral completion detection |
| No placeholders | writing-plans | Every task has complete code |
| Fresh subagent per task | subagent-driven-development | Each loop iteration with clean context |

**Key patterns from ~/dev/oh-my-openagent:**

| Pattern | Source | RustyCode Implementation |
|---------|--------|--------------------------|
| Ralph Loop | Ralph Loop plugin | Core iterate-until-done loop |
| Adversarial verification | Oracle review | Two-stage verification |
| Continuation injection | Todo Continuation Enforcer | next_prompt() for PartialProgress |
| Done signal detection | `<promise>DONE</promise>` | Behavioral evidence detection |
