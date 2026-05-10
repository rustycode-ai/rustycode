//! AST system prompt template and output parser.
//!
//! Provides the system prompt for Adaptive Structured Thinking, a parser for
//! phase-delimited model output, and validation logic that enforces phase
//! ordering and skip rules.

use serde::{Deserialize, Serialize};

use super::types::{AstPhase, ComplexityLevel};
use rustycode_prompt::PromptResolver;

// Constants

/// The AST system prompt template.
///
/// Designed to stay well under 500 tokens. Instructs the model to follow the
/// six-phase pipeline and produce markdown headers for each phase.
pub const AST_SYSTEM_PROMPT: &str = "You are a task executor using Adaptive Structured Thinking (AST).

Follow the phases in order:
1. CLASSIFY — assess complexity, define success criteria
2. RESEARCH — gather context (optional for TRIVIAL tasks)
3. SKELETON — plan milestones with dependencies
4. EXPAND — break near-term milestones into atomic steps
5. EXECUTE — run steps with thinking off
6. VERIFY — check results against success criteria

Rules:
- Keep thinking bounded and phase-specific.
- Do not over-plan trivial work.
- Research before committing to a plan on anything non-trivial.
- For complex tasks, optionally run proposal selection inside SKELETON before choosing the milestone map.
- Expand only the near-term milestones.
- Execute with thinking off.
- Recover locally when a step fails.
- Verify against explicit success criteria.

Tool Selection by Phase:
- CLASSIFY: read_file, grep, glob — read-only
- RESEARCH: semantic_search, lsp_hover, lsp_definition, grep, web_fetch
- SKELETON: no tool calls — planning only
- EXPAND: todo_write (optional)
- EXECUTE: write_file, edit_file, bash, run_tests
- VERIFY: bash, run_tests, read_file

Complexity Routing:
- TRIVIAL: skip RESEARCH, compact SKELETON, direct EXECUTE
- MODERATE: all phases, bounded thinking
- COMPLEX: full phases, rolling-wave EXPAND

EXECUTION BOUNDARY (mandatory):
After the SKELETON phase is complete, you MUST begin writing files and executing steps. You may NOT continue planning, analyzing, or reasoning about the task. Analysis paralysis is a known failure mode — the transition from SKELETON to EXPAND/EXECUTE is a hard gate, not a soft suggestion.

Output each phase clearly using markdown headers.
";

/// Resolve the AST system prompt through the layering chain.
pub fn resolve_system_prompt(resolver: &PromptResolver) -> String {
    resolver.resolve("ast", "system", AST_SYSTEM_PROMPT)
}

/// Canonical phase ordering used for parsing and validation.
///
/// `Complete` and `Failed` are terminal states that never appear as `##`
/// headers in model output, so they are excluded here.
const CANONICAL_PHASES: [AstPhase; 6] = [
    AstPhase::Classify,
    AstPhase::Research,
    AstPhase::Skeleton,
    AstPhase::Expand,
    AstPhase::Execute,
    AstPhase::Verify,
];

/// Phase names that are valid as `##` headers in model output.
const VALID_PHASE_NAMES: [&str; 6] = [
    "CLASSIFY", "RESEARCH", "SKELETON", "EXPAND", "EXECUTE", "VERIFY",
];

// Types

/// Parsed AST output from a model response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedAstOutput {
    /// Ordered phases extracted from the output.
    pub phases: Vec<ParsedPhase>,
    /// Validation errors discovered during parsing / validation.
    pub validation_errors: Vec<String>,
    /// `true` when `validation_errors` is empty.
    pub is_valid: bool,
}

/// A single parsed phase from model output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPhase {
    /// Phase name (e.g. `"CLASSIFY"`).
    pub name: String,
    /// Content between this header and the next (or end of output).
    pub content: String,
    /// 1-based line number where the `## PHASE_NAME` header appears.
    pub line_number: usize,
}

// Parsing

/// Parse and validate AST output from a model response.
///
/// Scans for lines matching `## PHASE_NAME` at the start of a line, collects
/// the content between headers, then runs validation.
pub fn parse_ast_output(output: &str) -> ParsedAstOutput {
    let phases = extract_phases(output);
    let validation_errors = validate_phases(&phases);
    let is_valid = validation_errors.is_empty();

    ParsedAstOutput {
        phases,
        validation_errors,
        is_valid,
    }
}

/// Extract phases by scanning for `## PHASE_NAME` headers.
///
/// Content between headers belongs to the preceding phase. Any content
/// before the first header is discarded.
fn extract_phases(output: &str) -> Vec<ParsedPhase> {
    let mut phases: Vec<ParsedPhase> = Vec::new();

    for (line_idx, line) in output.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(phase_name) = parse_phase_header(trimmed) {
            phases.push(ParsedPhase {
                name: phase_name.to_owned(),
                content: String::new(),
                line_number: line_idx + 1, // 1-based
            });
        } else if let Some(last) = phases.last_mut() {
            // Append to the current phase's content.
            if !last.content.is_empty() {
                last.content.push('\n');
            }
            last.content.push_str(line);
        }
        // Lines before the first header are intentionally discarded.
    }

    // Trim trailing whitespace from each phase content.
    for phase in &mut phases {
        phase.content = phase.content.trim_end().to_owned();
    }

    phases
}

/// Check whether a trimmed line is a valid AST phase header.
///
/// A valid header is exactly `## PHASE_NAME` (two hashes, one space, then
/// one of the six canonical phase names). Returns the phase name on match.
fn parse_phase_header(line: &str) -> Option<&'static str> {
    let rest = line.strip_prefix("## ")?;
    VALID_PHASE_NAMES
        .iter()
        .find(|&&name| name == rest)
        .copied()
}

// Validation

/// Validate extracted phases without complexity context.
///
/// Checks:
/// - Only valid phase names appear as headers (enforced by extraction).
/// - No extra top-level `##` sections (non-phase headers).
/// - Phases appear in canonical order.
fn validate_phases(phases: &[ParsedPhase]) -> Vec<String> {
    let mut errors = Vec::new();

    // Check for extra `##` sections that are not valid phase headers.
    // This is already enforced by `extract_phases`, but we keep the slot
    // for clarity and future extension.

    // Check ordering: the index of each phase in `CANONICAL_PHASES` must
    // be strictly non-decreasing.
    let mut last_canonical_idx: Option<usize> = None;
    for phase in phases {
        let canonical_idx = canonical_index(&phase.name);
        match canonical_idx {
            Some(idx) => {
                if let Some(prev) = last_canonical_idx {
                    if idx < prev {
                        errors.push(format!(
                            "Phase {} appears after phase {} (wrong order)",
                            phase.name, CANONICAL_PHASES[prev]
                        ));
                    }
                }
                last_canonical_idx = Some(idx);
            }
            None => {
                // Should not happen because extraction only picks valid names,
                // but defensive coding.
                errors.push(format!("Unknown phase: {}", phase.name));
            }
        }
    }

    // Check for duplicate phases.
    let mut seen = std::collections::HashSet::new();
    for phase in phases {
        if seen.contains(&phase.name) {
            errors.push(format!("Duplicate phase: {}", phase.name));
        }
        seen.insert(&phase.name);
    }

    errors
}

/// Validate phase ordering against a known complexity level.
///
/// Additional rules on top of the basic ordering check:
/// - CLASSIFY must always be present.
/// - RESEARCH may be skipped for TRIVIAL tasks.
/// - SKELETON, EXPAND, EXECUTE, VERIFY must always be present.
pub fn validate_phase_order(phases: &[ParsedPhase], complexity: ComplexityLevel) -> Vec<String> {
    let mut errors = validate_phases(phases);

    let phase_names: Vec<&str> = phases.iter().map(|p| p.name.as_str()).collect();

    // Required phases for each complexity level.
    let required: &[&str] = match complexity {
        ComplexityLevel::Trivial => &["CLASSIFY", "SKELETON", "EXPAND", "EXECUTE", "VERIFY"],
        ComplexityLevel::Moderate | ComplexityLevel::Complex => &[
            "CLASSIFY", "RESEARCH", "SKELETON", "EXPAND", "EXECUTE", "VERIFY",
        ],
    };

    for &req in required {
        if !phase_names.contains(&req) {
            errors.push(format!("Missing required phase: {req}"));
        }
    }

    errors
}

/// Return the index of `name` in `CANONICAL_PHASES`, or `None`.
fn canonical_index(name: &str) -> Option<usize> {
    CANONICAL_PHASES.iter().position(|p| p.to_string() == name)
}

// Phase prompt builder

/// Build the prompt for a specific AST phase.
///
/// Produces a concise instruction telling the model what to output for the
/// given phase, parameterized by complexity and any accumulated context.
pub fn build_phase_prompt(phase: AstPhase, complexity: ComplexityLevel, context: &str) -> String {
    let complexity_str = match complexity {
        ComplexityLevel::Trivial => "TRIVIAL",
        ComplexityLevel::Moderate => "MODERATE",
        ComplexityLevel::Complex => "COMPLEX",
    };

    let phase_instruction = match phase {
        AstPhase::Classify => format!(
            "## CLASSIFY\n\
             - Task: <brief description>\n\
             - Complexity: {complexity_str}\n\
             - Success criteria: <list of measurable criteria>\n\
             - Route: <DirectExecute|StandardSequence|RollingWave>\n\
             \n\
             Assess the task and define success criteria."
        ),
        AstPhase::Research => "## RESEARCH\n\
             - Relevant files: <list of files>\n\
             - Patterns: <existing patterns found>\n\
             - Dependencies: <libraries or crates needed>\n\
             - Risks: <identified risks>\n\
             - Constraints: <known constraints>\n\
             \n\
             Gather context from the codebase before committing to a plan."
            .to_string(),
        AstPhase::Skeleton => {
            let bedd_section = if complexity == ComplexityLevel::Complex {
                "\n\
                 For complex tasks, include proposal selection:\n\
                 ### Proposals\n\
                 1. id: P1\n   approach: <description>\n   tradeoffs: <trade-offs>\n\
                 \n\
                 ### Evaluation\n\
                 - P1: feasibility=N risk=N alignment=N effort=N -> score=N\n\
                 \n\
                 ### Decision\n\
                 - selected: P1\n\
                 - reason: <why>"
            } else {
                ""
            };
            format!(
                "## SKELETON{bedd_section}\n\
                 \n\
                 Define milestones with dependency ordering.\n\
                 For each milestone:\n\
                 - M<N>: <description> -> depends_on: [<milestone ids>]"
            )
        }
        AstPhase::Expand => "\
            ## EXPAND\n\
            \n\
            Expand near-term milestones into atomic steps.\n\
            For each milestone:\n\
            ### Milestone <N>: <description>\n\
            1. <concrete step>\n\
            2. <concrete step>"
            .to_owned(),
        AstPhase::Execute => "\
            ## EXECUTE\n\
            \n\
            Execute each step. Report results inline.\n\
            1. <step description> -> <result>"
            .to_owned(),
        AstPhase::Verify => "\
            ## VERIFY\n\
            \n\
            Check results against each success criterion.\n\
            - Criterion <N>: <description> -> PASS|PARTIAL|FAIL\n\
            Overall: PASS|PARTIAL|FAIL"
            .to_owned(),
        AstPhase::Complete | AstPhase::Failed => {
            // Terminal phases are not prompted.
            String::new()
        }
    };

    let context_section = if context.is_empty() {
        String::new()
    } else {
        format!("\n\nContext from previous phases:\n{context}")
    };

    format!("{phase_instruction}{context_section}")
}

/// Build a phase prompt using the prompt resolver for the template content.
///
/// Falls back to the hardcoded `build_phase_prompt` output when no override
/// is found in the resolver chain.
pub fn build_phase_prompt_resolved(
    phase: AstPhase,
    complexity: ComplexityLevel,
    context: &str,
    resolver: &PromptResolver,
) -> String {
    let name = match phase {
        AstPhase::Classify => "classify",
        AstPhase::Research => "research",
        AstPhase::Skeleton => "skeleton",
        AstPhase::Expand => "expand",
        AstPhase::Execute => "execute",
        AstPhase::Verify => "verify",
        AstPhase::Complete | AstPhase::Failed => return String::new(),
    };

    let vars = serde_json::json!({
        "complexity": match complexity {
            ComplexityLevel::Trivial => "TRIVIAL",
            ComplexityLevel::Moderate => "MODERATE",
            ComplexityLevel::Complex => "COMPLEX",
        },
        "bedd_section": if complexity == ComplexityLevel::Complex {
            "\n\nFor complex tasks, include proposal selection:\n### Proposals\n1. id: P1\n   approach: <description>\n   tradeoffs: <trade-offs>\n\n### Evaluation\n- P1: feasibility=N risk=N alignment=N effort=N -> score=N\n\n### Decision\n- selected: P1\n- reason: <why>"
        } else {
            ""
        },
    });

    let template = resolver.resolve("ast/phases", name, "");
    let prompt = if template.is_empty() {
        build_phase_prompt(phase, complexity, context)
    } else {
        match resolver.render("ast/phases", name, "", &vars) {
            Ok(rendered) => {
                let context_section = if context.is_empty() {
                    String::new()
                } else {
                    format!("\n\nContext from previous phases:\n{context}")
                };
                format!("{rendered}{context_section}")
            }
            Err(_) => build_phase_prompt(phase, complexity, context),
        }
    };
    prompt
}

// Token estimation

/// Estimate the token count of a string.
///
/// Uses a simple heuristic: split on whitespace. Each word is approximately
/// one token. This overcounts relative to the ~4-chars-per-token BPE average,
/// making it a conservative upper bound for budget checking.
pub fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

// Extra section detection

/// Detect non-phase `##` headers in raw output text.
///
/// Returns a list of (`header_text`, `line_number`) for any `##` lines that are
/// not valid AST phase headers. Used by the parser to populate validation
/// errors.
pub fn detect_extra_sections(output: &str) -> Vec<(String, usize)> {
    let mut extras = Vec::new();

    for (line_idx, line) in output.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("## ") {
            if !VALID_PHASE_NAMES.contains(&rest) {
                extras.push((rest.to_owned(), line_idx + 1));
            }
        }
    }

    extras
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    // -- Token estimation ----------------------------------------------------

    #[test]
    fn system_prompt_under_500_tokens() {
        let tokens = estimate_tokens(AST_SYSTEM_PROMPT);
        assert!(
            tokens < 500,
            "AST_SYSTEM_PROMPT estimated at {tokens} tokens, must be < 500"
        );
    }

    #[test]
    fn test_ast_prompt_includes_phase_tool_guidance() {
        assert!(AST_SYSTEM_PROMPT.contains("CLASSIFY"));
        assert!(AST_SYSTEM_PROMPT.contains("EXECUTE"));
        assert!(AST_SYSTEM_PROMPT.contains("Complexity Routing"));
    }

    // -- Phase header parsing ------------------------------------------------

    #[test]
    fn parse_header_valid() {
        assert_eq!(parse_phase_header("## CLASSIFY"), Some("CLASSIFY"));
        assert_eq!(parse_phase_header("## RESEARCH"), Some("RESEARCH"));
        assert_eq!(parse_phase_header("## SKELETON"), Some("SKELETON"));
        assert_eq!(parse_phase_header("## EXPAND"), Some("EXPAND"));
        assert_eq!(parse_phase_header("## EXECUTE"), Some("EXECUTE"));
        assert_eq!(parse_phase_header("## VERIFY"), Some("VERIFY"));
    }

    #[test]
    fn parse_header_invalid() {
        assert_eq!(parse_phase_header("### CLASSIFY"), None);
        assert_eq!(parse_phase_header("# CLASSIFY"), None);
        assert_eq!(parse_phase_header("CLASSIFY"), None);
        assert_eq!(parse_phase_header("## COMPLETE"), None);
        assert_eq!(parse_phase_header("## FAILED"), None);
        assert_eq!(parse_phase_header("## UNKNOWN"), None);
        assert_eq!(parse_phase_header(""), None);
    }

    #[test]
    fn parse_header_whitespace() {
        assert_eq!(
            parse_phase_header("  ## CLASSIFY"),
            None,
            "leading spaces are not trimmed by header parser"
        );
        assert_eq!(
            parse_phase_header("##  CLASSIFY"),
            None,
            "double space not allowed"
        );
    }

    // -- Full output parsing -------------------------------------------------

    #[test]
    fn parse_valid_trivial_output() {
        let output = r"## CLASSIFY
- Task: Fix typo in README.md
- Complexity: TRIVIAL
- Success criteria: File changed correctly
- Route: DirectExecute

## SKELETON
- Milestones: [M1: Fix typo]

## EXPAND
### Milestone 1: Fix typo
1. Edit README.md to fix the typo

## EXECUTE
1. Edit README.md -> exit=0

## VERIFY
- Criterion 1: File changed correctly -> PASS
Overall: PASS
";

        let parsed = parse_ast_output(output);
        assert!(
            parsed.is_valid,
            "Validation errors: {:?}",
            parsed.validation_errors
        );
        assert_eq!(parsed.phases.len(), 5);

        assert_eq!(parsed.phases[0].name, "CLASSIFY");
        assert!(parsed.phases[0].content.contains("TRIVIAL"));
        assert_eq!(parsed.phases[0].line_number, 1);

        assert_eq!(parsed.phases[1].name, "SKELETON");
        assert!(parsed.phases[1].content.contains("M1"));

        assert_eq!(parsed.phases[2].name, "EXPAND");
        assert!(parsed.phases[2].content.contains("Milestone 1"));

        assert_eq!(parsed.phases[3].name, "EXECUTE");
        assert!(parsed.phases[3].content.contains("exit=0"));

        assert_eq!(parsed.phases[4].name, "VERIFY");
        assert!(parsed.phases[4].content.contains("PASS"));
    }

    #[test]
    fn parse_valid_moderate_output() {
        let output = r"## CLASSIFY
- Task: Add unit tests for auth module
- Complexity: MODERATE
- Success criteria: Tests pass, Coverage >= 80%
- Route: StandardSequence

## RESEARCH
- Relevant files: src/auth/*.rs
- Patterns: existing test patterns in tests/auth_test.rs
- Dependencies: tokio, mockall
- Risks: async test patterns
- Constraints: must not modify production code

## SKELETON
- M1: Setup test fixtures -> depends_on: []
- M2: Write unit tests -> depends_on: [M1]
- M3: Verify coverage -> depends_on: [M2]

## EXPAND
### Milestone 1: Setup test fixtures
1. Create test/fixtures/mod.rs
2. Add mock implementations

### Milestone 2: Write unit tests
1. Test login success case
2. Test login failure case

## EXECUTE
1. Create test/fixtures/mod.rs -> exit=0
2. Add mock implementations -> exit=0
3. Test login success -> exit=0

## VERIFY
- Criterion 1: Tests pass -> PASS
- Criterion 2: Coverage >= 80% -> PASS
Overall: PASS
";

        let parsed = parse_ast_output(output);
        assert!(
            parsed.is_valid,
            "Validation errors: {:?}",
            parsed.validation_errors
        );
        assert_eq!(parsed.phases.len(), 6);
        assert_eq!(parsed.phases[0].name, "CLASSIFY");
        assert_eq!(parsed.phases[1].name, "RESEARCH");
        assert_eq!(parsed.phases[2].name, "SKELETON");
        assert_eq!(parsed.phases[3].name, "EXPAND");
        assert_eq!(parsed.phases[4].name, "EXECUTE");
        assert_eq!(parsed.phases[5].name, "VERIFY");
    }

    #[test]
    fn parse_valid_complex_bedd_output() {
        let output = r"## CLASSIFY
- Task: Implement JWT authentication with refresh tokens
- Complexity: COMPLEX
- Success criteria: Auth flow works, Token refresh works
- Route: RollingWave

## RESEARCH
- Relevant files: src/auth/*.rs, src/middleware/*.rs
- Patterns: existing middleware chain
- Dependencies: jsonwebtoken crate
- Risks: security implications, token storage
- Constraints: must support refresh token rotation

## SKELETON
### Proposals
1. id: P1
   approach: Full JWT library integration
   tradeoffs: battle-tested but heavier

### Evaluation
- P1: feasibility=8 risk=3 alignment=9 effort=7 -> score=7.5

### Decision
- selected: P1
- reason: higher feasibility and lower risk

### Milestones
1. M1: Add JWT dependencies -> depends_on: []
2. M2: Implement token generation -> depends_on: [M1]

## EXPAND
### Milestone 1: Add JWT dependencies
1. Add jsonwebtoken to Cargo.toml

### Milestone 2: Implement token generation
1. Create src/auth/jwt.rs

## EXECUTE
1. Add jsonwebtoken -> exit=0
2. Create jwt.rs -> exit=0

## VERIFY
- Criterion 1: Auth flow works -> PASS
- Criterion 2: Token refresh works -> PASS
Overall: PASS
";

        let parsed = parse_ast_output(output);
        assert!(
            parsed.is_valid,
            "Validation errors: {:?}",
            parsed.validation_errors
        );
        assert_eq!(parsed.phases.len(), 6);

        // Verify the BEDD content is captured inside SKELETON.
        let skeleton = &parsed.phases[2];
        assert_eq!(skeleton.name, "SKELETON");
        assert!(skeleton.content.contains("Proposals"));
        assert!(skeleton.content.contains("P1"));
        assert!(skeleton.content.contains("score=7.5"));
        assert!(skeleton.content.contains("selected: P1"));
    }

    // -- Validation: wrong phase order ----------------------------------------

    #[test]
    fn validation_detects_wrong_phase_order() {
        let output = "\
## EXECUTE
1. Do thing -> exit=0

## CLASSIFY
- Task: Something
- Complexity: TRIVIAL
- Route: DirectExecute
";

        let parsed = parse_ast_output(output);
        assert!(!parsed.is_valid);
        assert!(
            parsed
                .validation_errors
                .iter()
                .any(|e| e.contains("wrong order")),
            "Expected wrong-order error, got: {:?}",
            parsed.validation_errors
        );
    }

    // -- Validation: missing required phases ----------------------------------

    #[test]
    fn validation_detects_missing_phases_for_moderate() {
        // Only CLASSIFY present.
        let output = "\
## CLASSIFY
- Task: Something
- Complexity: MODERATE
";

        let parsed = parse_ast_output(output);
        // Basic parse should succeed (no ordering issue with one phase).
        assert!(parsed.is_valid, "Single phase should parse validly");

        // But phase-order validation with MODERATE should flag missing phases.
        let errors = validate_phase_order(&parsed.phases, ComplexityLevel::Moderate);
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: RESEARCH")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: SKELETON")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: EXPAND")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: EXECUTE")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: VERIFY")));
    }

    // -- Validation: RESEARCH skip allowed for TRIVIAL ------------------------

    #[test]
    fn validation_allows_research_skip_for_trivial() {
        let output = "\
## CLASSIFY
- Task: Fix typo
- Complexity: TRIVIAL
- Route: DirectExecute

## SKELETON
- M1: Fix typo -> depends_on: []

## EXPAND
### Milestone 1: Fix typo
1. Edit file

## EXECUTE
1. Edit file -> exit=0

## VERIFY
- Criterion 1: Fixed -> PASS
Overall: PASS
";

        let parsed = parse_ast_output(output);
        assert!(
            parsed.is_valid,
            "Validation errors: {:?}",
            parsed.validation_errors
        );

        let errors = validate_phase_order(&parsed.phases, ComplexityLevel::Trivial);
        assert!(
            errors.is_empty(),
            "TRIVIAL should allow RESEARCH skip, got: {errors:?}"
        );
    }

    #[test]
    fn validation_rejects_research_skip_for_moderate() {
        let output = "\
## CLASSIFY
- Task: Add tests
- Complexity: MODERATE

## SKELETON
- M1: Add tests

## EXPAND
1. Add test

## EXECUTE
1. Run test

## VERIFY
Overall: PASS
";

        let parsed = parse_ast_output(output);
        let errors = validate_phase_order(&parsed.phases, ComplexityLevel::Moderate);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Missing required phase: RESEARCH")),
            "MODERATE should require RESEARCH, got: {errors:?}"
        );
    }

    // -- Validation: extra sections rejected ----------------------------------

    #[test]
    fn validation_rejects_extra_sections() {
        let output = "\
## CLASSIFY
- Task: Something

## RESEARCH
- Files: src/main.rs

## DEPLOY
- Deploy to production

## SKELETON
- M1: Build

## EXPAND
1. Step

## EXECUTE
1. Step

## VERIFY
Overall: PASS
";

        let extras = detect_extra_sections(output);
        assert_eq!(extras.len(), 1);
        assert_eq!(extras[0].0, "DEPLOY");
    }

    // -- Phase prompt builder -------------------------------------------------

    #[test]
    fn build_phase_prompt_classify() {
        let prompt = build_phase_prompt(AstPhase::Classify, ComplexityLevel::Moderate, "");
        assert!(prompt.contains("CLASSIFY"));
        assert!(prompt.contains("MODERATE"));
        assert!(prompt.contains("Success criteria"));
    }

    #[test]
    fn build_phase_prompt_research() {
        let prompt = build_phase_prompt(AstPhase::Research, ComplexityLevel::Complex, "");
        assert!(prompt.contains("RESEARCH"));
        assert!(prompt.contains("Relevant files"));
    }

    #[test]
    fn build_phase_prompt_skeleton_complex_includes_bedd() {
        let prompt = build_phase_prompt(AstPhase::Skeleton, ComplexityLevel::Complex, "");
        assert!(prompt.contains("Proposals"));
        assert!(prompt.contains("Evaluation"));
        assert!(prompt.contains("Decision"));
    }

    #[test]
    fn build_phase_prompt_skeleton_moderate_no_bedd() {
        let prompt = build_phase_prompt(AstPhase::Skeleton, ComplexityLevel::Moderate, "");
        assert!(!prompt.contains("Proposals"));
        assert!(!prompt.contains("Evaluation"));
    }

    #[test]
    fn build_phase_prompt_expand() {
        let prompt = build_phase_prompt(AstPhase::Expand, ComplexityLevel::Trivial, "");
        assert!(prompt.contains("EXPAND"));
        assert!(prompt.contains("atomic steps"));
    }

    #[test]
    fn build_phase_prompt_execute() {
        let prompt = build_phase_prompt(AstPhase::Execute, ComplexityLevel::Trivial, "");
        assert!(prompt.contains("EXECUTE"));
    }

    #[test]
    fn build_phase_prompt_verify() {
        let prompt = build_phase_prompt(AstPhase::Verify, ComplexityLevel::Trivial, "");
        assert!(prompt.contains("VERIFY"));
        assert!(prompt.contains("PASS"));
    }

    #[test]
    fn build_phase_prompt_with_context() {
        let prompt = build_phase_prompt(
            AstPhase::Execute,
            ComplexityLevel::Trivial,
            "Previous phase produced 3 steps.",
        );
        assert!(prompt.contains("Context from previous phases"));
        assert!(prompt.contains("Previous phase produced 3 steps"));
    }

    #[test]
    fn build_phase_prompt_terminal_phases_empty() {
        assert!(build_phase_prompt(AstPhase::Complete, ComplexityLevel::Trivial, "").is_empty());
        assert!(build_phase_prompt(AstPhase::Failed, ComplexityLevel::Trivial, "").is_empty());
    }

    // -- Edge cases ----------------------------------------------------------

    #[test]
    fn parse_empty_output() {
        let parsed = parse_ast_output("");
        assert!(parsed.phases.is_empty());
        assert!(parsed.is_valid);
    }

    #[test]
    fn parse_single_phase() {
        let parsed = parse_ast_output("## CLASSIFY\n- Task: Something\n");
        assert_eq!(parsed.phases.len(), 1);
        assert_eq!(parsed.phases[0].name, "CLASSIFY");
        assert!(parsed.is_valid);
    }

    #[test]
    fn parse_no_phases_found() {
        let parsed = parse_ast_output("This is just plain text.\nNo headers here.\n");
        assert!(parsed.phases.is_empty());
        assert!(parsed.is_valid);
    }

    #[test]
    fn parse_content_before_first_header_discarded() {
        let parsed = parse_ast_output("Preamble text.\n## CLASSIFY\n- Task: X\n");
        assert_eq!(parsed.phases.len(), 1);
        assert_eq!(parsed.phases[0].name, "CLASSIFY");
        assert!(!parsed.phases[0].content.contains("Preamble"));
    }

    #[test]
    fn parse_duplicate_phases_flagged() {
        let output = "\
## CLASSIFY
- Task: X

## EXECUTE
1. Step

## CLASSIFY
- Task: Y
";

        let parsed = parse_ast_output(output);
        assert!(!parsed.is_valid);
        assert!(
            parsed
                .validation_errors
                .iter()
                .any(|e| e.contains("Duplicate phase")),
            "Expected duplicate-phase error, got: {:?}",
            parsed.validation_errors
        );
    }

    #[test]
    fn parse_phase_with_empty_content() {
        let output = "## CLASSIFY\n\n## SKELETON\n- M1: Do thing\n";
        let parsed = parse_ast_output(output);
        assert_eq!(parsed.phases.len(), 2);
        assert_eq!(parsed.phases[0].name, "CLASSIFY");
        assert!(parsed.phases[0].content.trim().is_empty());
        assert_eq!(parsed.phases[1].name, "SKELETON");
    }

    #[test]
    fn validate_phase_order_all_present_for_complex() {
        let output = "\
## CLASSIFY
- Task: X

## RESEARCH
- Files: a.rs

## SKELETON
- M1: Step

## EXPAND
1. Step

## EXECUTE
1. Step

## VERIFY
Overall: PASS
";

        let parsed = parse_ast_output(output);
        let errors = validate_phase_order(&parsed.phases, ComplexityLevel::Complex);
        assert!(
            errors.is_empty(),
            "Expected no errors for valid complex output, got: {errors:?}"
        );
    }

    #[test]
    fn validate_phase_order_missing_classify_always_fails() {
        let output = "\
## RESEARCH
- Files: a.rs

## SKELETON
- M1: Step

## EXPAND
1. Step

## EXECUTE
1. Step

## VERIFY
Overall: PASS
";

        let parsed = parse_ast_output(output);
        // Even for trivial, CLASSIFY is required.
        let errors = validate_phase_order(&parsed.phases, ComplexityLevel::Trivial);
        assert!(errors
            .iter()
            .any(|e| e.contains("Missing required phase: CLASSIFY")));
    }

    // -- Serialization roundtrip ----------------------------------------------

    #[test]
    fn parsed_ast_output_serialization_roundtrip() {
        let output = ParsedAstOutput {
            phases: vec![ParsedPhase {
                name: "CLASSIFY".to_owned(),
                content: "- Task: X".to_owned(),
                line_number: 1,
            }],
            validation_errors: vec![],
            is_valid: true,
        };

        let json = serde_json::to_string(&output).unwrap();
        let back: ParsedAstOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(back.phases.len(), 1);
        assert_eq!(back.phases[0].name, "CLASSIFY");
        assert!(back.is_valid);
    }

    // -- detect_extra_sections ------------------------------------------------

    #[test]
    fn detect_extra_sections_none() {
        let output = "## CLASSIFY\n- Task: X\n## EXECUTE\n1. Step\n";
        assert!(detect_extra_sections(output).is_empty());
    }

    #[test]
    fn detect_extra_sections_with_extras() {
        let output = "## CLASSIFY\n- Task: X\n## CUSTOM_SECTION\nstuff\n## EXECUTE\n1. Step\n";
        let extras = detect_extra_sections(output);
        assert_eq!(extras.len(), 1);
        assert_eq!(extras[0].0, "CUSTOM_SECTION");
        assert_eq!(extras[0].1, 3);
    }

    // -- estimate_tokens -----------------------------------------------------

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_single_word() {
        assert_eq!(estimate_tokens("hello"), 1);
    }

    #[test]
    fn estimate_tokens_sentence() {
        let count = estimate_tokens("hello world foo bar");
        assert_eq!(count, 4);
    }
}
