//! Re-exports from [`task_context`] for backward compatibility.
//!
//! This module re-exports the canonical `TaskContext`, `TaskPhase`, and related
//! types from `task_context`. Use `task_context` directly in new code.

pub use crate::task_context::{TaskComplexity, TaskConstraints, TaskContext, TaskPhase};

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_task_context_new() {
        let ctx = TaskContext::new("task-1".into(), "original request".into());
        assert_eq!(ctx.task_id, "task-1");
        assert_eq!(ctx.current_phase, TaskPhase::Planning);
        assert_eq!(ctx.current_tier, 2);
        assert_eq!(ctx.attempt_count, 0);
    }

    #[test]
    fn test_transition() {
        let mut ctx = TaskContext::new("task-1".into(), "request".into());
        ctx.transition_to(TaskPhase::Tier2Execution);
        assert_eq!(ctx.current_phase, TaskPhase::Tier2Execution);
    }

    #[test]
    fn test_escalate() {
        let mut ctx = TaskContext::new("task-1".into(), "request".into());
        ctx.attempt_count = 3;
        ctx.escalate();
        assert_eq!(ctx.current_tier, 3);
        assert_eq!(ctx.attempt_count, 0);
    }

    #[test]
    fn test_escalate_saturating() {
        let mut ctx = TaskContext::new("task-1".into(), "request".into());
        ctx.current_tier = 255;
        ctx.escalate();
        // Tiers cap at 5 (max tier); 255.saturating_add(1).min(5) == 5
        assert_eq!(ctx.current_tier, 5);
    }

    #[test]
    fn test_initial_phase_is_planning() {
        let ctx = TaskContext::new("t1".into(), "objective".into());
        assert_eq!(ctx.current_phase, TaskPhase::Planning);
    }

    #[test]
    fn test_initial_tier_is_2() {
        let ctx = TaskContext::new("t1".into(), "objective".into());
        assert_eq!(ctx.current_tier, 2);
    }

    #[test]
    fn test_initial_attempt_count_is_zero() {
        let ctx = TaskContext::new("t1".into(), "objective".into());
        assert_eq!(ctx.attempt_count, 0);
    }

    #[test]
    fn test_multiple_phase_transitions() {
        let mut ctx = TaskContext::new("t1".into(), "obj".into());
        ctx.transition_to(TaskPhase::Tier2Execution);
        assert_eq!(ctx.current_phase, TaskPhase::Tier2Execution);
        ctx.transition_to(TaskPhase::Tier3Review);
        assert_eq!(ctx.current_phase, TaskPhase::Tier3Review);
        ctx.transition_to(TaskPhase::Completed);
        assert_eq!(ctx.current_phase, TaskPhase::Completed);
    }

    #[test]
    fn test_escalate_resets_attempt_count() {
        let mut ctx = TaskContext::new("t1".into(), "obj".into());
        ctx.attempt_count = 5;
        ctx.escalate();
        assert_eq!(ctx.attempt_count, 0);
        assert_eq!(ctx.current_tier, 3);
    }

    #[test]
    fn test_escalate_from_tier_3() {
        let mut ctx = TaskContext::new("t1".into(), "obj".into());
        ctx.current_tier = 3;
        ctx.attempt_count = 2;
        ctx.escalate();
        assert_eq!(ctx.current_tier, 4);
        assert_eq!(ctx.attempt_count, 0);
    }

    #[test]
    fn test_task_context_preserves_objective() {
        let ctx = TaskContext::new("id-42".into(), "build the thing".into());
        assert_eq!(ctx.task_id, "id-42");
        assert_eq!(ctx.original_request, "build the thing");
    }
}
