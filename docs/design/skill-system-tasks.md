# Skill Management System — Implementation Tasks

> Companion to: docs/design/skill-system-spec.md, docs/design/skill-system-requirements.md

## Phase 1: Foundation — Skill Registry & Metadata ✅ COMPLETE

**Goal**: Extended skill metadata, multi-source loading, deduplication.
**Delivers**: FR-1.1 through FR-1.8
**Depends on**: Nothing (can start immediately)

### Task 1.1: Define Core Types ✅

**Files**: `crates/rustycode-skill/src/types.rs` (new)

Create the core type definitions:

- [x] `SkillId` — newtype wrapper around String
- [x] `SkillSource` enum — `Bundled | Managed | User | Project | Mcp | Plugin | Dynamic`
- [x] `ActivationMode` enum — `AlwaysOn | Conditional | Semantic | UserInvoked | ModelDecided`
- [x] `ActivationSpec` struct — mode, paths, allowed_tools, effort, model_override, user_invocable, model_invocable
- [x] `ExecutionContext` enum — `Inline | Fork { agent: Option<String> }`
- [x] `ProcedureKind` enum — `Prompt(String) | Pipeline(Pipeline)`
- [x] `Pipeline` struct — stages, parallel_groups
- [x] `PipelineStage` struct — id, name, instructions, role, allowed_tools, success_criteria, human_checkpoint
- [x] `QualityGrade` enum — A, B, C, D, F
- [x] `LifecycleState` enum — Discovered, Active, Watch, Demoted, Archived
- [x] `SkillQuality` struct — telemetry_score, graph_score, intake_score, routing_score, grade
- [x] `SkillDefinition` struct — identity (name, description, when_to_use, version, source), activation, procedure, quality, lifecycle
- [x] Derive `Serialize, Deserialize, Clone, Debug` on all types
- [x] Add `rustycode-skill/src/types.rs` to `lib.rs` as public module

### Task 1.2: Extend Metadata Parser ✅

**Files**: `crates/rustycode-skill/src/metadata.rs`

Extend the existing frontmatter parser to extract new fields:

- [x] Parse `when_to_use` from frontmatter → String
- [x] Parse `allowed-tools` (YAML list) → Vec<String>
- [x] Parse `paths` (YAML list) → Vec<glob::Pattern>
- [x] Parse `context` → ExecutionContext (inline/fork)
- [x] Parse `agent` → Option<String>
- [x] Parse `effort` → Option<EffortLevel>
- [x] Parse `disable-model-invocation` → bool
- [x] Parse `user-invocable` → bool
- [x] Parse `argument-hint` → Option<String>
- [x] Parse `arguments` → Vec<String>
- [x] Parse `version` → Option<String>
- [x] Return `ActivationSpec` and `ProcedureSpec` from parsed frontmatter

### Task 1.3: Implement SkillRegistry ✅

**Files**: `crates/rustycode-skill/src/registry.rs` (new)

Replace `SkillManager` with a multi-source registry:

- [x] `SkillRegistry::new()` — create empty registry
- [x] `load_from_dir(path, source)` — scan directory for SKILL.md files, parse into `Skill` objects
- [x] `register_bundled()` — register compiled-in skills
- [x] `register_mcp()` — register MCP-provided skills
- [x] `deduplicate()` — resolve symlinks, keep highest-priority on collision
- [x] `get_all()` → Vec<&Skill>
- [x] `get_by_name(name)` → Option<&Skill>
- [x] `get_active()` → Vec<&Skill>
- [x] `get_conditional()` → Vec<&Skill>
- [x] `estimate_frontmatter_tokens(skill)` → u32
- [x] Add `glob` crate to dependencies

### Task 1.4: Add Skill Events to EventBus ✅

**Files**: `crates/rustycode-bus/src/events.rs`

Add new event types:

- [x] `SkillActivatedEvent` — `{ skill_name, trigger, activated_at }`
- [x] `SkillDeactivatedEvent` — `{ skill_name, reason, duration, tokens_used }`
- [x] `SkillSuggestedEvent` — `{ skill_name, reason, score, unmatched_signals }`
- [x] `SkillQualityAssessedEvent` — `{ skill_name, grade, scores }`
- [x] All events implement `Event` trait with proper `event_type()` strings: `"skill.activated"`, etc.

### Task 1.5: Tests ✅

- [x] Unit test: frontmatter parsing with all new fields
- [x] Unit test: frontmatter parsing with minimal fields (defaults)
- [x] Unit test: multi-source loading with priority override
- [x] Unit test: deduplication by realpath
- [x] Unit test: conditional vs unconditional skill separation
- [x] Unit test: token estimation from frontmatter

---

## Phase 2: Activation & Discovery ✅ COMPLETE

**Goal**: Context-aware skill activation, conditional skills, dynamic walk-up discovery.
**Delivers**: FR-2.1 through FR-2.9, FR-3.1 through FR-3.4, FR-7.1 through FR-7.4
**Depends on**: Phase 1 complete

### Task 2.1: Implement ActivationManager ✅

**Files**: `crates/rustycode-skill/src/activation.rs` (new)

- [x] `ActivationManager::new(registry, total_budget)` — create with reference to registry
- [x] `evaluate_for_context()` → Vec<SkillRecommendation> — score all skills against context
- [x] `activate(name, trigger)` → Result<ActiveSkill> — load skill body, emit event
- [x] `deactivate(name, reason)` → Result<()> — remove from active, emit event, reclaim budget
- [x] `is_active(name)` → bool
- [x] `get_active_skills()` → Vec<&ActiveSkill>
- [x] `allocate_budget(recommendations)` → Vec<ActiveSkill> — proportional allocation
- [x] `evict_lowest_priority()` — remove lowest-scoring active skill
- [x] `ActiveSkill` struct: skill_id, trigger, activated_at, token_budget, tokens_used, last_accessed
- [x] `SkillRecommendation` struct: skill_id, score, activation_mode, estimated_tokens

### Task 2.2: Implement Conditional Skills ✅

**Files**: `crates/rustycode-skill/src/activation.rs`

- [x] Store conditional skills separately (not in active set)
- [x] `activate_for_paths(file_paths)` → Vec<String> — match paths against conditional skill globs
- [x] Use `glob::Pattern` for matching
- [x] Promote matched conditional skills to active set
- [x] Track activated conditional skill names for session persistence
- [x] Emit `SkillActivatedEvent` with trigger `Conditional`

### Task 2.3: Implement Dynamic Walk-Up Discovery ✅

**Files**: `crates/rustycode-skill/src/discovery.rs` (new)

- [x] `discover_for_paths(file_paths)` → Vec<String> — walk up from file paths looking for `.rustycode/skills/`
- [x] Sort discovered directories deepest-first (closer overrides)
- [x] Load skills from discovered directories
- [x] Register in SkillRegistry with `Dynamic` source
- [x] Memoize checked paths (Set<String>) to avoid re-scanning

### Task 2.4: Implement CapabilityCurator Agent Skeleton ✅

**Files**: `crates/rustycode-skill/src/curator.rs` (new)

- [x] `CapabilityCurator` struct holding: registry, activation_manager, intent_log
- [x] `observe_tool_execution()` — extract intent signals from tool invocations
- [x] `extract_signals(tool_name, tool_input)` → Vec<String> — keyword extraction
- [x] `detect_unmatched_signals()` → Vec<String> — signals not covered by loaded skills
- [x] `suggest_for_unmatched(signals)` — produce suggestions with evidence
- [x] Curator failures are caught and logged, never block main conversation

### Task 2.5: Tests ✅

- [x] Unit test: activation modes
- [x] Unit test: conditional skill path matching
- [x] Unit test: budget allocation and eviction
- [x] Unit test: walk-up discovery path traversal
- [x] Unit test: intent signal extraction

---

## Phase 3: Quality & Lifecycle ✅ COMPLETE

**Goal**: Quality scoring, lifecycle state machine, file watching.
**Delivers**: FR-4.1 through FR-4.6, FR-5.1 through FR-5.7, FR-3.5, FR-3.6
**Depends on**: Phase 2 complete

### Task 3.1: Implement Quality Scoring ✅

**Files**: `crates/rustycode-skill/src/quality.rs` (new)

- [x] `QualityScorer` struct
- [x] `compute_score(skill, telemetry, graph, intake, routing)` → SkillQuality
- [x] Weighted total: telemetry×0.40 + graph×0.25 + intake×0.20 + routing×0.15
- [x] `grade_from_score(score)` → QualityGrade
- [x] Persist to `~/.rustycode/skill-quality/<slug>.json`
- [x] Load existing scores on startup
- [x] Incremental update: only recompute skills used this session

### Task 3.2: Implement Lifecycle FSM ✅

**Files**: `crates/rustycode-skill/src/lifecycle.rs` (new)

- [x] Hand-rolled FSM (no external crate needed)
- [x] States: Discovered, Active, Watch, Demoted, Archived (+ Retired, Error)
- [x] Events: QualityScored, SessionUsed, UserPromoted, UserArchived, AgeThreshold, ConfirmedDelete, etc.
- [x] Valid transition table per spec
- [x] Persist lifecycle state to sidecar JSON
- [x] `observe_score(slug, grade)` — idempotent, safe to call from hooks

### Task 3.3: Implement File Watching ✅

**Files**: `crates/rustycode-skill/src/watcher.rs` (new)

- [x] Use `notify` crate for filesystem watching
- [x] Watch user skills dir, project skills dir
- [x] Debounce events (configurable window)
- [x] On change: reload into registry
- [x] Graceful error handling (missing dirs, permission errors)

### Task 3.4: Curator Proactive Mode ✅

**Files**: `crates/rustycode-skill/src/curator.rs`

- [x] `on_session_end()` — run quality scoring for skills used this session
- [x] `run_lifecycle_transitions()` — check grades, apply FSM transitions
- [x] MIN_EVIDENCE threshold for suggestion gating

### Task 3.5: Tests ✅

- [x] Unit test: quality scoring computation
- [x] Unit test: grade assignment thresholds
- [x] Unit test: lifecycle state transitions (all valid paths)
- [x] Unit test: lifecycle rejects invalid transitions
- [x] Unit test: watcher debouncing

---

## Phase 4: Pipelines & Orchestration ✅ COMPLETE

**Goal**: Multi-stage skill procedures with agent assignment.
**Delivers**: FR-6.1 through FR-6.6
**Depends on**: Phase 2 complete

### Task 4.1: Parse Pipeline from SKILL.md Body ✅

**Files**: `crates/rustycode-skill/src/procedure.rs` (new)

- [x] Parse `### N. Stage Name` headings → PipelineStage
- [x] Parse per-stage annotations: Execution, Allowed tools, Success criteria, Human checkpoint
- [x] Detect parallel stages (sub-numbers: 3a, 3b)
- [x] Fallback: if no stages detected, wrap entire body as `ProcedureKind::Prompt`

### Task 4.2: Pipeline → Task DAG Conversion ✅

**Files**: `crates/rustycode-skill/src/procedure.rs`

- [x] Convert Pipeline to TaskDag with dependency tracking
- [x] Each stage becomes a DAG node with dependencies on prior stages
- [x] Parallel stages become independent nodes
- [x] Topological ordering for execution

### Task 4.3: Tests ✅

- [x] Unit test: pipeline parsing from markdown
- [x] Unit test: parallel stage detection
- [x] Unit test: DAG construction from pipeline
- [x] Unit test: fallback to Instruction when no stages detected

---

## Phase 5: Self-Improvement & Skillify ✅ COMPLETE

**Goal**: Skills that evolve, sessions that become skills.
**Delivers**: FR-7.7, FR-7.8, FR-8.1 through FR-8.5, FR-9.1 through FR-9.4
**Depends on**: Phase 3 complete

### Task 5.1: Skill Improvement Hook ✅

**Files**: `crates/rustycode-skill/src/improvement.rs` (new)

- [x] Turn-based improvement hook (configurable interval)
- [x] Analyze corrections to propose SkillUpdateProposals
- [x] Proposals target specific fields (when_to_use, allowed_tools, steps, description)
- [x] `rewrite_preserving_frontmatter()` — rewrite SKILL.md keeping original frontmatter

### Task 5.2: Capability Graph ✅

**Files**: `crates/rustycode-skill/src/graph.rs` (new)

- [x] `CapabilityGraph` wrapping `petgraph::stable_graph::StableGraph`
- [x] Node types: Skill, Tool, Agent
- [x] Edge types: Uses, Requires, AssignedTo, RelatedTo, ConflictsWith
- [x] `add_skill()`, `add_tool()`, `add_agent()`, `add_edge()`
- [x] `walk_from(skill, max_hops)` → Vec<(SkillId, f32)> — related skills with score decay
- [x] `centrality_score(skill)` → f32 — degree centrality
- [x] Persist graph to JSON (serialize/deserialize roundtrip)

### Task 5.3: Skillify (Session Capture) ✅

**Files**: `crates/rustycode-skill/src/bundled.rs`

- [x] `SkillifyBuilder` — builder pattern for generating SKILL.md
- [x] Generates frontmatter + markdown body with steps, arguments, tools
- [x] `write_skill_to_dir()` — writes to disk at user/project skills dir
- [x] Full roundtrip: build → generate_markdown → write → file exists

### Task 5.4: Tests ✅

- [x] Unit test: skill improvement proposal extraction
- [x] Unit test: SKILL.md rewrite preserving frontmatter
- [x] Unit test: capability graph construction and traversal
- [x] Unit test: graph centrality computation
- [x] Unit test: graph serialize/deserialize roundtrip
- [x] Unit test: SkillifyBuilder minimal, full, and markdown generation

---

## Migration Strategy

Phase 1 introduces new types alongside the existing `SkillManager` and `ProgressiveLoader`. The migration path:

1. **Phase 1**: New types coexist with old. `SkillManager` still works. New `SkillRegistry` is additive.
2. **Phase 2**: `ActivationManager` replaces `ProgressiveLoader`'s `find_relevant()`. Old API still works but delegates to new.
3. **Phase 3**: Quality and lifecycle are entirely new — no conflict.
4. **Phase 4**: Pipeline replaces `WorkflowEngine`. Both exist until pipeline is proven.
5. **Phase 5**: New features — no migration needed.

At each phase, run:
```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
