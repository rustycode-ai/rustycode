//! Task decomposition — local heuristic decomposition without LLM calls.
//!
//! Produces a structured plan skeleton from task text using keyword matching,
//! task type detection, and concept extraction. Injected into the system prompt
//! so the LLM starts with a pre-computed plan instead of having to call
//! `structured_thinking` first.

use crate::ast::clarity::ClarityReport;
use crate::error::Result;
use crate::types::{Difficulty, OutputType, Step};

/// A decomposed task with a generated plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DecomposedTask {
    pub original_task: String,
    pub task_category: String,
    pub steps: Vec<Step>,
    pub estimated_difficulty: Difficulty,
}

/// Task type detected from keywords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskType {
    Implement,
    Refactor,
    Fix,
    Debug,
    Design,
    Explore,
    Test,
    Migrate,
    General,
}

impl TaskType {
    fn label(self) -> &'static str {
        match self {
            Self::Implement => "Implementation",
            Self::Refactor => "Refactoring",
            Self::Fix => "Bug Fix",
            Self::Debug => "Debugging",
            Self::Design => "Design/Architecture",
            Self::Explore => "Exploration",
            Self::Test => "Testing",
            Self::Migrate => "Migration",
            Self::General => "General",
        }
    }
}

/// Detect the task type from keyword signals.
pub fn detect_task_type(text: &str) -> TaskType {
    let lower = text.to_lowercase();
    let signals: [(TaskType, &[&str]); 9] = [
        (TaskType::Implement, &["implement", "build", "create", "write", "develop", "construct"]),
        (TaskType::Refactor, &["refactor", "restructure", "reorganize", "clean up", "rewrite"]),
        (TaskType::Fix, &["fix", "repair", "patch", "resolve", "correct"]),
        (TaskType::Debug, &["debug", "diagnose", "troubleshoot", "investigate why"]),
        (TaskType::Design, &["design", "architecture", "architect", "plan", "blueprint"]),
        (TaskType::Explore, &["explore", "analyze", "understand", "examine", "investigate"]),
        (TaskType::Test, &["test", "verify", "validate", "assert", "coverage"]),
        (TaskType::Migrate, &["migrate", "port", "upgrade", "convert"]),
        (TaskType::General, &[]),
    ];

    for (task_type, keywords) in &signals {
        if keywords.iter().any(|kw| lower.contains(kw)) {
            return *task_type;
        }
    }
    TaskType::General
}

/// Extract key concepts/nouns from the task text for specific plan steps.
pub fn extract_concepts(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let technical_signals: [(&str, &str); 30] = [
        // Languages & runtimes
        ("python", "Python"),
        ("javascript", "JavaScript"),
        ("typescript", "TypeScript"),
        ("rust", "Rust"),
        ("go ", "Go"),
        ("java ", "Java"),
        ("node.js", "Node.js"),
        ("nodejs", "Node.js"),
        // Domains
        ("interpreter", "interpreter"),
        ("compiler", "compiler"),
        ("parser", "parser"),
        ("vm ", "virtual machine"),
        ("virtual machine", "virtual machine"),
        ("database", "database"),
        ("api", "API"),
        ("server", "server"),
        ("client", "client"),
        ("protocol", "protocol"),
        ("algorithm", "algorithm"),
        ("data structure", "data structure"),
        // Architecture
        ("auth", "authentication"),
        ("authentication", "authentication"),
        ("middleware", "middleware"),
        ("pipeline", "pipeline"),
        ("cache", "caching"),
        ("queue", "queue"),
        // Hardware/ISA
        ("mips", "MIPS ISA"),
        ("risc-v", "RISC-V ISA"),
        ("x86", "x86 ISA"),
        ("instruction", "instruction set"),
    ];

    let mut concepts: Vec<String> = technical_signals
        .iter()
        .filter(|(kw, _)| lower.contains(kw))
        .map(|(_, label)| (*label).to_string())
        .collect();

    concepts.sort();
    concepts.dedup();
    concepts
}

/// Generate plan steps for an implementation task.
fn implement_steps(text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];

    // Step 1: Always read the spec/context
    steps.push("Read the task specification and any referenced files to understand all requirements".to_string());

    // Step 2: Core implementation based on concepts
    if !concepts.is_empty() {
        let concept_list = concepts.join(", ");
        steps.push(format!("Design the core {concept_list} components"));
        steps.push(format!("Implement the {concept_list} modules"));
    } else {
        steps.push("Design the core architecture and data structures".to_string());
        steps.push("Implement the main logic".to_string());
    }

    // Step 3: Edge cases (check for specific keywords)
    let lower = text.to_lowercase();
    if lower.contains("edge case") || lower.contains("error") || lower.contains("boundary") {
        steps.push("Handle edge cases and error conditions".to_string());
    } else {
        steps.push("Add edge case handling and error conditions".to_string());
    }

    // Step 4: Integration
    steps.push("Wire components together and verify integration".to_string());

    // Step 5: Test
    if lower.contains("test") {
        steps.push("Run the provided tests and ensure all pass".to_string());
    } else {
        steps.push("Test the implementation against expected behavior".to_string());
    }

    steps
}

/// Generate plan steps for a debugging task.
fn debug_steps(_text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];
    steps.push("Reproduce the issue — confirm the failing behavior".to_string());

    if !concepts.is_empty() {
        steps.push(format!("Inspect the {concepts} components for the root cause", concepts = concepts.join(", ")));
    } else {
        steps.push("Locate the root cause by tracing the failing code path".to_string());
    }

    steps.push("Identify the specific code change needed".to_string());
    steps.push("Apply the fix".to_string());
    steps.push("Verify the fix resolves the issue without regressions".to_string());
    steps
}

/// Generate plan steps for a refactoring task.
fn refactor_steps(_text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];
    steps.push("Understand the current code structure and dependencies".to_string());

    if !concepts.is_empty() {
        steps.push(format!("Identify what needs to change in the {concepts} layer", concepts = concepts.join(", ")));
    } else {
        steps.push("Identify the specific refactoring targets".to_string());
    }

    steps.push("Apply changes incrementally, verifying after each step".to_string());
    steps.push("Run tests to confirm no behavior change".to_string());
    steps
}

/// Generate plan steps for an exploration/analysis task.
fn explore_steps(_text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];
    steps.push("Survey the codebase — find relevant files and entry points".to_string());

    if !concepts.is_empty() {
        steps.push(format!("Trace the {concepts} flow through the code", concepts = concepts.join(", ")));
    } else {
        steps.push("Trace the relevant code paths".to_string());
    }

    steps.push("Document findings and answer the question".to_string());
    steps
}

/// Generate plan steps for a design task.
fn design_steps(_text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];
    steps.push("Gather requirements and constraints".to_string());

    if !concepts.is_empty() {
        steps.push(format!("Design the {concepts} architecture", concepts = concepts.join(", ")));
    } else {
        steps.push("Design the target architecture".to_string());
    }

    steps.push("Define interfaces and data flow".to_string());
    steps.push("Validate the design against requirements".to_string());
    steps
}

/// Generate plan steps for a test task.
fn test_steps(_text: &str, _concepts: &[String]) -> Vec<String> {
    vec![
        "Understand what needs to be tested — review the target code".to_string(),
        "Identify test cases: happy path, edge cases, error conditions".to_string(),
        "Write the tests".to_string(),
        "Run tests and verify expected results".to_string(),
    ]
}

/// Generate plan steps for a migration task.
fn migrate_steps(_text: &str, concepts: &[String]) -> Vec<String> {
    let mut steps = vec![];
    steps.push("Understand the source and target — read both specs".to_string());

    if !concepts.is_empty() {
        steps.push(format!("Map {concepts} concepts from source to target", concepts = concepts.join(", ")));
    } else {
        steps.push("Map concepts from source to target".to_string());
    }

    steps.push("Implement the migration incrementally".to_string());
    steps.push("Verify equivalence between source and target".to_string());
    steps
}

/// Generate plan steps for a general/unknown task type.
fn general_steps(_text: &str, _concepts: &[String]) -> Vec<String> {
    vec![
        "Understand the task — read any specs or referenced files".to_string(),
        "Plan the approach".to_string(),
        "Implement the solution".to_string(),
        "Verify the result".to_string(),
    ]
}

/// Estimate difficulty from complexity score.
fn estimate_difficulty(complexity: f64) -> Difficulty {
    if complexity < 2.0 {
        Difficulty::Easy
    } else if complexity < 3.5 {
        Difficulty::Medium
    } else {
        Difficulty::Hard
    }
}

/// Local heuristic decomposition — no LLM calls.
///
/// Takes task text and an optional clarity report, produces a formatted plan
/// string ready for injection into the system prompt.
pub fn decompose_local(
    task: &str,
    clarity_report: Option<&ClarityReport>,
    complexity: f64,
) -> String {
    let task_type = detect_task_type(task);
    let concepts = extract_concepts(task);
    let difficulty = estimate_difficulty(complexity);

    let step_strings = match task_type {
        TaskType::Implement => implement_steps(task, &concepts),
        TaskType::Fix | TaskType::Debug => debug_steps(task, &concepts),
        TaskType::Refactor => refactor_steps(task, &concepts),
        TaskType::Explore => explore_steps(task, &concepts),
        TaskType::Design => design_steps(task, &concepts),
        TaskType::Test => test_steps(task, &concepts),
        TaskType::Migrate => migrate_steps(task, &concepts),
        TaskType::General => general_steps(task, &concepts),
    };

    let mut plan = format!(
        "## Pre-computed Plan\n- **Type:** {type_label}\n- **Difficulty:** {difficulty_label}\n",
        type_label = task_type.label(),
        difficulty_label = match difficulty {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        },
    );

    if !concepts.is_empty() {
        plan.push_str(&format!(
            "- **Key concepts:** {}\n",
            concepts.join(", ")
        ));
    }

    plan.push_str("\n### Suggested Steps\n");
    for (i, step) in step_strings.iter().enumerate() {
        plan.push_str(&format!("{}. {step}\n", i + 1));
    }

    // Append clarity gaps if available
    if let Some(report) = clarity_report {
        if !report.questions.is_empty() {
            plan.push_str("\n### Gaps to Address\n");
            for q in &report.questions {
                plan.push_str(&format!("- **{}**: {}\n", q.dimension, q.question));
            }
        }
    }

    plan.push_str("\nFollow this plan. Adapt as you learn during implementation.\n");

    plan
}

// ---------------------------------------------------------------------------
// Trait-based interface (for async/LLM-based decomposition in the future)
// ---------------------------------------------------------------------------

#[allow(async_fn_in_trait)]
pub trait TaskDecomposer: Send + Sync {
    async fn decompose(&self, task: &str, category: &str) -> Result<DecomposedTask>;
}

pub struct Decomposer {}

impl Decomposer {
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for Decomposer {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskDecomposer for Decomposer {
    async fn decompose(&self, task: &str, category: &str) -> Result<DecomposedTask> {
        let task_type = detect_task_type(task);
        let concepts = extract_concepts(task);
        let step_strings = match task_type {
            TaskType::Implement => implement_steps(task, &concepts),
            TaskType::Fix | TaskType::Debug => debug_steps(task, &concepts),
            TaskType::Refactor => refactor_steps(task, &concepts),
            TaskType::Explore => explore_steps(task, &concepts),
            TaskType::Design => design_steps(task, &concepts),
            TaskType::Test => test_steps(task, &concepts),
            TaskType::Migrate => migrate_steps(task, &concepts),
            TaskType::General => general_steps(task, &concepts),
        };

        let steps: Vec<Step> = step_strings
            .into_iter()
            .enumerate()
            .map(|(i, desc)| Step {
                id: format!("step-{}", i + 1),
                index: i as u8,
                description: desc,
                expected_output_type: OutputType::Verification,
                suggested_tool: Some("bash".into()),
                retry_on_failure: true,
                required_resources: crate::guard::RequiredResources::default(),
            })
            .collect();

        let difficulty = if concepts.len() > 3 {
            Difficulty::Hard
        } else if !concepts.is_empty() {
            Difficulty::Medium
        } else {
            Difficulty::Easy
        };

        Ok(DecomposedTask {
            original_task: task.to_string(),
            task_category: category.to_string(),
            steps,
            estimated_difficulty: difficulty,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn detect_implement_type() {
        assert_eq!(detect_task_type("implement a MIPS interpreter"), TaskType::Implement);
        assert_eq!(detect_task_type("build a web server"), TaskType::Implement);
        assert_eq!(detect_task_type("create a new module"), TaskType::Implement);
    }

    #[test]
    fn detect_fix_type() {
        assert_eq!(detect_task_type("fix the null pointer crash"), TaskType::Fix);
        assert_eq!(detect_task_type("resolve the timeout issue"), TaskType::Fix);
    }

    #[test]
    fn detect_debug_type() {
        assert_eq!(detect_task_type("debug the race condition"), TaskType::Debug);
        assert_eq!(detect_task_type("diagnose why the tests fail"), TaskType::Debug);
    }

    #[test]
    fn detect_refactor_type() {
        assert_eq!(detect_task_type("refactor the auth module"), TaskType::Refactor);
        assert_eq!(detect_task_type("restructure the database layer"), TaskType::Refactor);
    }

    #[test]
    fn detect_explore_type() {
        assert_eq!(detect_task_type("explore the codebase"), TaskType::Explore);
        assert_eq!(detect_task_type("analyze the performance"), TaskType::Explore);
    }

    #[test]
    fn detect_design_type() {
        assert_eq!(detect_task_type("design the API gateway"), TaskType::Design);
        assert_eq!(detect_task_type("architecture for the event system"), TaskType::Design);
    }

    #[test]
    fn detect_test_type() {
        assert_eq!(detect_task_type("test the parser module"), TaskType::Test);
        assert_eq!(detect_task_type("add test coverage for the parser"), TaskType::Test);
    }

    #[test]
    fn detect_migrate_type() {
        assert_eq!(detect_task_type("migrate from REST to GraphQL"), TaskType::Migrate);
        assert_eq!(detect_task_type("port the C code to Rust"), TaskType::Migrate);
    }

    #[test]
    fn detect_general_type() {
        assert_eq!(detect_task_type("hello world"), TaskType::General);
        assert_eq!(detect_task_type("make it go faster"), TaskType::General);
    }

    #[test]
    fn extract_concepts_from_mips_task() {
        let concepts = extract_concepts("implement a MIPS interpreter in Node.js");
        assert!(concepts.contains(&"MIPS ISA".to_string()));
        assert!(concepts.contains(&"interpreter".to_string()));
        assert!(concepts.contains(&"Node.js".to_string()));
    }

    #[test]
    fn extract_concepts_deduplicates() {
        let concepts = extract_concepts("the parser and the parser module");
        assert_eq!(concepts.len(), 1);
    }

    #[test]
    fn extract_concepts_empty_for_unknown() {
        let concepts = extract_concepts("do the thing");
        assert!(concepts.is_empty());
    }

    #[test]
    fn decompose_local_produces_plan() {
        let plan = decompose_local(
            "implement a MIPS VM interpreter in Node.js",
            None,
            4.0,
        );
        assert!(plan.contains("Pre-computed Plan"));
        assert!(plan.contains("Implementation"));
        assert!(plan.contains("MIPS ISA"));
        assert!(plan.contains("Suggested Steps"));
        assert!(plan.contains("1."));
    }

    #[test]
    fn decompose_local_includes_clarity_gaps() {
        let report = ClarityReport {
            scores: crate::ast::clarity::ClarityScore {
                goal: 0.5,
                constraints: 0.5,
                success_criteria: 0.5,
                context: 0.5,
            },
            ambiguity: 0.5,
            questions: vec![
                crate::ast::clarity::ClarificationQuestion {
                    dimension: crate::ast::clarity::ClarityDimension::Goal,
                    question: "What instruction set version?".to_string(),
                    rationale: "MIPS version not specified".to_string(),
                },
            ],
            enriched_task: None,
        };
        let plan = decompose_local("implement MIPS VM", Some(&report), 4.0);
        assert!(plan.contains("Gaps to Address"));
        assert!(plan.contains("What instruction set version?"));
    }

    #[test]
    fn decompose_local_fix_type() {
        let plan = decompose_local("fix the null pointer dereference in the parser", None, 2.5);
        assert!(plan.contains("Bug Fix"));
        assert!(plan.contains("Reproduce"));
    }

    #[test]
    fn decompose_local_refactor_type() {
        let plan = decompose_local("refactor the database module to use connection pooling", None, 3.0);
        assert!(plan.contains("Refactoring"));
        assert!(plan.contains("database"));
    }

    #[test]
    fn decompose_local_empty_input() {
        let plan = decompose_local("", None, 1.0);
        assert!(plan.contains("Pre-computed Plan"));
    }

    #[test]
    fn decompose_local_short_task() {
        let plan = decompose_local("fix typo", None, 1.0);
        assert!(plan.contains("1."));
    }

    #[test]
    fn decompose_local_difficulty_mapping() {
        let easy = decompose_local("fix typo", None, 1.0);
        assert!(easy.contains("Easy"));
        let medium = decompose_local("implement auth middleware", None, 2.5);
        assert!(medium.contains("Medium"));
        let hard = decompose_local("implement a full MIPS interpreter", None, 4.5);
        assert!(hard.contains("Hard"));
    }

    #[tokio::test]
    async fn trait_decompose_returns_multi_step() {
        let decomposer = Decomposer::new();
        let result = decomposer
            .decompose("implement a MIPS interpreter", "code")
            .await
            .unwrap();
        assert!(result.steps.len() > 1, "should produce multiple steps, got {}", result.steps.len());
        assert_eq!(result.original_task, "implement a MIPS interpreter");
    }

    #[tokio::test]
    async fn trait_decompose_returns_original_task() {
        let decomposer = Decomposer::new();
        let result = decomposer
            .decompose("Build a web server", "code")
            .await
            .unwrap();
        assert_eq!(result.original_task, "Build a web server");
        assert_eq!(result.task_category, "code");
    }

    #[tokio::test]
    async fn trait_decompose_returns_steps() {
        let decomposer = Decomposer::new();
        let result = decomposer
            .decompose("Implement auth", "code")
            .await
            .unwrap();
        assert!(!result.steps.is_empty());
        assert_eq!(result.steps[0].id, "step-1");
        assert_eq!(result.steps[0].index, 0);
    }

    #[tokio::test]
    async fn trait_decompose_step_description_contains_task() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Fix the bug", "debug").await.unwrap();
        assert!(result.steps[0].description.contains("issue") || result.steps[0].description.contains("Fix"));
    }

    #[tokio::test]
    async fn trait_decompose_serialization_roundtrip() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("Build feature", "code").await.unwrap();
        let json = serde_json::to_string(&result).unwrap();
        let back: DecomposedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(result.original_task, back.original_task);
        assert_eq!(result.task_category, back.task_category);
        assert_eq!(result.steps.len(), back.steps.len());
    }

    #[test]
    fn test_decomposed_task_fields() {
        let task = DecomposedTask {
            original_task: "test".into(),
            task_category: "code".into(),
            steps: vec![],
            estimated_difficulty: Difficulty::Medium,
        };
        assert_eq!(task.original_task, "test");
        assert_eq!(task.task_category, "code");
        assert!(task.steps.is_empty());
        assert_eq!(task.estimated_difficulty, Difficulty::Medium);
    }

    #[test]
    fn detect_task_type_priority_first_match() {
        // "implement" should match before "fix" even if "fix" appears later
        assert_eq!(
            detect_task_type("implement a fix for the parser"),
            TaskType::Implement
        );
    }

    #[test]
    fn extract_concepts_multiple_domains() {
        let concepts = extract_concepts(
            "build a Python API with cache and a database",
        );
        assert!(concepts.contains(&"Python".to_string()));
        assert!(concepts.contains(&"API".to_string()));
        assert!(concepts.contains(&"caching".to_string()));
        assert!(concepts.contains(&"database".to_string()));
    }

    // ── Comprehensive test suite ──────────────────────────────────────────

    // Task type detection — additional keyword coverage

    #[test]
    fn detect_implement_aliases() {
        assert_eq!(detect_task_type("develop a plugin system"), TaskType::Implement);
        assert_eq!(detect_task_type("construct the data pipeline"), TaskType::Implement);
    }

    #[test]
    fn detect_refactor_aliases() {
        assert_eq!(detect_task_type("reorganize the module structure"), TaskType::Refactor);
        assert_eq!(detect_task_type("restructure the codebase"), TaskType::Refactor);
    }

    #[test]
    fn detect_fix_aliases() {
        assert_eq!(detect_task_type("repair the corrupted index"), TaskType::Fix);
        assert_eq!(detect_task_type("patch the security vulnerability"), TaskType::Fix);
        assert_eq!(detect_task_type("correct the calculation error"), TaskType::Fix);
    }

    #[test]
    fn detect_debug_aliases() {
        assert_eq!(detect_task_type("troubleshoot the connection timeout"), TaskType::Debug);
        assert_eq!(detect_task_type("investigate why the server hangs"), TaskType::Debug);
    }

    #[test]
    fn detect_explore_aliases() {
        assert_eq!(detect_task_type("examine the log output"), TaskType::Explore);
        assert_eq!(detect_task_type("understand the deployment process"), TaskType::Explore);
    }

    #[test]
    fn detect_design_aliases() {
        assert_eq!(detect_task_type("plan the migration strategy"), TaskType::Design);
        assert_eq!(detect_task_type("blueprint the notification system"), TaskType::Design);
        assert_eq!(detect_task_type("architect the event-driven pipeline"), TaskType::Design);
    }

    #[test]
    fn detect_migrate_aliases() {
        assert_eq!(detect_task_type("port the Python CLI to Go"), TaskType::Migrate);
        assert_eq!(detect_task_type("upgrade the database schema"), TaskType::Migrate);
        assert_eq!(detect_task_type("convert the XML config to YAML"), TaskType::Migrate);
    }

    #[test]
    fn detect_test_aliases() {
        assert_eq!(detect_task_type("validate the input sanitization"), TaskType::Test);
        assert_eq!(detect_task_type("assert the output matches the schema"), TaskType::Test);
    }

    #[test]
    fn detect_case_insensitive() {
        assert_eq!(detect_task_type("IMPLEMENT the feature"), TaskType::Implement);
        assert_eq!(detect_task_type("FIX The Bug"), TaskType::Fix);
        assert_eq!(detect_task_type("DEBUG the issue"), TaskType::Debug);
    }

    #[test]
    fn detect_keyword_embedded_in_word() {
        // "testing" contains "test" — should match Test
        assert_eq!(detect_task_type("testing is important"), TaskType::Test);
        // "examined" contains "examine" — should match Explore
        assert_eq!(detect_task_type("examined the dataset"), TaskType::Explore);
    }

    #[test]
    fn detect_first_keyword_wins() {
        // "implement" before "refactor"
        assert_eq!(
            detect_task_type("implement then refactor the module"),
            TaskType::Implement
        );
        // "fix" before "debug" in signal order
        assert_eq!(
            detect_task_type("debug and fix the issue"),
            TaskType::Fix
        );
    }

    // Concept extraction — broader coverage

    #[test]
    fn extract_rust_concept() {
        let concepts = extract_concepts("write a Rust parser");
        assert!(concepts.contains(&"Rust".to_string()));
    }

    #[test]
    fn extract_go_concept() {
        let concepts = extract_concepts("build a Go microservice");
        assert!(concepts.contains(&"Go".to_string()));
    }

    #[test]
    fn extract_java_concept() {
        let concepts = extract_concepts("create a Java Spring Boot app");
        assert!(concepts.contains(&"Java".to_string()));
    }

    #[test]
    fn extract_typescript_concept() {
        let concepts = extract_concepts("write a TypeScript SDK");
        assert!(concepts.contains(&"TypeScript".to_string()));
    }

    #[test]
    fn extract_nodejs_concept() {
        let concepts = extract_concepts("build a Node.js backend");
        assert!(concepts.contains(&"Node.js".to_string()));
    }

    #[test]
    fn extract_nodejs_no_space_concept() {
        let concepts = extract_concepts("set up nodejs environment");
        assert!(concepts.contains(&"Node.js".to_string()));
    }

    #[test]
    fn extract_hardware_isa_concepts() {
        let concepts = extract_concepts("implement MIPS instruction decoder");
        assert!(concepts.contains(&"MIPS ISA".to_string()));
        assert!(concepts.contains(&"instruction set".to_string()));
    }

    #[test]
    fn extract_riscv_concept() {
        let concepts = extract_concepts("build a RISC-V emulator");
        assert!(concepts.contains(&"RISC-V ISA".to_string()));
    }

    #[test]
    fn extract_x86_concept() {
        let concepts = extract_concepts("write an x86 disassembler");
        assert!(concepts.contains(&"x86 ISA".to_string()));
    }

    #[test]
    fn extract_architecture_concepts() {
        let concepts = extract_concepts(
            "add auth middleware with a cache layer and message queue",
        );
        assert!(concepts.contains(&"authentication".to_string()));
        assert!(concepts.contains(&"middleware".to_string()));
        assert!(concepts.contains(&"caching".to_string()));
        assert!(concepts.contains(&"queue".to_string()));
    }

    #[test]
    fn extract_vm_concept() {
        let concepts = extract_concepts("build a virtual machine");
        assert!(concepts.contains(&"virtual machine".to_string()));
    }

    #[test]
    fn extract_vm_shorthand_concept() {
        let concepts = extract_concepts("implement a VM for bytecode");
        // "vm " matches the "vm " signal → "virtual machine"
        assert!(concepts.contains(&"virtual machine".to_string()));
    }

    #[test]
    fn extract_concepts_no_false_positives() {
        // Words that contain substrings but shouldn't match
        let concepts = extract_concepts("test the history module");
        // "api" is a substring of many words but the signal is ("api", "API")
        // "history" doesn't contain any signal keyword as a whole
        assert!(!concepts.contains(&"API".to_string()));
    }

    // decompose_local — comprehensive plan generation

    #[test]
    fn decompose_implement_plan_has_read_step() {
        let plan = decompose_local("implement a cache layer", None, 3.0);
        assert!(plan.contains("Read the task specification"));
    }

    #[test]
    fn decompose_implement_with_concepts() {
        let plan = decompose_local("implement a Python interpreter", None, 4.0);
        assert!(plan.contains("Python"));
        assert!(plan.contains("interpreter"));
        assert!(plan.contains("Design the core"));
        assert!(plan.contains("Implement the"));
    }

    #[test]
    fn decompose_implement_without_concepts() {
        let plan = decompose_local("implement the feature", None, 3.0);
        assert!(plan.contains("Design the core architecture"));
    }

    #[test]
    fn decompose_implement_with_edge_case_keyword() {
        let plan = decompose_local("implement error handling for edge cases", None, 3.0);
        assert!(plan.contains("Handle edge cases"));
    }

    #[test]
    fn decompose_implement_with_test_keyword() {
        let plan = decompose_local("implement the algorithm with test cases", None, 3.0);
        assert!(plan.contains("Run the provided tests"));
    }

    #[test]
    fn decompose_debug_plan_structure() {
        let plan = decompose_local("debug the parser crash", None, 2.5);
        assert!(plan.contains("Debugging"));
        assert!(plan.contains("Reproduce the issue"));
        assert!(plan.contains("Identify the specific code change"));
        assert!(plan.contains("Apply the fix"));
        assert!(plan.contains("Verify the fix"));
    }

    #[test]
    fn decompose_debug_with_concepts() {
        let plan = decompose_local("debug the database query timeout", None, 3.0);
        assert!(plan.contains("Inspect the"));
        assert!(plan.contains("database"));
    }

    #[test]
    fn decompose_refactor_plan_structure() {
        let plan = decompose_local("refactor the auth module", None, 3.0);
        assert!(plan.contains("Refactoring"));
        assert!(plan.contains("Understand the current code structure"));
        assert!(plan.contains("Apply changes incrementally"));
        assert!(plan.contains("Run tests"));
    }

    #[test]
    fn decompose_explore_plan_structure() {
        let plan = decompose_local("explore the rendering pipeline", None, 2.0);
        assert!(plan.contains("Exploration"));
        assert!(plan.contains("Survey the codebase"));
        assert!(plan.contains("Trace the"));
        assert!(plan.contains("Document findings"));
    }

    #[test]
    fn decompose_design_plan_structure() {
        let plan = decompose_local("design the notification system", None, 3.5);
        assert!(plan.contains("Design/Architecture"));
        assert!(plan.contains("Gather requirements"));
        assert!(plan.contains("Define interfaces"));
        assert!(plan.contains("Validate the design"));
    }

    #[test]
    fn decompose_test_plan_structure() {
        let plan = decompose_local("test the API endpoint", None, 2.0);
        assert!(plan.contains("Testing"));
        assert!(plan.contains("happy path, edge cases"));
        assert!(plan.contains("Write the tests"));
        assert!(plan.contains("Run tests"));
    }

    #[test]
    fn decompose_migrate_plan_structure() {
        let plan = decompose_local("migrate from Python to Rust", None, 3.5);
        assert!(plan.contains("Migration"));
        assert!(plan.contains("Understand the source and target"));
        assert!(plan.contains("Map Python, Rust concepts"));
        assert!(plan.contains("Implement the migration"));
        assert!(plan.contains("Verify equivalence"));
    }

    #[test]
    fn decompose_general_plan_structure() {
        let plan = decompose_local("do something useful", None, 1.0);
        assert!(plan.contains("General"));
        assert!(plan.contains("Understand the task"));
        assert!(plan.contains("Plan the approach"));
        assert!(plan.contains("Implement the solution"));
        assert!(plan.contains("Verify the result"));
    }

    #[test]
    fn decompose_difficulty_easy() {
        let plan = decompose_local("fix typo", None, 0.5);
        assert!(plan.contains("Easy"));
        assert!(!plan.contains("Hard"));
    }

    #[test]
    fn decompose_difficulty_medium() {
        let plan = decompose_local("refactor the auth module", None, 2.5);
        assert!(plan.contains("Medium"));
        assert!(!plan.contains("Easy"));
    }

    #[test]
    fn decompose_difficulty_hard() {
        let plan = decompose_local("build a full compiler", None, 4.5);
        assert!(plan.contains("Hard"));
        assert!(!plan.contains("Medium"));
    }

    #[test]
    fn decompose_clarity_gaps_multiple_questions() {
        let report = ClarityReport {
            scores: crate::ast::clarity::ClarityScore {
                goal: 0.3,
                constraints: 0.4,
                success_criteria: 0.2,
                context: 0.6,
            },
            ambiguity: 0.6,
            questions: vec![
                crate::ast::clarity::ClarificationQuestion {
                    dimension: crate::ast::clarity::ClarityDimension::Goal,
                    question: "What is the target?".to_string(),
                    rationale: "Unclear goal".to_string(),
                },
                crate::ast::clarity::ClarificationQuestion {
                    dimension: crate::ast::clarity::ClarityDimension::SuccessCriteria,
                    question: "How do we measure success?".to_string(),
                    rationale: "No success criteria".to_string(),
                },
            ],
            enriched_task: None,
        };
        let plan = decompose_local("implement something", Some(&report), 3.0);
        assert!(plan.contains("Gaps to Address"));
        assert!(plan.contains("What is the target?"));
        assert!(plan.contains("How do we measure success?"));
    }

    #[test]
    fn decompose_no_clarity_gaps_when_no_questions() {
        let report = ClarityReport {
            scores: crate::ast::clarity::ClarityScore {
                goal: 0.9,
                constraints: 0.9,
                success_criteria: 0.9,
                context: 0.9,
            },
            ambiguity: 0.1,
            questions: vec![],
            enriched_task: None,
        };
        let plan = decompose_local("implement cache", Some(&report), 2.0);
        assert!(!plan.contains("Gaps to Address"));
    }

    #[test]
    fn decompose_plan_always_has_follow_instruction() {
        let plan = decompose_local("anything", None, 1.0);
        assert!(plan.contains("Follow this plan"));
    }

    #[test]
    fn decompose_plan_always_has_suggested_steps() {
        let plan = decompose_local("anything", None, 1.0);
        assert!(plan.contains("Suggested Steps"));
    }

    #[test]
    fn decompose_concepts_appear_in_plan() {
        let plan = decompose_local("build a Rust compiler with cache", None, 4.0);
        assert!(plan.contains("Key concepts:"));
        assert!(plan.contains("Rust"));
        assert!(plan.contains("compiler"));
        assert!(plan.contains("caching"));
    }

    #[test]
    fn decompose_no_concepts_no_key_concepts_line() {
        let plan = decompose_local("do the thing", None, 1.0);
        assert!(!plan.contains("Key concepts:"));
    }

    #[test]
    fn decompose_steps_are_numbered() {
        let plan = decompose_local("implement a server", None, 3.0);
        // Check sequential numbering
        assert!(plan.contains("1. "));
        assert!(plan.contains("2. "));
        assert!(plan.contains("3. "));
    }

    // estimate_difficulty edge cases

    #[test]
    fn difficulty_boundary_easy_to_medium() {
        // complexity < 2.0 → Easy
        assert!(matches!(estimate_difficulty(1.99), Difficulty::Easy));
        // complexity == 2.0 → Medium
        assert!(matches!(estimate_difficulty(2.0), Difficulty::Medium));
    }

    #[test]
    fn difficulty_boundary_medium_to_hard() {
        // complexity < 3.5 → Medium
        assert!(matches!(estimate_difficulty(3.49), Difficulty::Medium));
        // complexity == 3.5 → Hard
        assert!(matches!(estimate_difficulty(3.5), Difficulty::Hard));
    }

    #[test]
    fn difficulty_zero() {
        assert!(matches!(estimate_difficulty(0.0), Difficulty::Easy));
    }

    #[test]
    fn difficulty_max() {
        assert!(matches!(estimate_difficulty(5.0), Difficulty::Hard));
    }

    // Trait-based Decomposer — additional coverage

    #[tokio::test]
    async fn trait_decompose_fix_produces_debug_steps() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("fix the crash in production", "bug").await.unwrap();
        assert!(result.steps.len() >= 3, "fix should produce at least 3 steps");
        assert!(result.steps[0].description.contains("Reproduce"));
    }

    #[tokio::test]
    async fn trait_decompose_refactor_has_incremental_steps() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("refactor the database layer", "code").await.unwrap();
        let descs: Vec<&str> = result.steps.iter().map(|s| s.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("incrementally")));
    }

    #[tokio::test]
    async fn trait_decompose_explore_has_survey_step() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("explore the authentication flow", "analysis").await.unwrap();
        assert!(result.steps[0].description.contains("Survey"));
    }

    #[tokio::test]
    async fn trait_decompose_design_has_interfaces_step() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("design the event system", "arch").await.unwrap();
        let descs: Vec<&str> = result.steps.iter().map(|s| s.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("interfaces")));
    }

    #[tokio::test]
    async fn trait_decompose_migrate_has_equivalence_step() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("port the Python tool to Rust", "migration").await.unwrap();
        let descs: Vec<&str> = result.steps.iter().map(|s| s.description.as_str()).collect();
        assert!(descs.iter().any(|d| d.contains("equivalence")));
    }

    #[tokio::test]
    async fn trait_decompose_general_has_four_steps() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("make it go faster", "perf").await.unwrap();
        assert_eq!(result.steps.len(), 4);
    }

    #[tokio::test]
    async fn trait_decompose_difficulty_from_concepts() {
        let decomposer = Decomposer::new();
        // Many concepts → Hard
        let result = decomposer
            .decompose("implement a Python interpreter with MIPS instruction set and caching", "code")
            .await
            .unwrap();
        assert_eq!(result.estimated_difficulty, Difficulty::Hard);
    }

    #[tokio::test]
    async fn trait_decompose_difficulty_no_concepts() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("do stuff", "misc").await.unwrap();
        assert_eq!(result.estimated_difficulty, Difficulty::Easy);
    }

    #[tokio::test]
    async fn trait_decompose_step_ids_sequential() {
        let decomposer = Decomposer::new();
        let result = decomposer.decompose("implement auth", "code").await.unwrap();
        for (i, step) in result.steps.iter().enumerate() {
            assert_eq!(step.id, format!("step-{}", i + 1));
            assert_eq!(step.index, i as u8);
        }
    }

    // DecomposedTask serialization

    #[test]
    fn decomposed_task_json_roundtrip() {
        let task = DecomposedTask {
            original_task: "test task".into(),
            task_category: "code".into(),
            steps: vec![Step {
                id: "step-1".into(),
                index: 0,
                description: "Do the thing".into(),
                expected_output_type: OutputType::Verification,
                suggested_tool: Some("bash".into()),
                retry_on_failure: true,
                required_resources: crate::guard::RequiredResources::default(),
            }],
            estimated_difficulty: Difficulty::Hard,
        };
        let json = serde_json::to_string(&task).unwrap();
        let back: DecomposedTask = serde_json::from_str(&json).unwrap();
        assert_eq!(back.original_task, "test task");
        assert_eq!(back.steps.len(), 1);
        assert_eq!(back.steps[0].description, "Do the thing");
        assert_eq!(back.estimated_difficulty, Difficulty::Hard);
    }

    // TaskType label

    #[test]
    fn task_type_labels() {
        assert_eq!(TaskType::Implement.label(), "Implementation");
        assert_eq!(TaskType::Refactor.label(), "Refactoring");
        assert_eq!(TaskType::Fix.label(), "Bug Fix");
        assert_eq!(TaskType::Debug.label(), "Debugging");
        assert_eq!(TaskType::Design.label(), "Design/Architecture");
        assert_eq!(TaskType::Explore.label(), "Exploration");
        assert_eq!(TaskType::Test.label(), "Testing");
        assert_eq!(TaskType::Migrate.label(), "Migration");
        assert_eq!(TaskType::General.label(), "General");
    }

    // Realistic benchmark-like task inputs

    #[test]
    fn decompose_mips_vm_benchmark_task() {
        let plan = decompose_local(
            "Implement a MIPS VM interpreter in Node.js that can execute basic MIPS assembly programs. \
             The interpreter should support arithmetic instructions (ADD, SUB, MUL, DIV), \
             memory operations (LW, SW), and control flow (BEQ, BNE, J).",
            None,
            4.5,
        );
        assert!(plan.contains("Implementation"));
        assert!(plan.contains("Hard"));
        assert!(plan.contains("MIPS ISA"));
        assert!(plan.contains("Node.js"));
        assert!(plan.contains("instruction set"));
        assert!(plan.contains("Suggested Steps"));
    }

    #[test]
    fn decompose_swe_bench_like_task() {
        let plan = decompose_local(
            "Fix the bug in django.contrib.auth where logout() doesn't clear the session properly",
            None,
            3.0,
        );
        assert!(plan.contains("Bug Fix"));
        assert!(plan.contains("Reproduce"));
    }

    #[test]
    fn decompose_livebench_analysis_task() {
        let plan = decompose_local(
            "analyze the performance of the sorting algorithm and find bottlenecks",
            None,
            3.5,
        );
        assert!(plan.contains("Exploration"));
        assert!(plan.contains("algorithm"));
    }

    #[test]
    fn decompose_porting_task() {
        let plan = decompose_local(
            "port the Python codebase to Rust",
            None,
            4.0,
        );
        assert!(plan.contains("Migration"));
        assert!(plan.contains("Python"));
        assert!(plan.contains("Rust"));
    }

    // Edge cases

    #[test]
    fn decompose_whitespace_only() {
        let plan = decompose_local("   ", None, 1.0);
        assert!(plan.contains("Pre-computed Plan"));
    }

    #[test]
    fn decompose_very_long_task() {
        let long_task = "implement ".to_string() + &"a very complex feature ".repeat(100);
        let plan = decompose_local(&long_task, None, 5.0);
        assert!(plan.contains("Implementation"));
        assert!(plan.contains("Hard"));
    }

    #[test]
    fn decompose_unicode_task() {
        let plan = decompose_local("implement a parser for 中文 text files", None, 3.0);
        assert!(plan.contains("Pre-computed Plan"));
        assert!(plan.contains("parser"));
    }

    #[test]
    fn detect_type_with_special_chars() {
        assert_eq!(detect_task_type("fix: null-pointer in auth"), TaskType::Fix);
        assert_eq!(detect_task_type("[BUG] debug the race condition"), TaskType::Debug);
    }
}
