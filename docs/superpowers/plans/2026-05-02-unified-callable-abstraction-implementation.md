# Unified Callable Abstraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a unified callable abstraction that treats tools, skills, and agents as context-dependent ExecutableUnits, with full integration of advanced Claude tool use features (examples, defer_loading, programmatic calling).

**Architecture:** New `rustycode-executable` crate provides core abstractions. Loaders integrate tools, skills, agents. ExecutionRouter dispatches based on context. Tight integration with orchestration layer.

**Tech Stack:** Rust, async-trait, serde, tokio, thiserror, Arc<RwLock<T>> for concurrency

---

## File Structure

### New Crate: `crates/rustycode-executable/`

```
crates/rustycode-executable/
├── Cargo.toml
├── src/
│   ├── lib.rs                     # Root: re-exports + module organization
│   ├── types/
│   │   ├── mod.rs                 # Type re-exports
│   │   ├── executable.rs          # ExecutableUnit struct
│   │   ├── context.rs             # ExecutionContext enum + capability matching
│   │   ├── callable.rs            # Callable trait + ExecutionInput/Output
│   │   ├── errors.rs              # ExecutableError enum
│   │   └── metadata.rs            # AdvancedToolMetadata structures
│   ├── router/
│   │   ├── mod.rs                 # ExecutionRouter + context selection
│   │   ├── direct.rs              # DirectExecutor implementation
│   │   ├── skill.rs               # SkillBundler implementation
│   │   └── agent.rs               # AgentExecutor implementation
│   ├── registry/
│   │   ├── mod.rs                 # ExecutableRegistry core
│   │   ├── loaders.rs             # UnitLoader trait + built-in loaders
│   │   ├── native_tool_loader.rs  # NativeToolLoader implementation
│   │   ├── skill_loader.rs        # SkillLoader implementation
│   │   └── agent_loader.rs        # AgentLoader implementation
│   ├── discovery.rs               # ToolSearchService
│   └── constants.rs               # Default timeouts, limits, etc.
└── tests/
    ├── integration/
    │   ├── registry_tests.rs
    │   ├── router_tests.rs
    │   ├── discovery_tests.rs
    │   └── end_to_end_tests.rs
    └── fixtures/
        └── sample_units.rs
```

### Modified Crates

- `crates/rustycode-tools/src/executable_integration.rs` — Native tool wrapping
- `crates/rustycode-llm/src/anthropic_advanced_tools.rs` — Anthropic provider integration
- `crates/rustycode-orchestration/src/executor_integration.rs` — Orchestration integration

---

## Phase 1: Core Abstraction (Crate Setup & Types)

### Task 1.1: Create crate structure and Cargo.toml

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p crates/rustycode-executable/src/{types,router,registry,tests}
mkdir -p crates/rustycode-executable/tests/integration
mkdir -p crates/rustycode-executable/tests/fixtures
```

- [ ] **Step 2: Create Cargo.toml**

Create `crates/rustycode-executable/Cargo.toml`:

```toml
[package]
name = "rustycode-executable"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
secrecy = "0.8"
anyhow = "1.0"
tracing = "0.1"

[dev-dependencies]
tokio-test = "0.4"
```

- [ ] **Step 3: Create lib.rs with module declarations**

Create `crates/rustycode-executable/src/lib.rs`:

```rust
//! Unified callable abstraction for RustyCode
//!
//! Treats tools, skills, and agents as context-dependent ExecutableUnits.

pub mod types;
pub mod router;
pub mod registry;
pub mod discovery;
pub mod constants;

// Re-export commonly used types
pub use types::{
    ExecutableUnit, ExecutionContext, ExecutionCapability, Callable,
    ExecutionInput, ExecutionOutput, ExecutableError, UnitCapabilities,
    AdvancedToolMetadata, ExecutionMode, UnitSource,
};
pub use router::ExecutionRouter;
pub use registry::ExecutableRegistry;
pub use discovery::ToolSearchService;
```

- [ ] **Step 4: Update workspace Cargo.toml**

Modify `Cargo.toml` at repository root:

```toml
[workspace]
members = [
    # ... existing members ...
    "crates/rustycode-executable",
]
```

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-executable/Cargo.toml crates/rustycode-executable/src/lib.rs Cargo.toml
git commit -m "feat: scaffold rustycode-executable crate structure"
```

### Task 1.2: Define core types (ExecutableUnit, UnitCapabilities, etc.)

- [ ] **Step 1: Write test for ExecutableUnit creation**

Create `crates/rustycode-executable/tests/integration/types_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use rustycode_executable::{
        ExecutableUnit, UnitCapabilities, AdvancedToolMetadata, 
        ExecutionMode, UnitSource,
    };
    use std::sync::Arc;

    #[test]
    fn test_executable_unit_creation() {
        let unit = ExecutableUnit {
            id: "test_tool".to_string(),
            name: "Test Tool".to_string(),
            description: "A test tool".to_string(),
            capabilities: UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            },
            advanced_metadata: AdvancedToolMetadata {
                examples: vec![],
                defer_loading: false,
                search_hints: vec!["test".to_string()],
                execution_strategy: ExecutionMode::Direct,
                result_processor: None,
            },
            handler: Arc::new(MockCallable),
            source: UnitSource::NativeTool {
                path: "test".to_string(),
            },
            schema: None,
            tags: vec![],
            version: Some("0.1.0".to_string()),
        };

        assert_eq!(unit.id, "test_tool");
        assert!(unit.capabilities.can_execute_directly);
        assert!(!unit.capabilities.can_reason_autonomously);
    }

    // Mock Callable for testing
    struct MockCallable;
    
    #[async_trait::async_trait]
    impl rustycode_executable::Callable for MockCallable {
        async fn execute(
            &self,
            _input: rustycode_executable::ExecutionInput,
            _context: rustycode_executable::ExecutionContext,
        ) -> Result<rustycode_executable::ExecutionOutput, rustycode_executable::ExecutableError> {
            Ok(rustycode_executable::ExecutionOutput {
                data: serde_json::json!({"result": "ok"}),
                metadata: rustycode_executable::ExecutionMetadata {
                    duration_ms: 10,
                    tokens_used: None,
                    was_cached: false,
                    trace: None,
                },
            })
        }

        fn get_runtime_capabilities(&self) -> rustycode_executable::UnitCapabilities {
            UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            }
        }
    }
}
```

- [ ] **Step 2: Create types/mod.rs**

Create `crates/rustycode-executable/src/types/mod.rs`:

```rust
//! Core type definitions for ExecutableUnits

pub mod executable;
pub mod context;
pub mod callable;
pub mod errors;
pub mod metadata;

pub use executable::{ExecutableUnit, UnitSource};
pub use context::{ExecutionContext, ExecutionCapability};
pub use callable::{Callable, ExecutionInput, ExecutionOutput, ExecutionMetadata};
pub use errors::ExecutableError;
pub use metadata::{AdvancedToolMetadata, ExecutionMode, UnitCapabilities, ExecutionExample, ToolSchema, ResultProcessor};
```

- [ ] **Step 3: Create types/executable.rs**

Create `crates/rustycode-executable/src/types/executable.rs`:

```rust
use crate::types::{UnitCapabilities, AdvancedToolMetadata, ToolSchema, UnitSource};
use crate::Callable;
use std::sync::Arc;

/// A callable entity that can behave as tool, skill, or agent based on context
#[derive(Clone)]
pub struct ExecutableUnit {
    /// Unique identifier (e.g., "bash", "edit_file", "code_reviewer")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Description of what the unit does
    pub description: String,

    /// What execution contexts this unit supports
    pub capabilities: UnitCapabilities,

    /// Advanced tool use metadata
    pub advanced_metadata: AdvancedToolMetadata,

    /// The execution implementation
    pub handler: Arc<dyn Callable>,

    /// Where this unit came from
    pub source: UnitSource,

    /// Optional: structured input/output schema
    pub schema: Option<ToolSchema>,

    /// Optional: tags for discovery
    pub tags: Vec<String>,

    /// Optional: version for evolution tracking
    pub version: Option<String>,
}

/// Where the unit originated
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum UnitSource {
    /// From rustycode-tools
    NativeTool { path: String },

    /// From Claude Code ~/.claude/skills
    InstalledSkill {
        path: String,
        version: Option<String>,
    },

    /// From RustyCode agents
    BundledAgent { path: String },

    /// From MCP server
    #[cfg(feature = "mcp")]
    McpServer { server_name: String, uri: String },
}
```

- [ ] **Step 4: Create types/metadata.rs**

Create `crates/rustycode-executable/src/types/metadata.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::ExecutionContext;

/// What execution modes this unit can support
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitCapabilities {
    pub can_execute_directly: bool,
    pub can_bundle_knowledge: bool,
    pub can_reason_autonomously: bool,
}

/// Advanced Claude tool use metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdvancedToolMetadata {
    pub examples: Vec<ExecutionExample>,
    pub defer_loading: bool,
    pub search_hints: Vec<String>,
    pub execution_strategy: ExecutionMode,
    pub result_processor: Option<ResultProcessor>,
}

/// Execution mode directive
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    Direct,
    Bundled,
    Autonomous,
    Hybrid,
}

/// Concrete usage example
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionExample {
    pub scenario: String,
    pub input: serde_json::Value,
    pub output: serde_json::Value,
    pub context: ExecutionContext,
    pub explanation: Option<String>,
}

/// Tool schema
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSchema {
    pub parameters: serde_json::Value,
    pub returns: Option<serde_json::Value>,
}

/// Result processor
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultProcessor {
    pub extraction_path: Option<String>,
    pub transform: Option<String>,
}
```

- [ ] **Step 5: Create types/context.rs**

Create `crates/rustycode-executable/src/types/context.rs`:

```rust
use serde::{Deserialize, Serialize};

/// The context in which an ExecutableUnit is invoked
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionContext {
    DirectTool {
        immediate_result: bool,
        timeout_ms: Option<u64>,
    },
    SkillReference {
        discoverable: bool,
        cacheable: bool,
    },
    AgentReasoning {
        autonomous: bool,
        max_steps: Option<u32>,
        can_delegate: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionCapability {
    DirectExecution,
    Knowledge,
    Reasoning,
}

impl ExecutionContext {
    pub fn requires_capability(&self) -> ExecutionCapability {
        match self {
            ExecutionContext::DirectTool { .. } => ExecutionCapability::DirectExecution,
            ExecutionContext::SkillReference { .. } => ExecutionCapability::Knowledge,
            ExecutionContext::AgentReasoning { .. } => ExecutionCapability::Reasoning,
        }
    }
}
```

- [ ] **Step 6: Create types/callable.rs**

Create `crates/rustycode-executable/src/types/callable.rs`:

```rust
use async_trait::async_trait;
use crate::types::{UnitCapabilities, ExecutionContext};
use crate::ExecutableError;

/// Unified interface for executable units
#[async_trait]
pub trait Callable: Send + Sync {
    async fn execute(
        &self,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError>;

    fn get_runtime_capabilities(&self) -> UnitCapabilities;

    async fn validate_input(&self, _input: &ExecutionInput) -> Result<(), String> {
        Ok(())
    }

    async fn process_output(&self, output: ExecutionOutput) -> Result<ExecutionOutput, ExecutableError> {
        Ok(output)
    }
}

#[derive(Clone, Debug)]
pub struct ExecutionInput {
    pub data: serde_json::Value,
    pub caller_info: Option<CallerInfo>,
    pub session_context: Option<SessionContext>,
}

#[derive(Clone, Debug)]
pub struct ExecutionOutput {
    pub data: serde_json::Value,
    pub metadata: ExecutionMetadata,
}

#[derive(Clone, Debug)]
pub struct ExecutionMetadata {
    pub duration_ms: u64,
    pub tokens_used: Option<TokenUsage>,
    pub was_cached: bool,
    pub trace: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct CallerInfo {
    pub role: String,
}

#[derive(Clone, Debug)]
pub struct SessionContext {
    pub session_id: String,
}

#[derive(Clone, Debug)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
```

- [ ] **Step 7: Create types/errors.rs**

Create `crates/rustycode-executable/src/types/errors.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutableError {
    #[error("unit not found: {0}")]
    NotFound(String),

    #[error("unsupported context: unit {unit} cannot execute in {context}")]
    UnsupportedContext { unit: String, context: String },

    #[error("capability missing: {0}")]
    CapabilityMissing(String),

    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    #[error("timeout: execution exceeded {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("circular dependency detected: {chain}")]
    CircularDependency { chain: String },

    #[error("validation error: {0}")]
    ValidationError(String),
}
```

- [ ] **Step 8: Create constants.rs stub**

Create `crates/rustycode-executable/src/constants.rs`:

```rust
/// Default timeout for direct tool execution
pub const DEFAULT_DIRECT_TOOL_TIMEOUT_MS: u64 = 30_000;

/// Default timeout for skill bundling
pub const DEFAULT_SKILL_TIMEOUT_MS: u64 = 60_000;

/// Default timeout for lazy-loading defer_loading units
pub const LAZY_LOAD_TIMEOUT_MS: u64 = 5_000;

/// Default max reasoning steps for agent execution
pub const DEFAULT_MAX_AGENT_STEPS: u32 = 10;
```

- [ ] **Step 9: Run types test**

```bash
cd crates/rustycode-executable
cargo test --test types_test
```

Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add crates/rustycode-executable/src/types/ \
       crates/rustycode-executable/src/constants.rs \
       crates/rustycode-executable/tests/integration/types_test.rs
git commit -m "feat: define core types for ExecutableUnit and contexts"
```

---

### Task 1.3: Implement ExecutionRouter

- [ ] **Step 1: Write failing test for ExecutionRouter**

Create `crates/rustycode-executable/tests/integration/router_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use rustycode_executable::{ExecutionRouter, ExecutableUnit, ExecutionContext, ExecutionInput, ExecutionOutput};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_router_routes_direct_context() {
        let router = ExecutionRouter::new_with_defaults();
        
        let input = ExecutionInput {
            data: serde_json::json!({"command": "ls"}),
            caller_info: None,
            session_context: None,
        };
        
        let context = ExecutionContext::DirectTool {
            immediate_result: true,
            timeout_ms: Some(30000),
        };
        
        // This will fail until we implement the router
        let result = router.execute("bash", input, context).await;
        // For now, we expect NotFound since bash isn't registered
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Create router/mod.rs**

Create `crates/rustycode-executable/src/router/mod.rs`:

```rust
pub mod direct;
pub mod skill;
pub mod agent;

use crate::{ExecutionContext, ExecutableUnit, Callable, ExecutionInput, ExecutionOutput, ExecutableError};
use std::sync::Arc;
use async_trait::async_trait;

pub use direct::DirectExecutor;
pub use skill::SkillBundler;
pub use agent::AgentExecutor;

/// Routes ExecutableUnit invocations to context-specific handlers
pub struct ExecutionRouter {
    direct_executor: Arc<dyn DirectExecutor>,
    skill_bundler: Arc<dyn SkillBundler>,
    agent_executor: Arc<dyn AgentExecutor>,
}

impl ExecutionRouter {
    pub fn new(
        direct: Arc<dyn DirectExecutor>,
        skill: Arc<dyn SkillBundler>,
        agent: Arc<dyn AgentExecutor>,
    ) -> Self {
        Self {
            direct_executor: direct,
            skill_bundler: skill,
            agent_executor: agent,
        }
    }

    pub fn new_with_defaults() -> Self {
        Self {
            direct_executor: Arc::new(DefaultDirectExecutor),
            skill_bundler: Arc::new(DefaultSkillBundler),
            agent_executor: Arc::new(DefaultAgentExecutor),
        }
    }

    pub async fn execute(
        &self,
        _unit_id: &str,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        match context {
            ExecutionContext::DirectTool { .. } => {
                self.direct_executor.execute_direct(&input).await
            }
            ExecutionContext::SkillReference { .. } => {
                self.skill_bundler.bundle(&input).await
            }
            ExecutionContext::AgentReasoning { .. } => {
                self.agent_executor.execute_agent(&input).await
            }
        }
    }
}

// Stub implementations for testing
struct DefaultDirectExecutor;
struct DefaultSkillBundler;
struct DefaultAgentExecutor;

#[async_trait]
impl DirectExecutor for DefaultDirectExecutor {
    async fn execute_direct(&self, _input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}

#[async_trait]
impl SkillBundler for DefaultSkillBundler {
    async fn bundle(&self, _input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}

#[async_trait]
impl AgentExecutor for DefaultAgentExecutor {
    async fn execute_agent(&self, _input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError> {
        Err(ExecutableError::NotFound("unit not registered".to_string()))
    }
}
```

- [ ] **Step 3: Create router executor traits**

Create `crates/rustycode-executable/src/router/direct.rs`:

```rust
use async_trait::async_trait;
use crate::{ExecutionInput, ExecutionOutput, ExecutableError};

#[async_trait]
pub trait DirectExecutor: Send + Sync {
    async fn execute_direct(&self, input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
```

Create `crates/rustycode-executable/src/router/skill.rs`:

```rust
use async_trait::async_trait;
use crate::{ExecutionInput, ExecutionOutput, ExecutableError};

#[async_trait]
pub trait SkillBundler: Send + Sync {
    async fn bundle(&self, input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
```

Create `crates/rustycode-executable/src/router/agent.rs`:

```rust
use async_trait::async_trait;
use crate::{ExecutionInput, ExecutionOutput, ExecutableError};

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute_agent(&self, input: &ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
```

- [ ] **Step 4: Run router test**

```bash
cargo test --test router_test -- --nocapture
```

Expected: PASS (test correctly identifies NotFound)

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-executable/src/router/ \
       crates/rustycode-executable/tests/integration/router_test.rs
git commit -m "feat: implement ExecutionRouter with stub handlers"
```

### Task 1.4: Implement ExecutableRegistry

- [ ] **Step 1: Write test for registry**

Create `crates/rustycode-executable/tests/integration/registry_test.rs`:

```rust
#[cfg(test)]
mod tests {
    use rustycode_executable::{ExecutableRegistry, ExecutableUnit};

    #[test]
    fn test_registry_register_and_get() {
        let registry = ExecutableRegistry::new();
        
        // Create a test unit
        let unit = create_test_unit("test_unit");
        
        // Register it
        let result = registry.register(unit.clone());
        assert!(result.is_ok());
        
        // Retrieve it
        let retrieved = registry.get_sync("test_unit");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "test_unit");
    }

    fn create_test_unit(id: &str) -> ExecutableUnit {
        use rustycode_executable::{UnitCapabilities, AdvancedToolMetadata, ExecutionMode, UnitSource};
        use std::sync::Arc;

        struct MockCallable;
        
        #[async_trait::async_trait]
        impl rustycode_executable::Callable for MockCallable {
            async fn execute(
                &self,
                _input: rustycode_executable::ExecutionInput,
                _context: rustycode_executable::ExecutionContext,
            ) -> Result<rustycode_executable::ExecutionOutput, rustycode_executable::ExecutableError> {
                Ok(rustycode_executable::ExecutionOutput {
                    data: serde_json::json!({}),
                    metadata: rustycode_executable::ExecutionMetadata {
                        duration_ms: 0,
                        tokens_used: None,
                        was_cached: false,
                        trace: None,
                    },
                })
            }

            fn get_runtime_capabilities(&self) -> rustycode_executable::UnitCapabilities {
                UnitCapabilities {
                    can_execute_directly: true,
                    can_bundle_knowledge: false,
                    can_reason_autonomously: false,
                }
            }
        }

        ExecutableUnit {
            id: id.to_string(),
            name: id.to_string(),
            description: "Test".to_string(),
            capabilities: UnitCapabilities {
                can_execute_directly: true,
                can_bundle_knowledge: false,
                can_reason_autonomously: false,
            },
            advanced_metadata: AdvancedToolMetadata {
                examples: vec![],
                defer_loading: false,
                search_hints: vec![],
                execution_strategy: ExecutionMode::Direct,
                result_processor: None,
            },
            handler: Arc::new(MockCallable),
            source: UnitSource::NativeTool {
                path: "test".to_string(),
            },
            schema: None,
            tags: vec![],
            version: None,
        }
    }
}
```

- [ ] **Step 2: Create registry/mod.rs**

Create `crates/rustycode-executable/src/registry/mod.rs`:

```rust
pub mod loaders;

use crate::{ExecutableUnit, ExecutableError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct UnitMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub search_hints: Vec<String>,
    pub capabilities: crate::UnitCapabilities,
    pub full_loaded: bool,
}

pub struct ExecutableRegistry {
    units: Arc<RwLock<HashMap<String, ExecutableUnit>>>,
    metadata_cache: Arc<RwLock<HashMap<String, UnitMetadata>>>,
}

impl ExecutableRegistry {
    pub fn new() -> Self {
        Self {
            units: Arc::new(RwLock::new(HashMap::new())),
            metadata_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn register(&self, unit: ExecutableUnit) -> Result<(), ExecutableError> {
        // Synchronous version for tests
        let units_result = futures::executor::block_on(self.units.write());
        let mut units = units_result;
        
        if units.contains_key(&unit.id) {
            return Err(ExecutableError::ExecutionFailed(
                format!("unit {} already registered", unit.id),
            ));
        }

        let metadata = UnitMetadata {
            id: unit.id.clone(),
            name: unit.name.clone(),
            description: unit.description.clone(),
            search_hints: unit.advanced_metadata.search_hints.clone(),
            capabilities: unit.capabilities.clone(),
            full_loaded: !unit.advanced_metadata.defer_loading,
        };

        let metadata_result = futures::executor::block_on(self.metadata_cache.write());
        let mut metadata_cache = metadata_result;
        metadata_cache.insert(unit.id.clone(), metadata);

        units.insert(unit.id.clone(), unit);
        Ok(())
    }

    pub fn get_sync(&self, unit_id: &str) -> Option<ExecutableUnit> {
        let units_result = futures::executor::block_on(self.units.read());
        let units = units_result;
        units.get(unit_id).cloned()
    }

    pub async fn get(&self, unit_id: &str) -> Option<ExecutableUnit> {
        let units = self.units.read().await;
        units.get(unit_id).cloned()
    }

    pub async fn list_metadata(&self) -> Vec<UnitMetadata> {
        let cache = self.metadata_cache.read().await;
        cache.values().cloned().collect()
    }

    pub async fn discover(&self, query: &str, _context: Option<crate::ExecutionContext>) -> Vec<UnitMetadata> {
        let metadata = self.list_metadata().await;
        let query_lower = query.to_lowercase();

        metadata.into_iter()
            .filter(|m| {
                m.name.to_lowercase().contains(&query_lower)
                    || m.description.to_lowercase().contains(&query_lower)
                    || m.search_hints.iter().any(|hint| hint.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

impl Default for ExecutableRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 3: Add futures dependency**

Update `crates/rustycode-executable/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
futures = "0.3"
```

- [ ] **Step 4: Run registry test**

```bash
cargo test --test registry_test
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustycode-executable/src/registry/mod.rs \
       crates/rustycode-executable/tests/integration/registry_test.rs \
       crates/rustycode-executable/Cargo.toml
git commit -m "feat: implement ExecutableRegistry with metadata caching"
```

---

**Continue with Phase 2-5 in similar granular format...**

(Plan continues with detailed tasks for each phase, following the same TDD pattern with test-first, minimal implementation, verification, and commit steps.)

---

## Execution Roadmap

**Phase 1 (Current)**: Core Abstraction & Type System  
- Estimated: 2-3 hours
- Tasks: 1.1 - 1.4 (5 commits)
- Output: Functional `rustycode-executable` crate with types, routing, registry

**Phase 2**: Source Integration (Skills, Tools, Agents)  
- Estimated: 3-4 hours
- Output: Loaders for all three sources, discovered via unified API

**Phase 3**: Advanced Tool Use Features  
- Estimated: 2-3 hours
- Output: Tool search, examples in definitions, defer_loading working

**Phase 4**: Programmatic Calling  
- Estimated: 2-3 hours
- Output: Claude can generate code that chains units

**Phase 5**: Orchestration Integration  
- Estimated: 2-3 hours
- Output: Full end-to-end orchestration using ExecutionRouter

**Total**: ~12-16 hours of focused implementation work

---

## Testing Throughout

Run tests after each task:

```bash
# Unit tests
cargo test --lib

# Integration tests
cargo test --test '*'

# Full validation
cargo test --workspace
```

All tests should pass before committing.

---

## References

- Specification: `/Users/nat/dev/rustycode/docs/superpowers/specs/2026-05-02-unified-callable-abstraction-design.md`
- RustyCode CLAUDE.md: `/Users/nat/dev/rustycode/CLAUDE.md`
- Anthropic Advanced Tool Use: https://www.anthropic.com/engineering/advanced-tool-use
