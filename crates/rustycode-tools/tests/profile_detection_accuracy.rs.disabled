//! Tool profile detection accuracy tests
//!
//! Comprehensive tests to validate that ToolProfile::from_prompt()
//! accurately detects user intent across diverse prompts.

use rustycode_tools::ToolProfile;

#[test]
fn test_profile_detection_explore_intent_keywords() {
    // Test explicit explore intent keywords
    let explore_prompts = vec![
        "Show me how authentication works",
        "What does the UserService do?",
        "Find all uses of the auth function",
        "Explain the architecture of this module",
        "List all files in the src directory",
        "Display the current configuration",
        "Where is the database connection?",
        "How does the caching work?",
        "What's the structure of this project?",
        "Search for TODO comments in the codebase",
    ];

    for prompt in explore_prompts {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected,
            ToolProfile::Explore,
            "Expected Explore for prompt '{}', got {:?}",
            prompt,
            detected
        );
    }
}

#[test]
fn test_profile_detection_implement_intent_keywords() {
    // Test explicit implement intent keywords
    let implement_prompts = vec![
        "Create a new user endpoint",
        "Add authentication to the API",
        "Implement a new feature for user management",
        "Write tests for the auth module",
        "Generate a REST controller for users",
        "Build a new component for the dashboard",
        "Add a new field to the User struct",
        "Create a migration for the orders table",
        "Implement the checkout flow",
        "Add error handling to the service layer",
    ];

    for prompt in implement_prompts {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected,
            ToolProfile::Implement,
            "Expected Implement for prompt '{}', got {:?}",
            prompt,
            detected
        );
    }
}

#[test]
fn test_profile_detection_debug_intent_keywords() {
    // Test explicit debug intent keywords
    let debug_prompts = vec![
        "Fix the failing test in auth module",
        "Debug the authentication error",
        "Why is the login not working?",
        "Investigate the database connection issue",
        "Resolve the panic in main.rs",
        "Diagnose the performance problem",
        "Fix the broken build",
        "Debug why the API returns 500",
        "Troubleshoot the deployment failure",
        "Fix the memory leak in the cache",
    ];

    for prompt in debug_prompts {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected,
            ToolProfile::Debug,
            "Expected Debug for prompt '{}', got {:?}",
            prompt,
            detected
        );
    }
}

#[test]
fn test_profile_detection_ops_intent_keywords() {
    // Test explicit ops intent keywords
    let ops_prompts = vec![
        "Run the test suite",
        "Deploy the application to production",
        "Build the project with cargo build",
        "Start the development server",
        "Execute the database migration",
        "Run cargo check to verify code",
        "Install dependencies",
        "Run the linter",
        "Execute the benchmark tests",
        "Deploy to staging environment",
    ];

    for prompt in ops_prompts {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected,
            ToolProfile::Ops,
            "Expected Ops for prompt '{}', got {:?}",
            prompt,
            detected
        );
    }
}

#[test]
fn test_profile_detection_with_ambiguous_prompts() {
    // Test ambiguous prompts - should default to Explore (safest option)
    let ambiguous_prompts = vec![
        "Help me with the code", // No clear keyword
        "Look at this file",     // Could be explore or debug
        "Work on the feature",   // Vague
        "The authentication",    // Noun phrase
        "User management",       // Noun phrase
    ];

    for prompt in ambiguous_prompts {
        let detected = ToolProfile::from_prompt(prompt);
        // Ambiguous prompts should at least not crash and return a valid profile
        println!("Ambiguous prompt '{}' -> {:?}", prompt, detected);
        // We don't assert specific profile for ambiguous cases,
        // just verify detection doesn't panic
    }
}

#[test]
fn test_profile_detection_accuracy_metric() {
    // Calculate accuracy percentage across diverse prompts
    let test_cases = vec![
        // Explore cases (10)
        ("Show me the auth system", ToolProfile::Explore),
        ("What does function X do?", ToolProfile::Explore),
        ("Find all uses of Y", ToolProfile::Explore),
        ("Explain the architecture", ToolProfile::Explore),
        ("List all files", ToolProfile::Explore),
        ("Display the config", ToolProfile::Explore),
        ("Where is the file?", ToolProfile::Explore),
        ("How does this work?", ToolProfile::Explore),
        ("Search for patterns", ToolProfile::Explore),
        ("What's the structure?", ToolProfile::Explore),
        // Implement cases (10)
        ("Create a new endpoint", ToolProfile::Implement),
        ("Add authentication", ToolProfile::Implement),
        ("Implement feature X", ToolProfile::Implement),
        ("Write tests", ToolProfile::Implement),
        ("Generate code", ToolProfile::Implement),
        ("Build component", ToolProfile::Implement),
        ("Add field to struct", ToolProfile::Implement),
        ("Create migration", ToolProfile::Implement),
        ("Implement flow", ToolProfile::Implement),
        ("Add error handling", ToolProfile::Implement),
        // Debug cases (10)
        ("Fix the failing test", ToolProfile::Debug),
        ("Debug this error", ToolProfile::Debug),
        ("Why is X failing?", ToolProfile::Debug),
        ("Investigate issue", ToolProfile::Debug),
        ("Resolve the panic", ToolProfile::Debug),
        ("Diagnose problem", ToolProfile::Debug),
        ("Fix broken build", ToolProfile::Debug),
        ("Debug the crash", ToolProfile::Debug),
        ("Troubleshoot issue", ToolProfile::Debug),
        ("Fix memory leak", ToolProfile::Debug),
        // Ops cases (10)
        ("Run the tests", ToolProfile::Ops),
        ("Deploy to prod", ToolProfile::Ops),
        ("Build project", ToolProfile::Ops),
        ("Start server", ToolProfile::Ops),
        ("Execute migration", ToolProfile::Ops),
        ("Run cargo check", ToolProfile::Ops),
        ("Install deps", ToolProfile::Ops),
        ("Run linter", ToolProfile::Ops),
        ("Execute benchmark", ToolProfile::Ops),
        ("Deploy to staging", ToolProfile::Ops),
    ];

    let mut correct = 0;
    let total = test_cases.len();

    for (prompt, expected) in test_cases {
        let detected = ToolProfile::from_prompt(prompt);
        if detected == expected {
            correct += 1;
        } else {
            println!(
                "MISMATCH: '{}' -> Expected {:?}, got {:?}",
                prompt, expected, detected
            );
        }
    }

    let accuracy = (correct as f64 / total as f64) * 100.0;
    println!(
        "Profile detection accuracy: {}/{} ({:.1}%)",
        correct, total, accuracy
    );

    // Assert accuracy is at least 90%
    assert!(
        accuracy >= 90.0,
        "Profile detection accuracy {:.1}% is below 90% threshold",
        accuracy
    );
}

#[test]
fn test_profile_detection_case_insensitive() {
    // Test that detection is case-insensitive
    let test_cases = vec![
        ("SHOW ME THE CODE", ToolProfile::Explore),
        ("create a new file", ToolProfile::Implement),
        ("FIX the bug", ToolProfile::Debug),
        ("Run the tests", ToolProfile::Ops),
    ];

    for (prompt, expected) in test_cases {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected, expected,
            "Case-insensitive detection failed for '{}'",
            prompt
        );
    }
}

#[test]
fn test_profile_detection_with_punctuation() {
    // Test that detection works with various punctuation
    let test_cases = vec![
        ("Show me the code?", ToolProfile::Explore),
        ("Create a file.", ToolProfile::Implement),
        ("Fix the bug!", ToolProfile::Debug),
        ("Run the tests...", ToolProfile::Ops),
        ("Show me: the code", ToolProfile::Explore),
        ("Create... a file", ToolProfile::Implement),
    ];

    for (prompt, expected) in test_cases {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected, expected,
            "Punctuation handling failed for '{}'",
            prompt
        );
    }
}

#[test]
fn test_profile_detection_multi_sentence_prompts() {
    // Test detection with multi-sentence prompts
    let test_cases = vec![
        (
            "Show me the auth system. Then explain how it works.",
            ToolProfile::Explore,
        ),
        (
            "First, create a new endpoint. Then add authentication.",
            ToolProfile::Implement,
        ),
        (
            "I need to debug this issue. Can you fix the error?",
            ToolProfile::Debug,
        ),
        ("Run the tests and deploy to production.", ToolProfile::Ops),
    ];

    for (prompt, expected) in test_cases {
        let detected = ToolProfile::from_prompt(prompt);
        assert_eq!(
            detected, expected,
            "Multi-sentence detection failed for '{}'",
            prompt
        );
    }
}
