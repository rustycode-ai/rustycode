//! Structured utility functions for LLM interactions
//!
//! This module provides helper functions inspired by Anthropic's Claude Cookbooks
//! for common LLM operations.

use crate::provider::{ChatMessage, CompletionRequest, LLMProvider};
use anyhow::Result;
use regex::Regex;
use std::sync::Arc;

/// Simple wrapper for LLM calls with system prompt support
///
/// This is the "go-to" function for making LLM requests with minimal boilerplate.
///
/// # Example
///
/// ```ignore
/// use rustycode_llm::utils::llm_call;
///
/// let response = llm_call(
///     &provider,
///     "What is the capital of France?",
///     Some("You are a helpful geography assistant."),
///     "claude-sonnet-4-6"
/// ).await?;
/// ```
pub async fn llm_call(
    provider: &Arc<dyn LLMProvider>,
    prompt: &str,
    system_prompt: Option<&str>,
    model: &str,
) -> Result<String> {
    let mut messages = Vec::new();

    if let Some(system) = system_prompt {
        messages.push(ChatMessage::system(system.to_string()));
    }

    messages.push(ChatMessage::user(prompt.to_string()));

    let request = CompletionRequest::new(model.to_string(), messages)
        .with_max_tokens(4096)
        .with_temperature(0.1);

    let response = LLMProvider::complete(&**provider, request).await?;

    Ok(response.content)
}

/// Extract content from XML tags
///
/// Parses structured responses that use XML-style tags like `<thinking>...</thinking>`
/// or `<summary>...</summary>`.
///
/// # Example
///
/// ```ignore
/// use rustycode_llm::utils::extract_xml;
///
/// let text = "<thinking>This is my reasoning</thinking>\nAnd here's the answer.";
/// let thinking = extract_xml(text, "thinking").unwrap();
/// assert_eq!(thinking, "This is my reasoning");
/// ```
pub fn extract_xml(text: &str, tag: &str) -> Option<String> {
    let pattern = format!(r"<{}>(.*?)</{}>", tag, tag);
    let re = Regex::new(&pattern).ok()?;
    re.captures(text)?.get(1).map(|m| m.as_str().to_string())
}

/// Extract content from XML tags (with multi-line support)
///
/// Similar to `extract_xml` but supports tags that span multiple lines.
pub fn extract_xml_multiline(text: &str, tag: &str) -> Option<String> {
    let pattern = format!(
        r"(?s)<{}>(.*?)</{}>",
        regex::escape(tag),
        regex::escape(tag)
    );
    let re = Regex::new(&pattern).ok()?;
    re.captures(text)?
        .get(1)
        .map(|m: regex::Match| m.as_str().to_string())
}

/// Extract all occurrences of a tag
///
/// Returns all matches for the given tag in order.
pub fn extract_xml_all(text: &str, tag: &str) -> Vec<String> {
    let escaped_tag = regex::escape(tag);
    let pattern = format!(r"<{}>(.*?)</{}>", escaped_tag, escaped_tag);
    let re = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };

    re.captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Extract all occurrences of a tag (multi-line)
pub fn extract_xml_all_multiline(text: &str, tag: &str) -> Vec<String> {
    let pattern = format!(
        r"(?s)<{}>(.*?)</{}>",
        regex::escape(tag),
        regex::escape(tag)
    );
    let Ok(re) = Regex::new(&pattern) else {
        return Vec::new();
    };

    re.captures_iter(text)
        .filter_map(|c: regex::Captures| c.get(1).map(|m: regex::Match| m.as_str().to_string()))
        .collect()
}

/// Check if text contains a specific tag
pub fn has_tag(text: &str, tag: &str) -> bool {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);
    text.contains(&open_tag) && text.contains(&close_tag)
}

/// Strip XML tags from text
///
/// Removes all occurrences of the specified tags while keeping the content.
pub fn strip_xml_tags(text: &str, tag: &str) -> String {
    let result = text.replace(&format!("<{}>", tag), "");
    result.replace(&format!("</{}>", tag), "")
}

/// Parse a structured summary
///
/// Extracts sections from a summary formatted with the standard structure:
/// - Task Overview
/// - Current State
/// - Important Discoveries
/// - Next Steps
/// - Context to Preserve
pub fn parse_summary(summary: &str) -> Summary {
    Summary {
        task_overview: extract_section(summary, &["Task Overview", "Overview"]),
        current_state: extract_section(summary, &["Current State", "State"]),
        important_discoveries: extract_section(summary, &["Important Discoveries", "Discoveries"]),
        next_steps: extract_section(summary, &["Next Steps", "Steps"]),
        context_to_preserve: extract_section(summary, &["Context to Preserve", "Context"]),
    }
}

/// Extract a section from structured text
///
/// Looks for any of the alternative headers and extracts content until the next header.
fn extract_section(text: &str, headers: &[&str]) -> Option<String> {
    // Find the first header
    let header_pos = headers.iter().find_map(|h| text.find(h));

    let start = match header_pos {
        Some(pos) => {
            // Find the end of the header line
            let after_header = &text[pos..];
            after_header.find('\n').map_or(pos, |nl| pos + nl + 1)
        }
        None => return None,
    };

    // Get the substring starting from `start`
    let remaining = &text[start..];

    // Collect lines until we hit a new header or empty line
    let mut content_lines = Vec::new();
    for line in remaining.lines() {
        if line.starts_with('#') || line.starts_with("**") {
            break;
        }
        if line.trim().is_empty() && !content_lines.is_empty() {
            // Empty line ends the section
            break;
        }
        if !line.trim().is_empty() {
            content_lines.push(line.trim());
        }
    }

    if content_lines.is_empty() {
        None
    } else {
        Some(content_lines.join(" "))
    }
}

/// A structured summary
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub task_overview: Option<String>,
    pub current_state: Option<String>,
    pub important_discoveries: Option<String>,
    pub next_steps: Option<String>,
    pub context_to_preserve: Option<String>,
}

impl Summary {
    /// Check if the summary is empty
    pub fn is_empty(&self) -> bool {
        self.task_overview.is_none()
            && self.current_state.is_none()
            && self.important_discoveries.is_none()
            && self.next_steps.is_none()
            && self.context_to_preserve.is_none()
    }
}

/// Token count estimator
///
/// Uses the canonical word-based heuristic from rustycode-protocol.
pub fn estimate_tokens(text: &str) -> usize {
    rustycode_protocol::estimate_tokens(text)
}

/// Chunk text into smaller pieces
///
/// Splits text into chunks of approximately max_tokens each.
pub fn chunk_text(text: &str, max_tokens: usize) -> Vec<String> {
    let max_chars = max_tokens * 4;
    let mut chunks = Vec::new();

    // Try to split at sentence boundaries
    // Pre-allocate current string with estimated capacity
    let mut current = String::with_capacity(max_chars);
    let mut char_count = 0;

    for sentence in text.split_terminator(['.', '!', '?']) {
        let sentence_with_period = format!("{}.", sentence.trim());
        let sentence_len = sentence_with_period.chars().count();

        if char_count + sentence_len > max_chars && !current.is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
            char_count = 0;
        }

        current.push_str(&sentence_with_period);
        current.push(' ');
        char_count = char_count.saturating_add(sentence_len).saturating_add(1);
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

// ── Reasoning Model Support ──────────────────────────────────────────────────

/// Reasoning effort level for OpenAI-style reasoning models.
///
/// Controls how much compute the model spends "thinking" before responding.
/// Higher effort = more thorough reasoning but slower and more expensive.
///
/// Inspired by goose's `extract_reasoning_effort` in `providers/utils.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ReasoningEffort {
    /// Minimal reasoning — fast, cheap
    Low,
    /// Balanced reasoning — default for reasoning models
    Medium,
    /// Maximum reasoning — slowest, most thorough
    High,
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReasoningEffort::Low => write!(f, "low"),
            ReasoningEffort::Medium => write!(f, "medium"),
            ReasoningEffort::High => write!(f, "high"),
            #[allow(unreachable_patterns)]
            _ => write!(f, "medium"),
        }
    }
}

/// Extract the base model name and reasoning effort from a reasoning model identifier.
///
/// OpenAI reasoning models (o1, o2, o3, o4, gpt-5) may have an effort suffix:
/// - `o3-low` → (`o3`, Some(Low))
/// - `o3-mini-medium` → (`o3-mini`, Some(Medium))
/// - `o1-preview` → (`o1-preview`, None)  (no explicit effort)
///
/// For non-reasoning models, returns the original name with `None`.
///
/// Inspired by goose's `extract_reasoning_effort` in `providers/utils.rs`.
///
/// # Example
///
/// ```
/// use rustycode_llm::utils::{extract_reasoning_effort, ReasoningEffort};
///
/// let (base, effort) = extract_reasoning_effort("o3-low");
/// assert_eq!(base, "o3");
/// assert_eq!(effort, Some(ReasoningEffort::Low));
///
/// let (base, effort) = extract_reasoning_effort("gpt-4o");
/// assert_eq!(base, "gpt-4o");
/// assert_eq!(effort, None);
/// ```
pub fn extract_reasoning_effort(model_name: &str) -> (String, Option<ReasoningEffort>) {
    let is_reasoning_model = model_name.starts_with("o1")
        || model_name.starts_with("o2")
        || model_name.starts_with("o3")
        || model_name.starts_with("o4")
        || model_name.starts_with("gpt-5");

    if !is_reasoning_model {
        return (model_name.to_string(), None);
    }

    // Check if the last segment after '-' is an effort level
    if let Some(last_dash) = model_name.rfind('-') {
        let suffix = &model_name[last_dash + 1..];
        let prefix = &model_name[..last_dash];

        // Don't strip if the prefix would be empty or just a version number
        if prefix.is_empty() || prefix.chars().all(|c| c.is_ascii_digit()) {
            return (model_name.to_string(), Some(ReasoningEffort::Medium));
        }

        match suffix {
            "low" => (prefix.to_string(), Some(ReasoningEffort::Low)),
            "medium" => (prefix.to_string(), Some(ReasoningEffort::Medium)),
            "high" => (prefix.to_string(), Some(ReasoningEffort::High)),
            _ => (model_name.to_string(), Some(ReasoningEffort::Medium)),
        }
    } else {
        // Reasoning model without any dash (e.g., "o3") — default to medium
        (model_name.to_string(), Some(ReasoningEffort::Medium))
    }
}

/// Check if a model is a reasoning model (o1/o2/o3/o4/gpt-5 family).
///
/// # Example
///
/// ```
/// use rustycode_llm::utils::is_reasoning_model;
///
/// assert!(is_reasoning_model("o3-mini"));
/// assert!(is_reasoning_model("o1-preview"));
/// assert!(is_reasoning_model("gpt-5-turbo"));
/// assert!(!is_reasoning_model("gpt-4o"));
/// assert!(!is_reasoning_model("claude-sonnet-4-6"));
/// ```
pub fn is_reasoning_model(model_name: &str) -> bool {
    model_name.starts_with("o1")
        || model_name.starts_with("o2")
        || model_name.starts_with("o3")
        || model_name.starts_with("o4")
        || model_name.starts_with("gpt-5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_xml() {
        let text = "<thinking>This is my reasoning</thinking>\nAnd here's the answer.";
        assert_eq!(
            extract_xml(text, "thinking"),
            Some("This is my reasoning".to_string())
        );
        assert_eq!(extract_xml(text, "missing"), None);
    }

    #[test]
    fn test_extract_xml_multiline() {
        let text = "<summary>\nLine 1\nLine 2\nLine 3\n</summary>";
        assert_eq!(
            extract_xml_multiline(text, "summary"),
            Some("\nLine 1\nLine 2\nLine 3\n".to_string())
        );
    }

    #[test]
    fn test_extract_xml_all() {
        let text = "<item>First</item>\n<item>Second</item>\n<item>Third</item>";
        let items = extract_xml_all(text, "item");
        assert_eq!(items, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn test_has_tag() {
        assert!(has_tag("<tag>content</tag>", "tag"));
        assert!(!has_tag("<tag>content", "tag"));
        assert!(!has_tag("content</tag>", "tag"));
    }

    #[test]
    fn test_strip_xml_tags() {
        let text = "<tag>content</tag>";
        assert_eq!(strip_xml_tags(text, "tag"), "content");
    }

    #[test]
    fn test_parse_summary() {
        let summary = r#"## Task Overview
Build a REST API

## Current State
Implemented user endpoints

## Next Steps
Add authentication"#;

        let parsed = parse_summary(summary);
        assert_eq!(parsed.task_overview, Some("Build a REST API".to_string()));
        assert_eq!(
            parsed.current_state,
            Some("Implemented user endpoints".to_string())
        );
        assert_eq!(parsed.next_steps, Some("Add authentication".to_string()));
    }

    #[test]
    fn test_estimate_tokens() {
        // Rough estimate: ~4 chars per token
        let text = "Hello world! This is a test.";
        let estimate = estimate_tokens(text);
        assert!(estimate > 0 && estimate < 20);
    }

    #[test]
    fn test_chunk_text() {
        let text = "Sentence one. Sentence two. Sentence three. Sentence four.";
        let chunks = chunk_text(text, 10); // Small chunk size for testing
        assert!(!chunks.is_empty());
        // Each chunk should be reasonably sized
        for chunk in chunks {
            assert!(chunk.len() <= 50); // 10 tokens * ~4 chars/token + padding
        }
    }

    // ── Reasoning Effort Tests ─────────────────────────────────────────────────

    #[test]
    fn test_extract_reasoning_effort_o3_low() {
        let (base, effort) = extract_reasoning_effort("o3-low");
        assert_eq!(base, "o3");
        assert_eq!(effort, Some(ReasoningEffort::Low));
    }

    #[test]
    fn test_extract_reasoning_effort_o3_high() {
        let (base, effort) = extract_reasoning_effort("o3-high");
        assert_eq!(base, "o3");
        assert_eq!(effort, Some(ReasoningEffort::High));
    }

    #[test]
    fn test_extract_reasoning_effort_o3_mini_medium() {
        let (base, effort) = extract_reasoning_effort("o3-mini-medium");
        assert_eq!(base, "o3-mini");
        assert_eq!(effort, Some(ReasoningEffort::Medium));
    }

    #[test]
    fn test_extract_reasoning_effort_o3_no_suffix() {
        let (base, effort) = extract_reasoning_effort("o3");
        assert_eq!(base, "o3");
        assert_eq!(effort, Some(ReasoningEffort::Medium));
    }

    #[test]
    fn test_extract_reasoning_effort_o1_preview() {
        let (base, effort) = extract_reasoning_effort("o1-preview");
        // "preview" is not a recognized effort, so defaults to Medium
        assert_eq!(base, "o1-preview");
        assert_eq!(effort, Some(ReasoningEffort::Medium));
    }

    #[test]
    fn test_extract_reasoning_effort_non_reasoning() {
        let (base, effort) = extract_reasoning_effort("gpt-4o");
        assert_eq!(base, "gpt-4o");
        assert_eq!(effort, None);

        let (base, effort) = extract_reasoning_effort("claude-sonnet-4-6");
        assert_eq!(base, "claude-sonnet-4-6");
        assert_eq!(effort, None);
    }

    #[test]
    fn test_extract_reasoning_effort_gpt5() {
        let (base, effort) = extract_reasoning_effort("gpt-5-low");
        assert_eq!(base, "gpt-5");
        assert_eq!(effort, Some(ReasoningEffort::Low));
    }

    #[test]
    fn test_is_reasoning_model() {
        assert!(is_reasoning_model("o1"));
        assert!(is_reasoning_model("o1-preview"));
        assert!(is_reasoning_model("o3-mini"));
        assert!(is_reasoning_model("o4"));
        assert!(is_reasoning_model("gpt-5"));
        assert!(is_reasoning_model("gpt-5-turbo"));
        assert!(!is_reasoning_model("gpt-4o"));
        assert!(!is_reasoning_model("gpt-4-turbo"));
        assert!(!is_reasoning_model("claude-sonnet-4-6"));
        assert!(!is_reasoning_model("mistral-large"));
    }

    #[test]
    fn test_reasoning_effort_display() {
        assert_eq!(ReasoningEffort::Low.to_string(), "low");
        assert_eq!(ReasoningEffort::Medium.to_string(), "medium");
        assert_eq!(ReasoningEffort::High.to_string(), "high");
    }
}
