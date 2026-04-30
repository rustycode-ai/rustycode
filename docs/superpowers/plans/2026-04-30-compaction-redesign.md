# Compaction Redesign — Design & Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Date:** 2026-04-30 | **Status:** Approved | **Scope:** `rustycode-runtime`, `rustycode-protocol`, `rustycode-session`, `rustycode-tui`

**Goal:** Replace three independent compaction implementations with a single hybrid pipeline that keeps always-present context outside the message stream, degrades through three tiers (Snip→Summarize→Truncate), and iteratively tightens until under budget.

**Architecture:** New `compaction/` module in `rustycode-runtime` replaces the unused `compaction.rs`. Shared types go in `rustycode-protocol`. TUI compaction delegates to the new pipeline. Session compaction is preserved for snapshot/restore but calls through to the new engine.

**Tech Stack:** Rust, tokio (async LLM calls), anyhow/thiserror (errors), serde (config), rustycode-llm (LLMProvider trait), rustycode-protocol (shared types), rustycode-bus (events)

---

## Part I: Design Specification

### Problem Statement

RustyCode has **three** independent compaction implementations totaling ~2,974 lines:

1. **`rustycode-runtime/src/compaction.rs`** (1,181 lines) — `CompactionService`, single-pass LLM summarization. **Currently unused** (no consumers).
2. **`rustycode-session/src/compaction.rs`** (872 lines) — `CompactionEngine`, multiple strategies, synchronous. Active in session persistence with `CompactionSnapshot` disk recovery.
3. **`rustycode-tui/src/memory/compaction.rs`** (921 lines) — `ContextMonitor`, TUI-specific. Active in event loop.

Plus a **fourth** `CompactionConfig` in `rustycode-config/src/lib.rs:436`.

All share no types, no strategies, and no test coverage overlap. The runtime implementation has no re-injection, no boundary markers, no structured format, and no degradation.

### Core Architecture

#### SessionContextBlock

Always-present context block stored **outside the message stream**, never compacted, re-injected after every compaction pass. Not stored in message history — recomputed on each turn from live zone data sources. Never serialized to disk.

**Four zones (~850 tokens total):**

| Zone | Content | Source | Est. Tokens |
|------|---------|--------|-------------|
| Environment | OS, shell, pwd, git branch, date/time | `SessionContext` | ~150 |
| Session State | Active files, recent edits, todo state | `rustycode_llm::conversation::ConversationManager` (via `rustycode_core::session`) | ~200 |
| Tools & Skills | Tool schemas, active skills, agent catalog | `ToolRegistry`, `SkillManager` | ~300 |
| Multi-Agent | Active agents, task assignments, team state | `TaskList`, team config | ~200 |

```rust
pub struct SessionContextBlock {
    environment: Box<dyn ContextZone>,
    session_state: Box<dyn ContextZone>,
    tools: Box<dyn ContextZone>,
    multi_agent: Box<dyn ContextZone>,
    cached_render: Option<String>,
    token_count: usize,
}

pub trait ContextZone: Send + Sync {
    fn render(&self) -> String;
    fn is_stale(&self) -> bool;
    fn estimated_tokens(&self) -> usize;
}
```

#### TokenBudget

Uses real `Usage.input_tokens` (u32 widened to usize) from API responses.

```rust
pub struct TokenBudget {
    pub current_input_tokens: usize,  // Usage.input_tokens as usize
    pub context_window: usize,
    pub reserved_output: usize,
    pub always_present_tokens: usize,  // ~850
    pub compaction_buffer: usize,       // 6000
}

impl TokenBudget {
    pub fn conversation_capacity(&self) -> usize {
        self.context_window
            .saturating_sub(self.reserved_output)
            .saturating_sub(self.always_present_tokens)
            .saturating_sub(self.compaction_buffer)
    }
    pub fn trigger_threshold(&self) -> usize { (self.conversation_capacity() as f64 * 0.78) as usize }
    pub fn target_size(&self) -> usize { (self.conversation_capacity() as f64 * 0.50) as usize }
    pub fn update_from_usage(&mut self, usage: &Usage) { self.current_input_tokens = usage.input_tokens as usize; }
}
```

### Compaction Pipeline

#### Three Tiers

| Tier | Method | LLM Cost | When Used |
|------|--------|----------|-----------|
| Snip | Trim tool output, remove thinking blocks | Free | First pass always |
| Summarize | LLM structured summary | Medium | When snip is insufficient |
| Truncate | Keep only N tail turns | Free | When summarize exceeds budget |

**Emergency:** If all three tiers fail within 3 passes, hard-trim to minimum viable context (system prompt + last user turn + always-present block). If no user turn, preserve last assistant turn with preceding user message.

#### Iterative Tightening Loop

**Definition of a "pass":** One measurement cycle. Snip is always applied for free before each pass. A pass = one attempt at Summarize or Truncate + measuring the result. `max_passes: 3` = up to 3 Summarize/Truncate calls total.

A "turn" = one complete user→assistant round trip including any tool_use/tool_result pairs within it.

**Pass flow:**
```
Pass 1: Snip(trim all tool output) → measure → under budget? → done
         ↓ over budget
         Summarize(Full template, tail_turns=2) → measure → under budget? → done
         ↓ over budget
Pass 2: plan.tighten() → Snip → Summarize(Compact template, tail_turns=1) → measure → done?
         ↓ over budget
Pass 3: plan.tighten() → Snip → Truncate(tail_turns=0, hard cut) → measure → done?
         ↓ still over
Emergency: minimum_viable_context()
```

#### Structured Summary Templates

- **Full** (9 sections): Primary Request, Key Concepts, Files/Code, Errors/Fixes, Problem Solving, User Messages, Pending Tasks, Current Work, Next Step
- **Compact** (5 sections): Goal, Progress, Decisions Made, Active Files, Next Step
- **Minimal** (2 sections): What the user wants, Where we are right now

Progression: `Full.degrade() → Compact`, `Compact.degrade() → Minimal`, `Minimal.degrade() → Minimal` (idempotent).

#### Boundary Markers

After compaction, message stream is rebuilt as:
```
[system: boundary marker: "--- Conversation Compact ---"]
[system: SessionContextBlock render]
[system: compaction summary]
[user/assistant: preserved tail turns]
[new messages...]
```

### Crate Integration

#### Module Structure
```
crates/rustycode-runtime/src/compaction/
├── mod.rs              // Public API: CompactPipeline, re-exports
├── context_block.rs    // SessionContextBlock + ContextZone trait
├── budget.rs           // TokenBudget
├── pipeline.rs         // CompactPipeline: tier orchestration + tightening loop
├── plan.rs             // CompactionPlan + SummaryTemplate + tightening logic
└── tiers/
    ├── mod.rs          // CompactionTier trait + TierResult
    ├── snip.rs         // SnipTier: tool output trimming
    ├── summarize.rs    // SummarizeTier: LLM structured summary
    └── truncate.rs     // TruncateTier: hard cut to tail turns
```

Shared types in `rustycode-protocol/src/compaction.rs`: `HybridCompactionConfig`, `CompactionResult`, `CompactionTierUsed`, `CompactionError`, `SummaryTemplate`.

#### Migration Strategy

1. **Phase 1-3:** Build new pipeline alongside existing code. No changes to session/TUI.
2. **Phase 4:** `CompactionService` becomes thin wrapper delegating to `CompactPipeline`. TUI updates to use new config. Session keeps `CompactionSnapshot` but delegates compaction to new engine.
3. **Cleanup:** Remove old `CompactionService` entirely. `HybridCompactionConfig` → `CompactionConfig`.

#### Observability

- `CompactionEvent` emitted on `EventBus` with `CompactionResult`, pass count, elapsed time
- Concurrent compaction guard: `AtomicBool` in `CompactPipeline`
- Summarize tier failures: API error/timeout/empty/malformed → fall through to Truncate
- Cost tracking: `info!` log of Summarize tier `Usage`

#### Future Enhancement: Context Awareness

`remaining_tokens` not available in current `Usage` struct. Once Anthropic provider exposes it, `TokenBudget` will prefer it for trigger decisions. **Not part of initial implementation.**

### Configuration

Named `HybridCompactionConfig` (to avoid collision with existing `CompactionConfig` in runtime). Rename back after migration.

```rust
impl Default for HybridCompactionConfig {
    fn default() -> Self {
        Self {
            trigger_threshold_pct: 0.78,
            target_pct: 0.50,
            max_tightening_passes: 3,
            initial_tail_turns: 2,
            max_tool_output_lines: 50,
            compaction_buffer_tokens: 6000, // Full summary (~3K) + prompt (~500) + overhead
        }
    }
}
```

Configurable via `.rustycode.toml`:
```toml
[compaction]
trigger_threshold = 0.78
target = 0.50
max_passes = 3
tail_turns = 2
max_tool_output_lines = 50
```

### Success Criteria

1. Always-present context (~850 tokens) is never sent to compaction LLM
2. After compaction, all 4 zones are re-injected — LLM retains workspace state
3. Compaction degrades gracefully through 3 passes, never crashes
4. Token counting uses real API `usage.input_tokens` when available
5. Trigger at 78% of conversation capacity, target 50% after compaction
6. All existing tests continue to pass
7. New module has 80%+ test coverage

### Out of Scope

- Server-side compaction implementation (awareness only)
- Multi-agent team context sharing across sessions
- Compaction of image/multimodal content (images preserved verbatim; only text trimmed/summarized)
- Token-level caching of unchanged context zones

---

## Part II: Implementation Plan

### File Structure

#### New Files (in `rustycode-runtime/src/compaction/`)
| File | Responsibility |
|------|---------------|
| `mod.rs` | Public API, re-exports |
| `context_block.rs` | `SessionContextBlock`, `ContextZone` trait, `StringZone` |
| `budget.rs` | `TokenBudget`, token counting, trigger logic |
| `pipeline.rs` | `CompactPipeline` — tier orchestration, tightening loop |
| `plan.rs` | `CompactionPlan`, tightening logic |
| `tiers/mod.rs` | `TierResult` struct |
| `tiers/snip.rs` | `SnipTier` — tool output trimming, thinking block removal |
| `tiers/summarize.rs` | `SummarizeTier` — LLM structured summary |
| `tiers/truncate.rs` | `TruncateTier` — hard cut to tail turns |

#### New Files (in `rustycode-protocol/src/`)
| File | Responsibility |
|------|---------------|
| `compaction.rs` | `HybridCompactionConfig`, `CompactionResult`, `CompactionTierUsed`, `SummaryTemplate`, `CompactionError` |

#### Modified Files
| File | Change |
|------|--------|
| `rustycode-runtime/src/compaction.rs` | Rename to `compaction_legacy.rs` during transition, delete after |
| `rustycode-runtime/src/lib.rs:102` | Module declaration update |
| `rustycode-protocol/src/lib.rs` | Add `pub mod compaction;` |

---

### Task 1: Protocol Compaction Types

**Files:**
- Create: `crates/rustycode-protocol/src/compaction.rs`
- Modify: `crates/rustycode-protocol/src/lib.rs` (add `pub mod compaction;`)

- [ ] **Step 1: Write `compaction.rs` with types + 7 tests**

Types: `HybridCompactionConfig` (Default, Serialize, Deserialize), `SummaryTemplate` (Full/Compact/Minimal, `degrade()`, `section_count()`), `CompactionTierUsed` enum, `CompactionResult` struct, `CompactionError` thiserror enum.

Tests: default config values, degrade Full→Compact→Minimal, Minimal idempotent, section counts, JSON serialize, JSON round-trip.

- [ ] **Step 2: Add `pub mod compaction;` to `crates/rustycode-protocol/src/lib.rs`**

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustycode-protocol -- compaction`
Expected: 7 tests PASS

- [ ] **Step 4: Commit**

```
feat(protocol): add compaction shared types
```

---

### Task 2: TokenBudget

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/budget.rs`
- Create: `crates/rustycode-runtime/src/compaction/mod.rs` (initial)
- Rename: `compaction.rs` → `compaction_legacy.rs`
- Modify: `crates/rustycode-runtime/src/lib.rs`

- [ ] **Step 1: Write `budget.rs` with 8 tests**

`TokenBudget::new(context_window, reserved_output)`, `conversation_capacity()`, `trigger_threshold()`, `target_size()`, `update_from_usage(&Usage)` (u32 as usize), `update_from_estimate(usize)`, `should_compact()`.

Tests: capacity subtracts overhead, 78% trigger, 50% target, compact when over, not when under, usage widening, estimate fallback, small window saturates to 0.

- [ ] **Step 2: Create `compaction/mod.rs` re-exporting `TokenBudget`**

- [ ] **Step 3: Rename old file and update lib.rs**

```bash
mv crates/rustycode-runtime/src/compaction.rs crates/rustycode-runtime/src/compaction_legacy.rs
```

In `lib.rs`, add `pub mod compaction_legacy;` alongside existing `pub mod compaction;` (now points to directory).

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustycode-runtime -- budget`
Expected: 8 tests PASS

- [ ] **Step 5: Commit**

```
feat(runtime): add TokenBudget and compaction module skeleton
```

---

### Task 3: CompactionPlan

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/plan.rs`
- Modify: `crates/rustycode-runtime/src/compaction/mod.rs`

- [ ] **Step 1: Write `plan.rs` with 7 tests**

`CompactionPlan::from_config(&HybridCompactionConfig)`, `tighten()` (reduces tail_turns, halves tool output lines, degrades template, disables thinking), `aggression_level()`.

Tests: initial from config, tighten reduces turns, halves lines, disables thinking, three times reaches minimal, floor on tool output lines ≥ 10, aggression level tracks template.

- [ ] **Step 2: Add `pub mod plan; pub use plan::CompactionPlan;` to mod.rs**

- [ ] **Step 3: Run tests, commit**

```
feat(runtime): add CompactionPlan with tightening logic
```

---

### Task 4: SessionContextBlock

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/context_block.rs`
- Modify: `crates/rustycode-runtime/src/compaction/mod.rs`

- [ ] **Step 1: Write `context_block.rs` with 6 tests**

`ContextZone` trait (render, is_stale, estimated_tokens, name), `StringZone` impl, `SessionContextBlock::new(4 zones)`, `render()` with XML wrapping and caching, `token_count()`, `invalidate()`.

Tests: XML wrapped output, caching, token count estimate, invalidate forces rerender, zone stale until rendered, string zone token estimate.

- [ ] **Step 2: Add re-exports to mod.rs, run tests, commit**

```
feat(runtime): add SessionContextBlock with ContextZone trait
```

---

### Task 5: SnipTier

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/tiers/mod.rs`
- Create: `crates/rustycode-runtime/src/compaction/tiers/snip.rs`
- Modify: `crates/rustycode-runtime/src/compaction/mod.rs`

- [ ] **Step 1: Write `tiers/mod.rs` with `TierResult` struct and `tiers/snip.rs` with 6 tests**

`SnipTier::new(max_tool_output_lines)`, `compact(messages) → TierResult`. Trims tool results exceeding max lines (adds "... N lines truncated ..." footer). Removes thinking blocks (`<thinking>`, `<analysis>`). Non-tool messages untouched.

Tests: short output unchanged, long output truncated with count, thinking removed, analysis removed, user/assistant preserved, tokens_removed estimated.

- [ ] **Step 2: Add tier re-exports to mod.rs, run tests, commit**

```
feat(runtime): add SnipTier — tool output trimming and thinking removal
```

---

### Task 6: TruncateTier

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/tiers/truncate.rs`
- Modify: `crates/rustycode-runtime/src/compaction/tiers/mod.rs`

- [ ] **Step 1: Write `truncate.rs` with 6 tests**

`TruncateTier::new(tail_turns)`, `compact(messages) → TierResult`. Finds user message boundaries, keeps last N complete turns (user + assistant + any tool pairs). `tail_turns=0` returns empty.

Tests: zero returns empty, keeps last 2 turns (4 messages), keeps turn with tool results, fewer messages than tail keeps all, no user messages keeps tail, tokens_removed positive.

- [ ] **Step 2: Update mod.rs, run tests, commit**

```
feat(runtime): add TruncateTier — hard cut to tail turns
```

---

### Task 7: SummarizeTier

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/tiers/summarize.rs`
- Modify: `crates/rustycode-runtime/src/compaction/tiers/mod.rs`

- [ ] **Step 1: Write `summarize.rs` with 5 tests + 3 prompt constants**

`SummarizeTier::new(template, tail_turns)`, `build_prompt()` returning FULL/COMPACT/MINIMAL prompt strings, `compact(messages, llm).await` splitting into summarize/preserve portions, calling LLM with `max_tokens: 3000`, returning summary + preserved turns.

`find_summary_split()`: finds last N user message indices.

Tests: full prompt contains sections, compact prompt, minimal prompt, split with enough turns, split with fewer turns than tail.

- [ ] **Step 2: Update mod.rs, run tests, commit**

```
feat(runtime): add SummarizeTier — LLM structured summary
```

---

### Task 8: CompactPipeline

**Files:**
- Create: `crates/rustycode-runtime/src/compaction/pipeline.rs`
- Modify: `crates/rustycode-runtime/src/compaction/mod.rs`

- [ ] **Step 1: Write `pipeline.rs` with 4 tests**

`CompactPipeline::new(config)`, `compact(messages, budget, context_block, llm).await`. Concurrent guard via `AtomicBool`. Iterative tightening: for each pass, Snip free → measure → Summarize (if not last pass) → measure → tighten if over → next pass → Truncate on last pass → emergency trim.

`emergency_trim()`: keep last user turn + following assistant. No user → keep last 2 messages.

`estimate_tokens()`: chars / 4 across all messages.

Tests: emergency trim keeps last user turn, no user keeps last 2, estimate tokens, concurrent guard blocks.

- [ ] **Step 2: Add `pub mod pipeline; pub use pipeline::CompactPipeline;` to mod.rs**

- [ ] **Step 3: Run tests, commit**

```
feat(runtime): add CompactPipeline with iterative tightening
```

---

### Task 9: TUI Integration Shim

**Files:**
- Create: `crates/rustycode-tui/src/compaction_context.rs`
- Modify: `crates/rustycode-tui/src/memory/compaction.rs` (add import)

- [ ] **Step 1: Create adapter**

`CompactPipelineAdapter` bridges TUI's existing compaction config to `HybridCompactionConfig` and holds a `CompactPipeline`.

- [ ] **Step 2: Run TUI tests, commit**

```
feat(tui): add CompactPipelineAdapter for hybrid compaction
```

---

### Task 10: Cleanup Legacy

**Files:**
- Delete: `crates/rustycode-runtime/src/compaction_legacy.rs`
- Modify: `crates/rustycode-runtime/src/lib.rs` (remove legacy module)

- [ ] **Step 1: Remove legacy file and module declaration**

- [ ] **Step 2: Run `cargo test -p rustycode-runtime` and `cargo clippy -p rustycode-runtime -- -D warnings`**

- [ ] **Step 3: Commit**

```
chore(runtime): remove legacy compaction.rs
```

---

### Task 11: Final Verification

- [ ] **Step 1:** `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings
- [ ] **Step 2:** `cargo test --workspace` — all tests pass
- [ ] **Step 3:** Verify module structure: 9 files in `compaction/`
- [ ] **Step 4:** Count new tests: ~45+
- [ ] **Step 5:** Final commit

```
feat: hybrid compaction pipeline complete — snip/summarize/truncate tiers
```
