# Semantic Search Design for RustyCode

## Overview

Build a **local, embedding-based semantic code search** that complements (not replaces) grep and LSP tools.

## Key Design Decisions

### 1. When to Use Semantic Search vs grep vs LSP

| Query Type | Best Tool | Why |
|------------|-----------|-----|
| "find the auth middleware" | **semantic_search** | User doesn't know function name |
| "where is `validate_jwt` defined?" | **lsp_definition** | Exact symbol, LSP is instant |
| "show me all JWT validation code" | **semantic_search** | Intent-based, may span files |
| "find files with 'auth' in the name" | **glob** | Filename pattern |
| "grep for `Unauthorized` responses" | **grep** | Exact text pattern |
| "how do we handle rate limiting?" | **semantic_search** | Conceptual query |
| "all usages of `RateLimiter`" | **lsp_references** | Exact symbol references |
| "what's the type of this variable?" | **lsp_hover** | LSP provides type info |
| "show me error handling patterns" | **semantic_search** | Pattern matching by intent |

### 2. Activation Strategy

**Automatic fallback chain** (agent decides based on query):
```
User query → Analyze intent → Choose tool
  │
  ├─ Contains exact symbol name? → lsp_*
  ├─ Looks like a file pattern? → glob
  ├─ Contains regex-like syntax? → grep
  └─ Natural language intent? → semantic_search
```

**User-triggered** via:
- `/semantic <query>` - explicit semantic search
- "find code that..." - natural language triggers
- "where do we..." - intent-based queries
- "how does X work?" - conceptual questions

### 3. Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   rustycode-tools                        │
│  ┌─────────────────────────────────────────────────┐    │
│  │           SemanticSearchTool                     │    │
│  │  - Analyzes query intent                         │    │
│  │  - Routes to appropriate search backend          │    │
│  └─────────────────────────────────────────────────┘    │
│                          │                               │
│         ┌────────────────┼────────────────┐             │
│         │                │                │             │
│         ▼                ▼                ▼             │
│   ┌──────────┐   ┌──────────┐   ┌──────────────┐       │
│   │   grep   │   │  LSP     │   │  Semantic    │       │
│   │  (text)  │   │ (symbol) │   │  (embedding) │       │
│   └──────────┘   └──────────┘   └──────────────┘       │
│                                        │                 │
└────────────────────────────────────────┼────────────────┘
                                         │
                        ┌────────────────▼──────────────┐
                        │   rustycode-vector-memory     │
                        │   - BGE-Small embeddings      │
                        │   - Cosine similarity search  │
                        │   - Persistent index (.json)  │
                        └───────────────────────────────┘
```

### 4. Index Strategy

**What to index:**
- Function/method bodies (parsed via Tree-sitter or regex heuristics)
- Docstrings and comments
- Type definitions
- Module/class names

**Chunk size:** 50-200 lines (semantic units, not arbitrary chunks)

**Index location:** `.rustycode/semantic_index/` per-project

**Index update:**
- Lazy: Index on first search if not present
- Incremental: Watch for file changes (like `mgrep watch`)
- Manual: `/reindex` command to force rebuild

### 5. Query Processing

**Pipeline:**
```
1. User query → Parse intent
2. Generate embedding (BGE-Small, 384-dim)
3. Cosine similarity search against index
4. Rerank by:
   - Score threshold (>0.7 = high confidence)
   - Recency (newer files weighted higher)
   - File type (.rs > .md for code queries)
5. Return top-k results with context
```

### 6. Result Format

```markdown
**Semantic Search Results for: "how do we validate JWT tokens?"**

📄 **crates/rustycode-auth/src/jwt.rs:45-78** (score: 0.89)
```rust
/// Validates a JWT token against the configured secret
/// Returns Ok(TokenData) on success, Err(JwtError) on failure
pub fn validate_jwt(token: &str, secret: &SecretKey) -> Result<TokenData, JwtError> {
    // ... implementation
}
```

📄 **crates/rustycode-auth/src/middleware.rs:112-145** (score: 0.82)
```rust
/// Authentication middleware that validates JWT from Authorization header
async fn auth_middleware(req: Request) -> Result<Response, AuthError> {
    let token = extract_bearer_token(&req)?;
    let data = validate_jwt(&token, &state.secret)?;
    // ...
}
```

⚡ Indexed 1,234 chunks from 89 files | Search: 47ms
```

### 7. When NOT to Use Semantic Search

- **Small codebases (<10 files)**: grep is faster, no index needed
- **Exact symbol lookups**: LSP is instant and precise
- **Regex patterns**: grep handles complex patterns better
- **Binary files**: Not indexable
- **Generated code**: May change frequently, index staleness issues

### 8. Performance Targets

| Metric | Target |
|--------|--------|
| Index build (10k files) | <30 seconds |
| Query latency | <500ms |
| Index size overhead | <5% of codebase size |
| Memory usage | <200MB for 10k file index |
| Precision@5 | >80% relevant results |

### 9. Implementation Phases

**Phase 1: Core Infrastructure** ✅ COMPLETE
- [x] Add `rustycode-vector-memory` dependency to `rustycode-tools`
- [x] Create `CodeEmbedder` wrapper around fastembed
- [x] Build file chunker (split files into semantic units)
- [ ] Create index persistence layer (deferred - lazy rebuild is sufficient)

**Phase 2: Search Tool** ✅ COMPLETE
- [x] Implement `SemanticSearchTool` with query intent analysis
- [x] Add routing logic (semantic vs grep vs LSP) - `route_query()` function
- [x] Format results with context and scores

**Phase 3: Index Management** ✅ PARTIAL (lazy indexing complete)
- [x] Lazy indexing (build on first query)
- [ ] File watcher for incremental updates
- [ ] Cache invalidation on file changes
- [ ] `/reindex` slash command (manual trigger via `rebuild_index()` method available)

**Phase 4: Optimization** ✅ PARTIAL (multi-language complete)
- [ ] Batch embedding computation
- [ ] ANN search (if brute-force becomes slow)
- [ ] Hybrid scoring (semantic + keyword match)
- [x] Multi-language support (Rust, Python, Java, Go, JavaScript/TypeScript)

---

## Comparison with Existing Tools

| Feature | mgrep | goose | rustycode (proposed) |
|---------|-------|-------|---------------------|
| Embeddings | Cloud API | API via provider | **Local (fastembed)** |
| Index | Cloud vector store | N/A | **Local JSON** |
| Language | TS/Node.js | Rust | **Rust** |
| Privacy | Data sent to cloud | Depends on provider | **100% local** |
| Setup | npm install + API key | Configure provider | **cargo build** |
| Cost | API usage | API usage | **Free (one-time download)** |

---

## Integration Points

1. **Agent Mode Selection**: Agent can auto-choose semantic search for intent queries
2. **Slash Commands**: `/semantic`, `/reindex`
3. **Tool Routing**: `SemanticSearchTool` can delegate to grep/LSP when appropriate
4. **Ensemble Memory**: Store successful search patterns in `VectorMemory`
5. **TUI Display**: Show semantic search results in chat panel with code previews
