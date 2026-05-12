//! Intelligent tool selection system
//!
//! Provides context-aware tool selection inspired by `OpenCode`:
//! - Multi-level filtering (global, agent, context)
//! - Usage-based ranking
//! - Keyword prediction
//!
//! ## Tool Profiles
//!
//! - **Explore**: `read_file`, `list_dir`, grep, glob (code discovery)
//! - **Implement**: `write_file`, edit, bash, test (code changes)
//! - **Debug**: `lsp_diagnostics`, `lsp_hover`, bash, grep (debugging)
//! - **Ops**: git, bash, `web_fetch` (operations)
//! - **All**: All tools available

use crate::edit_format::{self, EditFormat};
use crate::tool_names as tn;
use crate::tool_selection::{ToolProfile, UsageTracker};
use crate::ToolRegistry;
use std::collections::HashSet;

/// Check if text contains a word (not just substring).
/// Uses word boundaries to avoid false positives.
/// Optimized with lazy static regex compilation.
fn contains_word(text: &str, word: &str) -> bool {
    // Single word - check with word boundaries
    // Use cached regex to avoid repeated compilation
    static WORD_CACHE: std::sync::LazyLock<
        parking_lot::Mutex<lru::LruCache<String, regex::Regex>>,
    > = std::sync::LazyLock::new(|| {
        parking_lot::Mutex::new(lru::LruCache::new(
            #[allow(clippy::expect_used)]
            std::num::NonZeroUsize::new(128).expect("128 is nonzero"),
        ))
    });

    // Handle multi-word phrases
    if word.contains(' ') {
        return text.contains(word);
    }

    let pattern = format!(r"\b{word}\b");

    let mut cache = WORD_CACHE.lock();

    // Try to get from cache first
    if let Some(re) = cache.get(&pattern) {
        return re.is_match(text);
    }

    // Not in cache, compile and insert
    #[allow(clippy::expect_used)]
    let re = regex::Regex::new(&pattern)
        .unwrap_or_else(|_| regex::Regex::new(r"\b\w+\b").expect("valid regex"));
    let is_match = re.is_match(text);
    cache.put(pattern, re);

    is_match
}

/// Compute a weighted keyword score for a profile.
fn score_keywords(text: &str, keywords: &[(&str, usize)]) -> usize {
    keywords
        .iter()
        .map(|&(keyword, weight)| {
            if contains_word(text, keyword) {
                weight
            } else {
                0
            }
        })
        .sum()
}

/// Detect a tool profile from prompt content using weighted scoring.
///
/// Each profile gets a score based on keyword matches. The profile with
/// the highest score wins, with a minimum threshold to avoid false positives.
pub fn profile_from_prompt(prompt: &str) -> ToolProfile {
    let lower = prompt.to_lowercase();

    let explore_keywords = [
        ("what", 3),
        ("how", 3),
        ("where", 3),
        ("which", 3),
        ("explain", 3),
        ("understand", 3),
        ("show", 3),
        ("display", 3),
        ("find", 2),
        ("inspect", 2),
        ("search", 2),
        ("list", 2),
        ("explore", 2),
        ("look at", 2),
        ("check", 2),
        ("read", 2),
        ("see", 2),
        ("structure", 1),
        ("architecture", 1),
        ("overview", 1),
        ("logic", 2),
        ("implementation", 2),
        ("pattern", 2),
        ("handle", 2),
        ("validate", 2),
    ];

    let implement_keywords = [
        ("create", 3),
        ("write", 3),
        ("implement", 3),
        ("add", 3),
        ("generate", 3),
        ("make", 3),
        ("build", 2),
        ("refactor", 2),
        ("change", 2),
        ("update", 2),
        ("modify", 2),
        ("edit", 2),
        ("improve", 2),
    ];

    let debug_keywords = [
        ("debug", 3),
        ("diagnose", 3),
        ("investigate", 2),
        ("troubleshoot", 3),
        ("error", 3),
        ("bug", 3),
        ("issue", 2),
        ("broken", 2),
        ("fail", 2),
        ("failing", 3),
        ("failure", 3),
        ("crash", 2),
        ("panic", 2),
        ("leak", 2),
        ("why", 2),
        ("fix", 2),
    ];

    let ops_keywords = [
        ("deploy", 3),
        ("release", 3),
        ("restart", 3),
        ("stop", 2),
        ("run", 2),
        ("execute", 2),
        ("start", 2),
        ("install", 2),
        ("commit", 2),
        ("push", 2),
        ("git", 2),
        ("build", 2),
        ("test", 2),
        ("cargo", 1),
        ("npm", 1),
    ];

    let explore_score = score_keywords(&lower, &explore_keywords);
    let implement_score = score_keywords(&lower, &implement_keywords);
    let debug_score = score_keywords(&lower, &debug_keywords);
    let ops_score = score_keywords(&lower, &ops_keywords);

    let max_score = explore_score
        .max(implement_score)
        .max(debug_score)
        .max(ops_score);

    if max_score < 2 {
        return ToolProfile::All;
    }

    if max_score == explore_score {
        ToolProfile::Explore
    } else if max_score == implement_score {
        ToolProfile::Implement
    } else if max_score == debug_score {
        ToolProfile::Debug
    } else {
        ToolProfile::Ops
    }
}

/// Selects and ranks tools based on context and usage
#[derive(Debug, Clone)]
pub struct ToolSelector {
    profile: ToolProfile,
    usage: UsageTracker,
    /// Custom override: always include these tools
    always_include: HashSet<String>,
    /// Custom override: always exclude these tools
    always_exclude: HashSet<String>,
    /// Model identifier for model-aware edit format selection
    model_id: Option<String>,
}

impl Default for ToolSelector {
    fn default() -> Self {
        Self {
            profile: ToolProfile::All,
            usage: UsageTracker::new(),
            always_include: HashSet::new(),
            always_exclude: HashSet::new(),
            model_id: None,
        }
    }
}

impl ToolSelector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tool profile
    pub const fn with_profile(mut self, profile: ToolProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Set the model identifier for model-aware edit format selection.
    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = Some(model_id.into());
        self
    }

    /// Get the current edit format based on model, if set.
    pub fn edit_format(&self) -> Option<EditFormat> {
        self.model_id
            .as_deref()
            .map(edit_format::select_edit_format)
    }

    /// Add a tool to always include
    pub fn always_include(mut self, tool: impl Into<String>) -> Self {
        self.always_include.insert(tool.into());
        self
    }

    /// Add a tool to always exclude
    pub fn always_exclude(mut self, tool: impl Into<String>) -> Self {
        self.always_exclude.insert(tool.into());
        self
    }

    /// Record a tool usage (updates ranking)
    pub fn record_use(&mut self, tool: &str) {
        self.usage.record_use(tool);
    }

    /// Get tools that should be filtered from suggestions for the current profile.
    const fn filtered_suggestions() -> &'static [&'static str] {
        &["invalid", "patch", "batch", "internal"]
    }

    /// Get ranked tools for current profile, adjusted for model capabilities.
    ///
    /// Uses tag-based autodiscovery from the registry instead of hardcoded lists.
    pub fn select_tools(&self, registry: &ToolRegistry) -> Vec<String> {
        let tags = self.profile.required_tags();
        let mut available: Vec<String> = if tags.is_empty() {
            // ToolProfile::All — return everything
            registry.list_all_names()
        } else {
            registry
                .list_for_tags(&tags)
                .into_iter()
                .map(|info| info.name)
                .collect()
        };

        // Model-aware edit format adjustment
        if let Some(ref model) = self.model_id {
            let format = edit_format::select_edit_format(model);

            let primary = format.primary_tool().to_string();
            if !available.contains(&primary) {
                available.retain(|t| t != tn::EDIT);
                available.push(primary);
            }

            if self.profile == ToolProfile::Implement || self.profile == ToolProfile::All {
                for tool_name in format.tool_names() {
                    let name = tool_name.to_string();
                    if !available.contains(&name) {
                        available.push(name);
                    }
                }
            }
        }

        // Add always_include tools
        for tool in &self.always_include {
            if !available.contains(tool) {
                available.push(tool.clone());
            }
        }

        // Remove always_exclude tools
        available.retain(|tool| !self.always_exclude.contains(tool));

        // Sort by usage frequency (most used first)
        available.sort_by(|a, b| {
            let count_a = self.usage.usage_count(a);
            let count_b = self.usage.usage_count(b);
            count_b.cmp(&count_a)
        });

        available
    }

    /// Get tools that should appear in suggestions
    pub fn suggest_tools(&self, registry: &ToolRegistry) -> Vec<String> {
        let available = self.select_tools(registry);
        available
            .into_iter()
            .filter(|tool| !Self::filtered_suggestions().contains(&tool.as_str()))
            .collect()
    }

    /// Predict which tools might be needed based on prompt
    pub fn predict_from_prompt(&self, prompt: &str, registry: &ToolRegistry) -> Vec<String> {
        let profile = profile_from_prompt(prompt);
        let tags = profile.required_tags();
        let mut tools: Vec<String> = if tags.is_empty() {
            registry.list_all_names()
        } else {
            registry
                .list_for_tags(&tags)
                .into_iter()
                .map(|info| info.name)
                .collect()
        };

        // Boost frequently used tools to the top
        tools.sort_by(|a, b| {
            let count_a = self.usage.usage_count(a);
            let count_b = self.usage.usage_count(b);
            count_b.cmp(&count_a)
        });

        // Return top 10 predicted tools
        tools.into_iter().take(10).collect()
    }

    /// Convert tool list to format suitable for LLM
    pub fn format_tools_for_llm(&self, tools: &[String]) -> String {
        tools.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_detection_explore() {
        assert_eq!(
            profile_from_prompt("Show me the main function"),
            ToolProfile::Explore
        );
    }

    #[test]
    fn test_profile_detection_implement() {
        assert_eq!(
            profile_from_prompt("Create a new user model"),
            ToolProfile::Implement
        );
    }

    #[test]
    fn test_profile_detection_debug() {
        assert_eq!(
            profile_from_prompt("Debug this authentication error"),
            ToolProfile::Debug
        );
    }

    #[test]
    fn test_profile_detection_ops() {
        assert_eq!(
            profile_from_prompt("Deploy to production"),
            ToolProfile::Ops
        );
    }

    #[test]
    fn test_profile_detection_questions() {
        assert_eq!(
            profile_from_prompt("What is the main function?"),
            ToolProfile::Explore
        );
        assert_eq!(
            profile_from_prompt("How does this work?"),
            ToolProfile::Explore
        );
        assert_eq!(
            profile_from_prompt("Add a new feature"),
            ToolProfile::Implement
        );
    }

    #[test]
    fn test_explore_profile_tools() {
        let tools = ToolProfile::Explore.available_tools();
        assert!(tools.contains(&"Read"));
        assert!(tools.contains(&"Grep"));
        assert!(!tools.contains(&"Write"));
    }

    #[test]
    fn test_usage_tracking() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("Read");
        tracker.record_use("Read");
        tracker.record_use("Bash");

        assert_eq!(tracker.usage_count("Read"), 2);
        assert_eq!(tracker.usage_count("Bash"), 1);
        assert_eq!(tracker.usage_count("Grep"), 0);
    }

    // ToolSelector integration tests requiring &ToolRegistry are in rustycode-tools crate
    // These were removed as they require a full ToolRegistry implementation

    // select_tools() requires a &ToolRegistry parameter, so this test is skipped
    // until integration tests can be properly set up

    #[test]
    fn test_edit_format_accessor() {
        let selector = ToolSelector::new().with_model("claude-sonnet-4-6");
        assert_eq!(selector.edit_format(), Some(EditFormat::ClaudeNative));

        let selector = ToolSelector::new().with_model("gpt-4o");
        assert_eq!(selector.edit_format(), Some(EditFormat::SearchReplace));

        let selector = ToolSelector::new();
        assert_eq!(selector.edit_format(), None);
    }

    // Integration tests requiring &ToolRegistry are skipped in unit tests
    // They will be covered by integration tests in rustycode-tools
}
