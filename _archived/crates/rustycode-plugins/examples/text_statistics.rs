//! Example Tool Plugin: Text Statistics
//!
//! This example demonstrates how to implement a `ToolPlugin` that analyzes text
//! and provides statistics like word count, line count, character count, and
//! estimated token count.
//!
//! This plugin shows:
//! - Implementation of the `ToolPlugin` trait
//! - Configuration handling (verbose mode, output format)
//! - Structured output with tool descriptors
//! - Error handling and validation

// Example file: allow common clippy noise for demonstration code.
#![allow(
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::unnecessary_literal_bound,
    clippy::uninlined_format_args,
    clippy::unused_self
)]

use anyhow::Result;
use serde_json::{json, Value};

// Import plugin system components
use rustycode_plugins::{
    config::{ConfigValue, PluginConfig},
    metadata::PluginMetadata,
    traits::{ToolDescriptor, ToolPlugin},
};

/// Text Statistics Tool Plugin
///
/// Provides tools for analyzing text content, including:
/// - Word count
/// - Line count
/// - Character count
/// - Estimated token count (using common tokenization approximations)
#[derive(Clone)]
pub struct TextStatisticsTool {
    /// Plugin configuration
    config: PluginConfig,
}

impl TextStatisticsTool {
    /// Create a new `TextStatistics` tool with default configuration
    pub fn new() -> Self {
        let mut config = PluginConfig::new("text-statistics");
        config.set("verbose", ConfigValue::Bool(false));
        config.set("output_format", ConfigValue::String("text".to_string()));

        Self { config }
    }

    /// Create with custom configuration
    pub const fn with_config(config: PluginConfig) -> Self {
        Self { config }
    }

    /// Analyze text and return statistics
    fn analyze_text(&self, text: &str) -> TextAnalysis {
        let words = text.split_whitespace().count();
        let lines = text.lines().count();
        let chars = text.len();
        let chars_no_whitespace = text.chars().filter(|c| !c.is_whitespace()).count();

        // Rough token estimation: ~4 characters per token on average
        // This is a simplified heuristic; actual tokenization varies by LLM
        let estimated_tokens = (chars as f64 / 4.0).ceil() as usize; // usize fits in f64 on 64-bit

        TextAnalysis {
            word_count: words,
            line_count: lines,
            char_count: chars,
            char_count_no_whitespace: chars_no_whitespace,
            estimated_tokens,
        }
    }

    /// Format analysis as text
    /// Reads verbose setting from config
    fn format_as_text(&self, analysis: &TextAnalysis) -> String {
        let verbose = self.config.get_bool("verbose").unwrap_or(false);

        if verbose {
            format!(
                "Text Statistics:\\n  Words: {}\\n  Lines: {}\\n  Characters: {}\\n  Characters (no whitespace): {}\\n  Estimated Tokens: {}",
                analysis.word_count,
                analysis.line_count,
                analysis.char_count,
                analysis.char_count_no_whitespace,
                analysis.estimated_tokens
            )
        } else {
            format!(
                "Words: {}, Lines: {}, Chars: {}, Tokens: {}",
                analysis.word_count,
                analysis.line_count,
                analysis.char_count,
                analysis.estimated_tokens
            )
        }
    }

    /// Format analysis as JSON
    /// Reads `output_format` setting from config (currently always returns JSON)
    fn format_as_json(&self, analysis: &TextAnalysis) -> Value {
        let _format = self
            .config
            .get_string("output_format")
            .unwrap_or_else(|| "json".to_string());

        json!({
            "word_count": analysis.word_count,
            "line_count": analysis.line_count,
            "character_count": analysis.char_count,
            "character_count_no_whitespace": analysis.char_count_no_whitespace,
            "estimated_tokens": analysis.estimated_tokens
        })
    }
}

/// Analysis result structure
#[derive(Debug, Clone)]
struct TextAnalysis {
    word_count: usize,
    line_count: usize,
    char_count: usize,
    char_count_no_whitespace: usize,
    estimated_tokens: usize,
}

impl Default for TextStatisticsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPlugin for TextStatisticsTool {
    fn name(&self) -> &str {
        "text-statistics"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Analyzes text content and provides word count, line count, character count, and estimated token count"
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(self.name(), self.version(), self.description())
    }

    fn init(&self) -> Result<()> {
        println!("TextStatistics plugin initialized");
        Ok(())
    }

    fn shutdown(&self) -> Result<()> {
        println!("TextStatistics plugin shutting down");
        Ok(())
    }

    fn get_tools(&self) -> Result<Vec<ToolDescriptor>> {
        Ok(vec![
            ToolDescriptor::new(
                "analyze_text",
                "Analyze text and get statistics (word count, line count, character count, estimated tokens)",
                json!({
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "The text to analyze"
                        }
                    },
                    "required": ["text"]
                }),
            ),
        ])
    }

    fn config_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "verbose": {
                    "type": "boolean",
                    "description": "Enable verbose output with detailed formatting",
                    "default": false
                },
                "output_format": {
                    "type": "string",
                    "enum": ["text", "json"],
                    "description": "Output format for results",
                    "default": "text"
                }
            }
        })
    }
}

// Example usage for documentation
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_statistics_tool_creation() {
        let tool = TextStatisticsTool::new();
        assert_eq!(tool.name(), "text-statistics");
        assert_eq!(tool.version(), "1.0.0");
        assert!(tool.description().contains("Analyzes text"));
    }

    #[test]
    fn test_text_analysis() {
        let tool = TextStatisticsTool::new();
        let analysis = tool.analyze_text("hello world\nthis is a test");

        assert_eq!(analysis.word_count, 6);
        assert_eq!(analysis.line_count, 2);
        assert!(analysis.char_count > 0);
        assert!(analysis.estimated_tokens > 0);
    }

    #[test]
    fn test_text_formatting_text() {
        let tool = TextStatisticsTool::new();
        let analysis = tool.analyze_text("hello world");
        let formatted = tool.format_as_text(&analysis);

        assert!(formatted.contains("Words:"));
        assert!(formatted.contains("2")); // 2 words
    }

    #[test]
    fn test_text_formatting_json() {
        let tool = TextStatisticsTool::new();
        let analysis = tool.analyze_text("hello world");
        let formatted = tool.format_as_json(&analysis);

        assert!(formatted["word_count"].is_number());
        assert_eq!(formatted["word_count"].as_u64(), Some(2));
    }

    #[test]
    fn test_tool_plugin_trait() {
        let tool = TextStatisticsTool::new();
        assert!(tool.init().is_ok());
        assert!(tool.get_tools().is_ok());

        let tools = tool.get_tools().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "analyze_text");

        assert!(tool.shutdown().is_ok());
    }

    #[test]
    fn test_config_schema() {
        let tool = TextStatisticsTool::new();
        let schema = tool.config_schema();

        assert!(schema["properties"]["verbose"].is_object());
        assert!(schema["properties"]["output_format"].is_object());
    }
}

fn main() {
    println!("Text Statistics Plugin Example");
    println!("==============================\n");

    let tool = TextStatisticsTool::new();

    println!("Plugin: {}", tool.name());
    println!("Version: {}", tool.version());
    println!("Description: {}\n", tool.description());

    let sample_text =
        "The quick brown fox jumps over the lazy dog.\nThis is a test of the text analysis tool.";

    println!("Sample text:");
    println!("{}\n", sample_text);

    let analysis = tool.analyze_text(sample_text);

    println!("Analysis (summary):");
    println!("{}\n", tool.format_as_text(&analysis));

    println!("Analysis (verbose) - demonstrates config usage:");
    let mut verbose_config = PluginConfig::new("text-statistics");
    verbose_config.set("verbose", ConfigValue::Bool(true));
    let verbose_tool = TextStatisticsTool::with_config(verbose_config);
    println!("{}\n", verbose_tool.format_as_text(&analysis));

    println!("Analysis (JSON):");
    println!("{}", tool.format_as_json(&analysis));
}
