//! Random worktree name generator.
//!
//! Produces names in the pattern: adjective-verbing-noun
//! e.g. "noble-roaming-karp", "swift-whistling-matsumoto"
//!
//! Matches orchestra-2's worktree-name-gen.ts implementation.

use rand::RngExt;

/// Adjectives for worktree names
const ADJECTIVES: &[&str] = &[
    "agile", "bold", "brave", "bright", "calm", "clear", "cool", "crisp", "dapper", "eager",
    "fair", "fast", "fierce", "fine", "fleet", "fond", "gentle", "glad", "grand", "happy", "keen",
    "kind", "lively", "lucid", "mellow", "merry", "mighty", "neat", "nimble", "noble", "plucky",
    "polite", "proud", "quiet", "rapid", "ready", "serene", "sharp", "sleek", "sleepy", "smooth",
    "snappy", "steady", "sturdy", "sunny", "sure", "swift", "tidy", "tough", "tranquil", "vivid",
    "warm", "wise", "witty", "zesty",
];

/// Verbs (gerund form) for worktree names
const VERBS: &[&str] = &[
    "baking",
    "bouncing",
    "building",
    "carving",
    "chasing",
    "climbing",
    "coding",
    "crafting",
    "dancing",
    "dashing",
    "diving",
    "drawing",
    "dreaming",
    "drifting",
    "drumming",
    "exploring",
    "fishing",
    "floating",
    "flying",
    "forging",
    "gliding",
    "growing",
    "hiking",
    "humming",
    "jumping",
    "juggling",
    "knitting",
    "laughing",
    "leaping",
    "mapping",
    "mixing",
    "painting",
    "planting",
    "playing",
    "racing",
    "reading",
    "riding",
    "roaming",
    "rowing",
    "running",
    "sailing",
    "singing",
    "skating",
    "sketching",
    "spinning",
    "squishing",
    "surfing",
    "swimming",
    "thinking",
    "threading",
    "tracing",
    "walking",
    "weaving",
    "whistling",
    "writing",
];

/// Nouns for worktree names
const NOUNS: &[&str] = &[
    "atlas",
    "aurora",
    "balloon",
    "beacon",
    "bolt",
    "brook",
    "canyon",
    "cedar",
    "comet",
    "cook",
    "coral",
    "cosmos",
    "crest",
    "dawn",
    "delta",
    "echo",
    "ember",
    "falcon",
    "fern",
    "flare",
    "frost",
    "gale",
    "glacier",
    "grove",
    "harbor",
    "hawk",
    "horizon",
    "iris",
    "jade",
    "karp",
    "lantern",
    "lark",
    "luna",
    "maple",
    "marsh",
    "matsumoto",
    "mesa",
    "nebula",
    "oasis",
    "orbit",
    "otter",
    "pebble",
    "phoenix",
    "pine",
    "prism",
    "puppy",
    "quartz",
    "raven",
    "reef",
    "ridge",
    "river",
    "sage",
    "shore",
    "sierra",
    "spark",
    "sprout",
    "stone",
    "summit",
    "thorn",
    "tide",
    "topaz",
    "trail",
    "vale",
    "violet",
    "wave",
    "willow",
    "zenith",
];

/// Generate a random worktree name
///
/// Produces names in the pattern: `adjective-verbing-noun`
///
/// # Examples
/// ```
/// use rustycode_orchestration::isolation::worktree_name_gen::generate_worktree_name;
///
/// let name = generate_worktree_name();
/// // Returns: "noble-roaming-karp"
/// // Or: "swift-whistling-matsumoto"
/// // Or: "brave-flying-phoenix"
/// ```
///
pub fn generate_worktree_name() -> String {
    let mut rng = rand::rng();

    let adjective = pick(ADJECTIVES, &mut rng);
    let verb = pick(VERBS, &mut rng);
    let noun = pick(NOUNS, &mut rng);

    format!("{adjective}-{verb}-{noun}")
}

/// Pick a random element from an array using the provided RNG
fn pick<'a, T>(arr: &'a [T], rng: &mut impl RngExt) -> &'a T {
    &arr[rng.random_range(0..arr.len())]
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_worktree_name_format() {
        let name = generate_worktree_name();

        // Check format: adjective-verbing-noun
        assert_eq!(name.split('-').count(), 3);

        // Check that all parts are lowercase
        assert!(name.chars().all(|c| c.is_lowercase() || c == '-'));
    }

    #[test]
    fn test_generate_worktree_name_multiple() {
        // Generate multiple names and verify they're different
        let names = std::iter::repeat_with(generate_worktree_name)
            .take(100)
            .collect::<Vec<_>>();

        // Check that we got 100 names
        assert_eq!(names.len(), 100);

        // Check that all names are valid format
        for name in &names {
            assert_eq!(name.split('-').count(), 3, "Invalid name format: {name}");
        }

        // Check that there's at least some variety (not all the same)
        let unique_names: std::collections::HashSet<_> = names.iter().collect();
        assert!(unique_names.len() > 1, "All names are the same");
    }

    #[test]
    fn test_adjectives_not_empty() {
        assert!(!ADJECTIVES.is_empty());
        assert!(ADJECTIVES.len() > 50);
    }

    #[test]
    fn test_verbs_not_empty() {
        assert!(!VERBS.is_empty());
        assert!(VERBS.len() > 40);
    }

    #[test]
    fn test_nouns_not_empty() {
        assert!(!NOUNS.is_empty());
        assert!(NOUNS.len() > 50);
    }

    #[test]
    fn test_all_words_lowercase() {
        // Verify all word lists are lowercase
        for adj in ADJECTIVES {
            assert_eq!(*adj, adj.to_lowercase());
        }

        for verb in VERBS {
            assert_eq!(*verb, verb.to_lowercase());
        }

        for noun in NOUNS {
            assert_eq!(*noun, noun.to_lowercase());
        }
    }

    #[test]
    fn test_verbs_ending_with_ing() {
        // Most verbs should end with "ing" (gerund form)
        let ing_count = VERBS.iter().filter(|v| v.ends_with("ing")).count();
        assert!(
            ing_count > VERBS.len() / 2,
            "Not enough verbs end with 'ing'"
        );
    }

    #[test]
    fn test_pick_from_array() {
        let mut rng = rand::rng();
        let arr = vec!["a", "b", "c", "d", "e"];
        let result = pick(&arr, &mut rng);

        // Result should be one of the array elements
        assert!(arr.contains(result));
    }

    #[test]
    fn test_generate_worktree_name_determinism_with_seed() {
        // We can't easily test determinism without setting a seed
        // But we can verify that the function produces valid output
        for _ in 0..10 {
            let name = generate_worktree_name();
            assert_eq!(name.split('-').count(), 3);
        }
    }

    #[test]
    fn test_example_names() {
        // Verify the examples from the docstring are possible
        let contains = |word: &str| {
            ADJECTIVES.contains(&word) || VERBS.contains(&word) || NOUNS.contains(&word)
        };

        // "noble-roaming-karp"
        assert!(contains("noble"));
        assert!(contains("roaming"));
        assert!(contains("karp"));

        // "swift-whistling-matsumoto"
        assert!(contains("swift"));
        assert!(contains("whistling"));
        assert!(contains("matsumoto"));
    }
}
