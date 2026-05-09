use crate::error::Result;
use crate::execution_trace::ExecutionTrace;
use crate::state_machine::TaskContext;
use crate::types::Step;

#[derive(Default)]
pub struct PlanRefiner {}

impl PlanRefiner {
    pub const fn new() -> Self {
        Self {}
    }

    #[allow(clippy::missing_const_for_fn)]
    pub fn analyze_and_suggest(
        &self,
        _trace: &ExecutionTrace,
        _context: &TaskContext,
    ) -> Result<RefinementResult> {
        Ok(RefinementResult {
            has_refinements: false,
            suggestions: Vec::new(),
        })
    }
}

pub struct RefinementResult {
    pub has_refinements: bool,
    pub suggestions: Vec<Step>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::execution_trace::TraceEntry;
    use crate::state_machine::TaskPhase;

    #[test]
    fn test_plan_refiner_returns_no_refinements() {
        let refiner = PlanRefiner::new();
        let trace = ExecutionTrace::new("task-1".into());
        let ctx = TaskContext::new("task-1".into(), "test task".into());
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn test_plan_refiner_default() {
        let refiner = PlanRefiner::default();
        let trace = ExecutionTrace::new("task-2".into());
        let ctx = TaskContext::new("task-2".into(), "test".into());
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
    }

    #[test]
    fn test_plan_refiner_with_non_empty_trace() {
        let refiner = PlanRefiner::new();
        let mut trace = ExecutionTrace::new("task-3".into());
        trace.append(TraceEntry::new_success(
            "step-1".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({"cmd": "echo hello"}),
            "hello".into(),
            Some(0),
            0.001,
        ));
        let ctx = TaskContext::new("task-3".into(), "run echo".into());
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
    }

    #[test]
    fn test_plan_refiner_with_advanced_context() {
        let refiner = PlanRefiner::new();
        let trace = ExecutionTrace::new("task-4".into());
        let mut ctx = TaskContext::new("task-4".into(), "complex task".into());
        ctx.advance_phase(TaskPhase::Tier3Review);
        ctx.add_cost(0.5);
        ctx.add_tokens(1000);
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
    }

    #[test]
    fn test_plan_refiner_with_escalated_context() {
        let refiner = PlanRefiner::new();
        let trace = ExecutionTrace::new("task-5".into());
        let mut ctx = TaskContext::new("task-5".into(), "escalated task".into());
        ctx.escalate();
        assert_eq!(ctx.current_tier, 3);
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
    }

    #[test]
    fn test_plan_refiner_multiple_calls_independent() {
        let refiner = PlanRefiner::new();
        let trace = ExecutionTrace::new("task-6".into());
        let ctx1 = TaskContext::new("task-6".into(), "first".into());
        let ctx2 = TaskContext::new("task-7".into(), "second".into());

        let result1 = refiner.analyze_and_suggest(&trace, &ctx1).unwrap();
        let result2 = refiner.analyze_and_suggest(&trace, &ctx2).unwrap();

        assert!(!result1.has_refinements);
        assert!(!result2.has_refinements);
        assert!(result1.suggestions.is_empty());
        assert!(result2.suggestions.is_empty());
    }

    #[test]
    fn test_refinement_result_fields() {
        let result = RefinementResult {
            has_refinements: false,
            suggestions: vec![],
        };
        assert!(!result.has_refinements);
        assert!(result.suggestions.is_empty());
    }

    #[test]
    fn test_plan_refiner_with_failed_step_in_trace() {
        let refiner = PlanRefiner::new();
        let mut trace = ExecutionTrace::new("task-7".into());
        trace.append(TraceEntry::new_success(
            "step-1".into(),
            0,
            2,
            "Bash".into(),
            serde_json::json!({}),
            "output".into(),
            Some(1),
            0.001,
        ));
        let ctx = TaskContext::new("task-7".into(), "failing task".into());
        let result = refiner.analyze_and_suggest(&trace, &ctx).unwrap();
        assert!(!result.has_refinements);
    }
}
