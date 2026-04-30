# TUI Orchestration Harness — Parallel Implementation Plan

> **Goal:** Build a reasoning harness that makes the LLM smarter at planning and finishing complex tasks.

**Architecture:** Three independent work streams that converge:
1. **Stream A (Quality & Strategy):** Quality detection + strategy selection logic
2. **Stream B (Structured Thinking):** Tool schema + storage/retrieval for multi-phase tasks
3. **Stream C (TUI Integration):** Wire everything into the TUI message flow
4. **Stream D (Validation):** Terminal Bench testing (can start after A+B complete)

**Timeline:** 1 week (7 calendar days), 3-4 people working in parallel.

**Dependencies:** Minimal cross-blocking. All streams can start immediately. Convergence point: Day 4-5 when A+B complete, C can wire them together.

---

## Stream A: Quality Detection & Strategy Selection

**Owner:** Person A  
**Duration:** 2-3 days  
**Deliverable:** Two modules that can be tested independently

### File Structure
- Create: `crates/rustycode-orchestration/src/quality_detector.rs`
- Create: `crates/rustycode-orchestration/src/strategy_selector.rs`
- Modify: `crates/rustycode-orchestration/src/types.rs` (add types)
- Modify: `crates/rustycode-orchestration/src/lib.rs` (export modules)

### Task A1: Quality Detector (1 day)

**What:** Heuristic scoring of LLM response quality without pre-testing.

**Acceptance Criteria:**
- [ ] Scoring function runs on any text response
- [ ] Returns score 0-7 (5.0+ = high quality)
- [ ] Evaluates: specificity (algorithm names), depth (explanations), completeness (edge cases), uncertainty (caveats)
- [ ] 5+ unit tests covering high/low/medium quality responses
- [ ] Compiles and passes clippy
- [ ] Committed with message: `feat: add quality detector`

**Detailed Steps:**

```bash
# Step 1: Create quality_detector.rs with this structure:
crates/rustycode-orchestration/src/quality_detector.rs
  - pub struct QualityDetector
  - pub fn evaluate_response_quality(&self, response: &str) -> QualityScore
  - pub struct QualityScore { specificity, depth, completeness, uncertainty, total }
  - Helper functions: score_specificity(), score_depth(), score_completeness(), score_uncertainty()
  - #[cfg(test)] mod tests with 5 tests

# Step 2: Add to types.rs
pub struct QualityScore {
    pub specificity: f64,      // 0-5: named algorithms vs vague
    pub depth: f64,            // 0-5: explanations vs assertions
    pub completeness: f64,     // 0-5: edge cases covered
    pub uncertainty: f64,      // 0-2: caveats stated
    pub total: f64,            // 0-7: combined
}

impl QualityScore {
    pub fn is_high_quality(&self) -> bool { self.total >= 5.0 }
}

# Step 3: Export from lib.rs
pub mod quality_detector;
pub use quality_detector::QualityDetector;
pub use types::QualityScore;

# Step 4: Run tests
cargo test -p rustycode-orchestration quality_detector --lib

# Step 5: Commit
git add crates/rustycode-orchestration/src/quality_detector.rs \
        crates/rustycode-orchestration/src/types.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add quality detector with heuristic scoring"
```

**Test Examples:**

```rust
#[test]
fn test_high_quality_response() {
    let detector = QualityDetector::new();
    let response = "I'll use DFS because O(n+e) complexity is better than BFS O(n) space. \
                    However, if the maze has cycles, we need visited tracking.";
    let score = detector.evaluate_response_quality(response);
    assert!(score.is_high_quality());
    assert!(score.specificity > 4.0);
    assert!(score.uncertainty > 0.0);
}

#[test]
fn test_low_quality_response() {
    let detector = QualityDetector::new();
    let response = "I'll try to solve this problem carefully.";
    let score = detector.evaluate_response_quality(response);
    assert!(!score.is_high_quality());
}
```

---

### Task A2: Strategy Selector (1-1.5 days)

**What:** Pick reasoning strategy based on task complexity + response quality.

**Acceptance Criteria:**
- [ ] Decision function: `select(complexity: f64, quality: QualityScore, confidence: u32) -> Strategy`
- [ ] Returns one of: DirectExecution, QuickSelfEval, SequentialThinking, PhasedOrchestration
- [ ] Tests show correct routing for 4+ scenarios
- [ ] Compiles and passes clippy
- [ ] Committed with message: `feat: add strategy selector`

**Detailed Steps:**

```bash
# Step 1: Add Strategy enum to types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Strategy {
    DirectExecution,        // Simple task, high quality → execute immediately
    QuickSelfEval,          // Moderate task, good quality → ask LLM to self-eval, then execute
    SequentialThinking,     // Moderate complexity, medium confidence → step-by-step prompts
    PhasedOrchestration,    // High complexity, low confidence → phase 1-3 detailed, 4+ outlined
}

# Step 2: Create strategy_selector.rs
pub struct StrategySelector;

impl StrategySelector {
    pub fn select(complexity: f64, quality: QualityScore, confidence: u32) -> Strategy {
        // Decision tree:
        // complexity < 2.0 && quality.total >= 5.0 && confidence >= 85 → DirectExecution
        // complexity < 3.0 && quality.total >= 4.0 && confidence >= 75 → QuickSelfEval
        // complexity < 4.0 → SequentialThinking
        // else → PhasedOrchestration
    }
}

# Step 3: Write 4+ unit tests
#[test]
fn test_select_direct_for_simple_high_quality() { ... }
#[test]
fn test_select_sequential_for_moderate() { ... }
#[test]
fn test_select_phased_for_complex() { ... }
#[test]
fn test_select_quick_eval_for_moderate_quality() { ... }

# Step 4: Run tests
cargo test -p rustycode-orchestration strategy_selector --lib

# Step 5: Commit
git add crates/rustycode-orchestration/src/strategy_selector.rs \
        crates/rustycode-orchestration/src/types.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add strategy selector with complexity-based routing"
```

---

### Task A3: Complexity Detection Helper (0.5 days)

**What:** Helper function to classify task complexity from user message.

**Acceptance Criteria:**
- [ ] Function: `detect_complexity(text: &str) -> f64` returns 0.0-5.0
- [ ] Keyword-based heuristic (explore, refactor, design, fix, etc.)
- [ ] 3+ unit tests
- [ ] Compiles

**Detailed Steps:**

```bash
# Add to strategy_selector.rs or types.rs
pub fn detect_complexity(text: &str) -> f64 {
    let lower = text.to_lowercase();
    let keywords = [
        ("explore", 4.0),
        ("design", 3.5),
        ("refactor", 2.5),
        ("implement", 2.0),
        ("fix", 1.0),
    ];
    
    keywords.iter()
        .filter(|(kw, _)| lower.contains(kw))
        .map(|(_, score)| score)
        .max()
        .copied()
        .unwrap_or(1.5)
}

# Test
#[test]
fn test_detect_explore_high_complexity() {
    assert!(detect_complexity("explore the maze") > 3.5);
}

#[test]
fn test_detect_fix_low_complexity() {
    assert!(detect_complexity("fix this typo") < 2.0);
}
```

---

### Validation Checkpoint (End of Day 2)

Before handing off to Stream C:

```bash
# Verify everything compiles and tests pass
cargo test -p rustycode-orchestration --lib 2>&1 | grep -E "(test result|passed)"
cargo clippy -p rustycode-orchestration --all-targets -- -D warnings

# Expected: All tests pass, zero warnings
```

**Definition of Done for Stream A:**
- [ ] Quality detector and strategy selector compile without warnings
- [ ] 8+ unit tests pass (5 quality + 4 strategy + 3 complexity)
- [ ] All code committed to main branch
- [ ] Person A notifies Stream C: "Ready for integration"

---

## Stream B: Structured Thinking Tool & Storage

**Owner:** Person B  
**Duration:** 2-3 days  
**Deliverable:** Tool schema + storage layer that can be tested independently

### File Structure
- Create: `crates/rustycode-orchestration/src/structured_thinking_tool.rs`
- Create: `crates/rustycode-orchestration/src/reasoning_store.rs`
- Modify: `crates/rustycode-orchestration/src/types.rs` (add StructuredThought type)
- Modify: `crates/rustycode-orchestration/src/lib.rs` (export modules)

### Task B1: Structured Thinking Tool Schema (1 day)

**What:** JSON schema that LLM calls repeatedly to record structured reasoning.

**Acceptance Criteria:**
- [ ] OpenAI-compatible tool definition (JSON)
- [ ] Fields: thought, phase, type (decision/constraint/validation/learning/hypothesis), confidence, next_thought_needed, references, metadata
- [ ] System prompt guidance for using the tool
- [ ] 2+ unit tests for schema validity
- [ ] Compiles

**Detailed Steps:**

```bash
# Step 1: Add StructuredThought type to types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredThought {
    pub thought: String,
    pub phase: u32,
    pub thought_type: ThoughtType,
    pub references: Vec<String>,
    pub confidence: u32,           // 0-100
    pub next_thought_needed: bool,
    pub branch_id: Option<String>,
    pub metadata: ThoughtMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ThoughtType {
    Decision,
    Constraint,
    Validation,
    Learning,
    Hypothesis,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThoughtMetadata {
    pub algorithm_choice: Option<String>,
    pub rationale: Option<String>,
    pub alternatives_rejected: Option<Vec<String>>,
    pub validation_points: Option<Vec<String>>,
}

# Step 2: Create structured_thinking_tool.rs
pub struct StructuredThinkingToolSchema;

impl StructuredThinkingToolSchema {
    pub fn openai_tool_definition() -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "structured_thinking",
                "description": "Record a step of your structured reasoning...",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "thought": { "type": "string" },
                        "phase": { "type": "integer" },
                        "type": { "type": "string", "enum": ["decision", "constraint", "validation", "learning", "hypothesis"] },
                        "confidence": { "type": "integer", "minimum": 0, "maximum": 100 },
                        "next_thought_needed": { "type": "boolean" },
                        "references": { "type": "array", "items": { "type": "string" } },
                        "metadata": { "type": "object" }
                    },
                    "required": ["thought", "phase", "type", "confidence", "next_thought_needed"]
                }
            }
        })
    }

    pub fn system_prompt_guidance() -> &'static str {
        "When asked to solve complex problems, use the structured_thinking tool..."
    }
}

# Step 3: Write unit tests
#[test]
fn test_schema_is_valid_json() {
    let schema = StructuredThinkingToolSchema::openai_tool_definition();
    assert_eq!(schema["type"], "function");
}

#[test]
fn test_serialize_structured_thought() {
    let thought = StructuredThought {
        thought: "Analysis of DFS vs BFS".to_string(),
        phase: 1,
        thought_type: ThoughtType::Decision,
        ..Default::default()
    };
    let json = serde_json::to_string(&thought).unwrap();
    assert!(json.contains("\"phase\":1"));
}

# Step 4: Run tests
cargo test -p rustycode-orchestration structured_thinking_tool --lib

# Step 5: Commit
git add crates/rustycode-orchestration/src/structured_thinking_tool.rs \
        crates/rustycode-orchestration/src/types.rs \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add structured thinking tool schema for LLM reasoning"
```

---

### Task B2: Reasoning Storage & Retrieval (1-1.5 days)

**What:** Save/retrieve structured thinking across task phases using file-based storage.

**Acceptance Criteria:**
- [ ] Store function: save thought to phase-indexed directory
- [ ] Retrieve function: get thought by task_id, phase, index
- [ ] Phase summary function: compress phase learnings for next phase context
- [ ] 4+ unit tests with temp directories
- [ ] Compiles and passes clippy

**Detailed Steps:**

```bash
# Step 1: Create reasoning_store.rs
pub struct ReasoningStore {
    base_path: PathBuf,
}

impl ReasoningStore {
    pub fn new(base_path: PathBuf) -> Self { ... }

    pub fn store_thought(&self, task_id: &str, phase: u32, thought: &StructuredThought) -> Result<()> {
        // Create task_id/phase_N/thoughts.jsonl
        // Append thought as JSON line
    }

    pub fn get_phase_thoughts(&self, task_id: &str, phase: u32) -> Result<Vec<StructuredThought>> {
        // Read and parse all thoughts in phase
    }

    pub fn get_phase_summary(&self, task_id: &str, phase: u32) -> Result<PhaseSummary> {
        // Aggregate decisions, learnings, constraints
    }

    pub fn get_context_for_next_phase(&self, task_id: &str, phase: u32) -> Result<serde_json::Value> {
        // Compressed summary: key decisions, learnings, avg confidence
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseSummary {
    pub phase: u32,
    pub thought_count: usize,
    pub average_confidence: f64,
    pub decisions_made: Vec<String>,
    pub key_learnings: Vec<String>,
}

# Step 2: Write unit tests with tempfile
#[test]
fn test_store_and_retrieve_thoughts() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let store = ReasoningStore::new(temp_dir.path().to_path_buf());
    
    let thought = StructuredThought { ... };
    store.store_thought("task1", 1, &thought).unwrap();
    
    let retrieved = store.get_thought("task1", 1, 0).unwrap();
    assert_eq!(retrieved.thought, thought.thought);
}

#[test]
fn test_get_phase_summary() { ... }

#[test]
fn test_get_context_for_next_phase() { ... }

# Step 3: Add tempfile to Cargo.toml dev-dependencies
# (if not present: cargo add --dev tempfile)

# Step 4: Run tests
cargo test -p rustycode-orchestration reasoning_store --lib

# Step 5: Commit
git add crates/rustycode-orchestration/src/reasoning_store.rs \
        crates/rustycode-orchestration/Cargo.toml \
        crates/rustycode-orchestration/src/lib.rs
git commit -m "feat: add reasoning storage and phase-based retrieval"
```

---

### Validation Checkpoint (End of Day 2)

```bash
# Verify everything compiles
cargo test -p rustycode-orchestration --lib 2>&1 | grep -E "(test result|passed)"
cargo clippy -p rustycode-orchestration --all-targets -- -D warnings

# Expected: All tests pass, zero warnings
```

**Definition of Done for Stream B:**
- [ ] Structured thinking tool and reasoning store compile without warnings
- [ ] 6+ unit tests pass
- [ ] All code committed to main branch
- [ ] Person B notifies Stream C: "Ready for integration"

---

## Stream C: TUI Integration

**Owner:** Person C  
**Duration:** 2-3 days (starts Day 3, after A+B complete)  
**Deliverable:** Wired message flow that routes complex tasks through orchestration

### Prerequisites
- ✅ Stream A complete (QualityDetector, StrategySelector, complexity detection)
- ✅ Stream B complete (StructuredThinkingToolSchema, ReasoningStore)

### File Structure
- Modify: `crates/rustycode-tui/src/app/service_integration.rs`
- Create: `crates/rustycode-tui/src/app/orchestration_integration.rs` (new)
- Modify: `crates/rustycode-tui/src/app/event_loop.rs` (optional: handle events)

### Task C1: Add Complexity Routing to ServiceManager (1 day)

**What:** Detect task complexity and route messages to either direct stream or orchestration path.

**Acceptance Criteria:**
- [ ] `send_message()` now calls complexity detector
- [ ] If simple (complexity < 3.0): direct streaming path (unchanged)
- [ ] If complex (complexity >= 3.0): orchestration path (new)
- [ ] Both paths work end-to-end
- [ ] Integration test: verify routing decision
- [ ] Compiles and passes clippy

**Detailed Steps:**

```bash
# Step 1: Create orchestration_integration.rs
pub struct OrchestrationIntegration {
    quality_detector: QualityDetector,
    strategy_selector: StrategySelector,
    reasoning_store: ReasoningStore,
}

impl OrchestrationIntegration {
    pub fn new(storage_path: PathBuf) -> Self { ... }

    pub fn analyze_message(&self, text: &str) -> (f64, Strategy) {
        let complexity = detect_complexity(text);
        let quality = QualityScore::default();  // Unknown at start
        let confidence = 50;
        let strategy = StrategySelector::select(complexity, quality, confidence);
        (complexity, strategy)
    }
}

# Step 2: Modify service_integration.rs send_message()
// Before: send_message(text) → stream_llm_response(text)
// After:
fn send_message(&mut self, text: &str) {
    let (complexity, strategy) = self.orchestration.analyze_message(text);
    
    if complexity < 3.0 {
        // Direct streaming (existing path)
        self.send_message_with_history(text);
    } else {
        // Orchestration path (new)
        self.send_message_with_orchestration(text, strategy);
    }
}

fn send_message_with_orchestration(&mut self, text: &str, strategy: Strategy) {
    // For now: same as direct, but with strategy logged
    // Later: use strategy to choose prompting approach
    self.send_message_with_history(text);
}

# Step 3: Write integration test
#[test]
fn test_simple_message_uses_direct_path() {
    let mut service_manager = ServiceManager::new(...);
    service_manager.send_message("what is fn main?");
    // Verify message was sent directly (no orchestration overhead)
}

#[test]
fn test_complex_message_uses_orchestration() {
    let mut service_manager = ServiceManager::new(...);
    service_manager.send_message("refactor the entire auth module");
    // Verify strategy was selected and message was routed
}

# Step 4: Run integration tests
cargo test -p rustycode-tui orchestration --lib

# Step 5: Commit
git add crates/rustycode-tui/src/app/orchestration_integration.rs \
        crates/rustycode-tui/src/app/service_integration.rs \
        crates/rustycode-tui/src/app/mod.rs
git commit -m "feat: add complexity routing to TUI service manager"
```

---

### Task C2: Wire Structured Thinking Tool into LLM Calls (1 day)

**What:** When strategy is PhasedOrchestration or SequentialThinking, include the structured thinking tool in LLM system prompt.

**Acceptance Criteria:**
- [ ] Detect when structured thinking should be enabled
- [ ] Add tool definition to LLM system prompt
- [ ] LLM can call the tool and we capture tool calls
- [ ] Captured thoughts are stored via ReasoningStore
- [ ] Compiles and passes clippy

**Detailed Steps:**

```bash
# Step 1: Extend send_message_with_orchestration()
fn send_message_with_orchestration(&mut self, text: &str, strategy: Strategy) {
    match strategy {
        Strategy::PhasedOrchestration | Strategy::SequentialThinking => {
            // Include structured thinking tool
            let mut system_prompt = self.get_base_system_prompt().to_string();
            system_prompt.push_str("\n\n");
            system_prompt.push_str(StructuredThinkingToolSchema::system_prompt_guidance());
            
            self.send_message_with_tools(text, &system_prompt, vec![
                StructuredThinkingToolSchema::openai_tool_definition()
            ]);
        }
        _ => {
            // Direct execution, no tool
            self.send_message_with_history(text);
        }
    }
}

# Step 2: Handle tool calls
// When LLM calls structured_thinking tool:
// 1. Parse the JSON
// 2. Create StructuredThought
// 3. Store via reasoning_store.store_thought(task_id, phase, thought)
// 4. Return response to LLM to continue

fn handle_structured_thought_tool_call(
    &self,
    task_id: &str,
    tool_input: serde_json::Value
) -> Result<()> {
    let thought: StructuredThought = serde_json::from_value(tool_input)?;
    self.orchestration.reasoning_store.store_thought(task_id, thought.phase, &thought)?;
    Ok(())
}

# Step 3: Integration test
#[test]
fn test_structured_thinking_tool_called_and_stored() {
    // Simulate LLM response with tool call
    // Verify thought is stored in reasoning_store
}

# Step 4: Run tests
cargo test -p rustycode-tui orchestration --lib

# Step 5: Commit
git add crates/rustycode-tui/src/app/orchestration_integration.rs \
        crates/rustycode-tui/src/app/service_integration.rs
git commit -m "feat: wire structured thinking tool into orchestration path"
```

---

### Task C3: Multi-Phase Task Continuation (0.5 days)

**What:** When a task moves to phase 2, retrieve phase 1 summary and pass it as context.

**Acceptance Criteria:**
- [ ] Phase 1 complete → ReasoningStore.get_context_for_next_phase()
- [ ] Prepend context to phase 2 prompt
- [ ] LLM sees: "In phase 1 we decided X, learned Y. Now for phase 2..."
- [ ] Compiles

**Detailed Steps:**

```bash
# Step 1: Add to orchestration_integration.rs
pub fn get_phase_context(&self, task_id: &str, completed_phase: u32) -> Option<String> {
    if let Ok(context) = self.reasoning_store.get_context_for_next_phase(task_id, completed_phase) {
        // Format as markdown for LLM
        Some(format!(
            "## Phase {} Summary\n\nKey decisions:\n{}\n\nLearnings:\n{}\n\n---\n\nNow for phase {}:",
            completed_phase,
            context["key_decisions"].as_array()
                .map(|v| v.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
            context["discovered_patterns"].as_array()
                .map(|v| v.iter().map(|s| format!("- {}", s)).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default(),
            completed_phase + 1
        ))
    } else {
        None
    }
}

# Step 2: Use in send_message_with_orchestration()
if completed_phase > 0 {
    if let Some(context) = self.get_phase_context(task_id, completed_phase) {
        prompt = format!("{}\n\n{}", context, prompt);
    }
}

# Step 3: Compile and test
cargo build -p rustycode-tui

# Step 4: Commit
git add crates/rustycode-tui/src/app/orchestration_integration.rs
git commit -m "feat: add multi-phase context retrieval"
```

---

### Validation Checkpoint (End of Day 4)

```bash
# Full workspace test
cargo test --workspace --lib 2>&1 | tail -30
cargo clippy --workspace --all-targets -- -D warnings

# Expected: All tests pass, zero warnings, both TUI and orchestration modules working
```

**Definition of Done for Stream C:**
- [ ] Complexity routing works (simple messages bypass orchestration)
- [ ] Complex messages trigger orchestration path with strategy selection
- [ ] Structured thinking tool is included in system prompt when needed
- [ ] Tool calls are captured and stored
- [ ] Phase context is retrieved and passed to next phase
- [ ] All code committed to main branch
- [ ] Person C notifies Stream D: "Ready for validation"

---

## Stream D: Terminal Bench Validation (Parallel with C, Days 3-5)

**Owner:** Person D (optional, can overlap with C)  
**Duration:** 1-2 days  
**Deliverable:** Validation report showing the harness works

### Prerequisites
- ✅ Streams A, B, C complete

### Task D1: Set Up Test Harness (0.5 days)

**What:** Create a test wrapper to run Terminal Bench tasks with orchestration enabled.

**Acceptance Criteria:**
- [ ] Can run N Terminal Bench tasks sequentially
- [ ] Captures: task name, complexity, strategy chosen, iterations, success/fail
- [ ] Saves results to JSON
- [ ] Compiles

**Detailed Steps:**

```bash
# Create tests/terminal_bench_orchestration.rs
#[tokio::test]
async fn test_terminal_bench_simple_tasks() {
    let tasks = vec![
        ("Fix typo in README", "simple"),
        ("Implement bubble sort", "moderate"),
        ("Explore maze solution", "complex"),
    ];
    
    for (task_desc, expected_complexity) in tasks {
        let (actual_complexity, strategy) = analyze_task(task_desc);
        assert_eq!(expected_complexity, categorize_complexity(actual_complexity));
        
        // Run task and capture result
        let result = run_task_with_orchestration(task_desc).await;
        println!("{}: {:?}", task_desc, result);
    }
}

# Run
cargo test --test terminal_bench_orchestration -- --nocapture
```

---

### Task D2: Validate on 20-30 Terminal Bench Tasks (1.5 days)

**What:** Run the harness on real tasks and collect metrics.

**Acceptance Criteria:**
- [ ] Run at least 20 tasks across all complexity levels
- [ ] Collect: task, complexity score, strategy chosen, success/fail
- [ ] Success rate >= 70% (most complex tasks should succeed)
- [ ] Strategy selection is reasonable (complex tasks use Phased, not Direct)
- [ ] Generate a report with findings

**Detailed Steps:**

```bash
# Run Terminal Bench subset
cargo test --test terminal_bench_orchestration -- --nocapture 2>&1 | tee validation_results.log

# Parse results and generate report
# Expected output:
# Task: "Explore 10x10 maze"
# Complexity: 4.2
# Strategy: PhasedOrchestration
# Result: Success ✓
# Thoughts captured: 8
# Phases executed: 2

# Summary:
# Total tasks: 25
# Success rate: 80%
# Avg complexity: 3.1
# Strategy distribution:
#   DirectExecution: 3
#   SequentialThinking: 8
#   PhasedOrchestration: 14
```

---

### Task D3: Create Validation Report (0.5 days)

**What:** Document what works, what doesn't, findings.

**Acceptance Criteria:**
- [ ] Report: `docs/superpowers/validation/ORCHESTRATION-HARNESS-VALIDATION-2026-04-25.md`
- [ ] Includes: summary, test matrix, success rates, strategy effectiveness, key learnings

**Detailed Steps:**

```bash
# Create docs/superpowers/validation/ORCHESTRATION-HARNESS-VALIDATION-2026-04-25.md

# Contents:
# # Orchestration Harness Validation Report
#
# **Date:** 2026-04-25
# **Tasks Run:** 25
# **Success Rate:** 80%
#
# ## Test Matrix
# | Task | Complexity | Strategy | Result |
# |------|-----------|----------|--------|
# | Maze 10x10 | 4.2 | PhasedOrchestration | ✓ |
# | Fix typo | 1.0 | DirectExecution | ✓ |
# ...
#
# ## Findings
# 1. Complexity detection works well for exploration tasks
# 2. Phased orchestration successful on complex tasks (90% success)
# 3. Sequential thinking useful for moderate tasks (75% success)
# 4. Structured thinking tool captures good decision rationale
#
# ## Recommendations
# - Increase max_iterations in PhasedOrchestration from 10 to 15
# - Tune confidence threshold for QuickSelfEval (currently too conservative)
# - Add explicit "planning phase" prompt for phase 1
#
# ## What's Next
# - Adaptive learning per model variant
# - Modal visualization in TUI
# - Graceful degradation fallbacks
```

---

### Definition of Done for Stream D:
- [ ] 20+ Terminal Bench tasks executed
- [ ] Results captured in JSON
- [ ] Validation report written
- [ ] Success rate >= 70% (target)
- [ ] Key learnings documented for phase 2 improvements

---

## Convergence & Final Validation (Day 5-6)

Once all streams complete:

```bash
# Final integration test
cargo test --workspace --lib 2>&1 | grep -E "(test result|passed)"
cargo clippy --workspace --all-targets -- -D warnings

# Expected: 30+ tests passing, zero warnings
```

### Acceptance Criteria for Full Harness:
- [ ] Quality detector + strategy selector: working (8+ tests)
- [ ] Structured thinking + storage: working (6+ tests)
- [ ] TUI integration: working (routing verified)
- [ ] Terminal Bench validation: 70%+ success rate
- [ ] All code committed with clear messages
- [ ] Validation report finalized

---

## Handoff Checklist

**When all streams complete:**

- [ ] All code merged to `main`
- [ ] Validation report in `/docs/superpowers/validation/`
- [ ] README updated with "How to Use" section
- [ ] Brief team sync on findings and next phase options

**Next Phase Options (after validation):**
1. Adaptive learning per model variant
2. Modal visualization of reasoning graph
3. Graceful degradation fallbacks
4. Performance tuning on larger task sets

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Stream C blocking on Streams A+B | Start C design/boilerplate on Day 2 while A+B finish |
| Compilation failures during integration | Daily check: `cargo build --workspace` should always pass |
| Test flakiness | Use inline tests only, avoid flaky network mocks |
| Storage path conflicts in tests | Each test uses `tempfile::TempDir` for isolation |

---

## Success Criteria

**By end of Day 6:**
- ✅ Harness is in production (merged to main)
- ✅ 70%+ Terminal Bench success rate
- ✅ 3-4 people can work independently with minimal blocking
- ✅ Clear validation report for phase 2 decisions
- ✅ Codebase ready for adding learning + visualization layers
