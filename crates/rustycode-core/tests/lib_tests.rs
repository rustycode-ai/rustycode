#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::uninlined_format_args
)]

use rustycode_core::{
    generate_plan_with_llm, generate_smart_plan, select_code_excerpts, Runtime,
    StepExecutorRegistry,
};
use rustycode_llm::mock::MockProvider;
use rustycode_llm::provider::ProviderError;
use rustycode_protocol::{ContextSectionKind, EventKind, SessionId};
use std::fs;
use std::path::PathBuf;

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("rustycode-core-{}", SessionId::new()));
    fs::create_dir_all(&path).unwrap();
    path
}

// ── Step Executor Tests ────────────────────────────────────────────────

#[test]
fn step_executor_registry_can_register_and_retrieve() {
    let mut registry = StepExecutorRegistry::new();
    let executor = registry.default_executor(PathBuf::from("."));
    registry.register("generic".to_string(), executor.clone());

    assert!(registry.get("generic").is_some());
    assert!(registry.get("nonexistent").is_none());
}

// ──────────────────────────────────────────────────────────────────────

#[test]
#[ignore = "Complex integration test - requires specific file setup"]
fn run_assembles_context_from_local_config() {
    let cwd = temp_dir();
    let data_dir = cwd.join("data");
    let skills_dir = cwd.join("skills");
    let memory_dir = cwd.join("memory");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::create_dir_all(&memory_dir).unwrap();
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::create_dir_all(skills_dir.join("reviewer")).unwrap();
    fs::write(
        skills_dir.join("reviewer").join("SKILL.md"),
        "# Reviewer\n\nFinds regressions.\n",
    )
    .unwrap();
    fs::write(memory_dir.join("notes.md"), "prefer concise summaries\n").unwrap();
    fs::write(
        cwd.join("src").join("parser.rs"),
        "pub fn parse_feature_gate() {\n    let feature_gate = true;\n}\n",
    )
    .unwrap();
    // Config loader searches for .rustycode/config.json, not .rustycode.json
    let config_dir = cwd.join(".rustycode");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.json"),
        format!(
            "{{\n  \"data_dir\": \"{}\",\n  \"skills_dir\": \"{}\",\n  \"memory_dir\": \"{}\",\n  \"lsp_servers\": []\n}}\n",
            data_dir.display(),
            skills_dir.display(),
            memory_dir.display()
        ),
    )
    .unwrap();

    let runtime = Runtime::load(&cwd).unwrap();
    let _ = runtime.run(&cwd, "previous task for history").unwrap();
    let report = runtime.run(&cwd, "Inspect parser feature gate").unwrap();

    assert_eq!(report.memory.len(), 1);
    assert_eq!(report.skills.len(), 1);
    assert_eq!(report.recent_tasks, vec!["previous task for history"]);
    assert!(!report.code_excerpts.is_empty());
    assert!(report.code_excerpts[0].path.ends_with("parser.rs"));
    assert_eq!(report.context_plan.total_budget, 8_000);
    assert_eq!(report.context_plan.reserved_budget, 8_000);
    assert!(report.context_plan.sections.iter().any(|section| {
        section.kind == ContextSectionKind::RecentTurns && !section.items.is_empty()
    }));
    assert!(report
        .context_plan
        .sections
        .iter()
        .any(|section| section.kind == ContextSectionKind::Memory && !section.items.is_empty()));
    assert!(report
        .context_plan
        .sections
        .iter()
        .any(|section| section.kind == ContextSectionKind::Skills && !section.items.is_empty()));
    let tool_report = runtime
        .run_tool(
            &cwd,
            "Read".to_string(),
            serde_json::json!({ "path": ".rustycode/config.json" }),
        )
        .unwrap();
    let events = runtime.session_events(&tool_report.session.id).unwrap();
    assert!(tool_report.result.error.is_none()); // success = no error
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, EventKind::ToolExecuted)));
}

#[test]
fn code_excerpt_selection_prefers_task_matches() {
    let cwd = temp_dir();
    fs::create_dir_all(cwd.join("src")).unwrap();
    fs::write(
        cwd.join("src").join("planner.rs"),
        "pub fn planner_budget() {\n    let budget = 10;\n}\n",
    )
    .unwrap();
    fs::write(
        cwd.join("README.md"),
        "# RustyCode\n\nGeneral project notes.\n",
    )
    .unwrap();

    let excerpts = select_code_excerpts(&cwd, "planner budget", 2).unwrap();

    assert_eq!(excerpts.len(), 2);
    assert!(excerpts[0].path.ends_with("planner.rs"));
    assert!(excerpts[0].score >= excerpts[1].score);
}

// LLM plan generation tests

#[tokio::test]
async fn generate_plan_with_llm_parses_pure_json() {
    let json = r#"
        {
          "summary": "Do the thing",
          "approach": "Simple approach",
          "steps": [
            {
              "title": "Step One",
              "description": "Do step one",
              "tools": ["Read"],
              "expected_outcome": "Done",
              "rollback_hint": "N/A"
            }
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
    assert_eq!(plan.files_to_modify, vec!["src/lib.rs".to_string()]);
    assert_eq!(plan.risks, vec!["low risk".to_string()]);
}

#[tokio::test]
async fn generate_plan_with_llm_parses_markdown_wrapped_json() {
    let body = r#"
        {
          "summary": "Wrapped",
          "approach": "Wrap approach",
          "steps": [
            { "title": "Wrapped Step", "description": "x", "tools": [], "expected_outcome": "ok", "rollback_hint": "N/A" }
          ]
        }
        "#;

    let wrapped = format!("Here is the plan:\n```json\n{}\n```", body);

    let provider = MockProvider::from_text(wrapped);
    let plan = generate_plan_with_llm(&provider, "task", &[]).expect("parsed wrapped plan");
    assert_eq!(plan.summary, "Wrapped");
    assert_eq!(plan.steps.len(), 1);
    assert_eq!(plan.steps[0].title, "Wrapped Step");
}

#[test]
fn generate_smart_plan_falls_back_when_llm_fails() {
    let provider = MockProvider::new(
        vec![Err(ProviderError::Api("simulated failure".to_string()))],
        None,
    );
    let plan = generate_smart_plan("do stuff", &[], Some(&provider));
    assert!(plan.summary.starts_with("Plan for:"));
    assert!(!plan.steps.is_empty());
    assert_eq!(plan.steps[0].title, "Explore codebase");
}
