//! Tests for tool profile detection accuracy
//!
//! Tests profile detection against labeled examples to measure accuracy.

use crate::tool_selector::ToolProfile;

/// Test case with expected profile
struct ProfileTestCase {
    prompt: &'static str,
    expected: ToolProfile,
    description: &'static str,
}

#[cfg(test)]
mod accuracy_tests {
    use super::*;

    /// Real-world user prompts with their correct classifications
    /// This is our "ground truth" dataset for accuracy measurement
    const TEST_CASES: &[ProfileTestCase] = &[
        // Explore prompts (code discovery)
        ProfileTestCase {
            prompt: "Show me how authentication works",
            expected: ToolProfile::Explore,
            description: "Understanding existing code",
        },
        ProfileTestCase {
            prompt: "What files are in the src directory?",
            expected: ToolProfile::Explore,
            description: "Directory exploration",
        },
        ProfileTestCase {
            prompt: "Find where the user model is defined",
            expected: ToolProfile::Explore,
            description: "Code search",
        },
        ProfileTestCase {
            prompt: "How does the tool selector work?",
            expected: ToolProfile::Explore,
            description: "Architecture understanding",
        },

        // Implement prompts (code changes)
        ProfileTestCase {
            prompt: "Create a new user endpoint",
            expected: ToolProfile::Implement,
            description: "Creating new feature",
        },
        ProfileTestCase {
            prompt: "Fix the authentication bug",
            expected: ToolProfile::Implement,
            description: "Bug fix",
        },
        ProfileTestCase {
            prompt: "Add validation to the form",
            expected: ToolProfile::Implement,
            description: "Adding functionality",
        },
        ProfileTestCase {
            prompt: "Refactor the database layer",
            expected: ToolProfile::Implement,
            description: "Refactoring",
        },
        ProfileTestCase {
            prompt: "Update the README with new instructions",
            expected: ToolProfile::Implement,
            description: "Documentation update",
        },

        // Debug prompts (troubleshooting)
        ProfileTestCase {
            prompt: "Debug the failing test",
            expected: ToolProfile::Debug,
            description: "Test failure investigation",
        },
        ProfileTestCase {
            prompt: "Why is the API returning 500?",
            expected: ToolProfile::Debug,
            description: "Error investigation",
        },
        ProfileTestCase {
            prompt: "The build is broken, investigate",
            expected: ToolProfile::Debug,
            description: "Build failure",
        },
        ProfileTestCase {
            prompt: "Check for memory leaks",
            expected: ToolProfile::Debug,
            description: "Performance debugging",
        },

        // Ops prompts (deployment/operations)
        ProfileTestCase {
            prompt: "Deploy to production",
            expected: ToolProfile::Ops,
            description: "Deployment",
        },
        ProfileTestCase {
            prompt: "Run the test suite",
            expected: ToolProfile::Ops,
            description: "Test execution",
        },
        ProfileTestCase {
            prompt: "Commit these changes",
            expected: ToolProfile::Ops,
            description: "Git operations",
        },
        ProfileTestCase {
            prompt: "Restart the server",
            expected: ToolProfile::Ops,
            description: "Server operations",
        },

        // Ambiguous/edge cases
        ProfileTestCase {
            prompt: "Help",
            expected: ToolProfile::All,
            description: "Too ambiguous",
        },
        ProfileTestCase {
            prompt: "Create a test file to explore the API",
            expected: ToolProfile::Implement, // Primary intent is creating
            description: "Mixed intent - implement should win",
        },
        ProfileTestCase {
            prompt: "What tests should I write?",
            expected: ToolProfile::Explore, // Asking for information
            description: "Question about testing",
        },
    ];

    #[test]
    fn profile_detection_accuracy() {
        let mut correct = 0;
        let mut total = TEST_CASES.len();

        for test_case in TEST_CASES {
            let detected = ToolProfile::from_prompt(test_case.prompt);
            let is_correct = detected == test_case.expected;

            if !is_correct {
                eprintln!(
                    "❌ FAIL: \"{}\"\n   Expected: {:?}, Detected: {:?}\n   Description: {}",
                    test_case.prompt, test_case.expected, detected, test_case.description
                );
            } else {
                correct += 1;
            }
        }

        let accuracy = (correct as f64 / total as f64) * 100.0;
        println!("\n📊 Profile Detection Accuracy: {:.1}% ({}/{})", accuracy, correct, total);

        // Target: 90% accuracy
        assert!(
            accuracy >= 90.0,
            "Profile detection accuracy {:.1}% is below 90% target",
            accuracy
        );
    }

    #[test]
    fn explore_detection_accuracy() {
        let explore_cases: Vec<_> = TEST_CASES
            .iter()
            .filter(|tc| tc.expected == ToolProfile::Explore)
            .collect();

        let correct = explore_cases
            .iter()
            .filter(|tc| ToolProfile::from_prompt(tc.prompt) == ToolProfile::Explore)
            .count();

        let accuracy = (correct as f64 / explore_cases.len() as f64) * 100.0;
        println!(
            "Explore accuracy: {:.1}% ({}/{})",
            accuracy,
            correct,
            explore_cases.len()
        );

        assert!(
            accuracy >= 80.0,
            "Explore detection accuracy too low: {:.1}%",
            accuracy
        );
    }

    #[test]
    fn implement_detection_accuracy() {
        let implement_cases: Vec<_> = TEST_CASES
            .iter()
            .filter(|tc| tc.expected == ToolProfile::Implement)
            .collect();

        let correct = implement_cases
            .iter()
            .filter(|tc| {
                ToolProfile::from_prompt(tc.prompt) == ToolProfile::Implement
            })
            .count();

        let accuracy = (correct as f64 / implement_cases.len() as f64) * 100.0;
        println!(
            "Implement accuracy: {:.1}% ({}/{})",
            accuracy,
            correct,
            implement_cases.len()
        );

        assert!(
            accuracy >= 80.0,
            "Implement detection accuracy too low: {:.1}%",
            accuracy
        );
    }
}
