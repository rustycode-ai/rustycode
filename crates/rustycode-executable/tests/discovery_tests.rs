//! Integration tests for `ToolSearchService` and `NativeToolLoader` examples
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{make_agent_unit, make_skill_unit, make_tool_unit};
use rustycode_executable::discovery::ToolSearchOptions;
use rustycode_executable::registry::loaders::UnitLoader;
use rustycode_executable::registry::native_tool_loader::NativeToolLoader;
use rustycode_executable::{ExecutableRegistry, ToolSearchService};
use std::sync::Arc;

fn setup_search() -> (Arc<ExecutableRegistry>, ToolSearchService) {
    let registry = Arc::new(ExecutableRegistry::new());

    // Register a variety of units with different hints
    registry.register(make_tool_unit("bash")).unwrap();
    registry.register(make_tool_unit("grep")).unwrap();
    registry.register(make_tool_unit("glob")).unwrap();
    registry.register(make_skill_unit("code_review")).unwrap();
    registry.register(make_agent_unit("architect")).unwrap();

    let search = ToolSearchService::new(registry.clone());
    (registry, search)
}

#[tokio::test]
async fn search_finds_by_name_exact_match() {
    let (_registry, search) = setup_search();
    let results = search
        .search("grep", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert_eq!(results[0].id, "grep");
    assert!(results[0].relevance_score > 0.0);
}

#[tokio::test]
async fn search_finds_by_description_substring() {
    let (_registry, search) = setup_search();
    // All units have "Test tool/skill/agent: <name>" as description
    let results = search
        .search("skill", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.id == "code_review"));
}

#[tokio::test]
async fn search_finds_by_search_hints() {
    let (_registry, search) = setup_search();
    // "architect" agent has "agent" as a search hint
    let results = search
        .search("agent", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(!results.is_empty());
    assert!(results.iter().any(|r| r.id == "architect"));
}

#[tokio::test]
async fn search_returns_empty_for_no_match() {
    let (_registry, search) = setup_search();
    let results = search
        .search("nonexistent_xyz", ToolSearchOptions::default())
        .await
        .unwrap();

    assert!(results.is_empty());
}

#[tokio::test]
async fn search_respects_limit() {
    let (_registry, search) = setup_search();
    let options = ToolSearchOptions {
        limit: 2,
        ..ToolSearchOptions::default()
    };

    let results = search.search("", options).await.unwrap();
    assert!(results.len() <= 2);
}

#[tokio::test]
async fn search_exact_name_scores_highest() {
    let (_registry, search) = setup_search();
    let results = search
        .search("bash", ToolSearchOptions::default())
        .await
        .unwrap();

    // Exact name match should score 2.0, the highest
    let bash_result = results.iter().find(|r| r.id == "bash").unwrap();
    assert!(bash_result.relevance_score >= 2.0);
}

#[tokio::test]
async fn search_includes_full_definitions_when_requested() {
    let (_registry, search) = setup_search();
    let options = ToolSearchOptions {
        include_full_definitions: true,
        limit: 10,
    };

    let results = search.search("grep", options).await.unwrap();
    // grep has no schema, so full_definition should be None
    let grep_result = results.iter().find(|r| r.id == "grep").unwrap();
    assert!(grep_result.full_definition.is_none());
}

#[tokio::test]
async fn search_results_sorted_by_relevance() {
    let (_registry, search) = setup_search();
    let results = search
        .search("bash", ToolSearchOptions::default())
        .await
        .unwrap();

    // Verify descending order
    for i in 1..results.len() {
        assert!(results[i - 1].relevance_score >= results[i].relevance_score);
    }
}

// --- NativeToolLoader example tests ---

#[tokio::test]
async fn native_loader_bash_has_examples() {
    let loader = NativeToolLoader::new(vec!["bash".to_string()]);
    let units = loader.load_units().await.unwrap();

    assert_eq!(units.len(), 1);
    let bash = &units[0];
    assert_eq!(bash.id, "bash");
    let examples = &bash.advanced_metadata.examples;
    assert_eq!(examples.len(), 2);

    // First example: list files
    assert!(!examples[0].scenario.is_empty());
    assert!(!examples[0].input.is_null());
    assert!(!examples[0].output.is_null());
    assert!(examples[0].explanation.is_some());

    // Second example: find files
    assert!(!examples[1].scenario.is_empty());
    assert!(!examples[1].input.is_null());
    assert!(!examples[1].output.is_null());
    assert!(examples[1].explanation.is_some());
}

#[tokio::test]
async fn native_loader_read_has_example() {
    let loader = NativeToolLoader::new(vec!["read".to_string()]);
    let units = loader.load_units().await.unwrap();

    assert_eq!(units.len(), 1);
    let examples = &units[0].advanced_metadata.examples;
    assert_eq!(examples.len(), 1);
    assert!(!examples[0].scenario.is_empty());
    assert!(examples[0].explanation.is_some());
}

#[tokio::test]
async fn native_loader_edit_has_example() {
    let loader = NativeToolLoader::new(vec!["edit".to_string()]);
    let units = loader.load_units().await.unwrap();

    let examples = &units[0].advanced_metadata.examples;
    assert_eq!(examples.len(), 1);
    assert!(
        examples[0].input.get("old_string").is_some(),
        "edit example should include old_string field"
    );
    assert!(
        examples[0].input.get("new_string").is_some(),
        "edit example should include new_string field"
    );
}

#[tokio::test]
async fn native_loader_unknown_tool_has_no_examples() {
    let loader = NativeToolLoader::new(vec!["custom_tool_xyz".to_string()]);
    let units = loader.load_units().await.unwrap();

    assert_eq!(units.len(), 1);
    let examples = &units[0].advanced_metadata.examples;
    assert!(examples.is_empty(), "unknown tools should have no examples");
}

#[tokio::test]
async fn native_loader_multiple_tools_mixed_examples() {
    let loader = NativeToolLoader::new(vec![
        "bash".to_string(),
        "unknown_tool".to_string(),
        "glob".to_string(),
    ]);
    let units = loader.load_units().await.unwrap();

    assert_eq!(units.len(), 3);

    // bash has 2 examples
    assert_eq!(units[0].advanced_metadata.examples.len(), 2);
    // unknown has 0
    assert!(units[1].advanced_metadata.examples.is_empty());
    // glob has 1
    assert_eq!(units[2].advanced_metadata.examples.len(), 1);
}

#[tokio::test]
async fn native_loader_example_content_is_valid_json() {
    let loader = NativeToolLoader::new(vec![
        "bash".to_string(),
        "read".to_string(),
        "write".to_string(),
        "grep".to_string(),
    ]);
    let units = loader.load_units().await.unwrap();

    for unit in &units {
        for example in &unit.advanced_metadata.examples {
            // scenario must be a non-empty string
            assert!(
                !example.scenario.trim().is_empty(),
                "example scenario must not be empty for unit {}",
                unit.id
            );
            // input must be an object
            assert!(
                example.input.is_object(),
                "example input must be a JSON object for unit {}",
                unit.id
            );
            // output must be present
            assert!(
                !example.output.is_null(),
                "example output must not be null for unit {}",
                unit.id
            );
            // explanation, when present, must be non-empty
            if let Some(explanation) = &example.explanation {
                assert!(
                    !explanation.trim().is_empty(),
                    "example explanation must not be empty if present for unit {}",
                    unit.id
                );
            }
        }
    }
}

#[tokio::test]
async fn native_loader_aliases_share_examples() {
    // "read" and "read_file" should both get examples
    let loader = NativeToolLoader::new(vec!["read".to_string(), "read_file".to_string()]);
    let units = loader.load_units().await.unwrap();

    assert_eq!(units[0].advanced_metadata.examples.len(), 1);
    assert_eq!(units[1].advanced_metadata.examples.len(), 1);

    // Both should describe the same scenario
    assert_eq!(
        units[0].advanced_metadata.examples[0].scenario,
        units[1].advanced_metadata.examples[0].scenario,
        "aliased tool names should produce the same example scenarios"
    );
}
