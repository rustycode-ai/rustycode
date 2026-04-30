#![allow(
    clippy::if_not_else,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    clippy::unnecessary_wraps
)]

//! Integration tests for the interactive prompt system
//!
//! These tests verify the prompt system works correctly in various scenarios,
//! including validation, default values, and non-interactive mode.

use rustycode_cli::prompt::{Confirm, Input, MultiSelect, PromptConfig, Select, ValidationError};
use std::io;

/// Helper to simulate user input
#[allow(dead_code)]
fn simulate_input(_input: &str) -> io::Result<()> {
    // In a real test environment, you'd use a more sophisticated mocking approach
    // For now, this is a placeholder showing the concept
    Ok(())
}

#[test]
fn test_confirm_creation() {
    let confirm = Confirm::new("Continue?");
    // Test that Confirm can be created
    assert_eq!(confirm.message, "Continue?");
}

#[test]
fn test_confirm_with_default() {
    let confirm = Confirm::new("Test").with_default(true);
    // Test that Confirm can be created with default
    assert_eq!(confirm.message, "Test");
}

#[test]
fn test_select_basic() {
    let _select = Select::<&str>::new("Choose a color")
        .option("Red", "red")
        .option("Green", "green")
        .option("Blue", "blue");

    // Test that Select can be created with options
    let _select2 = Select::<&str>::new("Choose").options(vec![("A", "a"), ("B", "b"), ("C", "c")]);

    // Test that Select can be created with default
    let _select3 = Select::<&str>::new("Choose")
        .option("A", "a")
        .option("B", "b")
        .with_default(1);
}

#[test]
fn test_input_basic() {
    let input = Input::new("Enter name");
    assert_eq!(input.message, "Enter name");
}

#[test]
fn test_input_with_default() {
    let input = Input::new("Enter name").with_default("Guest");
    assert_eq!(input.message, "Enter name");
}

#[test]
fn test_input_validation() {
    let input = Input::new("Enter age").validate(|age| {
        if let Ok(num) = age.parse::<usize>() {
            if !(1..=120).contains(&num) {
                Err("Age must be between 1 and 120".into())
            } else {
                Ok(())
            }
        } else {
            Err("Age must be a number".into())
        }
    });

    // Test that Input can be created with validation
    assert_eq!(input.message, "Enter age");
}

#[test]
fn test_multiselect_basic() {
    let _multiselect = MultiSelect::<&str>::new("Select features")
        .option("Feature 1", "f1")
        .option("Feature 2", "f2")
        .option("Feature 3", "f3");

    // Test that MultiSelect can be created
    let _multiselect2 = MultiSelect::<&str>::new("Select features").options(vec![
        ("Feature 1", "f1"),
        ("Feature 2", "f2"),
        ("Feature 3", "f3"),
    ]);
}

#[test]
fn test_multiselect_with_constraints() {
    let multiselect = MultiSelect::<&str>::new("Select features")
        .option("A", "a")
        .option("B", "b")
        .option("C", "c")
        .min_selections(1)
        .max_selections(2);

    // Test that MultiSelect can be created with constraints
    assert_eq!(multiselect.message, "Select features");
}

#[test]
fn test_multiselect_with_defaults() {
    let multiselect = MultiSelect::<&str>::new("Select features")
        .option("A", "a")
        .option("B", "b")
        .option("C", "c")
        .with_defaults(vec![0, 1]);

    // Test that MultiSelect can be created with defaults
    assert_eq!(multiselect.message, "Select features");
}

#[test]
fn test_prompt_config_global_yes() {
    let config1 = PromptConfig::new();
    assert!(!config1.global_yes_enabled());

    config1.set_global_yes(true);
    assert!(config1.global_yes_enabled());

    config1.set_global_yes(false);
    assert!(!config1.global_yes_enabled());
}

#[test]
fn test_prompt_config_global_static() {
    // Test the global static instance
    let config = PromptConfig::global();
    assert!(!config.global_yes_enabled());

    PromptConfig::set_global_yes_enabled(true);
    assert!(PromptConfig::global().global_yes_enabled());

    PromptConfig::set_global_yes_enabled(false);
    assert!(!PromptConfig::global().global_yes_enabled());
}

#[test]
fn test_validation_error_from_string() {
    let err: ValidationError = "Test error".into();
    assert_eq!(err.message, "Test error");
    assert_eq!(err.to_string(), "Test error");
}

#[test]
fn test_validation_error_from_str() {
    let err = ValidationError::from("Another error");
    assert_eq!(err.message, "Another error");
    assert_eq!(err.to_string(), "Another error");
}

// Integration-style test that demonstrates the API
#[test]
fn test_prompt_api_composition() {
    // This test demonstrates how prompts can be composed and configured

    // Confirm with default
    let confirm = Confirm::new("Continue?")
        .with_default(true)
        .with_config(PromptConfig::new());
    assert_eq!(confirm.message, "Continue?");

    // Select with options and default
    let select = Select::<&str>::new("Choose option")
        .option("Option A", "a")
        .option("Option B", "b")
        .option("Option C", "c")
        .with_default(1)
        .validate(|value| {
            if *value == "invalid" {
                Err("Invalid option".into())
            } else {
                Ok(())
            }
        });
    assert_eq!(select.message, "Choose option");

    // Input with validation
    let input = Input::new("Enter email")
        .with_default("user@example.com")
        .validate(|email| {
            if email.contains('@') {
                Ok(())
            } else {
                Err("Email must contain @".into())
            }
        });
    assert_eq!(input.message, "Enter email");

    // Multi-select with constraints
    let multiselect = MultiSelect::<&str>::new("Select features")
        .option("Feature 1", "f1")
        .option("Feature 2", "f2")
        .option("Feature 3", "f3")
        .with_defaults(vec![0])
        .min_selections(1)
        .max_selections(2);
    assert_eq!(multiselect.message, "Select features");
}

#[test]
fn test_email_validation() {
    // Test a realistic validation scenario
    let input = Input::new("Enter email").validate(|email| {
        if email.contains('@') && email.contains('.') {
            Ok(())
        } else {
            Err("Invalid email format".into())
        }
    });

    assert_eq!(input.message, "Enter email");
}

#[test]
fn test_non_empty_validation() {
    // Test non-empty validation
    let input = Input::new("Enter your name").validate(|name| {
        if name.trim().is_empty() {
            Err("Name cannot be empty".into())
        } else {
            Ok(())
        }
    });

    assert_eq!(input.message, "Enter your name");
}

#[test]
fn test_range_validation() {
    // Test range validation
    let input = Input::new("Enter port number").validate(|port| match port.parse::<u16>() {
        Ok(n) if n >= 1024 => Ok(()),
        Ok(_) => Err("Port must be 1024 or higher".into()),
        Err(_) => Err("Port must be a valid number".into()),
    });

    assert_eq!(input.message, "Enter port number");
}
