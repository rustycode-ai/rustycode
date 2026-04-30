//! Idle detection, doom loop detection, and task enforcement for the agent loop.
//!
//! Detects when the agent is making no progress (text-only responses, repeated
//! tool calls, identical tool calls) and injects escalating nudge messages.

/// Severity of idle state — determines nudge intensity.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleLevel {
    /// Agent is making progress (using tools with varied patterns)
    Active,
    /// 2 consecutive idle iterations — gentle reminder
    GentleNudge,
    /// 3 consecutive idle iterations — task reminder with original prompt
    TaskReminder,
    /// 4+ consecutive or 3+ repeated tool patterns — force assessment
    StopAndAssess,
}

/// Number of consecutive identical tool calls that triggers doom loop detection.
const DOOM_LOOP_THRESHOLD: usize = 3;

/// A single tool call with its name and serialized input for comparison.
#[derive(Debug, Clone)]
pub struct ToolCallSignature {
    pub name: String,
    pub input_json: String,
}

impl ToolCallSignature {
    pub fn new(name: &str, input_json: &str) -> Self {
        Self {
            name: name.to_string(),
            input_json: input_json.to_string(),
        }
    }
}

/// Result of doom loop detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoomLoopResult {
    /// Whether a doom loop was detected
    pub detected: bool,
    /// The tool name involved in the loop (if detected)
    pub tool_name: Option<String>,
    /// Number of consecutive identical calls
    pub consecutive_count: usize,
}

/// Tracks whether the agent is making progress or is idle.
pub struct IdleDetector {
    /// Consecutive iterations with no tool use (text-only responses)
    consecutive_idle: usize,
    /// Tool names from the last iteration (for loop detection)
    last_tool_names: Vec<String>,
    /// Consecutive iterations with the same tool call pattern
    consecutive_repeated: usize,
    /// The original task prompt (for reminder nudges)
    original_task: String,
    /// Recent tool call signatures for doom loop detection
    recent_signatures: Vec<ToolCallSignature>,
}

impl IdleDetector {
    /// Create a new detector with the original task description.
    pub fn new(original_task: &str) -> Self {
        Self {
            consecutive_idle: 0,
            last_tool_names: Vec::new(),
            consecutive_repeated: 0,
            original_task: original_task.to_string(),
            recent_signatures: Vec::new(),
        }
    }

    /// Record what happened in an iteration.
    pub fn record_iteration(&mut self, had_tool_use: bool, tool_names: &[String]) {
        if had_tool_use {
            if self.is_repeated_pattern(tool_names) {
                self.consecutive_repeated = self.consecutive_repeated.saturating_add(1);
                self.consecutive_idle = 0;
            } else {
                self.consecutive_idle = 0;
                self.consecutive_repeated = 0;
            }
            self.last_tool_names = tool_names.to_vec();
        } else {
            self.consecutive_idle = self.consecutive_idle.saturating_add(1);
            self.consecutive_repeated = 0;
        }
    }

    /// Record tool call signatures for doom loop detection.
    ///
    /// Call this with the actual tool calls (name + input JSON) from each iteration.
    /// Returns a `DoomLoopResult` indicating if a doom loop was detected.
    ///
    /// A doom loop is when the agent calls the exact same tool with the exact same
    /// input DOOM_LOOP_THRESHOLD times in a row.
    pub fn record_tool_signatures(&mut self, signatures: &[ToolCallSignature]) -> DoomLoopResult {
        // Update doom loop tracking
        self.recent_signatures.extend_from_slice(signatures);

        // Only keep enough history for detection
        let max_keep = DOOM_LOOP_THRESHOLD * 2;
        if self.recent_signatures.len() > max_keep {
            let drain_count = self.recent_signatures.len() - max_keep;
            self.recent_signatures.drain(0..drain_count);
        }

        // Check for doom loop: last N signatures are all identical
        if self.recent_signatures.len() >= DOOM_LOOP_THRESHOLD {
            let last_n: Vec<&ToolCallSignature> = self
                .recent_signatures
                .iter()
                .rev()
                .take(DOOM_LOOP_THRESHOLD)
                .collect();

            let first = &last_n[0];
            let all_same = last_n
                .iter()
                .all(|s| s.name == first.name && s.input_json == first.input_json);

            if all_same {
                return DoomLoopResult {
                    detected: true,
                    tool_name: Some(first.name.clone()),
                    consecutive_count: DOOM_LOOP_THRESHOLD,
                };
            }
        }

        DoomLoopResult {
            detected: false,
            tool_name: None,
            consecutive_count: 0,
        }
    }

    /// Get the current idle level.
    pub fn idle_level(&self) -> IdleLevel {
        if self.consecutive_idle >= 4 || self.consecutive_repeated >= 3 {
            IdleLevel::StopAndAssess
        } else if self.consecutive_idle >= 3 {
            IdleLevel::TaskReminder
        } else if self.consecutive_idle >= 2 {
            IdleLevel::GentleNudge
        } else {
            IdleLevel::Active
        }
    }

    /// Get a nudge message to inject into the conversation, if the agent is idle.
    /// Returns None if the agent is active.
    pub fn get_nudge_message(&self) -> Option<String> {
        match self.idle_level() {
            IdleLevel::Active => None,
            IdleLevel::GentleNudge => Some(
                "You have not used any tools in the last 2 iterations. \
                 Remember: you MUST use tools (read_file, write_file, bash, grep) \
                 to make progress. Stop explaining and start doing."
                    .to_string(),
            ),
            IdleLevel::TaskReminder => Some(format!(
                "REMINDER — Original task: \"{}\"\n\n\
                 You appear to be stuck without making progress. \
                 Use your available tools to take a concrete step toward completing this task.",
                self.original_task
            )),
            IdleLevel::StopAndAssess => Some(
                "STOP AND ASSESS\n\n\
                 You have been idle or repeating the same action for multiple iterations.\n\
                 Before continuing:\n\
                 1. State what is blocking you\n\
                 2. Identify what tool you need to use next\n\
                 3. Take exactly one concrete action\n\n\
                 If you cannot complete the task, explain clearly what is preventing you."
                    .to_string(),
            ),
        }
    }

    /// Get a doom loop nudge message.
    pub fn doom_loop_nudge(tool_name: &str) -> String {
        format!(
            "DOOM LOOP DETECTED: You have called the `{tool_name}` tool with identical \
             arguments {threshold} times in a row. This is not making progress.\n\n\
             You must either:\n\
             1. Change your approach — use a different tool or different arguments\n\
             2. Explain what is blocking you and what you need\n\
             3. If the task is complete, stop and summarize what was done",
            tool_name = tool_name,
            threshold = DOOM_LOOP_THRESHOLD,
        )
    }

    /// Reset the detector (e.g., after progress is made).
    pub fn reset(&mut self) {
        self.consecutive_idle = 0;
        self.consecutive_repeated = 0;
        self.last_tool_names.clear();
        self.recent_signatures.clear();
    }

    /// Get the consecutive idle count for external consumers.
    pub fn consecutive_idle_count(&self) -> usize {
        self.consecutive_idle
    }

    /// Get the consecutive repeated pattern count.
    pub fn consecutive_repeated_count(&self) -> usize {
        self.consecutive_repeated
    }

    /// Check if the current tool pattern matches the last iteration.
    fn is_repeated_pattern(&self, tool_names: &[String]) -> bool {
        if self.last_tool_names.is_empty() || tool_names.is_empty() {
            return false;
        }
        // Same set of tool names (order-independent)
        let mut current = tool_names.to_vec();
        let mut last = self.last_tool_names.clone();
        current.sort();
        last.sort();
        current == last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_detector_is_active() {
        let detector = IdleDetector::new("implement auth");
        assert_eq!(detector.idle_level(), IdleLevel::Active);
        assert!(detector.get_nudge_message().is_none());
    }

    #[test]
    fn test_idle_after_two_text_only_iterations() {
        let mut detector = IdleDetector::new("implement auth");
        detector.record_iteration(false, &[]);
        detector.record_iteration(false, &[]);
        assert_eq!(detector.idle_level(), IdleLevel::GentleNudge);
        assert!(detector.get_nudge_message().is_some());
    }

    #[test]
    fn test_task_reminder_at_three() {
        let mut detector = IdleDetector::new("implement auth");
        for _ in 0..3 {
            detector.record_iteration(false, &[]);
        }
        assert_eq!(detector.idle_level(), IdleLevel::TaskReminder);
        let msg = detector.get_nudge_message().unwrap();
        assert!(msg.contains("implement auth"));
    }

    #[test]
    fn test_stop_and_assess_at_four() {
        let mut detector = IdleDetector::new("implement auth");
        for _ in 0..4 {
            detector.record_iteration(false, &[]);
        }
        assert_eq!(detector.idle_level(), IdleLevel::StopAndAssess);
        let msg = detector.get_nudge_message().unwrap();
        assert!(msg.contains("STOP AND ASSESS"));
    }

    #[test]
    fn test_reset_clears_idle() {
        let mut detector = IdleDetector::new("implement auth");
        for _ in 0..3 {
            detector.record_iteration(false, &[]);
        }
        assert_eq!(detector.idle_level(), IdleLevel::TaskReminder);
        detector.reset();
        assert_eq!(detector.idle_level(), IdleLevel::Active);
    }

    #[test]
    fn test_tool_use_resets_idle() {
        let mut detector = IdleDetector::new("implement auth");
        detector.record_iteration(false, &[]);
        detector.record_iteration(false, &[]);
        assert_eq!(detector.idle_level(), IdleLevel::GentleNudge);
        // Tool use resets idle
        detector.record_iteration(true, &["read_file".to_string()]);
        assert_eq!(detector.idle_level(), IdleLevel::Active);
    }

    #[test]
    fn test_repeated_pattern_detection() {
        let mut detector = IdleDetector::new("implement auth");
        let tools = vec!["read_file".to_string()];
        detector.record_iteration(true, &tools); // sets baseline, repeated=0
        detector.record_iteration(true, &tools); // repeated=1
        detector.record_iteration(true, &tools); // repeated=2
        detector.record_iteration(true, &tools); // repeated=3 -> StopAndAssess
        assert_eq!(detector.idle_level(), IdleLevel::StopAndAssess);
    }

    #[test]
    fn test_different_tools_dont_trigger_repeated() {
        let mut detector = IdleDetector::new("implement auth");
        detector.record_iteration(true, &["read_file".to_string()]);
        detector.record_iteration(true, &["write_file".to_string()]);
        detector.record_iteration(true, &["bash".to_string()]);
        assert_eq!(detector.idle_level(), IdleLevel::Active);
    }

    #[test]
    fn test_nudge_message_content() {
        let mut detector = IdleDetector::new("fix the bug");
        detector.record_iteration(false, &[]);
        detector.record_iteration(false, &[]);
        let msg = detector.get_nudge_message().unwrap();
        assert!(msg.contains("tools"));
    }

    // Doom loop detection tests

    #[test]
    fn test_doom_loop_not_triggered_with_different_calls() {
        let mut detector = IdleDetector::new("fix bug");
        let r1 = detector
            .record_tool_signatures(&[ToolCallSignature::new("read_file", r#"{"path": "/a.rs"}"#)]);
        assert!(!r1.detected);
        let r2 = detector
            .record_tool_signatures(&[ToolCallSignature::new("read_file", r#"{"path": "/b.rs"}"#)]);
        assert!(!r2.detected);
    }

    #[test]
    fn test_doom_loop_triggered_with_identical_calls() {
        let mut detector = IdleDetector::new("fix bug");
        let sig = ToolCallSignature::new("read_file", r#"{"path": "/a.rs"}"#);
        detector.record_tool_signatures(std::slice::from_ref(&sig));
        detector.record_tool_signatures(std::slice::from_ref(&sig));
        let result = detector.record_tool_signatures(std::slice::from_ref(&sig));
        assert!(result.detected);
        assert_eq!(result.tool_name, Some("read_file".to_string()));
        assert_eq!(result.consecutive_count, 3);
    }

    #[test]
    fn test_doom_loop_resets_on_different_call() {
        let mut detector = IdleDetector::new("fix bug");
        let sig = ToolCallSignature::new("read_file", r#"{"path": "/a.rs"}"#);
        detector.record_tool_signatures(std::slice::from_ref(&sig));
        detector.record_tool_signatures(std::slice::from_ref(&sig));
        // Different call breaks the chain
        detector.record_tool_signatures(&[ToolCallSignature::new(
            "write_file",
            r#"{"path": "/a.rs"}"#,
        )]);
        // Same as first again — only 1 consecutive
        let result = detector.record_tool_signatures(&[sig]);
        assert!(!result.detected);
    }

    #[test]
    fn test_doom_loop_with_multiple_tools_per_iteration() {
        let mut detector = IdleDetector::new("fix bug");
        let s1 = ToolCallSignature::new("read_file", r#"{"path": "/a.rs"}"#);
        let s2 = ToolCallSignature::new("grep", r#"{"pattern": "todo"}"#);
        detector.record_tool_signatures(&[s1.clone(), s2.clone()]);
        detector.record_tool_signatures(&[s1.clone(), s2.clone()]);
        let result = detector.record_tool_signatures(&[s1, s2]);
        // All 6 signatures are read_file, grep, read_file, grep, read_file, grep
        // The last 3 are read_file, grep, grep — not all identical
        // Actually let me check: reversed last 3 are grep, read_file, grep
        // They're not all the same, so no doom loop
        // This is correct: multiple tools per iteration means we check the
        // individual call-level signatures, not iteration-level
        assert!(!result.detected);
    }

    #[test]
    fn test_doom_loop_nudge_message() {
        let msg = IdleDetector::doom_loop_nudge("bash");
        assert!(msg.contains("DOOM LOOP DETECTED"));
        assert!(msg.contains("bash"));
        assert!(msg.contains("3 times"));
    }

    #[test]
    fn test_doom_loop_with_different_inputs_same_name() {
        let mut detector = IdleDetector::new("fix bug");
        // Same tool name but different inputs should NOT trigger
        detector
            .record_tool_signatures(&[ToolCallSignature::new("read_file", r#"{"path": "/a.rs"}"#)]);
        detector
            .record_tool_signatures(&[ToolCallSignature::new("read_file", r#"{"path": "/b.rs"}"#)]);
        let result = detector
            .record_tool_signatures(&[ToolCallSignature::new("read_file", r#"{"path": "/c.rs"}"#)]);
        assert!(!result.detected);
    }

    #[test]
    fn test_signature_history_is_bounded() {
        let mut detector = IdleDetector::new("fix bug");
        // Add many signatures — should not grow unbounded
        for i in 0..100 {
            detector.record_tool_signatures(&[ToolCallSignature::new(
                "read_file",
                &format!(r#"{{"path": "/{}.rs"}}"#, i),
            )]);
        }
        // History should be capped at DOOM_LOOP_THRESHOLD * 2 = 6
        assert!(detector.recent_signatures.len() <= 6);
    }
}
