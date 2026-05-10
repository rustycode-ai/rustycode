#![allow(clippy::unwrap_used)]

use rustycode_skill::manager::SkillManager;
use std::fs;

#[allow(clippy::too_many_lines)]
fn setup_test_skills() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();

    // Create test skills with SKILL.md format
    let skills = vec![
        (
            "code-review",
            r#"---
name: code-review
description: Reviews code for quality issues
when-to-use: "Use when reviewing pull requests or checking code quality"
effort: low
categories:
  - code-review
  - quality
allowed-tools:
  - Bash
  - Read
---
# Code Review Skill

This skill helps review code for best practices.
"#,
        ),
        (
            "debugger",
            r#"---
name: debugger
description: Helps debug errors and exceptions
when-to-use: "Use when diagnosing bugs or unexpected behavior"
effort: medium
categories:
  - debugging
  - errors
allowed-tools:
  - Bash
  - Read
---
# Debugger Skill

This skill helps with debugging.
"#,
        ),
        (
            "rust-expert",
            r#"---
name: rust-expert
description: Rust-specific guidance
when-to-use: "Use for Rust programming questions"
effort: medium
categories:
  - rust
  - programming
allowed-tools:
  - "*"
activation:
  mode: conditional
  paths:
    - "*.rs"
    - "src/**/*.rs"
---
# Rust Expert Skill

Provides Rust-specific guidance.
"#,
        ),
        (
            "typescript-expert",
            r#"---
name: typescript-expert
description: TypeScript-specific guidance
when-to-use: "Use for TypeScript programming questions"
effort: medium
categories:
  - typescript
  - programming
allowed-tools:
  - "*"
activation:
  mode: conditional
  paths:
    - "*.ts"
    - "*.tsx"
    - "src/**/*.ts"
    - "src/**/*.tsx"
---
# TypeScript Expert Skill

Provides TypeScript-specific guidance.
"#,
        ),
        (
            "performance",
            r#"---
name: performance
description: Performance optimization guidance
when-to-use: "Use when optimizing for speed or memory"
effort: high
categories:
  - performance
  - optimization
allowed-tools:
  - Bash
---
# Performance Skill

Helps with performance optimization.
"#,
        ),
    ];

    for (name, content) in skills {
        let skill_dir = dir.path().join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    }

    dir
}

#[test]
fn activation_manager_context_scoring() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    // Test context-based activation
    let recs = mgr.activate_for_context("please review my pull request code");
    println!("Code review context recommendations: {:?}", recs);

    // code-review should be recommended
    assert!(
        recs.iter().any(|r| r.skill_id == "code-review"),
        "code-review should be recommended for code review context"
    );

    // Activate the top recommendation
    if let Some(rec) = recs.first() {
        let result = mgr.activate_skill(&rec.skill_id, "test-context");
        assert!(
            result.is_ok(),
            "Should be able to activate recommended skill"
        );
        assert!(
            mgr.is_active(&rec.skill_id),
            "Skill should be active after activation"
        );
    }
}

#[test]
fn activation_for_file_paths() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    // Test path-based conditional activation
    let activated = mgr.activate_for_paths(&["src/main.rs", "lib/utils.rs"]);
    println!("Activated for Rust files: {:?}", activated);

    // rust-expert should be activated for .rs files
    assert!(
        activated.iter().any(|id| id == "rust-expert"),
        "rust-expert should be activated for .rs files"
    );

    assert!(mgr.is_active("rust-expert"), "rust-expert should be active");
}

#[test]
fn activation_typescript_files() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    let activated = mgr.activate_for_paths(&["src/App.tsx", "src/utils.ts"]);
    println!("Activated for TypeScript files: {:?}", activated);

    assert!(
        activated.iter().any(|id| id == "typescript-expert"),
        "typescript-expert should be activated for .ts/.tsx files"
    );

    assert!(
        mgr.is_active("typescript-expert"),
        "typescript-expert should be active"
    );
}

#[test]
fn activation_no_match_for_unrelated_files() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    let activated = mgr.activate_for_paths(&["README.md", "package.json", "Cargo.toml"]);
    println!("Activated for config files: {:?}", activated);

    // No conditional skills should activate for these files
    assert!(
        activated.is_empty(),
        "No conditional skills should activate for config files"
    );
}

#[test]
fn budget_enforcement_prevents_over_activation() {
    let dir = setup_test_skills();

    // Very tight budget (1000 tokens)
    // High effort skill = 2000 tokens, so it should fail
    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(1_000)
        .build()
        .unwrap();

    // Try to activate a high-effort skill (performance = 2000 tokens)
    let result = mgr.activate_skill("performance", "manual");
    println!("Activation with tight budget: {:?}", result);

    // Should fail due to budget
    assert!(
        result.is_err(),
        "Should fail to activate skill exceeding budget"
    );
    if let Err(e) = result {
        let err_msg = e.to_string().to_lowercase();
        assert!(
            err_msg.contains("budget"),
            "Should fail with budget-related error, got: {}",
            e
        );
    }
}

#[test]
fn multiple_context_activations_coexist() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    // Activate for code review context
    let code_recs = mgr.activate_for_context("I need to review code");
    if let Some(rec) = code_recs.first() {
        mgr.activate_skill(&rec.skill_id, "code-review-context")
            .ok();
    }

    // Activate for debugging context
    let debug_recs = mgr.activate_for_context("I'm getting an error");
    if let Some(rec) = debug_recs.first() {
        mgr.activate_skill(&rec.skill_id, "debug-context").ok();
    }

    let active = mgr.active_definitions();
    println!(
        "Active skills: {:?}",
        active.iter().map(|s| &s.id).collect::<Vec<_>>()
    );

    assert!(
        !active.is_empty(),
        "Should have at least one skill activated"
    );
}

#[test]
fn active_skills_included_in_tool_scope() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    // Activate a skill
    mgr.activate_skill("code-review", "manual").ok();

    // Check tool scope includes the tools from the skill
    let scope = mgr.active_tool_scope();
    println!("Active tool scope: {:?}", scope);

    assert!(
        !scope.is_empty(),
        "Tool scope should contain tools from active skills"
    );
    assert!(
        scope.iter().any(|s| s == "Bash" || s == "Read"),
        "Tool scope should include Bash and Read from code-review skill"
    );
}

#[test]
fn deactivation_removes_from_scope() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    mgr.activate_skill("code-review", "manual").ok();
    let scope_active = mgr.active_tool_scope();
    assert!(
        !scope_active.is_empty()
            && (scope_active.iter().any(|s| s == "Bash")
                || scope_active.iter().any(|s| s == "Read")),
        "Should have tools in scope when active"
    );

    mgr.deactivate_skill("code-review");
    let scope_inactive = mgr.active_tool_scope();
    assert!(
        scope_inactive.is_empty()
            || (!scope_inactive.iter().any(|s| s == "Bash")
                && !scope_inactive.iter().any(|s| s == "Read")),
        "Should not have code-review's tools in scope when deactivated"
    );
}

#[test]
fn activation_recommendations_score_contextually() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    // Test different contexts return different recommendations
    let code_recs = mgr.activate_for_context("review pull request code quality");
    let debug_recs = mgr.activate_for_context("I'm getting a runtime error");
    let perf_recs = mgr.activate_for_context("optimize for performance");

    println!(
        "Code review recs: {:?}",
        code_recs
            .iter()
            .map(|r| (&r.skill_id, r.score))
            .collect::<Vec<_>>()
    );
    println!(
        "Debug recs: {:?}",
        debug_recs
            .iter()
            .map(|r| (&r.skill_id, r.score))
            .collect::<Vec<_>>()
    );
    println!(
        "Performance recs: {:?}",
        perf_recs
            .iter()
            .map(|r| (&r.skill_id, r.score))
            .collect::<Vec<_>>()
    );

    // Each context should have recommendations (order might vary by implementation)
    assert!(
        !code_recs.is_empty(),
        "Should have code review recommendations"
    );
    assert!(!debug_recs.is_empty(), "Should have debug recommendations");
    assert!(
        !perf_recs.is_empty(),
        "Should have performance recommendations"
    );
}

#[test]
fn end_to_end_activation_workflow() {
    let dir = setup_test_skills();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir.path())
        .token_budget(50_000)
        .build()
        .unwrap();

    println!("Initial active skills: {}", mgr.active_definitions().len());

    // Step 1: User starts working on Rust code
    let path_activated = mgr.activate_for_paths(&["src/main.rs"]);
    println!("Path-based activation: {:?}", path_activated);
    assert!(!path_activated.is_empty(), "Should activate for Rust files");

    // Step 2: User asks about optimization
    let context_recs = mgr.activate_for_context("how can I optimize this code");
    println!(
        "Context recommendations: {:?}",
        context_recs
            .iter()
            .map(|r| (&r.skill_id, r.score))
            .collect::<Vec<_>>()
    );

    if let Some(rec) = context_recs.iter().find(|r| r.skill_id == "performance") {
        mgr.activate_skill(&rec.skill_id, "user-request").ok();
    }

    // Step 3: Check active skills and their tool scope
    let active_tools = mgr.active_tool_scope();
    println!("Active skills available as tools: {:?}", active_tools);

    assert!(
        !active_tools.is_empty(),
        "Should have active skills available"
    );

    // Step 4: Session ends - cleanup
    mgr.end_session();
    println!("Session ended, quality metrics recorded");
}
