//! Compaction shared types for the hybrid compaction pipeline.
//!
//! These types are used by both `rustycode-runtime` (which performs compaction)
//! and `rustycode-tui` (which displays compaction state and results).

use crate::message::Message;
use serde::{Deserialize, Serialize};

// Configuration

/// Configuration for the hybrid compaction pipeline.
///
/// Controls when compaction triggers, how aggressively it reduces context,
/// and how many passes it may take.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridCompactionConfig {
    /// Fraction of the context window that triggers compaction (0.0 – 1.0).
    pub trigger_threshold_pct: f64,

    /// Target fraction after compaction (0.0 – 1.0).
    pub target_pct: f64,

    /// Maximum number of tightening passes when the first pass undershoots.
    pub max_tightening_passes: usize,

    /// Number of recent turns preserved verbatim (the "Tail").
    pub initial_tail_turns: usize,

    /// Maximum lines kept from each tool-result block during the Snip tier.
    pub max_tool_output_lines: usize,

    /// Tokens of headroom reserved below the target so the agent can keep
    /// working without immediately re-triggering compaction.
    pub compaction_buffer_tokens: usize,
}

impl Default for HybridCompactionConfig {
    fn default() -> Self {
        Self {
            trigger_threshold_pct: 0.78,
            target_pct: 0.50,
            max_tightening_passes: 3,
            initial_tail_turns: 2,
            max_tool_output_lines: 50,
            compaction_buffer_tokens: 6000,
        }
    }
}

// Summary progression

/// Template granularity for the Summarize tier.
///
/// Each level degrades to the next coarser level when token budget is still
/// exceeded after summarisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryTemplate {
    /// Full summary with all sections.
    Full,
    /// Compact summary with fewer sections.
    Compact,
    /// Minimal summary — only the essentials.
    Minimal,
}

impl SummaryTemplate {
    /// Degrade to the next coarser template.
    pub fn degrade(&self) -> Self {
        match self {
            Self::Full => Self::Compact,
            Self::Compact => Self::Minimal,
            Self::Minimal => Self::Minimal,
        }
    }

    /// Number of sections rendered at this granularity level.
    pub fn section_count(&self) -> usize {
        match self {
            Self::Full => 9,
            Self::Compact => 5,
            Self::Minimal => 2,
        }
    }
}

// Tier record

/// Records which compaction tier(s) were applied during a pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionTierUsed {
    /// Snip tier: tool results were trimmed.
    Snip {
        /// Number of tool-result blocks that were trimmed.
        tool_results_trimmed: usize,
    },

    /// Summarize tier: older turns were replaced with an LLM-generated summary.
    Summarize {
        /// Template granularity used.
        template: SummaryTemplate,
        /// Number of tail turns preserved verbatim.
        tail_preserved: usize,
    },

    /// Truncate tier: turns were simply dropped from the head.
    Truncate {
        /// How many turns were kept.
        turns_kept: usize,
    },

    /// Emergency compaction: last-resort drop of everything except the system
    /// prompt and the most recent turn.
    Emergency,
}

// Result

/// Result of a single compaction pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    /// Summary messages injected in place of the compacted turns (may be empty
    /// when only Snip or Truncate was used).
    pub summary_messages: Vec<Message>,

    /// Turns preserved verbatim (the "Tail").
    pub preserved_turns: Vec<Message>,

    /// Rendered context block (for display / debugging).
    pub context_block_render: String,

    /// Token count before compaction.
    pub tokens_before: usize,

    /// Token count after compaction.
    pub tokens_after: usize,

    /// Ordered list of tiers applied during this pass.
    pub tiers_used: Vec<CompactionTierUsed>,
}

// Error

/// Non-fatal errors that can occur during compaction.
///
/// These are never fatal — the caller should log them and either retry with a
/// degraded strategy or proceed with the original context unchanged.
#[derive(Debug, Clone, thiserror::Error)]
pub enum CompactionError {
    #[error("summarization failed: {0}")]
    SummarizationFailed(String),

    #[error("budget exceeded after {passes} passes ({tokens} tokens, budget {budget})")]
    BudgetExceeded {
        passes: usize,
        tokens: usize,
        budget: usize,
    },

    #[error("context block render failed: {0}")]
    ContextBlockRender(String),

    #[error("token counting failed: {0}")]
    TokenCounting(String),

    #[error("compaction already in progress")]
    AlreadyCompacting,
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sane_values() {
        let cfg = HybridCompactionConfig::default();
        assert!((cfg.trigger_threshold_pct - 0.78).abs() < f64::EPSILON);
        assert!((cfg.target_pct - 0.50).abs() < f64::EPSILON);
        assert_eq!(cfg.max_tightening_passes, 3);
        assert_eq!(cfg.initial_tail_turns, 2);
        assert_eq!(cfg.max_tool_output_lines, 50);
        assert_eq!(cfg.compaction_buffer_tokens, 6000);
    }

    #[test]
    fn summary_template_degrades_full_to_compact() {
        assert_eq!(SummaryTemplate::Full.degrade(), SummaryTemplate::Compact);
    }

    #[test]
    fn summary_template_degrades_compact_to_minimal() {
        assert_eq!(SummaryTemplate::Compact.degrade(), SummaryTemplate::Minimal);
    }

    #[test]
    fn summary_template_minimal_is_idempotent() {
        assert_eq!(SummaryTemplate::Minimal.degrade(), SummaryTemplate::Minimal);
    }

    #[test]
    fn summary_template_section_counts() {
        assert_eq!(SummaryTemplate::Full.section_count(), 9);
        assert_eq!(SummaryTemplate::Compact.section_count(), 5);
        assert_eq!(SummaryTemplate::Minimal.section_count(), 2);
    }

    #[test]
    fn config_serializes_to_json() {
        let cfg = HybridCompactionConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        assert!(
            json.contains("trigger_threshold_pct"),
            "json should contain trigger_threshold_pct field: {json}"
        );
        assert!(
            json.contains("0.78"),
            "json should contain default value 0.78: {json}"
        );
    }

    #[test]
    fn config_round_trips_through_json() {
        let cfg = HybridCompactionConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let decoded: HybridCompactionConfig = serde_json::from_str(&json).expect("deserialize");

        assert!((decoded.trigger_threshold_pct - cfg.trigger_threshold_pct).abs() < f64::EPSILON);
        assert!((decoded.target_pct - cfg.target_pct).abs() < f64::EPSILON);
        assert_eq!(decoded.max_tightening_passes, cfg.max_tightening_passes);
        assert_eq!(decoded.initial_tail_turns, cfg.initial_tail_turns);
        assert_eq!(decoded.max_tool_output_lines, cfg.max_tool_output_lines);
        assert_eq!(
            decoded.compaction_buffer_tokens,
            cfg.compaction_buffer_tokens
        );
    }
}
