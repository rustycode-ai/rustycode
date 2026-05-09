//! Doom Loop Detector
//!
//! Detects when an LLM agent is stuck repeating the same tool calls
//! with similar arguments, indicating a loop that won't resolve.
//! Inspired by `forge_app`'s `DoomLoopDetector`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of recent tool calls to track
const MAX_HISTORY: usize = 50;

/// Number of repeated calls before warning
const WARN_THRESHOLD: usize = 3;

/// Number of repeated calls before suggesting abort
const ABORT_THRESHOLD: usize = 5;

/// Time window for considering calls as "recent"
const WINDOW: Duration = Duration::from_mins(2);

/// A record of a single tool invocation
#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub tool_name: String,
    pub args_hash: u64,
    pub timestamp: Instant,
}

/// Result of doom loop detection
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DoomLoopStatus {
    /// No loop detected
    Clean,
    /// Same tool+args repeated N times - warning
    Warning {
        tool_name: String,
        repeat_count: usize,
        suggestion: String,
    },
    /// Same tool+args repeated many times - should abort
    Abort {
        tool_name: String,
        repeat_count: usize,
        suggestion: String,
    },
}

/// Detects repetitive tool call patterns that indicate an agent is stuck.
#[derive(Debug)]
pub struct DoomLoopDetector {
    history: VecDeque<ToolCallRecord>,
    max_history: usize,
    warn_threshold: usize,
    abort_threshold: usize,
    window: Duration,
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomLoopDetector {
    pub fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_HISTORY),
            max_history: MAX_HISTORY,
            warn_threshold: WARN_THRESHOLD,
            abort_threshold: ABORT_THRESHOLD,
            window: WINDOW,
        }
    }

    /// Record a tool call and check for doom loops.
    /// Returns the current loop status after recording.
    pub fn record(&mut self, tool_name: &str, args: &str) -> DoomLoopStatus {
        self.prune_old();

        let args_hash = self.hash_args(args);
        let record = ToolCallRecord {
            tool_name: tool_name.to_string(),
            args_hash,
            timestamp: Instant::now(),
        };

        // Count how many times this exact tool+hash combo appears in window
        let repeat_count = self
            .history
            .iter()
            .filter(|r| r.tool_name == tool_name && r.args_hash == args_hash)
            .count()
            + 1; // +1 for current call

        self.history.push_back(record);

        // Trim to max size
        while self.history.len() > self.max_history {
            self.history.pop_front();
        }

        if repeat_count >= self.abort_threshold {
            DoomLoopStatus::Abort {
                tool_name: tool_name.to_string(),
                repeat_count,
                suggestion: self.make_suggestion(tool_name, repeat_count, true),
            }
        } else if repeat_count >= self.warn_threshold {
            DoomLoopStatus::Warning {
                tool_name: tool_name.to_string(),
                repeat_count,
                suggestion: self.make_suggestion(tool_name, repeat_count, false),
            }
        } else {
            DoomLoopStatus::Clean
        }
    }

    /// Quick check if a tool call would trigger a warning, without recording it.
    pub fn would_warn(&self, tool_name: &str, args: &str) -> bool {
        let args_hash = self.hash_args(args);
        let count = self
            .history
            .iter()
            .filter(|r| r.tool_name == tool_name && r.args_hash == args_hash)
            .count()
            + 1;
        count >= self.warn_threshold
    }

    /// Get recent tool call names (for diagnostics)
    pub fn recent_tools(&self, limit: usize) -> Vec<&str> {
        self.history
            .iter()
            .rev()
            .take(limit)
            .map(|r| r.tool_name.as_str())
            .collect()
    }

    /// Clear all history
    pub fn reset(&mut self) {
        self.history.clear();
    }

    /// Remove entries older than the time window
    fn prune_old(&mut self) {
        let Some(cutoff) = Instant::now().checked_sub(self.window) else {
            self.history.clear();
            return;
        };
        while self.history.front().is_some_and(|r| r.timestamp < cutoff) {
            self.history.pop_front();
        }
    }

    /// Simple hash of args for similarity comparison
    fn hash_args(&self, args: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Normalize: trim and collapse whitespace for fuzzy matching
        let normalized: String = args
            .chars()
            .map(|c| if c.is_whitespace() { ' ' } else { c })
            .collect();
        let normalized = normalized.trim();

        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        hasher.finish()
    }

    fn make_suggestion(&self, tool_name: &str, count: usize, is_abort: bool) -> String {
        if is_abort {
            format!(
                "Agent called '{tool_name}' {count} times with the same arguments. \
                 Consider: (1) trying a different approach, \
                 (2) using a different tool, or \
                 (3) asking the user for guidance."
            )
        } else {
            format!(
                "Agent has called '{tool_name}' {count} times with similar arguments. \
                 This may indicate a loop. Consider trying an alternative approach."
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_on_first_call() {
        let mut detector = DoomLoopDetector::new();
        let status = detector.record("Read", r#"{"path": "/foo"}"#);
        assert_eq!(status, DoomLoopStatus::Clean);
    }

    #[test]
    fn test_warning_after_repeats() {
        let mut detector = DoomLoopDetector::new();
        let args = r#"{"path": "/foo"}"#;

        for _ in 0..2 {
            detector.record("Read", args);
        }
        let status = detector.record("Read", args);

        assert!(matches!(status, DoomLoopStatus::Warning { .. }));
    }

    #[test]
    fn test_abort_after_many_repeats() {
        let mut detector = DoomLoopDetector::new();
        let args = r#"{"path": "/foo"}"#;

        for _ in 0..5 {
            detector.record("Read", args);
        }

        let status = detector.record("Read", args);
        assert!(matches!(status, DoomLoopStatus::Abort { .. }));
    }

    #[test]
    fn test_different_args_no_warning() {
        let mut detector = DoomLoopDetector::new();

        for i in 0..5 {
            detector.record("Read", &format!(r#"{{"path": "/foo/{i}"}}"#));
        }
        // Different args each time should be clean
        let status = detector.record("Read", r#"{"path": "/foo/5"}"#);
        assert_eq!(status, DoomLoopStatus::Clean);
    }

    #[test]
    fn test_reset_clears_history() {
        let mut detector = DoomLoopDetector::new();
        detector.record("Read", r#"{"path": "/foo"}"#);
        detector.reset();
        assert!(detector.recent_tools(10).is_empty());
    }
}
