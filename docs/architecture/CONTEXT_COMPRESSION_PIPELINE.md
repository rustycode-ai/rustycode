# Context Compression Pipeline — Design Spec

**Date:** 2026-05-05
**Status:** Draft
**Affects crates:** rustycode-orchestration, rustycode-llm, rustycode-protocol, rustycode-tui, rustycode-core

## Problem

RustyCode sessions accumulate context faster than models can absorb it. Long-running TUI sessions, headless autonomous runs, and benchmark tasks all hit context limits. The existing `rustycode-orchestration/src/context/` module has 8 fully-implemented submodules (~4,500 LOC) handling compression, chunking, caching, budgeting, and token counting — but none of them are wired into any model call path. The compression infrastructure exists; the pipeline does not.

## Design Goals

1. Wire existing modules into a staged pipeline triggered before each model call
2. Preserve-then-summarize: split conversation into MUST-KEEP / COMPRESSIBLE / DISCARDABLE before any compression runs
3. Model-aware: different aggressiveness per model tier (local 7B vs Opus 4)
4. Provider-aware: cache optimization stages only for Anthropic (where prompt caching is available)
5. Circuit breaker: per-stage failure tracking, skip failing stages after 3 consecutive failures
6. Target all execution paths: TUI, headless, bench

## Architecture

### Pipeline Stages

```
┌─────────────────────────────────────────────────────┐
│  Stage 0: Relevance Scoring (conversation hijack)   │
│  Inject eval prompt → parse scores → strip exchange │
├─────────────────────────────────────────────────────┤
│  Stage 1: Budget Reduction                          │
│  Module: context_budget                             │
│  Compute token budgets per section, truncate        │
├─────────────────────────────────────────────────────┤
│  Stage 2: Tool Snipping                             │
│  Module: prompt_compressor                          │
│  Replace verbose tool outputs with summaries        │
├─────────────────────────────────────────────────────┤
│  Stage 3: Microcompact                              │
│  Module: summary_distiller                          │
│  Compress consecutive low-relevance turns           │
├─────────────────────────────────────────────────────┤
│  Stage 4: Context Collapse                          │
│  Module: semantic_chunker + prompt_compressor       │
│  TF-IDF scoring, keep top-K chunks by relevance     │
├─────────────────────────────────────────────────────┤
│  Stage 5: Cache Optimization (Anthropic only)       │
│  Module: prompt_cache_optimizer                     │
│  Reorder sections: static prefix → dynamic suffix   │
└─────────────────────────────────────────────────────┘
```

### Preserve-then-Summarize Pattern

Before any stage runs, the pipeline splits the conversation:

```
relevance_scores = stage_0_score(messages)     // conversation hijack
(must_keep, compressible, discardable) = partition(messages, relevance_scores)
compressed = pipeline_stages_1_through_5(compressible)
final = reassemble(must_keep, compressed)       // MUST-KEEP blocks never modified
```

This prevents the common failure mode where compaction removes something it knows it needs.

### Stage 0: Conversation-Hijack Relevance Scoring

**Novel mechanism** — validated with GLM-5.1 via z.ai (2026-05-05).

Instead of a separate OOB call, inject an ephemeral evaluation prompt into the conversation flow:

1. **Inject**: Append a user message asking the LLM to rate each turn's relevance (1-5) to the current task
2. **Parse**: Extract JSON array of `{index, score, reason}` from the response
3. **Strip**: Remove both the injected prompt and response from conversation history
4. **Store**: Attach scores as metadata on each message (`MessageMetadata::relevance_score`)

```
Conversation: [msg0, msg1, ..., msgN]
                     ↓ inject
Conversation: [msg0, msg1, ..., msgN, eval_prompt]
                     ↓ LLM response
Conversation: [msg0, msg1, ..., msgN, eval_prompt, eval_response]
                     ↓ strip + store scores
Conversation: [msg0, msg1, ..., msgN]  // each msg now has relevance_score metadata
```

**Live test results (GLM-5.1):**
- Correctly scored active task as 5, completed doc updates as 1
- More nuanced than heuristic mock: system prompt scored 2 (generic), not 5
- JSON parseable after stripping markdown fences
- Partitioning: 7% must-keep, 70% compressible, 21% discardable

**When to run Stage 0:**
- Only when context usage exceeds 60% (don't waste tokens when context is fresh)
- Not during fast sequential tool calls (batch every 5+ turns instead)
- Skipped entirely if the model context window > 200K tokens AND usage < 50%

### Model-Aware Configuration

```rust
struct CompressionProfile {
    /// Trigger: start compression when context usage exceeds this percentage
    trigger_threshold_pct: u8,       // default: 85
    /// Stages to run (0-5), in order
    stages: Vec<CompressionStage>,
    /// Aggressiveness for prompt_compressor stages
    compression_level: CompressionLevel,
    /// Per-model overrides
    model_overrides: HashMap<String, ModelCompressionOverride>,
}

struct ModelCompressionOverride {
    /// Override trigger threshold for specific model
    trigger_threshold_pct: Option<u8>,
    /// Override compression level
    compression_level: Option<CompressionLevel>,
    /// Skip specific stages
    skip_stages: Vec<CompressionStage>,
}
```

**Tier-based defaults:**

| Tier | Trigger | Level | Stages | Rationale |
|------|---------|-------|--------|-----------|
| Local 7B | 70% | Aggressive | 1-4 | Tiny context, must compress hard |
| Mid (Sonnet-class) | 80% | Moderate | 0-4 | Good but finite context |
| Large (Opus-class) | 85% | Light | 0-3, skip 4 | Rarely needs collapse |
| Anthropic | 85% | Light | 0-5 | Cache optimization is valuable |

### Hybrid Trigger

Two mechanisms, either can trigger compression:

1. **Percentage-based**: `current_tokens / context_window > threshold_pct`
2. **Fixed budget**: `remaining_tokens < MIN_BUDGET_TOKENS` (default: 4,096)

Whichever fires first wins. The fixed budget prevents edge cases where a large context window percentage still leaves insufficient room for a response.

### Circuit Breaker

```rust
struct StageHealth {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
}

// After 3 consecutive failures, skip the stage for 5 minutes
// Reset on success
```

Prevents a broken stage from wasting tokens on repeated failures.

## Wiring Points

### Primary: `orchestrator.rs:execute_step`

The pipeline runs **before** the model call, not inside providers. This keeps providers clean and lets the orchestrator make informed decisions.

```rust
// In execute_step (simplified):
fn execute_step(&mut self) -> Result<StepOutcome> {
    let usage = self.context_tracker.current_usage();
    let profile = self.compression_profile.for_model(&self.model_id);

    if usage.should_compress(&profile) {
        let messages = self.history.messages();
        let compressed = self.compression_pipeline.run(
            messages,
            &profile,
            &self.provider,  // for token counting
        )?;
        self.history.replace_messages(compressed);
    }

    // ... proceed with model call as normal
}
```

### Secondary: `streaming/tool_execution.rs`

For long TUI sessions with many tool calls, check compression need after each tool result is appended. This catches gradual context growth between orchestrator steps.

### Not in providers

Providers (`openai.rs`, `anthropic.rs`) stay clean — they receive already-compressed messages. This avoids coupling compression to provider implementations.

## New Types (in rustycode-protocol)

### `MessageMetadata` extension

```rust
// In message.rs, add to MessageMetadata:
pub relevance_score: Option<u8>,  // 1-5 from conversation hijack, None = not scored
```

### Pipeline types (in rustycode-orchestration)

```rust
// New file: context/pipeline.rs

pub struct CompressionPipeline {
    stage_health: HashMap<CompressionStage, StageHealth>,
    profile: CompressionProfile,
}

pub enum CompressionStage {
    RelevanceScoring = 0,  // conversation hijack
    BudgetReduction = 1,
    ToolSnipping = 2,
    Microcompact = 3,
    ContextCollapse = 4,
    CacheOptimization = 5,
}

pub struct PipelineResult {
    pub messages: Vec<Message>,
    pub stages_run: Vec<CompressionStage>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub stage_timings: HashMap<CompressionStage, Duration>,
}

pub struct StageHealth {
    pub consecutive_failures: u32,
    pub last_failure: Option<Instant>,
}
```

## File Changes

| File | Change |
|------|--------|
| `crates/rustycode-protocol/src/message.rs` | Add `relevance_score: Option<u8>` to `MessageMetadata` |
| `crates/rustycode-orchestration/src/context/pipeline.rs` | **New**: Pipeline orchestrator, stage runner, circuit breaker |
| `crates/rustycode-orchestration/src/context/mod.rs` | Re-export pipeline types |
| `crates/rustycode-orchestration/src/context/relevance_scorer.rs` | **New**: Conversation-hijack prompt builder, response parser, partition logic |
| `crates/rustycode-orchestration/src/lib.rs` | Re-export pipeline types |
| `crates/rustycode-orchestration/src/orchestrator.rs` | Wire pipeline check into `execute_step` |
| `crates/rustycode-tui/src/app/streaming/tool_execution.rs` | Secondary trigger check after tool results |
| `crates/rustycode-core/src/runtime/mod.rs` | Headless runtime trigger |

### Existing modules (no changes needed, just wired)

- `context_budget.rs` → Stage 1
- `prompt_compressor.rs` → Stages 2, 4
- `summary_distiller.rs` → Stage 3
- `semantic_chunker.rs` → Stage 4
- `prompt_cache_optimizer.rs` → Stage 5
- `token_counter.rs` → Used throughout for trigger checks

## Testing Strategy

### Unit tests (per-stage)

Each stage tested independently with fixed inputs:
- Stage 0: mock LLM response → parse scores → verify partition
- Stage 1: oversized context → budget computation → verify truncation at section boundary
- Stage 2: verbose tool outputs → snipping → verify summaries preserve key info
- Stage 3: consecutive low-relevance turns → distillation → verify compression ratio
- Stage 4: mixed-relevance chunks → TF-IDF scoring → verify top-K selection
- Stage 5: unsorted sections → cache optimization → verify static-first ordering

### Integration tests

- Full pipeline with `MockProvider`: verify end-to-end compression preserves must-keep messages
- Circuit breaker: inject stage failure → verify skip after 3 consecutive failures
- Model-aware: verify different profiles produce different compression levels
- Preserve-then-summarize: verify MUST-KEEP messages are never modified

### Live validation

- Conversation-hijack scoring via z.ai (GLM-5.1): **DONE** (2026-05-05)
- Anthropic cache optimization: manual test with Claude API

## Out of Scope

- **Summarization via separate LLM call**: The conversation-hijack approach avoids this cost. Could add later for high-accuracy scenarios.
- **Persistent compression state across sessions**: Compression is per-session, starts fresh each time.
- **User-configurable compression profiles**: Start with hardcoded tier defaults, add config file later.
- **Compression quality metrics**: Track token savings ratio, but don't optimize for it during V1.

## Validation Evidence

### Live Test: GLM-5.1 via z.ai (2026-05-05)

**Script**: `scripts/test_relevance_eval.py`
**Result**: LLM returned parseable relevance scores with nuanced ratings.

```
Input:  10-turn coding conversation (1,186 chars)
Output: 10 relevance scores (1-5 scale)

Partitioning:
  MUST-KEEP (score ≥ 4):    1 turn   (93 chars, 7%)
  COMPRESSIBLE (score 2-3): 7 turns  (840 chars, 70%)
  DISCARDABLE (score < 2):  2 turns  (253 chars, 21%)
```

The LLM correctly identified:
- Active task (rate limiting) as score 5 — the only must-keep
- Structural context (auth.rs location, test patterns) as score 3 — compressible reference
- Completed doc updates as score 1 — discardable

This confirms the conversation-hijack approach produces useful compression guidance without a separate OOB call.
