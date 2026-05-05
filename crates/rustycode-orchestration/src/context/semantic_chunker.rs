//! Orchestra Semantic Chunker — TF-IDF Relevance Scoring
//!
//! Splits code/text into semantic chunks and selects the most relevant ones:
//! * Content type detection (code, markdown, text)
//! * Semantic boundary detection
//! * TF-IDF relevance scoring
//! * Token optimization with omission indicators
//!
//! Critical for context optimization in autonomous systems.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

// SAFETY: These regex patterns are valid compile-time constants
#[allow(clippy::unwrap_used)]
static CODE_BOUNDARY_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(export\s+)?(async\s+)?(function|class|interface|type|const|enum)\s")
        .unwrap()
});
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static IMPORT_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\s*import\s").unwrap());
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static MARKDOWN_HEADING_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^#{1,6}\s").unwrap());

// ─── Types ────────────────────────────────────────────────────────────────────

/// A chunk of content with metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Chunk {
    /// Chunk content
    pub content: String,

    /// Start line (1-based)
    pub start_line: usize,

    /// End line (1-based)
    pub end_line: usize,

    /// Relevance score (0-1)
    pub score: f64,
}

/// Result of chunking operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkResult {
    /// Selected chunks
    pub chunks: Vec<Chunk>,

    /// Total chunks before filtering
    pub total_chunks: usize,

    /// Number of chunks omitted
    pub omitted_chunks: usize,

    /// Percentage of content saved
    pub savings_percent: usize,
}

/// Options for chunk splitting
#[derive(Debug, Clone, Default)]
pub struct ChunkOptions {
    /// Minimum lines per chunk
    pub min_lines: Option<usize>,

    /// Maximum lines per chunk
    pub max_lines: Option<usize>,
}

/// Options for relevance scoring
#[derive(Debug, Clone, Default)]
pub struct RelevanceOptions {
    /// Maximum chunks to select
    pub max_chunks: Option<usize>,

    /// Minimum lines per chunk
    pub min_chunk_lines: Option<usize>,

    /// Maximum lines per chunk
    pub max_chunk_lines: Option<usize>,

    /// Minimum relevance score (0-1)
    pub min_score: Option<f64>,
}

// ─── Constants ────────────────────────────────────────────────────────────────

/// Regex pattern for code boundaries
const CODE_BOUNDARY_RE: &str =
    r"^(export\s+)?(async\s+)?(function|class|interface|type|const|enum)\s";

/// Regex pattern for markdown headings
const MARKDOWN_HEADING_RE: &str = r"^#{1,6}\s";

/// Default minimum lines per chunk
const DEFAULT_MIN_LINES: usize = 3;

/// Default maximum lines per chunk
const DEFAULT_MAX_LINES: usize = 80;

/// Default maximum chunks to select
const DEFAULT_MAX_CHUNKS: usize = 5;

/// Default minimum relevance score
const DEFAULT_MIN_SCORE: f64 = 0.1;

/// Stop words for TF-IDF
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "to", "of", "in", "for", "on", "with",
    "at", "by", "from", "this", "that", "it", "as", "or", "and", "not", "but", "if", "do", "no",
    "so", "up", "its", "has", "had", "get", "set", "can", "may", "all", "use", "new", "one", "two",
    "also", "each", "than", "been", "into", "most", "only", "over", "such", "how", "some", "any",
    "our", "his", "her", "out", "did", "let", "say", "she",
];

// ─── Content Type Detection ───────────────────────────────────────────────────

/// Content type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContentType {
    Code,
    Markdown,
    Text,
}

/// Detect content type from lines
fn detect_content_type(lines: &[String]) -> ContentType {
    let mut code_signals = 0;
    let mut md_signals = 0;
    let sample_size = lines.len().min(50);

    for line in lines.iter().take(sample_size) {
        if CODE_BOUNDARY_REGEX.is_match(line) || IMPORT_REGEX.is_match(line) {
            code_signals += 1;
        }
        if MARKDOWN_HEADING_REGEX.is_match(line) {
            md_signals += 1;
        }
    }

    if md_signals >= 2 && md_signals > code_signals {
        ContentType::Markdown
    } else if code_signals >= 2 {
        ContentType::Code
    } else {
        ContentType::Text
    }
}

// ─── Tokenizer ───────────────────────────────────────────────────────────────

/// Tokenize text into words
fn tokenize(text: &str) -> Vec<String> {
    let stop_set: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    text.to_lowercase()
        .split(|c: char| c.is_whitespace() || !c.is_alphanumeric())
        .filter(|w| w.len() >= 2 && !stop_set.contains(*w))
        .map(ToString::to_string)
        .collect()
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Split content into semantic chunks
///
pub fn split_into_chunks(content: &str, options: Option<&ChunkOptions>) -> Vec<Chunk> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    let min_lines = options
        .and_then(|o| o.min_lines)
        .unwrap_or(DEFAULT_MIN_LINES);
    let max_lines = options
        .and_then(|o| o.max_lines)
        .unwrap_or(DEFAULT_MAX_LINES);

    let lines: Vec<String> = content.split('\n').map(ToString::to_string).collect();

    if lines.is_empty() {
        return Vec::new();
    }

    let content_type = detect_content_type(&lines);
    let boundaries = match content_type {
        ContentType::Code => find_code_boundaries(&lines),
        ContentType::Markdown => find_markdown_boundaries(&lines),
        ContentType::Text => find_text_boundaries(&lines),
    };

    // Always include 0 as first boundary
    let mut boundaries = boundaries;
    if boundaries.is_empty() || boundaries[0] != 0 {
        boundaries.insert(0, 0);
    }

    // Build raw chunks from boundaries
    let mut raw_chunks: Vec<Chunk> = Vec::new();
    for (i, &start) in boundaries.iter().enumerate() {
        let end = if i + 1 < boundaries.len() {
            boundaries[i + 1] - 1
        } else {
            lines.len() - 1
        };

        // Check if range is valid
        if start >= lines.len() || end >= lines.len() || start > end {
            continue;
        }

        let chunk_lines: &Vec<String> = &lines;
        let chunk_content: String = chunk_lines[start..=end]
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>()
            .join("\n");

        if !chunk_content.is_empty() {
            raw_chunks.push(Chunk {
                content: chunk_content,
                start_line: start + 1, // 1-based
                end_line: end + 1,     // 1-based
                score: 0.0,
            });
        }
    }

    // Split oversized chunks at max_lines
    let mut split_chunks: Vec<Chunk> = Vec::new();
    for chunk in raw_chunks {
        let chunk_line_count = chunk.end_line - chunk.start_line + 1;
        if chunk_line_count <= max_lines {
            split_chunks.push(chunk);
        } else {
            let chunk_lines: Vec<&str> = chunk.content.split('\n').collect();
            let mut offset = 0;
            while offset < chunk_lines.len() {
                let slice_end = (offset + max_lines).min(chunk_lines.len());
                let slice = &chunk_lines[offset..slice_end];

                if !slice.is_empty() {
                    split_chunks.push(Chunk {
                        content: slice.join("\n"),
                        start_line: chunk.start_line + offset,
                        end_line: chunk.start_line + offset + slice.len() - 1,
                        score: 0.0,
                    });
                }

                offset += max_lines;
            }
        }
    }

    // Merge tiny chunks into predecessor
    let mut merged: Vec<Chunk> = Vec::new();
    for chunk in split_chunks {
        let chunk_line_count = chunk.end_line - chunk.start_line + 1;
        if chunk_line_count < min_lines && !merged.is_empty() {
            #[allow(clippy::unwrap_used)]
            let prev = merged.last_mut().unwrap();
            prev.content = format!("{}\n{}", prev.content, chunk.content);
            prev.end_line = chunk.end_line;
        } else {
            merged.push(chunk);
        }
    }

    merged
}

fn find_code_boundaries(lines: &[String]) -> Vec<usize> {
    #[allow(clippy::unwrap_used)]
    let re = regex::Regex::new(CODE_BOUNDARY_RE).unwrap();
    let mut boundaries: Vec<usize> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if re.is_match(line) {
            // Also consider a blank line before a boundary marker
            if !boundaries.contains(&i) {
                boundaries.push(i);
            }
        }
    }

    boundaries
}

fn find_markdown_boundaries(lines: &[String]) -> Vec<usize> {
    #[allow(clippy::unwrap_used)]
    let re = regex::Regex::new(MARKDOWN_HEADING_RE).unwrap();
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| re.is_match(line))
        .map(|(i, _)| i)
        .collect()
}

fn find_text_boundaries(lines: &[String]) -> Vec<usize> {
    let mut boundaries: Vec<usize> = vec![0];
    for i in 1..lines.len() {
        if lines[i - 1].trim().is_empty() && !lines[i].trim().is_empty() {
            boundaries.push(i);
        }
    }
    boundaries
}

/// Score chunks by relevance to query using TF-IDF
///
pub fn score_chunks(chunks: &[Chunk], query: &str) -> Vec<Chunk> {
    if chunks.is_empty() {
        return Vec::new();
    }

    let query_terms = tokenize(query);
    if query_terms.is_empty() {
        return chunks
            .iter()
            .map(|c| Chunk {
                score: 0.0,
                ..c.clone()
            })
            .collect();
    }

    let total_chunks = chunks.len();

    // Pre-compute IDF for each query term
    let mut term_chunk_counts: HashMap<String, usize> = HashMap::new();
    let chunk_token_sets: Vec<HashSet<String>> = chunks
        .iter()
        .map(|chunk| tokenize(&chunk.content).into_iter().collect())
        .collect();

    for tokens in &chunk_token_sets {
        for term in &query_terms {
            if tokens.contains(term) {
                let count = term_chunk_counts.entry(term.clone()).or_insert(0);
                *count += 1;
            }
        }
    }

    let mut idf: HashMap<String, f64> = HashMap::new();
    for term in &query_terms {
        let df = *term_chunk_counts.get(term).unwrap_or(&0);
        idf.insert(
            term.clone(),
            (total_chunks as f64 / (1.0 + df as f64)).ln_1p(),
        );
    }

    // Score each chunk
    let mut scored: Vec<Chunk> = Vec::new();
    for chunk in chunks {
        let chunk_tokens = tokenize(&chunk.content);
        let total_terms = chunk_tokens.len();
        if total_terms == 0 {
            scored.push(Chunk {
                score: 0.0,
                ..chunk.clone()
            });
            continue;
        }

        // Count term frequencies
        let mut term_freq: HashMap<String, f64> = HashMap::new();
        for token in &chunk_tokens {
            *term_freq.entry(token.clone()).or_insert(0.0) += 1.0;
        }

        let mut score = 0.0;
        for term in &query_terms {
            let tf = *term_freq.get(term).unwrap_or(&0.0) / total_terms as f64;
            let term_idf = *idf.get(term).unwrap_or(&0.0);
            score += tf * term_idf;
        }

        scored.push(Chunk {
            score,
            ..chunk.clone()
        });
    }

    // Normalize to 0-1
    let max_score = scored.iter().map(|c| c.score).fold(0.0_f64, f64::max);
    if max_score > 0.0 {
        for chunk in &mut scored {
            chunk.score /= max_score;
        }
    }

    scored
}

/// Select chunks by relevance to query
///
pub fn chunk_by_relevance(
    content: &str,
    query: &str,
    options: Option<&RelevanceOptions>,
) -> ChunkResult {
    let max_chunks = options
        .and_then(|o| o.max_chunks)
        .unwrap_or(DEFAULT_MAX_CHUNKS);
    let min_score = options
        .and_then(|o| o.min_score)
        .unwrap_or(DEFAULT_MIN_SCORE);
    let min_lines = options
        .and_then(|o| o.min_chunk_lines)
        .unwrap_or(DEFAULT_MIN_LINES);
    let max_lines = options
        .and_then(|o| o.max_chunk_lines)
        .unwrap_or(DEFAULT_MAX_LINES);

    let raw_chunks = split_into_chunks(
        content,
        Some(&ChunkOptions {
            min_lines: Some(min_lines),
            max_lines: Some(max_lines),
        }),
    );

    if raw_chunks.is_empty() {
        return ChunkResult {
            chunks: Vec::new(),
            total_chunks: 0,
            omitted_chunks: 0,
            savings_percent: 0,
        };
    }

    let scored = score_chunks(&raw_chunks, query);

    // Filter by min_score and take top max_chunks by score
    let mut qualifying: Vec<Chunk> = scored
        .into_iter()
        .filter(|c| c.score >= min_score)
        .collect();

    qualifying.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut selected: Vec<Chunk> = qualifying.into_iter().take(max_chunks).collect();

    // Return in original document order (by start_line)
    selected.sort_by_key(|a| a.start_line);

    let total_chars = content.len();
    let selected_chars: usize = selected.iter().map(|c| c.content.len()).sum();
    let savings_percent = if total_chars > 0 {
        (((total_chars - selected_chars) as f64) / (total_chars as f64) * 100.0).round() as usize
    } else {
        0
    };

    ChunkResult {
        total_chunks: raw_chunks.len(),
        omitted_chunks: raw_chunks.len() - selected.len(),
        savings_percent,
        chunks: selected,
    }
}

/// Format chunks with omission indicators
///
pub fn format_chunks(result: &ChunkResult, file_path: &str) -> String {
    if result.chunks.is_empty() {
        return format!("[{file_path}: empty or no relevant chunks]");
    }

    let mut parts: Vec<String> = Vec::new();
    let mut last_end_line = 0;

    for chunk in &result.chunks {
        // Show omission gap
        if last_end_line > 0 && chunk.start_line > last_end_line + 1 {
            let gap_lines = chunk.start_line - last_end_line - 1;
            parts.push(format!("[...{gap_lines} lines omitted...]"));
        }

        parts.push(format!("[Lines {}-{}]", chunk.start_line, chunk.end_line));
        parts.push(chunk.content.clone());

        last_end_line = chunk.end_line;
    }

    parts.join("\n")
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("The quick brown fox jumps over the lazy dog");
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(!tokens.contains(&"the".to_string())); // stop word
        assert!(!tokens.contains(&"a".to_string())); // too short
    }

    #[test]
    fn test_detect_content_type_code() {
        let lines = vec![
            "function foo() {".to_string(),
            "  return 42;".to_string(),
            "}".to_string(),
            "export class Bar {".to_string(),
        ];
        assert_eq!(detect_content_type(&lines), ContentType::Code);
    }

    #[test]
    fn test_detect_content_type_markdown() {
        let lines = vec![
            "# Heading".to_string(),
            "Some text".to_string(),
            "## Subheading".to_string(),
            "More text".to_string(),
        ];
        assert_eq!(detect_content_type(&lines), ContentType::Markdown);
    }

    #[test]
    fn test_detect_content_type_text() {
        let lines = vec![
            "Just some plain text".to_string(),
            "With no special markers".to_string(),
            "Just regular content".to_string(),
        ];
        assert_eq!(detect_content_type(&lines), ContentType::Text);
    }

    #[test]
    fn test_split_into_chunks_empty() {
        let chunks = split_into_chunks("", None);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_split_into_chunks_code() {
        let content = r"
function foo() {
    return 42;
}

function bar() {
    return 24;
}
";

        let chunks = split_into_chunks(content, None);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_split_into_chunks_markdown() {
        let content = r"
# First Section

Some content here.

## Second Section

More content here.
";

        let chunks = split_into_chunks(content, None);
        assert!(chunks.len() >= 2);
    }

    #[test]
    fn test_score_chunks_empty_query() {
        let chunks = vec![Chunk {
            content: "Some content".to_string(),
            start_line: 1,
            end_line: 1,
            score: 0.0,
        }];

        let scored = score_chunks(&chunks, "");
        assert_eq!(scored[0].score, 0.0);
    }

    #[test]
    fn test_score_chunks_with_relevance() {
        let chunks = vec![
            Chunk {
                content: "Authentication system with login and logout".to_string(),
                start_line: 1,
                end_line: 1,
                score: 0.0,
            },
            Chunk {
                content: "Database connection pool management".to_string(),
                start_line: 2,
                end_line: 2,
                score: 0.0,
            },
        ];

        let scored = score_chunks(&chunks, "authentication login");
        assert!(scored[0].score > scored[1].score);
    }

    #[test]
    fn test_chunk_by_relevance_empty() {
        let result = chunk_by_relevance("", "test", None);
        assert_eq!(result.total_chunks, 0);
        assert_eq!(result.chunks.len(), 0);
    }

    #[test]
    fn test_chunk_by_relevance_filters() {
        let content = r"
Section about authentication and login systems.

Section about database management and connection pooling.

Section about user interface design and layouts.
";

        let result = chunk_by_relevance(content, "authentication", None);
        assert!(!result.chunks.is_empty());
        assert!(result.savings_percent > 0);
    }

    #[test]
    fn test_format_chunks_empty() {
        let result = ChunkResult {
            chunks: vec![],
            total_chunks: 0,
            omitted_chunks: 0,
            savings_percent: 0,
        };

        let formatted = format_chunks(&result, "test.rs");
        assert!(formatted.contains("empty or no relevant chunks"));
    }

    #[test]
    fn test_format_chunks_with_content() {
        let result = ChunkResult {
            chunks: vec![
                Chunk {
                    content: "line one\nline two".to_string(),
                    start_line: 1,
                    end_line: 2,
                    score: 1.0,
                },
                Chunk {
                    content: "line five\nline six".to_string(),
                    start_line: 5,
                    end_line: 6,
                    score: 0.5,
                },
            ],
            total_chunks: 2,
            omitted_chunks: 0,
            savings_percent: 0,
        };

        let formatted = format_chunks(&result, "test.rs");
        assert!(formatted.contains("[Lines 1-2]"));
        assert!(formatted.contains("[Lines 5-6]"));
        assert!(formatted.contains("[...2 lines omitted...]"));
    }
}
