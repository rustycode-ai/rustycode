//! ScalpelPhase — targeted surgical fixes for specific Judge failures.
//!
//! The Scalpel runs after Judge identifies specific failures (compile errors, type errors).
//! It makes minimal changes to fix only those failures, without redesign or scope creep.
//!
//! # Constraints
//! - Minimal: one precise edit per failure
//! - No redesign: cannot change declared module structure
//! - No new deps: must use only existing imports
//! - If a failure requires redesign, Scalpel sets done: false and escalates

use std::path::{Path, PathBuf};

/// The scalpel phase — targeted fixup after Judge failures.
pub struct ScalpelPhase {
    project_root: PathBuf,
}

impl ScalpelPhase {
    /// Create a new ScalpelPhase for the given project root.
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
        }
    }

    /// The project root this phase operates on.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Build the system prompt for the Scalpel LLM call.
    pub fn system_prompt(&self, judge_failures: &str) -> String {
        super::executor::prompts::scalpel_system_prompt(judge_failures)
    }

    /// Determine whether a set of failures is scalpel-appropriate
    /// (targeted fixups) or requires full Builder redesign.
    ///
    /// Heuristic: if all failures are in declared modules and involve
    /// compile errors (not logic errors), Scalpel can handle them.
    pub fn is_scalpel_appropriate(failures: &[String]) -> bool {
        // Scalpel handles compile errors and type errors.
        // Logic errors or test semantics need Builder redesign.
        let redesign_signals = [
            "logic",
            "approach",
            "redesign",
            "wrong output",
            "wrong result",
        ];
        !failures.iter().any(|f| {
            let lower = f.to_lowercase();
            redesign_signals.iter().any(|s| lower.contains(s))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalpel_phase_is_constructible() {
        let phase = ScalpelPhase::new("/tmp");
        assert_eq!(phase.project_root(), Path::new("/tmp"));
    }

    #[test]
    fn compile_errors_are_scalpel_appropriate() {
        let failures = vec![
            "error[E0308]: mismatched types at src/lib.rs:42".to_string(),
            "error[E0425]: cannot find value `foo` in this scope".to_string(),
        ];
        assert!(ScalpelPhase::is_scalpel_appropriate(&failures));
    }

    #[test]
    fn logic_errors_are_not_scalpel_appropriate() {
        let failures = vec!["wrong output: expected 42 got 0".to_string()];
        assert!(!ScalpelPhase::is_scalpel_appropriate(&failures));
    }

    #[test]
    fn approach_failures_need_builder() {
        let failures = vec!["redesign needed: current approach won't scale".to_string()];
        assert!(!ScalpelPhase::is_scalpel_appropriate(&failures));
    }

    #[test]
    fn missing_import_is_scalpel_appropriate() {
        let failures = vec!["error: cannot find value `Result` in this scope".to_string()];
        assert!(ScalpelPhase::is_scalpel_appropriate(&failures));
    }

    #[test]
    fn scalpel_generates_prompt() {
        let phase = ScalpelPhase::new("/tmp");
        let prompt = phase.system_prompt("compilation failed on line 42");
        assert!(prompt.contains("Scalpel"));
        assert!(prompt.contains("minimal"));
    }
}
