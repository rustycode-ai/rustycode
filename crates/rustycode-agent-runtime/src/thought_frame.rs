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
#[allow(clippy::expect_used)]
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
}
