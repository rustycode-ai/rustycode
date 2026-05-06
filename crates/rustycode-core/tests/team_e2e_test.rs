#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::float_cmp,
    clippy::manual_string_new,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else
)]
//! E2E tests for the TeamOrchestrator.
//!
//! These tests verify the full Builder→Skeptic→Judge→Coordinator
//! loop works end-to-end using a mock LLM client.

use anyhow::Result;
use async_trait::async_trait;
use rustycode_llm::provider::ChatMessage;
use rustycode_team::orchestrator::{is_scalpel_appropriate, TeamLLMClient};
use rustycode_team::{OrchestratorConfig, TeamOrchestrator};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

// Mock LLM Client for E2E

/// A mock LLM client that returns prescripted responses in LIFO order.
struct MockLLM {
    responses: Mutex<Vec<String>>,
}

impl MockLLM {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl TeamLLMClient for MockLLM {
    async fn complete(&self, _messages: Vec<ChatMessage>) -> Result<String> {
        let mut responses = self.responses.lock().unwrap();
        match responses.pop() {
            Some(response) => Ok(response),
            None => Err(anyhow::anyhow!("no more mock responses")),
        }
    }
}

// Helpers

/// Create a temp project directory with a Cargo.toml + src/lib.rs
fn setup_temp_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).unwrap();

    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"testproj\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .unwrap();

    std::fs::write(
        src.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn test_add() {\n        assert_eq!(add(2, 3), 5);\n    }\n}\n",
    )
    .unwrap();
    dir
}

/// Valid Builder JSON response. Each step in the plan needs one Builder call.
/// For Low risk tasks, the plan has 3 steps: Understand, Implement, Verify.
fn builder_json(approach: &str, path: &str, done: bool) -> String {
    format!(
        r#"{{
  "approach": "{}",
  "changes": [{{
    "path": "{}",
    "summary": "added function",
    "diff_hunk": "+pub fn new_fn() -> i32 {{\\n+    42\\n+}}\\n",
    "lines_added": 2,
    "lines_removed": 0
  }}],
  "claims": ["implemented changes"],
  "confidence": 0.9,
  "done": {}
}}"#,
        approach, path, done
    )
}

/// Skeptic approve response
fn skeptic_approve() -> String {
    r#"{
  "verdict": "approve",
  "verified": ["all claims verified"],
  "refuted": [],
  "insights": []
}"#
    .to_string()
}

/// Skeptic veto response
fn skeptic_veto(claim: &str, evidence: &str) -> String {
    format!(
        r#"{{
  "verdict": "veto",
  "verified": [],
  "refuted": [{{ "claim": "{}", "evidence": "{}" }}],
  "insights": ["hallucination detected"]
}}"#,
        claim, evidence
    )
}

/// Build mock responses for a complete 3-step plan (Low risk).
/// Each step = Skeptic (popped first) + Builder (popped second).
/// Responses are consumed LIFO.
fn three_step_plan_responses() -> Vec<String> {
    vec![
        // Step 3: Verify (popped last, consumed first in step 3)
        skeptic_approve(),
        builder_json("verify changes work", "src/lib.rs", true),
        // Step 2: Implement (consumed in step 2)
        skeptic_approve(),
        builder_json("implement the change", "src/lib.rs", true),
        // Step 1: Understand (consumed in step 1)
        skeptic_approve(),
        builder_json("read and understand code", "src/lib.rs", false),
    ]
}

// Test 1: Happy path — all 3 steps complete

#[tokio::test]
async fn team_e2e_happy_path() {
    let dir = setup_temp_project();
    let responses = three_step_plan_responses();

    let client = Arc::new(MockLLM::new(responses));
    let config = OrchestratorConfig {
        use_local_judge: false,
        ..OrchestratorConfig::default()
    };

    let orchestrator = TeamOrchestrator::with_client(dir.path(), client, config);
    let outcome = orchestrator
        .execute("add a hello function to lib.rs")
        .await
        .unwrap();

    assert!(outcome.success, "Expected success but got: {:?}", outcome);
    assert!(!outcome.files_modified.is_empty());
    assert!(outcome.turns > 0);
    assert!(outcome.final_trust > 0.0 && outcome.final_trust <= 1.0);
}

// Test 2: Skeptic veto causes step failure, retry exhausts budget

#[tokio::test]
async fn team_e2e_skeptic_veto() {
    let dir = setup_temp_project();

    // Provide enough responses for several retry attempts
    // Step 1 succeeds, then step 2 gets vetoed repeatedly
    let mut responses = Vec::new();

    // Add veto + builder for retries (max_retries_per_step = 3)
    for _ in 0..3 {
        responses.push(skeptic_veto("claim", "evidence"));
        responses.push(builder_json("retry implementation", "src/lib.rs", false));
    }

    // Step 1: understand (popped first)
    responses.push(skeptic_approve());
    responses.push(builder_json("read code", "src/lib.rs", false));

    let client = Arc::new(MockLLM::new(responses));
    let config = OrchestratorConfig {
        max_total_turns: 10,
        use_local_judge: false,
        ..OrchestratorConfig::default()
    };

    let orchestrator = TeamOrchestrator::with_client(dir.path(), client, config);
    let outcome = orchestrator.execute("add a hello function").await.unwrap();

    // Should fail — skeptic vetoed all retries
    assert!(
        !outcome.success,
        "Expected failure due to skeptic veto, got: {:?}",
        outcome
    );
}

// Test 3: Scalpel heuristic

#[test]
fn team_e2e_scalpel_heuristic() {
    // Compile errors should be scalpel-appropriate
    assert!(is_scalpel_appropriate("error[E0308]: mismatched types"));
    assert!(is_scalpel_appropriate(
        "error[E0425]: cannot find value `x` in this scope"
    ));
    // Logic errors should NOT be scalpel-appropriate
    assert!(!is_scalpel_appropriate("wrong output: expected 42 got 0"));
    assert!(!is_scalpel_appropriate("logic error in calculation"));
    assert!(!is_scalpel_appropriate(
        "approach is wrong, need to redesign"
    ));
}

// Test 4: Orchestrator outcome tracks all fields

#[tokio::test]
async fn team_e2e_outcome_structure() {
    let dir = setup_temp_project();
    let responses = three_step_plan_responses();

    let client = Arc::new(MockLLM::new(responses));
    let config = OrchestratorConfig {
        use_local_judge: false,
        ..OrchestratorConfig::default()
    };

    let orchestrator = TeamOrchestrator::with_client(dir.path(), client, config);
    let outcome = orchestrator.execute("add hello").await.unwrap();

    assert!(outcome.success);
    assert!(outcome.turns >= 1);
    assert!(outcome.final_trust > 0.0 && outcome.final_trust <= 1.0);
    assert!(!outcome.message.is_empty());
}
