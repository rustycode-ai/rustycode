//! Prompt Compressor (orchestra-2 pattern)
//!
//! Deterministic text compression for context reduction and cost optimization.
//!
//! Applies lossless and near-lossless transformations to reduce token count
//! while preserving semantic meaning. No LLM calls, no external dependencies.
//! Sub-millisecond for typical prompt sizes.
//!
//! # Why Compress Prompts?
//!
//! LLM API costs scale linearly with token count. Reducing prompt size:
//! - **Reduces API costs** proportionally
//! - **Improves latency** (less data to process)
//! - **Fits more context** in the window
//! - **Enables longer sessions** before checkpointing
//!
//! # Compression Techniques
//!
//! Applied in order (early = cheap, late = aggressive):
//!
//! 1. **Whitespace Normalization**: Collapse multiple spaces/newlines
//! 2. **Markdown Reduction**: Collapse verbose tables, lists
//! 3. **Phrase Abbreviation**: "In order to" → "To"
//! 4. **Pattern Deduplication**: Remove repeated boilerplate
//! 5. **Content Removal**: Empty sections, low-information text
//!
//! # Compression Levels
//!
//! - **Light**: Whitespace + basic markdown (5-15% savings)
//! - **Moderate**: + phrase abbreviation (15-30% savings)
//! - **Aggressive**: + pattern deduplication (30-50% savings)
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestration::prompt_compressor::{compress_prompt, CompressionOptions, CompressionLevel};
//!
//! let options = CompressionOptions {
//!     level: CompressionLevel::Moderate,
//!     preserve_headings: true,  // Keep for section-boundary truncation
//!     preserve_code_blocks: true, // Never modify code
//!     target_chars: Some(50_000), // Stop when we hit target
//! };
//!
//! let result = compress_prompt(&very_long_prompt, &options)?;
//! println!("Compressed: {} chars → {} chars ({}% savings)",
//!     result.original_chars,
//!     result.compressed_chars,
//!     result.savings_percent
//! );
//! ```
//!
//! # Determinism
//!
//! Compression is **deterministic**: same input always produces same output.
//! This is critical for:
//! - Reproducible behavior
//! - Consistent token counting
//! - Predictable costs
//!
//! # Safety
//!
//! - **Code blocks** are never modified (`preserve_code_blocks` = true)
//! - **Headings** preserved by default (for section detection)
//! - **Semantic meaning** preserved (only syntax is optimized)
//! - **Reversible** in most cases (lossless for code/structure)

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// SAFETY: These regex patterns are valid compile-time constants
#[allow(clippy::unwrap_used)]
static STRUCT_PATTERN_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^(\w[\w\s]*?):\s+(.+)$").unwrap());

/// Precompiled regex for collapsing table formatting
// SAFETY: This regex pattern is a valid compile-time constant
#[allow(clippy::unwrap_used)]
static TABLE_PADDING_RE: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\|[ \t]{2,}([^|\n]*?)[ \t]{2,}\|").unwrap());

/// Compression intensity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CompressionLevel {
    Light,
    Moderate,
    Aggressive,
}

/// Compression result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// The compressed content
    pub content: String,
    /// Original character count
    pub original_chars: usize,
    /// Compressed character count
    pub compressed_chars: usize,
    /// Savings percentage (0-100)
    pub savings_percent: f64,
    /// Which compression level was applied
    pub level: CompressionLevel,
    /// Number of transformations applied
    pub transformations_applied: usize,
}

/// Compression options
#[derive(Debug, Clone)]
pub struct CompressionOptions {
    /// Compression intensity
    pub level: CompressionLevel,
    /// Preserve markdown headings (useful for section-boundary truncation)
    pub preserve_headings: bool,
    /// Preserve code blocks verbatim
    pub preserve_code_blocks: bool,
    /// Target character count (compression stops when achieved)
    pub target_chars: Option<usize>,
}

impl Default for CompressionOptions {
    fn default() -> Self {
        Self {
            level: CompressionLevel::Moderate,
            preserve_headings: true,
            preserve_code_blocks: true,
            target_chars: None,
        }
    }
}

// ─── Phrase Abbreviation Map ────────────────────────────────────────────────

/// Build a regex that matches a verbose phrase (handles line wrapping)
fn phrase_pattern(phrase: &str) -> String {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    format!(r"\b{}\b", words.join(r"\s+"))
}

const VERBOSE_PHRASES: &[(&str, &str)] = &[
    ("In order to", "To"),
    ("It is important to note that", "Note:"),
    ("As mentioned previously", "(see above)"),
    ("The following", "These"),
    ("In addition to", "Also,"),
    ("Due to the fact that", "Because"),
    ("At this point in time", "Now"),
    ("For the purpose of", "For"),
    ("In the event that", "If"),
    ("With regard to", "Re:"),
    ("Prior to", "Before"),
    ("Subsequent to", "After"),
    ("In accordance with", "Per"),
    ("A number of", "Several"),
    ("In the case of", "For"),
    ("On the basis of", "Based on"),
];

// ─── Code Block Extraction ──────────────────────────────────────────────────

struct ExtractedBlocks {
    text: String,
    blocks: Vec<String>,
}

fn extract_code_blocks(content: &str) -> ExtractedBlocks {
    let mut blocks = Vec::new();
    let mut counter = 0;
    let mut result = String::new();
    let chars = content.chars();
    let mut in_code_block = false;
    let mut code_block_start = 0;
    let mut last_was_backtick = 0;
    let mut _chars_since_backtick = 0;

    for ch in chars {
        if ch == '`' {
            last_was_backtick += 1;
            _chars_since_backtick = 0;
        } else {
            if last_was_backtick == 3 {
                if in_code_block {
                    // End of code block
                    in_code_block = false;
                    // Remove the ``` from result
                    result.truncate(result.len() - 3);
                    // Extract the code block content
                    let start_idx = code_block_start + format!("\x00CODEBLOCK_{counter}\x00").len();
                    let code_content = result[start_idx..].to_string();
                    blocks.push(code_content);
                    counter += 1;
                } else {
                    // Start of code block
                    in_code_block = true;
                    code_block_start = result.len();
                    // Remove the ``` from result
                    result.truncate(result.len() - 3);
                    let placeholder = format!("\x00CODEBLOCK_{counter}\x00");
                    result.push_str(&placeholder);
                }
            }
            last_was_backtick = 0;
            _chars_since_backtick += 1;
        }

        if !in_code_block || last_was_backtick != 3 {
            result.push(ch);
        }
    }

    ExtractedBlocks {
        text: result,
        blocks,
    }
}

fn restore_code_blocks(text: &str, blocks: &[String]) -> String {
    let mut result = text.to_string();
    for (i, block) in blocks.iter().enumerate() {
        let placeholder = format!("\x00CODEBLOCK_{i}\x00");
        result = result.replace(&placeholder, &format!("```{block}\n```"));
    }
    result
}

// ─── Light Transformations ──────────────────────────────────────────────────

#[allow(clippy::unwrap_used)]
fn normalize_whitespace(content: &str) -> String {
    // Collapse 3+ consecutive blank lines to 2
    let re = Regex::new(r"(\n\s*){3,}\n").unwrap();
    let result = re.replace_all(content, "\n\n");
    // Trim trailing whitespace on every line
    result
        .to_string()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(clippy::unused_peekable)]
fn remove_markdown_comments(content: &str) -> String {
    let mut result = String::new();
    #[allow(clippy::unused_peekable)]
    let mut chars = content.chars().peekable();
    let mut in_comment = false;

    while let Some(ch) = chars.next() {
        if in_comment {
            if ch == '-' && chars.peek() == Some(&'-') {
                // Check for -->
                let mut iter = chars.clone();
                iter.next(); // consume second -
                if iter.next() == Some('>') {
                    in_comment = false;
                    chars.next(); // consume second -
                    chars.next(); // consume >
                }
            }
        } else {
            if ch == '<' && chars.peek() == Some(&'!') {
                // Check for <!--
                let mut iter = chars.clone();
                iter.next();
                if iter.next() == Some('-') && iter.next() == Some('-') {
                    in_comment = true;
                    chars.next(); // consume !
                    chars.next(); // consume -
                    chars.next(); // consume -
                    continue;
                }
            }
            if !in_comment {
                result.push(ch);
            }
        }
    }

    result
}

fn remove_horizontal_rules(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !matches!(trimmed, "---" | "***" | "___") || trimmed.len() > 3
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collapse_empty_list_items(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut prev_was_empty = false;

    for line in lines {
        let is_empty_list = line.trim().starts_with(['-', '*', '+']) && line.trim().len() <= 2;

        if is_empty_list && prev_was_empty {
            continue; // Skip consecutive empty list items
        }

        result.push(line);
        prev_was_empty = is_empty_list;
    }

    result.join("\n")
}

fn apply_light_transformations(content: &str) -> (String, usize) {
    let mut count = 0;
    let mut result = content.to_string();

    let after1 = normalize_whitespace(&result);
    if after1 != result {
        count += 1;
    }
    result = after1;

    let after2 = remove_markdown_comments(&result);
    if after2 != result {
        count += 1;
    }
    result = after2;

    let after3 = remove_horizontal_rules(&result);
    if after3 != result {
        count += 1;
    }
    result = after3;

    let after4 = collapse_empty_list_items(&result);
    if after4 != result {
        count += 1;
    }
    result = after4;

    (result, count)
}

// ─── Moderate Transformations ───────────────────────────────────────────────

fn abbreviate_verbose_phrases(content: &str) -> (String, usize) {
    let mut count = 0;
    let mut result = content.to_string();

    for &(pattern, replacement) in VERBOSE_PHRASES {
        let regex_pattern = phrase_pattern(pattern);
        if let Ok(re) = Regex::new(&regex_pattern) {
            let after = re.replace_all(&result, replacement);
            if after != result {
                count += 1;
            }
            result = after.into_owned();
        }
    }

    (result, count)
}

fn remove_boilerplate_lines(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Remove lines that are just N/A, (none), (empty), (not applicable)
            !matches!(
                trimmed.to_uppercase().as_str(),
                "N/A" | "(NONE)" | "(EMPTY)" | "(NOT APPLICABLE)"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn deduplicate_consecutive_lines(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        if i == 0 || *line != lines[i - 1] || line.trim().is_empty() {
            result.push(*line);
        }
    }

    result.join("\n")
}

fn collapse_table_formatting(content: &str) -> String {
    // Remove excessive padding in markdown table cells
    let mut result = String::new();

    for line in content.lines() {
        if line.contains('|') {
            // Collapse table cell padding
            let collapsed = TABLE_PADDING_RE.replace_all(line, "| $1 |");
            result.push_str(&collapsed);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    result
}

fn apply_moderate_transformations(content: &str) -> (String, usize) {
    let mut count = 0;
    let mut result = content.to_string();

    let (phrase_result, phrase_count) = abbreviate_verbose_phrases(&result);
    count += phrase_count;
    result = phrase_result;

    let after1 = remove_boilerplate_lines(&result);
    if after1 != result {
        count += 1;
    }
    result = after1;

    let after2 = deduplicate_consecutive_lines(&result);
    if after2 != result {
        count += 1;
    }
    result = after2;

    let after3 = collapse_table_formatting(&result);
    if after3 != result {
        count += 1;
    }
    result = after3;

    (result, count)
}

// ─── Aggressive Transformations ─────────────────────────────────────────────

#[allow(clippy::unwrap_used)]
fn remove_markdown_emphasis(content: &str) -> String {
    let mut result = content.to_string();

    // Bold: **text** or __text__
    result = Regex::new(r"\*\*(.+?)\*\*")
        .unwrap()
        .replace_all(&result, "$1")
        .into_owned();
    result = Regex::new(r"__(.+?)__")
        .unwrap()
        .replace_all(&result, "$1")
        .into_owned();

    // Italic: *text* or _text_ (simplified - removes all single emphasis markers)
    result = Regex::new(r"\*([^*\n]+?)\*")
        .unwrap()
        .replace_all(&result, "$1")
        .into_owned();
    result = Regex::new(r"_([^_\n]+?)_")
        .unwrap()
        .replace_all(&result, "$1")
        .into_owned();

    result
}

#[allow(clippy::unwrap_used)]
fn remove_markdown_links(content: &str) -> String {
    // [text](url) → text
    Regex::new(r"\[([^\]]+)\]\([^)]+\)")
        .unwrap()
        .replace_all(content, "$1")
        .into_owned()
}

fn truncate_long_lines(content: &str) -> String {
    content
        .lines()
        .map(|line| {
            if line.len() <= 300 {
                return line.to_string();
            }

            let truncate_zone_end = line.floor_char_boundary(300.min(line.len()));
            let truncate_zone = &line[..truncate_zone_end];
            let last_sentence_end = [
                truncate_zone.rfind(". "),
                truncate_zone.rfind("! "),
                truncate_zone.rfind("? "),
            ]
            .iter()
            .filter_map(|&opt| opt)
            .max();

            if let Some(idx) = last_sentence_end {
                if idx > 150 {
                    return line[..=idx].to_string();
                }
            }

            // Fallback: cut at last space before 300
            let last_space = truncate_zone.rfind(' ');
            if let Some(idx) = last_space {
                if idx > 150 {
                    return line[..idx].to_string();
                }
            }

            truncate_zone.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(clippy::unwrap_used)]
fn remove_bullet_markers(content: &str) -> String {
    // Remove bullet markers: - , * , + , numbered (1. 2. etc)
    Regex::new(r"^[ \t]*(?:[-*+]|\d+\.)\s+")
        .unwrap()
        .replace_all(content, "")
        .into_owned()
}

#[allow(clippy::unwrap_used)]
fn remove_blockquote_markers(content: &str) -> String {
    Regex::new(r"^[ \t]*>+\s?")
        .unwrap()
        .replace_all(content, "")
        .into_owned()
}

fn deduplicate_structural_patterns(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut last_was_structural = false;

    for line in lines {
        let trimmed = line.trim();
        // Detect structural patterns: "Key: value"
        let struct_match = STRUCT_PATTERN_RE.is_match(trimmed);

        if struct_match {
            if seen.contains(trimmed) {
                last_was_structural = true;
                continue;
            }
            seen.insert(trimmed.to_string());
            last_was_structural = true;
        } else {
            if !last_was_structural || trimmed.is_empty() {
                seen.clear();
            }
            last_was_structural = false;
        }
        result.push(line);
    }

    result.join("\n")
}

fn apply_aggressive_transformations(content: &str, _preserve_headings: bool) -> (String, usize) {
    let mut count = 0;
    let mut result = content.to_string();

    let after1 = remove_markdown_emphasis(&result);
    if after1 != result {
        count += 1;
    }
    result = after1;

    let after2 = remove_markdown_links(&result);
    if after2 != result {
        count += 1;
    }
    result = after2;

    let after3 = truncate_long_lines(&result);
    if after3 != result {
        count += 1;
    }
    result = after3;

    let after4 = remove_bullet_markers(&result);
    if after4 != result {
        count += 1;
    }
    result = after4;

    let after5 = remove_blockquote_markers(&result);
    if after5 != result {
        count += 1;
    }
    result = after5;

    let after6 = deduplicate_structural_patterns(&result);
    if after6 != result {
        count += 1;
    }
    result = after6;

    (result, count)
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Compress prompt content using deterministic text transformations
pub fn compress_prompt(content: &str, options: &CompressionOptions) -> CompressionResult {
    if content.is_empty() {
        return CompressionResult {
            content: String::new(),
            original_chars: 0,
            compressed_chars: 0,
            savings_percent: 0.0,
            level: options.level,
            transformations_applied: 0,
        };
    }

    let original_chars = content.len();
    let mut working = content.to_string();
    let mut total_transformations = 0;

    // Extract code blocks if preserving
    let code_blocks = if options.preserve_code_blocks {
        let extracted = extract_code_blocks(&working);
        working = extracted.text;
        Some(extracted.blocks)
    } else {
        None
    };

    // Apply light transformations (always)
    let (light_result, light_count) = apply_light_transformations(&working);
    working = light_result;
    total_transformations += light_count;

    // Check target
    if let Some(target) = options.target_chars {
        if working.len() <= target {
            let compressed_chars = working.len();
            return CompressionResult {
                content: working,
                original_chars,
                compressed_chars,
                savings_percent: if original_chars > 0 {
                    ((original_chars - compressed_chars) as f64 / original_chars as f64) * 100.0
                } else {
                    0.0
                },
                level: options.level,
                transformations_applied: total_transformations,
            };
        }
    }

    // Apply moderate transformations
    if options.level == CompressionLevel::Moderate || options.level == CompressionLevel::Aggressive
    {
        let (mod_result, mod_count) = apply_moderate_transformations(&working);
        working = mod_result;
        total_transformations += mod_count;

        if let Some(target) = options.target_chars {
            if working.len() <= target {
                let compressed_chars = working.len();
                return CompressionResult {
                    content: working,
                    original_chars,
                    compressed_chars,
                    savings_percent: if original_chars > 0 {
                        ((original_chars - compressed_chars) as f64 / original_chars as f64) * 100.0
                    } else {
                        0.0
                    },
                    level: options.level,
                    transformations_applied: total_transformations,
                };
            }
        }
    }

    // Apply aggressive transformations
    if options.level == CompressionLevel::Aggressive {
        let (agg_result, agg_count) =
            apply_aggressive_transformations(&working, options.preserve_headings);
        working = agg_result;
        total_transformations += agg_count;
    }

    // Restore code blocks if extracted
    let final_content = if let Some(blocks) = code_blocks {
        restore_code_blocks(&working, &blocks)
    } else {
        working
    };

    let compressed_chars = final_content.len();
    let savings_percent = if original_chars > 0 && compressed_chars < original_chars {
        ((original_chars - compressed_chars) as f64 / original_chars as f64) * 100.0
    } else {
        0.0
    };

    CompressionResult {
        content: final_content,
        original_chars,
        compressed_chars,
        savings_percent,
        level: options.level,
        transformations_applied: total_transformations,
    }
}

/// Compress with a target size — applies progressively more aggressive
/// compression until the target is reached or all transformations exhausted
pub fn compress_to_target(content: &str, target_chars: usize) -> CompressionResult {
    if content.len() <= target_chars {
        return CompressionResult {
            content: content.to_string(),
            original_chars: content.len(),
            compressed_chars: content.len(),
            savings_percent: 0.0,
            level: CompressionLevel::Light,
            transformations_applied: 0,
        };
    }

    let levels = [
        CompressionLevel::Light,
        CompressionLevel::Moderate,
        CompressionLevel::Aggressive,
    ];

    for level in levels {
        let result = compress_prompt(
            content,
            &CompressionOptions {
                level,
                preserve_headings: true,
                preserve_code_blocks: true,
                target_chars: Some(target_chars),
            },
        );

        if result.compressed_chars <= target_chars {
            return result;
        }

        if level == CompressionLevel::Aggressive {
            return result;
        }
    }

    // Unreachable
    compress_prompt(
        content,
        &CompressionOptions {
            level: CompressionLevel::Aggressive,
            preserve_headings: true,
            preserve_code_blocks: true,
            target_chars: Some(target_chars),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        let input = "Line 1\n\n\n\nLine 2";
        let output = normalize_whitespace(input);
        assert_eq!(output, "Line 1\n\nLine 2");
    }

    #[test]
    fn test_remove_horizontal_rules() {
        let input = "Text 1\n---\nText 2\n***\nText 3";
        let output = remove_horizontal_rules(input);
        assert!(!output.contains("---"));
        assert!(!output.contains("***"));
    }

    #[test]
    fn test_collapse_empty_list_items() {
        let input = "- Item 1\n-\n-\n- Item 2";
        let output = collapse_empty_list_items(input);
        assert!(output.matches("-\n").count() <= 1);
    }

    #[test]
    fn test_compress_prompt_light() {
        let content = "Line 1\n\n\n\nLine 2\n---\nLine 3";
        let result = compress_prompt(
            content,
            &CompressionOptions {
                level: CompressionLevel::Light,
                preserve_headings: true,
                preserve_code_blocks: true,
                target_chars: None,
            },
        );

        assert!(result.compressed_chars < content.len());
        assert_eq!(result.level, CompressionLevel::Light);
    }

    #[test]
    fn test_compress_prompt_moderate() {
        let content = "In order to complete the task, we need to proceed. \
                       In addition to this, we must also consider the following aspects.";
        let result = compress_prompt(
            content,
            &CompressionOptions {
                level: CompressionLevel::Moderate,
                preserve_headings: true,
                preserve_code_blocks: true,
                target_chars: None,
            },
        );

        assert!(result.compressed_chars < content.len());
        assert_eq!(result.level, CompressionLevel::Moderate);
    }

    #[test]
    fn test_compress_to_target() {
        let long_content = "Line 1\n\n\n\nLine 2\n---\nLine 3\n".repeat(10); // Creates content with redundant whitespace and horizontal rules
        let result = compress_to_target(&long_content, 200);

        assert!(result.compressed_chars <= long_content.len());
    }
}
