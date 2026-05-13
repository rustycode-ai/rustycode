//! Semantic Code Search with Embeddings
//!
//! This module provides semantic search over codebases using BGE-Small embeddings
//! via fastembed. It complements grep (keyword search) and LSP (symbol lookup) by
//! enabling intent-based queries like "find auth validation logic".

mod indexer;
mod searcher;
mod store;

pub use indexer::CodeIndexer;
pub use store::{CodeChunk, IndexMetadata, SearchResult, SemanticIndex};

use anyhow::Result;
use rustycode_tools_api::{ToolOutput, ToolPermission};
use schemars::JsonSchema;
use serde::Deserialize;

use super::semantic_search_state;
use searcher::{estimate_tokens, format_compact, format_full, format_minimal, should_auto_compact};

/// Parameters for semantic search tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// Natural language description of code to find. Examples:
    /// - 'find user authentication middleware'
    /// - 'how do we validate JWT tokens?'
    /// - 'show me database connection pooling logic'
    /// - 'where is rate limiting implemented?'
    ///
    /// Good queries describe INTENT, not exact symbols.
    pub query: String,

    /// Maximum number of results to return (default: 5, range: 1-20)
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Use compact output format to reduce token usage (default: false).
    /// Compact format: 'file:line (score) symbol | preview'.
    /// Auto-enabled for broad queries or top_k > 10.
    #[serde(default)]
    pub compact: bool,

    /// Use ultra-compact format: just file references without previews (default: false).
    /// Maximum token savings (~95%).
    #[serde(default)]
    pub minimal: bool,
}

fn default_top_k() -> usize {
    5
}

// -- Tool defined via macro ---------------------------------------------------

rustycode_tools_api::define_tool! {
    pub struct SemanticSearchTool;

    name: "SemanticSearch",
    description: r#"Search code by **intent/meaning** using AI embeddings (not keyword matching).

## When to use:
- **Conceptual queries**: "find auth validation logic", "how do we handle rate limiting?"
- **Unknown symbol names**: "where is JWT token validated?" (don't know function name)
- **Pattern discovery**: "show me error handling patterns", "how are database connections managed?"
- **Cross-file searches**: "find all logging configuration", "where are API routes defined?"

## When NOT to use:
- **Exact symbol lookup**: Use `lsp_definition` for "where is `validate_jwt` defined?"
- **Exact text patterns**: Use `grep` for specific strings, regex patterns
- **File names**: Use `glob` for "*.rs", "src/**/*.ts"

## Examples:
- "find user authentication middleware"
- "how do we validate JWT tokens?"
- "show me database connection pooling logic"
- "where is `UserService`?" -> use `lsp_definition`
- "grep for 'Unauthorized'" -> use `grep`

## Parameters:
- `query` (required): Natural language description of what to find
- `top_k` (optional, default 5): Max results to return (1-20)
- `compact` (optional, default false): Use compact format (~80% token savings)
- `minimal` (optional, default false): Use ultra-compact format (~95% token savings)

## Token Savings:
| Format | Tokens (5 results) | Use case |
|--------|-------------------|----------|
| **Full** | ~500-800 | Detailed analysis, first-time exploration |
| **Compact** | ~50-80 | Broad searches, iterative discovery |
| **Minimal** | ~20-30 | Scanning many results, reference lookup |

**Tip**: Use `compact: true` for broad queries. Auto-enabled for short queries or top_k > 10."#,
    permission: ToolPermission::Read,

    execute(params: SemanticSearchParams, ctx) {
        let session_id = ctx.session_id.as_deref().unwrap_or("default-session");
        let state = semantic_search_state::get_search_state(session_id)
            .ok_or_else(|| anyhow::anyhow!(
                "No search state configured for session '{session_id}'. \
                 This tool requires the TUI layer to set up semantic search state."
            ))?;

        let query = &params.query;
        let top_k = params.top_k;
        let compact_requested = params.compact;
        let minimal_requested = params.minimal;

        // Auto-compact for broad queries or large result sets
        let use_compact =
            compact_requested || minimal_requested || should_auto_compact(query, top_k);

        // Search
        let results = search_index(&state, query, top_k)?;

        if results.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No results found for query: '{}'\n\nTip: Try rephrasing your query or using grep for exact text patterns.",
                query
            )));
        }

        // Format results based on format flags
        let output = if minimal_requested {
            format_minimal(&state.project_root, &results)
        } else if use_compact {
            let meta_guard = state.metadata.lock();
            let meta = meta_guard.as_ref();
            format_compact(&state.project_root, query, &results, meta)
        } else {
            let meta_guard = state.metadata.lock();
            let meta = meta_guard.as_ref();
            format_full(&state.project_root, query, &results, meta)
        };

        // Add token estimation comment for compact/minimal formats
        let token_estimate = estimate_tokens(&output);
        let output_with_meta = if use_compact || minimal_requested {
            format!("{} [~{} tokens]\n", output, token_estimate)
        } else {
            output
        };

        Ok(ToolOutput::text(output_with_meta))
    }
}

// -- Free functions operating on session state --------------------------------

/// Get or build the index, returning search results
fn search_index(
    state: &semantic_search_state::SearchState,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>> {
    // Check if index exists and search
    {
        let guard = state.index.lock();
        if let Some(ref index) = *guard {
            if !index.is_empty() {
                return index.search(query, top_k);
            }
        }
    }

    // Build index
    let indexer = CodeIndexer::new();
    let new_index = indexer.index_directory(&state.project_root)?;
    let metadata = new_index.metadata().clone();

    // Replace index and search
    let mut index_guard = state.index.lock();
    *index_guard = Some(new_index);

    let mut meta_guard = state.metadata.lock();
    *meta_guard = Some(metadata);

    index_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Failed to build index"))?
        .search(query, top_k)
}

/// Force rebuild the index
#[allow(dead_code)] // Kept for future use
pub(crate) fn rebuild_index(state: &semantic_search_state::SearchState) -> Result<IndexMetadata> {
    let indexer = CodeIndexer::new();
    let new_index = indexer.index_directory(&state.project_root)?;
    let metadata = new_index.metadata().clone();

    let mut index_guard = state.index.lock();
    *index_guard = Some(new_index);

    let mut meta_guard = state.metadata.lock();
    *meta_guard = Some(metadata.clone());

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::searcher::cosine_similarity;
    use super::store::SemanticIndex;
    use super::*;
    use rustycode_tools_api::{route_query, SearchStrategy};
    use std::path::PathBuf;

    #[test]
    fn test_index_add_and_search() {
        let mut index = SemanticIndex::new().unwrap();

        index
            .add_chunk(CodeChunk {
                file_path: PathBuf::from("src/main.rs"),
                start_line: 1,
                end_line: 10,
                content:
                    "fn authenticate_user(token: &str) -> Result<User> { /* JWT validation */ }"
                        .into(),
                language: "rs".into(),
                symbol_name: Some("authenticate_user".to_string()),
                symbol_type: Some("function".to_string()),
            })
            .unwrap();

        index
            .add_chunk(CodeChunk {
                file_path: PathBuf::from("src/lib.rs"),
                start_line: 1,
                end_line: 10,
                content: "pub struct Config { database_url: String }".into(),
                language: "rs".into(),
                symbol_name: Some("Config".to_string()),
                symbol_type: Some("struct".to_string()),
            })
            .unwrap();

        let results = index.search("user authentication token", 5).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].chunk.content.contains("authenticate_user"));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        let d = vec![0.0, 1.0, 0.0];
        let sim_orthogonal = cosine_similarity(&c, &d);
        assert!((sim_orthogonal - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_should_auto_compact() {
        // Large top_k should trigger auto-compact
        assert!(should_auto_compact("auth", 15));
        assert!(!should_auto_compact("authentication middleware", 5));

        // Broad keywords should trigger auto-compact
        assert!(should_auto_compact("all authentication code", 5));
        assert!(should_auto_compact("overview of security", 5));
        assert!(should_auto_compact("summary of logging", 5));

        // Short queries should trigger auto-compact
        assert!(should_auto_compact("auth", 5)); // short + 1 word
        assert!(should_auto_compact("jwt", 5)); // short + 1 word
        assert!(!should_auto_compact("jwt validation logic", 5)); // 3 words

        // Specific queries should not trigger
        assert!(!should_auto_compact("where is validate_jwt defined", 5));
        assert!(!should_auto_compact(
            "how does authentication middleware work",
            5
        ));
    }

    #[test]
    fn test_format_minimal() {
        let mock_results = vec![SearchResult {
            chunk: CodeChunk {
                file_path: PathBuf::from("src/auth/mod.rs"),
                start_line: 45,
                end_line: 55,
                content: "fn validate_jwt(token: &str) -> Result<User> { }".into(),
                language: "rs".into(),
                symbol_name: Some("validate_jwt".to_string()),
                symbol_type: Some("function".to_string()),
            },
            score: 0.87,
        }];

        use std::env;
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("."));

        let minimal = format_minimal(&current_dir, &mock_results);
        let compact = format_compact(&current_dir, "jwt", &mock_results, None);
        let full = format_full(&current_dir, "jwt", &mock_results, None);

        // Verify minimal format
        assert!(minimal.contains("src/auth/mod.rs:45"));
        assert!(minimal.contains("[validate_jwt]"));
        assert!(!minimal.contains("```"));
        assert!(!minimal.contains("|"));

        // Verify token savings: minimal < compact < full
        let minimal_chars = minimal.len();
        let compact_chars = compact.len();
        let full_chars = full.len();

        assert!(
            minimal_chars < compact_chars,
            "minimal={} vs compact={}",
            minimal_chars,
            compact_chars
        );
        assert!(
            compact_chars < full_chars,
            "compact={} vs full={}",
            compact_chars,
            full_chars
        );
    }

    #[test]
    fn test_estimate_tokens() {
        let test_str = "hello world this is a test";
        let estimated = estimate_tokens(test_str);

        assert_eq!(estimated, 6);
    }

    #[test]
    fn test_compact_format_basic() {
        // Test compact format with mock results
        use std::env;
        let current_dir = env::current_dir().unwrap_or(PathBuf::from("."));

        let mock_results = vec![
            SearchResult {
                chunk: CodeChunk {
                    file_path: PathBuf::from("src/auth/mod.rs"),
                    start_line: 45,
                    end_line: 55,
                    content: "fn validate_jwt(token: &str) -> Result<User> { /* validates JWT token and returns user */ }".into(),
                    language: "rs".into(),
                    symbol_name: Some("validate_jwt".to_string()),
                    symbol_type: Some("function".to_string()),
                },
                score: 0.87,
            }
        ];

        let compact_output = format_compact(&current_dir, "jwt validation", &mock_results, None);
        let full_output = format_full(&current_dir, "jwt validation", &mock_results, None);

        // Verify compact format characteristics
        assert!(compact_output.contains("src/auth/mod.rs:45"));
        assert!(compact_output.contains("validate_jwt"));
        assert!(compact_output.contains("(0.87)"));
        assert!(!compact_output.contains("```"));

        // Verify full format characteristics
        assert!(full_output.contains("```"));
        assert!(full_output.contains("\u{1f4c4}"));

        // Token estimation: compact should use significantly fewer tokens
        let compact_chars = compact_output.len();
        let full_chars = full_output.len();
        assert!(
            compact_chars < full_chars,
            "Compact should use fewer chars: compact={} vs full={}",
            compact_chars,
            full_chars
        );

        // Compact should be smaller than full format
        assert!(
            compact_chars < full_chars,
            "Compact should be smaller: compact={} vs full={}",
            compact_chars,
            full_chars
        );
    }

    #[test]
    fn test_route_query() {
        // LSP routes
        assert_eq!(route_query("`validate_jwt`"), SearchStrategy::Lsp);
        assert_eq!(route_query("auth::middleware"), SearchStrategy::Lsp);
        assert_eq!(route_query("user.name"), SearchStrategy::Lsp);

        // Glob routes
        assert_eq!(route_query("*.rs"), SearchStrategy::Glob);
        assert_eq!(route_query("src/main.rs"), SearchStrategy::Glob);
        assert_eq!(route_query("src/**/*.ts"), SearchStrategy::Glob);

        // Grep routes
        assert_eq!(route_query(r"\d{3}"), SearchStrategy::Grep);
        assert_eq!(route_query("^error"), SearchStrategy::Grep);
        assert_eq!(route_query("\"Unauthorized\""), SearchStrategy::Grep);

        // Semantic routes
        assert_eq!(
            route_query("how do we validate JWT tokens?"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("find auth validation logic"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("where is user authentication handled"),
            SearchStrategy::Semantic
        );
        assert_eq!(
            route_query("explain the rate limiting implementation"),
            SearchStrategy::Semantic
        );

        // GrepThenSemantic for short queries
        assert_eq!(route_query("auth"), SearchStrategy::GrepThenSemantic);
        assert_eq!(route_query("jwt"), SearchStrategy::GrepThenSemantic);
        assert_eq!(
            route_query("authentication"),
            SearchStrategy::GrepThenSemantic
        );
    }
}
