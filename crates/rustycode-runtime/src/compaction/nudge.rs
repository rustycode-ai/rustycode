//! Turn-based nudge system for agent guidance.
//!
//! Inspired by SWE-bench experiment learnings, this module provides
//! turn-specific guidance messages that are injected alongside normal LLM
//! requests. Each [`NudgeProfile`] maps to a curated set of [`NudgeEntry`]
//! values that trigger at specific turn thresholds.
//!
//! The output format mirrors the piggyback compaction system's
//! `[Context Management]` prefix pattern: nudges are wrapped in
//! `[Agent Guidance] {message}` for consistent injection into the system
//! prompt.

/// A pre-built nudge profile for different task types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeProfile {
    /// Guidance for bug-fix tasks: diagnosis, targeted edit, verify cycle.
    BugFix,
    /// Guidance for feature implementation: plan, execute, verify, scope check.
    Feature,
    /// Guidance for exploration tasks: focus, deepen, wrap up.
    Exploration,
    /// Generic progress checks suitable for any task.
    Generic,
}

/// A single nudge entry mapping a turn threshold to guidance text.
#[derive(Debug, Clone)]
pub struct NudgeEntry {
    /// The nudge activates when the current turn is >= this threshold.
    pub turn_threshold: u32,
    /// The guidance message to inject.
    pub message: String,
}

/// Engine that provides turn-specific guidance messages.
///
/// Create with [`NudgeEngine::new`] for a built-in profile, or
/// [`NudgeEngine::from_entries`] for custom nudges. Call
/// [`NudgeEngine::nudge_suffix`] each turn to get the formatted guidance
/// string (or `None` if no nudge applies).
#[derive(Debug, Clone)]
pub struct NudgeEngine {
    /// Entries sorted by `turn_threshold` ascending.
    entries: Vec<NudgeEntry>,
}

impl NudgeEngine {
    /// Create an engine with built-in nudges for the given profile.
    pub fn new(profile: NudgeProfile) -> Self {
        let entries = match profile {
            NudgeProfile::BugFix => vec![
                NudgeEntry {
                    turn_threshold: 2,
                    message: "DIAGNOSIS CHECK: State the root cause in one sentence \
                        before editing. Format: the bug is in FILE.FUNCTION because \
                        REASON. If you cannot state it clearly, re-trace from the \
                        error."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 4,
                    message: "EDIT NOW: Make ONE targeted edit to ONE source file. \
                        The fix should be 1-3 lines. After editing, run the failing \
                        test IMMEDIATELY."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 6,
                    message: "If test passed: STOP. If test failed: re-read the NEW \
                        error carefully. The error tells you exactly what to fix \u{2014} \
                        do not guess, read it."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 10,
                    message: "RE-DIAGNOSE: If the test still fails after 3 edits, \
                        your diagnosis is wrong. Return to Phase 1 with BROADER \
                        search patterns. Check: is the function inherited? Imported \
                        from elsewhere?"
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 16,
                    message: "ALTERNATIVE APPROACH: Try the opposite of what you \
                        have been doing. Re-read the test error from scratch."
                        .to_string(),
                },
            ],
            NudgeProfile::Feature => vec![
                NudgeEntry {
                    turn_threshold: 2,
                    message: "PLAN: State your implementation approach in 2-3 \
                        sentences before writing code."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 5,
                    message: "EXECUTE: Start implementing now. Use edit for targeted \
                        changes, not write for full rewrites."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 8,
                    message: "VERIFY: Run the build and tests. Fix any issues before \
                        continuing."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 15,
                    message: "SCOPE CHECK: Are you over-engineering this? Prefer the \
                        simplest solution that works."
                        .to_string(),
                },
            ],
            NudgeProfile::Exploration => vec![
                NudgeEntry {
                    turn_threshold: 3,
                    message: "FOCUS: Summarize what you have learned so far. What is \
                        still unclear?"
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 8,
                    message: "DEEPEN: If you haven't found the answer, try broader \
                        search patterns or different file paths."
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 15,
                    message: "WRAP UP: Synthesize your findings. Provide a clear \
                        answer based on what you discovered."
                        .to_string(),
                },
            ],
            NudgeProfile::Generic => vec![
                NudgeEntry {
                    turn_threshold: 3,
                    message: "PROGRESS CHECK: What have you accomplished? What \
                        remains?"
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 8,
                    message: "If stuck, re-read the original request. Are you solving \
                        the right problem?"
                        .to_string(),
                },
                NudgeEntry {
                    turn_threshold: 15,
                    message: "Consider whether you need to ask the user for \
                        clarification."
                        .to_string(),
                },
            ],
        };

        Self { entries }
    }

    /// Create an engine from custom entries.
    ///
    /// Entries are sorted by `turn_threshold` ascending so that
    /// [`NudgeEngine::get_nudge`] can efficiently find the highest matching
    /// threshold.
    pub fn from_entries(mut entries: Vec<NudgeEntry>) -> Self {
        entries.sort_by_key(|e| e.turn_threshold);
        Self { entries }
    }

    /// Return the guidance message for the highest threshold <= `turn`.
    ///
    /// Returns `None` if no threshold is <= `turn` (e.g. turn 0 with all
    /// thresholds >= 2).
    pub fn get_nudge(&self, turn: u32) -> Option<&str> {
        // Entries are sorted ascending by threshold. Find the last entry
        // whose threshold is <= turn.
        let idx = self
            .entries
            .iter()
            .rposition(|e| e.turn_threshold <= turn)?;
        Some(&self.entries[idx].message)
    }

    /// Return the nudge wrapped in `[Agent Guidance] {message}` format.
    ///
    /// This follows the same injection pattern as the piggyback system's
    /// `[Context Management]` prefix. Returns `None` if no nudge applies
    /// for the given turn.
    pub fn nudge_suffix(&self, turn: u32) -> Option<String> {
        self.get_nudge(turn)
            .map(|msg| format!("[Agent Guidance] {msg}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_fix_profile_has_nudges() {
        let engine = NudgeEngine::new(NudgeProfile::BugFix);
        assert!(
            !engine.entries.is_empty(),
            "BugFix profile should have nudges"
        );
        // Should have 5 built-in entries.
        assert_eq!(engine.entries.len(), 5);
    }

    #[test]
    fn get_nudge_returns_highest_threshold() {
        let engine = NudgeEngine::new(NudgeProfile::BugFix);

        // Turn 5 should get turn 4's nudge, not turn 2's.
        let nudge = engine
            .get_nudge(5)
            .expect("should return a nudge for turn 5");
        assert!(
            nudge.contains("EDIT NOW"),
            "turn 5 should get the turn-4 EDIT NOW nudge, got: {nudge}"
        );
        assert!(
            !nudge.contains("DIAGNOSIS CHECK"),
            "turn 5 should NOT get the turn-2 DIAGNOSIS CHECK nudge"
        );
    }

    #[test]
    fn get_nudge_returns_none_below_all_thresholds() {
        let engine = NudgeEngine::new(NudgeProfile::BugFix);

        // Turn 0 should return None since the lowest threshold is 2.
        assert!(
            engine.get_nudge(0).is_none(),
            "turn 0 should return None when all thresholds are >= 2"
        );
        assert!(
            engine.get_nudge(1).is_none(),
            "turn 1 should return None when all thresholds are >= 2"
        );
    }

    #[test]
    fn nudge_suffix_formats_correctly() {
        let engine = NudgeEngine::new(NudgeProfile::BugFix);

        let suffix = engine
            .nudge_suffix(2)
            .expect("should return suffix for turn 2");
        assert!(
            suffix.starts_with("[Agent Guidance]"),
            "suffix should start with [Agent Guidance] prefix, got: {suffix}"
        );
        assert!(
            suffix.contains("DIAGNOSIS CHECK"),
            "suffix should contain the nudge message content"
        );

        // Below all thresholds should return None.
        assert!(
            engine.nudge_suffix(0).is_none(),
            "nudge_suffix should return None below all thresholds"
        );
    }

    #[test]
    fn from_entries_custom_profile() {
        let entries = vec![
            NudgeEntry {
                turn_threshold: 1,
                message: "Start".to_string(),
            },
            NudgeEntry {
                turn_threshold: 5,
                message: "Middle".to_string(),
            },
            NudgeEntry {
                turn_threshold: 10,
                message: "End".to_string(),
            },
        ];

        let engine = NudgeEngine::from_entries(entries);
        assert_eq!(engine.entries.len(), 3);

        assert_eq!(engine.get_nudge(1), Some("Start"));
        assert_eq!(engine.get_nudge(3), Some("Start"));
        assert_eq!(engine.get_nudge(5), Some("Middle"));
        assert_eq!(engine.get_nudge(9), Some("Middle"));
        assert_eq!(engine.get_nudge(10), Some("End"));
        assert_eq!(engine.get_nudge(99), Some("End"));
        assert_eq!(engine.get_nudge(0), None);
    }

    #[test]
    fn from_entries_sorts_by_threshold() {
        let entries = vec![
            NudgeEntry {
                turn_threshold: 10,
                message: "Last".to_string(),
            },
            NudgeEntry {
                turn_threshold: 2,
                message: "First".to_string(),
            },
            NudgeEntry {
                turn_threshold: 5,
                message: "Second".to_string(),
            },
        ];

        let engine = NudgeEngine::from_entries(entries);

        // Verify sorted order.
        assert_eq!(engine.entries[0].turn_threshold, 2);
        assert_eq!(engine.entries[1].turn_threshold, 5);
        assert_eq!(engine.entries[2].turn_threshold, 10);

        // Verify lookup still works correctly.
        assert_eq!(engine.get_nudge(3), Some("First"));
        assert_eq!(engine.get_nudge(7), Some("Second"));
        assert_eq!(engine.get_nudge(15), Some("Last"));
    }

    #[test]
    fn all_profiles_have_entries() {
        let profiles = [
            NudgeProfile::BugFix,
            NudgeProfile::Feature,
            NudgeProfile::Exploration,
            NudgeProfile::Generic,
        ];

        for profile in profiles {
            let engine = NudgeEngine::new(profile);
            assert!(
                !engine.entries.is_empty(),
                "{profile:?} profile should have at least one nudge entry"
            );

            // Every profile should produce nudges at turn 20 (well past all
            // thresholds).
            assert!(
                engine.get_nudge(20).is_some(),
                "{profile:?} profile should return a nudge at turn 20"
            );
        }
    }
}
