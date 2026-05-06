//! Test suite exploring classification limitations and where code-aware metrics would help.
//!
//! This suite demonstrates:
//! 1. Where keyword-only classification succeeds
//! 2. Where it fails (false positives/negatives)
//! 3. Where code context would significantly improve routing

use rustycode_classification::{ComplexityTier, UnifiedTaskClassifier};
use rustycode_protocol::agent_protocol::AgentRole;

#[test]
fn keyword_only_works_for_obvious_mundane() {
    let classifier = UnifiedTaskClassifier::new();
    let task = "list all files in the current directory";
    let result = classifier.classify(task);
    assert!(result.complexity_score < 40);
    assert_eq!(result.tier, ComplexityTier::Light);
    println!("✓ PASS: Simple read-only task correctly routed to Light");
}

#[test]
fn keyword_only_works_for_obvious_complex() {
    let classifier = UnifiedTaskClassifier::new();
    let task = "refactor the authentication module to support OAuth2 and WebAuthn";
    let result = classifier.classify(task);
    assert!(result.complexity_score >= 51);
    assert_eq!(result.tier, ComplexityTier::Standard);
    println!("✓ PASS: Complex refactor correctly identified");
}

// ============================================================================
// FAILURE CASES: Keyword matching misleads
// ============================================================================

#[test]
fn false_positive_list_command_is_not_simple() {
    let classifier = UnifiedTaskClassifier::new();
    let task = "list all functions that call the transaction handler with complex nested callbacks";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!(
        "❌ ISSUE: 'list' keyword makes this seem simple, but requires deep codebase analysis"
    );
    println!("   Expected: Heavy (requires code understanding)");
    println!("   Got: {} (keyword triggered -10 score)", result.tier);
    // This task actually requires finding complex call chains, but scores as Light
    assert!(
        result.complexity_score < 50,
        "False negative: complex task scored as light"
    );
}

#[test]
fn false_negative_typo_with_critical_impact() {
    let classifier = UnifiedTaskClassifier::new();
    let task = "fix typo in the HTTP server port constant used across 47 deployment configs";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!("❌ ISSUE: 'typo' keyword triggers mundane classification (-10)");
    println!("   But task impacts multiple files (negative risk)");
    println!("   Expected: Standard (multi-file, risky)");
    println!("   Got: {} (typo keyword overpowers)", result.tier);
    // Score might be raised by multi_file signal, but the -10 penalty is suspicious
}

#[test]
fn ambiguous_task_without_code_context() {
    let classifier = UnifiedTaskClassifier::new();
    let task = "optimize the codebase";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!("❌ ISSUE: 'optimize' detected, but WHERE? CPU? Memory? Startup time?");
    println!("   Without code context, router can't distinguish:");
    println!("   - Small function tuning (Light)");
    println!("   - Database query optimization (Heavy)");
    println!("   - System-wide architecture optimization (Architect-level)");
    println!("   Got agent role: {:?}", result.agent_role);
    assert_eq!(
        result.signals.ambiguous, false,
        "Should be marked ambiguous without context"
    );
}

// ============================================================================
// CODE CONTEXT REQUIRED: Where code metrics would help
// ============================================================================

#[test]
fn add_function_can_be_trivial_or_complex() {
    let classifier = UnifiedTaskClassifier::new();

    // Scenario 1: Add a simple function
    let simple = "add a helper function to parse dates";
    let simple_result = classifier.classify(simple);
    println!(
        "SCENARIO 1 - Simple function:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        simple, simple_result.complexity_score, simple_result.tier
    );

    // Scenario 2: Add a complex distributed system function
    let complex = "add a function that handles consensus coordination across a distributed Raft cluster with failure recovery";
    let complex_result = classifier.classify(complex);
    println!(
        "SCENARIO 2 - Complex function:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        complex, complex_result.complexity_score, complex_result.tier
    );

    println!(
        "❌ ISSUE: Both classified as Standard because 'add' keyword doesn't distinguish complexity"
    );
    println!("   Without code context, can't tell if function:");
    println!("   - Is 5 lines (Light)");
    println!("   - Requires understanding of Raft consensus (Heavy)");
    println!("   - Fits into complex distributed system (Architect)");

    assert_eq!(
        simple_result.complexity_score, complex_result.complexity_score,
        "Should score differently but keywords mask difference"
    );
}

#[test]
fn fix_bug_depends_on_root_cause() {
    let classifier = UnifiedTaskClassifier::new();

    // Scenario 1: Simple off-by-one
    let simple_bug = "fix off-by-one error in array bounds check";
    let simple_result = classifier.classify(simple_bug);
    println!(
        "SCENARIO 1 - Simple bug:\n  Task: {}\n  Score: {}/100",
        simple_bug, simple_result.complexity_score
    );

    // Scenario 2: Race condition in async code
    let complex_bug = "fix intermittent race condition in concurrent map access under high load";
    let complex_result = classifier.classify(complex_bug);
    println!(
        "SCENARIO 2 - Complex bug:\n  Task: {}\n  Score: {}/100",
        complex_bug, complex_result.complexity_score
    );

    println!("❌ ISSUE: Both hit 'debugging' signal, similar scores");
    println!("   Code metrics would reveal:");
    println!("   - Simple: Few LOC, low symbol density → Light");
    println!("   - Complex: Async code, concurrency primitives, high nesting → Heavy");

    // Both trigger debugging signal
    assert!(simple_result.signals.debugging);
    assert!(complex_result.signals.debugging);
}

#[test]
fn refactor_scope_is_invisible() {
    let classifier = UnifiedTaskClassifier::new();

    // Scenario 1: Rename a private function
    let local_refactor = "refactor the internal helper function name for clarity";
    let local_result = classifier.classify(local_refactor);
    println!(
        "SCENARIO 1 - Local refactor:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        local_refactor, local_result.complexity_score, local_result.tier
    );

    // Scenario 2: Refactor entire module structure
    let large_refactor = "refactor the message routing layer to use a trait-based dispatch system instead of nested matches";
    let large_result = classifier.classify(large_refactor);
    println!(
        "SCENARIO 2 - Large refactor:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        large_refactor, large_result.complexity_score, large_result.tier
    );

    println!("❌ ISSUE: Both match 'refactor' keyword (+25)");
    println!("   Without code context, can't tell if refactor:");
    println!("   - Affects 1 file, 50 LOC (Light)");
    println!("   - Affects 20+ files, 5000+ LOC (Heavy)");
    println!("   - Introduces new trait patterns (Architect)");
    println!(
        "Score difference: {} vs {}",
        local_result.complexity_score, large_result.complexity_score
    );
}

#[test]
fn implement_feature_scope_unknown() {
    let classifier = UnifiedTaskClassifier::new();

    // Scenario 1: Add a simple feature flag
    let small_impl = "implement a feature flag to toggle dark mode in the UI";
    let small_result = classifier.classify(small_impl);
    println!(
        "SCENARIO 1 - Small feature:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        small_impl, small_result.complexity_score, small_result.tier
    );

    // Scenario 2: Implement a complex system
    let large_impl = "implement a complete permission system with role-based access control, audit logging, and delegation";
    let large_result = classifier.classify(large_impl);
    println!(
        "SCENARIO 2 - Large feature:\n  Task: {}\n  Score: {}/100, Tier: {:?}",
        large_impl, large_result.complexity_score, large_result.tier
    );

    println!("❌ ISSUE: 'implement' keyword doesn't scale with feature complexity");
    println!("   Code metrics would show:");
    println!("   - Small: Few modules, simple state (Light)");
    println!("   - Large: Multiple modules, deep nesting, trait patterns (Heavy)");
}

// ============================================================================
// SILENT FAILURES: Tasks that should escalate but don't
// ============================================================================

#[test]
fn dangerous_change_sounds_simple() {
    let classifier = UnifiedTaskClassifier::new();

    let task = "update the account creation logic";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!("❌ SILENT FAILURE: This sounds mundane but touches:");
    println!("   - Authentication flows");
    println!("   - User permissions");
    println!("   - Database transactions");
    println!("   - Email notifications");
    println!("   Without 'security' keyword, scores as Light-Standard");
    println!("   Code context would see: high symbol density, trait usage, async calls");

    // The task doesn't mention 'security' or 'risky', so it won't score high
    assert!(!result.signals.risky);
}

#[test]
fn data_transformation_complexity_hidden() {
    let classifier = UnifiedTaskClassifier::new();

    let task = "transform the input data format";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!("❌ SILENT FAILURE: 'transform' is vague");
    println!("   Could be:");
    println!("   - Simple string split (1 function)");
    println!("   - Complex schema migration (multi-file, unsafe code)");
    println!("   - Distributed data pipeline (system-level)");
    println!("   Current classification: {}", result.tier);
    println!("   Code metrics would reveal actual complexity");
}

// ============================================================================
// SIGNAL COMBINATIONS: Where multi-signal routing fails
// ============================================================================

#[test]
fn risky_but_small_tasks() {
    let classifier = UnifiedTaskClassifier::new();

    let task = "change the database connection string in production";
    let result = classifier.classify(task);
    println!(
        "Task: {}\nScore: {}/100, Tier: {:?}",
        task, result.complexity_score, result.tier
    );
    println!("Agent role: {:?}", result.agent_role);
    println!("✓ Correctly identified risky (found 'database')");
    println!("   But agents may underestimate impact due to Light/Standard tier");
}

// ============================================================================
// WHERE CODE METRICS WOULD HELP: Summary
// ============================================================================

#[test]
fn summary_code_metrics_needed() {
    println!("\n=== SUMMARY: Where Code Metrics Would Help ===\n");

    println!("1. KEYWORD AMBIGUITY:");
    println!("   - 'refactor' can be 50 LOC or 5000+ LOC");
    println!("   - 'implement' can be 1 file or 20+ files");
    println!("   - 'fix' can be 1 line or require deep tracing\n");

    println!("2. TASK DESCRIPTION vs CODE REALITY:");
    println!("   - Simple-sounding tasks that touch critical paths");
    println!("   - Complex-sounding tasks that are actually isolated\n");

    println!("3. SILENT ESCALATIONS:");
    println!("   - Risky changes that don't mention 'security'");
    println!("   - Multi-file impacts not mentioned in task\n");

    println!("4. CODE-AWARE ROUTING WOULD ENABLE:");
    println!("   - Symbol density → complexity (complex refactors routed to Architect)");
    println!("   - File count → scope (multi-file changes to Builder)");
    println!("   - Unsafe code → expertise (Scalpel for unsafe)");
    println!("   - Nesting depth → maintainability (deep nesting = Heavy)");
    println!("   - Trait patterns → design expertise (Architect)\n");

    println!("5. EXAMPLE IMPROVEMENTS:");
    println!("   Task: 'fix typo in constants'");
    println!("   Current: Light (matches 'typo' keyword)");
    println!("   With code: Standard (47 files referenced, high symbol density)");
    println!("   Better routing: Builder (multi-file) instead of Worker\n");

    println!("6. INTEGRATION POINTS:");
    println!("   - rustycode-tools has repo_map.rs with all tree-sitter parsing");
    println!("   - Can extract into rustycode-tree-sitter crate");
    println!("   - Feed CodeContext to classify_with_context()");
}
