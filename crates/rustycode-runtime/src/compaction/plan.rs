//! Compaction plan with progressive tightening logic.
//!
//! [`CompactionPlan`] holds the parameters that control how aggressively the
//! compaction pipeline reduces context. When a single pass does not free enough
//! tokens, the plan is "tightened" — tail turns shrink, tool output lines are
//! halved, the summary template degrades, and thinking blocks are dropped.

use rustycode_protocol::compaction::{HybridCompactionConfig, SummaryTemplate};

/// Parameters controlling the aggressiveness of a compaction pass.
///
/// Start from [`CompactionPlan::from_config`] and call [`tighten`](Self::tighten)
/// after each pass that fails to bring the token count below target.
#[derive(Debug, Clone)]
pub struct CompactionPlan {
    /// Turns to preserve verbatim (the "tail").
    ///
    /// A "turn" is one complete user -> assistant round trip, including any
    /// tool_use / tool_result pairs.
    pub tail_turns: usize,

    /// Maximum lines retained from each tool-result block after snipping.
    pub max_tool_output_lines: usize,

    /// Template granularity for the Summarize tier.
    pub summary_template: SummaryTemplate,

    /// Whether to preserve thinking blocks in the compacted output.
    pub include_thinking: bool,

    /// Upper bound on tightening passes before giving up.
    pub max_passes: usize,
}

impl CompactionPlan {
    /// Build an initial (least aggressive) plan from configuration.
    ///
    /// The plan starts at `SummaryTemplate::Full` with thinking blocks
    /// included and the tail-turn / tool-output-line limits read directly
    /// from the config defaults.
    pub fn from_config(config: &HybridCompactionConfig) -> Self {
        Self {
            tail_turns: config.initial_tail_turns,
            max_tool_output_lines: config.max_tool_output_lines,
            summary_template: SummaryTemplate::Full,
            include_thinking: true,
            max_passes: config.max_tightening_passes,
        }
    }

    /// Increase compaction aggressiveness for the next pass.
    ///
    /// - `tail_turns` decrements by 1 (saturating at 0).
    /// - `max_tool_output_lines` halves (floor of 10).
    /// - `summary_template` degrades to the next coarser level.
    /// - `include_thinking` is forced off.
    pub fn tighten(&mut self) {
        self.tail_turns = self.tail_turns.saturating_sub(1);
        self.max_tool_output_lines = (self.max_tool_output_lines / 2).max(10);
        self.summary_template = self.summary_template.degrade();
        self.include_thinking = false;
    }

    /// Numeric aggression level derived from the summary template.
    ///
    /// | Level | Template |
    /// |-------|----------|
    /// | 0     | Full     |
    /// | 1     | Compact  |
    /// | 2     | Minimal  |
    pub fn aggression_level(&self) -> usize {
        match self.summary_template {
            SummaryTemplate::Full => 0,
            SummaryTemplate::Compact => 1,
            SummaryTemplate::Minimal => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HybridCompactionConfig {
        HybridCompactionConfig::default()
    }

    #[test]
    fn initial_plan_from_config() {
        let plan = CompactionPlan::from_config(&default_config());
        assert_eq!(plan.tail_turns, 2);
        assert_eq!(plan.max_tool_output_lines, 50);
        assert_eq!(plan.summary_template, SummaryTemplate::Full);
        assert!(plan.include_thinking);
        assert_eq!(plan.max_passes, 3);
    }

    #[test]
    fn tighten_reduces_tail_turns() {
        let mut plan = CompactionPlan::from_config(&default_config());
        plan.tighten();
        assert_eq!(plan.tail_turns, 1);
    }

    #[test]
    fn tighten_halves_tool_output_lines() {
        let mut plan = CompactionPlan::from_config(&default_config());
        assert_eq!(plan.max_tool_output_lines, 50);
        plan.tighten();
        assert_eq!(plan.max_tool_output_lines, 25);
    }

    #[test]
    fn tighten_disables_thinking() {
        let mut plan = CompactionPlan::from_config(&default_config());
        assert!(plan.include_thinking);
        plan.tighten();
        assert!(!plan.include_thinking);
    }

    #[test]
    fn tighten_three_times_reaches_minimal() {
        let mut plan = CompactionPlan::from_config(&default_config());
        assert_eq!(plan.summary_template, SummaryTemplate::Full);
        assert_eq!(plan.tail_turns, 2);

        plan.tighten();
        assert_eq!(plan.summary_template, SummaryTemplate::Compact);
        assert_eq!(plan.tail_turns, 1);

        plan.tighten();
        assert_eq!(plan.summary_template, SummaryTemplate::Minimal);
        assert_eq!(plan.tail_turns, 0);

        // Third tighten: template is already Minimal (idempotent), tail stays at 0.
        plan.tighten();
        assert_eq!(plan.summary_template, SummaryTemplate::Minimal);
        assert_eq!(plan.tail_turns, 0);
    }

    #[test]
    fn tighten_floor_on_tool_output_lines() {
        let mut plan = CompactionPlan {
            max_tool_output_lines: 10,
            ..CompactionPlan::from_config(&default_config())
        };
        // 10 / 2 = 5, but floor is 10.
        plan.tighten();
        assert_eq!(
            plan.max_tool_output_lines, 10,
            "tool output lines should never drop below 10"
        );
    }

    #[test]
    fn aggression_level_tracks_template() {
        let mut plan = CompactionPlan::from_config(&default_config());
        assert_eq!(plan.aggression_level(), 0);

        plan.tighten();
        assert_eq!(plan.aggression_level(), 1);

        plan.tighten();
        assert_eq!(plan.aggression_level(), 2);
    }
}
