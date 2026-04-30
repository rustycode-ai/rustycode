# Generative Programmer Patterns -- RustyCode: Complete Analysis

**Date**: 2026-04-25
**Research Method**: Full RSS feed analysis of generativeprogrammer.com (all articles) + cross-reference with RustyCode codebase
**Scope**: 50+ patterns from 9 source articles, mapped to RustyCode capabilities

---

## New Patterns from Previously Uncovered Articles

### Source: "12 Agentic Harness Patterns from Claude Code" (Apr 5, 2026)

These 12 patterns define the architecture of a production agentic coding assistant. RustyCode has partial coverage of several but significant gaps on others.

---

#### Pattern 1: Persistent Instruction File (CLAUDE.md)

**Description**: A project-level config file loaded every session, containing build/test commands, architecture rules, coding standards, and conventions. Functions as a machine-readable project constitution, not documentation for humans.

**Trade-off**: Maintenance burden. A stale instruction file is worse than none -- it actively misleads the agent.

**RustyCode mapping**:
- `rustycode-tools/src/hints_loader.rs`: Loads `.rustycodehints` and `CLAUDE.md` from project root
- `rustycode-prompt/src/layered.rs`: Scans for `AGENTS.md` and `CLAUDE.md` at multiple directory levels
- Status: **Partial** -- file loading exists, but no validation that loaded instructions remain current, no staleness detection, no structured schema for what belongs in the file

**Recommendation**: Add schema validation for instruction files. Flag stale instructions (e.g., references to deleted files, outdated build commands). Consider a `rustycode doctor` command that validates instruction accuracy.

---

#### Pattern 2: Scoped Context Assembly

**Description**: Load instructions dynamically from multiple files at different scopes: organization-wide, user-global, project-root, parent directories, child directories. Each scope contributes rules; closer scopes override farther ones. This is context CSS specificity.

**Trade-off**: Discoverability. Users don't know which file controls which behavior. Conflicting rules across scopes cause unpredictable agent behavior.

**RustyCode mapping**:
- `rustycode-prompt/src/layered.rs`: Implements layered instruction loading with directory traversal
- `rustycode-protocol/src/permission_modes.rs`: `PermissionRuleSource` has 6 precedence levels (Policy > CliArg > Session > ProjectSettings > LocalSettings > UserSettings)
- Status: **Have** -- scoped loading exists with proper precedence. This is one of RustyCode's stronger areas.

**Recommendation**: Add a `rustycode context` diagnostic command showing which scopes are active and which rules are in effect. Improves discoverability of the existing system.

---

#### Pattern 3: Tiered Memory

**Description**: Three memory layers with different access patterns:
1. **Compact index**: Capped at ~200 lines, always loaded, contains pointers to everything else
2. **Topic-specific files**: Loaded on demand when the topic arises (e.g., API patterns, deployment procedures)
3. **Session transcripts**: Searched only when needed, never loaded proactively

**Trade-off**: Deciding what goes where. The index must be curated; otherwise it becomes a dump.

**RustyCode mapping**:
- `crates/rustycode-memory/`: Memory crate exists but is thin
- `rustycode-tools/src/compaction.rs`: Progressive compaction with tool-response pruning exists
- Status: **Major Gap** -- RustyCode has compaction but not tiered memory. No always-loaded index concept. No topic-specific file loading. No MEMORY.md structure.

**Recommendation**: Implement the three-layer model. Start with MEMORY.md as the compact index (200-line cap). Add topic file loader. Session transcripts are already stored; wire them to on-demand search.

---

#### Pattern 4: Dream Consolidation

**Description**: A background process that reviews, deduplicates, prunes, and reorganizes agent memory during idle time. Called "autoDream" in Claude Code. Runs when the agent is not actively working on a task.

**Trade-off**: Consolidation itself costs tokens. Overly aggressive pruning deletes information the user needed.

**RustyCode mapping**:
- Status: **Missing** -- No background consolidation process. No idle-time maintenance. No deduplication of memory entries.

**Recommendation**: Build a consolidation pass that runs on session end or during idle. Start simple: deduplicate memory entries, prune entries older than N sessions, merge overlapping notes. Do not use LLM tokens for this -- deterministic deduplication first.

---

#### Pattern 5: Progressive Context Compaction

**Description**: Multiple compression stages tuned for content age:
1. **HISTORY_SNIP**: Remove oldest turns entirely
2. **Microcompact**: Light summarization of recent tool output
3. **CONTEXT_COLLAPSE**: Aggressive summarization of old conversation
4. **Autocompact**: Full LLM-powered summarization when thresholds hit

Recent content stays full fidelity. Old content gets progressively more compressed.

**Trade-off**: Lossy compression causes hallucination. The agent "remembers" a compressed version that may differ from the original.

**RustyCode mapping**:
- `rustycode-tools/src/compaction.rs`: Three-tier compaction (tool response pruning at 0/10/20/50/100%), middle-out removal, threshold-based triggering
- Status: **Partial** -- RustyCode has progressive tool-output pruning but not the four-stage age-based compression pipeline. No "HISTORY_SNIP" equivalent. No "Microcompact" layer.

**Recommendation**: Extend the existing compaction system with age-aware compression stages. The tool-response pruning is a good foundation. Add light summarization for old turns and aggressive collapse for very old turns. Track compression loss to detect hallucination risk.

---

#### Pattern 6: Explore-Plan-Act Loop

**Description**: Three distinct execution phases with increasing write permissions:
1. **Explore**: Read-only. Agent reads code, searches, gathers context. No modifications.
2. **Plan**: Discuss approach. Agent proposes changes, user reviews. Still no writes.
3. **Act**: Full tool access. Agent executes the plan.

Each phase uses distinct system prompts. Plan phase prompts emphasize architecture; Act phase prompts emphasize precision.

**Trade-off**: Adds turns before any output. Impatient users see "planning" and want results.

**RustyCode mapping**:
- `rustycode-orchestra/src/plan_mode.rs`: PlanMode with approval triggers
- `rustycode-protocol/src/permission_modes.rs`: `PermissionMode::Plan` allows read-only tools, blocks writes
- `rustycode-orchestra/src/planning/`: Planning infrastructure exists
- Status: **Partial** -- Plan mode exists as a permission state, but there is no enforced three-phase lifecycle. The agent can jump to writing. No distinct system prompts per phase.

**Recommendation**: Make Explore-Plan-Act the default execution lifecycle for non-trivial tasks. Enforce phase transitions. Generate distinct system prompts for Plan vs Act. Add a `--skip-explore` flag for users who want to skip ahead.

---

#### Pattern 7: Context-Isolated Subagents

**Description**: Separate agents with their own context windows, system prompts, and restricted tool access. Research agents cannot edit code. Planning agents cannot execute commands. Each subagent is purpose-built for its phase.

**Trade-off**: Coordination overhead. Nuance is lost in handoff between agents. The parent must synthesize partial results.

**RustyCode mapping**:
- `rustycode-agents/src/agents/subagent.rs`: SubAgent implementation exists
- `rustycode-orchestration/src/`: Musician/Editor/Composer tiers with different roles
- `rustycode-protocol/src/permission_modes.rs`: Different permission modes per tool
- Status: **Partial** -- Subagents exist and tiers have different roles, but context isolation is not enforced. Musician and Editor share the same context window. No tool access restriction per subagent.

**Recommendation**: Enforce context isolation between tiers. Give each tier its own context budget and tool subset. Research tier gets read-only tools. Planning tier gets read + write but no execute. Execution tier gets everything.

---

#### Pattern 8: Fork-Join Parallelism

**Description**: Spawn multiple subagents in parallel, each in an isolated git worktree. Parent's cached context is reused by each fork. Results are merged back.

**Trade-off**: Merge complexity when branches touch overlapping files. Conflict resolution can negate parallelism gains.

**RustyCode mapping**:
- `rustycode-orchestra/src/worktree/`: Worktree management exists
- `rustycode-orchestration/src/ensemble_strategy.rs`: Ensemble strategy (23.8K) for multi-agent coordination
- Status: **Partial** -- Worktree infrastructure and ensemble strategy exist, but fork-join with cached context reuse is not implemented. Ensemble strategy runs agents; it doesn't share cached context between forks.

**Recommendation**: Add context-sharing between parallel forks. The parent's loaded context (project structure, instruction files, recent history) should be snapshotted and injected into each forked agent rather than re-loaded from scratch.

---

#### Pattern 9: Progressive Tool Expansion

**Description**: Start with a small default tool set (~20 tools). Activate additional tools on demand as the task requires. Claude Code has ~60 total tools available but exposes fewer than 20 by default. The agent requests tool activation when needed.

**Trade-off**: Expansion logic complexity. Activating too late slows work. Activating too early clutters context.

**RustyCode mapping**:
- `rustycode-tools/src/registry_builder.rs`: Tool registry exists
- `rustycode-protocol/src/permission_modes.rs`: Permission modes gate tools, but statically
- Status: **Missing** -- All tools are either available or not, based on permission mode. No dynamic activation. No "request this tool" mechanism. No concept of a default subset vs. extended set.

**Recommendation**: Implement tool activation tiers. Default set: Read, Edit, Write, Bash, Grep, Glob (matching Claude Code's starting set). Extended tools activated on demand when the agent encounters a task requiring them. Track which tools are actually used per session to optimize the default set.

---

#### Pattern 10: Command Risk Classification

**Description**: Deterministic pre-parsing of every command. Per-tool permission gating with pattern matching. Shell commands classified by verb, flags, and target. Each tool has individual allow/ask/deny rules.

**Trade-off**: Rigidity. Classification rules need ongoing tuning. New command patterns require new rules.

**RustyCode mapping**:
- `rustycode-tools/src/smart_approve.rs`: 26K of command classification logic with verb-based categorization
- `rustycode-tools/src/security_patterns.rs`: Security pattern matching
- `rustycode-tools/src/security/`: Security validation module
- `rustycode-protocol/src/permission_modes.rs`: Per-tool permission rules with pattern matching
- Status: **Have** -- This is RustyCode's most complete area. Smart approve has read-only/ destructive classifications for many commands. Permission rules support allow/ask/deny with pattern matching.

**Recommendation**: Continue expanding classification coverage. Add telemetry to track which commands fall through to "ask" -- these are candidates for new classification rules.

---

#### Pattern 11: Single-Purpose Tool Design

**Description**: Replace the general shell with purpose-built tools: FileReadTool, FileEditTool, GrepTool, GlobTool. Each tool has typed inputs, constrained scope, and its own permission rules. The general shell becomes a fallback, not the primary interface.

**Trade-off**: Still need the general shell as fallback for anything the purpose-built tools don't cover.

**RustyCode mapping**:
- `rustycode-tools/src/executor/`: Tool executor framework with individual tool implementations
- `rustycode-tools/src/edit_format.rs`: Edit tool with flexible matching
- `rustycode-tools/src/line_endings.rs`: Purpose-built line-ending handling
- Status: **Have** -- RustyCode already has purpose-built tools (read, write, edit, grep, glob, bash) with typed inputs. The bash tool is the general fallback.

**Recommendation**: Audit bash tool usage. If the agent frequently uses bash for operations that could be purpose-built tools (e.g., file search, text processing), create new purpose-built tools for those operations.

---

#### Pattern 12: Deterministic Lifecycle Hooks

**Description**: Shell commands run automatically at specific lifecycle points. Claude Code has 25+ hook points: PreToolUse, PostToolUse, SessionStart, SessionEnd, CwdChanged, Error, PreCompact, PostCompact. Anything that must happen every time belongs in a hook, not in an instruction file.

**Trade-off**: Debugging difficulty. Hooks are invisible to the user unless they fail. Hook chains can have unexpected interactions.

**RustyCode mapping**:
- `rustycode-tools/src/hooks.rs`: Hook system with triggers: SessionStart, SessionEnd, PreToolUse, PostToolUse, PreCompact, PostCompact, Error
- `rustycode-tools/src/lifecycle.rs`: Lifecycle event infrastructure
- Status: **Partial** -- RustyCode has 7 hook points to Claude Code's 25+. Missing: CwdChanged, Notification, SubagentStart, SubagentEnd, Stop, PlanStart, PlanEnd, and others. Hook profiles (Minimal/Standard/Strict) exist.

**Recommendation**: Expand hook points to cover the full agent lifecycle. The 7 existing hooks are the critical ones; add hooks for directory changes, subagent lifecycle events, and planning phase transitions. Each new hook point is a low-cost, high-value extensibility win.

---

### Source: "Skill Authoring Patterns from Anthropic's Best Practices" (Apr 19, 2026)

14 patterns for building effective agent skills/instructions. These define how to write instructions that agents actually follow.

---

#### Pattern 13: Activation Metadata

**Description**: The skill's `description` field is the only signal used at selection time. Pack it with both triggers (when to fire) AND exclusions (when NOT to fire). Write descriptions "pushy" because agents under-trigger. Budget: 1536 characters max.

**RustyCode mapping**:
- `rustycode-skill/src/metadata.rs`: Skill metadata with description field
- `rustycode-skill/src/activation.rs`: Skill activation logic
- Status: **Partial** -- Skill metadata and activation exist, but no evidence of pushy descriptions, exclusion clauses, or the 1536-char budget constraint.

**Recommendation**: Enforce description length limits. Add exclusion clause support to skill metadata. Audit existing skill descriptions for under-triggering.

---

#### Pattern 14: Exclusion Clause

**Description**: End skill descriptions with explicit exclusions: "Do NOT use for blog articles, newsletters, documentation generation." Both positive triggers AND exclusions needed for accurate selection.

**RustyCode mapping**:
- Status: **Missing** -- No exclusion mechanism in skill metadata or activation logic.

**Recommendation**: Add `excludes` field to skill YAML frontmatter. Include exclusions in activation scoring. This prevents the agent from applying the wrong skill to a task.

---

#### Pattern 15: Context Budget

**Description**: Every token in a skill crowds out tokens from other skills. Default assumption: the model is smart. If removing a sentence wouldn't confuse a competent reader, remove it.

**RustyCode mapping**:
- `rustycode-tools/src/token_counter.rs`: Token counting exists
- Status: **Missing** -- No per-skill token budget enforcement. No mechanism to warn when loaded skills exceed a context budget.

**Recommendation**: Add context budget tracking for loaded skills. Warn when total skill context exceeds a threshold. Implement priority-based eviction when budget is exceeded.

---

#### Pattern 16: Progressive Disclosure

**Description**: SKILL.md as table of contents (under 500 lines), linking to domain-specific files. Keep reference graph shallow (one hop from SKILL.md). Long files get a TOC at the top.

**RustyCode mapping**:
- `rustycode-skill/src/graph.rs`: Skill dependency graph exists
- `rustycode-skill/src/discovery.rs`: Skill file discovery
- Status: **Partial** -- Graph structure exists but no enforcement of the 500-line TOC constraint or shallow reference depth.

**Recommendation**: Add depth limit to skill reference loading. Warn when skill files exceed size thresholds. Enforce TOC structure for large skill files.

---

#### Pattern 17: Control Tuning (Freedom Calibration)

**Description**: Match instruction freedom to task fragility:
- High freedom (text instructions) for code review
- Medium freedom (pseudocode) for deploy runbooks
- Low freedom (exact scripts) for database migrations

Authors consistently err toward over-constraining.

**RustyCode mapping**:
- Status: **Missing** -- No concept of instruction freedom levels. All instructions have equal weight.

**Recommendation**: Add a `strictness` field to skill YAML. Use it to calibrate how much the agent can deviate from the provided instructions. Default to medium.

---

#### Pattern 18: Explain-the-Why

**Description**: State the rule, then explain WHY so the model can generalize. "Use constructor injection. Field injection breaks testability because the field cannot be replaced in tests." beats "MUST use constructor injection."

**RustyCode mapping**:
- Status: **Meta** -- This is about instruction quality, not system architecture. The RustyCode CLAUDE.md already follows this pattern in some places ("Why it matters" sections).

**Recommendation**: Audit all system prompts and skill instructions for unexplained rules. Add "because" clauses to every constraint.

---

#### Pattern 19: Template Scaffold

**Description**: Ship templates with placeholders. Two modes:
- Strict: "ALWAYS use this exact template" for machine-parsed output
- Flexible: "A sensible default; adapt as needed" for documents

**RustyCode mapping**:
- `rustycode-tools/src/plan_templates.rs`: Plan templates exist (26K)
- `rustycode-prompt/src/`: Prompt templates
- Status: **Partial** -- Templates exist for plans and prompts. No explicit strict/flexible mode distinction.

**Recommendation**: Add template strictness mode. Strict templates validate output against the template structure. Flexible templates allow deviation.

---

#### Pattern 20: In-Skill Examples

**Description**: Embed 2-3 concrete input/output pairs. Templates show skeleton; examples show populated instances. Examples are the single most effective instruction technique.

**RustyCode mapping**:
- `rustycode-skill/src/workflows.rs`: Workflow definitions (46K, largest file in skill crate)
- Status: **Partial** -- Workflows may contain examples, but no systematic requirement for input/output pairs in skill definitions.

**Recommendation**: Require 2-3 input/output examples in every skill definition. Add validation in skill loading.

---

#### Pattern 21: Known Gotchas Section

**Description**: Dedicated section listing concrete failure modes: "Scanned PDFs return empty silently. Check page type first." Prevents the agent from walking into known traps.

**RustyCode mapping**:
- Status: **Missing** -- No structured "gotchas" or "pitfalls" section in skill definitions or project instructions.

**Recommendation**: Add `gotchas` field to skill YAML. Auto-surface relevant gotchas when a skill is activated. This is cheap to implement and prevents expensive mistakes.

---

#### Pattern 22: Execution Checklist

**Description**: Copyable checklist the agent pastes into its response and ticks off. Visible to both agent and user. Use for workflows with more than 3 steps.

**RustyCode mapping**:
- `rustycode-tools/src/todo.rs`: Todo tracking exists (20K)
- `rustycode-tools/src/todo_read.rs`: Todo reading
- Status: **Partial** -- Todo tracking exists, but no automatic checklist generation from skill workflows. No tick-off mechanism in agent output.

**Recommendation**: Auto-generate checklists from skill workflow steps. Display progress inline in agent responses.

---

#### Pattern 23: Self-Correcting Loop

**Description**: Produce output, run validator, if fails then fix and revalidate. Needs retry cap and fallback to user.

**RustyCode mapping**:
- `rustycode-orchestration/src/editor.rs`: Editor tier validates Musician output
- `rustycode-orchestration/src/verification_gates.rs`: Verification gates with retry (20K)
- `rustycode-tools/src/task_retry.rs`: Task retry logic (14K)
- Status: **Have** -- Self-correction with retry caps exists in the orchestration pipeline. Editor reviews Musician output. Verification gates validate results.

**Recommendation**: Make the retry cap and fallback-to-user behavior more visible. Log when the agent self-corrects and what it fixed.

---

#### Pattern 24: Plan-Validate-Execute

**Description**: Produce a verifiable intermediate artifact (JSON plan) before any side effects. The agent iterates on the plan freely; the real target is only touched once the plan validates. Distinct from Self-Correcting Loop, which iterates after work has already landed.

**RustyCode mapping**:
- `rustycode-orchestra/src/plan_mode.rs`: Plan mode with approval
- `rustycode-orchestration/src/plan_refiner.rs`: Plan refinement
- Status: **Partial** -- Plan mode exists, but the plan is not a structured verifiable artifact. No JSON schema validation of plans before execution.

**Recommendation**: Add structured plan schema. Validate plans against the schema before allowing execution. This separates planning errors from execution damage.

---

#### Pattern 25: Utility Bundle

**Description**: Ship purpose-built scripts alongside SKILL.md. The agent invokes them via bash; only the output consumes context. Scripts handle complex logic that would be error-prone as inline instructions.

**RustyCode mapping**:
- `rustycode-skill/src/lifecycle.rs`: Skill lifecycle management
- Status: **Missing** -- No mechanism to bundle executable scripts with skills. Skills contain instructions but not executable utilities.

**Recommendation**: Add `scripts/` directory support in skill packages. Allow skills to reference bundled scripts that the agent can invoke.

---

#### Pattern 26: Autonomy Calibration

**Description**: Declare `allowed-tools` list in YAML frontmatter. Pre-approves only the capabilities the skill needs. Pair with permission rules for actual restrictions.

**RustyCode mapping**:
- `rustycode-protocol/src/permission_modes.rs`: Permission rules with tool patterns
- Status: **Partial** -- Permission rules exist at the system level, but skills cannot declare their own allowed-tools lists. The permission system is global, not per-skill.

**Recommendation**: Add `allowed-tools` to skill YAML frontmatter. When a skill is activated, automatically configure permission rules for its declared tools. Revoke when the skill completes.

---

### Source: "Practical Lessons from the Claude Code Leak" (Apr 3, 2026)

10 lessons distilled from Claude Code's architecture. These are high-level insights, not individual patterns.

---

#### Lesson 1: CLAUDE.md is a config file, not a README

It is loaded every session and shapes every interaction. If the content is wrong, every interaction is wrong.

**RustyCode status**: Handled via `hints_loader.rs` and `layered.rs`. The instruction files are loaded into every session. No validation that they are correct.

---

#### Lesson 2: Three-layer memory (MEMORY.md index + topic files + transcripts)

Auto-memory capped at 200 lines. "autoDream" consolidation mode runs during idle.

**RustyCode status**: Compaction exists, but three-layer memory does not. No auto-memory index. No 200-line cap. No dream consolidation.

---

#### Lesson 3: Split instructions by scope (org, user, project, local, parent/child dirs)

Closer scopes override farther scopes. CSS specificity for agent instructions.

**RustyCode status**: Handled. The layered prompt system and permission rule precedence implement this correctly.

---

#### Lesson 4: Explore first, then plan, then code (distinct phases with system prompts)

**RustyCode status**: Partial. Plan mode exists but is not enforced as a lifecycle. No distinct system prompts per phase.

---

#### Lesson 5: Use subagents to isolate context (Explore agent is read-only)

**RustyCode status**: Partial. Subagents exist but context isolation is not enforced.

---

#### Lesson 6: Use worktrees for parallel work (fork-join with cached context reuse)

**RustyCode status**: Partial. Worktrees exist but fork-join with context sharing does not.

---

#### Lesson 7: Configure permissions at tool level (default tools expandable to 60+)

**RustyCode status**: Partial. Permission modes exist but all tools are always available within a mode. No progressive expansion.

---

#### Lesson 8: Reduce approval fatigue (93% of prompts auto-approved)

**RustyCode status**: Have. `smart_approve.rs` classifies commands. Auto mode handles most routine operations without user prompts.

---

#### Lesson 9: Use hooks for repeatable automation (25+ hook points)

**RustyCode status**: Partial. 7 hook points exist. Need ~18 more for full coverage.

---

#### Lesson 10: The harness matters more than the prompt

"Claude Code's secret sauce is probably not the model." The orchestration, memory, permissions, and lifecycle management are what make it effective.

**RustyCode status**: This is the core thesis of the orchestration crate. RustyCode already invests heavily in harness over raw prompting.

---

### Source: "State of AI-Assisted Coding in 2026" (Mar 29, 2026)

Industry analysis, not patterns, but shapes strategy.

**Key insight**: Local developer agents (CLI-native) are now the default serious workflow. The three-phase pipeline (Plan, Build, Review) is standard. Review is the emerging bottleneck.

**RustyCode implication**: RustyCode is positioned correctly as a local developer agent. The Plan and Build phases are covered. Review is the gap -- no dedicated review agent or review-specific workflow.

**Recommendation**: Build a dedicated review agent that runs after code generation. Wire it into the existing verification gates. Consider integration with external review tools (CodeRabbit, Qodo) for the Review phase.

---

### Source: "Taxonomy of AI Agents" (Nov 1, 2025)

Four agent archetypes that define what RustyCode could become.

---

#### Headless Agent

Decouple intelligence from UI. API-first. Functionality over conversation.

**RustyCode status**: Have. `rustycode-core` provides headless execution. CLI mode is headless. TB 2.0 agent runs headless.

---

#### Ambient Agent

Headless plus background operation. Event-driven, proactive, human-in-the-loop (notify/question/review).

**RustyCode status**: Missing. RustyCode is reactive -- it only works when prompted. No background operation. No event-driven triggers. No proactive notifications.

**Recommendation**: Add file-watch triggers. When a watched file changes, run a pre-configured check. This is the path from "tool" to "assistant."

---

#### Durable Agent

Persist full execution history. Recover from crashes. Avoid re-executing side effects.

**RustyCode status**: Partial. Session persistence exists. Checkpoint/recovery exists. But crash recovery is not robust -- the agent may re-execute side effects after recovery.

**Recommendation**: Add side-effect ledger. Track every state-mutating action. On recovery, skip already-completed side effects. This prevents duplicate deployments, duplicate commits, duplicate API calls.

---

#### Deep Agent

Multi-agent systems with planning, persistent memory, sub-agent delegation. "Shift from reactive prompting to proactive problem solving."

**RustyCode status**: Partial. Multi-tier orchestration (Musician/Editor/Composer) exists. Deep-thinker provides structured reasoning. But the system is still reactive -- no proactive planning or autonomous delegation.

**Recommendation**: Build on the existing orchestration foundation to add proactive capabilities. The tiers and thinking modules are there; wire them to background triggers.

---

### Source: "Agent Communication Protocols Landscape" (Jun 2025)

Protocol landscape for inter-agent and agent-to-tool communication.

| Protocol | Stars | Purpose | RustyCode relevance |
|----------|-------|---------|---------------------|
| MCP | 100K+ | Context-oriented, client-server, JSON-RPC | **Have** -- `rustycode-mcp` crate |
| A2A | 20K+ | Inter-agent, HTTP(S), SSE, async-first | **Missing** -- no inter-agent protocol |
| AG-UI | 4K+ | Agent-to-UI communication, event-driven | **Partial** -- TUI exists but not event-driven protocol |
| agents.json | -- | OpenAPI-based for website agent compatibility | **Missing** -- no agent-compatible API layer |
| ANP | -- | Decentralized identity via W3C DID | **Not relevant** -- not RustyCode's scope |

**Recommendation**: MCP support is good. A2A support would enable RustyCode agents to coordinate with other agents (Claude Code, Codex, etc.). This is a future consideration, not immediate priority.

---

### Source: "Applying Kubernetes Patterns to LLM Workloads" (Mar 2026)

Infrastructure patterns, less directly relevant to RustyCode's agent-level focus. Two insights worth noting:

1. **Model Data Staging**: Pre-load models before the agent needs them. RustyCode should pre-warm LLM connections on session start rather than waiting for the first prompt.

2. **Token-Aware Routing**: Route requests based on expected token cost, not just availability. RustyCode should route simple tasks to cheaper/faster models and complex tasks to more capable models.

**RustyCode status**: Partial. Task classification exists (`LocalTaskClassifier` in CLI). Model routing based on task complexity is not implemented.

---

## Previously Covered Patterns (Refined)

These 7 patterns were in the original analysis. They are refined here with insights from the new articles.

---

### Pattern A: Reflection Loop (Self-Critique)

*Source: Issue #6 -- Andrew Ng's Core AI Agent Patterns*

**The pattern**: Model critiques and refines its own output before returning.

**Refinement from new sources**: The Claude Code leak confirms this is implemented via context-isolated subagents (Pattern 7 above), not a single model thinking twice. The reviewer is a separate agent with its own context and tools, not the same model with a "reflect" prompt.

**RustyCode mapping**:
- Have: Editor tier reviews Musician output
- Missing: The Editor should be a context-isolated subagent, not a tier escalation within the same context window
- Missing: Confidence scoring -- no signal about "how certain are we?"

**Opportunity**: Restructure Editor tier as a context-isolated subagent with read-only tools.

---

### Pattern B: Multi-Agent Orchestration

*Source: Issue #16 -- Microsoft Multi-Agent Patterns*

**The patterns** (5 approaches): Sequential, Concurrent, Group chat, Maker-checker, Handoff.

**Refinement from new sources**: The Claude Code patterns add fork-join parallelism (Pattern 8) and context-isolated subagents (Pattern 7) as distinct from simple concurrent execution. The key insight: "Context engineering is replacing prompt engineering." How information flows between agents matters more than individual prompts.

**RustyCode mapping**:
- Have: Sequential tier escalation, Maker-checker
- Missing: Context optimization between agents (what does the next tier need?)
- Missing: Fork-join parallelism with cached context reuse
- Missing: Group chat / negotiation between agents

**Opportunity**: Formalize tier handoff protocol with explicit context engineering.

---

### Pattern C: Tool Use and Domain-Specific Capabilities

*Source: Issue #6, #16 -- Anthropic + Shopify insights*

**Refinement from new sources**: Single-purpose tool design (Pattern 11) and progressive tool expansion (Pattern 9) are the concrete implementations of this principle. Generic APIs don't work; agents need domain-specific tools with typed inputs and constrained scope.

**RustyCode mapping**:
- Have: Purpose-built tools with typed inputs, permission system
- Missing: Progressive tool expansion (always-on vs. on-demand)
- Missing: Domain-specific tool wrappers with pre-call validation

**Opportunity**: Layer domain-specific tool wrappers. Add tool activation tiers.

---

### Pattern D: Structured Output Formats

*Source: Issue #2 -- "Does Prompt Format Matter?"*

**The finding**: JSON templates outperform plain text by 40% in code translation tasks.

**Refinement from new sources**: The skill authoring patterns reinforce this. Template scaffolds (Pattern 19) with strict mode enforce structured output. The key is not just JSON but validated JSON -- a schema that the output must conform to.

**RustyCode mapping**:
- Partial: Structured thinking uses JSON, but only for complex tasks
- Missing: Format enforcement for all tier responses
- Missing: Output validation against schemas

**Opportunity**: Define JSON schemas for tier outputs. Validate before handoff.

---

### Pattern E: Evaluation and Quality Gates

*Source: "Key Generative AI Concepts" -- Quality Assurance section*

**The framework**: LLM-as-Judge, Metric-based, Human feedback.

**Refinement from new sources**: The skill authoring patterns add the Self-Correcting Loop (Pattern 23) and Plan-Validate-Execute (Pattern 24) as evaluation mechanisms. Evaluation is not just a gate at the end -- it's a loop throughout execution.

**RustyCode mapping**:
- Have: Verification gates, self-correcting loops, task retry
- Missing: LLM-as-Judge (second model evaluating output)
- Missing: CI-integrated evaluation
- Missing: Production quality monitoring

**Opportunity**: Add LLM-as-Judge as an optional evaluation tier. Wire into CI.

---

### Pattern F: Project Discoverability for AI

*Source: "7 Steps to Make Your OSS Project AI-Ready"*

**The pattern**: Projects need AI-readable artifacts: llms.txt, AGENTS.md, MCP endpoints.

**Refinement from new sources**: The scoped context assembly pattern (Pattern 2) shows this is not just about having the files -- it's about loading them at the right scope with the right precedence. AGENTS.md at project root is table stakes.

**RustyCode mapping**:
- Have: Layered loading of AGENTS.md and CLAUDE.md at multiple scopes
- Missing: llms.txt support
- Missing: Structured schema for what belongs in these files

**Opportunity**: Add llms.txt support. Document the expected content structure for instruction files.

---

### Pattern G: Domain Context Over Generic Prompting

*Source: Issue #17, Issue #16 -- Rod Johnson, Russ Miles*

**The insight**: AI agents fail without domain context. Autonomy levels must match task complexity.

**Refinement from new sources**: The taxonomy of agents (Headless, Ambient, Durable, Deep) provides a maturity model for domain context. Control tuning (Pattern 17) adds the nuance that instruction freedom should match task fragility -- not all tasks need the same level of domain specificity.

**RustyCode mapping**:
- Major gap: RustyCode is domain-agnostic
- Missing: No way to inject domain models into sessions
- Missing: No autonomy levels (when to auto-fix vs. suggest vs. ask)
- Missing: No control tuning (instruction freedom matching task fragility)

**Opportunity**: Build domain context ingestion system. Add autonomy configuration per task type.

---

## Comprehensive Gap Analysis

| # | Pattern | Source | RustyCode Status | Gap Severity |
|---|---------|--------|------------------|--------------|
| 1 | Persistent Instruction File | Claude Code 12 | Partial (loading exists, no validation) | Medium |
| 2 | Scoped Context Assembly | Claude Code 12 | **Have** (layered prompt + precedence) | None |
| 3 | Tiered Memory (3-layer) | Claude Code 12 | **Missing** (compaction only) | **Critical** |
| 4 | Dream Consolidation | Claude Code 12 | **Missing** | High |
| 5 | Progressive Context Compaction | Claude Code 12 | Partial (tool pruning exists) | Medium |
| 6 | Explore-Plan-Act Loop | Claude Code 12 | Partial (plan mode, no enforcement) | **High** |
| 7 | Context-Isolated Subagents | Claude Code 12 | Partial (subagents, no isolation) | **High** |
| 8 | Fork-Join Parallelism | Claude Code 12 | Partial (worktrees, no context sharing) | Medium |
| 9 | Progressive Tool Expansion | Claude Code 12 | **Missing** | **High** |
| 10 | Command Risk Classification | Claude Code 12 | **Have** (smart_approve) | None |
| 11 | Single-Purpose Tool Design | Claude Code 12 | **Have** | None |
| 12 | Deterministic Lifecycle Hooks | Claude Code 12 | Partial (7 of 25+ hooks) | Medium |
| 13 | Activation Metadata | Skill Authoring | Partial (metadata, no pushy descriptions) | Low |
| 14 | Exclusion Clause | Skill Authoring | **Missing** | Medium |
| 15 | Context Budget | Skill Authoring | **Missing** | Medium |
| 16 | Progressive Disclosure | Skill Authoring | Partial (graph, no depth limit) | Low |
| 17 | Control Tuning | Skill Authoring | **Missing** | Medium |
| 18 | Explain-the-Why | Skill Authoring | Partial (meta-level) | Low |
| 19 | Template Scaffold | Skill Authoring | Partial (templates, no strict/flexible) | Low |
| 20 | In-Skill Examples | Skill Authoring | Partial (workflows, no examples requirement) | Low |
| 21 | Known Gotchas Section | Skill Authoring | **Missing** | Medium |
| 22 | Execution Checklist | Skill Authoring | Partial (todos, no auto-checklist) | Low |
| 23 | Self-Correcting Loop | Skill Authoring | **Have** (Editor + verification + retry) | None |
| 24 | Plan-Validate-Execute | Skill Authoring | Partial (plan mode, no schema validation) | Medium |
| 25 | Utility Bundle | Skill Authoring | **Missing** | Medium |
| 26 | Autonomy Calibration | Skill Authoring | Partial (permission rules, no per-skill scope) | Medium |
| A | Reflection Loop (Self-Critique) | Andrew Ng | Partial (Editor, not context-isolated) | Medium |
| B | Multi-Agent Orchestration | Microsoft | Partial (sequential + maker-checker) | Medium |
| C | Domain-Specific Tools | Anthropic/Shopify | Have (purpose-built tools) | Low |
| D | Structured Output Formats | Format Study | Partial (thinking only) | Medium |
| E | Evaluation + Quality Gates | GenAI Concepts | Partial (verification gates) | **High** |
| F | Project Discoverability | AI-Ready OSS | **Have** (layered loading) | None |
| G | Domain Context Injection | Johnson/Miles | **Missing** | **Critical** |
| -- | Ambient Agent (background) | Taxonomy | **Missing** | High |
| -- | Durable Agent (side-effect ledger) | Taxonomy | **Missing** | High |
| -- | A2A Protocol | Protocols | **Missing** | Low (future) |
| -- | Review Agent | State of AI | **Missing** | Medium |

**Summary**:
- **Have / Strong**: 6 patterns (Scoped Context, Risk Classification, Single-Purpose Tools, Self-Correcting Loop, Project Discoverability, Headless Agent)
- **Partial**: 18 patterns (infrastructure exists but incomplete)
- **Missing / Critical**: 2 patterns (Dream Consolidation, Ambient Agent)
- **High Priority but now Implemented or Partially Implemented**: Explore-Plan-Act enforcement, Context-Isolated Subagents, Domain Context, Progressive Tool Expansion, LLM-as-Judge
- **Missing / Medium-Low**: 8 patterns (skill authoring details, utility bundles, protocols)

---

## Revised Improvement Phases

The phases are reordered based on the complete pattern set. The original Phase 1 (Context Engineering Protocol) is absorbed into a broader memory and lifecycle overhaul.

---

### Phase 1: Memory Architecture (3-4 weeks)

**Goal**: Implement the three-layer memory model that production agents require.

This is the single highest-impact gap. Without proper memory, every other improvement is built on sand.

**Work items**:
1. **MEMORY.md index layer**: Capped at 200 lines, always loaded, contains pointers to topic files and session summaries. Structure: `# Active Context` + `# Topic Index` + `# Recent Decisions`.
2. **Topic file loader**: On-demand loading of topic-specific files (e.g., API patterns, deployment procedures, architecture decisions). Triggered by keyword matching from the index.
3. **Session transcript search**: Existing session storage wired to on-demand search. Never loaded proactively.
4. **Dream consolidation**: Deterministic pass on session end -- deduplicate entries, prune entries older than N sessions, merge overlapping notes. No LLM tokens for this pass.
5. **Compaction upgrade**: Extend existing progressive compaction with age-aware stages (HISTORY_SNIP, Microcompact, CONTEXT_COLLAPSE).

**Success metrics**:
- MEMORY.md index stays under 200 lines across 20+ sessions
- Topic files load in <100ms on demand
- Dream consolidation reduces memory size by 30%+ without losing key decisions
- Compaction preserves recent context at full fidelity

**Current implementation status**:
- Implemented in `rustycode-memory`
- Transcript search, topic loading, and consolidation are wired
- Remaining work is primarily the end-of-session dream consolidation flow

---

### Phase 2: Explore-Plan-Act Lifecycle (2-3 weeks)

**Goal**: Enforce the three-phase execution model with distinct permissions and prompts.

**Work items**:
1. **Phase enforcement**: Agent starts in Explore mode (read-only tools). Transitions to Plan mode after context gathering. Transitions to Act mode after user approves plan.
2. **Distinct system prompts**: Plan prompts emphasize architecture and trade-offs. Act prompts emphasize precision and verification.
3. **Structured plan schema**: Plans are JSON artifacts validated against a schema before execution begins. Invalid plans never reach Act phase.
4. **Skip-ahead flag**: `--skip-explore` and `--skip-plan` for users who want to jump phases.

**Success metrics**:
- All non-trivial tasks pass through Explore -> Plan -> Act
- Plan validation catches schema violations before execution
- User override available via flags
- Phase transitions logged in execution trace

**Current implementation status**:
- Implemented across `rustycode-protocol`, `rustycode-prompt`, and `rustycode-orchestration`
- Phase lifecycle, prompts, and pipeline transitions are wired
- Remaining work is mostly integration polish and UX surface area

---

### Phase 3: Context-Isolated Subagents (2-3 weeks)

**Goal**: Each tier operates in its own context window with restricted tools.

**Work items**:
1. **Context isolation**: Each tier (Musician, Editor, Composer) gets its own context budget. No context leakage between tiers.
2. **Tool restriction per tier**: Research tier = read-only. Planning tier = read + write (no exec). Execution tier = everything.
3. **Handoff protocol**: Explicit context package passed between tiers. Contains: task description, relevant code, constraints, previous tier's assessment.
4. **Fork-join with shared context cache**: For parallel tasks, snapshot parent context and inject into each fork.

**Success metrics**:
- Each tier has independent context budget tracking
- Tool restrictions enforced (no write calls from research tier)
- Handoff packages contain all necessary context (no "I don't have that information" errors)
- Parallel forks start within 500ms of each other

**Current implementation status**:
- Core runtime hooks are implemented in `rustycode-orchestration`
- `TierIsolation`, `HandoffPackage`, and `ForkJoinExecutor` exist and are wired into the step pipeline
- Remaining work is broader adoption across any additional orchestration entry points

---

### Phase 4: Domain Context + Autonomy Levels (3-4 weeks)

**Goal**: RustyCode understands project specifics and respects user-configured autonomy.

**Work items**:
1. **Domain context format**: YAML-based project descriptor with architecture rules, preferred patterns, build commands, test strategies.
2. **Domain context loader**: Read domain context from `.rustycode/domain.yaml` (or equivalent). Inject into system prompts for each tier.
3. **Autonomy levels** (0-4):
   - L0: Suggest only (no action)
   - L1: Ask permission before executing
   - L2: Execute, notify user
   - L3: Execute, notify after
   - L4: Full autonomy (CI/CD only)
4. **Control tuning**: Per-task-type freedom calibration. Code review = high freedom. Database migration = low freedom.
5. **Side-effect ledger**: Track every state-mutating action. On recovery, skip already-completed side effects.

**Success metrics**:
- Domain context improves task success rate by 15%+
- Autonomy levels respected across all tiers
- Control tuning adjusts agent behavior per task type
- Side-effect ledger prevents duplicate operations after crash recovery

**Current implementation status**:
- Domain context is implemented in `rustycode-config` and injected into prompts
- The memory layer persists a discoverable domain topic
- Autonomy and side-effect plumbing are in place, but some deeper hooks remain partial

---

### Phase 5: Progressive Tooling + Hooks (2-3 weeks)

**Goal**: Dynamic tool activation and comprehensive lifecycle hooks.

**Work items**:
1. **Tool activation tiers**: Default set (Read, Edit, Write, Bash, Grep, Glob). Extended set activated on demand. Track usage per session to optimize default.
2. **Hook point expansion**: From 7 to 20+ hook points. Add: CwdChanged, SubagentStart, SubagentEnd, PlanStart, PlanEnd, ErrorRecovery, ContextSwitch.
3. **Per-skill tool scoping**: Skills declare `allowed-tools` in YAML. Auto-configure permissions when skill activates.
4. **Context budget for skills**: Track token cost of loaded skills. Evict low-priority skills when budget exceeded.

**Success metrics**:
- Default tool set covers 90%+ of common tasks without expansion
- 20+ hook points available
- Skill activation auto-configures tool permissions
- Skill context budget enforced

**Current implementation status**:
- Tool tiers, expanded hooks, per-skill scoping, and skill budgeting are implemented
- The remaining gap is higher-level runtime wiring across the rest of the orchestration surface

---

### Phase 6: Skill Authoring + Quality (2-3 weeks)

**Goal**: Complete the skill authoring system with production-quality patterns.

**Work items**:
1. **Exclusion clauses**: Add `excludes` field to skill YAML. Used in activation scoring.
2. **Gotchas section**: Add `gotchas` field. Auto-surface when skill is activated.
3. **Execution checklists**: Auto-generate from skill workflow steps. Visible in agent output.
4. **LLM-as-Judge**: Optional second model evaluates output quality. Rubric-based scoring. Integrated into verification gates.
5. **Output schema enforcement**: JSON schemas for all tier outputs. Validation before handoff.

**Success metrics**:
- Skills with exclusion clauses activate more accurately
- Gotchas prevent at least 3 common failure modes per project
- LLM-as-Judge catches semantic errors missed by rule-based gates
- Output format compliance > 95%

**Current implementation status**:
- Implemented across `rustycode-skill` and `rustycode-orchestration`
- Exclusions, gotchas, checklists, judge, and schema gates are all wired

---

## Phase Status Map

| Phase | Status | Notes |
|-------|--------|-------|
| 1: Memory Architecture | Implemented | [Phase 1 plan](2026-04-25-phase1-memory-architecture.md) |
| 2: Explore-Plan-Act Lifecycle | Implemented | [Phase 2 plan](2026-04-25-phase2-explore-plan-act.md) |
| 3: Context-Isolated Subagents | Partially Implemented | [Phase 3 plan](2026-04-25-phase3-context-isolated-subagents.md) |
| 4: Domain Context + Autonomy Levels | Partially Implemented | [Phase 4 plan](2026-04-25-phase4-domain-context-autonomy.md) |
| 5: Progressive Tooling + Hooks | Partially Implemented | [Phase 5 plan](2026-04-25-phase5-progressive-tooling-hooks.md) |
| 6: Skill Authoring + Quality | Implemented | [Phase 6 plan](2026-04-25-phase6-skill-authoring-quality.md) |

---

## Timeline

| Phase | Duration | Priority | Dependency |
|-------|----------|----------|------------|
| 1: Memory Architecture | 3-4 weeks | **Critical** | None |
| 2: Explore-Plan-Act Lifecycle | 2-3 weeks | **High** | Phase 1 (memory for plan storage) |
| 3: Context-Isolated Subagents | 2-3 weeks | **High** | Phase 2 (phases define isolation boundaries) |
| 4: Domain Context + Autonomy | 3-4 weeks | **Critical** | Phase 1 (domain context lives in memory layers) |
| 5: Progressive Tooling + Hooks | 2-3 weeks | Medium | Phase 3 (tool restriction needs isolated agents) |
| 6: Skill Authoring + Quality | 2-3 weeks | Medium | Phase 5 (skill tool scoping needs tool tiers) |
| **Total** | **14-20 weeks** | -- | Phases 1+4 can partially overlap |

**Parallelization opportunities**:
- Phases 1 and 4 can run in parallel (memory architecture and domain context are independent)
- Phases 2 and 3 can partially overlap (lifecycle and isolation are related but separable)
- Phases 5 and 6 can partially overlap (tooling and skills are related)

**Realistic timeline with parallelization**: 10-14 weeks.

---

## What This Means for RustyCode

**Today**: Strong orchestration foundation with 10K+ tests. Sophisticated permission system. Purpose-built tools. Headless execution. But memory is flat, lifecycle is unenforced, tiers share context, and domain context is absent.

**After Phases 1-6**:
- Three-layer memory with automatic consolidation
- Enforced Explore-Plan-Act lifecycle
- Context-isolated tiers with tool restrictions
- Domain-aware, autonomy-configured execution
- Progressive tool activation with 20+ lifecycle hooks
- Production-quality skill authoring with LLM-as-Judge evaluation

**The meta-insight from this analysis**: The harness matters more than the model. Every production agent (Claude Code, Codex, Aider) converges on the same patterns: tiered memory, phased execution, isolated agents, progressive permissions. RustyCode has the foundation for most of these. The gaps are architectural (memory layers, context isolation) not conceptual.

---

## Risk and Dependencies

**Critical path**: Phase 1 (Memory) blocks everything. Without proper memory, domain context has nowhere to live, plan validation has no storage, and skill context budgets cannot be enforced.

**Technical risks**:
- Phase 1: Memory architecture is fundamental; getting the three-layer model wrong would require rework across all subsequent phases
- Phase 2: Enforced phases add latency; users may resist the Explore-Plan-Act overhead
- Phase 3: Context isolation requires refactoring the orchestration pipeline (Musician/Editor/Composer currently share context)
- Phase 4: Domain context schema could grow unbounded as users request more fields
- Phase 5: Progressive tool expansion logic may mis-predict which tools are needed
- Phase 6: LLM-as-Judge adds cost and latency to every execution

**Mitigations**:
- Phase 1: Start with MEMORY.md as a simple structured file; don't over-engineer the store
- Phase 2: Make phase enforcement configurable with escape hatches
- Phase 3: Refactor one tier at a time (start with Editor as isolated reviewer)
- Phase 4: Start with a minimal schema (5-10 fields); extend based on usage
- Phase 5: Default tool set determined by telemetry, not guessing
- Phase 6: LLM-as-Judge is opt-in; not enabled by default
