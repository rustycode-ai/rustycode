# Skill Management System — Architectural Specification

> Status: DRAFT
> Date: 2026-04-20
> Authors: Nat + Sisyphus
> Motivated by: ctx (~/dev/ctx), OpenCode (~/dev/opencode), Cursor, Windsurf, Cline, Aider

## 1. Problem Statement

RustyCode has a skill system (`rustycode-skill`) that supports basic discovery and keyword matching, but lacks:

1. **Context-aware activation** — skills activate only via manual/keyword match, not by file paths, semantic similarity, or model decision
2. **Quality feedback** — no telemetry on whether skills are useful, no lifecycle management
3. **Dynamic discovery** — skills are only found at startup, not discovered at runtime
4. **Structured procedures** — skills are flat prompts, not multi-stage pipelines with agent assignment
5. **Self-improvement** — skills don't evolve based on usage patterns

## 2. Design Principles

1. **Skill is the pivot** — one abstraction bridges user intent to tool execution across all layers
2. **Pipeline is a skill property** — `ProcedureKind::Prompt` (simple) vs `ProcedureKind::Pipeline` (complex), not a separate concept
3. **Orchestration strategies are composable** — Team, Ensemble, Workflow are variants of pipeline orchestration, not separate agent types
4. **The Curator is an agent** — uses the same EventBus, TaskScheduler, and communication patterns as every other agent
5. **MCP extends naturally** — new MCP servers add tools, which the Curator matches to skills
6. **Quality flows backward** — task completion → telemetry → quality score → lifecycle state → skill improvement
7. **Progressive enhancement** — Phase 1 (registry + activation) is independently useful; later phases add capability without breaking earlier ones

## 3. The Five-Layer Model

```
Layer 4: WORK MANAGEMENT    Task → Todo → Scheduler
Layer 3: ORCHESTRATION      Pipeline → Team → Ensemble → Workflow
Layer 2: AGENCY             Agent = Identity + Role + Skills + Tools + State
Layer 1: PROCEDURE          Skill = When + What + How + Who + Verify
Layer 0: CAPABILITY         Tool (builtin) → MCP (external) → Plugin (dynamic)
```

Cross-cutting: **Capability Curator** (meta-agent operating across all layers)

## 4. Skill Definition

### 4.1 Skill Identity

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Unique identifier, used as `/name` command |
| `description` | `String` | One-line summary for model routing |
| `when_to_use` | `String` | Detailed trigger description — the model reads this to decide invocation |
| `version` | `String` | Semver for skill versioning |
| `source` | `SkillSource` | Where this skill was loaded from |

### 4.2 Activation Specification

| Field | Type | Description |
|-------|------|-------------|
| `mode` | `ActivationMode` | How this skill becomes active |
| `paths` | `Option<Vec<glob::Pattern>>` | Glob patterns for conditional activation |
| `allowed_tools` | `Vec<String>` | Tools this skill pre-authorizes |
| `effort` | `Option<EffortLevel>` | Low/Medium/High/max |
| `model_override` | `Option<String>` | Use a specific model for this skill |
| `user_invocable` | `bool` | Show in `/` autocomplete |
| `model_invocable` | `bool` | Allow model to invoke automatically |

#### Activation Modes

| Mode | Trigger | Example |
|------|---------|---------|
| `AlwaysOn` | Always loaded into context | Project coding conventions |
| `Conditional` | File path matches glob pattern | React skills activate when touching `.tsx` files |
| `Semantic` | Embedding similarity exceeds threshold | General auth skill for any auth-adjacent task |
| `UserInvoked` | User types `/skill-name` | `/skillify` to capture a workflow |
| `ModelDecided` | Model reads `when_to_use` and decides | Complex multi-step procedures |

### 4.3 Procedure Specification

| Field | Type | Description |
|-------|------|-------------|
| `kind` | `ProcedureKind` | Prompt (simple) or Pipeline (staged) |
| `context` | `ExecutionContext` | Inline (current session) or Fork (sub-agent) |
| `success_criteria` | `Vec<String>` | What "done" looks like for this skill |

#### ProcedureKind

```rust
pub enum ProcedureKind {
    /// Simple: single prompt, model figures out steps
    Prompt(String),
    /// Structured: explicit stages with dependencies, agent roles, and verification
    Pipeline(Pipeline),
}
```

#### Pipeline

```rust
pub struct Pipeline {
    pub stages: Vec<PipelineStage>,
    pub parallel_groups: Vec<Vec<StageId>>,
}

pub struct PipelineStage {
    pub id: StageId,
    pub name: String,
    pub instructions: String,
    pub role: AgentRole,
    pub allowed_tools: Vec<String>,
    pub success_criteria: Vec<String>,
    pub human_checkpoint: bool,
}
```

### 4.4 Quality Scoring

Four-signal model (adapted from ctx):

| Signal | Weight | Source |
|--------|--------|--------|
| Telemetry | 40% | Load count, retention rate, session duration |
| Graph centrality | 25% | Degree/betweenness in capability graph |
| Intake quality | 20% | Structural checks, duplicate detection |
| Routing accuracy | 15% | Was the skill useful when activated? |

Grades: A (0.80-1.00), B (0.60-0.79), C (0.40-0.59), D (0.20-0.39), F (0.00-0.19)

### 4.5 Lifecycle State Machine

```
Discovered ──quality≥C──► Active ──grade=C──► Watch
                           ▲                   │
                           │              grade=D×2
                     promote                  │
                           │                  ▼
                        Active ◄────── Demoted
                                     14_days │ ▲
                                        ▼    │ promote
                                     Archived
                                     60_days │ ▲ confirmed_delete
                                        ▼    │ restore
                                     Deleted
```

Transitions:
- `Discovered → Active`: First quality score ≥ C
- `Active → Watch`: Grade drops to C
- `Watch → Active`: Grade recovers to ≥ B
- `Watch → Demoted`: 2 consecutive D grades
- `Active → Demoted`: 2 consecutive D grades
- `Demoted → Active`: Manual promote or grade recovers
- `Demoted → Archived`: 14 days in demoted state
- `Archived → Active`: Manual restore
- `Archived → Deleted`: 60 days archived + user confirmation

## 5. Skill Sources & Loading

Five sources, loaded in parallel at startup:

| Source | Path | Priority |
|--------|------|----------|
| Bundled | Compiled into binary | 1 (lowest) |
| Managed | Policy-enforced (enterprise) | 2 |
| User | `~/.rustycode/skills/` | 3 |
| Project | `.rustycode/skills/` | 4 |
| MCP | From MCP server connections | 5 |
| Plugin | From loaded plugins | 6 |
| Dynamic | Walk-up discovery at runtime | 7 (highest) |

Higher-priority sources override lower when names collide.

### Dynamic Walk-Up Discovery

When a tool touches a file, walk up from that file to the project root looking for `.rustycode/skills/` directories. Deeper paths override shallower ones. Gitignored directories are skipped.

### Conditional Activation

Skills with `paths` frontmatter start in a "conditional" state. When a tool touches a file matching the glob pattern, the skill is promoted to "active" for the remainder of the session.

## 6. Context Budget

- Total skill context budget: 25,000 tokens (configurable)
- Per-skill allocation: proportional to relevance score, minimum 2,000 tokens
- Budget reclamation: when budget exceeded, evict lowest-scoring active skill (FIFO for ties)
- Progressive loading: metadata always loaded (~50 tokens/skill), full content loaded on activation

## 7. Capability Curator Agent

### 7.1 Role

The Curator is a `SpecialistAgent` registered in the `AgentRegistry` with role `Curator`. It has three operational modes:

### 7.2 Passive Mode (always on)

- Subscribes to `tool.executed`, `context.assembled`, `session.started`, `session.completed`
- Extracts intent signals from tool names and tool inputs
- Maintains an intent log (in-memory, session-scoped)
- Detects unmatched signals (signals not covered by loaded skills)
- Emits `skill.suggested` events when unmatched signals accumulate

### 7.3 Reactive Mode (on context change)

- When `context.assembled` fires, score all known skills against the context
- Activate conditional skills (path match)
- Discover walk-up skill directories
- Allocate context budget
- Emit `skill.activated` / `skill.deactivated` events

### 7.4 Proactive Mode (session-end + periodic)

- Quality scoring: compute 4-signal scores for skills used this session
- Lifecycle management: transition FSM states based on grades
- Skill improvement: LLM-driven analysis of user corrections → propose SKILL.md updates
- Behavior mining: detect co-invocation patterns → suggest toolbox bundles
- Graph maintenance: update capability graph with new edges

### 7.4 Curator Tools

The Curator has access to a restricted set of tools:
- `Read`, `Glob`, `Grep` — scan filesystem for skills
- `Write`, `Edit` — update skill manifests and SKILL.md files
- `skill_scan` — custom tool for repo stack analysis
- `skill_quality_check` — custom tool for quality scoring
- LLM query tool — for skill improvement analysis

## 8. File Formats

### 8.1 Skill Directory Structure

```
.rustycode/skills/
├── auth-implementation/
│   └── SKILL.md              # Frontmatter + markdown body
├── tdd-workflow/
│   ├── SKILL.md
│   └── references/           # Optional reference files
│       └── red-green-refactor.md
├── _demoted/                 # Demoted skills (lifecycle)
│   └── old-skill/
│       └── SKILL.md
└── _archive/                 # Archived skills (lifecycle)
    └── deprecated-skill/
        └── SKILL.md
```

### 8.2 SKILL.md Format

```markdown
---
name: auth-implementation
description: Implement authentication for REST API endpoints
when_to_use: Use when the user wants to add authentication, authorization, or security middleware to a REST API. Examples: 'add auth', 'JWT authentication', 'secure the API', 'add login'.
version: "1.0.0"
allowed-tools:
  - Read
  - Write
  - Edit
  - Bash(cargo:*)
  - Grep
  - Glob
paths:
  - "src/api/**"
  - "src/auth/**"
context: fork
agent: general
effort: high
user-invocable: true
disable-model-invocation: false
---

# Auth Implementation

## Inputs
- `$auth_type`: JWT, session, or API key
- `$routes`: Which routes to protect

## Goal
Implement authentication middleware and route guards for the specified API endpoints.

## Steps

### 1. Design Auth Architecture
Analyze existing code structure and design the auth middleware contract.
- **Execution**: Direct
- **Success criteria**: Interface documentation produced, types defined

### 2. Implement Middleware
Write the auth middleware, route handlers, and tests.
- **Execution**: Task agent
- **Allowed tools**: Read, Write, Edit, Bash(cargo:test)
- **Success criteria**: `cargo test` passes, middleware compiles

### 3. Security Review
Run security review on the implementation.
- **Execution**: Teammate
- **Success criteria**: No HIGH/CRITICAL findings

### 4. Integrate
Wire middleware into the router, run full test suite.
- **Execution**: Direct
- **Success criteria**: All tests pass, CI green
```

### 8.3 Skill Manifest

```json
{
  "active": [
    { "name": "auth-implementation", "source": "project", "activated_at": "2026-04-20T12:00:00Z", "trigger": "model_decided" }
  ],
  "conditional": [
    { "name": "react-patterns", "paths": ["src/components/**/*.tsx"] }
  ],
  "suggested": [
    { "name": "security-review", "reason": "Graph neighbor of auth-implementation", "score": 0.72 }
  ],
  "budget": {
    "total": 25000,
    "allocated": 20000,
    "used": 12450
  }
}
```

### 8.4 Event Log

```jsonl
{"ts":"2026-04-20T12:00:00Z","event":"skill.activated","skill":"auth-implementation","trigger":"model_decided","session_id":"ses_abc123"}
{"ts":"2026-04-20T12:05:00Z","event":"skill.content_loaded","skill":"auth-implementation","tokens":3200,"session_id":"ses_abc123"}
{"ts":"2026-04-20T12:30:00Z","event":"skill.deactivated","skill":"auth-implementation","reason":"session_end","retained":true,"session_id":"ses_abc123"}
```

### 8.5 Quality Sidecar

```json
{
  "slug": "auth-implementation",
  "grade": "A",
  "scores": {
    "telemetry": 0.85,
    "graph": 0.70,
    "intake": 0.90,
    "routing": 0.80
  },
  "weighted_total": 0.82,
  "load_count": 15,
  "retention_rate": 0.87,
  "last_scored": "2026-04-20T12:00:00Z"
}
```

## 9. Crate Organization

```
rustycode-skill/
├── src/
│   ├── lib.rs              # Public API re-exports
│   ├── registry.rs         # SkillRegistry (multi-source loading, dedup)
│   ├── activation.rs       # ActivationManager (modes, conditional, budget)
│   ├── metadata.rs         # SkillMetadata parsing from YAML frontmatter
│   ├── procedure.rs        # ProcedureKind, Pipeline, PipelineStage
│   ├── quality.rs          # 4-signal quality scoring
│   ├── lifecycle.rs        # FSM: Discovered → Active → Watch → Demoted → Archived
│   ├── discovery.rs        # Dynamic walk-up discovery + conditional activation
│   ├── curator.rs          # CapabilityCurator agent (subscribes to events)
│   ├── graph.rs            # CapabilityGraph (petgraph wrapper)
│   ├── events.rs           # Skill-related event types
│   ├── manifest.rs         # Skill manifest read/write
│   ├── improvement.rs      # LLM-driven skill improvement
│   └── bundled.rs          # Bundled skill registration

rustycode-bus/
├── src/
│   ├── events.rs           # ADD: SkillActivated, SkillDeactivated, SkillSuggested, SkillQualityAssessed

rustycode-orchestra/
├── src/
│   └── ...                 # REGISTER: Curator as SpecialistAgent
```

## 10. Crate Dependencies

| Crate | Purpose | Version |
|-------|---------|---------|
| `petgraph` | Capability graph | 0.8 |
| `serde-saphyr` | YAML frontmatter parsing | 0.0.8 |
| `glob` | Path pattern matching | 0.3 |
| `notify` | File watching | 6.x |
| Existing: `rustycode-bus` | Event bus | workspace |
| Existing: `rustycode-vector-memory` | Semantic matching | workspace |
| Existing: `rustycode-memory` | Confidence scoring | workspace |

## 11. Relationship to Existing Crates

| Existing Component | Relationship |
|---|---|
| `SkillManager` | Replaced by `SkillRegistry` (multi-source) |
| `ProgressiveLoader` | Absorbed into `ActivationManager` (metadata-first + content-on-demand) |
| `WorkflowEngine` | Replaced by `ProcedureKind::Pipeline` |
| `EventBus` | Extended with skill events |
| `MemoryEntry` (confidence) | Reused for quality score storage |
| `VectorMemory` | Reused for semantic skill matching |
| `AgentLifecycleManager` | Curator agent follows existing lifecycle |
| `TaskScheduler` | Curator tasks scheduled through existing system |

## 12. Non-Goals (Explicitly Out of Scope)

- Skill marketplace / remote registry (future)
- Cross-tool SKILL.md standard compliance (future)
- Skill hot-reload via network (future)
- Multi-user skill sharing (future)
- Skill dependency resolution / install chains (future)
