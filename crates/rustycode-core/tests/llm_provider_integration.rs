#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
use rustycode_core::generate_plan_with_llm;
use rustycode_llm::mock::MockProvider;

#[test]
fn mock_provider_parses_pure_json() {
    let json = r#"
    {
      "summary": "Do the thing",
      "approach": "Simple approach",
      "steps": [
        { "title": "Step One", "description": "Do step one", "tools": ["Read"], "expected_outcome": "Done", "rollback_hint": "N/A" }
      ],
      "files_to_modify": ["src/lib.rs"],
      "risks": ["low risk"]
    }
    "#;

    let provider = MockProvider::from_text(json);
    let plan = generate_plan_with_llm(&provider, "task", &["Read"]).expect("parsed plan");

    assert_eq!(plan.summary, "Do the thing");
    assert_eq!(plan.approach, "Simple approach");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].title, "Step One");
}

#[test]
fn mock_provider_parses_markdown_wrapped_json() {
    let body = r#"
    {
      "summary": "Wrapped",
      "approach": "Wrap approach",
      "steps": [ { "title": "Wrapped Step", "description": "x", "tools": [], "expected_outcome": "ok", "rollback_hint": "N/A" } ]
    }
    "#;

    let wrapped = format!("Here is the plan:\n```json\n{}\n```", body);
    let provider = MockProvider::from_text(wrapped);
    let plan = generate_plan_with_llm(&provider, "task", &[]).expect("parsed wrapped plan");

    assert_eq!(plan.summary, "Wrapped");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].title, "Wrapped Step");
}
