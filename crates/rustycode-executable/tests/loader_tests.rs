//! Integration tests for `UnitLoader` implementations
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::missing_const_for_fn,
    clippy::unnecessary_literal_bound
)]

use std::sync::Arc;

use async_trait::async_trait;
use rustycode_executable::registry::loaders::UnitLoader;
use rustycode_executable::{
    AdvancedToolMetadata, Callable, ExecutableError, ExecutableRegistry, ExecutableUnit,
    ExecutionContext, ExecutionInput, ExecutionMetadata, ExecutionMode, UnitCapabilities,
    UnitSource,
};

// Helpers (self-contained, no dependency on tests/common which has issues)

/// Simple callable that echoes input data back
struct EchoCallable;

#[async_trait]
impl Callable for EchoCallable {
    async fn execute(
        &self,
        input: ExecutionInput,
        _context: ExecutionContext,
    ) -> Result<rustycode_executable::ExecutionOutput, ExecutableError> {
        Ok(rustycode_executable::ExecutionOutput {
            data: input.data,
            metadata: ExecutionMetadata {
                duration_ms: 1,
                tokens_used: None,
                was_cached: false,
                trace: None,
            },
        })
    }

    fn get_runtime_capabilities(&self) -> UnitCapabilities {
        UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        }
    }
}

/// Create a basic tool `ExecutableUnit` for testing
fn make_tool_unit(id: &str) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: format!("Test tool: {id}"),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading: false,
            search_hints: vec![id.to_string()],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(EchoCallable),
        source: UnitSource::NativeTool {
            path: format!("tools/{id}"),
        },
        schema: None,
        tags: vec![],
        version: None,
    }
}

// MockLoader for testing the trait interface without filesystem I/O

struct MockLoader {
    units: Vec<ExecutableUnit>,
}

impl MockLoader {
    fn new(units: Vec<ExecutableUnit>) -> Self {
        Self { units }
    }
}

#[async_trait]
impl UnitLoader for MockLoader {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn load_units(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(self.units.clone())
    }
}

// Trait interface tests

#[tokio::test]
async fn test_loader_basic_interface() {
    let loader = MockLoader::new(vec![make_tool_unit("test_tool")]);
    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "test_tool");
}

#[tokio::test]
async fn test_loader_load_by_id() {
    let unit = make_tool_unit("bash");
    let mock = MockLoader::new(vec![unit]);
    let found = mock.load("bash").await.unwrap();
    assert_eq!(found.id, "bash");
}

#[tokio::test]
async fn test_loader_missing_unit() {
    let loader = MockLoader::new(vec![]);
    let result = loader.load("nonexistent").await;
    assert!(matches!(result, Err(ExecutableError::NotFound(_))));
}

// Registry + loader integration tests

#[tokio::test]
async fn test_registry_register_from_loader() {
    let registry = ExecutableRegistry::new();
    let loader = MockLoader::new(vec![make_tool_unit("test_tool")]);

    registry.register_from_loader(&loader).await.unwrap();

    let unit = registry.get_sync("test_tool").unwrap();
    assert_eq!(unit.id, "test_tool");
}

#[tokio::test]
async fn test_registry_register_from_loader_duplicate_handling() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("existing")).unwrap();

    let loader = MockLoader::new(vec![make_tool_unit("existing")]);
    let result = registry.register_from_loader(&loader).await;

    // Should fail on duplicate
    assert!(result.is_err());
}

#[tokio::test]
async fn test_registry_register_from_loader_multiple_units() {
    let registry = ExecutableRegistry::new();
    let loader = MockLoader::new(vec![
        make_tool_unit("bash"),
        make_tool_unit("read"),
        make_tool_unit("write"),
    ]);

    registry.register_from_loader(&loader).await.unwrap();

    assert!(registry.get_sync("bash").is_some());
    assert!(registry.get_sync("read").is_some());
    assert!(registry.get_sync("write").is_some());
    assert!(registry.get_sync("nonexistent").is_none());
}

// NativeToolLoader tests

#[tokio::test]
async fn test_native_tool_loader_creates_units() {
    let loader = rustycode_executable::registry::native_tool_loader::NativeToolLoader::new(vec![
        "bash".to_string(),
        "read".to_string(),
    ]);

    assert_eq!(loader.name(), "native_tools");

    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 2);
    assert!(units.iter().any(|u| u.id == "bash"));
    assert!(units.iter().any(|u| u.id == "read"));
}

#[tokio::test]
async fn test_native_tool_loader_individual_load() {
    let loader = rustycode_executable::registry::native_tool_loader::NativeToolLoader::new(vec![
        "bash".to_string(),
    ]);

    let unit = loader.load("bash").await.unwrap();
    assert_eq!(unit.id, "bash");
}

#[tokio::test]
async fn test_native_tool_loader_missing() {
    let loader = rustycode_executable::registry::native_tool_loader::NativeToolLoader::new(vec![
        "bash".to_string(),
    ]);

    let result = loader.load("nonexistent").await;
    assert!(matches!(result, Err(ExecutableError::NotFound(_))));
}

#[tokio::test]
async fn test_native_tool_loader_stale_default() {
    let loader = rustycode_executable::registry::native_tool_loader::NativeToolLoader::new(vec![]);
    // Default is_stale returns false
    assert!(!loader.is_stale().await);
}

// SkillLoader tests

#[tokio::test]
async fn test_skill_loader_name() {
    let loader = rustycode_executable::registry::skill_loader::SkillLoader::new(
        std::path::PathBuf::from("/nonexistent"),
    );
    assert_eq!(loader.name(), "skills");
}

#[tokio::test]
async fn test_skill_loader_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let loader =
        rustycode_executable::registry::skill_loader::SkillLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert!(units.is_empty());
}

#[tokio::test]
async fn test_skill_loader_discovers_skills() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("test_skill.md"),
        "---\nname: test_skill\n---\ncontent",
    )
    .unwrap();

    let loader =
        rustycode_executable::registry::skill_loader::SkillLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "skill:test_skill");
}

#[tokio::test]
async fn test_skill_loader_load_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("my_skill.md"), "skill content").unwrap();

    let loader =
        rustycode_executable::registry::skill_loader::SkillLoader::new(tmp.path().to_path_buf());
    let unit = loader.load("skill:my_skill").await.unwrap();
    assert_eq!(unit.id, "skill:my_skill");
}

#[tokio::test]
async fn test_skill_loader_ignores_non_md_files() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("data.json"), "{}").unwrap();
    std::fs::write(tmp.path().join("readme.txt"), "text").unwrap();
    std::fs::write(tmp.path().join("valid.md"), "skill").unwrap();

    let loader =
        rustycode_executable::registry::skill_loader::SkillLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "skill:valid");
}

// AgentLoader tests

#[tokio::test]
async fn test_agent_loader_name() {
    let loader = rustycode_executable::registry::agent_loader::AgentLoader::new(
        std::path::PathBuf::from("/nonexistent"),
    );
    assert_eq!(loader.name(), "agents");
}

#[tokio::test]
async fn test_agent_loader_empty_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let loader =
        rustycode_executable::registry::agent_loader::AgentLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert!(units.is_empty());
}

#[tokio::test]
async fn test_agent_loader_discovers_agents() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("test_agent.md"), "# Test Agent").unwrap();

    let loader =
        rustycode_executable::registry::agent_loader::AgentLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "agent:test_agent");
}

#[tokio::test]
async fn test_agent_loader_discovers_yaml_agents() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("yaml_agent.yaml"), "name: yaml_agent").unwrap();

    let loader =
        rustycode_executable::registry::agent_loader::AgentLoader::new(tmp.path().to_path_buf());
    let units = loader.load_units().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "agent:yaml_agent");
}

#[tokio::test]
async fn test_agent_loader_load_by_id() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("coder.md"), "# Coder agent").unwrap();

    let loader =
        rustycode_executable::registry::agent_loader::AgentLoader::new(tmp.path().to_path_buf());
    let unit = loader.load("agent:coder").await.unwrap();
    assert_eq!(unit.id, "agent:coder");
}
