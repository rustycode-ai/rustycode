//! Intent classification for agent behavior adjustment.
//!
//! IntentGate classifies user prompts into intent categories using keyword heuristics (no LLM call).
//! This prevents common misinterpretation failures like implementing when
//! asked to explain, or explaining when asked to fix.

use crate::modes::WorkingMode;

/// Classification of the user's intent from their prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum IntentCategory {
    /// User wants code written, a feature built, or a file created
    Implementation,
    /// User wants to understand, explore, or research something (read-only)
    Investigation,
    /// User wants a concept explained or a question answered
    Explanation,
    /// User wants existing code restructured without changing behavior
    Refactoring,
    /// User wants an architectural plan or approach designed
    Planning,
    /// User wants tests written or existing tests run
    Testing,
    /// User wants performance tuning or deep code analysis
    Analytical,
    /// User wants to troubleshoot or fix bugs
    Diagnostic,
}

/// Intent classification with a confidence score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntentAssessment {
    /// The detected intent category.
    pub category: IntentCategory,
    /// Confidence in the classification, from 0.0 to 1.0.
    pub confidence: f64,
}

impl IntentAssessment {
    /// Get the recommended working mode for this assessment.
    pub fn recommended_mode(&self) -> WorkingMode {
        self.category.recommended_mode()
    }
}

impl IntentCategory {
    /// Map intent to the best WorkingMode.
    pub fn recommended_mode(&self) -> WorkingMode {
        match self {
            IntentCategory::Implementation => WorkingMode::Code,
            IntentCategory::Investigation => WorkingMode::Debug,
            IntentCategory::Explanation => WorkingMode::Ask,
            IntentCategory::Refactoring => WorkingMode::Code,
            IntentCategory::Planning => WorkingMode::Plan,
            IntentCategory::Testing => WorkingMode::Test,
            IntentCategory::Analytical => WorkingMode::Debug,
            IntentCategory::Diagnostic => WorkingMode::Debug,
        }
    }

    /// Stable key for config lookup and routing overrides.
    pub const fn as_key(&self) -> &'static str {
        match self {
            IntentCategory::Implementation => "implementation",
            IntentCategory::Investigation => "investigation",
            IntentCategory::Explanation => "explanation",
            IntentCategory::Refactoring => "refactoring",
            IntentCategory::Planning => "planning",
            IntentCategory::Testing => "testing",
            IntentCategory::Analytical => "analytical",
            IntentCategory::Diagnostic => "diagnostic",
        }
    }

    /// Produce a short instruction suffix to append to the system prompt
    /// that makes the agent aware of the detected intent.
    pub fn prompt_suffix(&self) -> &'static str {
        match self {
            IntentCategory::Implementation => {
                "The user wants code written or a feature built. Use tools (write_file, bash) \
                 to create or modify files. Prefer writing code over explaining it in text.\n\
                 \n\
                 Before writing, decode what the user actually needs: their request is \
                 surface-level. Check: does the codebase already have an API for this? \
                 What would success look like? What is the minimum change that achieves it?\n\
                 \n\
                 For changes touching multiple files: grep for the pattern across the codebase \
                 first to understand scope. Edit source files, not test files. Apply the same \
                 pattern consistently across all affected files."
            }
            IntentCategory::Investigation => {
                "The user wants to investigate or debug something. Use read-only tools \
                 (read_file, grep, bash for inspection). Report findings clearly. \
                 Do not modify files unless explicitly asked.\n\
                 \n\
                 Before investigating, decode what the user actually needs: the stated \
                 problem may be a symptom of a different root cause. Trace the actual \
                 failure path — follow imports, check call chains, read the error output. \
                 The error message may name module A but the real problem is in module B.\n\
                 \n\
                 When tracing: grep broadly first, then read specific files. Estimate \
                 scope early — does this affect one function or the entire module? \
                 Report which files are affected and how they connect."
            }
            IntentCategory::Explanation => {
                "The user is asking for an explanation or answer. Focus on clear, \
                 concise responses. Do not modify any files unless explicitly asked.\n\
                 \n\
                 Before explaining, decode what they really want to know: are they \
                 asking about a concept, a specific code path, or debugging guidance? \
                 Point to relevant code locations when helpful."
            }
            IntentCategory::Refactoring => {
                "The user wants code restructured without changing behavior. \
                 Use tools to read and rewrite files. Preserve all existing \
                 functionality and tests.\n\
                 \n\
                 Before refactoring, estimate scope: how many files reference what's changing? \
                 Grep the ENTIRE codebase first. Count references. If >5 files, plan a \
                 systematic pass: edit all source files, then verify imports resolve.\n\
                 \n\
                 Edit SOURCE files (not test files) unless tests need updating too. \
                 After each batch of edits, grep again for any missed references. \
                 Run tests after all edits to confirm no behavior changed."
            }
            IntentCategory::Planning => {
                "The user wants an architectural plan or design. Analyze the codebase \
                 using read-only tools, then produce a structured plan. \
                 Do not modify files unless explicitly asked.\n\
                 \n\
                 Before planning, find existing patterns that solve similar problems. \
                 Check: does the codebase already have a plugin/config/registration pattern? \
                 Propose the approach that follows existing conventions."
            }
            IntentCategory::Testing => {
                "The user wants tests written or run. Use tools to read existing code, \
                 write test files, and run the test suite. Verify tests pass.\n\
                 \n\
                 Before writing tests, understand what behavior to verify. Read the \
                 implementation first. Write tests that exercise edge cases, not just \
                 the happy path. Follow the project's existing test patterns."
            }
            IntentCategory::Analytical => {
                "The user wants analytical work (performance tuning, audit). Use \
                 analysis tools, profile code if needed, and report findings \
                 with data/metrics.\n\
                 \n\
                 Before analyzing, understand the baseline: what's the current behavior? \
                 Measure before and after. Report specific numbers, not vague assessments."
            }
            IntentCategory::Diagnostic => {
                "The user wants to fix a bug. Before fixing, decode the real intent:\n\
                 1. What does the error/test actually check? (the test IS the specification)\n\
                 2. What function/module does it exercise? Trace imports, don't guess.\n\
                 3. Why does it fail? Identify root cause: wrong logic, missing param, wrong module.\n\
                 4. Estimate scope: is this a 1-file fix or a multi-file refactor?\n\
                 5. What exact change fixes it? Name the file and line before editing.\n\
                 \n\
                 For multi-file changes: grep the ENTIRE codebase for the pattern first.\n\
                 Edit SOURCE files, not test files. After fixing one file, grep for remaining references.\n\
                 \n\
                 Common traps: error names module A but bug is in B; issue says 'fix X' \
                 but test checks Y; multiple similar files exist — grep the EXACT import; \
                 editing test files when source files need the fix. \
                 Fix the ROOT CAUSE, not the symptom. Verify with tests."
            }
        }
    }
}

/// Strong-signal prefixes that indicate a specific intent.
const EXPLANATION_PREFIXES: &[&str] = &[
    "explain",
    "what is",
    "what are",
    "what does",
    "what do",
    "why does",
    "why do",
    "why is",
    "how does",
    "how do",
    "how is",
    "describe",
    "tell me about",
    "define",
    "can you explain",
    "help me understand",
];

const IMPLEMENTATION_KEYWORDS: &[&str] = &[
    "create",
    "build",
    "implement",
    "add",
    "develop",
    "write a",
    "make a",
    "new file",
    "new function",
    "new module",
    "generate",
    "scaffold",
    "set up",
    "install",
];

const REFACTORING_KEYWORDS: &[&str] = &[
    "refactor",
    "restructure",
    "reorganize",
    "rename",
    "clean up",
    "simplify",
    "extract",
    "move",
    "consolidate",
    "deduplicate",
];

const TESTING_KEYWORDS: &[&str] = &[
    "test",
    "spec",
    "coverage",
    "unit test",
    "integration test",
    "e2e test",
    "write tests",
    "add tests",
    "run tests",
    "check that",
    "verify that",
];

const PLANNING_KEYWORDS: &[&str] = &[
    "plan",
    "architect",
    "design the",
    "roadmap",
    "how should",
    "approach for",
    "strategy for",
    "propose",
    "evaluate options",
];

const INVESTIGATION_KEYWORDS: &[&str] = &[
    "Find",
    "Search",
    "where is",
    "find",
    "locate",
    "list all",
    "show me",
    "investigate",
    "debug",
    "fix",
    "error",
    "bug",
    "issue",
    "broken",
    "not working",
    "failing",
    "trace",
    "diagnose",
];

/// Classify a user prompt into an IntentCategory using keyword heuristics.
/// No LLM call — pure string matching, runs in microseconds.
pub fn classify_intent(prompt: &str) -> IntentCategory {
    classify_intent_with_confidence(prompt).category
}

/// Classify a user prompt into an intent category and confidence score.
pub fn classify_intent_with_confidence(prompt: &str) -> IntentAssessment {
    let lower = prompt.to_lowercase();
    let trimmed = lower.trim();

    // Pass 1: Strong signal prefixes (explanation is most distinctive)
    for prefix in EXPLANATION_PREFIXES {
        if trimmed.starts_with(prefix) {
            return IntentAssessment {
                category: IntentCategory::Explanation,
                confidence: 0.94,
            };
        }
    }

    // Pass 1: Strong signal keywords — count matches per category
    let impl_score = count_matches(&lower, IMPLEMENTATION_KEYWORDS);
    let refactor_score = count_matches(&lower, REFACTORING_KEYWORDS);
    let test_score = count_matches(&lower, TESTING_KEYWORDS);
    let plan_score = count_matches(&lower, PLANNING_KEYWORDS);
    let invest_score = count_matches(&lower, INVESTIGATION_KEYWORDS);

    let scores = [
        (invest_score, IntentCategory::Investigation),
        (plan_score, IntentCategory::Planning),
        (test_score, IntentCategory::Testing),
        (refactor_score, IntentCategory::Refactoring),
        (impl_score, IntentCategory::Implementation),
    ];

    // On ties, prefer more specific intents (later in array wins)
    let (best_score, best_intent) = scores
        .into_iter()
        .max_by_key(|(score, _)| *score)
        .unwrap_or((0, IntentCategory::Implementation));

    let confidence = if best_score == 0 {
        0.40
    } else {
        let total_score = impl_score + refactor_score + test_score + plan_score + invest_score;
        let mut confidence = (best_score as f64).mul_add(0.12, 0.56);
        if total_score > best_score {
            confidence -= 0.05;
        }
        confidence.clamp(0.40, 0.96)
    };

    IntentAssessment {
        category: if best_score > 0 {
            best_intent
        } else {
            // Default: implementation is the safest default for a coding agent
            IntentCategory::Implementation
        },
        confidence,
    }
}

/// Count how many keywords from the list appear in the text.
fn count_matches(text: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|kw| text.contains(**kw)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explanation_intent() {
        assert_eq!(
            classify_intent("explain what the main function does"),
            IntentCategory::Explanation
        );
        assert_eq!(
            classify_intent("What is a Result type?"),
            IntentCategory::Explanation
        );
        assert_eq!(
            classify_intent("Why does this compile?"),
            IntentCategory::Explanation
        );
        assert_eq!(
            classify_intent("How does the event loop work?"),
            IntentCategory::Explanation
        );
        assert_eq!(
            classify_intent("describe the architecture"),
            IntentCategory::Explanation
        );
    }

    #[test]
    fn test_implementation_intent() {
        assert_eq!(
            classify_intent("create a new HTTP client module"),
            IntentCategory::Implementation
        );
        assert_eq!(
            classify_intent("implement user authentication"),
            IntentCategory::Implementation
        );
        assert_eq!(
            classify_intent("build a REST API for todos"),
            IntentCategory::Implementation
        );
        assert_eq!(
            classify_intent("add logging to the parser"),
            IntentCategory::Implementation
        );
    }

    #[test]
    fn test_refactoring_intent() {
        assert_eq!(
            classify_intent("refactor the database layer"),
            IntentCategory::Refactoring
        );
        assert_eq!(
            classify_intent("reorganize the module structure"),
            IntentCategory::Refactoring
        );
        assert_eq!(
            classify_intent("rename parse_config to load_config"),
            IntentCategory::Refactoring
        );
    }

    #[test]
    fn test_testing_intent() {
        assert_eq!(
            classify_intent("write tests for the parser"),
            IntentCategory::Testing
        );
        assert_eq!(
            classify_intent("add test coverage for auth module"),
            IntentCategory::Testing
        );
        assert_eq!(
            classify_intent("run tests and fix failures"),
            IntentCategory::Testing
        );
    }

    #[test]
    fn test_planning_intent() {
        assert_eq!(
            classify_intent("plan the migration to async"),
            IntentCategory::Planning
        );
        assert_eq!(
            classify_intent("design the architecture for the payment system"),
            IntentCategory::Planning
        );
        assert_eq!(
            classify_intent("how should we approach the database migration?"),
            IntentCategory::Planning
        );
    }

    #[test]
    fn test_investigation_intent() {
        assert_eq!(
            classify_intent("find where the auth token is validated"),
            IntentCategory::Investigation
        );
        assert_eq!(
            classify_intent("debug the connection timeout issue"),
            IntentCategory::Investigation
        );
        assert_eq!(
            classify_intent("fix the failing CI build"),
            IntentCategory::Investigation
        );
        assert_eq!(
            classify_intent("the server is not working after deploy"),
            IntentCategory::Investigation
        );
    }

    #[test]
    fn test_default_to_implementation() {
        assert_eq!(
            classify_intent("make it faster"),
            IntentCategory::Implementation
        );
        assert_eq!(classify_intent("x"), IntentCategory::Implementation);
    }

    #[test]
    fn test_intent_assessment_confidence() {
        let assessment = classify_intent_with_confidence("explain how the parser works");
        assert_eq!(assessment.category, IntentCategory::Explanation);
        assert!(assessment.confidence > 0.9);
    }

    #[test]
    fn test_intent_assessment_ambiguous_confidence() {
        let assessment = classify_intent_with_confidence("help with this");
        assert_eq!(assessment.category, IntentCategory::Implementation);
        assert!(assessment.confidence < 0.6);
    }

    #[test]
    fn test_recommended_mode_mapping() {
        assert_eq!(
            IntentCategory::Implementation.recommended_mode(),
            WorkingMode::Code
        );
        assert_eq!(
            IntentCategory::Investigation.recommended_mode(),
            WorkingMode::Debug
        );
        assert_eq!(
            IntentCategory::Explanation.recommended_mode(),
            WorkingMode::Ask
        );
        assert_eq!(
            IntentCategory::Refactoring.recommended_mode(),
            WorkingMode::Code
        );
        assert_eq!(
            IntentCategory::Planning.recommended_mode(),
            WorkingMode::Plan
        );
        assert_eq!(
            IntentCategory::Testing.recommended_mode(),
            WorkingMode::Test
        );
    }

    #[test]
    fn test_prompt_suffix_not_empty() {
        let intents = [
            IntentCategory::Implementation,
            IntentCategory::Investigation,
            IntentCategory::Explanation,
            IntentCategory::Refactoring,
            IntentCategory::Planning,
            IntentCategory::Testing,
            IntentCategory::Analytical,
            IntentCategory::Diagnostic,
        ];
        for intent in intents {
            assert!(
                !intent.prompt_suffix().is_empty(),
                "{:?} has empty prompt suffix",
                intent
            );
        }
    }
}
