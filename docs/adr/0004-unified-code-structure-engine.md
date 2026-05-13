# ADR 0004: Unified Code Structure Engine

- Status: Proposed
- Date: 2026-05-13

## Decision

Replace the three independent symbol extraction systems (`code_index`, `repo_map`, `semantic_search`) with a single code structure engine that extracts symbols from source files **preserving hierarchy** (parent-child nesting), then renders the same data into multiple output formats for different consumers.

## Context

### Problem Statement

The `indexing/` module in `rustycode-tools` contains three independent symbol extraction systems that parse the same source files to find the same symbols (functions, structs, classes, methods), but use different strategies, different type representations, and produce different output formats:

| Module | Extraction Strategy | Type Representation | Hierarchy | Tests |
|--------|-------------------|--------------------|-----------|-------|
| `code_index` | Regex string-matching | `Symbol` + `SymbolKind` (14 variants) | 1 level (`parent: Option<String>`) | 4 |
| `repo_map` | Tree-sitter AST | `SymbolInfo` + `SymbolKind` (11 variants) | None (flat `Vec`) | 3 |
| `semantic_search` | Regex string-matching | `Option<(String, String)>` per line | None | 12 |

This causes three categories of problems:

1. **Logic duplication**: ~75% of extraction logic is duplicated between `code_index/crawler.rs` (426 LOC) and `semantic_search/chunker.rs` (270 LOC) for the 5 core languages. `repo_map` does the same work via tree-sitter (626 LOC across 4 parsers) but discards the hierarchy it traverses.

2. **Hierarchy gap**: None of the three systems fully represent code nesting. `code_index` tracks one level of parent (impl/class), `repo_map` walks the tree-sitter AST hierarchically but flattens results into a `Vec<SymbolInfo>` with no parent field, and `semantic_search` has no hierarchy at all. The LSP `DocumentSymbol` protocol (used by `providers/symbol.rs`) has full recursive trees via `children: Vec<DocumentSymbol>`, but this data never flows back to the indexing systems.

3. **Bug accumulation**: Investigation found 12 confirmed bugs across the three modules (4 in `code_index`, 5 in `semantic_search`, 3 in `repo_map`). Because each module has its own extraction logic, fixes must be applied three times.

### Investigation Findings

**Type representations** (4 independent):
- `code_index::Symbol` + `SymbolKind` (14 variants) — the only public API
- `repo_map::SymbolInfo` + `SymbolKind` (11 variants) — internal
- `semantic_search` returns `Option<(String, String)>` — internal
- `agent-runtime::SymbolRef` (kind: String) — downstream mapping

**Confirmed bugs** (12):
- `code_index` (4): `pub(crate)` visibility dropped, impl reset on `}` at column 0, Python method misattribution, `export default function` not detected
- `semantic_search` (5): Generic params leak into names, comment/string false positives, `async def` not detected, `export async function` not handled, arrow functions missed
- `repo_map` (3): All impl functions are `Function` never `Method`, no macro detection, no nested function detection

**Blast radius**: Small. Only 2 files outside `indexing/` consume symbol types directly:
- `providers/explore.rs` — uses `CodeIndex` API (preserved in merge)
- `agent-runtime/intelligence.rs` — maps `CodeIndex::Symbol` to `SymbolRef`

**Consumer sites** (14 across 6 crates):
- Agent system prompt enrichment (session.rs) — `repo_map(2000)` → `# Codebase Structure`
- Per-turn context injection (session.rs) — `file_outline(path)` → `[Code context]`
- TUI workspace context builder (workspace_context.rs) — `RepoMap::build(2000)` → `## Code Structure Map`
- Headless prompt orchestrator (prompt.rs) — workspace_context via template
- Explore/inspect tools (explore.rs) — `CodeIndex` search + `symbols_overview()`
- LSP symbol tools (symbols_overview.rs) — JSON grouped by kind
- Core session storage (session.rs) — `workspace_context: String`
- Agent sub-tasks (agents/mod.rs) — workspace_context forwarding
- Prompt templates — reference code structure tools
- Prompt compressor — preserves code blocks
- Semantic chunker — code boundary detection
- LLM tool definitions — registers code structure tools
- TUI conversation service — compacts to 2200 chars

### OSS Evidence

Research into production code intelligence tools shows a clear pattern:

| Tool | Hierarchy Model | Approach |
|------|----------------|----------|
| LSP DocumentSymbol | `children: Vec<DocumentSymbol>` (recursive tree) | Industry standard for IDEs |
| Sourcegraph SCIP | Flat occurrences + `enclosing_range` for containment | Optimized for streaming/indexing |
| tree-sitter-tags | Flat captures, hierarchy reconstructed from AST ranges | Query-based extraction |
| Aider repo map | Flat tags → graph ranking → indented tree rendering | LLM-optimized with token budgets |
| VS Code outline | Uses LSP `DocumentSymbol` with `children` | Tree widget rendering |

**Key finding**: No mature project uses both tree-sitter AND regex for the same language. tree-sitter parses 500 functions in 1.76ms (C) / 6.77ms (Go) — fast enough to be the sole backend. The `tree-sitter-tags` crate has 932K downloads.

## Proposed Design

### Core Data Model

One canonical symbol type with recursive hierarchy, placed in `rustycode-protocol` (per AGENTS.md: shared types across crates go here):

```rust
// crates/rustycode-protocol/src/code_symbol.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub end_line: usize,
    pub signature: Option<String>,
    pub docs: Option<String>,
    pub visibility: Option<Visibility>,
    pub is_async: bool,
    pub is_exported: bool,
    pub children: Vec<CodeSymbol>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function, Method, Struct, Enum, Trait, Impl, Class,
    Module, Constant, TypeAlias, Variable, Macro, Interface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Crate,
    Restricted,
    Private,
}

#[derive(Debug, Clone)]
pub struct FileOutline {
    pub path: PathBuf,
    pub symbols: Vec<CodeSymbol>,  // top-level symbols; children nested inside
    pub imports: Vec<String>,
}
```

**Why `children: Vec<CodeSymbol>` over `parent: Option<String>`**:
- tree-sitter naturally produces a tree — we preserve what it gives us
- All output formats can be derived from a tree (flatten to any depth)
- LSP DocumentSymbol uses this model — we match the industry standard
- A single `parent` string only supports 1 level of nesting; recursive children support unlimited depth
- Range-based containment (SCIP-style) requires storing byte ranges and reconstruction logic — more complex with no benefit for our use case

### Extraction: One Engine

```rust
// crates/rustycode-tools/src/indexing/symbols/extract.rs

pub fn extract_file(path: &Path, content: &str) -> FileOutline {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    match Lang::from_ext(ext) {
        Some(lang) if lang.has_tree_sitter() => tree_sitter::extract(lang, path, content),
        _ => fallback::extract(ext, content),
    }
}
```

Tree-sitter for Rust, Python, JavaScript/TypeScript, Go (existing parsers moved from `repo_map/parser/`). Regex fallback for Java, C, Ruby, and other languages without grammars — but only **one copy** of the fallback, not three.

### Rendering: Multiple Formats from One Tree

The tree is rendered into different formats for different consumers. Each renderer walks the same `FileOutline` data:

**Renderer 1: Token-budgeted repo map** (system prompt, TUI workspace context)
```
src/auth/mod.rs:
  struct User
    fn new(name: String) -> Self
    fn greet(&self) -> String
  impl Display for User
    fn fmt(&self, f: &mut Formatter) -> Result
```
Indentation reflects actual nesting. Token budget truncates less-important files first.

**Renderer 2: Single-file outline** (per-turn context injection)
```
12:struct User
  25:fn new(name: String) -> Self
  37:fn greet(&self) -> String
```
No budget constraint, single file, line-number prefixed.

**Renderer 3: JSON grouped by kind** (LLM tool output)
```json
{"struct": ["User"], "method": ["new", "greet", "fmt"]}
```
Flatten tree to specified depth, group by `SymbolKind`.

**Renderer 4: Flat search index** (trigram/word search)
```rust
// Walk tree, flatten to Vec<IndexedSymbol> with parent_path: "User::new"
// for the existing trigram/word search. CodeIndex API unchanged.
```

### Module Structure

```
indexing/
├── symbols/                        ← NEW: unified extraction + data model
│   ├── mod.rs                      — re-exports CodeSymbol, SymbolKind, extract_file()
│   ├── extract.rs                  — pub fn extract_file() → FileOutline
│   ├── tree_sitter/                — primary backend (moved from repo_map/parser/)
│   │   ├── mod.rs                  — tree-sitter dispatch
│   │   ├── rust.rs                 — preserves impl → method hierarchy
│   │   ├── python.rs               — preserves class → method hierarchy
│   │   ├── javascript.rs           — preserves class → method hierarchy
│   │   └── go.rs                   — preserves receiver → method hierarchy
│   └── fallback.rs                 — regex for languages without grammars
│
├── renderers/                      ← NEW: format the tree for each consumer
│   ├── mod.rs                      — re-exports
│   ├── repo_map.rs                 — token-budgeted indented text
│   ├── file_outline.rs             — single-file outline text
│   ├── json_overview.rs            — grouped JSON
│   └── search_index.rs             — flat Vec with parent_path for search
│
├── code_index/                     ← CONSUMER: keeps search API, new backend
│   ├── mod.rs                      — CodeIndex struct (API unchanged)
│   ├── storage.rs                  — TrigramIndex, WordIndex (unchanged)
│   ├── query.rs                    — format_symbols() (unchanged)
│   └── tests.rs
│
├── semantic_search/                ← CONSUMER: keeps vector search, new backend
│   ├── mod.rs
│   ├── indexer.rs                  — calls symbols::extract_file(), uses name+kind
│   ├── searcher.rs                 — (unchanged)
│   ├── store.rs                    — (unchanged)
│   └── tests.rs
│
├── repo_map/                       ← CONSUMER: keeps token budgeting, new backend
│   ├── mod.rs                      — RepoMap struct, to_map_string()
│   ├── languages.rs                — Lang enum (unchanged)
│   └── tests.rs
│
├── providers/                      ← NEW: LLM-callable tools backed by unified layer
│   ├── find_symbol.rs              — symbol search (find_symbol tool)
│   └── code_context.rs             — symbol + context retrieval (code_context tool)
│
└── mod.rs                          — feature gates, re-exports
```

**Files deleted**: `crawler.rs` (426 LOC), `chunker.rs` (270 LOC), `repo_map/parser/generic.rs` regex paths (partial, ~80 LOC). Net reduction: ~500 LOC of duplicated extraction logic.

**Files moved**: `repo_map/parser/rust.rs`, `python.rs`, `javascript.rs`, `generic.rs` Go paths → `symbols/tree_sitter/`. Same tree-sitter code, now outputs `CodeSymbol` trees instead of flat `Vec<SymbolInfo>`.

### LLM Tool Surface

The extraction engine also powers two new LLM-callable tools that replace grep/read_file for the most common code exploration tasks:

**`find_symbol`** — structured symbol search across the codebase
```json
{
  "name": "find_symbol",
  "description": "Find functions, types, or methods by name across the project. Faster and more precise than grep for code navigation.",
  "params": {
    "query":        "string — name or partial name to search",
    "kind":         "optional enum — Function | Method | Struct | Enum | Trait | Class | ...",
    "file_pattern": "optional string — glob to restrict scope (e.g. 'src/auth/**')",
    "limit":        "number — max results (default 10)"
  }
}
```
Output (compact, ~20 tokens per result):
```
src/auth/mod.rs:42  fn verify_token(token: &str) -> Result<Claims>
src/auth/mod.rs:71  fn decode_claims(jwt: &str) -> Result<Claims>  [impl JwtProvider]
```
Backend: walks `FileOutline` trees from the in-memory `CodeIndex`.

**`code_context`** — retrieve a symbol's implementation with surrounding context
```json
{
  "name": "code_context",
  "description": "Get a symbol's full signature, doc comment, and implementation. More focused than read_file.",
  "params": {
    "file":         "string — relative file path",
    "symbol":       "string — symbol name to locate",
    "lines_around": "number — context lines above/below (default 5)"
  }
}
```
Output (~50–200 tokens, bounded by `lines_around`):
```
src/auth/mod.rs:42  fn verify_token
/// Verifies a JWT token and returns the decoded claims.
─────────────────────────────────────────
pub fn verify_token(token: &str) -> Result<Claims> {
    let claims = decode_claims(token)?;
    if claims.exp < now() { return Err(Expired); }
    Ok(claims)
}
─────────────────────────────────────────
```
Backend: locates symbol in `CodeIndex`, reads `line..end_line + lines_around` from disk.

Both tools are registered in `registry/default.rs` with `ToolPermission::Read`. Implementation lives in `providers/find_symbol.rs` and `providers/code_context.rs`.

## Migration Plan

Each step is independently mergeable with no breakage between steps.

### Step 1: Define types in rustycode-protocol (~1h)
- Add `CodeSymbol`, `SymbolKind`, `Visibility`, `FileOutline` to `rustycode-protocol`
- Add `Serialize`/`Deserialize` derives
- No behavior changes — pure type additions

### Step 2: Create symbols/ module with tree-sitter extraction (~2h)
- Create `indexing/symbols/` directory
- Move tree-sitter parsers from `repo_map/parser/` to `symbols/tree_sitter/`
- Modify each parser to output `Vec<CodeSymbol>` with `children` populated
- Add regex fallback module (`fallback.rs`) — one copy, covering Java/C/Ruby
- Wire `extract_file()` function

### Step 3: Create renderers (~1.5h)
- `repo_map.rs` renderer — walks `FileOutline` tree, produces indented text with token budget
- `file_outline.rs` renderer — walks single file's `CodeSymbol` tree, produces line-prefixed outline
- `json_overview.rs` renderer — flattens tree, groups by kind
- `search_index.rs` renderer — flattens tree with parent paths for search indexing

### Step 4: Switch repo_map consumer (~1h)
- `RepoMap::build()` calls `symbols::extract_file()` instead of direct tree-sitter
- `to_map_string()` calls `renderers::repo_map::render()`
- Delete `repo_map/parser/` (moved to `symbols/tree_sitter/`)
- Verify: system prompt and TUI workspace context render correctly

### Step 5: Switch code_index consumer (~1.5h)
- `CodeIndex::build()` calls `symbols::extract_file()` instead of `crawler.rs`
- Maps `CodeSymbol` tree → flat search index via `renderers::search_index`
- `CodeIndex` public API (`find_symbols()`, `search()`, `file_outline()`, `format_symbols()`) unchanged
- Delete `crawler.rs`
- Verify: explore tool, per-turn context injection work correctly

### Step 6: Switch semantic_search consumer (~1h)
- `indexer.rs` calls `symbols::extract_file()` instead of `chunker.rs`
- Pulls `name` + `kind.to_string()` from `CodeSymbol` tree for `CodeChunk` metadata
- Delete `chunker.rs`
- Verify: vector search results include correct symbol annotations

### Step 7: Add edge-case tests (~1.5h)
- Comments inside function bodies (should not create symbols)
- String literals containing function-like patterns
- `pub(crate)` visibility
- Generic parameters (`fn foo<T>(x: T)`)
- `export default function` (JS)
- `async def` (Python)
- Nested functions
- Macro definitions
- All tests run against the single extraction engine

### Step 8: Add LLM-callable tools (~1.5h)
- `providers/find_symbol.rs` — wraps `CodeIndex::find_symbols()`, formats results
- `providers/code_context.rs` — locates symbol, reads file range, formats output
- Register both in `registry/default.rs`
- Add tool descriptions for LLM (explain when to prefer over grep/read_file)

**Total estimated effort: ~11.5h**

## Consequences

### Positive

- **Eliminates ~500 LOC** of duplicated extraction logic
- **Fixes all 12 confirmed bugs** in one pass (single engine to fix)
- **Adds hierarchy support** — `children: Vec<CodeSymbol>` enables proper nesting visualization
- **Better repo map** — system prompt gets indented hierarchy instead of flat list
- **Better file outlines** — per-turn context shows actual nesting depth
- **Single source of truth** — one extraction to test, one to debug, one to extend with new languages
- **Aligns with planned crate extraction** — `rustycode-tools-indexing/` (already in tools README) can pull the whole `indexing/` directory as-is
- **Aligns with LSP model** — `CodeSymbol` mirrors `lsp_types::DocumentSymbol` shape
- **Faster LLM code exploration** — `find_symbol` and `code_context` give the LLM structured answers to "where is X defined?" and "what does X do?" without reading whole files or running grep

### Negative

- **tree-sitter is now required for all extraction** — regex fallback covers languages without grammars, but is less accurate. This is already the status quo for `repo_map`.
- **`CodeSymbol` is larger** than `SymbolInfo` — includes `children: Vec`, `visibility`, `is_async`, `is_exported`. For the repo map use case (token-budgeted text), most fields are unused. The overhead is small (a few KB per file) compared to the tree-sitter parse itself.
- **Migration must be incremental** — changing the data model mid-indexing could break running sessions. The step-by-step plan above ensures each step is independently mergeable.

### Risks

- **tree-sitter parser regression** — Moving parsers from `repo_map/parser/` to `symbols/tree_sitter/` could introduce subtle bugs if the `CodeSymbol` output differs from the old `Vec<SymbolInfo>` output. Mitigated by keeping the same tree-sitter query logic and adding edge-case tests in Step 7.
- **Consumer format mismatch** — If a renderer produces slightly different text than the old hardcoded formatter, downstream prompts may break. Mitigated by literally comparing old vs new output in tests before switching each consumer.

## Alternatives Considered

### Option A: Merge into new crate `rustycode-code-parsing/`
**Rejected**: Adds a new crate boundary before proving the design works. The `rustycode-tools-indexing/` crate extraction (already planned) can happen later. Starting as a module within `indexing/` is lower risk.

### Option B: Conservative tests-only approach
**Rejected**: Adding tests to three separate extraction systems doesn't fix the duplication or the hierarchy gap. The bugs will recur because the extraction logic remains triplicated.

### Option C: LSP-only extraction
**Rejected**: LSP servers are not always available (no server installed, language not supported, startup latency). tree-sitter is fast, offline, and covers the 5 core languages. LSP `DocumentSymbol` remains available as a supplemental tool for on-demand queries.

### Flat model with range containment (SCIP-style)
**Rejected**: More complex (requires byte range tracking and containment queries) with no benefit for our use case. We're not building a cross-repo index — we're building in-memory outlines for LLM context. Recursive children are simpler and match the LSP model we already consume.

## References

- `crates/rustycode-tools/src/indexing/` — current implementation
- `crates/rustycode-tools/README.md` — planned `rustycode-tools-indexing` crate extraction
- `crates/rustycode-protocol/` — target for shared types
- `docs/architecture/ARCHITECTURE-ROADMAP.md` Phase 4 — crate decomposition
- LSP Specification 3.16 — `DocumentSymbol` interface
- Sourcegraph SCIP — `enclosing_range` containment model
- tree-sitter-tags — query-based symbol extraction (932K downloads)
- Aider `repomap.py` — graph-ranked repo map with token budgeting
