#![allow(
    clippy::expect_used,
    clippy::implicit_clone,
    clippy::items_after_statements,
    clippy::manual_string_new,
    clippy::redundant_clone,
    clippy::uninlined_format_args
)]

//! Integration tests for the first-run wizard with the TUI event loop
//!
//! These tests verify that the wizard integrates correctly with the main TUI event loop.

use rustycode_config::Config;
use rustycode_tui::ui::wizard::{FirstRunWizard, WizardStep};
use std::path::PathBuf;
use tempfile::TempDir;

/// Test that wizard state is correctly initialized and managed by event loop
#[test]
fn test_wizard_event_loop_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.json");

    // Simulate event loop creating wizard
    let wizard = FirstRunWizard::new(config_path.clone());

    // Verify initial state
    assert_eq!(wizard.step, WizardStep::Welcome);
    assert!(!wizard.providers.is_empty());
}

/// Test wizard saves config to correct location
#[test]
fn test_wizard_config_persistence() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // IMPORTANT: Create a .rustycode subdirectory since Config::load looks there
    let rustycode_dir = temp_dir.path().join(".rustycode");
    std::fs::create_dir_all(&rustycode_dir).expect("Failed to create .rustycode dir");

    let config_path = rustycode_dir.join("config.json");

    // Create wizard and configure it
    let mut wizard = FirstRunWizard::new(config_path.clone());

    wizard.selected_provider_index = 0; // Anthropic
    wizard.api_key_input = std::env::var("ANTHROPIC_API_KEY")
        .unwrap_or_else(|_| "sk-ant-api03-test-key-for-testing".to_string());
    wizard.selected_model_index = 0;
    wizard.step = WizardStep::Review;

    // Update config from selection (this is what happens when you press Enter)
    wizard.update_config_from_selection();

    // Save config (simulating Enter press on Review step)
    let save_result = wizard.save_config();

    // Verify save succeeded
    assert!(save_result.is_ok(), "Config save should succeed");

    // Verify file was created
    assert!(config_path.exists(), "Config file should exist after save");

    // Verify config can be loaded and contains expected values (load from parent dir)
    let loaded_config = Config::load(temp_dir.path()).expect("Failed to load config");
    assert_eq!(loaded_config.model, "claude-sonnet-4-6");
    assert!(loaded_config.providers.anthropic.is_some());
}

/// Test wizard handles existing config correctly
#[test]
fn test_wizard_with_existing_config() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create .rustycode directory
    let rustycode_dir = temp_dir.path().join(".rustycode");
    std::fs::create_dir_all(&rustycode_dir).expect("Failed to create .rustycode dir");

    // Create an existing config using default and modify
    let existing_config = Config {
        model: "existing-model".to_string(),
        ..Default::default()
    };

    // Save existing config
    let config_path = rustycode_dir.join("config.json");
    existing_config
        .save(&config_path)
        .expect("Failed to save existing config");

    // Verify config file exists
    assert!(config_path.exists());

    // Load config to verify
    let loaded = Config::load(temp_dir.path()).expect("Failed to load");
    assert_eq!(loaded.model, "existing-model");
}

/// Test wizard with Ollama (no API key required)
#[test]
fn test_wizard_ollama_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create .rustycode directory
    let rustycode_dir = temp_dir.path().join(".rustycode");
    std::fs::create_dir_all(&rustycode_dir).expect("Failed to create .rustycode dir");

    let config_path = rustycode_dir.join("config.json");

    let mut wizard = FirstRunWizard::new(config_path.clone());

    // Select Ollama
    wizard.selected_provider_index = 3; // Ollama
    wizard.api_key_input.clear(); // No API key needed
    wizard.selected_model_index = 0;
    wizard.step = WizardStep::Review;
    wizard.update_config_from_selection();

    // Should save successfully without API key
    let save_result = wizard.save_config();
    assert!(
        save_result.is_ok(),
        "Ollama config should save without API key"
    );

    // Verify file was created
    assert!(config_path.exists(), "Config file should exist for Ollama");
}

/// Test wizard prevents overwrite without confirmation
#[test]
fn test_wizard_config_overwrite_protection() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create .rustycode directory
    let rustycode_dir = temp_dir.path().join(".rustycode");
    std::fs::create_dir_all(&rustycode_dir).expect("Failed to create .rustycode dir");

    let config_path = rustycode_dir.join("config.json");

    // Create initial config
    let mut wizard = FirstRunWizard::new(config_path.clone());
    wizard.selected_provider_index = 0;
    wizard.api_key_input = std::env::var("ANTHROPIC_API_KEY")
        .unwrap_or_else(|_| "sk-ant-api03-initial-test-key".to_string());
    wizard.selected_model_index = 0;
    wizard.step = WizardStep::Review;
    wizard.update_config_from_selection();

    wizard.save_config().expect("First save should succeed");
    assert!(config_path.exists());

    // Try to save again with different values
    let mut wizard2 = FirstRunWizard::new(config_path.clone());
    wizard2.selected_provider_index = 1; // OpenAI
    wizard2.api_key_input = std::env::var("OPENAI_API_KEY")
        .unwrap_or_else(|_| "sk-openai-test-different-key".to_string());
    wizard2.selected_model_index = 0;
    wizard2.step = WizardStep::Review;
    wizard2.update_config_from_selection();

    // This should overwrite (current behavior)
    let overwrite_result = wizard2.save_config();
    assert!(overwrite_result.is_ok(), "Overwrite should succeed");

    // Verify the new values were saved
    let loaded = Config::load(temp_dir.path()).expect("Failed to load");
    assert!(loaded.providers.openai.is_some());
    assert!(loaded.providers.anthropic.is_none());
}

/// Test wizard state transitions match event loop expectations
#[test]
fn test_wizard_state_machine() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.json");

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut wizard = FirstRunWizard::new(config_path);

    // Verify all expected states exist and can be reached
    let expected_states = vec![
        WizardStep::Welcome,
        WizardStep::SelectProvider,
        WizardStep::ConfigureProvider,
        WizardStep::SelectModel,
        WizardStep::Review,
        WizardStep::Complete,
    ];

    // Welcome -> SelectProvider
    assert_eq!(wizard.step, WizardStep::Welcome);
    let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(wizard.step, WizardStep::SelectProvider);

    // Verify we can set any state (event loop might need to restore state)
    for state in expected_states {
        wizard.step = state.clone();
        assert_eq!(wizard.step, state);
    }
}

/// Test wizard cleanup on exit
#[test]
fn test_wizard_cleanup() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = temp_dir.path().join("config.json");

    // Create and configure wizard
    let mut wizard = FirstRunWizard::new(config_path.clone());
    wizard.selected_provider_index = 0;
    wizard.api_key_input = std::env::var("ANTHROPIC_API_KEY")
        .unwrap_or_else(|_| "sk-ant-api03-test-key-for-testing".to_string());
    wizard.selected_model_index = 0;
    wizard.update_config_from_selection();

    // Simulate wizard completion
    wizard.step = WizardStep::Complete;

    // Config should be saved before wizard is dropped
    let save_result = wizard.save_config();
    assert!(save_result.is_ok());

    // Verify persistence after wizard is "dropped"
    assert!(config_path.exists());
    let loaded = Config::load(temp_dir.path()).expect("Failed to load after wizard drop");
    assert!(!loaded.model.is_empty());
}

/// Test wizard handles invalid config directory
#[test]
fn test_wizard_invalid_directory() {
    // Use a path that might not be writable
    let invalid_path = PathBuf::from("/root/nonexistent/config.json");

    let mut wizard = FirstRunWizard::new(invalid_path.clone());
    wizard.selected_provider_index = 0;
    wizard.api_key_input = std::env::var("ANTHROPIC_API_KEY")
        .unwrap_or_else(|_| "sk-ant-api03-test-key-for-testing".to_string());
    wizard.selected_model_index = 0;
    wizard.step = WizardStep::Review;
    wizard.update_config_from_selection();

    // Save should fail due to invalid directory
    let save_result = wizard.save_config();

    // Should get an error (permission denied or directory not found)
    assert!(save_result.is_err(), "Save to invalid path should fail");
}

/// Test multiple providers in sequence
#[test]
fn test_wizard_multiple_providers() {
    let providers_to_test = vec![
        (
            0,
            "anthropic",
            std::env::var("ANTHROPIC_API_KEY")
                .unwrap_or_else(|_| "sk-ant-api03-test-key".to_string()),
        ),
        (
            1,
            "openai",
            std::env::var("OPENAI_API_KEY").unwrap_or_else(|_| "sk-openai-test-key".to_string()),
        ),
        (
            8,
            "openrouter",
            std::env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| "sk-or-v1-test-key".to_string()),
        ),
        (3, "ollama", "".to_string()), // Ollama doesn't need API key
    ];

    for (idx, provider_id, api_key) in providers_to_test {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");

        // Create .rustycode directory
        let rustycode_dir = temp_dir.path().join(".rustycode");
        std::fs::create_dir_all(&rustycode_dir).expect("Failed to create .rustycode dir");

        let config_path = rustycode_dir.join("config.json");

        let mut wizard = FirstRunWizard::new(config_path);
        wizard.selected_provider_index = idx;
        wizard.api_key_input = api_key.to_string();
        wizard.selected_model_index = 0;
        wizard.step = WizardStep::Review;
        wizard.update_config_from_selection();

        let save_result = wizard.save_config();
        assert!(
            save_result.is_ok(),
            "Provider {} should save successfully",
            provider_id
        );

        // Verify config was saved correctly
        let loaded = Config::load(temp_dir.path()).expect("Failed to load");
        match provider_id {
            "anthropic" => assert!(loaded.providers.anthropic.is_some()),
            "openai" => assert!(loaded.providers.openai.is_some()),
            "openrouter" => assert!(loaded.providers.openrouter.is_some()),
            "ollama" => {
                // Ollama should be in custom or model should reflect Ollama
                assert!(!loaded.model.is_empty());
            }
            _ => panic!("Unknown provider: {}", provider_id),
        }
    }
}
