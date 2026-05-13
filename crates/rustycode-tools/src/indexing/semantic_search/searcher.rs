//! Cosine similarity calculation and result formatting.

use super::store::{IndexMetadata, SearchResult};

/// Compute cosine similarity between two vectors
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a > 0.0 && norm_b > 0.0 {
        dot / (norm_a * norm_b)
    } else {
        0.0
    }
}

/// Get relative path for display, falling back to absolute if not under project root
fn get_display_path(file_path: &std::path::Path, project_root: &std::path::Path) -> String {
    file_path
        .strip_prefix(project_root)
        .unwrap_or(file_path)
        .display()
        .to_string()
}

/// Determine if auto-compact should be used based on query characteristics
pub(crate) fn should_auto_compact(query: &str, top_k: usize) -> bool {
    // Auto-compact for large result sets
    if top_k > 10 {
        return true;
    }

    // Auto-compact for broad queries (few specific keywords)
    let query_lower = query.to_lowercase();
    let broad_keywords = [
        "all",
        "everything",
        "any",
        "every",
        "broad",
        "overview",
        "summary",
    ];

    if broad_keywords.iter().any(|kw| query_lower.contains(kw)) {
        return true;
    }

    // Auto-compact for very short queries (likely to return many results)
    if query.split_whitespace().count() <= 2 && query.len() < 15 {
        return true;
    }

    false
}

/// Format results in compact single-line format
///
/// Format: `file:line (score) symbol | preview...`
pub(crate) fn format_compact(
    project_root: &std::path::Path,
    query: &str,
    results: &[SearchResult],
    metadata: Option<&IndexMetadata>,
) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!("Semantic results for '{}':\n", query));

    // Results - single line each
    for result in results {
        let rel_path = get_display_path(&result.chunk.file_path, project_root);

        // file:line (score) symbol | preview
        output.push_str(&format!(
            "  {}:{} ({:.2})",
            rel_path, result.chunk.start_line, result.score
        ));

        if let Some(ref symbol) = result.chunk.symbol_name {
            output.push_str(&format!(" `{}`", symbol));
        }

        // Preview: first 80 chars of content on single line
        let preview: String = result
            .chunk
            .content
            .lines()
            .take(1)
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .chars()
            .take(80)
            .collect();

        if result.chunk.content.chars().count() > 80 {
            output.push_str(&format!(" | {}...\n", preview));
        } else {
            output.push_str(&format!(" | {}\n", preview));
        }
    }

    // Footer with index stats
    if let Some(m) = metadata {
        output.push_str(&format!(
            "\n[Indexed {} chunks from {} files]",
            m.total_chunks, m.total_files
        ));
    }

    output
}

/// Format results in full detailed format
pub(crate) fn format_full(
    project_root: &std::path::Path,
    query: &str,
    results: &[SearchResult],
    metadata: Option<&IndexMetadata>,
) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "**Semantic Search Results for: \"{}\"**\n\n",
        query
    ));

    for result in results.iter() {
        let rel_path = get_display_path(&result.chunk.file_path, project_root);

        output.push_str(&format!(
            "\u{1f4c4} **{}:{}-{}** (score: {:.2})\n",
            rel_path, result.chunk.start_line, result.chunk.end_line, result.score
        ));

        if let Some(ref symbol) = result.chunk.symbol_name {
            output.push_str(&format!(
                "   *Symbol:* `{}` ({})\n",
                symbol,
                result.chunk.symbol_type.as_deref().unwrap_or("unknown")
            ));
        }

        output.push_str(&format!("```{}\n", result.chunk.language));

        // Show first 10 lines of content
        let preview_lines: Vec<&str> = result.chunk.content.lines().take(10).collect();
        for line in preview_lines {
            output.push_str(line);
            output.push('\n');
        }
        if result.chunk.content.lines().count() > 10 {
            output.push_str("...\n");
        }
        output.push_str("```\n\n");
    }

    if let Some(m) = metadata {
        output.push_str(&format!(
            "\u{26a1} Indexed {} chunks from {} files",
            m.total_chunks, m.total_files
        ));
    }

    output
}

/// Format results in ultra-compact format: just file references
///
/// Format: `file:line (score) [symbol]`
pub(crate) fn format_minimal(project_root: &std::path::Path, results: &[SearchResult]) -> String {
    let mut output = String::new();

    for result in results {
        let rel_path = get_display_path(&result.chunk.file_path, project_root);

        output.push_str(&format!(
            "{}:{} ({:.2})",
            rel_path, result.chunk.start_line, result.score
        ));

        if let Some(ref symbol) = result.chunk.symbol_name {
            output.push_str(&format!(" [{}]", symbol));
        }
        output.push('\n');
    }

    output
}

pub(crate) fn estimate_tokens(output: &str) -> usize {
    rustycode_protocol::estimate_tokens(output)
}
