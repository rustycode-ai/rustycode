//! Early-stop policy — halts the agent loop when heuristics detect stagnation.
//!
//! Conditions (any one triggers stop):
//! - N+ turns since last edit (after at least one edit was made, default 5)
//! - Same file edited N+ times (thrashing, default 6)
//! - N+ total edits across all files (scope creep, default 15)
//! - N consecutive error-only turns (default 3)
//!
//! All thresholds are configurable via `EarlyStopPolicy::with_thresholds()`.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use super::{AgentPlugin, TurnContext};

const EDIT_TOOLS: &[&str] = &["edit", "write", "apply_patch", "Edit", "Write"];

/// Default thresholds for early-stop policy.
const DEFAULT_MAX_TURNS_SINCE_EDIT: usize = 5;
const DEFAULT_MAX_SAME_FILE_EDITS: usize = 6;
const DEFAULT_MAX_TOTAL_EDITS: usize = 15;
const DEFAULT_MAX_CONSECUTIVE_ERRORS: usize = 3;

fn is_error_output(output: &str) -> bool {
    output.starts_with("Error ")
        || output.starts_with("ERROR: ")
        || output.starts_with("error: ")
        || (output.contains("[exit code:") && !output.contains("[exit code: 0]"))
        || output.contains("command not found")
        || output.contains("No such file or directory")
        || output.contains("Permission denied")
}

fn extract_path(input: &Value) -> Option<String> {
    input
        .get("path")
        .or_else(|| input.get("file_path"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Policy that detects stagnation patterns and requests early loop termination.
///
/// Thresholds are configurable via [`EarlyStopPolicy::with_thresholds`].
pub struct EarlyStopPolicy {
    turns_since_edit: usize,
    file_edit_counts: HashMap<String, usize>,
    total_edits: usize,
    consecutive_error_turns: usize,
    made_edits: bool,
    had_non_error_this_turn: bool,
    tool_count_this_turn: usize,
    error_count_this_turn: usize,
    edits_this_turn: usize,
    /// Maximum total edits before stopping (default 15).
    max_total_edits: usize,
    /// Maximum turns since last edit before stopping (default 5).
    max_turns_since_edit: usize,
    /// Maximum edits to the same file before stopping (default 6).
    max_same_file_edits: usize,
    /// Maximum consecutive error-only turns before stopping (default 3).
    max_consecutive_errors: usize,
}

impl EarlyStopPolicy {
    /// Create a new policy with default thresholds.
    pub fn new() -> Self {
        Self {
            turns_since_edit: 0,
            file_edit_counts: HashMap::new(),
            total_edits: 0,
            consecutive_error_turns: 0,
            made_edits: false,
            had_non_error_this_turn: false,
            tool_count_this_turn: 0,
            error_count_this_turn: 0,
            edits_this_turn: 0,
            max_total_edits: DEFAULT_MAX_TOTAL_EDITS,
            max_turns_since_edit: DEFAULT_MAX_TURNS_SINCE_EDIT,
            max_same_file_edits: DEFAULT_MAX_SAME_FILE_EDITS,
            max_consecutive_errors: DEFAULT_MAX_CONSECUTIVE_ERRORS,
        }
    }

    /// Create a policy with custom thresholds.
    pub fn with_thresholds(
        max_total_edits: usize,
        max_turns_since_edit: usize,
        max_same_file_edits: usize,
        max_consecutive_errors: usize,
    ) -> Self {
        Self {
            max_total_edits,
            max_turns_since_edit,
            max_same_file_edits,
            max_consecutive_errors,
            ..Self::new()
        }
    }

    /// Returns the maximum total edits threshold.
    pub fn max_total_edits(&self) -> usize {
        self.max_total_edits
    }

    /// Returns the maximum turns since last edit threshold.
    pub fn max_turns_since_edit(&self) -> usize {
        self.max_turns_since_edit
    }

    /// Returns the maximum same-file edits threshold.
    pub fn max_same_file_edits(&self) -> usize {
        self.max_same_file_edits
    }

    /// Returns the maximum consecutive error turns threshold.
    pub fn max_consecutive_errors(&self) -> usize {
        self.max_consecutive_errors
    }
}

impl Default for EarlyStopPolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentPlugin for EarlyStopPolicy {
    async fn on_tool_result(
        &mut self,
        tool_name: &str,
        _tool_id: &str,
        input: &Value,
        output: &mut String,
    ) {
        self.tool_count_this_turn += 1;

        if EDIT_TOOLS.contains(&tool_name) {
            self.made_edits = true;
            self.turns_since_edit = 0;
            self.total_edits += 1;
            self.edits_this_turn += 1;
            if let Some(path) = extract_path(input) {
                *self.file_edit_counts.entry(path).or_insert(0) += 1;
            }
        }

        if is_error_output(output) {
            self.error_count_this_turn += 1;
        } else {
            self.had_non_error_this_turn = true;
        }
    }

    async fn should_stop(&mut self, _ctx: &TurnContext) -> bool {
        if self.tool_count_this_turn > 0
            && !self.had_non_error_this_turn
            && self.error_count_this_turn == self.tool_count_this_turn
        {
            self.consecutive_error_turns += 1;
        } else {
            self.consecutive_error_turns = 0;
        }

        if self.made_edits && self.edits_this_turn == 0 {
            self.turns_since_edit += 1;
        }

        self.tool_count_this_turn = 0;
        self.error_count_this_turn = 0;
        self.had_non_error_this_turn = false;
        self.edits_this_turn = 0;

        if self.made_edits && self.turns_since_edit >= self.max_turns_since_edit {
            return true;
        }
        if let Some(count) = self.file_edit_counts.values().max() {
            if *count >= self.max_same_file_edits {
                return true;
            }
        }
        if self.total_edits >= self.max_total_edits {
            return true;
        }
        if self.consecutive_error_turns >= self.max_consecutive_errors {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ctx(turn: usize) -> TurnContext {
        TurnContext {
            turn,
            total_input_tokens: 0,
            total_output_tokens: 0,
            cwd: PathBuf::from("/tmp"),
        }
    }

    #[tokio::test]
    async fn stops_after_max_turns_since_last_edit() {
        let mut policy = EarlyStopPolicy::new();
        let edit_input = serde_json::json!({"path": "foo.rs", "content": "hello"});

        let mut output = "ok".to_string();
        policy
            .on_tool_result("Edit", "1", &edit_input, &mut output)
            .await;
        assert!(!policy.should_stop(&make_ctx(0)).await);

        // Read turns between edits should not trigger stop until threshold
        for turn in 1..5 {
            let mut output = "file contents".to_string();
            policy
                .on_tool_result("Read", &format!("{turn}"), &Value::Null, &mut output)
                .await;
            assert!(
                !policy.should_stop(&make_ctx(turn)).await,
                "should not stop at turn {turn}"
            );
        }

        // 6th turn since edit should trigger stop (default threshold is 5)
        let mut output = "more contents".to_string();
        policy
            .on_tool_result("Read", "6", &Value::Null, &mut output)
            .await;
        assert!(policy.should_stop(&make_ctx(6)).await);
    }

    #[tokio::test]
    async fn stops_on_same_file_thrashing() {
        let mut policy = EarlyStopPolicy::new();
        let edit_input = serde_json::json!({"path": "main.rs", "content": "x"});

        // Default threshold is 6 same-file edits
        for i in 0..6 {
            let mut output = "ok".to_string();
            policy
                .on_tool_result("Edit", &format!("{i}"), &edit_input, &mut output)
                .await;
        }
        assert!(policy.should_stop(&make_ctx(0)).await);
    }

    #[tokio::test]
    async fn stops_on_total_edits_exceeding_limit() {
        let mut policy = EarlyStopPolicy::new();

        // Default threshold is 15 total edits
        for i in 0..15 {
            let input = serde_json::json!({"path": format!("file_{i}.rs"), "content": "x"});
            let mut output = "ok".to_string();
            policy
                .on_tool_result("Edit", &format!("{i}"), &input, &mut output)
                .await;
        }
        assert!(policy.should_stop(&make_ctx(0)).await);
    }

    #[tokio::test]
    async fn stops_on_consecutive_error_turns() {
        let mut policy = EarlyStopPolicy::new();

        for turn in 0..3 {
            let mut output = "Error something failed".to_string();
            policy
                .on_tool_result("Bash", &format!("{turn}"), &Value::Null, &mut output)
                .await;
            let should_stop = policy.should_stop(&make_ctx(turn)).await;
            if turn < 2 {
                assert!(!should_stop, "should not stop at turn {turn}");
            } else {
                assert!(should_stop, "should stop at turn {turn}");
            }
        }
    }

    #[tokio::test]
    async fn does_not_stop_when_conditions_not_met() {
        let mut policy = EarlyStopPolicy::new();

        for turn in 0..5 {
            let mut output = "file contents".to_string();
            policy
                .on_tool_result("Read", &format!("{turn}"), &Value::Null, &mut output)
                .await;
            assert!(!policy.should_stop(&make_ctx(turn)).await);
        }
    }

    #[test]
    fn error_detection_covers_known_patterns() {
        assert!(is_error_output("Error file not found"));
        assert!(is_error_output("ERROR: something bad"));
        assert!(is_error_output("error: compilation failed"));
        assert!(is_error_output("output [exit code: 1]"));
        assert!(is_error_output("bash: command not found"));
        assert!(is_error_output("No such file or directory"));
        assert!(is_error_output("Permission denied"));

        assert!(!is_error_output("file contents here"));
        assert!(!is_error_output("output [exit code: 0]"));
        assert!(!is_error_output("All tests passed"));
    }

    #[test]
    fn default_impl_is_new() {
        let default = EarlyStopPolicy::default();
        let new = EarlyStopPolicy::new();
        assert_eq!(default.turns_since_edit, new.turns_since_edit);
        assert_eq!(default.total_edits, new.total_edits);
        assert_eq!(default.consecutive_error_turns, new.consecutive_error_turns);
        assert_eq!(default.made_edits, new.made_edits);
        assert_eq!(default.max_total_edits, DEFAULT_MAX_TOTAL_EDITS);
        assert_eq!(default.max_turns_since_edit, DEFAULT_MAX_TURNS_SINCE_EDIT);
        assert_eq!(default.max_same_file_edits, DEFAULT_MAX_SAME_FILE_EDITS);
        assert_eq!(
            default.max_consecutive_errors,
            DEFAULT_MAX_CONSECUTIVE_ERRORS
        );
    }

    #[tokio::test]
    async fn custom_thresholds_override_defaults() {
        // Use aggressive thresholds to verify they're respected
        let mut policy = EarlyStopPolicy::with_thresholds(2, 1, 2, 5);

        // 2 edits should trigger stop (custom max_total_edits=2)
        for i in 0..2 {
            let input = serde_json::json!({"path": format!("file_{i}.rs"), "content": "x"});
            let mut output = "ok".to_string();
            policy
                .on_tool_result("Edit", &format!("{i}"), &input, &mut output)
                .await;
        }
        assert!(policy.should_stop(&make_ctx(0)).await);
    }

    #[tokio::test]
    async fn does_not_stop_before_custom_thresholds() {
        let mut policy = EarlyStopPolicy::with_thresholds(100, 100, 100, 100);

        for i in 0..20 {
            let input = serde_json::json!({"path": format!("file_{i}.rs"), "content": "x"});
            let mut output = "ok".to_string();
            policy
                .on_tool_result("Edit", &format!("{i}"), &input, &mut output)
                .await;
            assert!(
                !policy.should_stop(&make_ctx(i)).await,
                "should not stop with high thresholds at turn {i}"
            );
        }
    }
}
