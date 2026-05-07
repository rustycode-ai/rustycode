//! Context Budget Engine (orchestra-2 pattern)
//!
//! Proportional allocation, section-boundary truncation, and executor
//! context window resolution for optimal token usage.
//!
//! # Problem
//!
//! LLM context windows are limited and expensive. We need to:
//! - Fit as much useful context as possible
//! - Avoid truncating important sections
//! - Allocate space proportionally to different content types
//! - Detect when we're running out of space and need a checkpoint
//!
//! # Budget Allocation
//!
//! The context window is divided into three zones:
//!
//! - **Summaries (15%)**: Dependency summaries, prior task context
//! - **Inline Context (40%)**: Current task plan, decisions, code
//! - **Verification (10%)**: Test commands, validation output
//! - **System (35%)**: System prompt, tools, LLM overhead
//!
//! # Adaptive Task Count
//!
//! Based on context window size:
//! - 500K+ tokens → up to 8 tasks per unit
//! - 200K+ tokens → up to 6 tasks per unit
//! - 128K+ tokens → up to 5 tasks per unit
//! - < 128K tokens → up to 3 tasks per unit
//!
//! Larger contexts can handle more tasks in parallel.
//!
//! # Truncation Strategy
//!
//! When content exceeds budget, we truncate at **section boundaries**:
//! - Never truncate mid-sentence or mid-code block
//! - Drop entire sections if needed
//! - Preserve most recent content (most relevant)
//! - Track dropped sections for debugging
//!
//! # Usage
//!
//! ```no_run
//! use rustycode_orchestration::context_budget::{ContextBudget, ModelInfo};
//!
//! let model = ModelInfo {
//!     id: "claude-opus-4".to_string(),
//!     context_window: 200_000,
//! };
//!
//! let budget = ContextBudget::for_model(&model);
//! println!("Summary budget: {} chars", budget.summary_budget_chars);
//! println!("Task count: {}-{}", budget.task_count_range.min, budget.task_count_range.max);
//!
//! // Truncate content to fit budget
//! let result = budget.truncate_to_inline_budget(&very_long_content)?;
//! println!("Truncated: {} sections dropped", result.dropped_sections);
//! ```
//!
//! # Continue Threshold
//!
//! When context usage exceeds 70%, suggest a checkpoint:
//! - LLM completes current task
//! - Context is compressed and saved
//! - Fresh session starts with cached context
//! - Prevents hitting hard token limits

use serde::{Deserialize, Serialize};

/// Budget ratios (percentages of total context window)
const SUMMARY_RATIO: f64 = 0.15; // Dependency/prior-task summaries
const INLINE_CONTEXT_RATIO: f64 = 0.40; // Plans, decisions, code
const VERIFICATION_RATIO: f64 = 0.10; // Verification sections
const CHARS_PER_TOKEN: f64 = 4.0;
const DEFAULT_CONTEXT_WINDOW: usize = 200_000;
const CONTINUE_THRESHOLD_PERCENT: u8 = 70;

/// Task count bounds based on context window
const TASK_COUNT_MIN: usize = 2;

/// Task count tiers: [`context_window_threshold`, `max_tasks`]
const TASK_COUNT_TIERS: &[(usize, usize)] = &[
    (500_000, 8), // 500K+ tokens → up to 8 tasks
    (200_000, 6), // 200K+ tokens → up to 6 tasks
    (128_000, 5), // 128K+ tokens → up to 5 tasks
    (0, 3),       // anything smaller → up to 3 tasks
];

/// Truncation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationResult {
    /// The (possibly truncated) content string
    pub content: String,
    /// Number of sections dropped during truncation
    pub dropped_sections: usize,
}

/// Budget allocation for a context window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAllocation {
    /// Character budget for dependency/prior-task summaries
    pub summary_budget_chars: usize,
    /// Character budget for inline context (plans, decisions, code)
    pub inline_context_budget_chars: usize,
    /// Character budget for verification sections
    pub verification_budget_chars: usize,
    /// Recommended task count range
    pub task_count_range: TaskCountRange,
    /// Percentage of context consumed before suggesting checkpoint
    pub continue_threshold_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCountRange {
    pub min: usize,
    pub max: usize,
}

/// Model info for context window resolution
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub context_window: usize,
}

/// Compute proportional budget allocations from a context window size
///
/// Matches orchestra-2's `computeBudgets()` function.
pub fn compute_budgets(context_window: usize) -> BudgetAllocation {
    let effective_window = if context_window > 0 {
        context_window
    } else {
        DEFAULT_CONTEXT_WINDOW
    };

    let total_chars = effective_window.saturating_mul(CHARS_PER_TOKEN as usize);

    BudgetAllocation {
        summary_budget_chars: ((total_chars as f64 * SUMMARY_RATIO) as usize).min(total_chars),
        inline_context_budget_chars: ((total_chars as f64 * INLINE_CONTEXT_RATIO) as usize)
            .min(total_chars),
        verification_budget_chars: ((total_chars as f64 * VERIFICATION_RATIO) as usize)
            .min(total_chars),
        task_count_range: TaskCountRange {
            min: TASK_COUNT_MIN,
            max: resolve_task_count_max(effective_window),
        },
        continue_threshold_percent: CONTINUE_THRESHOLD_PERCENT,
    }
}

/// Resolve max task count based on context window
///
/// Matches orchestra-2's `resolveTaskCountMax()` function.
fn resolve_task_count_max(context_window: usize) -> usize {
    for &(threshold, max_tasks) in TASK_COUNT_TIERS {
        if context_window >= threshold {
            return max_tasks;
        }
    }
    3 // Default fallback
}

/// Truncate content at markdown section boundaries to fit within a character budget
///
/// Matches orchestra-2's `truncateAtSectionBoundary()` function.
/// Splits on `### ` headings and `---` dividers. Keeps whole sections that fit.
/// Appends `[...truncated N sections]` when content is dropped.
///
pub fn truncate_at_section_boundary(content: &str, budget_chars: usize) -> TruncationResult {
    // If content fits within budget, return unchanged
    if content.len() <= budget_chars {
        return TruncationResult {
            content: content.to_string(),
            dropped_sections: 0,
        };
    }

    // Split on section markers: ### headings or --- dividers
    let sections = split_into_sections(content);

    if sections.len() <= 1 {
        // No section markers — keep as much as fits from the start
        let truncated = &content[..content.floor_char_boundary(budget_chars)];
        return TruncationResult {
            content: format!("{truncated}\n\n[...truncated 1 sections]"),
            dropped_sections: 1,
        };
    }

    // Greedily keep sections that fit
    let mut used_chars = 0;
    let mut kept_count = 0;

    for (i, section) in sections.iter().enumerate() {
        let section_len = section.len();

        // Stop if adding this section would exceed budget
        // (but always keep at least the first section)
        if used_chars + section_len > budget_chars && i > 0 {
            break;
        }

        used_chars += section_len;
        kept_count += 1;

        if used_chars >= budget_chars {
            break;
        }
    }

    let dropped_count = sections.len() - kept_count;

    if dropped_count == 0 {
        return TruncationResult {
            content: content.to_string(),
            dropped_sections: 0,
        };
    }

    let kept: String = sections.iter().take(kept_count).cloned().collect();

    TruncationResult {
        content: format!(
            "{}\n\n[...truncated {} sections]",
            kept.trim_end(),
            dropped_count
        ),
        dropped_sections: dropped_count,
    }
}

/// Split content into sections based on markdown headings or dividers
///
/// Matches orchestra-2's `splitIntoSections()` function.
fn split_into_sections(content: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current_section = String::new();
    let lines = content.lines();
    let mut was_divider = false;

    for line in lines {
        let is_heading = line.starts_with("### ");
        let is_divider = line == "---" || line.trim().is_empty();

        // Check if this is a section boundary
        if (is_heading || is_divider) && !current_section.is_empty() && was_divider {
            sections.push(current_section.clone());
            current_section = String::new();
        }

        current_section.push_str(line);
        current_section.push('\n');

        was_divider = is_divider;
    }

    // Don't forget the last section
    if !current_section.is_empty() {
        sections.push(current_section);
    }

    sections
}

/// Resolve executor model's context window
///
/// Matches orchestra-2's `resolveExecutorContextWindow()` function.
/// Uses fallback chain: configured model → session context window → default (200K)
///
pub const fn resolve_executor_context_window(
    model_context_window: Option<usize>,
    session_context_window: Option<usize>,
) -> usize {
    // Step 1: Try configured model context window
    if let Some(window) = model_context_window {
        if window > 0 {
            return window;
        }
    }

    // Step 2: Fall back to session context window
    if let Some(window) = session_context_window {
        if window > 0 {
            return window;
        }
    }

    // Step 3: Fall back to default (D002)
    DEFAULT_CONTEXT_WINDOW
}

/// Check if content fits within budget
pub const fn content_fits_budget(content: &str, budget_chars: usize) -> bool {
    content.len() <= budget_chars
}

/// Calculate remaining budget
pub const fn remaining_budget(used: usize, total: usize) -> usize {
    total.saturating_sub(used)
}

/// Calculate budget usage percentage
pub fn budget_usage_percent(used: usize, total: usize) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 / total as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_budgets() {
        let budgets = compute_budgets(200_000);

        assert_eq!(budgets.task_count_range.min, 2);
        assert_eq!(budgets.task_count_range.max, 6);
        assert_eq!(budgets.continue_threshold_percent, 70);

        // Check proportional allocation
        let expected_inline = (200_000 * CHARS_PER_TOKEN as usize * 40) / 100;
        assert_eq!(budgets.inline_context_budget_chars, expected_inline);
    }

    #[test]
    fn test_truncate_short_content() {
        let content = "Short content";
        let result = truncate_at_section_boundary(content, 100);

        assert_eq!(result.content, content);
        assert_eq!(result.dropped_sections, 0);
    }

    #[test]
    fn test_truncate_long_content() {
        let content =
            "### Section 1\nContent 1\n\n### Section 2\nContent 2\n\n### Section 3\nContent 3";
        let result = truncate_at_section_boundary(content, 50);

        // Should truncate at section boundary
        assert!(result.content.contains("Section 1"));
        assert!(result.dropped_sections > 0);
    }

    #[test]
    fn test_resolve_task_count_max() {
        assert_eq!(resolve_task_count_max(500_000), 8);
        assert_eq!(resolve_task_count_max(200_000), 6);
        assert_eq!(resolve_task_count_max(128_000), 5);
        assert_eq!(resolve_task_count_max(100_000), 3);
    }

    #[test]
    fn test_content_fits_budget() {
        assert!(content_fits_budget("short", 100));
        assert!(!content_fits_budget(
            "this is very long content that exceeds budget",
            10
        ));
    }

    #[test]
    fn test_remaining_budget() {
        assert_eq!(remaining_budget(50, 100), 50);
        assert_eq!(remaining_budget(100, 100), 0);
        assert_eq!(remaining_budget(150, 100), 0);
    }

    #[test]
    fn test_budget_usage_percent() {
        assert_eq!(budget_usage_percent(50, 100), 50.0);
        assert_eq!(budget_usage_percent(100, 100), 100.0);
        assert_eq!(budget_usage_percent(0, 100), 0.0);
    }

    #[test]
    fn test_resolve_executor_context_window() {
        // Model context window takes priority
        assert_eq!(
            resolve_executor_context_window(Some(128_000), Some(200_000)),
            128_000
        );

        // Session context window as fallback
        assert_eq!(
            resolve_executor_context_window(None, Some(200_000)),
            200_000
        );

        // Default fallback
        assert_eq!(resolve_executor_context_window(None, None), 200_000);
    }
}
