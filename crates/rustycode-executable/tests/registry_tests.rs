//! Integration tests for `ExecutableRegistry`
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{make_agent_unit, make_skill_unit, make_tool_unit, make_tool_unit_with_schema};
use rustycode_executable::ExecutableRegistry;

#[test]
fn register_and_retrieve_unit() {
    let registry = ExecutableRegistry::new();
    let unit = make_tool_unit("Bash");
    let id = unit.id.clone();

    registry.register(unit).expect("register should succeed");

    let retrieved = registry.get_sync(&id).expect("unit should exist");
    assert_eq!(retrieved.id, "Bash");
    assert_eq!(retrieved.name, "Bash");
}

#[tokio::test]
async fn register_and_retrieve_async() {
    let registry = ExecutableRegistry::new();
    let unit = make_tool_unit("read");
    let id = unit.id.clone();

    registry.register(unit).expect("register should succeed");

    let retrieved = registry.get(&id).await.expect("unit should exist");
    assert_eq!(retrieved.id, "read");
}

#[test]
fn duplicate_registration_fails() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("edit")).unwrap();
    let result = registry.register(make_tool_unit("edit"));
    assert!(result.is_err());
}

#[test]
fn get_nonexistent_returns_none() {
    let registry = ExecutableRegistry::new();
    assert!(registry.get_sync("nonexistent").is_none());
}

#[tokio::test]
async fn list_metadata_returns_all_units() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("a")).unwrap();
    registry.register(make_tool_unit("b")).unwrap();
    registry.register(make_skill_unit("c")).unwrap();

    let metadata = registry.list_metadata().await;
    assert_eq!(metadata.len(), 3);
}

#[tokio::test]
async fn discover_by_name() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("Grep")).unwrap();
    registry.register(make_tool_unit("Glob")).unwrap();
    registry.register(make_tool_unit("Bash")).unwrap();

    let results = registry.discover("Grep", None).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "Grep");
}

#[tokio::test]
async fn discover_by_description() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("x")).unwrap();
    registry.register(make_skill_unit("y")).unwrap();
    registry.register(make_agent_unit("z")).unwrap();

    let results = registry.discover("skill", None).await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "y");
}

#[tokio::test]
async fn discover_by_search_hints() {
    let registry = ExecutableRegistry::new();
    registry.register(make_agent_unit("agent1")).unwrap();

    let results = registry.discover("agent", None).await;
    assert!(!results.is_empty());
}

#[tokio::test]
async fn metadata_reflects_defer_loading() {
    let registry = ExecutableRegistry::new();

    let tool = make_tool_unit("instant");
    let skill = make_skill_unit("lazy"); // defer_loading: true

    registry.register(tool).unwrap();
    registry.register(skill).unwrap();

    let md = registry.list_metadata().await;
    let instant_md = md.iter().find(|m| m.id == "instant").unwrap();
    let lazy_md = md.iter().find(|m| m.id == "lazy").unwrap();

    assert!(instant_md.full_loaded);
    assert!(!lazy_md.full_loaded);
}

#[test]
fn unit_with_schema_preserved() {
    let registry = ExecutableRegistry::new();
    let unit = make_tool_unit_with_schema("schematized");
    registry.register(unit).unwrap();

    let retrieved = registry.get_sync("schematized").unwrap();
    assert!(retrieved.schema.is_some());
    let schema = retrieved.schema.unwrap();
    assert!(schema.parameters["properties"]["path"].is_object());
}
