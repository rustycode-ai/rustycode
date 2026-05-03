//! Tool-specific result summarizer.
//!
//! Distills tool outputs before LLM processing to reduce token usage while
//! preserving error messages, final results, and key structured fields.

use crate::summary::summary_config::SummaryConfig;
use anyhow::Result;
use regex::Regex;
use serde_json::Value;

/// Number of head/tail lines to keep for file content summarization.
const FILE_HEAD_LINES: usize = 10;
const FILE_TAIL_LINES: usize = 10;

/// Tool type constants used for dispatch.
const TOOL_BASH: &str = "bash";
const TOOL_JSON: &str = "json";
const TOOL_API: &str = "api_response";
const TOOL_FILE: &str = "file_content";
const TOOL_READ: &str = "read_file";

/// Summarizes tool outputs to reduce token consumption.
pub struct ResultSummarizer {
    config: SummaryConfig,
}

impl ResultSummarizer {
    /// Create a new summarizer with the given configuration.
    pub const fn new(config: SummaryConfig) -> Self {
        Self { config }
    }

    /// Main entry point: summarize a tool output based on tool type.
    ///
    /// If the output is shorter than `max_output_chars`, it is returned as-is.
    /// Otherwise, dispatches to the appropriate tool-specific summarizer.
    pub fn summarize(&self, tool_type: &str, output: &str) -> Result<String> {
        if output.len() <= self.config.max_output_chars {
            return Ok(output.to_string());
        }

        let summarized = match tool_type {
            TOOL_BASH => self.summarize_bash_output(output),
            TOOL_JSON | TOOL_API => self.summarize_json_output(output),
            TOOL_FILE | TOOL_READ => self.summarize_file_content(output),
            _ => {
                // Check custom extractors before falling back to generic.
                self.apply_custom_extractor(tool_type, output)
                    .unwrap_or_else(|| self.summarize_generic(output))
            }
        };

        Ok(summarized)
    }

    /// Summarize bash output: keep ERROR/WARN lines plus the final result line.
    pub fn summarize_bash_output(&self, output: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        if lines.is_empty() {
            return String::new();
        }

        let mut kept = Vec::new();
        let mut added = std::collections::HashSet::new();

        // Collect error/warning lines.
        if self.config.preserve_errors {
            for (i, line) in lines.iter().enumerate() {
                let upper = line.to_uppercase();
                if upper.contains("ERROR")
                    || upper.contains("ERR:")
                    || upper.contains("WARN")
                    || upper.contains("WARNING:")
                    || upper.contains("FATAL")
                    || upper.contains("PANIC")
                && added.insert(i)
                {
                    kept.push(*line);
                }
            }
        }

        // Preserve the final non-empty line as the result.
        if self.config.preserve_final_result {
            if let Some((idx, last)) = lines
                .iter()
                .enumerate()
                .rev()
                .find(|(_, l)| !l.trim().is_empty())
            {
                if added.insert(idx) {
                    kept.push(*last);
                }
            }
        }

        // If we have nothing meaningful, take up to max_bash_lines from tail.
        if kept.is_empty() {
            let start = lines.len().saturating_sub(self.config.max_bash_lines);
            let tail: Vec<&str> = lines[start..].to_vec();
            if start > 0 {
                return format!("... [{} lines omitted] ...\n{}", start, tail.join("\n"));
            }
            return tail.join("\n");
        }

        // Enforce line limit.
        if kept.len() > self.config.max_bash_lines {
            let drop = kept.len() - self.config.max_bash_lines;
            kept = kept[drop..].to_vec();
            format!("... [{} lines omitted] ...\n{}", drop, kept.join("\n"))
        } else {
            kept.join("\n")
        }
    }

    /// Summarize JSON output: parse and extract configured keys.
    pub fn summarize_json_output(&self, output: &str) -> String {
        let trimmed = output.trim();

        // Try to parse as JSON.
        match serde_json::from_str::<Value>(trimmed) {
            Ok(Value::Object(map)) => {
                let mut extracted = serde_json::Map::with_capacity(self.config.json_extract_keys.len());

                for key in &self.config.json_extract_keys {
                    if let Some(val) = map.get(key).cloned() {
                        extracted.insert(key.clone(), val);
                    }
                }

                // If nothing matched, fall back to a generic truncation.
                if extracted.is_empty() {
                    return self.summarize_generic(trimmed);
                }

                let summary = Value::Object(extracted);
                // Best-effort pretty print; ignore serialization errors.
                serde_json::to_string_pretty(&summary)
                    .unwrap_or_else(|_| self.summarize_generic(trimmed))
            }
            Ok(Value::Array(arr)) => {
                // For arrays, show count and first few items.
                let total = arr.len();
                let preview: Vec<_> = arr.iter().take(3).cloned().collect();
                let mut obj = serde_json::Map::new();
                obj.insert("total_items".into(), Value::Number(total.into()));
                obj.insert(
                    "preview".into(),
                    Value::Array(preview),
                );
                serde_json::to_string_pretty(&Value::Object(obj))
                    .unwrap_or_else(|_| self.summarize_generic(trimmed))
            }
            _ => {
                // Not a JSON object/array; fall back to generic.
                self.summarize_generic(trimmed)
            }
        }
    }

    /// Summarize API responses. Delegates to JSON summarizer.
    pub fn summarize_api_response(&self, output: &str) -> String {
        self.summarize_json_output(output)
    }

    /// Summarize file content: keep first and last N lines with a truncation marker.
    pub fn summarize_file_content(&self, output: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        let total = lines.len();

        if total <= FILE_HEAD_LINES + FILE_TAIL_LINES {
            return output.to_string();
        }

        let head: Vec<&str> = lines.iter().take(FILE_HEAD_LINES).copied().collect();
        let tail: Vec<&str> = lines
            .iter()
            .skip(total.saturating_sub(FILE_TAIL_LINES))
            .copied()
            .collect();
        let omitted = total - FILE_HEAD_LINES - FILE_TAIL_LINES;

        format!(
            "{}\n... [{} lines omitted] ...\n{}",
            head.join("\n"),
            omitted,
            tail.join("\n")
        )
    }

    /// Generic summarizer: truncate output with a marker.
    pub fn summarize_generic(&self, output: &str) -> String {
        let limit = self.config.max_output_chars;
        if output.len() <= limit {
            return output.to_string();
        }

        // Try to truncate at a character boundary.
        let mut end = limit.saturating_sub("[truncated]".len());
        // Floor to char boundary to avoid panicking on multi-byte characters.
        while !output.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}[truncated]", &output[..end])
    }

    /// Rough token estimate: ~4 chars per token.
    pub const fn estimate_tokens(&self, content: &str) -> usize {
        content.len().div_ceil(4)
    }

    /// Compute reduction ratio between original and summarized content.
    ///
    /// Returns a value in 0.0..=1.0 where 0.0 means no reduction and
    /// values approaching 1.0 mean aggressive reduction.
    pub fn reduction_ratio(&self, original: &str, summarized: &str) -> f64 {
        if original.is_empty() {
            return 0.0;
        }
        let orig_len = original.len() as f64;
        let sum_len = summarized.len() as f64;
        let ratio = 1.0 - (sum_len / orig_len);
        ratio.clamp(0.0, 1.0)
    }

    /// Apply a custom regex extractor if one is configured for this tool type.
    fn apply_custom_extractor(&self, tool_type: &str, output: &str) -> Option<String> {
        let pattern = self.config.custom_extractors.get(tool_type)?;
        let re = Regex::new(pattern).ok()?;

        let matching: Vec<&str> = output
            .lines()
            .filter(|line| re.is_match(line))
            .take(self.config.max_bash_lines)
            .collect();

        if matching.is_empty() {
            None
        } else {
            Some(matching.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_summarizer() -> ResultSummarizer {
        ResultSummarizer::new(SummaryConfig::default())
    }

    #[test]
    fn test_summarizes_bash_output() {
        let summarizer = default_summarizer();
        let output = "\
[INFO] Starting build
[DEBUG] Checking dependencies
[DEBUG] Compiling module A
[DEBUG] Compiling module B
[ERROR] Failed to link: undefined symbol foo
[DEBUG] Cleaning up
Build finished with errors";
        let result = summarizer.summarize_bash_output(output);

        // Must preserve the ERROR line.
        assert!(
            result.contains("Failed to link: undefined symbol foo"),
            "error line should be preserved, got: {result}"
        );
        // Must preserve the final result line.
        assert!(
            result.contains("Build finished with errors"),
            "final result line should be preserved, got: {result}"
        );
        // DEBUG lines should be dropped.
        assert!(
            !result.contains("[DEBUG]"),
            "debug lines should be dropped, got: {result}"
        );
    }

    #[test]
    fn test_summarizes_json_response() {
        let summarizer = default_summarizer();
        let output = r#"{
            "status": "ok",
            "timestamp": "2026-05-03T12:00:00Z",
            "request_id": "abc-123-def-456",
            "metadata": {"region": "us-east-1", "version": "2.0.0"},
            "result": {"items": [1, 2, 3]},
            "debug_trace": "very long trace string..."
        }"#;
        let result = summarizer.summarize_json_output(output);

        // Should contain configured extract keys.
        assert!(
            result.contains("\"status\""),
            "should extract status key, got: {result}"
        );
        assert!(
            result.contains("\"result\""),
            "should extract result key, got: {result}"
        );
        // Should not contain non-extracted keys.
        assert!(
            !result.contains("request_id"),
            "should not contain non-extracted keys, got: {result}"
        );
    }

    #[test]
    fn test_token_reduction() {
        let summarizer = default_summarizer();
        // Build a large bash output with mostly debug noise.
        let mut lines: Vec<String> = (0..200)
            .map(|i| format!("[DEBUG] Processing item {i}"))
            .collect();
        lines.push("[ERROR] Something went wrong".to_string());
        lines.push("Final result: 42 items processed".to_string());
        let output = lines.join("\n");

        let summarized = summarizer.summarize_bash_output(&output);

        // Should achieve at least 50% reduction.
        let ratio = summarizer.reduction_ratio(&output, &summarized);
        assert!(
            ratio >= 0.5,
            "expected at least 50% reduction, got {:.1}%",
            ratio * 100.0
        );
    }

    #[test]
    fn test_short_output_unchanged() {
        let summarizer = default_summarizer();
        let short = "This is a short output";
        let result = summarizer
            .summarize(TOOL_BASH, short)
            .expect("summarize should succeed");
        assert_eq!(
            result, short,
            "short output should be returned as-is"
        );
    }

    #[test]
    fn test_file_content_summary() {
        let summarizer = default_summarizer();
        let lines: Vec<String> = (0..50).map(|i| format!("Line {i}")).collect();
        let output = lines.join("\n");

        let result = summarizer.summarize_file_content(&output);

        // Should contain first lines.
        assert!(
            result.contains("Line 0"),
            "should contain first line, got: {result}"
        );
        assert!(
            result.contains("Line 9"),
            "should contain 10th line, got: {result}"
        );
        // Should contain last lines.
        assert!(
            result.contains("Line 49"),
            "should contain last line, got: {result}"
        );
        // Should NOT contain middle lines.
        assert!(
            !result.contains("Line 20"),
            "should not contain middle lines, got: {result}"
        );
        // Should contain truncation marker.
        assert!(
            result.contains("lines omitted"),
            "should contain truncation marker, got: {result}"
        );
    }

    #[test]
    fn test_generic_truncation() {
        let summarizer = default_summarizer();
        let long = "x".repeat(5000);
        let result = summarizer.summarize_generic(&long);

        assert!(
            result.contains("[truncated]"),
            "should contain truncation marker, got: {result}"
        );
        assert!(
            result.len() < long.len(),
            "result should be shorter than input"
        );
    }

    #[test]
    fn test_reduction_ratio_calculation() {
        let summarizer = default_summarizer();

        // No reduction.
        let ratio = summarizer.reduction_ratio("hello", "hello");
        assert!(
            (ratio - 0.0).abs() < 0.01,
            "expected ~0.0, got {ratio}"
        );

        // 50% reduction.
        let ratio = summarizer.reduction_ratio("abcdefghij", "abcde");
        assert!(
            (ratio - 0.5).abs() < 0.01,
            "expected ~0.5, got {ratio}"
        );

        // Full reduction (empty summary).
        let ratio = summarizer.reduction_ratio("hello", "");
        assert!(
            (ratio - 1.0).abs() < 0.01,
            "expected ~1.0, got {ratio}"
        );

        // Empty original should be 0.0.
        let ratio = summarizer.reduction_ratio("", "");
        assert!(
            (ratio - 0.0).abs() < 0.01,
            "expected 0.0 for empty original, got {ratio}"
        );
    }

    #[test]
    fn test_custom_extractor() {
        let mut config = SummaryConfig::default();
        config.custom_extractors.insert(
            "custom_tool".into(),
            r"^\[RESULT\]".into(),
        );
        let summarizer = ResultSummarizer::new(config);

        let output = (0..100)
            .map(|i| format!("[LOG] Entry {i}"))
            .chain(std::iter::once("[RESULT] success: 42".into()))
            .collect::<Vec<String>>()
            .join("\n");

        // Make it exceed the char limit so summarize dispatches.
        let padded = format!("{output}\n{}", "padding ".repeat(200));
        let result = summarizer
            .summarize("custom_tool", &padded)
            .expect("summarize should succeed");

        assert!(
            result.contains("[RESULT] success: 42"),
            "custom extractor should keep matching lines, got: {result}"
        );
    }

    #[test]
    fn test_json_array_summary() {
        let summarizer = default_summarizer();
        let output = serde_json::to_string(&(0..100).collect::<Vec<i32>>()).unwrap();
        let result = summarizer.summarize_json_output(&output);

        assert!(
            result.contains("\"total_items\""),
            "should contain total_items, got: {result}"
        );
        assert!(
            result.contains("\"preview\""),
            "should contain preview, got: {result}"
        );
    }

    #[test]
    fn test_empty_bash_output() {
        let summarizer = default_summarizer();
        let result = summarizer.summarize_bash_output("");
        assert!(result.is_empty(), "empty input should produce empty output");
    }

    #[test]
    fn test_bash_no_errors_keeps_tail() {
        let summarizer = default_summarizer();
        let lines: Vec<String> = (0..100).map(|i| format!("[INFO] Item {i}")).collect();
        let output = lines.join("\n");
        let result = summarizer.summarize_bash_output(&output);

        // Should keep the last line since no errors and preserve_final_result is true.
        assert!(
            result.contains("Item 99"),
            "should keep final result line, got: {result}"
        );
    }

    #[test]
    fn test_multibyte_truncation_safe() {
        let summarizer = default_summarizer();
        // Emoji-heavy content that could slice at a bad boundary.
        let content = "hello world ".repeat(500) + "\u{1F600}\u{1F601}\u{1F602}";
        // Should not panic on multi-byte boundary handling.
        let result = summarizer.summarize_generic(&content);
        assert!(
            result.contains("[truncated]"),
            "should truncate safely, got len={}",
            result.len()
        );
        // Verify the result is valid UTF-8.
        assert!(result.is_char_boundary(result.len()));
    }
}
