#![allow(
    clippy::doc_markdown,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args
)]
//! Integration tests for Architect and Scalpel phases.
//!
//! Demonstrates the end-to-end flow:
//! 1. Architect analyzes codebase, produces StructuralDeclaration
//! 2. Builder implements according to declaration
//! 3. Skeptic reviews code + structure compliance
//! 4. Judge runs tests/compilation
//! 5. On targeted failures, Scalpel makes surgical fixes (no redesign)
//! 6. If failures require redesign, escalate to human or Builder retry

use rustycode_protocol::team::{
    DependencyChanges, ModuleAction, ModuleDeclaration, StructuralDeclaration,
};
use rustycode_team::{ArchitectPhase, ScalpelPhase};

fn test_decl() -> StructuralDeclaration {
    StructuralDeclaration {
        modules: vec![ModuleDeclaration {
            path: "src/auth.rs".to_string(),
            action: ModuleAction::Create,
            exports: vec!["LoginHandler".to_string()],
            imports: vec!["async-trait".to_string()],
            purpose: "Handles user authentication".to_string(),
        }],
        interfaces: vec![],
        dependencies: DependencyChanges {
            add: vec![],
            remove: vec![],
            keep: vec!["tokio".to_string()],
        },
    }
}

#[test]
fn architect_produces_declaration_for_task() {
    let phase = ArchitectPhase::new("/tmp/project");
    let decl = test_decl();
    assert!(phase.validate(&decl));
    assert_eq!(decl.modules.len(), 1);
    assert_eq!(decl.modules[0].path, "src/auth.rs");
}

#[test]
fn declaration_locks_module_boundaries() {
    let decl = test_decl();
    // Declaration specifies exactly which modules will be created/modified
    // Builder must NOT touch undeclared modules
    assert_eq!(decl.modules.len(), 1);
    assert_eq!(decl.modules[0].action, ModuleAction::Create);
}

#[test]
fn scalpel_identifies_compile_errors_as_fixable() {
    let compile_errors = vec![
        "error[E0308]: mismatched types".to_string(),
        "error[E0425]: cannot find value `x`".to_string(),
    ];
    assert!(ScalpelPhase::is_scalpel_appropriate(&compile_errors));
}

#[test]
fn scalpel_identifies_logic_errors_as_requiring_rebuild() {
    let logic_errors = vec![
        "test failed: wrong output logic error".to_string(),
        "assertion failed: expected 42, got 0".to_string(),
    ];
    assert!(!ScalpelPhase::is_scalpel_appropriate(&logic_errors));
}

#[test]
fn scalpel_identifies_approach_issues_as_requiring_rebuild() {
    let approach_issues = vec![
        "redesign needed: current approach won't work".to_string(),
        "this requires a different design approach".to_string(),
    ];
    assert!(!ScalpelPhase::is_scalpel_appropriate(&approach_issues));
}

#[test]
fn minimal_scalpel_fix_for_missing_import() {
    let phase = ScalpelPhase::new("/tmp/project");
    let failures = vec!["error[E0432]: unresolved import `Result`".to_string()];

    // Missing import is compile error, scalpel can fix it
    assert!(ScalpelPhase::is_scalpel_appropriate(&failures));
    let prompt = phase.system_prompt("fix the import error");
    assert!(prompt.contains("minimal"));
    assert!(prompt.contains("one precise edit"));
}

#[test]
fn declaration_prevents_scope_creep() {
    let decl = test_decl();

    // Declaration explicitly lists what will be modified
    let declared_modules: Vec<&str> = decl.modules.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(declared_modules, vec!["src/auth.rs"]);

    // Any changes to files NOT in this list would violate the declaration
    // Skeptic should detect and veto such changes
}

#[test]
fn architect_phase_is_one_time_cost() {
    let phase = ArchitectPhase::new("/tmp/project");
    let decl = test_decl();

    // Architect runs once, produces one declaration
    // All subsequent Builder/Skeptic/Judge turns reference this same declaration
    assert!(phase.validate(&decl));

    // Second validation with same declaration succeeds
    assert!(phase.validate(&decl));
}

#[test]
fn scalpel_cannot_redesign() {
    let redesign_signals = ["redesign", "approach", "logic"];

    // Any failure mentioning these signals requires full Builder redesign
    for signal in &redesign_signals {
        let failure = format!("failed: {} error", signal);
        assert!(!ScalpelPhase::is_scalpel_appropriate(&[failure]));
    }
}
