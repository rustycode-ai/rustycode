//! Thinking message spinner for display.
//!
//! Provides rotating thinking messages during AI reasoning phases, similar to goose's
//! `thinking.rs` pattern. Shows playful messages while the AI is processing.

use rand::prelude::IndexedRandom;

const THINKING_MESSAGES: &[&str] = &[
    // RustyCode-themed thinking messages
    "Compiling thoughts",
    "Analyzing AST nodes",
    "Resolving imports",
    "Checking borrow checker",
    "Optimizing lifetimes",
    "Matching patterns",
    "Traversing module graph",
    "Inferring types",
    "Resolving paths",
    "Tokenizing input",
    "Building symbol table",
    "Linting code",
    "Formatting output",
    "Generating suggestions",
    "Computing fix",
    "Running clippy",
    "Evaluating constraints",
    "Simplifying expression",
    "Inlining function",
    "Monomorphizing",
    "Checking ownership",
    "Validating lifetime",
    "Reordering fields",
    "Flattening control flow",
    "Eliminating dead code",
    "Deduplicating logic",
    "Strengthening types",
    "Weakening constraints",
    "Pushing boundaries",
    "Resolving ambiguity",
    "Unifying interfaces",
    "Normalizing representation",
    "Abstracting complexity",
    "Crystallizing intent",
    "Distilling patterns",
    "Formulating approach",
    "Synthesizing solution",
    "Mapping dependencies",
    "Evaluating candidates",
    "Ranking alternatives",
    "Scoring options",
    "Planning sequence",
    "Orchestrating execution",
    "Balancing trade-offs",
    "Verifying correctness",
    "Establishing invariants",
    "Proving equivalence",
    "Refining strategy",
    "Iterating on design",
    "Converging on answer",
    // Goose-inspired messages
    "Spreading wings",
    "Honking thoughtfully",
    "Waddling to conclusions",
    "Flapping wings excitedly",
    "Preening code feathers",
    "Gathering digital breadcrumbs",
    "Paddling through data",
    "Migrating thoughts",
    "Nesting ideas",
    "Squawking calculations",
    "Consulting the silicon oracle",
    "Pondering existential queries",
    "Processing neural pathways",
    "Exploring decision trees",
    "Reasoning about edge cases",
    "Analyzing dependencies",
    "Tracing call graph",
    "Building mental model",
    "Considering alternatives",
    "Weighing tradeoffs",
    "Mapping control flow",
    "Deducing intent",
    "Inferring types",
    "Organizing structure",
    "Hunting bugs",
    "Chasing pointers",
    "Untangling lifetimes",
    "Unwinding complexity",
    "Unpacking abstractions",
    "Deciphering error messages",
    "Investigating regressions",
    "Searching for invariants",
    "Finding race conditions",
    "Discovering patterns",
    "Optimizing queries",
    "Benchmarking code paths",
    "Profiling hot spots",
    "Reducing allocations",
    "Caching results",
    "Memoizing computations",
    "Parallelizing work",
    "Thinking in circles",
    "Thinking in spirals",
    "Thinking in parallel",
    "Thinking in rust",
    "Thinking in code",
    "Thinking in systems",
    "Thinking in patterns",
    "Thinking in abstractions",
    "Thinking in solutions",
    "Thinking in algorithms",
    "Thinking in data",
    "Thinking in types",
    "Thinking in errors",
    "Thinking in tests",
    "Channeling inner wisdom",
    "Reticulating logic gates",
    "Multiplexing thought streams",
    "Buffering inspiration",
    "Rendering possibilities",
    "Spawning ideas",
    "Joining mental threads",
    // More goose-inspired gems
    "Untangling spaghetti code",
    "Mining thought gems",
    "Defragmenting brain bits",
    "Compiling wisdom",
    "Debugging reality",
    "Baking fresh ideas",
    "Dancing with data",
    "Folding thought origami",
    "Growing solution trees",
    "Kindling knowledge",
    "Levitating logic gates",
    "Manifesting solutions",
    "Serenading semiconductors",
    "Taming tensors",
    "Wrangling widgets",
    "Yodeling yaml",
    "Crafting code crystals",
    "Knitting knowledge knots",
    "Brewing binary brilliance",
    "Pecking at problems",
    "Diving for answers",
    "Herding bytes",
    "Swimming through streams",
    "Hatching clever solutions",
    "Gliding through branches",
];

/// Returns a thinking message by animation frame index.
///
/// Use `frame % len` to cycle through messages predictably.
pub fn thinking_message(frame: usize) -> &'static str {
    let idx = frame % THINKING_MESSAGES.len();
    THINKING_MESSAGES[idx]
}

/// Returns a random thinking message from the list.
pub fn random_thinking_message() -> &'static str {
    let mut rng = rand::rng();
    THINKING_MESSAGES
        .choose(&mut rng)
        .unwrap_or(&THINKING_MESSAGES[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_thinking_message_deterministic() {
        assert_eq!(get_thinking_message(0), "Compiling thoughts");
        assert_eq!(get_thinking_message(1), "Analyzing AST nodes");
        assert_eq!(get_thinking_message(4), "Optimizing lifetimes");
        assert_eq!(
            get_thinking_message(THINKING_MESSAGES.len()),
            "Compiling thoughts"
        );
        assert_eq!(
            get_thinking_message(THINKING_MESSAGES.len() + 1),
            "Analyzing AST nodes"
        );
    }

    #[test]
    fn test_thinking_message_count() {
        assert!(THINKING_MESSAGES.len() >= 130);
    }
}
