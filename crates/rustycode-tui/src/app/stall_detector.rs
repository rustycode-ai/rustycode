//! Exploration stall detector for agent tool execution.

/// Consecutive read-only turns before triggering the stall nudge.
pub const STALL_THRESHOLD: usize = 3;

/// Maximum consecutive read-only turns before hard enforcement.
pub const MAX_EXPLORATION_TURNS: usize = 5;

/// Tool names that count as "code" (implementation).
const CODE_TOOLS: &[&str] = &[
    "write_file",
    "edit_file",
    "multiedit",
    "apply_patch",
    "claude_text_editor",
];

/// Tool names that are always read-only (exploration).
const READ_ONLY_TOOLS: &[&str] = &[
    "read_file",
    "grep",
    "glob",
    "find",
    "list_directory",
    "ls",
    "file",
    "web_search",
    "web_fetch",
    "lsp_diagnostics",
    "lsp_hover",
    "lsp_definition",
    "lsp_references",
    "lsp_document_symbols",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolCategory {
    /// Read-only — exploration/research
    Exploration,
    /// Write/edit — implementation
    Code,
    /// Bash — could be either, classified by content
    Shell,
    /// Unknown tool — assume exploration to be safe
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StallLevel {
    /// No stall detected
    Normal,
    /// Getting close — soft nudge
    Warning,
    /// Stall detected — strong nudge to write code
    Stalled,
    /// Hard enforcement — must write code
    Critical,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct StallDetector {
    /// Consecutive turns that were purely exploration (no code tools).
    consecutive_exploration_turns: usize,
    /// Total exploration tool calls across all turns.
    total_exploration_calls: usize,
    /// Total code tool calls across all turns.
    total_code_calls: usize,
    /// Whether we've already injected a nudge this turn.
    nudge_injected_this_turn: bool,
    /// Track tools in current turn for classification.
    turn_has_code: bool,
    turn_exploration_count: usize,
}

impl Default for StallDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl StallDetector {
    pub fn new() -> Self {
        Self {
            consecutive_exploration_turns: 0,
            total_exploration_calls: 0,
            total_code_calls: 0,
            nudge_injected_this_turn: false,
            turn_has_code: false,
            turn_exploration_count: 0,
        }
    }

    /// Classify a tool call by name.
    pub fn classify_tool(tool_name: &str) -> ToolCategory {
        let lower = tool_name.to_lowercase();
        if CODE_TOOLS.contains(&lower.as_str()) {
            ToolCategory::Code
        } else if READ_ONLY_TOOLS.contains(&lower.as_str()) {
            ToolCategory::Exploration
        } else if lower == "bash" || lower == "shell" {
            ToolCategory::Shell
        } else {
            ToolCategory::Unknown
        }
    }

    /// Record a single tool call result.
    /// `success` indicates whether the tool execution succeeded.
    /// `bash_read_only` is only relevant for Shell category — true if
    /// the bash command was classified as read-only by SmartApprove.
    pub fn record_tool(&mut self, tool_name: &str, success: bool, bash_read_only: bool) {
        if !success {
            return;
        }

        let category = Self::classify_tool(tool_name);
        let is_code = match category {
            ToolCategory::Code => true,
            ToolCategory::Exploration => false,
            ToolCategory::Shell => !bash_read_only,
            ToolCategory::Unknown => false,
        };

        if is_code {
            self.total_code_calls += 1;
            self.turn_has_code = true;
        } else {
            self.total_exploration_calls += 1;
            self.turn_exploration_count += 1;
        }
    }

    /// Call at the end of each turn to finalize classification.
    /// Returns the stall level after this turn.
    pub fn end_turn(&mut self) -> StallLevel {
        if self.turn_has_code {
            self.consecutive_exploration_turns = 0;
        } else if self.turn_exploration_count > 0 {
            self.consecutive_exploration_turns += 1;
        }

        self.turn_has_code = false;
        self.turn_exploration_count = 0;
        self.nudge_injected_this_turn = false;

        self.stall_level()
    }

    /// Current stall level without modifying state.
    pub fn stall_level(&self) -> StallLevel {
        if self.consecutive_exploration_turns >= MAX_EXPLORATION_TURNS {
            StallLevel::Critical
        } else if self.consecutive_exploration_turns >= STALL_THRESHOLD {
            StallLevel::Stalled
        } else if self.consecutive_exploration_turns >= STALL_THRESHOLD - 2 {
            StallLevel::Warning
        } else {
            StallLevel::Normal
        }
    }

    /// Check if a nudge should be injected, marking it as injected if true.
    pub fn try_mark_nudge(&mut self) -> bool {
        if self.nudge_injected_this_turn {
            return false;
        }
        let level = self.stall_level();
        let should = matches!(level, StallLevel::Stalled | StallLevel::Critical);
        if should {
            self.nudge_injected_this_turn = true;
        }
        should
    }

    /// Generate the nudge message based on current stall level.
    pub fn nudge_message(&self) -> Option<String> {
        match self.stall_level() {
            StallLevel::Critical => Some(format!(
                "<system-reminder>\n\
                 EXPLORATION BUDGET EXHAUSTED. You have made {} consecutive read-only turns \
                 ({} exploration calls, {} code calls). You MUST now write code.\n\n\
                 RULES:\n\
                 - Use write_file, edit_file, or bash to produce output IMMEDIATELY\n\
                 - No more read_file, grep, glob, or exploration tools\n\
                 - Output complete working code. No placeholders, no TODOs\n\
                 - If you need more information, make reasonable assumptions and write the code\n\
                 - A partial implementation is better than continued exploration\n\
                 </system-reminder>",
                self.consecutive_exploration_turns,
                self.total_exploration_calls,
                self.total_code_calls,
            )),
            StallLevel::Stalled => Some(format!(
                "<system-reminder>\n\
                 You have spent {} turns exploring ({} read calls, {} write calls). \
                 Transition to implementation now.\n\n\
                 - You have enough information to start writing code\n\
                 - Use write_file or edit_file to produce the solution\n\
                 - Write incrementally if needed, but start producing code THIS turn\n\
                 </system-reminder>",
                self.consecutive_exploration_turns,
                self.total_exploration_calls,
                self.total_code_calls,
            )),
            StallLevel::Warning => Some(format!(
                "<system-reminder>\n\
                 Approaching exploration limit ({} of {} turns are read-only). \
                 Start writing code soon.\n\
                 </system-reminder>",
                self.consecutive_exploration_turns, STALL_THRESHOLD,
            )),
            StallLevel::Normal => None,
        }
    }

    /// Reset all state (e.g., on new conversation).
    pub fn reset(&mut self) {
        self.consecutive_exploration_turns = 0;
        self.total_exploration_calls = 0;
        self.total_code_calls = 0;
        self.nudge_injected_this_turn = false;
        self.turn_has_code = false;
        self.turn_exploration_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_stall_when_writing_code() {
        let mut det = StallDetector::new();
        det.record_tool("read_file", true, false);
        det.record_tool("write_file", true, false);
        assert_eq!(det.end_turn(), StallLevel::Normal);
    }

    #[test]
    fn test_stall_after_consecutive_read_only() {
        let mut det = StallDetector::new();
        for _ in 0..STALL_THRESHOLD {
            det.record_tool("read_file", true, false);
            det.end_turn();
        }
        assert_eq!(det.stall_level(), StallLevel::Stalled);
        assert!(det.nudge_message().is_some());
    }

    #[test]
    fn test_code_resets_stall_counter() {
        let mut det = StallDetector::new();
        for _ in 0..STALL_THRESHOLD - 1 {
            det.record_tool("read_file", true, false);
            det.end_turn();
        }
        det.record_tool("write_file", true, false);
        assert_eq!(det.end_turn(), StallLevel::Normal);
    }

    #[test]
    fn test_critical_level() {
        let mut det = StallDetector::new();
        for _ in 0..MAX_EXPLORATION_TURNS {
            det.record_tool("read_file", true, false);
            det.end_turn();
        }
        assert_eq!(det.stall_level(), StallLevel::Critical);
    }

    #[test]
    fn test_bash_classification() {
        assert_eq!(StallDetector::classify_tool("bash"), ToolCategory::Shell);
        assert_eq!(
            StallDetector::classify_tool("read_file"),
            ToolCategory::Exploration
        );
        assert_eq!(
            StallDetector::classify_tool("write_file"),
            ToolCategory::Code
        );
    }

    #[test]
    fn test_nudge_injected_once_per_turn() {
        let mut det = StallDetector::new();
        for _ in 0..STALL_THRESHOLD {
            det.record_tool("read_file", true, false);
            det.end_turn();
        }
        assert!(det.try_mark_nudge());
        assert!(!det.try_mark_nudge()); // Already injected
    }

    #[test]
    fn test_failed_tools_not_counted() {
        let mut det = StallDetector::new();
        det.record_tool("read_file", false, false); // Failed — not counted
        assert_eq!(det.end_turn(), StallLevel::Normal);
    }
}
