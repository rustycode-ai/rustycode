#![allow(
    unknown_lints,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::map_unwrap_or,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::redundant_clone,
    clippy::significant_drop_tightening,
    clippy::ptr_arg,
    clippy::format_in_format_args,
    clippy::let_and_return,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::manual_let_else,
    clippy::semicolon_if_nothing_returned,
    clippy::let_unit_value,
    clippy::unused_async,
    clippy::doc_markdown,
    clippy::unnecessary_lazy_evaluations
)]

use rustycode_orchestration::state_machine::TaskContext;
use rustycode_orchestration::{ExecutionTier, HandoffPackage, TierIsolation};

#[test]
fn test_handoff_from_context_respects_isolation_limits() {
    let mut ctx = TaskContext::new("t1".into(), "fix the bug".into());
    // Exercise editor tier budget
    ctx.current_tier = 3;

    let isolation = TierIsolation::with_defaults();

    let package = HandoffPackage::from_context(&ctx, ExecutionTier::Editor, None, Some(&isolation));
    assert!(
        package.budget_summary.is_some(),
        "Expected budget summary to be present"
    );
    let summary = package.budget_summary.unwrap();

    let expected = isolation
        .budget_for(ctx.current_tier)
        .expect("budget exists for tier")
        .limit();
    assert_eq!(
        summary.tokens_limit, expected,
        "Handoff should reflect TierIsolation limits"
    );
}
