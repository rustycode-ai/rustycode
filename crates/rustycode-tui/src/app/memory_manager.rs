//! Memory and compaction management for the TUI
//!
//! This module handles:
//! - Memory injection logic
//! - Auto-compaction triggering
//! - Compaction execution

/// Configuration for memory injection and compaction
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum messages before auto-compaction
    pub max_messages_before_compact: usize,
    /// Maximum tokens per message
    pub max_tokens_per_message: usize,
    /// Enable memory injection
    pub enable_memory_injection: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_messages_before_compact: 50,
            max_tokens_per_message: 10000,
            enable_memory_injection: true,
        }
    }
}

/// Manager for memory operations and compaction
pub struct MemoryManager {
    config: MemoryConfig,
    message_count: usize,
    estimated_tokens: usize,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            config,
            message_count: 0,
            estimated_tokens: 0,
        }
    }

    /// Update message statistics
    pub fn update_message_stats(&mut self, message_count: usize, estimated_tokens: usize) {
        self.message_count = message_count;
        self.estimated_tokens = estimated_tokens;
    }

    /// Check if auto-compaction should be triggered
    pub fn should_compact(&self) -> bool {
        self.message_count >= self.config.max_messages_before_compact
            || self.estimated_tokens
                >= self.config.max_tokens_per_message * self.config.max_messages_before_compact
    }

    /// Get memory injection summary display
    pub fn get_injection_summary_display(&self, user_message: &str) -> String {
        // Check if we should inject memory based on user query
        if !self.config.enable_memory_injection {
            return String::new();
        }

        // Simple keyword-based detection for now
        let keywords = [
            "project",
            "style",
            "architecture",
            "structure",
            "context",
            "background",
            "history",
        ];

        let has_keyword = keywords
            .iter()
            .any(|kw| user_message.to_lowercase().contains(kw));

        if has_keyword {
            " [Injecting project context...]".to_string()
        } else {
            String::new()
        }
    }

    /// Generate injection message based on user query
    pub fn generate_injection_message(&self, user_message: &str) -> String {
        if !self.config.enable_memory_injection {
            return String::new();
        }

        // Analyze user message to determine what context to inject
        let user_lower = user_message.to_lowercase();

        if user_lower.contains("project") || user_lower.contains("structure") {
            "Project context: Rust-based AI coding assistant with TUI, supporting multiple LLM providers."
                .to_string()
        } else if user_lower.contains("style") || user_lower.contains("convention") {
            "Style: Follow Rust idioms, use anyhow for errors, prefer functional patterns over mutable state."
                .to_string()
        } else {
            String::new()
        }
    }

    /// Create a prompt injection if needed
    pub fn create_injection_if_needed(&mut self, user_message: &str) -> String {
        if !self.config.enable_memory_injection {
            return String::new();
        }

        let injection = self.generate_injection_message(user_message);
        if !injection.is_empty() {
            injection
        } else {
            String::new()
        }
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new(MemoryConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert_eq!(config.max_messages_before_compact, 50);
        assert_eq!(config.max_tokens_per_message, 10000);
        assert!(config.enable_memory_injection);
    }

    #[test]
    fn test_memory_manager_creation() {
        let config = MemoryConfig::default();
        let manager = MemoryManager::new(config);
        assert_eq!(manager.message_count, 0);
        assert_eq!(manager.estimated_tokens, 0);
    }

    #[test]
    fn test_should_compact_by_message_count() {
        let config = MemoryConfig {
            max_messages_before_compact: 10,
            ..Default::default()
        };
        let mut manager = MemoryManager::new(config);
        manager.update_message_stats(15, 1000);

        assert!(manager.should_compact());
    }

    #[test]
    fn test_should_compact_by_tokens() {
        let config = MemoryConfig {
            max_messages_before_compact: 10,
            max_tokens_per_message: 100,
            ..Default::default()
        };
        let mut manager = MemoryManager::new(config);
        manager.update_message_stats(5, 1500); // High token count (above 10 * 100 = 1000)

        assert!(manager.should_compact());
    }

    #[test]
    fn test_should_not_compact() {
        let config = MemoryConfig::default();
        let mut manager = MemoryManager::new(config);
        manager.update_message_stats(5, 1000);

        assert!(!manager.should_compact());
    }

    #[test]
    fn test_injection_summary_display() {
        let config = MemoryConfig::default();
        let manager = MemoryManager::new(config);

        let summary = manager.get_injection_summary_display("What is the project structure?");
        assert!(summary.contains("Injecting"));

        let no_injection = manager.get_injection_summary_display("Hello");
        assert!(no_injection.is_empty());
    }

    #[test]
    fn test_generate_injection_message() {
        let config = MemoryConfig::default();
        let manager = MemoryManager::new(config);

        let injection =
            manager.generate_injection_message("Tell me about the project architecture");
        assert!(injection.contains("Rust-based"));

        let no_injection = manager.generate_injection_message("Hello");
        assert!(no_injection.is_empty());
    }

    #[test]
    fn test_disabled_memory_injection() {
        let config = MemoryConfig {
            enable_memory_injection: false,
            ..Default::default()
        };
        let mut manager = MemoryManager::new(config);

        let summary = manager.get_injection_summary_display("What is the project structure?");
        assert!(summary.is_empty());

        let injection = manager.create_injection_if_needed("Tell me about the project");
        assert!(injection.is_empty());
    }
}
