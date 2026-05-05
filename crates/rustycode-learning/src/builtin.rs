use crate::patterns::{
    ActionType, Pattern, PatternCategory, SuggestedAction, TriggerCondition, TriggerType,
};

/// Built-in patterns for common workflows
pub struct BuiltinPatterns;

impl BuiltinPatterns {
    /// Get all built-in patterns
    pub fn all() -> Vec<Pattern> {
        vec![
            Self::rust_error_handling(),
            Self::rust_async_patterns(),
            Self::rust_trait_implementations(),
            Self::debugging_compilation_errors(),
            Self::debugging_runtime_errors(),
            Self::test_driven_development(),
            Self::refactor_extract_function(),
            Self::generate_rust_docs(),
            Self::optimize_performance(),
            Self::git_workflow(),
        ]
    }

    /// Rust error handling pattern
    fn rust_error_handling() -> Pattern {
        let trigger = TriggerCondition::new(
            "rust-error-handling-trigger".to_string(),
            TriggerType::CodePattern,
            r"(unwrap\(\)|expect\(|\bpanic!)".to_string(),
        )
        .with_context_requirement("language".to_string(), "rust".to_string())
        .with_confidence_threshold(0.6);

        let _action = SuggestedAction::new(
            "rust-error-handling-action".to_string(),
            ActionType::Transform,
            "Replace unwrap/expect with proper error handling".to_string(),
            r#"
// Instead of:
result.unwrap()

// Use:
result?

// Or:
result.map_err(|e| anyhow::anyhow!("Context: {}", e))?
"#
            .to_string(),
        );

        Pattern::new(
            "rust-error-handling".to_string(),
            "Rust Error Handling".to_string(),
            PatternCategory::Coding,
            "Proper error handling with ? operator and Result types".to_string(),
        )
        .with_example("Replace unwrap() with ? operator".to_string())
        .with_example("Convert panic to Result".to_string())
        .with_confidence(0.8)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
        .with_metadata("severity".to_string(), "warning".to_string())
    }

    /// Rust async/await patterns
    fn rust_async_patterns() -> Pattern {
        let trigger = TriggerCondition::new(
            "rust-async-trigger".to_string(),
            TriggerType::CodePattern,
            r"(\.await|async fn)".to_string(),
        )
        .with_context_requirement("language".to_string(), "rust".to_string())
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "rust-async-action".to_string(),
            ActionType::Transform,
            "Proper async/await usage".to_string(),
            r"
// Use async fn for async operations
async fn fetch_data() -> Result<String> {
    // ...
}

// Use .await to call async functions
let data = fetch_data().await?;

// Consider error handling
let data = tokio::time::timeout(
    Duration::from_secs(5),
    fetch_data()
).await??
"
            .to_string(),
        );

        Pattern::new(
            "rust-async-patterns".to_string(),
            "Rust Async/Await Patterns".to_string(),
            PatternCategory::Coding,
            "Proper async/await usage and error handling".to_string(),
        )
        .with_example("Add async fn".to_string())
        .with_example("Use .await properly".to_string())
        .with_confidence(0.7)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Rust trait implementations
    fn rust_trait_implementations() -> Pattern {
        let trigger = TriggerCondition::new(
            "rust-traits-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(implement|derive).*trait".to_string(),
        )
        .with_context_requirement("language".to_string(), "rust".to_string())
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "rust-traits-action".to_string(),
            ActionType::Transform,
            "Implement trait properly".to_string(),
            r"
// Manual trait implementation
impl MyTrait for MyStruct {
    fn method(&self) -> Result<String> {
        // ...
    }
}

// Derived traits
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyStruct {
    field: String,
}
"
            .to_string(),
        );

        Pattern::new(
            "rust-trait-implementations".to_string(),
            "Rust Trait Implementations".to_string(),
            PatternCategory::Coding,
            "Proper trait implementation and derivation".to_string(),
        )
        .with_example("Implement Debug trait".to_string())
        .with_example("Derive Serialize".to_string())
        .with_confidence(0.6)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Debugging compilation errors
    fn debugging_compilation_errors() -> Pattern {
        let trigger = TriggerCondition::new(
            "compile-error-trigger".to_string(),
            TriggerType::ErrorPattern,
            r"error\[E\d+]".to_string(),
        )
        .with_context_requirement("context".to_string(), "compilation".to_string())
        .with_confidence_threshold(0.7);

        let _action = SuggestedAction::new(
            "compile-error-action".to_string(),
            ActionType::DebugStrategy,
            "Fix compilation error".to_string(),
            r"
# Compilation Error Debugging Strategy:

1. Read the error code (e.g., E0382)
2. Check the error message for details
3. Look at the suggested fixes
4. Run `cargo explain ERROR_CODE` for more info
5. Check the Rust documentation for the error

# Common fixes:
- E0382 (use of moved value): Clone or borrow instead of moving
- E0308 (type mismatch): Check type annotations and conversions
- E0061 (function call mismatch): Check argument types and count
- E0277 (trait not implemented): Implement required trait
"
            .to_string(),
        );

        Pattern::new(
            "debugging-compilation-errors".to_string(),
            "Debugging Compilation Errors".to_string(),
            PatternCategory::Debugging,
            "Systematic approach to fixing compilation errors".to_string(),
        )
        .with_example("Fix E0382 use after move".to_string())
        .with_example("Resolve E0308 type mismatch".to_string())
        .with_confidence(0.8)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Debugging runtime errors
    fn debugging_runtime_errors() -> Pattern {
        let trigger = TriggerCondition::new(
            "runtime-error-trigger".to_string(),
            TriggerType::ErrorPattern,
            r"(panicked at|unwrap failed|index out of bounds)".to_string(),
        )
        .with_context_requirement("context".to_string(), "runtime".to_string())
        .with_confidence_threshold(0.7);

        let _action = SuggestedAction::new(
            "runtime-error-action".to_string(),
            ActionType::DebugStrategy,
            "Fix runtime panic".to_string(),
            r"
# Runtime Error Debugging Strategy:

1. Check the panic message for location
2. Add debug logging before the panic point
3. Use a debugger to inspect variables
4. Add assertions to validate assumptions
5. Replace unwrap/expect with proper error handling

# Prevention:
- Use ? operator instead of unwrap()
- Use get() instead of [] for indexing
- Add bounds checking
- Use Result types instead of panicking
"
            .to_string(),
        );

        Pattern::new(
            "debugging-runtime-errors".to_string(),
            "Debugging Runtime Errors".to_string(),
            PatternCategory::Debugging,
            "Systematic approach to fixing runtime panics".to_string(),
        )
        .with_example("Fix panic in unwrap()".to_string())
        .with_example("Handle index out of bounds".to_string())
        .with_confidence(0.8)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Test-driven development pattern
    fn test_driven_development() -> Pattern {
        let trigger = TriggerCondition::new(
            "tdd-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(write|create|add).*test".to_string(),
        )
        .with_context_requirement("language".to_string(), "rust".to_string())
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "tdd-action".to_string(),
            ActionType::RunTests,
            "Follow TDD workflow".to_string(),
            r"
# Test-Driven Development Workflow:

1. RED: Write a failing test first
2. GREEN: Write minimal code to pass
3. IMPROVE: Refactor while keeping tests green

# Rust testing:
- Put tests in the same module with #[cfg(test)]
- Use assert_eq! for assertions
- Use should_panic for expected panics
- Run with: cargo test

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        let result = function_to_test();
        assert_eq!(result, expected);
    }
}
"
            .to_string(),
        );

        Pattern::new(
            "test-driven-development".to_string(),
            "Test-Driven Development".to_string(),
            PatternCategory::Testing,
            "Write tests first, then implement".to_string(),
        )
        .with_example("Create unit test".to_string())
        .with_example("Follow TDD cycle".to_string())
        .with_confidence(0.7)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Extract function refactoring
    fn refactor_extract_function() -> Pattern {
        let trigger = TriggerCondition::new(
            "extract-function-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(extract|refactor).*function".to_string(),
        )
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "extract-function-action".to_string(),
            ActionType::Refactor,
            "Extract function".to_string(),
            r"
# Function Extraction Guidelines:

1. Identify a block of code with a single purpose
2. Extract to a named function
3. Pass in required parameters
4. Return a meaningful result
5. Give it a descriptive name

// Before:
fn process() {
    let data = fetch();
    let parsed = parse(&data);
    let result = transform(&parsed);
    save(&result);
}

// After:
fn process() -> Result<()> {
    let data = fetch()?;
    let parsed = parse(&data)?;
    let result = transform(&parsed)?;
    save(&result)?;
    Ok(())
}
"
            .to_string(),
        );

        Pattern::new(
            "refactor-extract-function".to_string(),
            "Extract Function Refactoring".to_string(),
            PatternCategory::Refactoring,
            "Extract code blocks into well-named functions".to_string(),
        )
        .with_example("Extract duplicate code".to_string())
        .with_example("Break up long function".to_string())
        .with_confidence(0.6)
        .with_trigger(trigger)
    }

    /// Generate Rust documentation
    fn generate_rust_docs() -> Pattern {
        let trigger = TriggerCondition::new(
            "rust-docs-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(add|generate|write).*docs|documentation".to_string(),
        )
        .with_context_requirement("language".to_string(), "rust".to_string())
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "rust-docs-action".to_string(),
            ActionType::GenerateDocs,
            "Generate documentation".to_string(),
            r"
# Rust Documentation Guidelines:

/// Brief description
///
/// Longer description with details.
///
/// # Examples
///
/// ```
/// let result = function();
/// assert_eq!(result, expected);
/// ```
///
pub fn function() -> Result<Type> {
    // ...
}

// Run: cargo doc --open
"
            .to_string(),
        );

        Pattern::new(
            "generate-rust-docs".to_string(),
            "Generate Rust Documentation".to_string(),
            PatternCategory::Documentation,
            "Add comprehensive Rust documentation".to_string(),
        )
        .with_example("Add function docs".to_string())
        .with_example("Document module".to_string())
        .with_confidence(0.7)
        .with_trigger(trigger)
        .with_metadata("language".to_string(), "rust".to_string())
    }

    /// Performance optimization
    fn optimize_performance() -> Pattern {
        let trigger = TriggerCondition::new(
            "optimize-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(optimize|improve.*performance|speed up|make.*faster)".to_string(),
        )
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "optimize-action".to_string(),
            ActionType::Optimization,
            "Optimize performance".to_string(),
            r"
# Performance Optimization Strategy:

1. PROFILE FIRST: Use cargo flamegraph, perf, or Instruments
2. Measure before and after changes
3. Focus on hot paths identified by profiling
4. Common optimizations:
   - Reduce allocations
   - Use iterators instead of loops
   - Avoid cloning when possible
   - Use appropriate data structures
   - Cache expensive computations
5. Consider parallel processing for CPU-bound tasks
6. Use async/await for I/O-bound tasks

# Tools:
- cargo flamegraph
- cargo bench
- cargo计时
- heaptrack
"
            .to_string(),
        );

        Pattern::new(
            "optimize-performance".to_string(),
            "Performance Optimization".to_string(),
            PatternCategory::Optimization,
            "Systematic approach to improving performance".to_string(),
        )
        .with_example("Reduce allocations".to_string())
        .with_example("Use profiling".to_string())
        .with_confidence(0.6)
        .with_trigger(trigger)
    }

    /// Git workflow
    fn git_workflow() -> Pattern {
        let trigger = TriggerCondition::new(
            "git-workflow-trigger".to_string(),
            TriggerType::Intent,
            r"(?i)(commit|push|branch|merge).*git".to_string(),
        )
        .with_confidence_threshold(0.5);

        let _action = SuggestedAction::new(
            "git-workflow-action".to_string(),
            ActionType::SuggestCommand,
            "Suggest git workflow".to_string(),
            r#"
# Git Workflow Commands:

# Create feature branch
git checkout -b feature/your-feature-name

# Stage changes
git add path/to/file

# Commit with conventional commit message
git commit -m "feat: add new feature"

# Push to remote
git push -u origin feature/your-feature-name

# Merge to main
git checkout main
git merge feature/your-feature-name

# Commit message types:
- feat: New feature
- fix: Bug fix
- docs: Documentation changes
- test: Test changes
- refactor: Code refactoring
- perf: Performance improvements
- chore: Maintenance tasks
"#
            .to_string(),
        );

        Pattern::new(
            "git-workflow".to_string(),
            "Git Workflow".to_string(),
            PatternCategory::Coding,
            "Best practices for git operations".to_string(),
        )
        .with_example("Create feature branch".to_string())
        .with_example("Conventional commits".to_string())
        .with_confidence(0.7)
        .with_trigger(trigger)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_all_returns_patterns() {
        let patterns = BuiltinPatterns::all();
        assert!(!patterns.is_empty());
        assert_eq!(patterns.len(), 10);
    }

    #[test]
    fn test_builtin_all_have_unique_ids() {
        let patterns = BuiltinPatterns::all();
        let ids: Vec<&str> = patterns.iter().map(|p| p.id.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "Duplicate pattern IDs found");
    }

    #[test]
    fn test_builtin_all_have_names() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert!(!p.name.is_empty(), "Pattern {} has empty name", p.id);
        }
    }

    #[test]
    fn test_builtin_all_have_categories() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            // Category should be one of the known variants
            let cat = format!("{:?}", p.category);
            assert!(!cat.is_empty(), "Pattern {} has no category", p.id);
        }
    }

    #[test]
    fn test_builtin_all_have_descriptions() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert!(
                !p.description.is_empty(),
                "Pattern {} has empty description",
                p.id
            );
        }
    }

    #[test]
    fn test_builtin_patterns_have_confidence() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert!(p.confidence > 0.0, "Pattern {} has zero confidence", p.id);
            assert!(p.confidence <= 1.0, "Pattern {} has confidence > 1.0", p.id);
        }
    }

    #[test]
    fn test_builtin_patterns_have_triggers() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert!(
                !p.trigger_conditions.is_empty(),
                "Pattern {} has no trigger",
                p.id
            );
            let trigger = &p.trigger_conditions[0];
            assert!(
                !trigger.pattern.is_empty(),
                "Pattern {} has empty trigger pattern",
                p.id
            );
        }
    }

    #[test]
    fn test_builtin_pattern_ids_are_lowercase_kebab() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert_eq!(
                p.id,
                p.id.to_lowercase(),
                "Pattern ID '{}' is not lowercase",
                p.id
            );
            assert!(!p.id.contains(' '), "Pattern ID '{}' contains spaces", p.id);
        }
    }

    #[test]
    fn test_builtin_categories_covered() {
        use std::collections::HashSet;
        let patterns = BuiltinPatterns::all();
        let categories: HashSet<String> = patterns
            .iter()
            .map(|p| format!("{:?}", p.category))
            .collect();
        // Should have multiple distinct categories
        assert!(
            categories.len() >= 4,
            "Expected at least 4 categories, got {}",
            categories.len()
        );
    }

    #[test]
    fn test_builtin_rust_error_handling_pattern() {
        let patterns = BuiltinPatterns::all();
        let p = patterns
            .iter()
            .find(|p| p.id == "rust-error-handling")
            .unwrap();
        assert_eq!(p.name, "Rust Error Handling");
        assert!(!p.trigger_conditions.is_empty());
        assert!(p.trigger_conditions[0].pattern.contains("unwrap"));
    }

    #[test]
    fn test_builtin_tdd_pattern() {
        let patterns = BuiltinPatterns::all();
        let p = patterns
            .iter()
            .find(|p| p.id == "test-driven-development")
            .unwrap();
        assert_eq!(p.name, "Test-Driven Development");
        assert!(!p.examples.is_empty());
    }

    #[test]
    fn test_builtin_git_workflow_pattern() {
        let patterns = BuiltinPatterns::all();
        let p = patterns.iter().find(|p| p.id == "git-workflow").unwrap();
        assert_eq!(p.name, "Git Workflow");
    }

    #[test]
    fn test_builtin_patterns_examples_not_empty() {
        let patterns = BuiltinPatterns::all();
        for p in &patterns {
            assert!(!p.examples.is_empty(), "Pattern {} has no examples", p.id);
        }
    }

    #[test]
    fn test_builtin_refactor_extract_function() {
        let patterns = BuiltinPatterns::all();
        let p = patterns
            .iter()
            .find(|p| p.id == "refactor-extract-function")
            .unwrap();
        assert!(p.name.contains("Extract"));
    }

    #[test]
    fn test_builtin_optimize_performance() {
        let patterns = BuiltinPatterns::all();
        let p = patterns
            .iter()
            .find(|p| p.id == "optimize-performance")
            .unwrap();
        assert!(p.name.contains("Optimization"));
    }

    #[test]
    fn test_builtin_rust_async_pattern() {
        let patterns = BuiltinPatterns::all();
        let p = patterns
            .iter()
            .find(|p| p.id == "rust-async-patterns")
            .unwrap();
        assert!(p.name.contains("Async"));
    }
}
