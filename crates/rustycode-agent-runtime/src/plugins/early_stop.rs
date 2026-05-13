//! Early-stop policy — halts the agent loop when heuristics detect stagnation.
//!
//! Conditions (any one triggers stop):
//! - 2+ turns since last edit (after at least one edit was made)
//! - Same file edited 3+ times (thrashing)
//! - 4+ total edits across all files (scope creep)
//! - 3 consecutive error-only turns

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use super::{AgentPlugin, TurnContext};

const EDIT_TOOLS: &[&str] = &["edit", "write", "apply_patch", "Edit", "Write"];

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
}

impl EarlyStopPolicy {
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
        }
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

        if self.made_edits && self.turns_since_edit >= 2 {
            return true;
        }
        if let Some(count) = self.file_edit_counts.values().max() {
            if *count >= 3 {
                return true;
            }
        }
        if self.total_edits >= 4 {
            return true;
        }
        if self.consecutive_error_turns >= 3 {
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
    async fn stops_after_two_turns_since_last_edit() {
        let mut policy = EarlyStopPolicy::new();
        let edit_input = serde_json::json!({"path": "foo.rs", "content": "hello"});

        let mut output = "ok".to_string();
        policy
            .on_tool_result("Edit", "1", &edit_input, &mut output)
            .await;
        assert!(!policy.should_stop(&make_ctx(0)).await);

        let mut output = "file contents".to_string();
        policy
            .on_tool_result("Read", "2", &Value::Null, &mut output)
            .await;
        assert!(!policy.should_stop(&make_ctx(1)).await);

        let mut output = "more contents".to_string();
        policy
            .on_tool_result("Read", "3", &Value::Null, &mut output)
            .await;
        assert!(policy.should_stop(&make_ctx(2)).await);
    }

    #[tokio::test]
    async fn stops_on_same_file_thrashing() {
        let mut policy = EarlyStopPolicy::new();
        let edit_input = serde_json::json!({"path": "main.rs", "content": "x"});

        for i in 0..3 {
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

        for i in 0..4 {
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
    }
}
