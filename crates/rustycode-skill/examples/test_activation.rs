/// CLI tool to test skill activation end-to-end
///
/// Run with: cargo run --example test_activation
use rustycode_skill::manager::SkillManager;
use std::fs;

fn hr() {
    println!("{}", "-".repeat(60));
}

#[allow(
    clippy::expect_used,
    clippy::redundant_closure_for_method_calls,
    clippy::if_not_else,
    clippy::unnecessary_debug_formatting
)]
fn main() {
    // Setup
    println!("🧪 Skill Activation Test Suite\n");

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    create_test_skills(temp_dir.path());

    // Test 1: Context-based activation
    test_context_activation(temp_dir.path());

    // Test 2: Path-based activation
    test_path_activation(temp_dir.path());

    // Test 3: Tool scope integration
    test_tool_scope_integration(temp_dir.path());

    // Test 4: Budget enforcement
    test_budget_enforcement(temp_dir.path());

    println!("\n✅ All tests completed!");
}

#[allow(
    clippy::too_many_lines,
    clippy::expect_used,
    clippy::unnecessary_debug_formatting
)]
fn create_test_skills(dir: &std::path::Path) {
    let skills = vec![
        (
            "code-reviewer",
            "Code review skill",
            "Use for code quality checks",
            vec!["Bash", "Read"],
            "low",
            None,
        ),
        (
            "debugger",
            "Debugging helper",
            "Use for fixing errors",
            vec!["Bash", "Read"],
            "low",
            None,
        ),
        (
            "performance-optimizer",
            "Performance tuning",
            "Use for optimization",
            vec!["Bash"],
            "high",
            None,
        ),
        (
            "rust-guide",
            "Rust language guide",
            "Use for Rust questions",
            vec!["*"],
            "medium",
            Some(
                r#"activation:
  mode: conditional
  paths:
    - "*.rs"
    - "src/**/*.rs""#,
            ),
        ),
    ];

    for (name, desc, when, tools, effort, activation) in skills {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).expect("Failed to create skill dir");

        let tools_yaml = tools
            .iter()
            .map(|t| format!("  - {}", t))
            .collect::<Vec<_>>()
            .join("\n");

        let activation_yaml = activation.unwrap_or("");

        let content = format!(
            r#"---
name: {}
description: {}
when-to-use: "{}"
effort: {}
allowed-tools:
{}
{}
---

# {}

This is a test skill for {}
"#,
            name,
            desc,
            when,
            effort,
            tools_yaml,
            if activation_yaml.is_empty() {
                String::new()
            } else {
                format!("{}\n", activation_yaml)
            },
            name,
            when
        );

        fs::write(skill_dir.join("SKILL.md"), content).expect("Failed to write SKILL.md");
    }

    println!("✓ Created test skills in {}\n", dir.display());
}

#[allow(clippy::expect_used)]
fn test_context_activation(dir: &std::path::Path) {
    println!("📋 TEST 1: Context-Based Activation");
    hr();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir)
        .token_budget(50_000)
        .build()
        .expect("Failed to build SkillManager");

    let test_cases = vec![
        (
            "I need to review my code for quality issues",
            "code-reviewer",
        ),
        ("I'm getting an error, can you help debug?", "debugger"),
        (
            "How do I optimize this code for performance?",
            "performance-optimizer",
        ),
    ];

    for (context, expected) in test_cases {
        println!("\n  Input: \"{}\"", context);

        let recs = mgr.activate_for_context(context);

        println!("  Recommendations:");
        for rec in &recs {
            let marker = if rec.skill_id == expected { "✓" } else { " " };
            println!("    {} {} (score: {:.2})", marker, rec.skill_id, rec.score);
        }

        let is_active = mgr.is_active(expected);
        let status = if is_active {
            "✓ ACTIVE"
        } else {
            "✗ NOT ACTIVE"
        };
        println!("  Expected skill status: {} {}", expected, status);
    }
}

#[allow(clippy::expect_used, clippy::redundant_closure_for_method_calls)]
fn test_path_activation(dir: &std::path::Path) {
    println!("\n📋 TEST 2: Path-Based Activation");
    hr();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir)
        .token_budget(50_000)
        .build()
        .expect("Failed to build SkillManager");

    let test_cases: Vec<(Vec<&str>, &str)> = vec![
        (vec!["src/main.rs", "lib/utils.rs"], "Rust files"),
        (vec!["README.md", "package.json"], "Config files"),
    ];

    for (files, label) in test_cases {
        println!("\n  Input: {} files", label);
        for f in &files {
            println!("    - {}", f);
        }

        let file_refs: Vec<&str> = files.iter().map(|s| s.as_ref()).collect();
        let activated = mgr.activate_for_paths(&file_refs);

        if activated.is_empty() {
            println!("  No skills activated");
        } else {
            println!("  Activated:");
            for skill_id in activated {
                println!("    ✓ {}", skill_id);
            }
        }
    }
}

#[allow(clippy::expect_used, clippy::if_not_else)]
fn test_tool_scope_integration(dir: &std::path::Path) {
    println!("\n📋 TEST 3: Tool Scope Integration");
    hr();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir)
        .token_budget(50_000)
        .build()
        .expect("Failed to build SkillManager");

    println!("\n  Step 1: Activate a skill");
    match mgr.activate_skill("code-reviewer", "manual") {
        Ok(()) => println!("    ✓ Activated code-reviewer"),
        Err(e) => println!("    ✗ Failed: {}", e),
    }

    println!("\n  Step 2: Check if skill is active");
    if mgr.is_active("code-reviewer") {
        println!("    ✓ code-reviewer is active");
    } else {
        println!("    ✗ code-reviewer is NOT active");
    }

    println!("\n  Step 3: Check active definitions");
    let defs = mgr.active_definitions();
    if defs.is_empty() {
        println!("    ✗ No active definitions found!");
    } else {
        println!("    ✓ Active definitions:");
        for def in defs {
            println!("      - {}", def.id);
        }
    }

    println!("\n  Step 4: Check tool scope");
    let scope = mgr.active_tool_scope();
    if scope.is_empty() {
        println!("    ⚠️  Tool scope is EMPTY (BUG!)");
        println!("    This is the problem: activated skills aren't in tool scope");
    } else {
        println!("    ✓ Tools available:");
        for tool in scope {
            println!("      - {}", tool);
        }
    }

    println!("\n  Step 5: Deactivate and verify removal");
    mgr.deactivate_skill("code-reviewer");
    if mgr.is_active("code-reviewer") {
        println!("    ✗ Skill still active");
    } else {
        println!("    ✓ Skill deactivated");
    }
}

#[allow(clippy::expect_used)]
fn test_budget_enforcement(dir: &std::path::Path) {
    println!("\n📋 TEST 4: Budget Enforcement");
    hr();

    let mut mgr = SkillManager::builder()
        .user_skills_dir(dir)
        .token_budget(1_000) // Very tight budget
        .build()
        .expect("Failed to build SkillManager");

    println!("\n  Budget: 1,000 tokens (very tight)");
    println!("\n  Attempting to activate high-effort skill...");

    match mgr.activate_skill("performance-optimizer", "manual") {
        Ok(()) => {
            println!("    ⚠️  Skill activated despite tight budget");
        }
        Err(e) => {
            println!("    ✓ Budget enforcement worked: {}", e);
        }
    }
}
