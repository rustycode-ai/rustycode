# rustycode-vector-memory

Vector-based semantic memory for efficient knowledge retrieval.

## Purpose

Provides semantic search over accumulated knowledge using embeddings and vector similarity. Enables finding relevant memories based on meaning, not just keyword matching. Used for discovering patterns, learnings, and solutions from past work.

## Key Types

- `VectorMemory` — Main semantic memory system with embedding and search
- `MemoryType` — Category (Learnings, TaskTraces, CodePatterns, ToolUsage)
- `MemoryEntry` — Individual memory with content, embeddings, metadata
- `MemoryMeta` — Metadata (timestamp, source, confidence, tags)
- `SearchResult` — Memory with similarity score

## Memory Types

- **Learnings** — Team knowledge and discoveries ("tests live in /tests")
- **TaskTraces** — Execution traces of completed tasks (for pattern extraction)
- **CodePatterns** — Recurring patterns in codebase (idioms, conventions)
- **ToolUsage** — Tool patterns and edge cases learned through use

## Public API

```rust
use rustycode_vector_memory::{VectorMemory, MemoryType, MemoryEntry, MemoryMeta};
use std::path::Path;

// Create and initialize memory
let mut memory = VectorMemory::new(Path::new("/path/to/memory"))?;
memory.init()?;

// Add memory with embeddings
memory.add(
    "Test files use #[tokio::test] macro for async tests".to_string(),
    MemoryType::Learnings,
    MemoryMeta::default(),
)?;

// Semantic search
let results = memory.search("how to write async tests?", MemoryType::Learnings, 5)?;
for result in results {
    println!("Match (score {}): {}", result.score, result.entry.content);
}

// Get by ID
if let Some(entry) = memory.get("memory-id-123")? {
    println!("Memory: {}", entry.content);
}

// Delete
memory.delete("memory-id-123")?;
```

## Embedding Model

Uses **BGE-Small** (100M parameters):
- Efficient embedding generation
- 384-dimensional embeddings
- Fast similarity search
- Minimal resource usage

Similarities computed via cosine distance.

## Storage

Memories stored in JSONL format:
- One JSON object per line
- Embeddings stored alongside content
- Fast line-by-line reading for search

Format:
```json
{
  "id": "mem-123",
  "content": "Text of the memory",
  "embedding": [0.1, 0.2, ...],
  "memory_type": "learnings",
  "timestamp": "2026-04-22T10:00:00Z",
  "source": "session-456",
  "confidence": 0.85,
  "tags": ["async", "testing"]
}
```

## Features

- **Semantic Search** — Find by meaning, not just keywords
- **Multi-type** — Different memory categories
- **Metadata** — Source, confidence, tags for filtering
- **Similarity Scoring** — Ranked results with scores
- **Async Support** — Non-blocking search and insertion
- **Persistence** — Memories survive process restarts

## Dependencies

- `fastembed` — BGE embeddings with ONNX
- `serde` — JSON serialization
- `uuid` — Memory IDs
- `chrono` — Timestamps
- `anyhow` — Error handling

## Architecture Notes

Embeddings are computed once on insertion and cached. Searches scan JSONL file and compute similarity for all entries (no index yet). For large memory collections, consider implementing HNSW indexing (nearest neighbor graph).

Memory is thread-safe via Mutex. Concurrent reads allowed; writes are serialized.

## Testing

Tests verify embedding generation, similarity computation, search accuracy, and persistence.

## See Also

- `rustycode-memory` — Short-term context memory (complements vector memory)
- `rustycode-learning` — Conversation learning (extracts memories to store)
- `rustycode-observability` — Memory usage metrics
