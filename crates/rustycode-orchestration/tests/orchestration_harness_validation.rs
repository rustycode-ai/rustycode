//! Orchestration harness validation test.
//!
//! Exercises the full orchestration pipeline (quality detection → strategy selection →
//! tool injection → thought persistence → phase tracking) without requiring a live LLM.
//! Uses the "make MIPS interpreter" task as the primary complex test case.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::let_and_return,
    clippy::format_push_string,
    clippy::redundant_clone,
    clippy::match_single_binding,
    clippy::bool_to_int_with_if,
    clippy::unnecessary_lazy_evaluations,
    clippy::manual_let_else,
    clippy::collapsible_if,
    clippy::useless_conversion,
    clippy::cast_lossless,
    clippy::len_zero,
    clippy::ptr_arg,
    unused_imports,
    clippy::suboptimal_flops,
    clippy::ignored_unit_patterns
)]

use rustycode_orchestration::quality_detector::QualityDetector;
use rustycode_orchestration::reasoning_store::ReasoningStore;
use rustycode_orchestration::strategy_selector::StrategySelector;
use rustycode_orchestration::types::{QualityScore, ReasoningStrategy, StructuredThought};

/// Helper: build a QualityScore from individual axis values.
fn make_quality(specificity: f64, depth: f64, completeness: f64, uncertainty: f64) -> QualityScore {
    let total = specificity * 0.3 + depth * 0.3 + completeness * 0.25 + uncertainty * 0.15;
    QualityScore {
        specificity,
        depth,
        completeness,
        uncertainty,
        total,
    }
}

/// A "we haven't seen a response yet" neutral score — moderate across all axes.
fn unknown_quality() -> QualityScore {
    make_quality(2.0, 2.0, 2.0, 1.0) // total ≈ 1.85
}

/// A high-quality response score (what we'd expect after a good LLM answer).
fn high_quality() -> QualityScore {
    make_quality(4.5, 4.0, 4.0, 1.5) // total ≈ 3.925
}

struct TestCase {
    prompt: &'static str,
    expected_min_complexity: f64,
    expect_structured_thinking: bool,
}

const MIPS_INTERPRETER: TestCase = TestCase {
    prompt: "Implement a MIPS interpreter that supports R-type, I-type, and J-type instructions \
             with register file management, memory operations, and branch/jump handling. \
             Include a parser for MIPS assembly, support for labels, and a disassembler.",
    expected_min_complexity: 2.0, // "implement" keyword maps to 2.5
    expect_structured_thinking: true,
};

const FIX_TYPO: TestCase = TestCase {
    prompt: "fix the typo on line 42",
    expected_min_complexity: 0.0, // "fix" maps to 1.5
    expect_structured_thinking: false,
};

const REFACTOR_AUTH: TestCase = TestCase {
    prompt: "Refactor the authentication module to use JWT tokens instead of session cookies, \
             update all middleware, and ensure backward compatibility with existing sessions",
    expected_min_complexity: 2.5, // "refactor" maps to 3.0
    expect_structured_thinking: true,
};

// ─── Test 1: Complexity detection ────────────────────────────────────────────

#[test]
fn test_mips_interpreter_complexity_detection() {
    let complexity = StrategySelector::detect_complexity(MIPS_INTERPRETER.prompt);

    // "Implement" maps to 2.5 in the keyword table — at minimum it should exceed
    // the simple "fix" case (1.5). Multiple relevant keywords ("support", "include")
    // may push it higher.
    assert!(
        complexity >= 2.0,
        "MIPS interpreter should score at least moderate complexity, got {complexity:.2}"
    );
}

// ─── Test 2: Strategy selection for MIPS interpreter ─────────────────────────

#[test]
fn test_mips_interpreter_strategy_selection() {
    let complexity = StrategySelector::detect_complexity(MIPS_INTERPRETER.prompt);

    // With unknown quality (no LLM response yet) at moderate confidence,
    // complex prompts should still get structured thinking.
    let selector = StrategySelector::new();
    let strategy = selector.select(complexity, &unknown_quality(), 75);

    assert!(
        strategy.requires_structured_thinking(),
        "MIPS interpreter (complexity {complexity:.2}) should require structured thinking, got {strategy:?}"
    );

    // With high quality response at high confidence, the strategy should improve
    // but still be appropriate for the complexity level.
    let strategy_hq = selector.select(complexity, &high_quality(), 90);
    // Either way, the key invariant: complexity > simple tasks.
    assert!(
        strategy_hq.requires_structured_thinking() || !MIPS_INTERPRETER.expect_structured_thinking,
        "Consistency check: high quality should not downgrade below expectation"
    );
}

// ─── Test 3: Full pipeline (schema + guidance + strategy) ────────────────────

#[test]
fn test_mips_interpreter_full_pipeline() {
    let complexity = StrategySelector::detect_complexity(MIPS_INTERPRETER.prompt);
    let selector = StrategySelector::new();
    let strategy = selector.select(complexity, &unknown_quality(), 75);

    assert!(
        strategy.requires_structured_thinking(),
        "Strategy should require structured thinking: {strategy:?}"
    );

    // Verify tool schema is well-formed.
    let schema = rustycode_orchestration::StructuredThinkingToolSchema::schema();
    assert_eq!(schema["type"], "function");
    assert_eq!(schema["function"]["name"], "structured_thinking");
    assert!(schema["function"]["parameters"]["properties"]["thought"].is_object());
    assert!(schema["function"]["parameters"]["properties"]["phase"].is_object());
    assert!(schema["function"]["parameters"]["properties"]["type"].is_object());

    // Verify system prompt guidance is non-empty and references key concepts.
    let guidance = rustycode_orchestration::StructuredThinkingToolSchema::system_prompt_guidance();
    assert!(!guidance.is_empty());
    assert!(guidance.contains("structured_thinking"));
}

// ─── Test 4: Thought persistence across phases ───────────────────────────────

#[test]
fn test_mips_interpreter_thought_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let store = ReasoningStore::new(dir.path().to_path_buf());
    let task_id = "mips-interpreter-001";

    let thought1 = StructuredThought::new(
        "MIPS interpreter needs: instruction decoder, register file (32x i32), \
         memory (byte-addressable), and ALU. Use a fetch-decode-execute cycle."
            .to_string(),
        1,
        rustycode_orchestration::types::ThoughtType::Decision,
    );
    store.store_thought(task_id, 1, &thought1).unwrap();

    let thought2 = StructuredThought::new(
        "R-type instructions: opcode(6) rs(5) rt(5) rd(5) shamt(5) funct(6). \
         I-type: opcode(6) rs(5) rt(5) imm(16). J-type: opcode(6) addr(26)."
            .to_string(),
        1,
        rustycode_orchestration::types::ThoughtType::Constraint,
    );
    store.store_thought(task_id, 1, &thought2).unwrap();

    let thought3 = StructuredThought::new(
        "Verified: ADD, ADDI, LW, SW, BEQ, J cover the core instruction set. \
         Edge cases: overflow on signed add, misaligned memory access, branch delay slots."
            .to_string(),
        2,
        rustycode_orchestration::types::ThoughtType::Validation,
    );
    store.store_thought(task_id, 2, &thought3).unwrap();

    let context = store.context_for_next_phase(task_id, 3).unwrap();
    assert_eq!(context["phase"], 3);
    assert!(
        context["previous_summary"]["decisions_made"].is_array(),
        "Should have accumulated decisions"
    );
}

// ─── Test 5: Complexity spectrum across task types ───────────────────────────

#[test]
fn test_complexity_spectrum() {
    let cases = [
        (&MIPS_INTERPRETER, "MIPS interpreter"),
        (&FIX_TYPO, "fix typo"),
        (&REFACTOR_AUTH, "refactor auth"),
    ];

    for (case, label) in &cases {
        let complexity = StrategySelector::detect_complexity(case.prompt);
        assert!(
            complexity >= case.expected_min_complexity,
            "{label}: complexity {complexity:.2} < {:.2}",
            case.expected_min_complexity
        );

        let selector = StrategySelector::new();
        let strategy = selector.select(complexity, &unknown_quality(), 75);

        if case.expect_structured_thinking {
            assert!(
                strategy.requires_structured_thinking(),
                "{label}: expected structured thinking, got {strategy:?}"
            );
        }

        println!(
            "[{label}] complexity={complexity:.2} strategy={strategy:?} structured_thinking={}",
            strategy.requires_structured_thinking()
        );
    }
}

// ─── Test 6: Strategy differentiation (simple vs complex) ────────────────────

#[test]
fn test_strategy_differentiation() {
    let selector = StrategySelector::new();

    // Simple task with HIGH confidence + HIGH quality → should get DirectExecution.
    let simple_complexity = StrategySelector::detect_complexity(FIX_TYPO.prompt);
    let simple_strategy = selector.select(simple_complexity, &high_quality(), 95);
    assert!(
        !simple_strategy.requires_structured_thinking(),
        "Simple task (complexity {simple_complexity:.2}) with high quality + confidence \
         should not need structured thinking, got {simple_strategy:?}"
    );

    // Complex task with unknown quality → should get structured thinking.
    let complex_complexity = StrategySelector::detect_complexity(MIPS_INTERPRETER.prompt);
    let complex_strategy = selector.select(complex_complexity, &unknown_quality(), 75);
    assert!(
        complex_strategy.requires_structured_thinking(),
        "Complex task (complexity {complex_complexity:.2}) should need structured thinking, \
         got {complex_strategy:?}"
    );

    // Invariant: complex task should always score higher than simple task.
    assert!(
        complex_complexity > simple_complexity,
        "Complex complexity ({complex_complexity:.2}) should exceed simple ({simple_complexity:.2})"
    );
}

// ─── Test 7: Guidance text contains key concepts ─────────────────────────────

#[test]
fn test_guidance_contains_mips_relevant_instructions() {
    let guidance = rustycode_orchestration::StructuredThinkingToolSchema::system_prompt_guidance();
    assert!(guidance.contains("phase"), "Guidance should mention phases");
    assert!(
        guidance.contains("confidence"),
        "Guidance should mention confidence"
    );
    assert!(
        guidance.contains("thought"),
        "Guidance should mention thought types"
    );
}

// ─── Test 8: Quality score range validation on LLM responses ─────────────────

#[test]
fn test_quality_score_range() {
    let detector = QualityDetector::new();

    // QualityDetector is designed for LLM RESPONSES, not prompts.
    // Feed it realistic response-like text to validate scoring.
    let responses = [
        // Good response: specific, detailed, complete.
        "The MIPS interpreter uses a fetch-decode-execute cycle. \
         R-type format: opcode(6) rs(5) rt(5) rd(5) shamt(5) funct(6). \
         The ADD instruction performs rs + rt and stores in rd. \
         Edge cases: signed overflow triggers an exception when OF=1. \
         This approach was chosen because it separates concerns cleanly.",
        // Minimal response: short, vague.
        "Use a switch statement for opcodes.",
        // Detailed refactor response.
        "To refactor auth to JWT: 1) Replace session middleware with JWT validation middleware. \
         2) Add token generation in the login handler using HS256. \
         3) Implement refresh token rotation for security. \
         4) Backward compatibility: detect session cookie vs JWT in auth header, \
         route accordingly. Limitations: legacy clients need migration period.",
    ];

    for response in responses {
        let score = detector.evaluate(response);
        assert!(
            score.total >= 0.0,
            "score should be non-negative: {score:?}"
        );
    }

    let detailed = detector.evaluate(responses[0]);
    let minimal = detector.evaluate(responses[1]);
    assert!(
        detailed.total > minimal.total,
        "Detailed response ({:.2}) should outscore minimal ({:.2})",
        detailed.total,
        minimal.total
    );
}
