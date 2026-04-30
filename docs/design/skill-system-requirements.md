# Skill Management System — Requirements

> Companion to: docs/design/skill-system-spec.md

## FR — Functional Requirements

### FR-1: Skill Registry

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-1.1 | Load skills from multiple sources in parallel (bundled, managed, user, project, MCP, plugin) | Must | 1 |
| FR-1.2 | Deduplicate skills by canonical file path (resolve symlinks) | Must | 1 |
| FR-1.3 | Higher-priority source wins on name collision | Must | 1 |
| FR-1.4 | Parse SKILL.md YAML frontmatter into typed SkillMetadata | Must | 1 |
| FR-1.5 | Support legacy flat `.md` file format in commands/ directory | Should | 1 |
| FR-1.6 | Cache skill metadata with configurable TTL | Must | 1 |
| FR-1.7 | Estimate token count from frontmatter alone (without loading body) | Must | 1 |
| FR-1.8 | Provide async iterator over all known skills | Must | 1 |

### FR-2: Skill Activation

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-2.1 | Support 5 activation modes: AlwaysOn, Conditional (paths), Semantic, UserInvoked, ModelDecided | Must | 2 |
| FR-2.2 | Conditional skills start inactive, activate when tool touches matching file path | Must | 2 |
| FR-2.3 | Model-decided activation reads `when_to_use` field and task context | Must | 2 |
| FR-2.4 | Skill body loaded lazily (only on activation), not at startup | Must | 2 |
| FR-2.5 | Track active skills in a manifest with activation trigger and timestamp | Must | 2 |
| FR-2.6 | Allocate context budget across active skills proportional to relevance score | Should | 2 |
| FR-2.7 | Evict lowest-scoring skill when budget exceeded | Should | 2 |
| FR-2.8 | Support `disable-model-invocation` flag to prevent automatic activation | Must | 2 |
| FR-2.9 | Support `user-invocable` flag to show/hide in autocomplete | Must | 2 |

### FR-3: Dynamic Discovery

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-3.1 | Walk up from file path to project root discovering `.rustycode/skills/` directories | Must | 2 |
| FR-3.2 | Skip gitignored directories during walk-up | Must | 2 |
| FR-3.3 | Deeper skill directories override shallower on name collision | Must | 2 |
| FR-3.4 | Emit event when new skills are dynamically discovered | Should | 2 |
| FR-3.5 | Watch skill directories for filesystem changes and reload | Should | 3 |
| FR-3.6 | Debounce rapid filesystem events (300ms window) | Should | 3 |

### FR-4: Quality Scoring

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-4.1 | Compute quality score from 4 signals: telemetry (40%), graph (25%), intake (20%), routing (15%) | Must | 3 |
| FR-4.2 | Track load count, retention rate per skill | Must | 3 |
| FR-4.3 | Assign letter grade (A/B/C/D/F) based on weighted total | Must | 3 |
| FR-4.4 | Persist quality scores to sidecar JSON files | Must | 3 |
| FR-4.5 | Incremental score updates on session end | Should | 3 |
| FR-4.6 | Compute graph centrality scores for graph signal | Should | 3 |

### FR-5: Lifecycle Management

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-5.1 | Implement 5-state FSM: Discovered → Active → Watch → Demoted → Archived | Must | 3 |
| FR-5.2 | Transition to Watch on grade C | Must | 3 |
| FR-5.3 | Transition to Demoted after 2 consecutive D grades | Must | 3 |
| FR-5.4 | Transition Demoted → Archived after 14 days | Should | 3 |
| FR-5.5 | Physical filesystem move to `_demoted/` and `_archive/` directories | Should | 3 |
| FR-5.6 | Manual promote/restore operations | Should | 3 |
| FR-5.7 | Deleted state requires explicit user confirmation | Must | 3 |

### FR-6: Procedure / Pipeline

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-6.1 | Parse pipeline stages from SKILL.md body (### N. Stage Name headings) | Must | 4 |
| FR-6.2 | Each stage specifies: instructions, role, allowed_tools, success_criteria | Must | 4 |
| FR-6.3 | Stages with sub-numbers (3a, 3b) run in parallel | Should | 4 |
| FR-6.4 | `context: fork` creates a sub-agent for skill execution | Should | 4 |
| FR-6.5 | `context: inline` runs skill in current session | Must | 4 |
| FR-6.6 | Agent type override via `agent` frontmatter field | Should | 4 |

### FR-7: Capability Curator Agent

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-7.1 | Register as SpecialistAgent in AgentRegistry | Must | 2 |
| FR-7.2 | Subscribe to EventBus events for passive monitoring | Must | 2 |
| FR-7.3 | Extract intent signals from tool execution events | Must | 2 |
| FR-7.4 | Suggest skills when unmatched signals accumulate | Should | 2 |
| FR-7.5 | Run quality scoring on session end | Should | 3 |
| FR-7.6 | Run lifecycle state transitions on session end | Should | 3 |
| FR-7.7 | Detect co-invocation patterns (behavior mining) | Could | 5 |
| FR-7.8 | Propose skill improvements via LLM analysis | Could | 5 |

### FR-8: Skill Improvement

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-8.1 | Analyze recent user messages for corrections/preferences related to active skill | Could | 5 |
| FR-8.2 | Extract proposed updates as structured SkillUpdate objects | Could | 5 |
| FR-8.3 | Present proposed updates to user for approval | Could | 5 |
| FR-8.4 | Apply approved updates by rewriting SKILL.md (preserving frontmatter) | Could | 5 |
| FR-8.5 | Run analysis every N user turns (configurable, default 5) | Could | 5 |

### FR-9: Skillify (Session → Skill Capture)

| ID | Requirement | Priority | Phase |
|----|-------------|----------|-------|
| FR-9.1 | Capture repeatable session process into new SKILL.md | Could | 5 |
| FR-9.2 | Interactive interview to determine name, steps, success criteria | Could | 5 |
| FR-9.3 | Auto-detect arguments, allowed tools, and execution context from session | Could | 5 |
| FR-9.4 | Write SKILL.md to user-selected location (project or user) | Could | 5 |

## NFR — Non-Functional Requirements

| ID | Requirement | Target |
|----|-------------|--------|
| NFR-1 | Skill metadata loading must complete in <100ms for 200 skills | Startup latency |
| NFR-2 | Skill activation decision must take <10ms | Per-invocation latency |
| NFR-3 | Curator passive monitoring must add <1ms per tool execution | Runtime overhead |
| NFR-4 | Quality scoring must complete in <500ms per skill on session end | Session-end latency |
| NFR-5 | Event log must use append-only writes (no mutation) | Data integrity |
| NFR-6 | All file operations must validate paths stay within trusted roots | Security |
| NFR-7 | Curator failures must never block the main conversation | Fault isolation |
| NFR-8 | Skill loading must be safe against path traversal attacks | Security |
| NFR-9 | Support 500+ skills with no performance degradation | Scalability |
| NFR-10 | All new types must implement serde Serialize/Deserialize | Interop |

## Constraints

| ID | Constraint |
|----|-----------|
| C-1 | Must use `rustycode-bus::EventBus` for all cross-module communication |
| C-2 | Must follow existing error handling patterns (`anyhow` for app, `thiserror` for library) |
| C-3 | Must use `secrecy::SecretString` for any sensitive data |
| C-4 | Must not introduce `unsafe` code without explicit opt-in |
| C-5 | Must pass `cargo clippy --workspace -- -D warnings` |
| C-6 | Must use `serde-saphyr` (not `serde_yml` — security concerns) |
| C-7 | Must integrate with existing `ProgressiveLoader` patterns (metadata-first, content-on-demand) |
| C-8 | Existing `SkillManager` API consumers must not break during migration |
