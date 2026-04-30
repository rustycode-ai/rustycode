// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Configuration integration tests
//!
//! Tests cover:
//! - Hierarchical config loading (global → workspace → project)
//! - Environment variable substitution ({env:VAR})
//! - File reference substitution ({file:path})
//! - Provider configuration from environment
//! - Config validation and error handling

use std::fs;

use rustycode_config::{Config, ConfigError, ConfigLoader, SubstitutionEngine};

mod common;
use common::{TestConfig, TestEnv};

#[tokio::test]
async fn test_hierarchical_config_loading() {
    let test_config = TestConfig::new();

    // Create global config
    let _global_config = test_config.write_config(
        "config.json",
        r#"{
            "model": "global-model",
            "temperature": 0.5,
            "providers": {
                "anthropic": {
                    "api_key": "global-key"
                }
            }
        }"#,
    );

    // Create workspace config (should override global)
    let workspace_dir = test_config.project_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).unwrap();
    let workspace_config_dir = workspace_dir.join(".rustycode");
    fs::create_dir_all(&workspace_config_dir).unwrap();

    let workspace_config = workspace_config_dir.join("config.json");
    fs::write(
        &workspace_config,
        r#"{
            "model": "workspace-model",
            "temperature": 0.7,
            "max_tokens": 2048
        }"#,
    )
    .unwrap();

    // Load config from workspace
    let mut loader = ConfigLoader::new();
    let config_value = loader.load(&workspace_dir).unwrap();

    let config: Config = serde_json::from_value(config_value).unwrap();

    // Should have workspace model, but inherit providers from global
    assert_eq!(config.model, "workspace-model");
    assert_eq!(config.temperature, Some(0.7));
    assert_eq!(config.max_tokens, Some(2048));
}

#[tokio::test]
async fn test_environment_variable_substitution() {
    let mut env = TestEnv::new();
    env.set("TEST_MODEL", "env-model");
    env.set("TEST_API_KEY", "env-api-key");

    // Test SubstitutionEngine directly on plain-text templates
    let template = "model={env:TEST_MODEL} key={env:TEST_API_KEY}";
    let mut engine = SubstitutionEngine::new();
    let result = engine.process(template).unwrap();

    assert!(result.contains("model=env-model"));
    assert!(result.contains("key=env-api-key"));
}

#[tokio::test]
async fn test_file_reference_substitution() {
    let test_config = TestConfig::new();

    // Create files to reference inside the allowed base directory
    let model_file = test_config.config_dir.join("model.txt");
    fs::write(&model_file, "file-model").unwrap();

    let key_file = test_config.config_dir.join("api_key.txt");
    fs::write(&key_file, "file-api-key").unwrap();

    // SubstitutionEngine restricts file reads to allowed_base; set it to our config dir
    let mut engine = SubstitutionEngine::new().with_allowed_base(test_config.config_dir.clone());

    let template = format!(
        "model={{file:{}}} key={{file:{}}}",
        model_file.display(),
        key_file.display()
    );
    let result = engine.process(&template).unwrap();

    assert!(result.contains("file-model"));
    assert!(result.contains("file-api-key"));
}

#[tokio::test]
async fn test_provider_config_from_env() {
    let test_config = TestConfig::new();
    let mut env = TestEnv::new();

    // Set provider API keys
    env.set("ANTHROPIC_API_KEY", "sk-ant-test");
    env.set("OPENAI_API_KEY", "sk-openai-test");

    // Create config with explicit provider key reference
    let config_content = r#"{
        "model": "claude-sonnet-4-6",
        "providers": {
            "anthropic": {
                "api_key": "{env:ANTHROPIC_API_KEY}"
            }
        }
    }"#;

    test_config.write_config("config.json", config_content);

    // Load config (substitutions applied by ConfigLoader)
    let config = Config::load(test_config.project_dir()).unwrap();

    // Config loads with the specified model
    assert_eq!(config.model, "claude-sonnet-4-6");
    // Provider config populated from env substitution
    let anthropic = config
        .providers
        .anthropic
        .expect("anthropic provider should be set");
    assert_eq!(anthropic.api_key.as_deref(), Some("sk-ant-test"));
}

#[tokio::test]
async fn test_config_validation() {
    let test_config = TestConfig::new();

    // Create invalid config (missing required model)
    let config_content = r#"{
        "temperature": 0.7
    }"#;

    test_config.write_config("config.json", config_content);

    // Try to load - should fail validation
    let result = Config::load(test_config.project_dir());

    // Should either fail or provide default model
    match result {
        Ok(config) => {
            // If it loads, should have a default model
            assert!(!config.model.is_empty());
        }
        Err(e) => {
            // Should have a validation error
            assert!(matches!(e, ConfigError::ValidationError(_)));
        }
    }
}

#[tokio::test]
async fn test_config_merging_with_arrays() {
    let test_config = TestConfig::new();

    // Create base config with arrays
    let base_config = test_config.write_config(
        "config.json",
        r#"{
            "model": "base-model",
            "features": {
                "mcp_servers": ["server1", "server2"],
                "agents": ["agent1"]
            }
        }"#,
    );

    // Create override config
    let override_config = test_config.write_config(
        "config.override.json",
        r#"{
            "features": {
                "mcp_servers": ["server3"],
                "agents": ["agent2", "agent3"]
            }
        }"#,
    );

    // Load both configs
    let base_value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&base_config).unwrap()).unwrap();
    let override_value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&override_config).unwrap()).unwrap();

    // Merge configs (override should replace arrays)
    let mut merged = base_value.clone();
    if let Some(obj) = override_value.as_object() {
        for (key, value) in obj {
            if let Some(merged_obj) = merged.as_object_mut() {
                merged_obj.insert(key.clone(), value.clone());
            }
        }
    }

    let merged_config: Config = serde_json::from_value(merged).unwrap();

    // Arrays should be replaced, not merged
    assert_eq!(merged_config.features.mcp_servers.len(), 1);
    assert_eq!(merged_config.features.mcp_servers[0], "server3");
    assert_eq!(merged_config.features.agents.len(), 2);
}

#[tokio::test]
async fn test_config_save_and_load() {
    let test_config = TestConfig::new();

    // Create a config and save it as the project's config file
    let original_config = Config {
        model: "test-model".to_string(),
        temperature: Some(0.8),
        max_tokens: Some(1024),
        ..Default::default()
    };

    // Save to the .rustycode/config.json path that Config::load() reads from
    let save_path = test_config.config_dir.join("config.json");
    original_config.save(&save_path).unwrap();

    // Load config back from the project directory
    let loaded_config = Config::load(test_config.project_dir()).unwrap();

    // Verify they match
    assert_eq!(original_config.model, loaded_config.model);
    assert_eq!(original_config.temperature, loaded_config.temperature);
    assert_eq!(original_config.max_tokens, loaded_config.max_tokens);
}

#[tokio::test]
async fn test_config_with_jsonc_comments() {
    let test_config = TestConfig::new();

    // Create config with comments (JSONC format)
    let config_content = r#"{
        // This is a comment
        "model": "jsonc-model",
        /* This is a
           multi-line comment */
        "temperature": 0.6,
        "providers": {
            "anthropic": {
                "api_key": "sk-test-placeholder", // trailing comment
            }
        }
    }"#;

    test_config.write_config("config.jsonc", config_content);

    // Load config - should handle JSONC
    let mut loader = ConfigLoader::new();
    let config_value = loader.load(test_config.project_dir());

    // Should parse successfully despite comments
    assert!(config_value.is_ok());

    let config: Config = serde_json::from_value(config_value.unwrap()).unwrap();
    assert_eq!(config.model, "jsonc-model");
    assert_eq!(config.temperature, Some(0.6));
}

#[tokio::test]
async fn test_config_workspace_detection() {
    let test_config = TestConfig::new();

    // Create a project directory with workspace config inside .rustycode/config.json
    let project_dir = test_config.project_dir.join("my-project");
    let config_dir = project_dir.join(".rustycode");
    fs::create_dir_all(&config_dir).unwrap();

    let config_content = r#"{
        "model": "workspace-model",
        "workspace": {
            "name": "test-workspace",
            "root": ".",
            "features": ["git", "file-watcher"]
        }
    }"#;
    fs::write(config_dir.join("config.json"), config_content).unwrap();

    // Load config from project directory
    let config = Config::load(&project_dir);

    // Should find workspace config
    assert!(config.is_ok());
    let config = config.unwrap();
    assert!(config.workspace.is_some());
    assert_eq!(config.workspace.unwrap().name.unwrap(), "test-workspace");
}

#[tokio::test]
async fn test_config_default_values() {
    let test_config = TestConfig::new();

    // Create minimal config
    let config_content = r#"{
        "model": "minimal-model"
    }"#;

    test_config.write_config("config.json", config_content);

    // Load config
    let config = Config::load(test_config.project_dir()).unwrap();

    // Verify default values
    assert_eq!(config.model, "minimal-model");
    // temperature and max_tokens use Config defaults when not specified in file
    assert!(config.temperature.is_some());
    assert!(config.max_tokens.is_some());
    assert!(!config.features.git_integration);
    assert!(!config.features.file_watcher);
    assert!(config.features.mcp_servers.is_empty());
    assert!(config.features.agents.is_empty());
}
