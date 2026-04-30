# AST Implementation Guide

**Module**: `rustycode-orchestration::ast`
**Spec**: Adaptive Structured Thinking (AST) v0.4
**Status**: Implemented (Tasks T1-T8, T10-T15 complete, v0.4 action plan complete)

---

## What is AST?

Adaptive Structured Thinking (AST) is a 6-phase pipeline for agentic task execution. It adapts the level of planning structure based on task complexity:

| Complexity | Phases | Research | Milestones | Expansion |
|---|---|---|---|---|
| TRIVIAL | 3 | Skipped | 1 | All at once |
| MODERATE | 6 | Quick | 3-7 | All at once |
| COMPLEX | 6 | Full | 3-7 | Rolling wave (2 at a time) |

**Pipeline**: `CLASSIFY -> RESEARCH -> SKELETON -> EXPAND -> EXECUTE -> VERIFY`

---

## Quick Start

```rust
use rustycode_orchestration::ast::{AstPipeline, AstConfig, VerificationStatus};

// Create a pipeline for a workspace
let mut pipeline = AstPipeline::new(workspace_path.into());

// Run end-to-end
let status = pipeline.run("Fix typo in README.md")?;
assert_eq!(status, VerificationStatus::Pass);
```

---

## How Tasks Are Classified

The `TaskClassifier` uses a hybrid approach:

1. **Rule-based heuristics** for known patterns
2. **Word count fallback** for ambiguous cases

| Signal | Classification |
|---|---|
| "typo", "rename", "fix typo", "bump version" | TRIVIAL |
| "add test", "fix bug", "update", "extend" | MODERATE |
| "implement", "refactor", "architect", "migrate" | COMPLEX |
| <= 10 words (no signal) | TRIVIAL |
| 11-25 words (no signal) | MODERATE |
| 26+ words (no signal) | COMPLEX |

Override classification at the `CLASSIFY_COMPLETE` hook point.

---

## Phase Details

### Phase 0: CLASSIFY

```rust
let assessment = pipeline.classify("Add JWT auth with refresh tokens")?;
// assessment.complexity == ComplexityLevel::Complex
// assessment.route == PhaseRoute::RollingWave
// assessment.success_criteria == [Test criterion, Build criterion, ...]
```

### Phase 1: RESEARCH

Produces a `ContextBrief` with:
- `relevant_files`: Files likely involved
- `patterns_found`: Code patterns detected
- `dependencies`: External deps needed
- `risks`: Potential failure modes
- `constraints`: Known limitations

Skipped for TRIVIAL tasks (empty brief produced).

### Phase 2: SKELETON

Builds a `MilestoneSkeleton` with 1-7 milestones, each with:
- Description and deliverable
- Dependency chain (which milestones must complete first)

TRIVIAL tasks collapse to a single milestone.

### Phase 3a: EXPAND

Expands milestones into `ExecutionSegment`s with atomic steps:
- `action`: What to do
- `file_targets`: Files that will change
- `expected_command`: Command to run
- `verification_command`: How to verify success
- `is_risky`: Whether the step is dangerous
- `recovery_notes`: Fallback plan

### Phase 3b: EXECUTE

Runs steps sequentially with the injected `StepRunner`. On failure:
1. Stop execution at the failed step
2. Produce a `RecoveryAction` with diagnosis
3. Retry with replanned steps (up to `MAX_RETRIES = 2`)
4. If still failing, mark milestone as failed

### Phase 4: VERIFY

Checks collected `StepEvidence` against `SuccessCriterion`s:
- **Pass**: All criteria met
- **Partial**: No failures but some criteria ambiguous
- **Fail**: At least one criterion failed

---

## Recovery Flow

When a step fails:

```
Step fails
  -> Diagnosis (exit code, stderr, file targets)
  -> Research topics suggested
  -> Replanned steps generated
  -> Retry (up to MAX_RETRIES)
     -> If retry succeeds: continue
     -> If retry fails: milestone marked failed

Systemic failure (>= 2 milestones failed)
  -> Consultant escalation
  -> Proposed reclassification (e.g., TRIVIAL -> MODERATE)
  -> Strategy change proposed after 3+ failures
```

---

## Construction Crew Pattern

The crew assigns roles to pipeline phases:

| Role | Phase | Artifact |
|---|---|---|
| Scout | RESEARCH | ContextBrief |
| Architect | SKELETON + EXPAND | MilestoneSkeleton, ExecutionSegment |
| Builder | EXECUTE | StepEvidence |
| Inspector | VERIFY | VerificationReport |
| Consultant | Escalation | ConsultationReport |

### Using the Crew Orchestrator

```rust
use rustycode_orchestration::ast::handlers::CrewOrchestrator;

let mut orchestrator = CrewOrchestrator::new();
let (report, handler_results) = orchestrator.execute(&assessment, &workspace);
```

For custom runners:

```rust
orchestrator = CrewOrchestrator::with_runner(my_runner);
```

---

## Custom Step Runner

Implement `StepRunner` for real command execution:

```rust
use rustycode_orchestration::ast::StepRunner;
use rustycode_orchestration::ast::ExecutionStep;

struct RealRunner;

impl StepRunner for RealRunner {
    fn run(&self, step: &ExecutionStep, step_index: usize) -> StepEvidence {
        // Execute step.expected_command via shell
        // Capture exit code, stdout, stderr
        // Detect changed files
        StepEvidence {
            step_index,
            command_run: step.expected_command.clone(),
            exit_code: /* actual exit code */,
            stdout_summary: /* truncated stdout */,
            stderr_summary: /* truncated stderr */,
            changed_files: /* detected changes */,
            verification_passed: /* run verification_command if present */,
        }
    }
}

let pipeline = AstPipeline::with_runner(config, workspace, RealRunner);
```

---

## Hook Points

Hook into phase transitions for orchestration:

| Hook Point | When | Use Case |
|---|---|---|
| `CLASSIFY_COMPLETE` | After classification | Override complexity |
| `RESEARCH_COMPLETE` | After research | Redirect research |
| `SKELETON_COMPLETE` | After skeleton | Add/remove milestones |
| `EXPAND_COMPLETE` | After expansion | Modify steps |
| `EXECUTE_STEP_PRE` | Before each step | Skip/modify steps |
| `EXECUTE_STEP_POST` | After each step | Capture custom evidence |
| `VERIFY_COMPLETE` | After verification | Override pass/fail |

### Using the Hook Bridge

```rust
use rustycode_orchestration::ast::{AstHookBridge, AstHookPoint};

let bridge = AstHookBridge::new();
bridge.register(AstHookPoint::ClassifyComplete, |payload| {
    // Inspect or modify assessment
    AstHookResponse::Continue
});
```

---

## Persistent State

### Markdown Ledger (human-readable)

Written to `.ast/ledger.md` at each phase transition:

```markdown
# Task: Fix typo in README.md

## Assessment
- Complexity: TRIVIAL
- Goal: Fix typo in README.md
- Current phase: COMPLETE
```

### SQLite ProgressStore (machine-readable)

Tables: `tasks`, `milestones`, `milestone_dependencies`, `events`, `artifacts`, `subagent_runs`

Common queries:

```sql
-- Get task status
SELECT * FROM tasks WHERE task_id = ?;

-- Get milestone history
SELECT * FROM milestones WHERE task_id = ? ORDER BY milestone_index;

-- Get all events
SELECT * FROM events WHERE task_id = ? ORDER BY timestamp;

-- Get artifacts for a milestone
SELECT * FROM artifacts WHERE task_id = ? AND milestone_id = ?;
```

---

## Context Loading

The `ContextLoader` assembles prompts with priority-based eviction:

| Priority | Content | Token Budget |
|---|---|---|
| Critical | Assessment, success criteria, active milestones | Always included |
| High | Current segment, unresolved blockers | Evicted last |
| Medium | Recent evidence, phase context | Evicted under pressure |
| Low | Background context, history | Evicted first |

```rust
use rustycode_orchestration::ast::{ContextLoader, WorkingSet};

let loader = ContextLoader::new(8000); // 8k token budget
let assembled = loader.assemble(&working_set);
// assembled.sections contains priority-ordered content
// assembled.token_estimate approximates usage
```

---

## Shared Memory

Three memory interfaces for inter-agent communication:

| Interface | Backing | Use Case |
|---|---|---|
| `AgentMemory` | Key-value store | General agent state |
| `ProgressStoreMemory` | SQLite ProgressStore | Task/milestone queries |
| `LedgerMemory` | Markdown ledger | Decision/open question access |

---

## Best Practices

### Writing Good Task Descriptions

- Be specific: "Add JWT authentication with RS256 signing" > "Add auth"
- Include verification: "Build and run the test suite" > "Make it work"
- Mention files if known: "Fix the race condition in session.rs"

### When to Use Crew vs Single-Agent

- **Single-agent** (AstPipeline): TRIVIAL and MODERATE tasks
- **Crew** (CrewOrchestrator): COMPLEX tasks needing role separation

### Tuning Parameters

| Parameter | Default | When to Adjust |
|---|---|---|
| `rolling_wave_batch_size` | 2 | Increase for well-understood complex tasks |
| `max_recovery_retries` | 2 | Increase for flaky environments |
| `skip_research_for_trivial` | true | Set false for research-heavy codebases |
| `target_window_tokens` | 8000 | Increase for large codebases |

---

## Troubleshooting

### Task Classification is Wrong

Register a `CLASSIFY_COMPLETE` hook to override:

```rust
bridge.register(AstHookPoint::ClassifyComplete, |payload| {
    // Override to COMPLEX if needed
    AstHookResponse::Continue
});
```

### Research Missed Important Files

The research phase uses file extension and pattern matching. To redirect:
1. Register a `RESEARCH_COMPLETE` hook
2. Add files to `ContextBrief.relevant_files`
3. The skeleton builder will include them

### Step Fails Repeatedly

1. Check `RecoveryAction.diagnosis` for the failure reason
2. `RecoveryAction.research_needed` suggests investigation topics
3. After 3 failures, the Consultant proposes strategy change
4. Systemic failures (>= 2 milestones) trigger reclassification

### Verification Fails

1. Check `CriterionResult.evidence` for each criterion
2. If "No evidence found for verification command", the step's `command_run` didn't match
3. Ensure `ExecutionStep.expected_command` matches the criterion's `verification_command`

---

## Architecture

```
ast/
  mod.rs              Module registry, public re-exports
  types.rs            Core types (AstPhase, TaskAssessment, etc.)
  classifier.rs       Phase 0: CLASSIFY
  research.rs         Phase 1: RESEARCH
  skeleton.rs         Phase 2: SKELETON
  expander.rs         Phase 3a: EXPAND
  executor.rs         Phase 3b: EXECUTE
  verifier.rs         Phase 4: VERIFY
  pipeline.rs         Pipeline controller (ties all phases)
  handlers.rs         Crew role handlers + CrewOrchestrator
  crew.rs             Crew roles, dispatcher, handoff protocol
  bedd.rs             BEDD funnel (Brainstorm-Evaluate-Drop-Expand)
  ledger.rs           Markdown task ledger
  progress_store.rs   SQLite persistent state
  hooks.rs            Hook bridge and phase controller
  context_loader.rs   Smart prompt assembly
  shared_memory.rs    Inter-agent memory interfaces
  recovery.rs         Milestone recovery and failure classification
  prompt.rs           System prompt and output parsing
  tool_adapter.rs     Cross-harness tool-call normalization
```

---

## Tool-Call Adapter (v0.4)

The `ToolAdapter` trait normalizes tool names and arguments between AST prompt format and different execution harnesses:

```rust
use rustycode_orchestration::ast::{ToolHarness, get_adapter};

// Claude Code (identity — no translation)
let adapter = get_adapter(ToolHarness::ClaudeCode);
assert_eq!(adapter.normalize_tool_name("Write"), "Write");

// RustyCode (agent_tool_* naming)
let adapter = get_adapter(ToolHarness::RustyCode);
assert_eq!(adapter.normalize_tool_name("Write"), "agent_tool_write");
```

Set the harness in `AstConfig`:

```rust
let config = AstConfig {
    harness: ToolHarness::RustyCode,
    ..Default::default()
};
```

| Harness | Tool Name Mapping | Arg Mapping |
|---|---|---|
| ClaudeCode | Identity | Identity |
| RustyCode | `Write` → `agent_tool_write` | `file_path` → `path`, `content` → `data` |
| GeminiCli | `Write` → `gemini_Write` | Identity |
| Codex | `Write` → `codex_Write` | Identity |

---

## Execution Boundary (v0.4)

The system prompt enforces a hard gate after SKELETON:

> After SKELETON, you MUST begin writing files. Analysis paralysis is a known failure mode.

This prevents the model from spending its entire context budget on reasoning without producing code. The boundary is enforced in `AST_SYSTEM_PROMPT` (prompt.rs).

---

## Consultant Auto-Trigger (v0.4)

When a single milestone accumulates 3+ consecutive recovery retries, the pipeline automatically flags it for Consultant escalation:

```rust
// In pipeline config
let config = AstConfig {
    max_recovery_retries: 2,  // Per-step retry limit
    ..Default::default()
};

// After execute, check escalation
if pipeline.has_consultant_escalation() {
    let escalated = pipeline.escalated_milestones();
    // escalated contains milestone IDs that hit the threshold
}
```

Threshold constant: `CONSULTANT_TRIGGER_THRESHOLD = 3`

---

## Crew Handoff Completeness (v0.4)

Each `CrewHandoff` carries a `requirements_checklist` that ensures no requirements are dropped during role transfers:

```rust
let handoff = CrewHandoff {
    // ... standard fields ...
    requirements_checklist: vec![
        "Handle multi-line TOML strings".into(),
        "Support dotted keys in nested tables".into(),
    ],
};

// Check which requirements weren't acknowledged
let unacked = handoff.unacknowledged_requirements(&["Handle multi-line TOML strings".into()]);
assert_eq!(unacked, vec!["Support dotted keys in nested tables"]);
```

---

## Developer Integration Guide

### Integrating AST into a Custom Orchestrator

```rust
use rustycode_orchestration::ast::{
    AstPipeline, AstConfig, AstPhase, ToolHarness,
    AstHookBridge, AstHookPoint, AstHookResponse,
};

// 1. Create pipeline with harness config
let config = AstConfig {
    harness: ToolHarness::RustyCode,
    ledger_dir: workspace.join(".ast"),
    ..Default::default()
};
let mut pipeline = AstPipeline::with_runner(config, workspace, my_runner);

// 2. Register hooks for orchestration
let bridge = AstHookBridge::new();
bridge.register(AstHookPoint::ClassifyComplete, |payload| {
    // Inspect classification, optionally override
    AstHookResponse::Continue
});
bridge.register(AstHookPoint::ExecuteStepPre, |payload| {
    // Skip dangerous steps, modify commands
    AstHookResponse::Continue
});

// 3. Run phased (manual control)
let assessment = pipeline.classify(task_description)?;
pipeline.research()?;
pipeline.build_skeleton()?;

loop {
    pipeline.expand()?;
    if pipeline.snapshot().current_phase == AstPhase::Verify { break; }
    pipeline.execute()?;
}

let report = pipeline.verify()?;
```

### Wiring into the Event Bus

```rust
use rustycode_bus::EventBus;

let bus = EventBus::new();
bridge.register(AstHookPoint::VerifyComplete, move |payload| {
    bus.emit(AstEvent::TaskVerified {
        task_id: payload.task_id,
        status: payload.report.map(|r| r.overall),
    });
    AstHookResponse::Continue
});
```

### Querying the Progress Store

```rust
use rustycode_orchestration::ast::ProgressStore;

let store = ProgressStore::open(&db_path)?;
store.create_task(&task_record)?;
let milestones = store.get_milestones(&task_id)?;
let events = store.get_events(&task_id)?;
let artifacts = store.get_artifacts_for_milestone(&task_id, milestone_id)?;
```

### Reading the Ledger

```rust
use rustycode_orchestration::ast::TaskLedger;

let ledger = TaskLedger::read_from_file(&ledger_path)?;
// ledger has: title, assessment, brief, milestones, segments, decisions, questions
```

---

## FAQ

**Should classification be rule-based or model-based?**
Hybrid. The `TaskClassifier` uses rules first, with hook-based model override at `CLASSIFY_COMPLETE`.

**Should research be mandatory?**
Yes for MODERATE+. TRIVIAL skips research (empty brief).

**Skeleton maximum: 5 or 7 milestones?**
7 (per spec). The `SkeletonBuilder` generates 3-7 for MODERATE/COMPLEX.

**Headlight batch size for complex?**
2 milestones. Configurable via `AstConfig.rolling_wave_batch_size`.

**Can recovery widen scope?**
Only if systemic (>= 2 milestones failed). The Consultant proposes reclassification.

**Can orchestrator override classification?**
Yes, at the `CLASSIFY_COMPLETE` hook point.

**Does AST work across different model harnesses?**
Yes, via the `ToolAdapter` system (v0.4). Set `AstConfig.harness` to match your execution environment.

**What happens when the model gets stuck planning?**
The execution boundary (v0.4) enforces a hard gate: after SKELETON, the model must begin writing files. This prevents analysis paralysis.

**When does the Consultant engage?**
Automatically after 3 consecutive recovery retries on the same milestone (`CONSULTANT_TRIGGER_THRESHOLD`). Also triggered on systemic failures (>= 2 milestones failed).

**How do crew handoffs avoid dropping requirements?**
Each `CrewHandoff` carries a `requirements_checklist` (v0.4). Use `unacknowledged_requirements()` to detect gaps.

---

## CLI Commands

AST is available as a standalone CLI subcommand:

### Run a task through the AST pipeline

```bash
# Default harness (claude-code)
rustycode ast run --task "Add JWT auth with refresh tokens"

# Specific harness
rustycode ast run --task "Implement error handling" --harness rustycode

# Available harnesses: claude-code, rustycode, gemini, codex
```

Output shows:
- Task classification (complexity + route)
- Phase-by-phase progress
- Milestone completion status
- Verification results (✓/✗ per criterion)
- Ledger path

### Check AST status

```bash
# Show pipeline status for current workspace
rustycode ast status
```

Displays:
- Tasks recorded in the workspace's `.ast/` directory
- Current phase per task
- Milestone status (pending/active/done/failed)

### View the task ledger

```bash
# Show the human-readable markdown ledger
rustycode ast ledger
```

Displays the `.ast/LEDGER.md` file with full task history, decisions, and evidence.

### Integration with other commands

AST is also triggered automatically by:
- `rustycode agent new --task "..." --use-ast` — agent mode with AST pipeline
- `rustycode orchestra auto --use-ast` — autonomous mode with AST for complex tasks
- `rustycode auto "..."` — routes COMPLEX/MODERATE tasks to AST automatically when configured

---

## TUI Phase Progress

When running in TUI mode, AST phase progress is visible in the status bar:

```
⠋ AST [✓CLS][✓RES][●SKL][○EXP][○EXE][○VER] 2/5 milestones 3.2s
```

Indicators:
- ✓ = phase completed successfully
- ● = current phase (animated spinner)
- ○ = pending phase
- ✗ = phase failed

The status bar updates in real-time as phases complete and milestones progress.

---

## OrchestrationPipeline Integration

AST is integrated directly into the `OrchestrationPipeline` via the `structured_thinking` tool. When the LLM calls this tool, the `StepOrchestrator` intercepts the call and runs the full AST pipeline automatically.

For explicit, direct AST execution (bypassing the tiered pipeline):

```rust
use rustycode_orchestration::execute_with_ast;
use rustycode_orchestration::ast::ToolHarness;

let result = execute_with_ast(
    "Implement a new authentication system",
    workspace_path,
    ToolHarness::ClaudeCode
)?;
```

When using `OrchestrationPipeline`, you don't need to manually choose AST — the orchestrator detects when structured thinking is required and routes the task through the embedded AST pipeline. TRIVIAL tasks execute directly through the Musician tier; MODERATE/COMPLEX tasks engage AST automatically via the `structured_thinking` tool invocation.

