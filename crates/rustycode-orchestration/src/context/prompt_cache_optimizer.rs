// rustycode-orchestra/src/prompt_cache_optimizer.rs
//! Prompt Cache Optimizer — separates prompt content into cacheable static
//! prefixes and dynamic per-task suffixes to maximize provider cache hit rates.
//!
//! Anthropic caches by prefix match (up to 4 breakpoints, 90% savings).
//! `OpenAI` auto-caches prompts with 1024+ stable prefix tokens (50% savings).
//! Both benefit from placing static content first and dynamic content last.

use serde::{Deserialize, Serialize};

// Type Definitions

/// Content classification for cache optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ContentRole {
    /// Static content (reused across tasks)
    #[serde(rename = "static")]
    Static,

    /// Semi-static content (reused within scope)
    #[serde(rename = "semi-static")]
    SemiStatic,

    /// Dynamic content (per-task)
    #[serde(rename = "dynamic")]
    Dynamic,
}

/// A labeled section of prompt content with its cache role
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSection {
    /// Identifier for this section (for metrics/debugging)
    pub label: String,

    /// The content string
    pub content: String,

    /// Cache role: static (reused across tasks), semi-static (reused within scope), dynamic (per-task)
    pub role: ContentRole,
}

/// Result of optimizing prompt sections for caching
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheOptimizedPrompt {
    /// Assembled prompt with static content first, dynamic last
    pub prompt: String,

    /// Character count of the cacheable prefix (static + semi-static sections)
    pub cacheable_prefix_chars: usize,

    /// Total character count
    pub total_chars: usize,

    /// Estimated cache efficiency: cacheablePrefixChars / totalChars
    pub cache_efficiency: f64,

    /// Number of sections by role
    pub section_counts: SectionCounts,
}

/// Section counts by role
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SectionCounts {
    #[serde(default)]
    pub static_count: usize,

    #[serde(default)]
    pub semi_static_count: usize,

    #[serde(default)]
    pub dynamic_count: usize,
}

/// Cache hit rate usage metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheUsage {
    pub cache_read: usize,
    pub cache_write: usize,
    pub input: usize,
}

/// Provider type for savings estimation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Provider {
    Anthropic,
    OpenAI,
    Other,
}

// Label Classification

/// Labels that never change within a session
const STATIC_LABELS: &[&str] = &["system-prompt", "base-instructions", "executor-constraints"];

/// Prefix patterns for static labels (e.g. "template-*")
const STATIC_PREFIXES: &[&str] = &["template-"];

/// Labels that change per-slice but not per-task
const SEMI_STATIC_LABELS: &[&str] = &[
    "slice-plan",
    "decisions",
    "requirements",
    "roadmap",
    "prior-summaries",
    "project-context",
    "overrides",
];

/// Labels that change per-task
const DYNAMIC_LABELS: &[&str] = &[
    "task-plan",
    "task-instructions",
    "task-context",
    "file-contents",
    "diff-context",
    "verification-commands",
];

// Public API

/// Classify common Orchestra prompt sections by their caching potential.
/// Returns the appropriate `ContentRole` for a section label.
///
pub fn classify_section(label: &str) -> ContentRole {
    if STATIC_LABELS.contains(&label) {
        return ContentRole::Static;
    }

    if STATIC_PREFIXES.iter().any(|p| label.starts_with(p)) {
        return ContentRole::Static;
    }

    if SEMI_STATIC_LABELS.contains(&label) {
        return ContentRole::SemiStatic;
    }

    if DYNAMIC_LABELS.contains(&label) {
        return ContentRole::Dynamic;
    }

    // Conservative default: unknown labels are treated as dynamic
    ContentRole::Dynamic
}

/// Build a `PromptSection` from content with automatic role classification.
///
pub fn section(
    label: impl Into<String>,
    content: impl Into<String>,
    role: Option<ContentRole>,
) -> PromptSection {
    let label = label.into();
    let content = content.into();
    let role = role.unwrap_or_else(|| classify_section(&label));

    PromptSection {
        label,
        content,
        role,
    }
}

/// Optimize prompt sections for maximum cache hit rates.
/// Reorders sections: static first, then semi-static, then dynamic.
/// Preserves relative order within each role group.
///
pub fn optimize_for_caching(sections: &[PromptSection]) -> CacheOptimizedPrompt {
    let mut groups = [
        Vec::new(), // Static
        Vec::new(), // Semi-Static
        Vec::new(), // Dynamic
    ];

    // Group sections by role
    for s in sections {
        match s.role {
            ContentRole::Static => groups[0].push(s),
            ContentRole::SemiStatic => groups[1].push(s),
            ContentRole::Dynamic => groups[2].push(s),
        }
    }

    // Build ordered list: static -> semi-static -> dynamic
    let ordered: Vec<&PromptSection> = groups[0]
        .iter()
        .chain(groups[1].iter())
        .chain(groups[2].iter())
        .copied()
        .collect();

    // Join content with double newlines
    let prompt = ordered
        .iter()
        .map(|s| s.content.as_str())
        .collect::<Vec<&str>>()
        .join("\n\n");

    // Calculate character counts
    let static_chars: usize = groups[0].iter().map(|s| s.content.len()).sum();
    let semi_static_chars: usize = groups[1].iter().map(|s| s.content.len()).sum();

    // Account for separator characters between sections
    let static_separators = if groups[0].is_empty() {
        0
    } else {
        (groups[0].len() - 1) * 2 // "\n\n" between static sections
    };

    let semi_static_separators = if groups[1].is_empty() {
        0
    } else {
        (groups[1].len() - 1) * 2
    };

    // Separator between static and semi-static groups
    let group_separator = if !groups[0].is_empty() && !groups[1].is_empty() {
        2
    } else {
        0
    };

    let cacheable_prefix_chars = static_chars
        + semi_static_chars
        + static_separators
        + semi_static_separators
        + group_separator;

    let total_chars = prompt.len();
    let cache_efficiency = if total_chars > 0 {
        (cacheable_prefix_chars as f64) / (total_chars as f64)
    } else {
        0.0
    };

    CacheOptimizedPrompt {
        prompt,
        cacheable_prefix_chars,
        total_chars,
        cache_efficiency,
        section_counts: SectionCounts {
            static_count: groups[0].len(),
            semi_static_count: groups[1].len(),
            dynamic_count: groups[2].len(),
        },
    }
}

/// Estimate the cache savings for a given optimization result.
/// Based on provider pricing:
/// - Anthropic: 90% savings on cached tokens
/// - `OpenAI`: 50% savings on cached tokens
///
pub fn estimate_cache_savings(result: &CacheOptimizedPrompt, provider: Provider) -> f64 {
    match provider {
        Provider::Anthropic => result.cache_efficiency * 0.9,
        Provider::OpenAI => result.cache_efficiency * 0.5,
        Provider::Other => 0.0,
    }
}

/// Compute cache hit rate from token usage metrics.
/// Returns a percentage 0-100.
///
pub fn compute_cache_hit_rate(usage: CacheUsage) -> f64 {
    let denominator = usage.cache_read + usage.input;
    if denominator == 0 {
        return 0.0;
    }
    ((usage.cache_read as f64) / (denominator as f64)) * 100.0
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_section_static() {
        assert_eq!(classify_section("system-prompt"), ContentRole::Static);
        assert_eq!(classify_section("base-instructions"), ContentRole::Static);
        assert_eq!(
            classify_section("executor-constraints"),
            ContentRole::Static
        );
    }

    #[test]
    fn test_classify_section_static_prefix() {
        assert_eq!(classify_section("template-custom"), ContentRole::Static);
        assert_eq!(classify_section("template-v1"), ContentRole::Static);
    }

    #[test]
    fn test_classify_section_semi_static() {
        assert_eq!(classify_section("slice-plan"), ContentRole::SemiStatic);
        assert_eq!(classify_section("decisions"), ContentRole::SemiStatic);
        assert_eq!(classify_section("requirements"), ContentRole::SemiStatic);
        assert_eq!(classify_section("roadmap"), ContentRole::SemiStatic);
        assert_eq!(classify_section("prior-summaries"), ContentRole::SemiStatic);
        assert_eq!(classify_section("project-context"), ContentRole::SemiStatic);
        assert_eq!(classify_section("overrides"), ContentRole::SemiStatic);
    }

    #[test]
    fn test_classify_section_dynamic() {
        assert_eq!(classify_section("task-plan"), ContentRole::Dynamic);
        assert_eq!(classify_section("task-instructions"), ContentRole::Dynamic);
        assert_eq!(classify_section("task-context"), ContentRole::Dynamic);
        assert_eq!(classify_section("file-contents"), ContentRole::Dynamic);
        assert_eq!(classify_section("diff-context"), ContentRole::Dynamic);
        assert_eq!(
            classify_section("verification-commands"),
            ContentRole::Dynamic
        );
    }

    #[test]
    fn test_classify_section_unknown() {
        assert_eq!(classify_section("unknown-label"), ContentRole::Dynamic);
        assert_eq!(classify_section("custom-section"), ContentRole::Dynamic);
    }

    #[test]
    fn test_section_auto_classify() {
        let s = section("system-prompt", "You are helpful", None);
        assert_eq!(s.role, ContentRole::Static);
        assert_eq!(s.label, "system-prompt");
        assert_eq!(s.content, "You are helpful");
    }

    #[test]
    fn test_section_explicit_role() {
        let s = section("custom", "Content", Some(ContentRole::Static));
        assert_eq!(s.role, ContentRole::Static);
    }

    #[test]
    fn test_optimize_for_caching_ordering() {
        let sections = vec![
            section("task-instructions", "Task 1", None),
            section("system-prompt", "System", None),
            section("slice-plan", "Plan", None),
            section("task-instructions", "Task 2", None),
        ];

        let result = optimize_for_caching(&sections);

        // Should start with static content
        assert!(result.prompt.starts_with("System"));
        // Dynamic content should be last
        assert!(result.prompt.ends_with("Task 2"));
    }

    #[test]
    fn test_optimize_for_caching_efficiency() {
        let sections = vec![
            section("system-prompt", "Static content", None),
            section("task-instructions", "Dynamic", None),
        ];

        let result = optimize_for_caching(&sections);

        // Static content is 14 chars, which is the cacheable prefix
        assert_eq!(result.cacheable_prefix_chars, 14);
        assert_eq!(result.total_chars, 23); // "Static content\n\nDynamic" = 14 + 2 + 7
        assert_eq!(result.cache_efficiency, 14.0 / 23.0);
        assert!(result.cache_efficiency > 0.5);
        assert!(result.cache_efficiency < 1.0);
    }

    #[test]
    fn test_optimize_for_caching_section_counts() {
        let sections = vec![
            section("system-prompt", "S1", None),
            section("base-instructions", "S2", None),
            section("slice-plan", "SS1", None),
            section("task-instructions", "D1", None),
        ];

        let result = optimize_for_caching(&sections);

        assert_eq!(result.section_counts.static_count, 2);
        assert_eq!(result.section_counts.semi_static_count, 1);
        assert_eq!(result.section_counts.dynamic_count, 1);
    }

    #[test]
    fn test_optimize_for_caching_empty() {
        let sections = vec![];
        let result = optimize_for_caching(&sections);

        assert_eq!(result.prompt, "");
        assert_eq!(result.cacheable_prefix_chars, 0);
        assert_eq!(result.total_chars, 0);
        assert_eq!(result.cache_efficiency, 0.0);
        assert_eq!(result.section_counts.static_count, 0);
        assert_eq!(result.section_counts.semi_static_count, 0);
        assert_eq!(result.section_counts.dynamic_count, 0);
    }

    #[test]
    fn test_optimize_for_caching_all_static() {
        let sections = vec![
            section("system-prompt", "S1", None),
            section("base-instructions", "S2", None),
        ];

        let result = optimize_for_caching(&sections);

        // S1 (2) + S2 (2) + separator (2) = 6
        assert_eq!(result.cacheable_prefix_chars, 6);
        assert_eq!(result.total_chars, 6);
        assert_eq!(result.cache_efficiency, 1.0); // 100% cacheable
    }

    #[test]
    fn test_optimize_for_caching_all_dynamic() {
        let sections = vec![
            section("task-instructions", "D1", None),
            section("task-context", "D2", None),
        ];

        let result = optimize_for_caching(&sections);

        assert_eq!(result.cacheable_prefix_chars, 0); // Nothing cacheable
        assert_eq!(result.cache_efficiency, 0.0); // 0% cacheable
    }

    #[test]
    fn test_estimate_cache_savings_anthropic() {
        let sections = vec![
            section("system-prompt", "Static", None),
            section("task-instructions", "Dynamic", None),
        ];

        let result = optimize_for_caching(&sections);
        let savings = estimate_cache_savings(&result, Provider::Anthropic);

        assert!(savings > 0.0);
        assert!(savings < 1.0);
    }

    #[test]
    fn test_estimate_cache_savings_openai() {
        let sections = vec![
            section("system-prompt", "Static", None),
            section("task-instructions", "Dynamic", None),
        ];

        let result = optimize_for_caching(&sections);
        let savings = estimate_cache_savings(&result, Provider::OpenAI);

        assert!(savings > 0.0);
        assert!(savings < 1.0);
    }

    #[test]
    fn test_estimate_cache_savings_other() {
        let sections = vec![
            section("system-prompt", "Static", None),
            section("task-instructions", "Dynamic", None),
        ];

        let result = optimize_for_caching(&sections);
        let savings = estimate_cache_savings(&result, Provider::Other);

        assert_eq!(savings, 0.0);
    }

    #[test]
    fn test_estimate_cache_savings_comparison() {
        let sections = vec![
            section("system-prompt", "Static", None),
            section("task-instructions", "Dynamic", None),
        ];

        let result = optimize_for_caching(&sections);

        let anthropic = estimate_cache_savings(&result, Provider::Anthropic);
        let openai = estimate_cache_savings(&result, Provider::OpenAI);

        // Anthropic has higher savings rate (90% vs 50%)
        assert!(anthropic > openai);
    }

    #[test]
    fn test_compute_cache_hit_rate() {
        let usage = CacheUsage {
            cache_read: 1000,
            cache_write: 100,
            input: 500,
        };

        let hit_rate = compute_cache_hit_rate(usage);

        assert!((hit_rate - 66.666).abs() < 0.01); // ~66.67%
    }

    #[test]
    fn test_compute_cache_hit_rate_zero_denominator() {
        let usage = CacheUsage {
            cache_read: 0,
            cache_write: 0,
            input: 0,
        };

        let hit_rate = compute_cache_hit_rate(usage);

        assert_eq!(hit_rate, 0.0);
    }

    #[test]
    fn test_compute_cache_hit_rate_no_cache() {
        let usage = CacheUsage {
            cache_read: 0,
            cache_write: 0,
            input: 1000,
        };

        let hit_rate = compute_cache_hit_rate(usage);

        assert_eq!(hit_rate, 0.0);
    }

    #[test]
    fn test_compute_cache_hit_rate_perfect() {
        let usage = CacheUsage {
            cache_read: 1000,
            cache_write: 100,
            input: 0,
        };

        let hit_rate = compute_cache_hit_rate(usage);

        assert_eq!(hit_rate, 100.0);
    }

    #[test]
    fn test_optimize_preserves_order_within_groups() {
        let sections = vec![
            section("task-instructions", "D1", None),
            section("system-prompt", "S1", None),
            section("base-instructions", "S2", None),
            section("task-context", "D2", None),
            section("slice-plan", "SS1", None),
        ];

        let result = optimize_for_caching(&sections);

        // Static sections should be in order: S1, S2
        let s1_pos = result.prompt.find("S1").unwrap();
        let s2_pos = result.prompt.find("S2").unwrap();
        assert!(s1_pos < s2_pos);

        // Dynamic sections should be in order: D1, D2
        let d1_pos = result.prompt.find("D1").unwrap();
        let d2_pos = result.prompt.find("D2").unwrap();
        assert!(d1_pos < d2_pos);
    }

    #[test]
    fn test_section_counts_accurate() {
        let sections = vec![
            section("system-prompt", "S", None),
            section("base-instructions", "S2", None), // Changed from system-prompt-2
            section("slice-plan", "SS", None),
            section("task-instructions", "D", None),
        ];

        let result = optimize_for_caching(&sections);

        assert_eq!(result.section_counts.static_count, 2);
        assert_eq!(result.section_counts.semi_static_count, 1);
        assert_eq!(result.section_counts.dynamic_count, 1);
    }

    #[test]
    fn test_cacheable_prefix_includes_separators() {
        let sections = vec![
            section("system-prompt", "S1", None),
            section("base-instructions", "S2", None),
            section("slice-plan", "SS1", None),
        ];

        let result = optimize_for_caching(&sections);

        // S1 (2) + S2 (2) + SS1 (3) + separators = 11
        // Static chars: 2 + 2 = 4
        // Semi-static chars: 3
        // Static separators: (2-1)*2 = 2
        // Group separator: 2
        // Cacheable prefix: 4 + 3 + 2 + 0 + 2 = 11
        assert_eq!(result.cacheable_prefix_chars, 11);
        assert_eq!(result.total_chars, 11);
    }

    #[test]
    fn test_content_role_serialize() {
        // Test that ContentRole can be serialized/deserialized
        let role = ContentRole::Static;
        let json = serde_json::to_string(&role).unwrap();
        let deserialized: ContentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, deserialized);
    }

    #[test]
    fn test_content_role_serialize_semi_static() {
        let role = ContentRole::SemiStatic;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"semi-static\"");
        let deserialized: ContentRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, deserialized);
    }
}
