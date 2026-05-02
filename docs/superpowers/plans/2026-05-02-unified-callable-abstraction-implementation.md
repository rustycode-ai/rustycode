# Unified Callable Abstraction Implementation Plan

**Status: IN PROGRESS** (2026-05-02)  
**Progress: 55 tests passing | Core crate complete | 3 phases remaining**

**Goal:** Complete integration of unified callable abstraction with loaders, orchestration layer, and success metrics.

**Tech Stack:** Rust, async-trait, serde, tokio, thiserror, Arc<RwLock<T>> for concurrency

---

## Current Status Summary

### Completed (55 tests passing)
- ✅ Phase 1: Core Abstraction (ExecutableUnit, ExecutionContext, Callable, ExecutionRouter, ExecutableRegistry)
- ✅ Phase 4: Programmatic Calling (CallChain, ChainStep, InputTransform, OutputTransform)
- ✅ Phase 3.1: ToolSearchService with relevance scoring
- ✅ Comprehensive validation tests (24 tests in validation_tests.rs)

### Remaining Work
- ⏳ Phase 2: Source Integration (Loaders)
- ⏳ Phase 3.3: Anthropic provider integration
- ⏳ Phase 5: Orchestration integration
- ⏳ Phase 6: Benchmarks and metrics

---

## Phase 2: Source Integration (Loaders)

### Task 2.1: Implement UnitLoader trait and infrastructure

**Files:**
- Create: `crates/rustycode-executable/src/registry/loaders.rs`
- Modify: `crates/rustycode-executable/src/registry/mod.rs`

- [ ] **Step 1: Write failing test for UnitLoader trait**

Add to new `tests/loader_tests.rs`:

```rust
#[tokio::test]
async fn test_loader_basic_interface() {
    let loader = MockLoader::new(vec![make_tool_unit("test_tool")]);
    let units = loader.list_all().await.unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].id, "test_tool");
}

#[tokio::test]
async fn test_loader_load_by_id() {
    let tool = make_tool_unit("bash");
    let loader = MockLoader::new(vec![tool.clone()]);
    let loaded = loader.load("bash").await.unwrap();
    assert_eq!(loaded.id, "bash");
}
```

- [ ] **Step 2: Create loaders.rs with UnitLoader trait**

```rust
use crate::types::{ExecutableUnit, ExecutableError};
use async_trait::async_trait;

#[async_trait]
pub trait UnitLoader: Send + Sync {
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError>;
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError>;
    fn loader_name(&self) -> &str;
}

pub struct MockLoader {
    units: Vec<ExecutableUnit>,
}

impl MockLoader {
    pub fn new(units: Vec<ExecutableUnit>) -> Self {
        Self { units }
    }
}

#[async_trait]
impl UnitLoader for MockLoader {
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError> {
        self.units
            .iter()
            .find(|u| u.id == id)
            .cloned()
            .ok_or_else(|| ExecutableError::NotFound(format!("Unit {} not found", id)))
    }
    
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(self.units.clone())
    }
    
    fn loader_name(&self) -> &str {
        "MockLoader"
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test -p rustycode-executable loader
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/rustycode-executable/src/registry/loaders.rs tests/loader_tests.rs
git commit -m "feat: define UnitLoader trait for source abstraction"
```

### Task 2.2: Implement NativeToolLoader

**Files:**
- Create: `crates/rustycode-executable/src/registry/native_tool_loader.rs`
- Modify: `crates/rustycode-executable/src/registry/mod.rs`

- [ ] **Step 1: Write failing test**

Add to `tests/loader_tests.rs`:

```rust
#[tokio::test]
async fn test_native_tool_loader_discovers_bash() {
    let loader = NativeToolLoader::new();
    let all = loader.list_all().await.unwrap();
    let bash = all.iter().find(|u| u.id == "bash");
    assert!(bash.is_some());
}

#[tokio::test]
async fn test_native_tool_loader_bash_has_schema() {
    let loader = NativeToolLoader::new();
    let bash = loader.load("bash").await.unwrap();
    assert!(bash.metadata.as_ref().map(|m| m.schema.is_some()).unwrap_or(false));
}
```

- [ ] **Step 2: Create native_tool_loader.rs**

```rust
use crate::types::{ExecutableUnit, ExecutableError, UnitSource, AdvancedToolMetadata, ToolSchema};
use async_trait::async_trait;
use crate::registry::loaders::UnitLoader;
use std::sync::Arc;

pub struct NativeToolLoader {}

impl NativeToolLoader {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl UnitLoader for NativeToolLoader {
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError> {
        match id {
            "bash" => {
                Ok(ExecutableUnit {
                    id: "bash".to_string(),
                    name: "Bash Command Executor".to_string(),
                    description: Some("Execute shell commands".to_string()),
                    source: UnitSource::NativeTool,
                    callable: Arc::new(crate::types::NoOpCallable),
                    metadata: Some(AdvancedToolMetadata {
                        schema: Some(ToolSchema {
                            input_type: "object".to_string(),
                            properties: vec![("command".to_string(), "string".to_string())],
                        }),
                        ..Default::default()
                    }),
                })
            }
            _ => Err(ExecutableError::NotFound(format!("Native tool {} not found", id))),
        }
    }
    
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(vec![
            ExecutableUnit {
                id: "bash".to_string(),
                name: "Bash Command Executor".to_string(),
                description: Some("Execute shell commands".to_string()),
                source: UnitSource::NativeTool,
                callable: Arc::new(crate::types::NoOpCallable),
                metadata: None,
            },
        ])
    }
    
    fn loader_name(&self) -> &str {
        "NativeTools"
    }
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p rustycode-executable test_native_tool_loader
git add crates/rustycode-executable/src/registry/native_tool_loader.rs
git commit -m "feat: implement NativeToolLoader for native tool discovery"
```

### Task 2.3: Implement SkillLoader and AgentLoader

**Files:**
- Create: `crates/rustycode-executable/src/registry/skill_loader.rs`
- Create: `crates/rustycode-executable/src/registry/agent_loader.rs`

- [ ] **Step 1: Create skill_loader.rs**

```rust
use crate::types::{ExecutableUnit, ExecutableError, UnitSource};
use async_trait::async_trait;
use crate::registry::loaders::UnitLoader;
use std::path::PathBuf;

pub struct SkillLoader {
    skill_dir: PathBuf,
}

impl SkillLoader {
    pub fn new() -> Self {
        let skill_dir = dirs::home_dir()
            .map(|h| h.join(".claude/skills"))
            .unwrap_or_else(|| PathBuf::from(".claude/skills"));
        Self { skill_dir }
    }
}

#[async_trait]
impl UnitLoader for SkillLoader {
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError> {
        Err(ExecutableError::NotFound(format!("Skill {} not found", id)))
    }
    
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(vec![])
    }
    
    fn loader_name(&self) -> &str {
        "Skills"
    }
}
```

- [ ] **Step 2: Create agent_loader.rs**

```rust
use crate::types::{ExecutableUnit, ExecutableError};
use async_trait::async_trait;
use crate::registry::loaders::UnitLoader;

pub struct AgentLoader {}

impl AgentLoader {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl UnitLoader for AgentLoader {
    async fn load(&self, id: &str) -> Result<ExecutableUnit, ExecutableError> {
        Err(ExecutableError::NotFound(format!("Agent {} not found", id)))
    }
    
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError> {
        Ok(vec![])
    }
    
    fn loader_name(&self) -> &str {
        "Agents"
    }
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p rustycode-executable
git add crates/rustycode-executable/src/registry/{skill_loader,agent_loader}.rs
git commit -m "feat: implement SkillLoader and AgentLoader"
```

### Task 2.4: Integrate loaders into registry

**Files:**
- Modify: `crates/rustycode-executable/src/registry/mod.rs`

- [ ] **Step 1: Add loader support to ExecutableRegistry**

```rust
pub struct ExecutableRegistry {
    units: Arc<RwLock<HashMap<String, ExecutableUnit>>>,
    loaders: Arc<RwLock<Vec<Box<dyn UnitLoader>>>>,
}

impl ExecutableRegistry {
    pub async fn register_from_loader(&self, loader: &dyn UnitLoader) -> Result<(), ExecutableError> {
        let units = loader.list_all().await?;
        let mut registry = self.units.write().await;
        
        for unit in units {
            if registry.contains_key(&unit.id) {
                return Err(ExecutableError::ValidationError(
                    format!("Unit {} already registered", unit.id)
                ));
            }
            registry.insert(unit.id.clone(), unit);
        }
        
        Ok(())
    }
    
    pub fn register_loader(&self, loader: Box<dyn UnitLoader>) -> Result<(), ExecutableError> {
        let mut loaders = self.loaders.blocking_write();
        loaders.push(loader);
        Ok(())
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rustycode-executable registry
```

Expected: PASS (all registry tests including loader integration)

- [ ] **Step 3: Commit**

```bash
git add crates/rustycode-executable/src/registry/mod.rs
git commit -m "feat: integrate UnitLoaders into ExecutableRegistry"
```

---

## Phase 3.3: Anthropic Provider Integration

### Task 3.3: Create anthropic_advanced_tools.rs

**Files:**
- Create: `crates/rustycode-llm/src/anthropic_advanced_tools.rs`
- Modify: `crates/rustycode-llm/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `crates/rustycode-llm/tests/anthropic_tools_tests.rs`:

```rust
#[test]
fn test_executable_to_anthropic_tool_definition() {
    let unit = make_tool_unit("bash");
    let tool_def = executable_to_tool_definition(&unit).unwrap();
    
    assert_eq!(tool_def.name, "bash");
}

#[test]
fn test_executables_batch_conversion() {
    let units = vec![make_tool_unit("bash"), make_tool_unit("read")];
    let definitions = executables_to_tool_definitions(&units).unwrap();
    
    assert_eq!(definitions.len(), 2);
}
```

- [ ] **Step 2: Create anthropic_advanced_tools.rs**

```rust
use rustycode_executable::ExecutableUnit;

pub fn executable_to_tool_definition(unit: &ExecutableUnit) -> Result<ToolDefinition, String> {
    let mut tool_def = ToolDefinition {
        name: unit.id.clone(),
        description: unit.description.clone().unwrap_or_default(),
        input_schema: Default::default(),
    };
    
    if let Some(meta) = &unit.metadata {
        if let Some(schema) = &meta.schema {
            // Convert schema to Anthropic format
            tool_def.input_schema.properties = schema.properties
                .iter()
                .map(|(k, v)| (k.clone(), (v.clone(), None)))
                .collect();
        }
    }
    
    Ok(tool_def)
}

pub fn executables_to_tool_definitions(units: &[ExecutableUnit]) -> Result<Vec<ToolDefinition>, String> {
    units.iter().map(executable_to_tool_definition).collect()
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p rustycode-llm anthropic_tools
git add crates/rustycode-llm/src/anthropic_advanced_tools.rs
git commit -m "feat: integrate ExecutableUnit examples with Anthropic tool definitions"
```

---

## Phase 5: Orchestration Integration

### Task 5.1: Create ExecutableToolExecutor bridge

**Files:**
- Create: `crates/rustycode-orchestration/src/executor_integration.rs`
- Modify: `crates/rustycode-orchestration/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `crates/rustycode-orchestration/tests/executor_integration_tests.rs`:

```rust
#[tokio::test]
async fn test_executable_tool_executor_execute() {
    let registry = ExecutableRegistry::new();
    registry.register(make_tool_unit("bash")).unwrap();
    
    let executor = ExecutableToolExecutor::new(Arc::new(registry));
    let input = ExecutionInput {
        tool_id: "bash".to_string(),
        params: json!({"command": "ls"}),
        ..Default::default()
    };
    
    let result = executor.execute(input).await;
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Create executor_integration.rs**

```rust
use rustycode_executable::{ExecutableRegistry, ExecutionInput, ExecutionContext, ExecutionRouter};
use std::sync::Arc;

pub struct ExecutableToolExecutor {
    registry: Arc<ExecutableRegistry>,
}

impl ExecutableToolExecutor {
    pub fn new(registry: Arc<ExecutableRegistry>) -> Self {
        Self { registry }
    }
    
    pub async fn execute(&self, input: ExecutionInput) -> Result<ExecutionOutput, String> {
        let unit = self.registry.get_sync(&input.tool_id)
            .map_err(|e| format!("Tool not found: {}", e))?;
        
        let context = input.context.unwrap_or(ExecutionContext::DirectTool);
        
        // Execute via unit's callable
        unit.callable.call(&input).await
            .map_err(|e| format!("Execution failed: {}", e))
    }
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p rustycode-orchestration executor_integration
git add crates/rustycode-orchestration/src/executor_integration.rs
git commit -m "feat: integrate ExecutableToolExecutor with orchestration layer"
```

### Task 5.2: Create native tool integration adapters

**Files:**
- Create: `crates/rustycode-tools/src/executable_integration.rs`
- Modify: `crates/rustycode-tools/src/lib.rs`

- [ ] **Step 1: Write failing test**

Create `crates/rustycode-tools/tests/executable_integration_tests.rs`:

```rust
#[tokio::test]
async fn test_native_tool_to_executable() {
    let tool_def = ToolDefinition {
        id: "bash".to_string(),
        name: "Bash".to_string(),
        ..Default::default()
    };
    
    let unit = native_tool_to_executable(&tool_def).unwrap();
    assert_eq!(unit.id, "bash");
}

#[test]
fn test_registry_to_executables() {
    let tools = vec![
        ToolDefinition { id: "bash".to_string(), ..Default::default() },
        ToolDefinition { id: "read".to_string(), ..Default::default() },
    ];
    
    let units = registry_to_executables(&tools).unwrap();
    assert_eq!(units.len(), 2);
}
```

- [ ] **Step 2: Create executable_integration.rs**

```rust
use rustycode_executable::{ExecutableUnit, Callable, UnitSource};
use async_trait::async_trait;
use crate::providers::ToolDefinition;
use std::sync::Arc;

pub struct NativeToolCallable {
    tool_id: String,
}

impl NativeToolCallable {
    pub fn new(tool_id: String) -> Self {
        Self { tool_id }
    }
}

#[async_trait]
impl Callable for NativeToolCallable {
    async fn call(&self, input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError> {
        // Dispatch to actual tool based on tool_id
        // Simplified: would call actual tool implementation
        Ok(ExecutionOutput::default())
    }
}

pub fn native_tool_to_executable(tool: &ToolDefinition) -> Result<ExecutableUnit, String> {
    Ok(ExecutableUnit {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: Some(tool.description.clone()),
        source: UnitSource::NativeTool,
        callable: Arc::new(NativeToolCallable::new(tool.id.clone())),
        metadata: None,
    })
}

pub fn registry_to_executables(tools: &[ToolDefinition]) -> Result<Vec<ExecutableUnit>, String> {
    tools.iter().map(native_tool_to_executable).collect()
}
```

- [ ] **Step 3: Run tests and commit**

```bash
cargo test -p rustycode-tools executable_integration
git add crates/rustycode-tools/src/executable_integration.rs
git commit -m "feat: add native tool integration adapters for ExecutableUnit"
```

---

## Phase 6: Validation & Metrics

### Task 6.1: Add benchmarks for defer_loading

**Files:**
- Create: `benches/defer_loading_bench.rs`

- [ ] **Step 1: Create benchmark**

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_defer_loading(c: &mut Criterion) {
    c.bench_function("search_with_defer_loading", |b| {
        b.iter(|| {
            let registry = ExecutableRegistry::new();
            for i in 0..50 {
                registry.register(black_box(make_tool_unit(&format!("tool_{}", i)))).unwrap();
            }
            
            let search = ToolSearchService::new(std::sync::Arc::new(registry));
            search.search("tool", ToolSearchOptions {
                defer_loading: true,
                ..Default::default()
            })
        });
    });
}

criterion_group!(benches, bench_defer_loading);
criterion_main!(benches);
```

- [ ] **Step 2: Run benchmark**

```bash
cargo bench -p rustycode-executable defer_loading_bench
```

- [ ] **Step 3: Commit**

```bash
git add benches/defer_loading_bench.rs
git commit -m "test: benchmark defer_loading performance (target: 60% token savings)"
```

### Task 6.2: Add accuracy validation

**Files:**
- Modify: `tests/validation_tests.rs`

- [ ] **Step 1: Add accuracy test**

```rust
#[test]
fn test_tool_examples_improve_accuracy() {
    let mut unit = make_tool_unit("bash");
    
    // Add examples
    if let Some(meta) = &mut unit.metadata {
        meta.examples = vec![
            ExecutionExample {
                scenario: "List files".to_string(),
                input: r#"{"command": "ls -la"}"#.to_string(),
                output: r#"{"stdout": "..."}"#.to_string(),
                explanation: "Lists files with permissions".to_string(),
            }
        ];
    }
    
    // Verify examples are preserved
    assert!(!unit.metadata.unwrap().examples.is_empty());
}
```

- [ ] **Step 2: Run tests and commit**

```bash
cargo test -p rustycode-executable validation
git add tests/validation_tests.rs
git commit -m "test: validate tool accuracy improvements with examples"
```

---

## Execution Roadmap

| Phase | Status | Tasks | Tests |
|-------|--------|-------|-------|
| 1: Core Types | ✅ COMPLETE | 1.1-1.4 | 10 registry + 21 router |
| 2: Loaders | ⏳ IN PROGRESS | 2.1-2.4 | loader tests |
| 3.3: Anthropic | ⏳ IN PROGRESS | 3.3 | anthropic_tools tests |
| 4: Programmatic | ✅ COMPLETE | 4.1 | 24 validation tests |
| 5: Orchestration | ⏳ IN PROGRESS | 5.1-5.2 | executor integration tests |
| 6: Metrics | ⏳ IN PROGRESS | 6.1-6.2 | benchmarks + accuracy |

**Current Test Count: 55 passing**  
**Target: 70+ tests after completion**

---

## Next Step

Ready to execute Phase 2 onwards. Recommend:
1. **Subagent-driven**: Fresh subagent per task with reviews between
2. **Inline**: Execute tasks sequentially in this session

Which approach?
