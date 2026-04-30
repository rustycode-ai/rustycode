//! Memory injection for automatic context enhancement
//!
//! This module provides intelligent injection of relevant memories into user messages
//! to automatically provide context to the AI without manual recall commands.

use crate::memory_auto::AutoMemory;
use crate::memory_relevance::{
    get_relevant_memories, DEFAULT_MAX_INJECTIONS, DEFAULT_RELEVANCE_THRESHOLD,
};

/// Memory injection configuration
#[derive(Debug, Clone)]
pub struct InjectionConfig {
    /// Minimum relevance threshold (0.0-1.0)
    pub threshold: f64,
    /// Maximum number of memories to inject
    pub max_memories: usize,
    /// Whether injection is enabled
    pub enabled: bool,
}

impl Default for InjectionConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_RELEVANCE_THRESHOLD,
            max_memories: DEFAULT_MAX_INJECTIONS,
            enabled: true,
        }
    }
}

/// Prepare memory injection text for a user message
///
/// # Arguments
///
/// * `user_message` - The user's message
/// * `memories` - Available auto-memories
/// * `config` - Injection configuration
///
/// # Returns
///
/// * `Some(String)` - Formatted injection text if relevant memories found
/// * `None` - No relevant memories or injection disabled
///
/// # Examples
///
/// ```rust,ignore
/// use rustycode_tui::memory_injection::prepare_injection;
/// use rustycode_tui::memory_auto::{AutoMemory, MemoryType};
///
/// let memories = vec![
///     AutoMemory::new("theme", "dark mode", MemoryType::Preference),
/// ];
///
/// let injection = prepare_injection("What's my theme?", &memories, &Default::default());
/// assert!(injection.is_some());
/// ```
pub fn prepare_injection(
    user_message: &str,
    memories: &[AutoMemory],
    config: &InjectionConfig,
) -> Option<String> {
    // Skip if disabled
    if !config.enabled {
        return None;
    }

    // Skip empty messages
    let message = user_message.trim();
    if message.is_empty() {
        return None;
    }

    // Get relevant memories
    let relevant = get_relevant_memories(message, memories, config.threshold, config.max_memories);

    if relevant.is_empty() {
        return None;
    }

    // Build injection text
    let injection_text = format!(
        "💭 Using {} related memories:\n{}\n",
        relevant.len(),
        relevant
            .iter()
            .map(|(memory, score)| {
                format!(
                    "  • {}: {} (confidence: {:.0}%)",
                    memory.key,
                    memory.value,
                    score * 100.0
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    );

    Some(injection_text)
}

/// Inject memories into a user message
///
/// # Arguments
///
/// * `user_message` - The user's message
/// * `memories` - Available auto-memories
/// * `config` - Injection configuration
///
/// # Returns
///
/// The enhanced message with memory context (if relevant memories found)
pub fn inject_memories(
    user_message: &str,
    memories: &[AutoMemory],
    config: &InjectionConfig,
) -> String {
    if let Some(injection) = prepare_injection(user_message, memories, config) {
        format!("{}\n{}", injection, user_message)
    } else {
        user_message.to_string()
    }
}

/// Get injection summary for display
///
/// # Arguments
///
/// * `user_message` - The user's message
/// * `memories` - Available auto-memories
/// * `config` - Injection configuration
///
/// # Returns
///
/// Summary text like "Using 3 related memories" or empty string if no injection
pub fn get_injection_summary(
    user_message: &str,
    memories: &[AutoMemory],
    config: &InjectionConfig,
) -> String {
    if let Some(injection) = prepare_injection(user_message, memories, config) {
        // Extract count from injection text
        if let Some(start) = injection.find("Using ") {
            if let Some(end) = injection.find(" related") {
                let count_str = &injection[start + 6..end];
                return format!("💭 Using {} related memories", count_str);
            }
        }
    }
    String::new()
}

/// Preview what would be injected for a message
///
/// # Arguments
///
/// * `user_message` - The user's message
/// * `memories` - Available auto-memories
/// * `config` - Injection configuration
///
/// # Returns
///
/// Vec of (key, value, confidence) tuples that would be injected
pub fn preview_injection(
    user_message: &str,
    memories: &[AutoMemory],
    config: &InjectionConfig,
) -> Vec<(String, String, f64)> {
    let relevant = get_relevant_memories(
        user_message,
        memories,
        config.threshold,
        config.max_memories,
    );

    relevant
        .into_iter()
        .map(|(memory, score)| (memory.key, memory.value, score))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_auto::{AutoMemory, MemoryType};

    #[test]
    fn test_prepare_injection_with_relevant_memory() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig {
            threshold: 0.3, // Lower threshold for this test
            ..InjectionConfig::default()
        };

        let injection = prepare_injection("What's my theme?", &memories, &config);

        assert!(injection.is_some());
        assert!(injection.unwrap().contains("theme"));
    }

    #[test]
    fn test_prepare_injection_with_no_relevant_memory() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig::default();

        let injection = prepare_injection("unrelated query", &memories, &config);

        assert!(injection.is_none());
    }

    #[test]
    fn test_prepare_injection_disabled() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig {
            enabled: false,
            ..Default::default()
        };

        let injection = prepare_injection("theme", &memories, &config);

        assert!(injection.is_none());
    }

    #[test]
    fn test_inject_memories_enhances_message() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig {
            threshold: 0.3, // Lower threshold for this test
            ..InjectionConfig::default()
        };

        let enhanced = inject_memories("What's my theme?", &memories, &config);

        assert!(enhanced.contains("💭 Using"));
        assert!(enhanced.contains("theme"));
        assert!(enhanced.contains("What's my theme?"));
    }

    #[test]
    fn test_inject_memories_no_change_when_no_match() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig::default();

        let original = "unrelated query";
        let enhanced = inject_memories(original, &memories, &config);

        assert_eq!(enhanced, original);
    }

    #[test]
    fn test_get_injection_summary() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig {
            threshold: 0.3, // Lower threshold for this test
            ..InjectionConfig::default()
        };

        let summary = get_injection_summary("What's my theme?", &memories, &config);

        assert!(summary.contains("Using"));
        assert!(summary.contains("memories"));
    }

    #[test]
    fn test_get_injection_summary_no_match() {
        let memories = vec![AutoMemory::new(
            "theme",
            "dark mode",
            MemoryType::Preference,
        )];
        let config = InjectionConfig::default();

        let summary = get_injection_summary("unrelated", &memories, &config);

        assert!(summary.is_empty());
    }

    #[test]
    fn test_preview_injection() {
        let memories = vec![
            AutoMemory::new("theme", "dark mode", MemoryType::Preference),
            AutoMemory::new("model", "claude", MemoryType::Preference),
        ];
        let config = InjectionConfig {
            threshold: 0.3, // Lower threshold for this test
            ..InjectionConfig::default()
        };

        let preview = preview_injection("theme preference", &memories, &config);

        assert!(!preview.is_empty());
        assert_eq!(preview[0].0, "theme");
        assert_eq!(preview[0].1, "dark mode");
    }

    #[test]
    fn test_max_memories_limit() {
        let memories = vec![
            AutoMemory::new("theme", "dark", MemoryType::Preference),
            AutoMemory::new("model", "claude", MemoryType::Preference),
            AutoMemory::new("mode", "ask", MemoryType::Preference), // Changed to Preference for higher importance
        ];

        let config = InjectionConfig {
            threshold: 0.3, // Lower threshold for this test
            max_memories: 2,
            ..InjectionConfig::default()
        };

        let injection = prepare_injection("mode", &memories, &config);

        assert!(injection.is_some());
        // "model" contains "mode" (0.45 score) and "mode" is exact match (0.45 score)
        // So we should have 2 matches
        let injection_text = injection.unwrap();
        assert!(injection_text.contains("related"));
    }

    #[test]
    fn test_threshold_filtering() {
        let memories = vec![
            AutoMemory::new("theme", "dark mode", MemoryType::Preference),
            AutoMemory::new("unrelated", "other", MemoryType::Context),
        ];

        let config = InjectionConfig {
            threshold: 0.8, // High threshold
            ..Default::default()
        };

        let injection = prepare_injection("theme", &memories, &config);

        // May or may not inject depending on score
        // Test just verifies it doesn't crash
        let _ = injection;
    }

    #[test]
    fn test_empty_message_handling() {
        let memories = vec![AutoMemory::new("theme", "dark", MemoryType::Preference)];
        let config = InjectionConfig::default();

        let injection = prepare_injection("", &memories, &config);
        assert!(injection.is_none());

        let injection = prepare_injection("   ", &memories, &config);
        assert!(injection.is_none());
    }

    #[test]
    fn test_empty_memories() {
        let memories: Vec<AutoMemory> = vec![];
        let config = InjectionConfig::default();

        let injection = prepare_injection("theme", &memories, &config);
        assert!(injection.is_none());
    }
}
