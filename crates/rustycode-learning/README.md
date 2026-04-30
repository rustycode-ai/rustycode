# rustycode-learning

Conversation learning and memory extraction for RustyCode.

## Purpose

Analyzes conversations and execution traces to extract learnings, patterns, and insights. Automatically captures team knowledge from sessions and stores in memory for future reuse.

## Key Types

- `LearningExtractor` — Main extractor that analyzes conversations
- `Learning` — Extracted insight with confidence
- `LearningType` — Category (Pattern, Failure, Success, Convention, Edge Case)
- `ExtractionContext` — Context about where learning came from
- `ExtractionResult` — Results with extracted learnings

## Learning Types

- **Pattern** — Recurring code or workflow pattern
- **Failure** — Bug or error pattern to avoid
- **Success** — Successful pattern or approach
- **Convention** — Team convention or standard (naming, structure)
- **Edge Case** — Edge case or corner case discovered

## Public API

```rust
use rustycode_learning::{LearningExtractor, ExtractionContext};

// Create extractor
let extractor = LearningExtractor::new()?;

// Analyze conversation for learnings
let context = ExtractionContext::from_conversation(
    "sess_123",
    &messages,
    &session_state
)?;

let learnings = extractor.extract(&context)?;

// Store extracted learnings
for learning in learnings {
    if learning.confidence > 0.7 {
        // Store in vector memory or memory system
        memory.add(learning.text.clone(), learning.learning_type)?;
    }
}
```

## Extraction Process

1. **Message Analysis** — Parse conversation for patterns
2. **Code Review** — Analyze code changes and commits
3. **Error Analysis** — Extract lessons from failures
4. **Convention Detection** — Identify team conventions
5. **Pattern Recognition** — Spot recurring patterns
6. **Confidence Scoring** — Rate extraction confidence (0.0–1.0)
7. **Storage** — Save to memory system

## Features

- **Automatic Extraction** — No manual annotation needed
- **Confidence Scoring** — Know which learnings are reliable
- **Multi-type** — Different learning categories
- **Context Preservation** — Remember where learning came from
- **Duplicate Avoidance** — Don't store duplicates
- **Privacy** — Filter sensitive data before storing

## Dependencies

- `regex` — Pattern matching in text
- `serde` — Serialization
- `rustycode-protocol` — Core types
- `rustycode-memory` — Store learnings
- `rustycode-vector-memory` — Semantic memory storage
- `anyhow` — Error handling

## Architecture Notes

Learning extraction happens asynchronously after sessions complete. Extraction runs in background without blocking session.

Confidence scoring is heuristic-based:
- High confidence: Explicit in conversation ("we always do X")
- Medium: Inferred from patterns (repeated X three times)
- Low: Speculative (might extract false patterns)

Filters prevent storing secrets, personal data, or non-generalizable insights.

## Testing

Tests verify extraction accuracy, confidence scoring, and filtering.

## See Also

- `rustycode-memory` — Short-term memory (context)
- `rustycode-vector-memory` — Long-term semantic memory
- `rustycode-core` — Session management
- `rustycode-observability` — Learning metrics
