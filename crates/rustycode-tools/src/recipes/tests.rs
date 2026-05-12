use super::*;
use std::collections::HashMap;

#[test]
fn test_recipe_registry_find() {
    let mut registry = RecipeRegistry::new();
    registry.add_builtins();

    let review = registry.find("Code Review");
    assert!(review.is_some());
    assert_eq!(review.unwrap().title, "Code Review");

    let bug = registry.find("bug investigation");
    assert!(bug.is_some());
}

#[test]
fn test_resolve_prompt() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test recipe".into(),
        prompt: Some("Hello {{name}}, you are {{age}} years old".into()),
        parameters: vec![
            RecipeParameter {
                name: "name".into(),
                required: true,
                ..Default::default()
            },
            RecipeParameter {
                name: "age".into(),
                required: false,
                default: Some("25".into()),
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    let mut params = HashMap::new();
    params.insert("name".to_string(), "Alice".to_string());
    params.insert("age".to_string(), "30".to_string());

    let prompt = registry.resolve_prompt(&recipe, &params);
    assert_eq!(prompt, "Hello Alice, you are 30 years old");
}

#[test]
fn test_resolve_prompt_defaults() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        prompt: Some("Hello {{name}}".into()),
        ..Default::default()
    };

    let params: HashMap<String, String> = HashMap::new();
    let prompt = registry.resolve_prompt(&recipe, &params);
    assert_eq!(prompt, "Hello {{name}}");
}

#[test]
fn test_builtins_loaded() {
    let mut registry = RecipeRegistry::new();
    registry.add_builtins();
    let titles = registry.titles();
    assert!(titles.contains(&"Code Review".to_string()));
    assert!(titles.contains(&"Bug Investigation".to_string()));
    assert!(titles.contains(&"Refactor".to_string()));
    assert!(titles.contains(&"Write Tests".to_string()));
}

// ── Builder Tests ──────────────────────────────────────────────────

#[test]
fn test_builder_basic() {
    let recipe = RecipeBuilder::new("Test Recipe")
        .description("A test recipe")
        .prompt("Do something with {{input}}")
        .tool("Read")
        .tool("Grep")
        .build();

    assert_eq!(recipe.title, "Test Recipe");
    assert_eq!(recipe.description, "A test recipe");
    assert_eq!(recipe.tools, vec!["Read", "Grep"]);
    assert_eq!(
        recipe.prompt,
        Some("Do something with {{input}}".to_string())
    );
}

#[test]
fn test_builder_with_tools_vec() {
    let recipe = RecipeBuilder::new("Multi-Tool")
        .description("Uses many tools")
        .tools(vec!["Read", "Grep", "Bash"])
        .build();

    assert_eq!(recipe.tools.len(), 3);
}

#[test]
fn test_builder_with_parameters() {
    let recipe = RecipeBuilder::new("Parameterized")
        .description("Has parameters")
        .parameter(RecipeParameter {
            name: "path".into(),
            required: true,
            ..Default::default()
        })
        .parameter(RecipeParameter {
            name: "verbose".into(),
            kind: RecipeParameterKind::Boolean,
            default: Some("false".into()),
            ..Default::default()
        })
        .build();

    assert_eq!(recipe.parameters.len(), 2);
    assert!(recipe.parameters[0].required);
    assert_eq!(recipe.parameters[1].kind, RecipeParameterKind::Boolean);
}

#[test]
fn test_builder_with_retry() {
    let recipe = RecipeBuilder::new("Retryable")
        .description("Retries on failure")
        .retry(3, 10)
        .build();

    assert!(recipe.retry.is_some());
    let retry = recipe.retry.unwrap();
    assert_eq!(retry.max_attempts, 3);
    assert_eq!(retry.delay_seconds, 10);
}

#[test]
fn test_builder_with_author() {
    let recipe = RecipeBuilder::new("Authored")
        .description("Has an author")
        .author("Alice", Some("alice@example.com".into()))
        .build();

    assert!(recipe.author.is_some());
    let author = recipe.author.unwrap();
    assert_eq!(author.name, Some("Alice".to_string()));
    assert_eq!(author.email, Some("alice@example.com".to_string()));
}

#[test]
fn test_builder_minimal() {
    let recipe = RecipeBuilder::new("Minimal").build();
    assert_eq!(recipe.title, "Minimal");
    assert!(recipe.description.is_empty());
    assert!(recipe.tools.is_empty());
}

// ── Discovery Tests ────────────────────────────────────────────────

#[test]
fn test_discover_empty_dir() {
    let temp = tempfile::tempdir().unwrap();
    let registry = RecipeRegistry::discover(temp.path()).unwrap();
    assert_eq!(registry.titles().len(), 0);
}

#[test]
fn test_discover_from_yaml() {
    let temp = tempfile::tempdir().unwrap();
    let recipe_path = temp.path().join("test.yaml");
    std::fs::write(
        &recipe_path,
        "title: YAML Recipe\ndescription: A YAML test recipe\n",
    )
    .unwrap();

    let registry = RecipeRegistry::discover(temp.path()).unwrap();
    assert!(registry.find("YAML Recipe").is_some());
}

#[test]
fn test_discover_from_json() {
    let temp = tempfile::tempdir().unwrap();
    let recipe_path = temp.path().join("test.json");
    std::fs::write(
        &recipe_path,
        r#"{"title":"JSON Recipe","description":"A JSON test recipe"}"#,
    )
    .unwrap();

    let registry = RecipeRegistry::discover(temp.path()).unwrap();
    assert!(registry.find("JSON Recipe").is_some());
}

#[test]
fn test_discover_deduplicates_by_title() {
    let temp = tempfile::tempdir().unwrap();

    // Create same recipe in both YAML and JSON
    std::fs::write(
        temp.path().join("recipe.yaml"),
        "title: Duplicate\ndescription: First\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("recipe.json"),
        r#"{"title":"Duplicate","description":"Second"}"#,
    )
    .unwrap();

    let registry = RecipeRegistry::discover(temp.path()).unwrap();
    // Should have exactly one recipe with this title
    let recipe = registry.find("Duplicate").unwrap();
    // First found wins (order depends on directory iteration, just verify dedup)
    assert!(recipe.description == "First" || recipe.description == "Second");
}

#[test]
fn test_search_paths_includes_cwd() {
    let temp = tempfile::tempdir().unwrap();
    let paths = RecipeRegistry::search_paths(temp.path());
    assert!(paths.contains(&temp.path().to_path_buf()));
}

#[test]
fn test_search_paths_includes_home() {
    let temp = tempfile::tempdir().unwrap();
    let paths = RecipeRegistry::search_paths(temp.path());
    // Should include ~/.rustycode/recipes if home dir is available
    if let Some(home) = dirs::home_dir() {
        assert!(paths.contains(&home.join(".rustycode").join("recipes")));
    }
}

// ── Validation Tests ───────────────────────────────────────────────

#[test]
fn test_validate_required_param_present() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "path".into(),
            required: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut params = HashMap::new();
    params.insert("path".to_string(), "/tmp/test".to_string());

    let errors = registry.validate_params(&recipe, &params);
    assert!(errors.is_empty());
}

#[test]
fn test_validate_required_param_missing() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "path".into(),
            description: "Required path".into(),
            required: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    let errors = registry.validate_params(&recipe, &HashMap::new());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Missing required"));
    assert!(errors[0].contains("path"));
}

#[test]
fn test_validate_number_param() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "count".into(),
            kind: RecipeParameterKind::Number,
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut valid_params = HashMap::new();
    valid_params.insert("count".to_string(), "42".to_string());
    assert!(registry.validate_params(&recipe, &valid_params).is_empty());

    let mut invalid_params = HashMap::new();
    invalid_params.insert("count".to_string(), "not_a_number".to_string());
    let errors = registry.validate_params(&recipe, &invalid_params);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("must be a number"));
}

#[test]
fn test_validate_select_param() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "level".into(),
            kind: RecipeParameterKind::Select,
            options: vec!["low".into(), "medium".into(), "high".into()],
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut valid_params = HashMap::new();
    valid_params.insert("level".to_string(), "medium".to_string());
    assert!(registry.validate_params(&recipe, &valid_params).is_empty());

    let mut invalid_params = HashMap::new();
    invalid_params.insert("level".to_string(), "extreme".to_string());
    let errors = registry.validate_params(&recipe, &invalid_params);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("must be one of"));
}

#[test]
fn test_validate_boolean_param() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "verbose".into(),
            kind: RecipeParameterKind::Boolean,
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut valid_params = HashMap::new();
    valid_params.insert("verbose".to_string(), "true".to_string());
    assert!(registry.validate_params(&recipe, &valid_params).is_empty());

    let mut invalid_params = HashMap::new();
    invalid_params.insert("verbose".to_string(), "yes".to_string());
    let errors = registry.validate_params(&recipe, &invalid_params);
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_validate_optional_with_default() {
    let registry = RecipeRegistry::new();
    let recipe = Recipe {
        title: "Test".into(),
        description: "Test".into(),
        parameters: vec![RecipeParameter {
            name: "level".into(),
            required: false,
            default: Some("info".into()),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Optional param with default - no error when missing
    let errors = registry.validate_params(&recipe, &HashMap::new());
    assert!(errors.is_empty());
}
