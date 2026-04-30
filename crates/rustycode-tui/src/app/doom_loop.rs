//! Doom loop detection for agent tool execution.
//!
//! Detects when the AI agent is stuck repeating the same (tool, argument)
//! combination without making progress. Unlike [`crate::MistakeTracker`]
//! which counts consecutive failures, this module detects *repetition
//! patterns* — the same tool called on the same file/argument repeatedly.
//!
//! Inspired by OpenCode's `DOOM_LOOP_THRESHOLD` pattern.

/// Number of identical consecutive failures before declaring a doom loop.
pub const DOOM_LOOP_THRESHOLD: usize = 3;

/// Maximum number of tool-call records kept in the sliding window.
const WINDOW_SIZE: usize = 10;

/// A lightweight record of a single tool invocation.
#[derive(Debug, Clone)]
struct ToolCallRecord {
    /// Tool name (e.g. "edit_file", "bash", "write_file").
    tool_name: String,
    /// Primary argument — file path for file tools, command for bash, etc.
    /// Used to distinguish "edit_file(a.rs)" from "edit_file(b.rs)".
    key_arg: Option<String>,
    /// Whether the tool execution succeeded.
    success: bool,
}

impl ToolCallRecord {
    fn fingerprint(&self) -> String {
        match &self.key_arg {
            Some(arg) => format!("{}:{}", self.tool_name, arg),
            None => self.tool_name.clone(),
        }
    }
}

/// Detects repetitive tool-call patterns that indicate the agent is stuck.
///
/// # Detection logic
///
/// A "doom loop" is triggered when the same `(tool_name, key_arg)` pair
/// fails consecutively `DOOM_LOOP_THRESHOLD` times within the sliding
/// window. A single success or a different tool/arg breaks the streak.
///
/// A "soft doom loop" is triggered when the same `(tool_name, key_arg)`
/// pair is called (success or failure) `DOOM_LOOP_THRESHOLD * 2` times,
/// indicating the agent may be making progress but spinning on the same
/// target without converging.
#[derive(Debug, Clone, Default)]
pub struct DoomLoopDetector {
    records: Vec<ToolCallRecord>,
}

impl DoomLoopDetector {
    /// Create a new detector with an empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a tool call result.
    pub fn record(&mut self, tool_name: &str, key_arg: Option<&str>, success: bool) {
        self.records.push(ToolCallRecord {
            tool_name: tool_name.to_string(),
            key_arg: key_arg.map(|s| {
                // Truncate long args to keep memory bounded
                if s.len() > 200 {
                    let end = s.floor_char_boundary(197);
                    format!("{}...", &s[..end])
                } else {
                    s.to_string()
                }
            }),
            success,
        });

        // Maintain sliding window
        if self.records.len() > WINDOW_SIZE {
            self.records.remove(0);
        }
    }

    /// Returns `true` if a doom loop is currently detected.
    pub fn is_doom_loop(&self) -> bool {
        self.doom_loop_reason().is_some()
    }

    /// Returns a human-readable explanation if a doom loop is detected, or `None`.
    pub fn doom_loop_reason(&self) -> Option<String> {
        if self.records.len() < DOOM_LOOP_THRESHOLD {
            return None;
        }

        // Check the most recent fingerprint for consecutive failures
        let latest = &self.records[self.records.len() - 1];
        if latest.success {
            // A success breaks any failure-based doom loop
            // But check for soft doom loop (too many identical calls)
            return self.check_soft_doom_loop();
        }

        let fingerprint = latest.fingerprint();
        let consecutive_failures = self
            .records
            .iter()
            .rev()
            .take_while(|r| !r.success && r.fingerprint() == fingerprint)
            .count();

        if consecutive_failures >= DOOM_LOOP_THRESHOLD {
            return Some(format!(
                "Tool `{}`{} failed {} consecutive times — likely stuck",
                latest.tool_name,
                latest
                    .key_arg
                    .as_ref()
                    .map(|a| format!(" on `{}`", a))
                    .unwrap_or_default(),
                consecutive_failures
            ));
        }

        None
    }

    /// Check for "soft" doom loops — same tool+arg repeated many times
    /// regardless of success/failure.
    fn check_soft_doom_loop(&self) -> Option<String> {
        if self.records.is_empty() {
            return None;
        }

        let latest = &self.records[self.records.len() - 1];
        let fingerprint = latest.fingerprint();
        let total_same = self
            .records
            .iter()
            .filter(|r| r.fingerprint() == fingerprint)
            .count();

        let soft_threshold = DOOM_LOOP_THRESHOLD * 2;
        if total_same >= soft_threshold {
            return Some(format!(
                "Tool `{}`{} called {} times without convergence — may be stuck",
                latest.tool_name,
                latest
                    .key_arg
                    .as_ref()
                    .map(|a| format!(" on `{}`", a))
                    .unwrap_or_default(),
                total_same
            ));
        }

        None
    }

    /// Clear history. Call when a successful action breaks a potential loop.
    pub fn reset(&mut self) {
        self.records.clear();
    }

    /// Number of records currently in the sliding window.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_doom_loop_on_varied_calls() {
        let mut d = DoomLoopDetector::new();
        d.record("edit_file", Some("a.rs"), true);
        d.record("bash", Some("cargo build"), false);
        d.record("edit_file", Some("b.rs"), true);
        assert!(!d.is_doom_loop());
    }

    #[test]
    fn test_doom_loop_on_repeated_failures() {
        let mut d = DoomLoopDetector::new();
        d.record("edit_file", Some("src/main.rs"), false);
        d.record("edit_file", Some("src/main.rs"), false);
        assert!(!d.is_doom_loop()); // Only 2 — not yet
        d.record("edit_file", Some("src/main.rs"), false);
        assert!(d.is_doom_loop());
        let reason = d.doom_loop_reason().unwrap();
        assert!(reason.contains("edit_file"));
        assert!(reason.contains("src/main.rs"));
        assert!(reason.contains("3 consecutive"));
    }

    #[test]
    fn test_no_doom_loop_when_interleaved_with_success() {
        let mut d = DoomLoopDetector::new();
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("a.rs"), true); // Success resets
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("a.rs"), false);
        assert!(!d.is_doom_loop()); // Only 2 failures in a row
    }

    #[test]
    fn test_different_args_dont_trigger() {
        let mut d = DoomLoopDetector::new();
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("b.rs"), false);
        d.record("edit_file", Some("c.rs"), false);
        assert!(!d.is_doom_loop()); // All different args
    }

    #[test]
    fn test_reset_clears_detection() {
        let mut d = DoomLoopDetector::new();
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("a.rs"), false);
        d.record("edit_file", Some("a.rs"), false);
        assert!(d.is_doom_loop());
        d.reset();
        assert!(!d.is_doom_loop());
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn test_sliding_window_old_records_expire() {
        let mut d = DoomLoopDetector::new();
        // Fill with 8 varied records
        for i in 0..8 {
            d.record("bash", Some(&format!("cmd_{}", i)), true);
        }
        assert_eq!(d.len(), 8);
        // Add 3 more to exceed WINDOW_SIZE (10)
        d.record("edit_file", Some("x.rs"), false);
        d.record("edit_file", Some("x.rs"), false);
        d.record("edit_file", Some("x.rs"), false);
        assert!(d.is_doom_loop());
        assert_eq!(d.len(), 10); // Window capped
    }

    #[test]
    fn test_soft_doom_loop_many_same_calls() {
        let mut d = DoomLoopDetector::new();
        // 6 successful calls to the same tool+arg (DOOM_LOOP_THRESHOLD * 2)
        for _ in 0..6 {
            d.record("edit_file", Some("a.rs"), true);
        }
        // Latest is a success, so hard doom loop won't trigger
        // But soft doom loop should (6 >= 3*2)
        assert!(d.is_doom_loop());
        let reason = d.doom_loop_reason().unwrap();
        assert!(reason.contains("without convergence"));
    }

    #[test]
    fn test_empty_detector() {
        let d = DoomLoopDetector::new();
        assert!(!d.is_doom_loop());
        assert!(d.doom_loop_reason().is_none());
    }

    #[test]
    fn test_long_arg_truncated() {
        let mut d = DoomLoopDetector::new();
        let long_arg = "a".repeat(300);
        d.record("edit_file", Some(&long_arg), false);
        assert_eq!(d.records[0].key_arg.as_ref().unwrap().len(), 200);
    }

    #[test]
    fn test_bash_doom_loop() {
        let mut d = DoomLoopDetector::new();
        d.record("bash", Some("cargo build"), false);
        d.record("bash", Some("cargo build"), false);
        d.record("bash", Some("cargo build"), false);
        assert!(d.is_doom_loop());
        let reason = d.doom_loop_reason().unwrap();
        assert!(reason.contains("bash"));
        assert!(reason.contains("cargo build"));
    }
}
