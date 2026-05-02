# Unified Callable Abstraction for RustyCode

**Date**: 2026-05-02  
**Status**: Design Specification (Ready for Implementation)  
**Scope**: RustyCode-internal unification of tools, skills, and agents as context-dependent executables  
**Breaking Change**: Yes (tool/skill/agent system redesign)  
**Backward Compatibility**: Claude Code skill/tool ecosystem remains unchanged; external discovery unaffected  

---

## Executive Summary

This specification describes a unified callable abstraction for RustyCode that treats tools, skills, and agents as variants of a single concept: **ExecutableUnits**. The same underlying unit behaves as a tool (direct execution), skill (bundled knowledge), or agent (autonomous reasoning) based on **execution context**.

**Key Benefits:**
- Single mental model for orchestration (no special-casing tools vs. skills vs. agents)
- Foundation for advanced Claude tool use features (examples, defer_loading, programmatic calling)
- 60% token savings on large tool libraries (via defer_loading)
- 90% tool invocation accuracy (via examples)
- Enables Claude-generated code to orchestrate any unit type

**New Crate**: `rustycode-executable` (core abstraction)  
**Modified Crates**: `rustycode-tools`, `rustycode-skill`, `rustycode-agent`, `rustycode-orchestration`

---

## 1. Core Data Structures

### 1.1 ExecutableUnit

The unified representation of any callable entity in RustyCode.

```rust
/// A callable entity that can behave as tool, skill, or agent based on execution context.
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
    
    /// Advanced tool use metadata (examples, search hints, lazy loading)
    pub advanced_metadata: AdvancedToolMetadata,
    
    /// The execution implementation
    pub handler: Arc<dyn Callable>,
    
    /// Where this unit came from (native tool, Claude Code skill, RustyCode agent)
    pub source: UnitSource,
    
    /// Optional: structured input/output schema for programmatic calling
    pub schema: Option<ToolSchema>,
    
    /// Optional: tags for discovery and filtering
    pub tags: Vec<String>,
    
    /// Optional: version for evolution tracking
    pub version: Option<String>,
}

/// What execution modes this unit can support
#[derive(Clone, Debug)]
pub struct UnitCapabilities {
    /// Can be invoked as a direct tool (immediate execution)
    pub can_execute_directly: bool,
    
    /// Can be bundled as skill knowledge (discoverable, referenceable)
    pub can_bundle_knowledge: bool,
    
    /// Can be run as autonomous agent (reasoning loop, self-direction)
    pub can_reason_autonomously: bool,
}

/// Advanced Claude tool use features
#[derive(Clone, Debug)]
pub struct AdvancedToolMetadata {
    /// Usage examples for improved accuracy (72% → 90%)
    pub examples: Vec<ExecutionExample>,
    
    /// Whether to lazy-load full definition (token savings: 72K → 8.7K)
    pub defer_loading: bool,
    
    /// Keywords for tool discovery (used in Tool Search)
    pub search_hints: Vec<String>,
    
    /// How Claude should invoke this unit
    pub execution_strategy: ExecutionMode,
    
    /// Optional: how to process results from this unit
    pub result_processor: Option<ResultProcessor>,
}

/// Concrete usage example for the unit
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionExample {
    /// What was being done (e.g., "create a new file")
    pub scenario: String,
    
    /// Input parameters to the unit
    pub input: serde_json::Value,
    
    /// Expected output/result
    pub output: serde_json::Value,
    
    /// Context it was used in (Direct, Bundled, Autonomous, etc.)
    pub context: ExecutionContext,
    
    /// Optional: explanation of why this is the right invocation
    pub explanation: Option<String>,
}

/// How Claude should invoke this unit
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Direct invocation: Claude calls immediately, gets result
    Direct,
    
    /// Bundled: Unit's knowledge is included in system prompt
    Bundled,
    
    /// Autonomous: Unit runs its own reasoning loop
    Autonomous,
    
    /// Hybrid: Claude decides based on task complexity/requirements
    Hybrid,
}

/// Where the unit originated
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UnitSource {
    /// From rustycode-tools (native, in-process)
    NativeTool { path: String },
    
    /// From Claude Code ~/.claude/skills (external, loaded)
    InstalledSkill { 
        path: String,
        version: Option<String>,
    },
    
    /// From RustyCode's agent crate (in-process with reasoning)
    BundledAgent { path: String },
    
    /// From third-party MCP server
    #[cfg(feature = "mcp")]
    McpServer { server_name: String, uri: String },
}

/// Input/output schema for programmatic calling
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSchema {
    /// JSON schema for parameters
    pub parameters: JsonSchema,
    
    /// JSON schema for return value
    pub returns: Option<JsonSchema>,
}

/// How results from this unit should be processed
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResultProcessor {
    /// Extract field from result (e.g., "data.summary")
    pub extraction_path: Option<String>,
    
    /// Transform result (e.g., "summarize", "format_as_table")
    pub transform: Option<String>,
}
```

### 1.2 ExecutionContext

Determines how the unit behaves when invoked.

```rust
/// The context in which an ExecutableUnit is invoked
#[derive(Clone, Debug)]
pub enum ExecutionContext {
    /// Direct tool invocation: immediate execution, return result
    DirectTool {
        /// Whether result must be immediate (vs. streaming)
        immediate_result: bool,
        /// Optional: timeout for execution
        timeout_ms: Option<u64>,
    },
    
    /// Skill reference: include in knowledge context, make discoverable
    SkillReference {
        /// Whether the skill should be discoverable via search
        discoverable: bool,
        /// Whether results can be cached
        cacheable: bool,
    },
    
    /// Agent reasoning: run with autonomous reasoning loop
    AgentReasoning {
        /// Whether the agent can make decisions autonomously
        autonomous: bool,
        /// Max iterations of reasoning loop (None = unlimited)
        max_steps: Option<u32>,
        /// Whether agent can invoke other units
        can_delegate: bool,
    },
}

impl ExecutionContext {
    /// Does this context require the unit to have a specific capability?
    pub fn requires_capability(&self) -> ExecutionCapability {
        match self {
            ExecutionContext::DirectTool { .. } => ExecutionCapability::DirectExecution,
            ExecutionContext::SkillReference { .. } => ExecutionCapability::Knowledge,
            ExecutionContext::AgentReasoning { .. } => ExecutionCapability::Reasoning,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionCapability {
    DirectExecution,
    Knowledge,
    Reasoning,
}
```

### 1.3 Callable Trait

The interface all units must implement.

```rust
/// Unified interface for executable units
#[async_trait]
pub trait Callable: Send + Sync {
    /// Execute the unit with given input and context
    async fn execute(
        &self,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError>;
    
    /// Get additional metadata/capabilities at runtime
    fn get_runtime_capabilities(&self) -> UnitCapabilities;
    
    /// Optional: validate that input is correct before execution
    async fn validate_input(&self, input: &ExecutionInput) -> Result<(), ValidationError> {
        Ok(())
    }
    
    /// Optional: transform output before returning to caller
    async fn process_output(&self, output: ExecutionOutput) -> Result<ExecutionOutput, ExecutableError> {
        Ok(output)
    }
}

/// Input to a callable unit
#[derive(Clone, Debug)]
pub struct ExecutionInput {
    /// Raw input (typically JSON or string)
    pub data: serde_json::Value,
    
    /// Context about the caller
    pub caller_info: Option<CallerInfo>,
    
    /// Environment/session context
    pub session_context: Option<SessionContext>,
}

/// Output from a callable unit
#[derive(Clone, Debug)]
pub struct ExecutionOutput {
    /// Result data
    pub data: serde_json::Value,
    
    /// Metadata about execution
    pub metadata: ExecutionMetadata,
}

/// Metadata about execution
#[derive(Clone, Debug)]
pub struct ExecutionMetadata {
    /// Time taken to execute (ms)
    pub duration_ms: u64,
    
    /// Tokens used (if applicable)
    pub tokens_used: Option<TokenUsage>,
    
    /// Whether execution was cached
    pub was_cached: bool,
    
    /// Optional: execution trace/logs
    pub trace: Option<Vec<String>>,
}

/// Error from unit execution
#[derive(Debug, thiserror::Error)]
pub enum ExecutableError {
    #[error("unit not found: {0}")]
    NotFound(String),
    
    #[error("unsupported context: unit {unit} cannot execute in {context:?}")]
    UnsupportedContext { unit: String, context: String },
    
    #[error("capability missing: {0}")]
    CapabilityMissing(String),
    
    #[error("execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("timeout: execution exceeded {duration_ms}ms")]
    Timeout { duration_ms: u64 },
    
    #[error("circular dependency detected: {chain}")]
    CircularDependency { chain: Vec<String> },
    
    #[error("validation error: {0}")]
    ValidationError(String),
}
```

---

## 2. Execution Router

Routes unit invocations to appropriate handlers based on context.

```rust
/// Routes ExecutableUnit invocations to context-specific handlers
pub struct ExecutionRouter {
    registry: Arc<ExecutableRegistry>,
    direct_executor: Arc<dyn DirectExecutor>,
    skill_bundler: Arc<dyn SkillBundler>,
    agent_executor: Arc<dyn AgentExecutor>,
}

impl ExecutionRouter {
    /// Route a unit invocation to the appropriate handler
    pub async fn execute(
        &self,
        unit_id: &str,
        input: ExecutionInput,
        context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        // 1. Load unit
        let unit = self.registry.get(unit_id)
            .ok_or_else(|| ExecutableError::NotFound(unit_id.to_string()))?;
        
        // 2. Verify capability match
        if !context.unit_supports_context(&unit.capabilities) {
            return Err(ExecutableError::UnsupportedContext {
                unit: unit_id.to_string(),
                context: format!("{:?}", context),
            });
        }
        
        // 3. Route to handler
        match context {
            ExecutionContext::DirectTool { .. } => {
                self.direct_executor.execute(&unit, input).await
            }
            ExecutionContext::SkillReference { .. } => {
                self.skill_bundler.bundle(&unit, input).await
            }
            ExecutionContext::AgentReasoning { .. } => {
                self.agent_executor.execute(&unit, input).await
            }
        }
    }
    
    /// Execute with automatic context selection (Hybrid mode)
    pub async fn execute_hybrid(
        &self,
        unit_id: &str,
        input: ExecutionInput,
    ) -> Result<ExecutionOutput, ExecutableError> {
        let unit = self.registry.get(unit_id)
            .ok_or_else(|| ExecutableError::NotFound(unit_id.to_string()))?;
        
        let context = self.select_context(&unit, &input);
        self.execute(unit_id, input, context).await
    }
    
    /// Select best execution context for a unit and input
    fn select_context(&self, unit: &ExecutableUnit, input: &ExecutionInput) -> ExecutionContext {
        // Logic: choose based on unit capabilities, input complexity, execution mode
        if unit.advanced_metadata.execution_strategy == ExecutionMode::Autonomous
            && unit.capabilities.can_reason_autonomously
        {
            ExecutionContext::AgentReasoning {
                autonomous: true,
                max_steps: Some(10),
                can_delegate: true,
            }
        } else if unit.capabilities.can_execute_directly {
            ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: Some(30000),
            }
        } else {
            ExecutionContext::SkillReference {
                discoverable: true,
                cacheable: true,
            }
        }
    }
}

/// Trait for context-specific handlers
#[async_trait]
pub trait DirectExecutor: Send + Sync {
    async fn execute(&self, unit: &ExecutableUnit, input: ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}

#[async_trait]
pub trait SkillBundler: Send + Sync {
    async fn bundle(&self, unit: &ExecutableUnit, input: ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(&self, unit: &ExecutableUnit, input: ExecutionInput) -> Result<ExecutionOutput, ExecutableError>;
}
```

---

## 3. Registry & Discovery

### 3.1 ExecutableRegistry

Stores and retrieves units; handles lazy loading.

```rust
/// Central registry for all ExecutableUnits
pub struct ExecutableRegistry {
    units: Arc<RwLock<HashMap<String, ExecutableUnit>>>,
    
    /// Metadata cache for defer_loading support
    metadata_cache: Arc<RwLock<HashMap<String, UnitMetadata>>>,
    
    /// Registered loaders for different sources
    loaders: Arc<Vec<Box<dyn UnitLoader>>>,
}

/// Lightweight metadata for defer_loading units
#[derive(Clone, Debug)]
pub struct UnitMetadata {
    pub id: String,
    pub name: String,
    pub description: String,
    pub search_hints: Vec<String>,
    pub capabilities: UnitCapabilities,
    pub full_loaded: bool,
}

impl ExecutableRegistry {
    /// Register a unit in the registry
    pub fn register(&self, unit: ExecutableUnit) -> Result<(), ExecutableError> {
        let mut units = self.units.write().map_err(|_| ExecutableError::ExecutionFailed("lock failed".into()))?;
        
        if units.contains_key(&unit.id) {
            return Err(ExecutableError::ExecutionFailed(
                format!("unit {} already registered", unit.id)
            ));
        }
        
        // Cache metadata for defer_loading
        let metadata = UnitMetadata {
            id: unit.id.clone(),
            name: unit.name.clone(),
            description: unit.description.clone(),
            search_hints: unit.advanced_metadata.search_hints.clone(),
            capabilities: unit.capabilities.clone(),
            full_loaded: !unit.advanced_metadata.defer_loading,
        };
        
        self.metadata_cache.write()
            .ok()
            .map(|mut cache| cache.insert(unit.id.clone(), metadata));
        
        units.insert(unit.id.clone(), unit);
        Ok(())
    }
    
    /// Get a unit, lazy-loading full definition if needed
    pub async fn get(&self, unit_id: &str) -> Option<ExecutableUnit> {
        let units = self.units.read().ok()?;
        
        if let Some(unit) = units.get(unit_id) {
            return Some(unit.clone());
        }
        
        // If not loaded, try lazy-loading
        drop(units);
        
        for loader in self.loaders.iter() {
            if let Ok(Some(unit)) = loader.load(unit_id).await {
                self.register(unit.clone()).ok()?;
                return Some(unit);
            }
        }
        
        None
    }
    
    /// List all units (with defer_loading, returns metadata only)
    pub fn list_metadata(&self) -> Vec<UnitMetadata> {
        self.metadata_cache.read()
            .ok()
            .map(|cache| cache.values().cloned().collect())
            .unwrap_or_default()
    }
    
    /// Discover units matching criteria
    pub fn discover(
        &self,
        query: &str,
        context: Option<ExecutionContext>,
    ) -> Vec<UnitMetadata> {
        let metadata = self.list_metadata();
        
        metadata.into_iter()
            .filter(|m| {
                // Match by name, description, or search_hints
                let matches_query = m.name.to_lowercase().contains(&query.to_lowercase())
                    || m.description.to_lowercase().contains(&query.to_lowercase())
                    || m.search_hints.iter().any(|hint| hint.to_lowercase().contains(&query.to_lowercase()));
                
                // Match by capability if context specified
                let matches_context = context.as_ref().map_or(true, |ctx| {
                    ctx.unit_supports_context(&m.capabilities)
                });
                
                matches_query && matches_context
            })
            .collect()
    }
}

/// Trait for loading units from various sources
#[async_trait]
pub trait UnitLoader: Send + Sync {
    /// Load a unit from this source (None = not found)
    async fn load(&self, unit_id: &str) -> Result<Option<ExecutableUnit>, ExecutableError>;
    
    /// List all units available from this source
    async fn list_all(&self) -> Result<Vec<ExecutableUnit>, ExecutableError>;
}

/// Loader for native RustyCode tools
pub struct NativeToolLoader {
    tool_registry: Arc<ToolRegistry>,
}

/// Loader for Claude Code skills
pub struct SkillLoader {
    skills_dir: PathBuf,
}

/// Loader for RustyCode agents
pub struct AgentLoader {
    agents_dir: PathBuf,
}
```

### 3.2 Tool Search Integration

```rust
/// Integration with Anthropic's Tool Search feature
pub struct ToolSearchService {
    registry: Arc<ExecutableRegistry>,
}

impl ToolSearchService {
    /// Search for tools matching query, respecting defer_loading
    pub async fn search(
        &self,
        query: &str,
        options: ToolSearchOptions,
    ) -> Result<Vec<ToolSearchResult>, ExecutableError> {
        let metadata_list = self.registry.discover(query, None);
        
        let mut results = Vec::new();
        for metadata in metadata_list {
            let result = ToolSearchResult {
                id: metadata.id.clone(),
                name: metadata.name.clone(),
                description: metadata.description.clone(),
                
                // Only include full definition if defer_loading not enabled
                full_definition: if options.include_full_definitions {
                    self.registry.get(&metadata.id)
                        .await
                        .map(|unit| unit.schema.clone())
                } else {
                    None
                },
                
                relevance_score: self.calculate_relevance(&metadata, query),
            };
            
            results.push(result);
        }
        
        // Sort by relevance
        results.sort_by(|a, b| b.relevance_score.partial_cmp(&a.relevance_score).unwrap());
        
        Ok(results.into_iter().take(options.limit).collect())
    }
    
    fn calculate_relevance(&self, metadata: &UnitMetadata, query: &str) -> f32 {
        let query_lower = query.to_lowercase();
        
        let name_match = if metadata.name.to_lowercase() == query_lower { 2.0 } else { 0.0 };
        let hint_match = metadata.search_hints.iter()
            .filter(|hint| hint.to_lowercase().contains(&query_lower))
            .count() as f32 * 0.5;
        let desc_match = if metadata.description.to_lowercase().contains(&query_lower) { 0.3 } else { 0.0 };
        
        name_match + hint_match + desc_match
    }
}

#[derive(Clone)]
pub struct ToolSearchOptions {
    pub include_full_definitions: bool,
    pub limit: usize,
}

pub struct ToolSearchResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub full_definition: Option<ToolSchema>,
    pub relevance_score: f32,
}
```

---

## 4. Integration Points

### 4.1 With rustycode-llm (Anthropic Provider)

```rust
// In rustycode-llm/src/anthropic.rs
impl AnthropicProvider {
    /// Build tool definitions for Claude, integrating advanced tool use
    async fn build_tool_definitions(
        &self,
        units: &[ExecutableUnit],
        options: ToolDefinitionOptions,
    ) -> Result<Vec<ToolDefinition>, ProviderError> {
        let mut definitions = Vec::new();
        
        for unit in units {
            let def = ToolDefinition {
                name: unit.id.clone(),
                description: unit.description.clone(),
                input_schema: unit.schema.as_ref().map(|s| s.parameters.clone()),
                
                // Add examples from advanced_metadata
                examples: if options.include_examples {
                    Some(self.format_examples(&unit.advanced_metadata.examples))
                } else {
                    None
                },
            };
            
            definitions.push(def);
        }
        
        Ok(definitions)
    }
    
    /// Handle tool calls with programmatic calling support
    async fn handle_tool_call(
        &self,
        tool_call: ToolCall,
        router: &ExecutionRouter,
    ) -> Result<ToolResult, ProviderError> {
        let context = if tool_call.is_programmatic {
            ExecutionContext::DirectTool {
                immediate_result: true,
                timeout_ms: Some(30000),
            }
        } else {
            ExecutionContext::DirectTool {
                immediate_result: false,
                timeout_ms: Some(60000),
            }
        };
        
        let input = ExecutionInput {
            data: tool_call.input,
            caller_info: Some(CallerInfo { role: AgentRole::Assistant }),
            session_context: None,
        };
        
        router.execute(&tool_call.tool_name, input, context)
            .await
            .map_err(|e| ProviderError::ToolExecutionFailed(e.to_string()))
    }
}
```

### 4.2 With rustycode-orchestration

```rust
// In rustycode-orchestration/src/executor.rs
pub struct OrchestrationExecutor {
    router: Arc<ExecutionRouter>,
    reasoning_loop: ReasoningLoop,
}

impl OrchestrationExecutor {
    /// Execute with unified callable abstraction
    pub async fn execute_task(
        &self,
        task: &Task,
        available_units: Vec<ExecutableUnit>,
    ) -> Result<TaskResult, ExecutionError> {
        // Register all units
        for unit in available_units {
            self.router.registry.register(unit)?;
        }
        
        // Run reasoning loop
        self.reasoning_loop.execute(
            task,
            &self.router,
        ).await
    }
}

// ReasoningLoop can now invoke any unit type uniformly:
impl ReasoningLoop {
    async fn step(
        &self,
        action: Action,
        router: &ExecutionRouter,
    ) -> Result<ActionResult, Error> {
        let context = self.select_context_for_action(&action);
        router.execute(&action.unit_id, action.input, context).await
            .map_err(|e| Error::ExecutionFailed(e.to_string()))
    }
}
```

### 4.3 With rustycode-tools

```rust
// Existing native tools are wrapped as ExecutableUnits
pub fn native_tool_to_executable(tool: &Tool) -> ExecutableUnit {
    ExecutableUnit {
        id: tool.name.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: false,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],  // Populated per-tool during setup
            defer_loading: false,  // Most native tools small enough to load
            search_hints: vec![],
            execution_strategy: ExecutionMode::Direct,
            result_processor: None,
        },
        handler: Arc::new(NativeToolCallable { tool: tool.clone() }),
        source: UnitSource::NativeTool { path: "native".into() },
        schema: Some(tool_schema(tool)),
        tags: vec![],
        version: None,
    }
}
```

---

## 5. Implementation Checklist

### Phase 1: Core Abstraction (Crate: rustycode-executable)
- [ ] Define `ExecutableUnit`, `ExecutionContext`, `Callable` trait
- [ ] Implement `ExecutionRouter` with context-based routing
- [ ] Implement `ExecutableRegistry` with metadata caching
- [ ] Add `UnitLoader` trait and basic implementations
- [ ] Unit tests for routing, registration, lazy-loading
- [ ] Integration test: register native tool, invoke in Direct context

### Phase 2: Source Integration
- [ ] Create `NativeToolLoader`: wrap rustycode-tools
- [ ] Create `SkillLoader`: load Claude Code ~/.claude/skills
- [ ] Create `AgentLoader`: load RustyCode agents
- [ ] Loader tests and integration tests
- [ ] Integration test: all three source types discoverable

### Phase 3: Advanced Tool Use
- [ ] Wire `AdvancedToolMetadata` into all units
- [ ] Implement `ToolSearchService` with defer_loading
- [ ] Add examples to 50+ native tools (data-driven)
- [ ] Integrate with Anthropic provider for tool definitions
- [ ] Integration test: tool search returns correct metadata
- [ ] Integration test: examples appear in Claude's tool defs

### Phase 4: Programmatic Calling
- [ ] Add `ExecutionMode::Programmatic` support
- [ ] Implement code generation for unit calls
- [ ] Test: Claude-generated code can chain units
- [ ] Integration test: agent invokes tool via generated code

### Phase 5: Orchestration Refactor
- [ ] Update `ReasoningLoop` to use `ExecutionRouter`
- [ ] Replace tool-specific logic with generic unit handling
- [ ] Add context-selection logic for Hybrid mode
- [ ] Update all orchestration tests
- [ ] System test: full reasoning loop with mixed unit types

---

## 6. Testing Strategy

### Unit Tests (per module, target 80%+)
- `ExecutableUnit` creation, validation
- `ExecutionRouter` routing logic
- `ExecutableRegistry` registration, lookup, discovery
- Error cases and edge cases
- `ToolSearchService` ranking

### Integration Tests (`tests/` directory)
- [ ] Register native tool as unit → invoke in DirectTool context
- [ ] Load Claude Code skill → invoke in SkillReference context
- [ ] Register agent → invoke in AgentReasoning context
- [ ] Tool search with 50+ units, defer_loading enabled
- [ ] Tool definitions include examples
- [ ] Circular dependency detection
- [ ] Cross-context: same unit in multiple contexts
- [ ] Lazy-loading with timeout fallback

### System Tests (orchestration-level)
- [ ] Full reasoning loop discovers and uses units
- [ ] Token count with defer_loading (~60% savings)
- [ ] Tool invocation accuracy with examples (~72% → 90%)
- [ ] Programmatic calling chains tools/skills correctly
- [ ] Hybrid execution context works end-to-end
- [ ] All unit types coexist in registry without conflicts

### Performance Tests
- [ ] Registration time for 100+ units
- [ ] Discovery time for "read file" among 100+ units
- [ ] Lazy-loading reduces memory by 50%+
- [ ] Execution routing <5ms overhead

---

## 7. Error Handling & Recovery

| Error | Cause | Recovery |
|-------|-------|----------|
| `UnitNotFound` | Unit ID doesn't exist | Try fuzzy search; suggest alternatives |
| `UnsupportedContext` | Unit can't run in requested context | List compatible contexts |
| `CircularDependency` | Unit A → Unit B → Unit A | Block execution; log dependency chain |
| `LazyLoadTimeout` | Defer-loaded unit takes >5s to load | Use cached metadata; mark unavailable |
| `CapabilityMissing` | Unit lacks required capability | Don't invoke in that context; suggest alternatives |
| `ExecutionFailed` | Unit execution throws | Return error with full context; suggest retry |

---

## 8. API Examples

### Example 1: Direct Tool Execution
```rust
let unit_id = "bash";
let input = ExecutionInput {
    data: json!({"command": "ls -la"}),
    caller_info: None,
    session_context: None,
};

let result = router.execute(
    unit_id,
    input,
    ExecutionContext::DirectTool {
        immediate_result: true,
        timeout_ms: Some(30000),
    },
).await?;

println!("{}", result.data);
```

### Example 2: Tool Search with Defer Loading
```rust
let results = search_service.search(
    "read file",
    ToolSearchOptions {
        include_full_definitions: false,  // defer_loading
        limit: 5,
    },
).await?;

for result in results {
    println!("{}: {}", result.name, result.description);
    // Full definition NOT included (saves tokens)
}
```

### Example 3: Hybrid Execution
```rust
// Let router choose context based on unit capabilities
let result = router.execute_hybrid(
    "code_reviewer",
    input,
).await?;
```

---

## 9. Breaking Changes & Migration

- **Tool registry**: All tools must be wrapped as ExecutableUnits
- **Tool invocation**: Use `ExecutionRouter` instead of direct registry calls
- **Skill loading**: Skills loaded into registry; no special skill handling
- **Agent dispatch**: Agents invoked via `ExecutionRouter` with AgentReasoning context

**Backward Compatibility**: None for internal code (clean break). Claude Code skill/tool ecosystem unaffected.

---

## 10. Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Token savings | 60% | Compare context size with/without defer_loading |
| Tool accuracy | 90% | Test correct invocation rate with examples |
| Discovery time | <100ms | Measure for 100+ units |
| Registration overhead | <5ms per unit | Measure for 100+ registrations |
| Test coverage | 80%+ | Line coverage on core modules |
| System uptime | 99%+ | No crashes during orchestration |

---

## 11. References

- **Advanced Tool Use Article**: https://www.anthropic.com/engineering/advanced-tool-use
- **RustyCode CLAUDE.md**: /Users/nat/dev/rustycode/CLAUDE.md
- **Anthropic API Docs**: https://docs.anthropic.com/

