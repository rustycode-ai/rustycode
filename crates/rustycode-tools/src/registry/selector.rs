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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Check if text contains a word (not just substring)
/// Uses word boundaries to avoid false positives
/// Optimized with lazy static regex compilation
fn contains_word(text: &str, word: &str) -> bool {
    // Handle multi-word phrases
    if word.contains(' ') {
        return text.contains(word);
    }

    // Single word - check with word boundaries
    // Use cached regex to avoid repeated compilation
    let pattern = format!(r"\b{}\b", regex::escape(word));
    static WORD_CACHE: std::sync::LazyLock<
        parking_lot::Mutex<lru::LruCache<String, regex::Regex>>,
    > = std::sync::LazyLock::new(|| {
        parking_lot::Mutex::new(lru::LruCache::new(
            std::num::NonZeroUsize::new(128).unwrap(),
        ))
    });

    let mut cache = WORD_CACHE.lock();

    // Try to get from cache first
    if let Some(re) = cache.get(&pattern) {
        return re.is_match(text);
    }

    // Not in cache, compile and insert
    let re = regex::Regex::new(&pattern).unwrap_or_else(|_| regex::Regex::new(r"\b").unwrap());
    let is_match = re.is_match(text);
    cache.put(pattern, re);

    is_match
}

/// Tool usage profile for different workflows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ToolProfile {
    /// Code exploration and discovery
    Explore,
    /// Implementation and changes
    Implement,
    /// Debugging and diagnosis
    Debug,
    /// Operations and maintenance
    Ops,
    /// Refactoring and code restructuring
    Refactor,
    /// All tools available (default)
    All,
}

impl ToolProfile {
    pub fn required_tags(&self) -> &[&'static str] {
        match self {
            ToolProfile::Explore => &["explore"],
            ToolProfile::Implement => &["implement"],
            ToolProfile::Debug => &["debug"],
            ToolProfile::Ops => &["ops"],
            ToolProfile::Refactor => &["refactor"],
            ToolProfile::All => &[],
        }
    }

    /// Get tools that should NOT appear in suggestions for this profile
    pub fn filtered_suggestions(&self) -> &[&'static str] {
        // Global filters - tools that should never be suggested
        &["invalid", "patch", "batch", "internal"]
    }

    /// Detect profile from prompt content using weighted scoring
    ///
    /// Each profile gets a score based on keyword matches. The profile with
    /// the highest score wins, with a minimum threshold to avoid false positives.
    pub fn from_prompt(prompt: &str) -> Self {
        let lower = prompt.to_lowercase();

        // Define weighted keywords for each profile
        // (keyword, weight) - higher weight = more specific
        let explore_keywords = [
            // Question words (weight 3 - very specific to exploration)
            ("what", 3),
            ("how", 3),
            ("where", 3),
            ("which", 3),
            ("explain", 3),
            ("understand", 3),
            ("show", 3),
            ("display", 3),
            // Exploration actions (weight 2)
            ("find", 2),
            ("inspect", 2),
            ("search", 2),
            ("list", 2),
            ("explore", 2),
            ("look at", 2),
            ("check", 2),
            ("read", 2),
            ("see", 2),
            // Context words (weight 1)
            ("structure", 1),
            ("architecture", 1),
            ("overview", 1),
            // Semantic search triggers (weight 2)
            ("logic", 2),
            ("implementation", 2),
            ("pattern", 2),
            ("handle", 2),
            ("validate", 2),
        ];

        let implement_keywords = [
            // Creation (weight 3)
            ("create", 3),
            ("write", 3),
            ("implement", 3),
            ("add", 3),
            ("generate", 3),
            ("make", 3),
            ("build", 2), // build can also be ops
            // Modification (weight 2)
            ("refactor", 2),
            ("change", 2),
            ("update", 2),
            ("modify", 2),
            ("edit", 2),
            ("improve", 2),
        ];

        let debug_keywords = [
            // Debug-specific (weight 3)
            ("debug", 3),
            ("diagnose", 3),
            ("investigate", 2),
            ("troubleshoot", 3),
            // Error states (weight 3)
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
            // Debug context (weight 2 - higher than implement's "fix")
            ("why", 2),
            ("fix", 2), // fix can also be implement, but debug fix is specific
        ];

        let ops_keywords = [
            // Operations (weight 3)
            ("deploy", 3),
            ("release", 3),
            ("restart", 3),
            ("stop", 2),
            // Execution (weight 2)
            ("run", 2),
            ("execute", 2),
            ("start", 2),
            ("install", 2),
            // Git (weight 2)
            ("commit", 2),
            ("push", 2),
            ("git", 2),
            // Build/package (weight 2)
            ("build", 2),
            ("test", 2),
            ("cargo", 1),
            ("npm", 1),
        ];

        // Score each profile
        let mut explore_score = 0;
        let mut implement_score = 0;
        let mut debug_score = 0;
        let mut ops_score = 0;

        // Calculate scores with word boundary detection for accuracy
        for (keyword, weight) in explore_keywords {
            if contains_word(&lower, keyword) {
                explore_score += weight;
            }
        }
        for (keyword, weight) in implement_keywords {
            if contains_word(&lower, keyword) {
                implement_score += weight;
            }
        }
        for (keyword, weight) in debug_keywords {
            if contains_word(&lower, keyword) {
                debug_score += weight;
            }
        }
        for (keyword, weight) in ops_keywords {
            if contains_word(&lower, keyword) {
                ops_score += weight;
            }
        }

        // Find highest score
        let max_score = explore_score
            .max(implement_score)
            .max(debug_score)
            .max(ops_score);

        // Need minimum score to avoid false positives
        if max_score < 2 {
            return ToolProfile::All;
        }

        // Return profile with highest score
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
}

/// Tracks tool usage statistics for ranking
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageTracker {
    /// Count of uses per tool
    uses: HashMap<String, usize>,
    /// Last used timestamp (seconds since epoch)
    last_used: HashMap<String, u64>,
}

impl UsageTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool use
    /// Optimized to reduce allocations by reusing the tool string
    pub fn record_use(&mut self, tool: &str) {
        let tool_owned = tool.to_string();
        *self.uses.entry(tool_owned.clone()).or_insert(0) += 1;
        self.last_used.insert(
            tool_owned,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    /// Get usage count for a tool
    pub fn usage_count(&self, tool: &str) -> usize {
        self.uses.get(tool).copied().unwrap_or(0)
    }

    /// Get most recently used tools
    pub fn recent_tools(&self, limit: usize) -> Vec<String> {
        let mut tools: Vec<_> = self
            .last_used
            .iter()
            .map(|(tool, time)| (tool.clone(), *time))
            .collect();

        tools.sort_by_key(|a| std::cmp::Reverse(a.1));
        tools
            .into_iter()
            .take(limit)
            .map(|(tool, _)| tool)
            .collect()
    }

    /// Get most frequently used tools
    pub fn frequent_tools(&self, limit: usize) -> Vec<String> {
        let mut tools: Vec<_> = self
            .uses
            .iter()
            .map(|(tool, count)| (tool.clone(), *count))
            .collect();

        tools.sort_by_key(|a| std::cmp::Reverse(a.1));
        tools
            .into_iter()
            .take(limit)
            .map(|(tool, _)| tool)
            .collect()
    }

    /// Get comprehensive usage statistics for all tools
    pub fn get_statistics(&self) -> Vec<(String, usize, Option<u64>)> {
        let all_tools: std::collections::HashSet<_> =
            self.uses.keys().chain(self.last_used.keys()).collect();

        all_tools
            .into_iter()
            .map(|tool| {
                let count = self.usage_count(tool);
                let last_used = self.last_used.get(tool).copied();
                (tool.clone(), count, last_used)
            })
            .collect()
    }

    /// Get total number of tool uses across all tools
    pub fn total_uses(&self) -> usize {
        self.uses.values().sum()
    }

    /// Get number of unique tools used
    pub fn unique_tools(&self) -> usize {
        self.uses.len()
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
    pub fn with_profile(mut self, profile: ToolProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Set the model identifier for model-aware edit format selection.
    ///
    /// When set, `select_tools()` and `predict_from_prompt()` will include
    /// the model's preferred edit tools and exclude edit tools that the model
    /// doesn't handle well.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let selector = ToolSelector::new()
    ///     .with_model("claude-sonnet-4-6");
    /// // Claude models get text_editor_20250728 instead of generic "edit"
    /// ```
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

    /// Get ranked tools for current profile, adjusted for model capabilities.
    ///
    /// When a model is set via `with_model()`, edit tools are adjusted:
    /// - The model's preferred edit format tools are included
    /// - Generic "edit" is replaced with the model-specific tool
    /// - Tools the model doesn't support are deprioritized
    pub fn select_tools(&self) -> Vec<String> {
        let mut available: Vec<String> = self
            .profile
            .available_tools()
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Model-aware edit format adjustment
        if let Some(ref model) = self.model_id {
            let format = edit_format::select_edit_format(model);

            // Add the model's primary edit tool if not already present
            let primary = format.primary_tool().to_string();
            if !available.contains(&primary) {
                // Remove generic "edit" and replace with model-specific tool
                available.retain(|t| t != "edit");
                available.push(primary);
            }

            // For the Implement profile, ensure the model's full edit chain is available
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
    pub fn suggest_tools(&self) -> Vec<String> {
        let available = self.select_tools();
        let filtered: Vec<String> = available
            .into_iter()
            .filter(|tool| !self.profile.filtered_suggestions().contains(&tool.as_str()))
            .collect();

        filtered
    }

    /// Predict which tools might be needed based on prompt
    pub fn predict_from_prompt(&self, prompt: &str) -> Vec<String> {
        let profile = ToolProfile::from_prompt(prompt);
        let mut tools: Vec<String> = profile
            .available_tools()
            .iter()
            .map(|s| s.to_string())
            .collect();

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
    fn test_profile_detection() {
        assert_eq!(
            ToolProfile::from_prompt("Show me the main function"),
            ToolProfile::Explore
        );
        assert_eq!(
            ToolProfile::from_prompt("Create a new user model"),
            ToolProfile::Implement
        );
        assert_eq!(
            ToolProfile::from_prompt("Debug this authentication error"),
            ToolProfile::Debug
        );
        assert_eq!(
            ToolProfile::from_prompt("Deploy to production"),
            ToolProfile::Ops
        );

        // Additional edge cases
        assert_eq!(
            ToolProfile::from_prompt("What is the main function?"),
            ToolProfile::Explore
        );
        assert_eq!(
            ToolProfile::from_prompt("How does this work?"),
            ToolProfile::Explore
        );
        assert_eq!(
            ToolProfile::from_prompt("Add a new feature"),
            ToolProfile::Implement
        );
    }

    #[test]
    fn test_explore_profile_tools() {
        let tools = ToolProfile::Explore.available_tools();
        assert!(tools.contains(&"read_file"));
        assert!(tools.contains(&"grep"));
        assert!(!tools.contains(&"write_file"));
    }

    #[test]
    fn test_usage_tracking() {
        let mut tracker = UsageTracker::new();
        tracker.record_use("read_file");
        tracker.record_use("read_file");
        tracker.record_use("bash");

        assert_eq!(tracker.usage_count("read_file"), 2);
        assert_eq!(tracker.usage_count("bash"), 1);
        assert_eq!(tracker.usage_count("grep"), 0);
    }

    #[test]
    fn test_tool_selector_with_profile() {
        let selector = ToolSelector::new().with_profile(ToolProfile::Explore);

        let tools = selector.select_tools();
        assert!(tools.iter().any(|t| t == "read_file"));
        assert!(tools.iter().any(|t| t == "grep"));
    }

    #[test]
    fn test_tool_selector_custom_filters() {
        let selector = ToolSelector::new()
            .always_include("custom_tool")
            .always_exclude("bash");

        let tools = selector.select_tools();
        assert!(tools.iter().any(|t| t == "custom_tool"));
        assert!(!tools.iter().any(|t| t == "bash"));
    }

    #[test]
    fn test_prediction_from_prompt() {
        let mut selector = ToolSelector::new();
        selector.record_use("read_file");
        selector.record_use("read_file");

        let predicted = selector.predict_from_prompt("Show me authentication code");
        assert!(predicted.iter().any(|t| t == "read_file"));
        // read_file should be near the top due to high usage
        assert_eq!(predicted[0], "read_file");
    }

    // ── Model-Aware Edit Format Integration Tests ────────────────────────────────

    #[test]
    fn test_claude_model_gets_native_editor() {
        let selector = ToolSelector::new()
            .with_profile(ToolProfile::Implement)
            .with_model("claude-sonnet-4-6");

        let tools = selector.select_tools();
        // Claude models should get text_editor tool
        assert!(
            tools.iter().any(|t| t == "text_editor_20250728"),
            "Claude should get text_editor_20250728, got: {:?}",
            tools
        );
    }

    #[test]
    fn test_gpt_model_gets_edit_file() {
        let selector = ToolSelector::new()
            .with_profile(ToolProfile::Implement)
            .with_model("gpt-4o");

        let tools = selector.select_tools();
        // GPT models prefer edit_file (SearchReplace)
        assert!(
            tools.iter().any(|t| t == "edit_file"),
            "GPT should get edit_file, got: {:?}",
            tools
        );
    }

    #[test]
    fn test_model_replaces_generic_edit() {
        let selector = ToolSelector::new()
            .with_profile(ToolProfile::Implement)
            .with_model("claude-sonnet-4-6");

        let tools = selector.select_tools();
        // Generic "edit" should be replaced with model-specific tool
        assert!(
            !tools.iter().any(|t| t == "edit"),
            "Generic 'edit' should be replaced with model-specific tool, got: {:?}",
            tools
        );
    }

    #[test]
    fn test_edit_format_accessor() {
        let selector = ToolSelector::new().with_model("claude-sonnet-4-6");
        assert_eq!(selector.edit_format(), Some(EditFormat::ClaudeNative));

        let selector = ToolSelector::new().with_model("gpt-4o");
        assert_eq!(selector.edit_format(), Some(EditFormat::SearchReplace));

        let selector = ToolSelector::new();
        assert_eq!(selector.edit_format(), None);
    }

    #[test]
    fn test_all_profile_with_model_includes_full_chain() {
        let selector = ToolSelector::new()
            .with_profile(ToolProfile::All)
            .with_model("claude-opus-4-6");

        let tools = selector.select_tools();
        // All profile with Claude should include both text_editor versions
        assert!(tools.iter().any(|t| t == "text_editor_20250728"));
        assert!(tools.iter().any(|t| t == "text_editor_20250124"));
    }

    #[test]
    fn test_no_model_keeps_edit_file() {
        let selector = ToolSelector::new().with_profile(ToolProfile::Implement);

        let tools = selector.select_tools();
        // Without model, "edit_file" should be present
        assert!(
            tools.iter().any(|t| t == "edit_file"),
            "Without model, 'edit_file' should be present, got: {:?}",
            tools
        );
    }
}
