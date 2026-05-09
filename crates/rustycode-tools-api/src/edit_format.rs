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

use serde::{Deserialize, Serialize};

// ── Edit Format Types ─────────────────────────────────────────────────────────

/// The edit format strategy to use for a given model.
///
/// Each variant maps to a specific edit tool or combination of tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EditFormat {
    /// Claude's native `text_editor` tool (`str_replace`, create, insert, view, undo)
    ClaudeNative,
    /// Simple search-replace: find exact `old_text`, replace with `new_text`
    SearchReplace,
    /// Regex-enabled search and replace
    RegexReplace,
    /// Multi-file atomic edit operations
    MultiEdit,
    /// Whole-file replacement (write entire file content)
    WholeFile,
    /// Git patch / unified diff application
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
            Self::ClaudeNative => &["text_editor_20250728", "text_editor_20250124"],
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
pub fn detect_model_family(model_id: &str) -> ModelFamily {
    let lower = model_id.to_lowercase();

    if lower.starts_with("claude") {
        return ModelFamily::Claude;
    }
    if lower.starts_with("gpt")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("chatgpt")
        || lower.contains("ft:gpt")
    {
        return ModelFamily::OpenAI;
    }
    if lower.starts_with("gemini") || lower.contains("models/gemini") {
        return ModelFamily::Gemini;
    }
    if lower.starts_with("llama") || lower.contains("meta-llama") || lower.contains("llama-") {
        return ModelFamily::Llama;
    }
    if lower.starts_with("mistral")
        || lower.starts_with("open-mistral")
        || lower.starts_with("codestral")
        || lower.contains("mistral-large")
        || lower.contains("mistral-small")
    {
        return ModelFamily::Mistral;
    }
    if lower.starts_with("deepseek") {
        return ModelFamily::DeepSeek;
    }
    if lower.starts_with("qwen") {
        return ModelFamily::Qwen;
    }
    if lower.starts_with("nova") || lower.contains("amazon.nova") {
        return ModelFamily::Nova;
    }
    if lower.starts_with("grok") {
        return ModelFamily::Grok;
    }

    ModelFamily::Unknown
}

// ── Format Selection ───────────────────────────────────────────────────────────

/// Get the preferred edit formats for a model family, ordered by preference.
pub const fn preferred_formats(family: ModelFamily) -> &'static [EditFormat] {
    match family {
        ModelFamily::Claude => &[
            EditFormat::ClaudeNative,
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        ModelFamily::OpenAI => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::RegexReplace,
            EditFormat::WholeFile,
        ],
        ModelFamily::Gemini | ModelFamily::Mistral | ModelFamily::Grok => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::WholeFile,
        ],
        ModelFamily::DeepSeek => &[
            EditFormat::SearchReplace,
            EditFormat::MultiEdit,
            EditFormat::DiffPatch,
        ],
        ModelFamily::Llama | ModelFamily::Qwen | ModelFamily::Nova | ModelFamily::Unknown => {
            &[EditFormat::SearchReplace, EditFormat::WholeFile]
        }
    }
}

/// Select the best edit format for a given model identifier.
pub fn select_edit_format(model_id: &str) -> EditFormat {
    let family = detect_model_family(model_id);
    let formats = preferred_formats(family);
    formats[0]
}

/// Select the best edit format for a model, falling back through alternatives.
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

    EditFormat::SearchReplace
}

/// Get the full fallback chain for a model.
pub fn fallback_chain(model_id: &str) -> Vec<EditFormat> {
    let family = detect_model_family(model_id);
    preferred_formats(family).to_vec()
}

/// Check if a model supports a specific edit format.
pub fn supports_format(model_id: &str, format: EditFormat) -> bool {
    let family = detect_model_family(model_id);
    preferred_formats(family).contains(&format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_claude_family() {
        assert_eq!(
            detect_model_family("claude-sonnet-4-6"),
            ModelFamily::Claude
        );
        assert_eq!(detect_model_family("claude-opus-4-6"), ModelFamily::Claude);
    }

    #[test]
    fn test_detect_openai_family() {
        assert_eq!(detect_model_family("gpt-4o"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("o1-preview"), ModelFamily::OpenAI);
        assert_eq!(detect_model_family("o3-mini"), ModelFamily::OpenAI);
    }

    #[test]
    fn test_detect_gemini_family() {
        assert_eq!(detect_model_family("gemini-2.0-flash"), ModelFamily::Gemini);
    }

    #[test]
    fn test_detect_unknown_family() {
        assert_eq!(
            detect_model_family("some-random-model"),
            ModelFamily::Unknown
        );
    }

    #[test]
    fn test_claude_gets_native_editor() {
        assert_eq!(
            select_edit_format("claude-sonnet-4-6"),
            EditFormat::ClaudeNative
        );
    }

    #[test]
    fn test_gpt_gets_search_replace() {
        assert_eq!(select_edit_format("gpt-4o"), EditFormat::SearchReplace);
    }

    #[test]
    fn test_unknown_gets_safe_default() {
        assert_eq!(
            select_edit_format("unknown-model"),
            EditFormat::SearchReplace
        );
    }

    #[test]
    fn test_primary_tool() {
        assert_eq!(
            EditFormat::ClaudeNative.primary_tool(),
            "text_editor_20250728"
        );
        assert_eq!(EditFormat::SearchReplace.primary_tool(), "Edit");
    }

    #[test]
    fn test_edit_format_serialization() {
        let format = EditFormat::ClaudeNative;
        let json = serde_json::to_string(&format).unwrap();
        let back: EditFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(format, back);
    }
}
