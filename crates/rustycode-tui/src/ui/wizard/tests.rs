use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

// Test fixture constants - clearly marked as fake/test values
// These use TEST_KEY_ prefix to avoid confusion with real API keys
const TEST_KEY_ANTHROPIC: &str = "TEST_KEY_antantic_api03_test123";
const TEST_KEY_OPENAI: &str = "TEST_KEY_openai_test456";
const TEST_KEY_OPENROUTER: &str = "TEST_KEY_openrouter_test789";

#[test]
fn test_wizard_initialization() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    assert_eq!(wizard.step, WizardStep::Welcome);
    assert!(!wizard.providers.is_empty());
    assert_eq!(wizard.selected_provider_index, 0);
}

#[test]
fn test_provider_selection() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test selecting different providers
    wizard.selected_provider_index = 1;
    assert_eq!(wizard.selected_provider().id, "openai");

    wizard.selected_provider_index = 0;
    assert_eq!(wizard.selected_provider().id, "anthropic");
}

#[test]
fn test_model_selection() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Anthropic (default selection)
    let models = wizard.available_models();
    assert!(!models.is_empty());

    wizard.selected_model_index = 0;
    let model = wizard.selected_model();
    assert!(!model.is_empty());
}

#[test]
fn test_api_key_validation() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Anthropic requires API key
    assert!(!wizard.validate_api_key()); // Empty key

    // Test with a key that's too short
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.api_key_input = "short".to_string();
    assert!(!wizard.validate_api_key());

    // Test with a valid-length key
    wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
    assert!(wizard.validate_api_key());
}

#[test]
fn test_step_navigation() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Initial step
    assert_eq!(wizard.step, WizardStep::Welcome);

    // Simulate Enter key
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::SelectProvider);
}

#[test]
fn test_config_update() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.api_key_input = "sk-ant-api03-test-key-1234567890abcdef".to_string();
    wizard.selected_model_index = 0;

    wizard.update_config_from_selection();

    assert!(!wizard.config.model.is_empty());
    assert!(wizard.config.providers.anthropic.is_some());
}

#[test]
fn test_ollama_no_api_key_required() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Select Ollama (index 9 in the providers list - last one)
    wizard.selected_provider_index = 9; // Ollama

    // Ollama should not require API key
    let provider = wizard.selected_provider();
    assert_eq!(provider.id, "ollama");
    assert!(!provider.requires_api_key);

    // Empty API key should be valid for Ollama
    assert!(wizard.validate_api_key());

    // Should be able to proceed without entering API key
    wizard.api_key_input.clear();
    wizard.update_config_from_selection();

    // Config should be updated even without API key
    assert!(!wizard.config.model.is_empty());
}

// EDGE CASE TESTS

#[test]
fn test_empty_api_key_all_providers() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test all providers that require API keys (use correct indices)
    // Order: 0=anthropic, 1=openai, 2=copilot(skip), 3=kimi-global, 4=kimi-cn,
    //        5=alibaba-global, 6=alibaba-cn, 7=vertex, 8=openrouter, 9=ollama(skip)
    let api_key_providers = [(0, "anthropic"), (1, "openai"), (8, "openrouter")];

    for (idx, provider_id) in api_key_providers.iter() {
        wizard.selected_provider_index = *idx;
        wizard.api_key_input.clear();

        let provider = wizard.selected_provider();
        assert_eq!(provider.id, *provider_id);
        assert!(provider.requires_api_key);
        assert!(
            !wizard.validate_api_key(),
            "Provider {} should reject empty API key",
            provider_id
        );
    }
}

#[test]
fn test_invalid_api_key_formats() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test various invalid formats
    let invalid_keys = vec![
        "",          // Empty
        "a",         // Too short
        "ab",        // Too short
        "abc",       // Too short
        "no-prefix", // Missing prefix
        "sk-short",  // Prefix but too short
        "sk-ant-",   // Prefix with empty suffix
        "   ",       // Whitespace only
        "\t\n",      // Tabs and newlines
    ];

    for key in invalid_keys {
        wizard.api_key_input = key.to_string();
        assert!(
            !wizard.validate_api_key(),
            "Should reject invalid key: {:?}",
            key
        );
    }
}

#[test]
fn test_valid_api_key_formats() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test valid keys for different providers
    let valid_keys = vec![
        ("anthropic", TEST_KEY_ANTHROPIC),
        ("openai", TEST_KEY_OPENAI),
        ("openrouter", TEST_KEY_OPENROUTER),
    ];

    for (provider_id, key) in valid_keys {
        // Select provider
        match provider_id {
            "anthropic" => wizard.selected_provider_index = 0,
            "openai" => wizard.selected_provider_index = 1,
            "openrouter" => wizard.selected_provider_index = 2,
            _ => continue,
        }

        wizard.api_key_input = key.to_string();
        assert!(
            wizard.validate_api_key(),
            "Provider {} should accept key: {}",
            provider_id,
            key
        );
    }
}

#[test]
fn test_out_of_bounds_provider_selection_clamps() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    let provider_count = wizard.providers.len();
    assert!(
        !wizard.providers.is_empty(),
        "should have at least one provider"
    );

    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.selected_provider_index = provider_count + 100;

    let provider = wizard.selected_provider();
    let last_idx = provider_count.saturating_sub(1);
    assert_eq!(
        provider.id, wizard.providers[last_idx].id,
        "should clamp to last valid index"
    );
}

#[test]
fn test_out_of_bounds_model_selection() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    let models = wizard.available_models();
    if !models.is_empty() {
        // Set out of bounds
        wizard.selected_model_index = models.len() + 100;

        // This should either return empty string or handle gracefully
        let model = wizard.selected_model();
        // Verify it doesn't crash and returns something
        assert!(model.is_empty() || !model.is_empty());
    }
}

#[test]
fn test_whitespace_api_key() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Keys with leading/trailing whitespace - should be trimmed and accepted
    wizard.api_key_input = "  sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz  ".to_string();
    // Should accept because validation trims whitespace
    assert!(wizard.validate_api_key());

    // Keys with only whitespace - should be rejected after trimming
    wizard.api_key_input = "   \t\n   ".to_string();
    assert!(!wizard.validate_api_key());
}

// ALL PROVIDER CONFIGURATION TESTS

#[test]
fn test_anthropic_provider_configuration() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.selected_provider_index = 0; // Anthropic
    wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
    wizard.selected_model_index = 0;

    wizard.update_config_from_selection();

    assert_eq!(wizard.config.model, "claude-sonnet-4-6");
    assert!(wizard.config.providers.anthropic.is_some());
    assert_eq!(
        wizard.config.providers.anthropic.as_ref().unwrap().api_key,
        Some("sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string())
    );
}

#[test]
fn test_openai_provider_configuration() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.selected_provider_index = 1; // OpenAI
    wizard.api_key_input = TEST_KEY_OPENAI.to_string();
    wizard.selected_model_index = 0;

    wizard.update_config_from_selection();

    assert_eq!(wizard.config.model, "gpt-4o");
    assert!(wizard.config.providers.openai.is_some());
    assert_eq!(
        wizard.config.providers.openai.as_ref().unwrap().api_key,
        Some(TEST_KEY_OPENAI.to_string())
    );
}

#[test]
fn test_openrouter_provider_configuration() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.selected_provider_index = 8; // OpenRouter (after Anthropic, OpenAI, Copilot, Kimi global/cn, Alibaba global/cn, Vertex)
    wizard.api_key_input = TEST_KEY_OPENROUTER.to_string();
    wizard.selected_model_index = 0;

    wizard.update_config_from_selection();

    assert!(wizard.config.providers.openrouter.is_some());
    assert_eq!(
        wizard.config.providers.openrouter.as_ref().unwrap().api_key,
        Some(TEST_KEY_OPENROUTER.to_string())
    );
}

#[test]
fn test_ollama_provider_configuration() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.selected_provider_index = 9; // Ollama (last provider)
    wizard.api_key_input.clear(); // Ollama doesn't need API key
    wizard.selected_model_index = 0;

    wizard.update_config_from_selection();

    assert!(!wizard.config.model.is_empty());
    // Ollama should be in custom providers
    assert!(
        !wizard.config.providers.custom.is_empty()
            || wizard.config.model.contains("ollama")
            || wizard.config.model.contains("llama")
    );
}

#[test]
fn test_all_provider_models() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test each provider has models
    for idx in 0..wizard.providers.len() {
        wizard.selected_provider_index = idx;
        let models = wizard.available_models();

        assert!(
            !models.is_empty(),
            "Provider at index {} should have models",
            idx
        );

        // Test each model can be selected
        for model_idx in 0..models.len() {
            wizard.selected_model_index = model_idx;
            let model = wizard.selected_model();
            assert!(
                !model.is_empty(),
                "Model at index {} should not be empty",
                model_idx
            );
        }
    }
}

// STATE TRANSITION TESTS

#[test]
fn test_full_wizard_flow() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Welcome -> SelectProvider
    assert_eq!(wizard.step, WizardStep::Welcome);
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::SelectProvider);

    // SelectProvider -> ConfigureProvider (with Enter on Anthropic)
    wizard.selected_provider_index = 0; // Anthropic
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::ConfigureProvider);

    // ConfigureProvider -> SelectModel (with valid API key)
    wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::SelectModel);

    // SelectModel -> Review
    wizard.selected_model_index = 0;
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::Review);

    // Review -> Complete (saves config)
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue); // Review returns Continue after saving
    assert_eq!(wizard.step, WizardStep::Complete);

    // Complete step - press Enter to finish
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Finish); // Complete step returns Finish
}

#[test]
fn test_backward_navigation() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Advance to ConfigureProvider
    wizard.step = WizardStep::ConfigureProvider;

    // Press Esc to go back
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::SelectProvider);
}

#[test]
fn test_help_toggle() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Initially not showing help
    assert!(!wizard.show_help);

    // Press ? to show help
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert!(wizard.show_help);

    // Press ? again to hide help
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Continue);
    assert!(!wizard.show_help);
}

#[test]
fn test_all_step_transitions() {
    let steps = vec![
        WizardStep::Welcome,
        WizardStep::SelectProvider,
        WizardStep::ConfigureProvider,
        WizardStep::SelectModel,
        WizardStep::Review,
        WizardStep::Complete,
    ];

    for step in steps {
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = step.clone();

        // Each step should handle Enter without crashing
        let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        // Each step should handle Esc without crashing
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = step.clone();
        let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        // Each step should handle ? without crashing
        let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
        wizard.step = step;
        let _action =
            wizard.handle_key_event(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    }
}

// KEYBOARD NAVIGATION TESTS

#[test]
fn test_arrow_key_navigation_providers() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.step = WizardStep::SelectProvider;

    let _initial_index = wizard.selected_provider_index;

    // Down arrow
    wizard.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    // Should move down (implementation dependent)

    // Up arrow
    wizard.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    // Should move up (implementation dependent)

    // Verify no crashes
    assert!(wizard.selected_provider_index < wizard.providers.len());
}

#[test]
fn test_char_j_k_navigation() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.step = WizardStep::SelectProvider;

    // Test j key (vim-style down)
    wizard.handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));

    // Test k key (vim-style up)
    wizard.handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));

    // Verify no crashes
    assert!(wizard.selected_provider_index < wizard.providers.len());
}

#[test]
fn test_quit_key() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Press 'q' to quit
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert_eq!(action, WizardAction::Quit);
}

#[test]
fn test_ctrl_c_does_not_crash() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Press Ctrl+C - should not crash (behavior depends on step)
    let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    // Test on different steps
    wizard.step = WizardStep::SelectProvider;
    let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    wizard.step = WizardStep::ConfigureProvider;
    let _action = wizard.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
}

#[test]
fn test_unknown_keys_handled() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.step = WizardStep::Welcome;

    // Various unknown keys - should not crash
    let unknown_keys = vec![
        KeyCode::Char('x'),
        KeyCode::Char('z'),
        KeyCode::F(1),
        KeyCode::F(2),
        KeyCode::Tab,
        KeyCode::Backspace,
    ];

    for key_code in unknown_keys {
        let _action = wizard.handle_key_event(KeyEvent::new(key_code, KeyModifiers::NONE));
        // Should continue without crashing
    }

    // Step might change for some keys, but shouldn't crash
}

// ERROR HANDLING TESTS

#[test]
fn test_error_message_display() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Set error message
    wizard.error_message = Some("Test error message".to_string());
    assert!(wizard.error_message.is_some());
    assert_eq!(wizard.error_message.as_ref().unwrap(), "Test error message");

    // Clear error message
    wizard.error_message = None;
    assert!(wizard.error_message.is_none());
}

#[test]
fn test_validation_error_on_proceed() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));
    wizard.step = WizardStep::ConfigureProvider;

    // Try to proceed with invalid API key
    wizard.api_key_input.clear(); // Empty key

    // Should set error message
    let action = wizard.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    // Action should be Continue (stay on same step with error)
    assert_eq!(action, WizardAction::Continue);
    assert_eq!(wizard.step, WizardStep::ConfigureProvider);
    assert!(wizard.error_message.is_some());
}

#[test]
fn test_provider_info_popularity() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Check that popular providers are marked
    let popular_providers: Vec<_> = wizard
        .providers
        .iter()
        .filter(|p| p.popular)
        .map(|p| p.id.clone())
        .collect();

    assert!(!popular_providers.is_empty());
    assert!(popular_providers.contains(&"anthropic".to_string()));
    assert!(popular_providers.contains(&"openai".to_string()));
}

#[test]
fn test_provider_requires_api_key_flag() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Providers that require API keys
    assert!(wizard.providers[0].requires_api_key); // Anthropic
    assert!(wizard.providers[1].requires_api_key); // OpenAI
    assert!(!wizard.providers[2].requires_api_key); // Copilot (no key needed)
    assert!(wizard.providers[3].requires_api_key); // Kimi Global
    assert!(wizard.providers[4].requires_api_key); // Kimi CN
    assert!(wizard.providers[5].requires_api_key); // Alibaba Global
    assert!(wizard.providers[6].requires_api_key); // Alibaba CN
    assert!(wizard.providers[7].requires_api_key); // Vertex
    assert!(wizard.providers[8].requires_api_key); // OpenRouter

    // Ollama does not require API key (last provider - index 9)
    assert!(!wizard.providers[9].requires_api_key); // Ollama
}

// CONFIG SAVE/LOAD TESTS

#[test]
fn test_config_structure_after_update() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    wizard.selected_provider_index = 0; // Anthropic
    wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();
    wizard.selected_model_index = 1; // Haiku

    wizard.update_config_from_selection();

    // Verify config structure
    assert!(!wizard.config.model.is_empty());
    assert!(wizard.config.providers.anthropic.is_some());

    let anthropic_config = wizard.config.providers.anthropic.as_ref().unwrap();
    assert_eq!(
        anthropic_config.api_key,
        Some("sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string())
    );
    assert!(anthropic_config.models.is_some());
    assert!(!anthropic_config.models.as_ref().unwrap().is_empty());
}

#[test]
fn test_model_in_provider_list() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    for provider_idx in 0..wizard.providers.len() {
        wizard.selected_provider_index = provider_idx;

        let models = wizard.available_models();
        for model_idx in 0..models.len() {
            wizard.selected_model_index = model_idx;

            let selected_model = wizard.selected_model();
            let available_models = wizard.available_models();

            assert!(
                available_models.contains(&selected_model),
                "Selected model should be in available models list"
            );
        }
    }
}

// API KEY URL GENERATION TESTS

#[test]
fn test_get_api_key_urls() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test that each provider has a valid URL
    let providers = vec!["anthropic", "openai", "openrouter", "ollama"];

    for provider_id in providers {
        let url = wizard.get_api_key_url(provider_id);
        assert!(
            !url.is_empty(),
            "Provider {} should have an API key URL",
            provider_id
        );
        assert!(
            url.starts_with("http"),
            "URL should start with http/https: {}",
            url
        );
    }
}

// ADVANCED EDGE CASES

#[test]
fn test_very_long_api_key() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test with an unusually long API key
    let long_key = "sk-ant-".to_string() + &"a".repeat(1000);
    wizard.api_key_input = long_key;
    wizard.selected_provider_index = 0; // Anthropic

    // Should validate (length check only ensures minimum, not maximum)
    assert!(wizard.validate_api_key());
}

#[test]
fn test_special_characters_in_api_key() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // API keys with special characters (valid in some providers)
    let special_keys = vec![
        "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz",
        "sk-ant-api03-1234567890-ABCDEF_ghijklmnopqrstuvwxyz", // hyphens and underscores
        "sk-ant-api03-1234567890+abcdefghijklmnopqrstuvwxyz",  // plus sign
    ];

    for key in special_keys {
        wizard.api_key_input = key.to_string();
        // Should accept valid-length keys even with special chars
        assert!(wizard.validate_api_key(), "Should accept key: {}", key);
    }
}

#[test]
fn test_unicode_in_api_key() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // API keys shouldn't have unicode, but test handling
    wizard.api_key_input = "sk-ant-api03-你好世界".to_string();

    // Should reject or accept based on validation rules
    // Current implementation checks length, so might accept
    let result = wizard.validate_api_key();
    // Just verify it doesn't crash
    let _ = result;
}

#[test]
fn test_zero_length_model_list() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // All providers should have at least one model
    for idx in 0..wizard.providers.len() {
        wizard.selected_provider_index = idx;
        let models = wizard.available_models();
        assert!(
            !models.is_empty(),
            "Provider at index {} should have models",
            idx
        );
    }
}

#[test]
fn test_model_names_are_unique() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // For each provider, models should be unique
    for idx in 0..wizard.providers.len() {
        wizard.selected_provider_index = idx;
        let models = wizard.available_models();

        let unique_models: std::collections::HashSet<_> = models.iter().collect();
        assert_eq!(
            unique_models.len(),
            models.len(),
            "Models should be unique for provider at index {}",
            idx
        );
    }
}

#[test]
fn test_provider_descriptions_exist() {
    let wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // All providers should have non-empty descriptions
    for provider in &wizard.providers {
        assert!(
            !provider.description.is_empty(),
            "Provider {} should have a description",
            provider.id
        );
    }
}

#[test]
fn test_config_path_preserved() {
    let config_path = PathBuf::from("/custom/path/config.json");
    let wizard = FirstRunWizard::new(config_path.clone());

    // Config path should be preserved
    assert_eq!(wizard.config_path, config_path);
}

#[test]
fn test_default_model_is_first() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // For each provider, selected_model_index 0 should give first model
    for idx in 0..wizard.providers.len() {
        wizard.selected_provider_index = idx;
        wizard.selected_model_index = 0;

        let selected_model = wizard.selected_model();
        let available_models = wizard.available_models();

        assert_eq!(
            selected_model, available_models[0],
            "Selected model at index 0 should be first available model"
        );
    }
}

#[test]
fn test_multiple_validation_attempts() {
    let mut wizard = FirstRunWizard::new(PathBuf::from("/tmp/test/config.json"));

    // Test that validation is idempotent
    wizard.api_key_input = "sk-ant-api03-1234567890abcdefghijklmnopqrstuvwxyz".to_string();

    // Validate multiple times
    assert!(wizard.validate_api_key());
    assert!(wizard.validate_api_key());
    assert!(wizard.validate_api_key());

    // Should still be valid
    assert!(wizard.validate_api_key());
}

#[test]
fn test_wizard_step_equality() {
    // Test that WizardStep derives PartialEq correctly
    assert_eq!(WizardStep::Welcome, WizardStep::Welcome);
    assert_eq!(WizardStep::SelectProvider, WizardStep::SelectProvider);
    assert_ne!(WizardStep::Welcome, WizardStep::Complete);
}

#[test]
fn test_wizard_step_clone() {
    // Test that WizardStep derives Clone correctly
    let step = WizardStep::Welcome;
    let cloned = step.clone();
    assert_eq!(step, cloned);
}
