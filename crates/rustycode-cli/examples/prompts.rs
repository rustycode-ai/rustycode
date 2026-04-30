#![allow(clippy::too_many_lines, clippy::uninlined_format_args)]

//! Interactive Prompt System Examples
//!
//! This file demonstrates various use cases for the prompt system.
//! Run these examples to see how prompts work in different scenarios.
//!
//! Run with:
//! ```bash
//! cargo run --example prompts
//! ```

use rustycode_cli::prompt::{Confirm, Input, MultiSelect, Prompt, PromptConfig, Select};
use std::io;

fn main() -> io::Result<()> {
    println!("=== RustyCode Prompt System Examples ===\n");

    // Example 1: Simple confirmation
    println!("Example 1: Simple Confirmation");
    let confirmed = Confirm::new("Do you want to continue?").prompt()?;
    println!("You chose: {}\n", confirmed);

    // Example 2: Confirmation with default
    println!("Example 2: Confirmation with Default (Yes)");
    let confirmed = Confirm::new("Delete all files?")
        .with_default(false)
        .prompt()?;
    println!("You chose: {}\n", confirmed);

    // Example 3: Simple selection
    println!("Example 3: Choose from Options");
    let color = Select::new("Choose your favorite color:")
        .option("Red", "red")
        .option("Green", "green")
        .option("Blue", "blue")
        .option("Yellow", "yellow")
        .prompt()?;
    println!("You chose: {}\n", color);

    // Example 4: Selection with validation
    println!("Example 4: Choose with Validation");
    let size = Select::new("Select a file size:")
        .option("Small (10GB)", "small")
        .option("Medium (50GB)", "medium")
        .option("Large (100GB)", "large")
        .validate(|size| {
            if *size == "small" {
                Err("Small size is not recommended for production".into())
            } else {
                Ok(())
            }
        })
        .prompt()?;
    println!("You chose: {}\n", size);

    // Example 5: Text input
    println!("Example 5: Text Input");
    let name = Input::new("Enter your name:").prompt()?;
    println!("Hello, {}!\n", name);

    // Example 6: Text input with default
    println!("Example 6: Text Input with Default");
    let filename = Input::new("Enter filename:")
        .with_default("config.toml")
        .prompt()?;
    println!("Using filename: {}\n", filename);

    // Example 7: Text input with validation
    println!("Example 7: Text Input with Validation");
    let age = Input::new("Enter your age:")
        .validate(|age| match age.parse::<usize>() {
            Ok(num) if !(1..=120).contains(&num) => Err("Age must be between 1 and 120".into()),
            Ok(_) => Ok(()),
            Err(_) => Err("Age must be a valid number".into()),
        })
        .prompt()?;
    println!("Your age: {}\n", age);

    // Example 8: Multi-select
    println!("Example 8: Multi-Select Features");
    let features = MultiSelect::new("Select features to install:")
        .option("Database Support", "db")
        .option("Authentication", "auth")
        .option("Caching", "cache")
        .option("Monitoring", "monitoring")
        .option("Logging", "logging")
        .prompt()?;
    println!("Selected features: {:?}\n", features);

    // Example 9: Multi-select with defaults
    println!("Example 9: Multi-Select with Defaults");
    let tools = MultiSelect::new("Select development tools:")
        .option("Git", "git")
        .option("Docker", "docker")
        .option("VS Code", "vscode")
        .option("IntelliJ IDEA", "idea")
        .with_defaults(vec![0, 1])
        .prompt()?;
    println!("Selected tools: {:?}\n", tools);

    // Example 10: Multi-select with constraints
    println!("Example 10: Multi-Select with Constraints (min 1, max 2)");
    let options = MultiSelect::new("Select backup options:")
        .option("Full Backup", "full")
        .option("Incremental", "incremental")
        .option("Differential", "differential")
        .option("Compression", "compression")
        .min_selections(1)
        .max_selections(2)
        .prompt()?;
    println!("Selected options: {:?}\n", options);

    // Example 11: Email validation
    println!("Example 11: Email Validation");
    let email = Input::new("Enter your email:")
        .validate(|email| {
            if email.contains('@') && email.contains('.') {
                Ok(())
            } else {
                Err("Please enter a valid email address".into())
            }
        })
        .prompt()?;
    println!("Email: {}\n", email);

    // Example 12: Password confirmation
    println!("Example 12: Password Setup");
    let _password = Input::new("Enter password:")
        .validate(|pwd| {
            if pwd.len() < 8 {
                Err("Password must be at least 8 characters".into())
            } else {
                Ok(())
            }
        })
        .prompt()?;

    let confirmed = Confirm::new("Use this password?").prompt()?;

    if confirmed {
        println!("Password set!\n");
    } else {
        println!("Password cancelled.\n");
    }

    println!("=== Examples Complete ===");

    Ok(())
}

/// Example showing non-interactive mode (--yes flag)
#[allow(dead_code)]
fn example_non_interactive() -> io::Result<()> {
    println!("=== Non-Interactive Mode Example ===\n");

    // Enable global yes mode (simulating --yes flag)
    PromptConfig::set_global_yes_enabled(true);

    // These will all use defaults without prompting
    let confirmed = Confirm::new("Continue?").with_default(true).prompt()?;
    println!("Auto-confirmed: {}", confirmed);

    let choice = Select::new("Choose option:")
        .option("Option A", "a")
        .option("Option B", "b")
        .with_default(0)
        .prompt()?;
    println!("Auto-selected: {}", choice);

    let input = Input::new("Enter value:")
        .with_default("default_value")
        .prompt()?;
    println!("Auto-input: {}", input);

    let multi = MultiSelect::new("Select options:")
        .option("A", "a")
        .option("B", "b")
        .with_defaults(vec![0])
        .prompt()?;
    println!("Auto-selected: {:?}", multi);

    // Disable global yes mode
    PromptConfig::set_global_yes_enabled(false);

    println!("=== Non-Interactive Example Complete ===\n");

    Ok(())
}

/// Example showing a complex workflow with multiple prompts
#[allow(dead_code)]
fn example_workflow() -> io::Result<()> {
    println!("=== Complex Workflow Example ===\n");

    // Step 1: Confirm we want to create a new project
    let create_project = Confirm::new("Create a new project?")
        .with_default(true)
        .prompt()?;

    if !create_project {
        println!("Project creation cancelled.");
        return Ok(());
    }

    // Step 2: Get project name
    let project_name = Input::new("Enter project name:")
        .validate(|name| {
            if name.is_empty() {
                Err("Project name cannot be empty".into())
            } else if name.contains(' ') {
                Err("Project name cannot contain spaces".into())
            } else {
                Ok(())
            }
        })
        .prompt()?;

    // Step 3: Select project type
    let project_type = Select::new("Select project type:")
        .option("Web Application", "web")
        .option("CLI Tool", "cli")
        .option("Library", "lib")
        .option("Desktop App", "desktop")
        .prompt()?;

    // Step 4: Select features
    let features = MultiSelect::new("Select features to include:")
        .option("Database", "database")
        .option("Authentication", "auth")
        .option("API", "api")
        .option("Testing Framework", "testing")
        .option("Documentation", "docs")
        .min_selections(1)
        .prompt()?;

    // Step 5: Confirm configuration
    println!("\n=== Configuration Summary ===");
    println!("Project Name: {}", project_name);
    println!("Project Type: {}", project_type);
    println!("Features: {:?}", features);
    println!();

    let confirmed = Confirm::new("Create project with these settings?").prompt()?;

    if confirmed {
        println!("\nProject created successfully!");
    } else {
        println!("\nProject creation cancelled.");
    }

    println!("=== Workflow Complete ===\n");

    Ok(())
}
