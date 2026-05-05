//! Tracks repeated AI errors (failed tools, build failures, test failures) and

use rustycode_protocol::CircularBuffer;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Mistake {
    pub timestamp: Instant,
    pub mistake_type: MistakeType,
    /// Tool or operation that failed
    pub operation: String,
    pub error: String,
    /// Additional context that might help diagnose
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MistakeType {
    ToolFailed,
    BuildFailed,
    TestsFailed,
    RepeatedOperation,
    FileNotFoundError,
    SyntaxError,
    TypeError,
    Other,
}

impl MistakeType {
    pub fn display_name(&self) -> &'static str {
        match self {
            MistakeType::ToolFailed => "Tool execution failed",
            MistakeType::BuildFailed => "Build failed",
            MistakeType::TestsFailed => "Tests failed",
            MistakeType::RepeatedOperation => "Repeated operation",
            MistakeType::FileNotFoundError => "File not found",
            MistakeType::SyntaxError => "Syntax error",
            MistakeType::TypeError => "Type error",
            MistakeType::Other => "Error",
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RecoveryStrategy {
    SuggestAlternative {
        description: String,
        example: String,
    },
    EnableDebugMode,
    AskForGuidance {
        question: String,
    },
    ReviewContext,
    SimplifyApproach {
        suggestion: String,
    },
}

impl RecoveryStrategy {
    pub fn display_message(&self) -> String {
        match self {
            RecoveryStrategy::SuggestAlternative {
                description,
                example,
            } => {
                format!(
                    "💡 Try a different approach:\n{}\n\nExample: {}",
                    description, example
                )
            }
            RecoveryStrategy::EnableDebugMode => {
                "🔍 Enable debug mode to get more diagnostic information".to_string()
            }
            RecoveryStrategy::AskForGuidance { question } => {
                format!("❓ Need guidance:\n{}", question)
            }
            RecoveryStrategy::ReviewContext => {
                "📋 Taking a break to review the context and current state...".to_string()
            }
            RecoveryStrategy::SimplifyApproach { suggestion } => {
                format!("🔄 Try a simpler approach:\n{}", suggestion)
            }
        }
    }
}

const MAX_MISTAKE_HISTORY: usize = 100;

pub struct MistakeTracker {
    /// Bounded history of recent mistakes (auto-evicts oldest)
    mistakes: CircularBuffer<Mistake>,
    operation_counts: HashMap<String, usize>,
    max_mistakes: usize,
    warning_threshold: usize,
    /// Only mistakes within this duration are counted
    time_window: Duration,
}

impl MistakeTracker {
    pub fn new() -> Self {
        Self {
            mistakes: CircularBuffer::new(MAX_MISTAKE_HISTORY),
            operation_counts: HashMap::new(),
            max_mistakes: 5,
            warning_threshold: 3,
            time_window: Duration::from_mins(5), // 5 minutes
        }
    }

    pub fn record_mistake(
        &mut self,
        mistake_type: MistakeType,
        operation: String,
        error: String,
        context: String,
    ) {
        let mistake = Mistake {
            timestamp: Instant::now(),
            mistake_type,
            operation: operation.clone(),
            error,
            context,
        };

        // Clean up old mistakes outside time window
        self.cleanup_old_mistakes();

        // Add new mistake
        self.mistakes.push(mistake.clone());
        *self.operation_counts.entry(operation.clone()).or_insert(0) += 1;

        tracing::warn!(
            "Mistake recorded: {} - {} (total: {})",
            mistake_type.display_name(),
            operation,
            self.mistakes.len()
        );
    }

    pub fn should_warn(&mut self) -> bool {
        self.recent_mistake_count() >= self.warning_threshold
    }

    pub fn needs_recovery(&mut self) -> bool {
        self.recent_mistake_count() >= self.max_mistakes
    }

    pub fn recent_mistake_count(&mut self) -> usize {
        self.cleanup_old_mistakes();
        self.mistakes.len()
    }

    pub fn suggest_recovery(&mut self) -> Option<RecoveryStrategy> {
        if self.mistakes.is_empty() {
            return None;
        }

        let last_mistake = self.mistakes.last()?;
        let mistake_count = self.mistakes.len();

        // Analyze patterns in mistakes
        let same_operation_count = *self
            .operation_counts
            .get(&last_mistake.operation)
            .unwrap_or(&0);

        // Different strategies based on mistake patterns
        if same_operation_count >= 3 {
            // Same operation failing repeatedly - try something different
            return Some(RecoveryStrategy::SuggestAlternative {
                description: format!(
                    "The operation '{}' has failed {} times. This approach isn't working.",
                    last_mistake.operation, same_operation_count
                ),
                example: self
                    .get_alternative_example(&last_mistake.operation, &last_mistake.mistake_type),
            });
        }

        // Check for specific mistake types
        match &last_mistake.mistake_type {
            MistakeType::SyntaxError | MistakeType::TypeError => {
                Some(RecoveryStrategy::AskForGuidance {
                    question: "There are syntax/type errors. Should I:\n\
                         1. Review the full file for errors?\n\
                         2. Run a compiler/language server check?\n\
                         3. Try a different approach to fix this?"
                        .to_string(),
                })
            }
            MistakeType::BuildFailed | MistakeType::TestsFailed => {
                Some(RecoveryStrategy::SimplifyApproach {
                    suggestion: "Break down the changes into smaller, testable increments. \
                                  Make one change at a time and verify each step."
                        .to_string(),
                })
            }
            MistakeType::FileNotFoundError => Some(RecoveryStrategy::SuggestAlternative {
                description: "The file doesn't exist at that path. Let me search for it."
                    .to_string(),
                example: format!(
                    "Use 'glob' or 'grep' to find the correct file path for: {}",
                    last_mistake.operation
                ),
            }),
            _ if mistake_count >= 5 => Some(RecoveryStrategy::ReviewContext),
            _ => Some(RecoveryStrategy::EnableDebugMode),
        }
    }

    fn get_alternative_example(&self, operation: &str, mistake_type: &MistakeType) -> String {
        match (operation, mistake_type) {
            ("bash", _) => "Instead of bash, try using the specific tool (read_file, write_file, etc.) directly".to_string(),
            ("edit_file", _) => "Use apply_patch for multi-hunk changes, or read the file first to see its exact content".to_string(),
            (_, MistakeType::FileNotFoundError) => "Use 'glob' or 'list_dir' to find the correct file path first".to_string(),
            _ => "Let me try a different approach to solve this problem".to_string(),
        }
    }

    pub fn mistake_summary(&self) -> String {
        if self.mistakes.is_empty() {
            return "No mistakes recorded".to_string();
        }

        let mut summary = format!("Recent mistakes ({}):\n", self.mistakes.len());

        // Group by operation
        let mut by_operation: HashMap<&str, usize> = HashMap::new();
        for mistake in self.mistakes.to_vec() {
            *by_operation.entry(&mistake.operation).or_insert(0) += 1;
        }

        for (operation, count) in by_operation.iter() {
            summary.push_str(&format!("- {}: {} time(s)\n", operation, count));
        }

        summary
    }

    pub fn clear(&mut self) {
        self.mistakes.clear();
        self.operation_counts.clear();
    }

    fn cleanup_old_mistakes(&mut self) {
        let now = Instant::now();
        let recent: Vec<Mistake> = self
            .mistakes
            .to_vec()
            .into_iter()
            .filter(|m| now.duration_since(m.timestamp) < self.time_window)
            .cloned()
            .collect();

        self.mistakes.clear();
        for mistake in recent {
            self.mistakes.push(mistake);
        }

        // Recalculate operation counts
        self.operation_counts.clear();
        for mistake in self.mistakes.to_vec() {
            *self
                .operation_counts
                .entry(mistake.operation.clone())
                .or_insert(0) += 1;
        }
    }

    pub fn last_mistake(&self) -> Option<&Mistake> {
        self.mistakes.last()
    }

    pub fn is_operation_failing(&self, operation: &str) -> bool {
        *self.operation_counts.get(operation).unwrap_or(&0) >= 2
    }
}

impl Default for MistakeTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mistake_tracker_new() {
        let mut tracker = MistakeTracker::new();
        assert_eq!(tracker.recent_mistake_count(), 0);
        assert!(!tracker.should_warn());
        assert!(!tracker.needs_recovery());
    }

    #[test]
    fn test_mistake_tracking() {
        let mut tracker = MistakeTracker::new();

        tracker.record_mistake(
            MistakeType::ToolFailed,
            "bash".to_string(),
            "command not found".to_string(),
            "trying to run foo".to_string(),
        );

        assert_eq!(tracker.recent_mistake_count(), 1);
        assert!(!tracker.should_warn());
    }

    #[test]
    fn test_warning_threshold() {
        let mut tracker = MistakeTracker::new();

        for i in 0..3 {
            tracker.record_mistake(
                MistakeType::ToolFailed,
                "bash".to_string(),
                format!("error {}", i),
                "context".to_string(),
            );
        }

        assert!(tracker.should_warn());
        assert!(!tracker.needs_recovery());
    }

    #[test]
    fn test_recovery_threshold() {
        let mut tracker = MistakeTracker::new();

        for i in 0..5 {
            tracker.record_mistake(
                MistakeType::ToolFailed,
                "bash".to_string(),
                format!("error {}", i),
                "context".to_string(),
            );
        }

        assert!(tracker.needs_recovery());
        assert!(tracker.suggest_recovery().is_some());
    }

    #[test]
    fn test_clear_mistakes() {
        let mut tracker = MistakeTracker::new();

        tracker.record_mistake(
            MistakeType::ToolFailed,
            "bash".to_string(),
            "error".to_string(),
            "context".to_string(),
        );

        tracker.clear();
        assert_eq!(tracker.recent_mistake_count(), 0);
    }
}
