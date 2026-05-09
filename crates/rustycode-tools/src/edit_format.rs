//! Model-aware edit format selection.
//!
//! Different LLM models have varying capabilities when it comes to code editing.
//! Some models excel at structured search-replace operations, while others work
//! better with whole-file replacement or simple diffs.
//!
//! This module provides:
//! - `EditFormat` enum classifying available edit strategies
//! - Model family detection from model identifiers
//! - Capability-based format selection with fallback chains
//!
//! # Architecture
//!
//! The selection algorithm:
//! 1. Detect the model family from the model identifier string
//! 2. Look up the preferred edit formats for that family (ordered by preference)
//! 3. Filter by what's actually available in the tool registry
//! 4. Return the best available format, falling back gracefully
//!
//! # Example
//!
//! ```rust,ignore
//! use rustycode_tools::edit_format::{EditFormat, select_edit_format};
//!
//! let format = select_edit_format("claude-sonnet-4-6");
//! assert_eq!(format, EditFormat::ClaudeNative);
//!
//! let format = select_edit_format("gpt-4o");
//! assert_eq!(format, EditFormat::SearchReplace);
//!
//! let format = select_edit_format("unknown-model");
//! assert_eq!(format, EditFormat::SearchReplace); // safe default
//! ```

use serde::{Deserialize, Serialize};

// ── Edit Format Types ─────────────────────────────────────────────────────────

/// The edit format strategy to use for a given model.
///
/// Each variant maps to a specific edit tool or combination of tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EditFormat {
    /// Claude's native `text_editor` tool (`str_replace`, create, insert, view, undo)
    /// Best for: Claude models (opus, sonnet, haiku)
    ClaudeNative,
    /// Simple search-replace: find exact `old_text`, replace with `new_text`
    /// Best for: Most models that can follow structured instructions
    SearchReplace,
    /// Regex-enabled search and replace
    /// Best for: Models with strong regex understanding (GPT-4, Gemini)
    RegexReplace,
    /// Multi-file atomic edit operations
    /// Best for: Complex refactors across multiple files
    MultiEdit,
    /// Whole-file replacement (write entire file content)
    /// Best for: Models that struggle with partial edits
    WholeFile,
    /// Git patch / unified diff application
    /// Best for: Models that can produce valid diffs
    DiffPatch,
}

impl std::fmt::Display for EditFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClaudeNative => write!(f, "claude_native"),
            Self::SearchReplace => write!(f, "search_replace"),
            Self::RegexReplace => write!(f, "regex_replace"),
            Self::MultiEdit => write!(f, "multiedit"),
            Self::WholeFile => write!(f, "whole_file"),
            Self::DiffPatch => write!(f, "diff_patch"),
        }
    }
}

impl EditFormat {
    /// Get the tool name(s) that implement this edit format.
    pub const fn tool_names(&self) -> &'static [&'static str] {
        match self {
            Self::ClaudeNative => &["text_editor_20250124"],
            Self::SearchReplace => &["Edit"],
            Self::RegexReplace => &["search_replace"],
            Self::MultiEdit => &["multiedit"],
            Self::WholeFile => &["Write"],
            Self::DiffPatch => &["apply_patch"],
        }
    }

    /// Get the primary tool name for this format.
    pub fn primary_tool(&self) -> &'static str {
        self.tool_names()[0]
    }

    /// Human-readable description of the format.
    pub const fn description(&self) -> &'static str {
        match self {
            Self::ClaudeNative => "Claude native text editor (str_replace, create, insert)",
            Self::SearchReplace => "Simple search-replace (old_text → new_text)",
            Self::RegexReplace => "Regex-powered search and replace",
            Self::MultiEdit => "Multi-file atomic edit operations",
            Self::WholeFile => "Whole-file replacement",
            Self::DiffPatch => "Git patch / unified diff application",
        }
    }

    /// Get all available edit formats in preference order for general use.
    pub const fn all() -> &'static [Self] {
        &[
            Self::ClaudeNative,
            Self::SearchReplace,
            Self::MultiEdit,
            Self::RegexReplace,
            Self::DiffPatch,
            Self::WholeFile,
        ]
    }
}

// ── Model Family Detection ────────────────────────────────────────────────────

/// The family of LLM model, used to determine edit format preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModelFamily {
    /// Anthropic Claude models (opus, sonnet, haiku)
    Claude,
    /// `OpenAI` GPT models (gpt-4, gpt-4o, o1, o3)
    OpenAI,
    /// Google Gemini models
    Gemini,
    /// Meta Llama models (via various providers)
    Llama,
    /// Mistral models
    Mistral,
    /// `DeepSeek` models
    DeepSeek,
    /// Qwen models
    Qwen,
    /// Amazon Nova models
    Nova,
    /// Grok models
    Grok,
    /// Unknown model family
    Unknown,
}

impl std::fmt::Display for ModelFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Claude => write!(f, "claude"),
            Self::OpenAI => write!(f, "openai"),
            Self::Gemini => write!(f, "gemini"),
            Self::Llama => write!(f, "llama"),
            Self::Mistral => write!(f, "mistral"),
            Self::DeepSeek => write!(f, "deepseek"),
            Self::Qwen => write!(f, "qwen"),
            Self::Nova => write!(f, "nova"),
            Self::Grok => write!(f, "grok"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detect the model family from a model identifier string.
///
/// Handles common patterns:
/// - `claude-*` → Claude
/// - `gpt-*`, `o1-*`, `o3-*`, `chatgpt-*` → `OpenAI`
/// - `gemini-*`, `models/gemini-*` → Gemini
/// - `llama-*`, `meta-llama/*` → Llama
/// - `mistral-*`, `open-mistral-*` → Mistral
/// - `deepseek-*` → `DeepSeek`
/// - `qwen-*`, `Qwen/*` → Qwen
/// - `nova-*`, `amazon.nova-*` → Nova
/// - `grok-*` → Grok
pub fn detect_model_family(model_id: &str) -> ModelFamily {
    let lower = model_id.to_lowercase();

    // Anthropic Claude
    if lower.starts_with("claude") {
        return ModelFamily::Claude;
    }

    // OpenAI GPT and reasoning models
    if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("chatgpt")
        || lower.contains("ft:gpt")
    {
        return ModelFamily::OpenAI;
    }

    // Google Gemini
    if lower.starts_with("gemini") || lower.contains("models/gemini") {
        return ModelFamily::Gemini;
    }

    // Meta Llama
    if lower.starts_with("llama") || lower.contains("meta-llama") || lower.contains("llama-") {
        return ModelFamily::Llama;
    }

    // Mistral
    if lower.starts_with("mistral")
        || lower.starts_with("open-mistral")
        || lower.starts_with("codestral")
        || lower.contains("mistral-large")
        || lower.contains("mistral-small")
    {
        return ModelFamily::Mistral;
    }

    // DeepSeek
    if lower.starts_with("deepseek") {
        return ModelFamily::DeepSeek;
    }

    // Qwen
    if lower.starts_with("qwen") {
        return ModelFamily::Qwen;
    }

    // Amazon Nova
    if lower.starts_with("nova") || lower.contains("amazon.nova") {
        return ModelFamily::Nova;
    }

    // Grok
    if lower.starts_with("grok") {
        return ModelFamily::Grok;
    }

    ModelFamily::Unknown
}

// ── Format Selection ───────────────────────────────────────────────────────────

/// Get the preferred edit formats for a model family, ordered by preference.
///
/// The first format the model handles well is preferred. Later formats
/// are fallbacks when earlier ones aren't available.
pub const fn preferred_formats(family: ModelFamily) -> &'static [EditFormat] {
    match family {
        // Claude models: native text editor is best, fallback to search-replace
        ModelFamily::Claude => &[
            EditFormat::ClaudeNative,
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        // OpenAI models: strong at search-replace and multiedit
        ModelFamily::OpenAI => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::RegexReplace,
            EditFormat::WholeFile,
        ],
        // Gemini: good at structured edits
        ModelFamily::Gemini => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        // Open-weight models: simpler formats work better
        ModelFamily::Llama => &[EditFormat::SearchReplace, EditFormat::WholeFile],
        ModelFamily::Mistral => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        ModelFamily::DeepSeek => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::DiffPatch,
        ],
        ModelFamily::Qwen => &[EditFormat::SearchReplace, EditFormat::WholeFile],
        ModelFamily::Nova => &[EditFormat::SearchReplace, EditFormat::WholeFile],
        ModelFamily::Grok => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        // Unknown models: safest defaults
        ModelFamily::Unknown => &[EditFormat::SearchReplace, EditFormat::WholeFile],
    }
}

/// Select the best edit format for a given model identifier.
///
/// Returns the primary preferred format for the detected model family.
/// Use `select_with_fallback` if you need fallback chain selection.
pub fn select_edit_format(model_id: &str) -> EditFormat {
    let family = detect_model_family(model_id);
    let formats = preferred_formats(family);
    formats[0]
}

/// Select the best edit format for a model, falling back through alternatives.
///
/// Takes a list of available tool names and returns the first preferred
/// format whose tool is available. If no preferred format's tool is
/// available, falls back to `SearchReplace` as the universal default.
pub fn select_with_fallback(model_id: &str, available_tools: &[&str]) -> EditFormat {
    let family = detect_model_family(model_id);
    let formats = preferred_formats(family);

    for format in formats {
        if format
            .tool_names()
            .iter()
            .any(|t| available_tools.contains(t))
        {
            return *format;
        }
    }

    // Universal fallback
    EditFormat::SearchReplace
}

/// Get the full fallback chain for a model.
///
/// Returns all formats in preference order, useful for providing
/// the LLM with alternative edit strategies.
pub fn fallback_chain(model_id: &str) -> Vec<EditFormat> {
    let family = detect_model_family(model_id);
    preferred_formats(family).to_vec()
}

/// Check if a model supports a specific edit format.
pub fn supports_format(model_id: &str, format: EditFormat) -> bool {
    let family = detect_model_family(model_id);
    preferred_formats(family).contains(&format)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Model family detection tests
    #[test]
    fn test_detect_claude_family() {
        assert_eq!(
            detect_model_family("claude-sonnet-4-6"),
            ModelFamily::Claude
        );
        assert_eq!(detect_model_family("claude-opus-4-6"), ModelFamily::Claude);
        assert_eq!(
            detect_model_family("claude-haiku-4-5-20251001"),
            ModelFamily::Claude
        );
        assert_eq!(
            detect_model_family("claude-sonnet-4-6"),
            ModelFamily::Claude
        );
    }

    #[test]
    fn test_detect_openai_family() {
        assert_eq!(detect_model_family("gpt-4o"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("gpt-4-turbo"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("gpt-3.5-turbo"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("o1-preview"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("o3-mini"), ModelFamily::OpenAI);
        assert_eq!(
            detect_model_family("chatgpt-4o-latest"),
            ModelFamily::OpenAI
        );
    }

    #[test]
    fn test_detect_gemini_family() {
        assert_eq!(detect_model_family("gemini-2.0-flash"), ModelFamily::Gemini);
        assert_eq!(detect_model_family("gemini-1.5-pro"), ModelFamily::Gemini);
        assert_eq!(
            detect_model_family("models/gemini-ultra"),
            ModelFamily::Gemini
        );
    }

    #[test]
    fn test_detect_llama_family() {
        assert_eq!(detect_model_family("llama-3.1-70b"), ModelFamily::Llama);
        assert_eq!(
            detect_model_family("meta-llama/Llama-3-8B"),
            ModelFamily::Llama
        );
    }

    #[test]
    fn test_detect_mistral_family() {
        assert_eq!(
            detect_model_family("mistral-large-latest"),
            ModelFamily::Mistral
        );
        assert_eq!(
            detect_model_family("open-mistral-nemo"),
            ModelFamily::Mistral
        );
        assert_eq!(
            detect_model_family("codestral-latest"),
            ModelFamily::Mistral
        );
    }

    #[test]
    fn test_detect_deepseek_family() {
        assert_eq!(detect_model_family("deepseek-coder"), ModelFamily::DeepSeek);
        assert_eq!(detect_model_family("deepseek-chat"), ModelFamily::DeepSeek);
    }

    #[test]
    fn test_detect_qwen_family() {
        assert_eq!(detect_model_family("qwen-2.5-coder"), ModelFamily::Qwen);
    }

    #[test]
    fn test_detect_nova_family() {
        assert_eq!(detect_model_family("nova-pro"), ModelFamily::Nova);
        assert_eq!(detect_model_family("amazon.nova-micro"), ModelFamily::Nova);
    }

    #[test]
    fn test_detect_grok_family() {
        assert_eq!(detect_model_family("grok-2"), ModelFamily::Grok);
    }

    #[test]
    fn test_detect_unknown_family() {
        assert_eq!(
            detect_model_family("some-random-model"),
            ModelFamily::Unknown
        );
        assert_eq!(detect_model_family(""), ModelFamily::Unknown);
    }

    // Format selection tests
    #[test]
    fn test_claude_gets_native_editor() {
        assert_eq!(
            select_edit_format("claude-sonnet-4-6"),
            EditFormat::ClaudeNative
        );
        assert_eq!(
            select_edit_format("claude-opus-4-6"),
            EditFormat::ClaudeNative
        );
    }

    #[test]
    fn test_gpt_gets_search_replace() {
        assert_eq!(select_edit_format("gpt-4o"), EditFormat::SearchReplace);
    }

    #[test]
    fn test_gemini_gets_search_replace() {
        assert_eq!(
            select_edit_format("gemini-2.0-flash"),
            EditFormat::SearchReplace
        );
    }

    #[test]
    fn test_llama_gets_search_replace() {
        assert_eq!(
            select_edit_format("llama-3.1-70b"),
            EditFormat::SearchReplace
        );
    }

    #[test]
    fn test_unknown_gets_safe_default() {
        assert_eq!(
            select_edit_format("unknown-model"),
            EditFormat::SearchReplace
        );
    }

    // Fallback tests
    #[test]
    fn test_select_with_fallback_claude() {
        let available = &["Edit", "Write", "search_replace"];
        // Claude prefers ClaudeNative but edit_file (SearchReplace) is available
        let format = select_with_fallback("claude-sonnet-4-6", available);
        assert_eq!(format, EditFormat::SearchReplace);
    }

    #[test]
    fn test_select_with_fallback_native_available() {
        let available = &["text_editor_20250124", "Edit", "Write"];
        let format = select_with_fallback("claude-sonnet-4-6", available);
        assert_eq!(format, EditFormat::ClaudeNative);
    }

    #[test]
    fn test_select_with_fallback_nothing_available() {
        let available: &[&str] = &[];
        let format = select_with_fallback("gpt-4o", available);
        assert_eq!(format, EditFormat::SearchReplace); // universal fallback
    }

    // Tool mapping tests
    #[test]
    fn test_tool_names_mapping() {
        assert!(EditFormat::ClaudeNative
            .tool_names()
            .contains(&"text_editor_20250124"));
        assert!(EditFormat::SearchReplace.tool_names().contains(&"Edit"));
        assert!(EditFormat::RegexReplace
            .tool_names()
            .contains(&"search_replace"));
        assert!(EditFormat::MultiEdit.tool_names().contains(&"multiedit"));
        assert!(EditFormat::WholeFile.tool_names().contains(&"Write"));
        assert!(EditFormat::DiffPatch.tool_names().contains(&"apply_patch"));
    }

    #[test]
    fn test_primary_tool() {
        assert_eq!(
            EditFormat::ClaudeNative.primary_tool(),
            "text_editor_20250124"
        );
        assert_eq!(EditFormat::SearchReplace.primary_tool(), "Edit");
        assert_eq!(EditFormat::WholeFile.primary_tool(), "Write");
    }

    // Supports format tests
    #[test]
    fn test_supports_format() {
        assert!(supports_format(
            "claude-sonnet-4-6",
            EditFormat::ClaudeNative
        ));
        assert!(supports_format(
            "claude-sonnet-4-6",
            EditFormat::SearchReplace
        ));
        assert!(!supports_format("gpt-4o", EditFormat::ClaudeNative));
        assert!(supports_format("gpt-4o", EditFormat::SearchReplace));
    }

    // Fallback chain tests
    #[test]
    fn test_fallback_chain_claude() {
        let chain = fallback_chain("claude-sonnet-4-6");
        assert_eq!(chain[0], EditFormat::ClaudeNative);
        assert_eq!(chain[1], EditFormat::SearchReplace);
        assert!(chain.len() >= 3);
    }

    #[test]
    fn test_fallback_chain_gpt() {
        let chain = fallback_chain("gpt-4o");
        assert_eq!(chain[0], EditFormat::SearchReplace);
        assert_eq!(chain[1], EditFormat::MultiEdit);
    }

    // Display tests
    #[test]
    fn test_edit_format_display() {
        assert_eq!(EditFormat::ClaudeNative.to_string(), "claude_native");
        assert_eq!(EditFormat::SearchReplace.to_string(), "search_replace");
        assert_eq!(EditFormat::WholeFile.to_string(), "whole_file");
    }

    #[test]
    fn test_model_family_display() {
        assert_eq!(ModelFamily::Claude.to_string(), "claude");
        assert_eq!(ModelFamily::OpenAI.to_string(), "openai");
        assert_eq!(ModelFamily::Unknown.to_string(), "unknown");
    }

    // Description tests
    #[test]
    fn test_edit_format_description() {
        assert!(!EditFormat::ClaudeNative.description().is_empty());
        assert!(!EditFormat::SearchReplace.description().is_empty());
    }

    // Serialization tests
    #[test]
    fn test_edit_format_serialization() {
        let format = EditFormat::ClaudeNative;
        let json = serde_json::to_string(&format).unwrap();
        assert!(json.contains("ClaudeNative"));
        let back: EditFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(format, back);
    }
}
