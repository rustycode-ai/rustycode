//! ThoughtFrame: structured working memory for agent sessions.
//!
//! Tracks explored/modified files, stuck detection, and heuristic findings.
//! Generates state-derived thinking nudges instead of static text templates.
//!
//! Persistence is caller-controlled via `save_to`/`load_from` with explicit paths.

use crate::task_brief::TaskBrief;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Persistent working memory that tracks what the agent has done and generates
/// state-derived thinking nudges instead of static text templates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThoughtFrame {
    pub confidence: f32,
    /// Files read directly (READ, INSPECT). Not directories from search tools.
    pub explored_files: HashSet<String>,
    pub modified_files: HashSet<String>,
    pub findings: Vec<String>,
    pub stuck_counter: usize,
    pub last_edit_turn: usize,
    pub read_count: HashMap<String, usize>,
    pub turn: usize,
    pub max_turns: usize,
    /// Delegated-agent contract, set by the caller before the session starts.
    #[serde(default)]
    pub task_brief: Option<TaskBrief>,
}

impl ThoughtFrame {
    pub fn new(max_turns: usize) -> Self {
        Self {
            confidence: 0.0,
            explored_files: HashSet::new(),
            modified_files: HashSet::new(),
            findings: Vec::new(),
            stuck_counter: 0,
            last_edit_turn: 0,
            read_count: HashMap::new(),
            turn: 0,
            max_turns,
            task_brief: None,
        }
    }

    /// Record a tool call and update tracked state.
    ///
    /// Only file-level reads (READ, INSPECT) contribute to explored_files and stuck detection.
    /// Directory searches (GREP, GLOB, FIND, LIST_DIR) are excluded because their `path`
    /// field is a directory, not a file — mixing them in explored_files produces misleading nudges.
    pub fn record_tool(&mut self, turn: usize, tool_name: &str, input: &serde_json::Value) {
        use rustycode_protocol::tool_names as tn;

        let is_file_read = matches!(tool_name, tn::READ | tn::INSPECT);
        let is_write = matches!(
            tool_name,
            tn::WRITE
                | tn::EDIT
                | tn::MULTI_EDIT
                | tn::APPLY_PATCH
                | tn::SEARCH_REPLACE
                | tn::NOTEBOOK_EDIT
        );

        let mut explored_new = false;

        if is_file_read {
            if let Some(path) = extract_path(input) {
                explored_new = self.explored_files.insert(path.clone());
                *self.read_count.entry(path).or_insert(0) += 1;
            }
        }

        if is_write {
            if let Some(path) = extract_path(input) {
                self.modified_files.insert(path);
            }
            self.stuck_counter = 0;
            self.last_edit_turn = turn;
        } else if is_file_read && !explored_new {
            // Stuck = re-reading known files AND no writes for 3+ turns
            if self.last_edit_turn < turn.saturating_sub(3) {
                self.stuck_counter = self.stuck_counter.saturating_add(1);
            }
        }

        self.turn = turn;
    }

    /// Heuristically extract a key observation from tool output.
    pub fn record_finding(&mut self, tool_name: &str, output: &str) {
        use rustycode_protocol::tool_names as tn;
        if self.findings.len() >= 10 {
            return;
        }
        let finding = match tool_name {
            tn::BASH => {
                if output.contains("FAILED") || output.contains("error:") {
                    output
                        .lines()
                        .find(|l| l.contains("FAILED") || l.contains("error:"))
                        .map(|l| format!("Test/error: {}", l.chars().take(120).collect::<String>()))
                } else if output.to_ascii_lowercase().contains("passed")
                    && !output.to_ascii_lowercase().contains("failed")
                {
                    Some("Tests passing".to_string())
                } else {
                    None
                }
            }
            tn::GREP if !output.trim().is_empty() => output
                .lines()
                .next()
                .map(|l| format!("Found: {}", l.chars().take(120).collect::<String>())),
            _ => None,
        };
        if let Some(f) = finding {
            self.findings.push(f);
        }
    }

    /// Files that have been read more than once (anti-duplication signal).
    pub fn re_read_warnings(&self) -> Vec<&str> {
        self.read_count
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(path, _)| path.as_str())
            .collect()
    }

    /// Turns since last file write.
    pub fn turns_since_edit(&self) -> usize {
        if self.last_edit_turn == 0 && self.modified_files.is_empty() {
            self.turn
        } else {
            self.turn.saturating_sub(self.last_edit_turn)
        }
    }

    /// Generate a state-derived thinking nudge XML block.
    pub fn generate_nudge(&self) -> String {
        let remaining = self.max_turns.saturating_sub(self.turn);
        let progress = self.turn as f32 / self.max_turns.max(1) as f32;
        let turns_since = self.turns_since_edit();
        let is_stuck = self.stuck_counter >= 3;

        let mut lines = Vec::with_capacity(16);
        lines.push(format!(
            r#"<turn-reflection turn="{}/{}" remaining="{}">"#,
            self.turn, self.max_turns, remaining
        ));

        // What's been done
        if !self.explored_files.is_empty() {
            let explored: Vec<&str> = self
                .explored_files
                .iter()
                .take(8)
                .map(String::as_str)
                .collect();
            lines.push(format!(
                "Files explored ({}): {}",
                self.explored_files.len(),
                explored.join(", ")
            ));
        }
        if self.modified_files.is_empty() {
            lines.push("No files modified yet.".to_string());
        } else {
            let modified: Vec<&str> = self.modified_files.iter().map(String::as_str).collect();
            lines.push(format!("Files modified: {}", modified.join(", ")));
        }

        // Key findings (last 4)
        if !self.findings.is_empty() {
            lines.push("Key findings:".to_string());
            for f in self.findings.iter().rev().take(4) {
                lines.push(format!("  - {f}"));
            }
        }

        // Anti-duplication warning
        let rereads = self.re_read_warnings();
        if !rereads.is_empty() {
            lines.push(format!(
                "Re-read warning: {} — don't read these again.",
                rereads.join(", ")
            ));
        }

        // Stuck detection
        if is_stuck {
            lines.push(format!(
                "STUCK: {turns_since} turns since last edit. Make a targeted change NOW."
            ));
        } else if turns_since > 2 {
            lines.push(format!("Note: {turns_since} turns since last edit."));
        }

        // Phase-specific guidance
        if progress < 0.25 {
            lines.push(
                "Phase: EXPLORATION — form a specific hypothesis before making changes."
                    .to_string(),
            );
        } else if progress < 0.6 {
            let hint = if is_stuck {
                "Make a targeted change NOW."
            } else if !self.modified_files.is_empty() && self.findings.len() > 2 {
                "You have a fix in progress. Verify it works."
            } else {
                "Continue with your current approach."
            };
            lines.push(format!("Phase: ACTION — {hint}"));
        } else {
            lines.push(format!(
                "Phase: VERIFICATION — {remaining} turns left. Assume your fix is subtly broken until proven otherwise."
            ));
        }

        lines.push("</turn-reflection>".to_string());

        // Delegated-agent supplement
        if let Some(brief) = &self.task_brief {
            Self::append_task_brief_nudge(brief, self.turn, &mut lines);
        }

        lines.join("\n")
    }

    /// Append delegated-agent nudge lines inside the turn-reflection block.
    ///
    /// Per the plan:
    /// - Role hint on turns 1-2 only
    /// - Scope drift warning only when violated
    /// - Mission reminder every 5 turns, truncated
    fn append_task_brief_nudge(brief: &TaskBrief, turn: usize, lines: &mut Vec<String>) {
        // Role hint — first two turns
        if turn <= 2 {
            lines.push(brief.role_hint().to_string());
        }

        // Scope drift — only when the agent has explored outside scope
        // (checked against explored_files in the nudge's own data)
        // We don't have explored_files here, so we emit the scope boundary for
        // the model to self-correct. Only meaningful when path_scope is non-empty.
        if !brief.path_scope.is_empty() {
            let scope_str: Vec<&str> = brief
                .path_scope
                .iter()
                .map(|p| p.to_str().unwrap_or("?"))
                .collect();
            lines.push(format!("Assigned scope: {}", scope_str.join(", ")));
        }

        // Mission reminder — every 5 turns
        if turn.is_multiple_of(5) && turn > 0 {
            let truncated: String = brief.brief.chars().take(200).collect();
            lines.push(format!("Task: \"{truncated}\""));
        }
    }

    /// Persist the frame to an explicit path. The caller controls naming and location.
    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("ThoughtFrame: failed to create {}: {e}", parent.display());
                }
            }
        }
        let json = match serde_json::to_string_pretty(self) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("ThoughtFrame: failed to serialize: {e}");
                return;
            }
        };
        if let Err(e) = std::fs::write(path, json) {
            tracing::warn!("ThoughtFrame: failed to write {}: {e}", path.display());
        }
    }

    /// Load a previously persisted frame from an explicit path.
    pub fn load_from(path: &Path, max_turns: usize) -> Option<Self> {
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => {
                tracing::debug!("ThoughtFrame: no saved frame at {}: {e}", path.display());
                return None;
            }
        };
        serde_json::from_str(&raw)
            .map(|mut f: Self| {
                f.max_turns = max_turns;
                f
            })
            .map_err(|e| {
                tracing::warn!("ThoughtFrame: corrupt frame at {}: {e}", path.display());
                e
            })
            .ok()
    }
}

/// Extract a file path from a tool's JSON input.
/// Checks common keys: `path`, `file_path`, `absolute_path`, `file`.
fn extract_path(input: &serde_json::Value) -> Option<String> {
    input
        .get("path")
        .or_else(|| input.get("file_path"))
        .or_else(|| input.get("absolute_path"))
        .or_else(|| input.get("file"))
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn thought_frame_early_turn_shows_exploration_phase() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 1;
        let nudge = frame.generate_nudge();
        assert!(nudge.contains("<turn-reflection"));
        assert!(nudge.contains("turn=\"1/20\""));
        assert!(nudge.contains("EXPLORATION"));
    }

    #[test]
    fn thought_frame_mid_turn_shows_action_phase() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 8;
        frame.modified_files.insert("src/main.rs".to_string());
        frame.findings.push("Found: src/main.rs:42".to_string());
        frame
            .findings
            .push("Test/error: assertion failed".to_string());
        frame.findings.push("Tests passing".to_string());
        let nudge = frame.generate_nudge();
        assert!(nudge.contains("turn=\"8/20\""));
        assert!(nudge.contains("ACTION"));
        assert!(nudge.contains("src/main.rs"));
    }

    #[test]
    fn thought_frame_late_turn_shows_verification() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 17;
        let nudge = frame.generate_nudge();
        assert!(nudge.contains("remaining=\"3\""));
        assert!(nudge.contains("VERIFICATION"));
        assert!(nudge.contains("subtly broken"));
    }

    #[test]
    fn thought_frame_stuck_detection() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 10;
        frame.stuck_counter = 4;
        frame.last_edit_turn = 3;
        let nudge = frame.generate_nudge();
        assert!(nudge.contains("STUCK"));
    }

    #[test]
    fn thought_frame_re_read_warning() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 5;
        frame.explored_files.insert("src/auth.rs".to_string());
        frame.read_count.insert("src/auth.rs".to_string(), 3);
        let nudge = frame.generate_nudge();
        assert!(nudge.contains("Re-read warning"));
        assert!(nudge.contains("src/auth.rs"));
    }

    #[test]
    fn record_tool_tracks_file_reads_not_searches() {
        let mut frame = ThoughtFrame::new(20);

        // READ with file_path → tracked
        frame.record_tool(1, "Read", &serde_json::json!({"file_path": "/src/main.rs"}));
        assert!(frame.explored_files.contains("/src/main.rs"));

        // GREP with path (directory) → NOT tracked in explored_files
        frame.record_tool(
            2,
            "Grep",
            &serde_json::json!({"pattern": "TODO", "path": "/src"}),
        );
        assert!(!frame.explored_files.contains("/src"));
        assert_eq!(frame.explored_files.len(), 1);

        // Stuck counter should not increment for re-reads of same file on consecutive turns
        // because last_edit_turn (0) is not < turn(2) - 3 = -1 (saturates to 0)
        assert_eq!(frame.stuck_counter, 0);
    }

    #[test]
    fn stuck_counter_only_increments_for_repeated_file_reads() {
        let mut frame = ThoughtFrame::new(20);

        // Turn 1: read file A → new exploration
        frame.record_tool(1, "Read", &serde_json::json!({"file_path": "/src/a.rs"}));
        assert_eq!(frame.stuck_counter, 0);

        // Turn 5: read file A again → re-read, and last_edit_turn(0) < 5-3=2
        frame.record_tool(5, "Read", &serde_json::json!({"file_path": "/src/a.rs"}));
        assert_eq!(frame.stuck_counter, 1);

        // Turn 6: write → resets stuck counter
        frame.record_tool(
            6,
            "Write",
            &serde_json::json!({"path": "/src/a.rs", "content": "fix"}),
        );
        assert_eq!(frame.stuck_counter, 0);
        assert_eq!(frame.last_edit_turn, 6);

        // Turn 10: read new file → new exploration, no stuck increment
        frame.record_tool(10, "Read", &serde_json::json!({"file_path": "/src/b.rs"}));
        assert_eq!(frame.stuck_counter, 0);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("rustycode_test_frame");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("test-frame-{}.json", std::process::id()));

        let mut frame = ThoughtFrame::new(20);
        frame.turn = 5;
        frame.explored_files.insert("src/main.rs".to_string());
        frame.modified_files.insert("src/lib.rs".to_string());
        frame.findings.push("Tests passing".to_string());
        frame.save_to(&path);

        let loaded = ThoughtFrame::load_from(&path, 25).expect("load");
        assert_eq!(loaded.turn, 5);
        assert_eq!(loaded.max_turns, 25); // overridden by load
        assert!(loaded.explored_files.contains("src/main.rs"));
        assert!(loaded.modified_files.contains("src/lib.rs"));
        assert_eq!(loaded.findings.len(), 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_none() {
        let loaded = ThoughtFrame::load_from(Path::new("/nonexistent/frame.json"), 20);
        assert!(loaded.is_none());
    }

    #[test]
    fn task_brief_nudge_role_hint_on_early_turns() {
        use crate::task_brief::TaskBrief;
        use rustycode_protocol::agent_protocol::AgentRole;
        use rustycode_protocol::tool_names as tn;
        use std::path::PathBuf;

        let brief = TaskBrief {
            role: AgentRole::Researcher,
            brief: "Investigate auth module".into(),
            path_scope: vec![PathBuf::from("src/auth")],
            allowed_tools: vec![tn::READ.into(), tn::GREP.into()],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.turn = 1;
        frame.task_brief = Some(brief);

        let nudge = frame.generate_nudge();
        assert!(nudge.contains("Explorer: read and map the area"));
        assert!(nudge.contains("Assigned scope: src/auth"));
        // Turn 1 is not a multiple of 5, so no mission reminder
        assert!(!nudge.contains("Task:"));
    }

    #[test]
    fn task_brief_nudge_mission_reminder_every_5_turns() {
        use crate::task_brief::TaskBrief;
        use rustycode_protocol::agent_protocol::AgentRole;

        let brief = TaskBrief {
            role: AgentRole::Builder,
            brief: "Fix the auth bug in token refresh".into(),
            path_scope: vec![],
            allowed_tools: vec![],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.turn = 5;
        frame.task_brief = Some(brief);

        let nudge = frame.generate_nudge();
        assert!(nudge.contains("Task: \"Fix the auth bug in token refresh\""));
        // Turn 5 > 2, so no role hint
        assert!(!nudge.contains("Implementer: make targeted"));
    }

    #[test]
    fn task_brief_nudge_no_nudge_without_brief() {
        let mut frame = ThoughtFrame::new(20);
        frame.turn = 5;
        frame.task_brief = None;

        let nudge = frame.generate_nudge();
        assert!(!nudge.contains("Assigned scope:"));
        assert!(!nudge.contains("Task:"));
    }

    #[test]
    fn task_brief_persists_through_save_load() {
        use crate::task_brief::TaskBrief;
        use rustycode_protocol::agent_protocol::AgentRole;
        use rustycode_protocol::tool_names as tn;
        use std::path::PathBuf;

        let dir = std::env::temp_dir().join("rustycode_test_frame_brief");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("test-brief-{}.json", std::process::id()));

        let brief = TaskBrief {
            role: AgentRole::Skeptic,
            brief: "Review PR changes".into(),
            path_scope: vec![PathBuf::from("src/")],
            allowed_tools: vec![tn::READ.into(), tn::GREP.into()],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.turn = 3;
        frame.task_brief = Some(brief);
        frame.save_to(&path);

        let loaded = ThoughtFrame::load_from(&path, 20).expect("load");
        let loaded_brief = loaded.task_brief.expect("task_brief");
        assert_eq!(loaded_brief.role, AgentRole::Skeptic);
        assert_eq!(loaded_brief.brief, "Review PR changes");
        assert!(loaded_brief.allowed_tools.contains(&tn::READ.to_string()));
        assert!(!loaded_brief.allowed_tools.contains(&tn::BASH.to_string()));

        let _ = std::fs::remove_file(&path);
    }

    /// Backward-compat: a JSON file written before TaskBrief existed should
    /// load with `task_brief: None` thanks to `#[serde(default)]`.
    #[test]
    fn load_pre_task_brief_json_succeeds() {
        let dir = std::env::temp_dir().join("rustycode_test_frame_compat");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("compat-{}.json", std::process::id()));

        let legacy_json = r#"{
            "confidence": 0.5,
            "explored_files": ["src/main.rs"],
            "modified_files": [],
            "findings": [],
            "stuck_counter": 0,
            "last_edit_turn": 0,
            "read_count": {},
            "turn": 3,
            "max_turns": 20
        }"#;
        std::fs::write(&path, legacy_json).expect("write");

        let loaded = ThoughtFrame::load_from(&path, 25).expect("load");
        assert_eq!(loaded.turn, 3);
        assert_eq!(loaded.max_turns, 25);
        assert!(loaded.task_brief.is_none());
        assert!(loaded.explored_files.contains("src/main.rs"));

        let _ = std::fs::remove_file(&path);
    }

    /// Negative test: role hint should NOT appear after turn 2.
    #[test]
    fn task_brief_nudge_role_hint_absent_after_turn_2() {
        use crate::task_brief::TaskBrief;
        use rustycode_protocol::agent_protocol::AgentRole;

        let brief = TaskBrief {
            role: AgentRole::Researcher,
            brief: "Investigate auth module".into(),
            path_scope: vec![],
            allowed_tools: vec![],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.task_brief = Some(brief);

        // Turn 3 — role hint should be absent
        frame.turn = 3;
        let nudge = frame.generate_nudge();
        assert!(
            !nudge.contains("Explorer: read and map the area"),
            "role hint should not appear at turn 3"
        );

        frame.turn = 10;
        let nudge = frame.generate_nudge();
        assert!(
            !nudge.contains("Explorer: read and map the area"),
            "role hint should not appear at turn 10"
        );
    }

    /// Negative test: mission reminder should NOT appear on non-multiples-of-5 turns.
    #[test]
    fn task_brief_nudge_reminder_absent_on_non_multiples_of_5() {
        use crate::task_brief::TaskBrief;
        use rustycode_protocol::agent_protocol::AgentRole;

        let brief = TaskBrief {
            role: AgentRole::Builder,
            brief: "Fix the auth bug".into(),
            path_scope: vec![],
            allowed_tools: vec![],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.task_brief = Some(brief);

        frame.turn = 4;
        let nudge = frame.generate_nudge();
        assert!(
            !nudge.contains("Task: \"Fix the auth bug\""),
            "reminder should not appear at turn 4"
        );

        frame.turn = 6;
        let nudge = frame.generate_nudge();
        assert!(
            !nudge.contains("Task: \"Fix the auth bug\""),
            "reminder should not appear at turn 6"
        );

        frame.turn = 10;
        let nudge = frame.generate_nudge();
        assert!(
            nudge.contains("Task: \"Fix the auth bug\""),
            "reminder should appear at turn 10"
        );
    }

    // --- SWE-bench lifecycle simulation tests ---

    use rustycode_protocol::agent_protocol::AgentRole;
    use rustycode_protocol::tool_names as tn;
    use std::path::PathBuf;

    fn swe_bench_brief() -> TaskBrief {
        TaskBrief {
            role: AgentRole::Builder,
            brief: "Fix the race condition in the connection pool: \
                    when multiple threads call get_connection() simultaneously \
                    with an empty pool, they all create new connections instead \
                    of waiting. Add a semaphore or condition variable to coordinate."
                .into(),
            path_scope: vec![PathBuf::from("src/pool"), PathBuf::from("tests/pool")],
            allowed_tools: vec![
                tn::READ.into(),
                tn::WRITE.into(),
                tn::EDIT.into(),
                tn::GREP.into(),
                tn::LIST_DIR.into(),
                tn::GLOB.into(),
                tn::BASH.into(),
            ],
        }
    }

    /// Simulate a full 25-turn SWE-bench session with realistic tool calls
    /// and verify the ThoughtFrame + TaskBrief contract holds at every step.
    fn simulate_25_turns(frame: &mut ThoughtFrame) -> Vec<(usize, String)> {
        let max_turns = 25;
        (1..=max_turns)
            .map(|turn| {
                match turn {
                    1..=4 => {
                        let files = [
                            "src/pool/mod.rs",
                            "src/pool/connection.rs",
                            "src/pool/manager.rs",
                            "src/pool/config.rs",
                        ];
                        frame.record_tool(
                            turn,
                            tn::READ,
                            &serde_json::json!({"file_path": files[turn - 1]}),
                        );
                        if turn <= 2 {
                            frame.record_tool(
                                turn,
                                tn::GREP,
                                &serde_json::json!({"pattern": "get_connection", "path": "src/pool"}),
                            );
                        }
                    }
                    5..=12 => {
                        if turn == 5 {
                            frame.record_tool(turn, tn::EDIT,
                                &serde_json::json!({"file_path": "src/pool/manager.rs", "old": "fn get_connection", "new": "async fn get_connection"}));
                            frame.record_finding(tn::BASH, "Test/error: assertion failed: pool size exceeded");
                        } else if turn == 6 {
                            frame.record_tool(turn, tn::EDIT, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        } else if turn == 7 {
                            frame.record_tool(turn, tn::READ, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        } else if turn == 8 {
                            frame.record_tool(turn, tn::BASH, &serde_json::json!({"command": "cargo test -p pool"}));
                            frame.record_finding(tn::BASH, "Test/error: deadlock detected in test_concurrent_access");
                        } else if turn == 9 {
                            frame.record_tool(turn, tn::EDIT, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        } else if turn == 10 {
                            frame.record_tool(turn, tn::BASH, &serde_json::json!({"command": "cargo test -p pool"}));
                            frame.record_finding(tn::BASH, "passed; 12 passed, 0 failed");
                        } else {
                            frame.record_tool(turn, tn::READ, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        }
                    }
                    13..=20 => {
                        if turn == 13 {
                            frame.record_tool(turn, tn::READ, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        } else if turn % 2 == 0 {
                            frame.record_tool(turn, tn::BASH, &serde_json::json!({"command": "cargo test -p pool"}));
                        } else {
                            frame.record_tool(turn, tn::READ, &serde_json::json!({"file_path": "src/pool/manager.rs"}));
                        }
                    }
                    _ => {
                        frame.record_tool(turn, tn::BASH, &serde_json::json!({"command": "cargo test --workspace"}));
                    }
                }
                let nudge = frame.generate_nudge();
                (turn, nudge)
            })
            .collect()
    }

    #[test]
    fn swe_bench_full_25_turn_lifecycle() {
        let brief = swe_bench_brief();
        let mut frame = ThoughtFrame::new(25);
        frame.task_brief = Some(brief);

        let nudges = simulate_25_turns(&mut frame);

        // Verify phase transitions
        assert!(
            nudges[0].1.contains("EXPLORATION"),
            "turn 1 should be exploration"
        );
        assert!(
            nudges[0].1.contains("Implementer: make targeted changes"),
            "turn 1 should have role hint"
        );

        // Turn 5 — still exploration (5/25 = 0.20 < 0.25), mission reminder
        assert!(
            nudges[4].1.contains("EXPLORATION"),
            "turn 5 should still be exploration (progress < 0.25)"
        );
        assert!(
            nudges[4].1.contains("Task: \"Fix the race condition"),
            "turn 5 should have mission reminder"
        );
        assert!(
            !nudges[4].1.contains("Implementer:"),
            "turn 5 should NOT have role hint"
        );

        // Turn 7 — action phase (7/25 = 0.28)
        assert!(
            nudges[6].1.contains("ACTION"),
            "turn 7 should be action phase"
        );

        // Turn 10 — mission reminder again
        assert!(
            nudges[9].1.contains("Task: \"Fix the race condition"),
            "turn 10 should have mission reminder"
        );

        // Turn 15 — mission reminder, still action/verification
        assert!(
            nudges[14].1.contains("Task: \"Fix the race condition"),
            "turn 15 should have mission reminder"
        );

        // Turn 16+ — verification phase
        assert!(
            nudges[15].1.contains("VERIFICATION"),
            "turn 16 should be verification phase"
        );
        assert!(
            nudges[15].1.contains("subtly broken"),
            "verification should mention subtlety"
        );

        // Turn 20 — mission reminder
        assert!(
            nudges[19].1.contains("Task: \"Fix the race condition"),
            "turn 20 should have mission reminder"
        );

        // Turn 25 — final turn
        assert!(
            nudges[24].1.contains("remaining=\"0\""),
            "turn 25 should show 0 remaining"
        );
        assert!(
            nudges[24].1.contains("Task: \"Fix the race condition"),
            "turn 25 should have mission reminder"
        );

        // Scope should always appear (path_scope is non-empty)
        for (turn, nudge) in &nudges {
            assert!(
                nudge.contains("Assigned scope: src/pool, tests/pool"),
                "turn {turn} should show scope boundary"
            );
        }

        // Stuck detection: turns 13-20 re-read manager.rs many times
        assert!(
            frame.stuck_counter >= 3,
            "should detect stuck after repeated re-reads without edits"
        );

        // Re-read warnings should exist
        let rereads = frame.re_read_warnings();
        assert!(
            rereads.contains(&"src/pool/manager.rs"),
            "manager.rs should have re-read warnings"
        );

        // Files explored should include pool files
        assert!(frame.explored_files.contains("src/pool/mod.rs"));
        assert!(frame.explored_files.contains("src/pool/manager.rs"));

        // Files modified should include manager.rs
        assert!(frame.modified_files.contains("src/pool/manager.rs"));

        // Save/load should preserve the full state
        let dir = std::env::temp_dir().join("rustycode_swe_bench_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("swe-bench-{}.json", std::process::id()));
        frame.save_to(&path);

        let loaded = ThoughtFrame::load_from(&path, 25).expect("load");
        assert_eq!(loaded.turn, 25);
        assert_eq!(loaded.explored_files.len(), frame.explored_files.len());
        assert_eq!(loaded.modified_files.len(), frame.modified_files.len());

        let loaded_brief = loaded.task_brief.expect("brief should survive save/load");
        assert_eq!(loaded_brief.role, AgentRole::Builder);
        assert!(loaded_brief.brief.starts_with("Fix the race condition"));
        assert_eq!(loaded_brief.allowed_tools.len(), 7);
        assert!(!loaded_brief
            .allowed_tools
            .contains(&"ApplyPatch".to_string()));

        let _ = std::fs::remove_file(&path);
    }

    /// Verify that a read-only role (Explore) accumulates stuck counter
    /// because it never writes, but the nudge correctly shows "STUCK" after
    /// 3+ re-reads without writes.
    #[test]
    fn swe_bench_explore_role_stuck_detection() {
        let brief = TaskBrief {
            role: AgentRole::Researcher,
            brief: "Map the authentication module architecture".into(),
            path_scope: vec![PathBuf::from("src/auth")],
            allowed_tools: vec![
                tn::READ.into(),
                tn::GREP.into(),
                tn::LIST_DIR.into(),
                tn::GLOB.into(),
            ],
        };

        let mut frame = ThoughtFrame::new(15);
        frame.task_brief = Some(brief);

        // Turns 1-3: explore different files (not stuck)
        frame.record_tool(
            1,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/mod.rs"}),
        );
        frame.record_tool(
            2,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/tokens.rs"}),
        );
        frame.record_tool(
            3,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/middleware.rs"}),
        );

        assert_eq!(frame.stuck_counter, 0, "exploring new files is not stuck");

        // Turns 4-7: re-read same files repeatedly (should trigger stuck)
        frame.record_tool(
            4,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/mod.rs"}),
        );
        frame.record_tool(
            5,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/mod.rs"}),
        );
        frame.record_tool(
            6,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/tokens.rs"}),
        );
        frame.record_tool(
            7,
            tn::READ,
            &serde_json::json!({"file_path": "src/auth/mod.rs"}),
        );

        let nudge = frame.generate_nudge();
        assert!(
            nudge.contains("STUCK"),
            "should detect stuck after re-reading without writing"
        );
        assert!(
            !nudge.contains("Explorer: read and map"),
            "turn 7 > 2, no role hint"
        );
        assert!(nudge.contains("Assigned scope: src/auth"));
    }

    /// Verify scope boundary enforcement in nudges.
    /// An Explore agent reading outside its scope should see scope boundary
    /// in the nudge, but the nudge data itself only shows the assigned scope.
    #[test]
    fn swe_bench_scope_boundary_shown_for_scoped_briefs() {
        let brief = TaskBrief {
            role: AgentRole::Scalpel,
            brief: "Debug the timeout in connection_pool.rs".into(),
            path_scope: vec![PathBuf::from("src/net/pool.rs")],
            allowed_tools: vec![
                tn::READ.into(),
                tn::GREP.into(),
                tn::LIST_DIR.into(),
                tn::GLOB.into(),
                tn::BASH.into(),
            ],
        };

        let mut frame = ThoughtFrame::new(20);
        frame.task_brief = Some(brief);
        frame.turn = 2;

        let nudge = frame.generate_nudge();
        assert!(nudge.contains("Debugger: find the root cause"));
        assert!(nudge.contains("Assigned scope: src/net/pool.rs"));
    }

    /// Verify that a very long brief gets truncated to 200 chars in the mission reminder.
    #[test]
    fn swe_bench_long_brief_truncated_in_reminder() {
        let long_brief = "A".repeat(500);
        let brief = TaskBrief {
            role: AgentRole::Builder,
            brief: long_brief,
            path_scope: vec![],
            allowed_tools: vec![tn::READ.into(), tn::WRITE.into()],
        };

        let mut frame = ThoughtFrame::new(25);
        frame.task_brief = Some(brief);
        frame.turn = 5;

        let nudge = frame.generate_nudge();
        let task_line = nudge
            .lines()
            .find(|l| l.starts_with("Task:"))
            .expect("should have Task line");
        let content = task_line.trim_start_matches("Task: \"");
        let content = content.trim_end_matches('"');
        assert!(
            content.len() <= 200,
            "brief in reminder should be at most 200 chars, got {}",
            content.len()
        );
    }

    /// Verify that ToolActivationManager correctly gates tools for every role,
    /// matching the deny-by-default policy.
    ///
    /// Uses the same tool lists as TaskRole::allowed_tools() in orchestration,
    /// but duplicated here to avoid a cross-crate dependency.
    #[test]
    fn swe_bench_all_roles_tool_gating_via_activation_manager() {
        use rustycode_tools_api::tiers::{ToolActivationManager, ToolTier};

        // (role_label, allowed_tools scope, tools that must be allowed, tools that must be denied)
        let roles_and_expectations: Vec<(&str, Vec<&str>, Vec<&str>, Vec<&str>)> = vec![
            (
                "Explore",
                vec!["Read", "Grep", "ListDir", "Glob", "FuzzyFind"],
                vec!["Read", "Grep", "ListDir", "Glob"],
                vec!["Write", "Edit", "Bash", "ApplyPatch", "MultiEdit"],
            ),
            (
                "Code",
                vec![
                    "Read",
                    "Write",
                    "Edit",
                    "Grep",
                    "ListDir",
                    "Glob",
                    "Bash",
                    "FuzzyFind",
                ],
                vec!["Read", "Write", "Edit", "Grep", "ListDir", "Glob", "Bash"],
                vec!["ApplyPatch", "MultiEdit", "NotebookEdit"],
            ),
            (
                "Verify",
                vec!["Read", "Grep", "ListDir", "Glob", "Bash", "FuzzyFind"],
                vec!["Read", "Grep", "ListDir", "Glob", "Bash"],
                vec!["Write", "Edit", "ApplyPatch"],
            ),
            (
                "Debug",
                vec!["Read", "Grep", "ListDir", "Glob", "Bash", "FuzzyFind"],
                vec!["Read", "Grep", "ListDir", "Glob", "Bash"],
                vec!["Write", "Edit"],
            ),
        ];

        for (role_label, scope_tools, allowed, denied) in &roles_and_expectations {
            let mut mgr = ToolActivationManager::new();
            let scope: Vec<String> = scope_tools.iter().map(|s| (*s).to_string()).collect();
            mgr.set_scope(scope);
            mgr.promote(ToolTier::Full);

            for tool in allowed {
                assert!(
                    mgr.is_tool_allowed(tool),
                    "{role_label} should allow {tool}"
                );
            }
            for tool in denied {
                assert!(
                    !mgr.is_tool_allowed(tool),
                    "{role_label} should deny {tool}"
                );
            }
        }
    }

    /// Save/load round-trip with full SWE-bench state: high turn count,
    /// many explored files, stuck counter, findings, task brief.
    #[test]
    fn swe_bench_save_load_preserves_full_state() {
        let dir = std::env::temp_dir().join("rustycode_swe_bench_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("roundtrip-{}.json", std::process::id()));

        let brief = TaskBrief {
            role: AgentRole::Builder,
            brief: "Fix race condition in pool".into(),
            path_scope: vec![PathBuf::from("src/pool")],
            allowed_tools: vec![
                tn::READ.into(),
                tn::WRITE.into(),
                tn::EDIT.into(),
                tn::BASH.into(),
            ],
        };

        let mut frame = ThoughtFrame::new(25);
        frame.task_brief = Some(brief);
        frame.turn = 18;
        frame.confidence = 0.75;
        frame.stuck_counter = 2;
        frame.last_edit_turn = 15;

        for i in 0..12 {
            let name = format!("src/pool/{i}.rs");
            frame.explored_files.insert(name.clone());
            if i < 3 {
                frame.read_count.insert(name, 2);
            }
        }
        frame
            .modified_files
            .insert("src/pool/manager.rs".to_string());
        frame
            .modified_files
            .insert("src/pool/connection.rs".to_string());
        frame
            .findings
            .push("Test/error: assertion failed".to_string());
        frame.findings.push("Tests passing".to_string());

        frame.save_to(&path);
        let loaded = ThoughtFrame::load_from(&path, 25).expect("load");

        assert_eq!(loaded.turn, 18);
        assert!((loaded.confidence - 0.75).abs() < f32::EPSILON);
        assert_eq!(loaded.stuck_counter, 2);
        assert_eq!(loaded.last_edit_turn, 15);
        assert_eq!(loaded.explored_files.len(), 12);
        assert_eq!(loaded.modified_files.len(), 2);
        assert_eq!(loaded.findings.len(), 2);
        assert_eq!(loaded.re_read_warnings().len(), 3);

        let loaded_brief = loaded.task_brief.clone().unwrap();
        assert_eq!(loaded_brief.role, AgentRole::Builder);
        assert_eq!(loaded_brief.path_scope.len(), 1);
        assert_eq!(loaded_brief.allowed_tools.len(), 4);

        let loaded_nudge = loaded.generate_nudge();
        assert!(loaded_nudge.contains("VERIFICATION"));
        assert!(loaded_nudge.contains("Re-read warning"));
        assert!(loaded_nudge.contains("Assigned scope: src/pool"));

        let _ = std::fs::remove_file(&path);
    }
}
