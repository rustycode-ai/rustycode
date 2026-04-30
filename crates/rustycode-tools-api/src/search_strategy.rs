//! Search strategy routing for code queries.
//!
//! Analyzes query intent and recommends the most appropriate search strategy
//! (LSP, grep, glob, semantic search, or combinations).

/// Search strategy to use for a given query
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Kept for future use
#[non_exhaustive]
pub enum SearchStrategy {
    /// Use LSP for exact symbol lookups
    Lsp,
    /// Use grep for exact text patterns
    Grep,
    /// Use glob for filename patterns
    Glob,
    /// Use semantic search for intent-based queries
    Semantic,
    /// Try grep first, fallback to semantic if no results
    GrepThenSemantic,
}

/// Analyze query intent and recommend search strategy
#[allow(dead_code)] // Kept for future use
pub fn route_query(query: &str) -> SearchStrategy {
    let q = query.trim().to_lowercase();

    // Exact symbol reference with backticks → LSP
    if query.contains('`') {
        return SearchStrategy::Lsp;
    }

    // Namespace::symbol or module::function → LSP
    if query.contains("::") {
        return SearchStrategy::Lsp;
    }

    // File extension pattern → Glob (check before dot pattern)
    if std::path::Path::new(query).extension().is_some_and(|ext| {
        ext.eq_ignore_ascii_case("rs")
            || ext.eq_ignore_ascii_case("py")
            || ext.eq_ignore_ascii_case("go")
            || ext.eq_ignore_ascii_case("js")
            || ext.eq_ignore_ascii_case("ts")
            || ext.eq_ignore_ascii_case("java")
            || ext.eq_ignore_ascii_case("tsx")
            || ext.eq_ignore_ascii_case("jsx")
    }) {
        return SearchStrategy::Glob;
    }

    // Glob pattern → Glob (but not just a trailing ? for questions)
    if query.contains('*') || (query.contains('?') && !query.trim().ends_with('?')) {
        return SearchStrategy::Glob;
    }

    // Method call pattern: single word with dot (e.g., "user.name") → LSP
    if query.contains('.') && query.split_whitespace().count() <= 2 && !query.contains('/') {
        return SearchStrategy::Lsp;
    }

    // Regex-like patterns → Grep
    if query.contains(r"\d")
        || query.contains(r"\s")
        || query.contains(r"\w")
        || query.contains("^[")
        || query.contains('$')
        || query.contains("[0-9]")
    {
        return SearchStrategy::Grep;
    }

    // Error message or exact string → Grep
    if query.starts_with('"') && query.ends_with('"') {
        return SearchStrategy::Grep;
    }

    // Quoted string anywhere in query → Grep
    if query.contains('"') {
        return SearchStrategy::Grep;
    }

    // Intent-based keywords → Semantic
    let semantic_triggers = [
        "how",
        "where",
        "what",
        "which",
        "find",
        "show",
        "explain",
        "logic",
        "implementation",
        "pattern",
        "handle",
        "validate",
        "authenticate",
        "authorize",
        "process",
        "workflow",
    ];

    for trigger in &semantic_triggers {
        if q.contains(trigger) {
            return SearchStrategy::Semantic;
        }
    }

    // Question format → Semantic
    if q.starts_with("how")
        || q.starts_with("where")
        || q.starts_with("what")
        || q.starts_with("why")
        || q.ends_with('?')
    {
        return SearchStrategy::Semantic;
    }

    // Short queries (1-2 words) without special chars → GrepThenSemantic
    if query.split_whitespace().count() <= 2
        && !query.contains(|c: char| !c.is_alphanumeric() && !c.is_whitespace())
    {
        return SearchStrategy::GrepThenSemantic;
    }

    // Default: semantic for longer natural language queries
    if query.split_whitespace().count() >= 3 {
        return SearchStrategy::Semantic;
    }

    // Fallback to grep
    SearchStrategy::Grep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backtick_routes_to_lsp() {
        assert_eq!(route_query("`validate_jwt`"), SearchStrategy::Lsp);
    }

    #[test]
    fn test_namespace_routes_to_lsp() {
        assert_eq!(route_query("foo::bar"), SearchStrategy::Lsp);
    }

    #[test]
    fn test_file_extension_routes_to_glob() {
        assert_eq!(route_query("main.rs"), SearchStrategy::Glob);
        assert_eq!(route_query("app.tsx"), SearchStrategy::Glob);
    }

    #[test]
    fn test_glob_pattern_routes_to_glob() {
        assert_eq!(route_query("src/**/*.rs"), SearchStrategy::Glob);
    }

    #[test]
    fn test_quoted_string_routes_to_grep() {
        assert_eq!(route_query("\"Unauthorized\""), SearchStrategy::Grep);
    }

    #[test]
    fn test_semantic_keyword_routes_to_semantic() {
        assert_eq!(
            route_query("how do we validate JWT tokens?"),
            SearchStrategy::Semantic
        );
    }

    #[test]
    fn test_short_word_routes_to_grep_then_semantic() {
        assert_eq!(route_query("auth"), SearchStrategy::GrepThenSemantic);
    }

    #[test]
    fn test_regex_pattern_routes_to_grep() {
        assert_eq!(route_query(r"\d+"), SearchStrategy::Grep);
    }

    #[test]
    fn test_method_call_routes_to_lsp() {
        assert_eq!(route_query("user.name"), SearchStrategy::Lsp);
    }
}
