# rustycode-tools API Reference

**Version:** 0.1.0
**Last Updated:** 2025-03-14

Complete API reference for the rustycode-tools framework, including all tool interfaces, metadata structures, and extension points.

## Table of Contents

- [Core Types](#core-types)
- [Tool Registry](#tool-registry)
- [Tool Executor](#tool-executor)
- [Built-in Tools](#built-in-tools)
- [Plugin System](#plugin-system)
- [Compile-Time Tools](#compile-time-tools)
- [Caching System](#caching-system)
- [Rate Limiting](#rate-limiting)
- [Security](#security)
- [Error Handling](#error-handling)

---

## Core Types

### ToolContext

Runtime context passed to every tool invocation.

```rust
pub struct ToolContext {
    pub cwd: PathBuf,              // Current working directory
    pub sandbox: SandboxConfig,    // Security sandbox configuration
    pub max_permission: ToolPermission,  // Maximum allowed permission
}
```

**Methods:**
- `new(cwd: impl AsRef<Path>) -> Self` - Create context with default settings
- `with_sandbox(self, sandbox: SandboxConfig) -> Self` - Set sandbox configuration
- `with_max_permission(self, perm: ToolPermission) -> Self` - Set max permission

### SandboxConfig

Security configuration for tool execution.

```rust
pub struct SandboxConfig {
    pub allowed_paths: Option<Vec<PathBuf>>,  // Allowed paths (None = all)
    pub denied_paths: Vec<PathBuf>,           // Blocked paths
    pub timeout_secs: Option<u64>,            // Execution timeout
    pub max_output_bytes: Option<usize>,      // Output size limit
}
```

**Methods:**
- `new() -> Self` - Create default config
- `allow_path(self, path: impl AsRef<Path>) -> Self` - Add allowed path
- `deny_path(self, path: impl AsRef<Path>) -> Self` - Add denied path
- `timeout(self, secs: u64) -> Self` - Set timeout
- `max_output(self, bytes: usize) -> Self` - Set output limit

### ToolPermission

Permission levels for tool operations.

```rust
pub enum ToolPermission {
    None,      // No restrictions
    Read,      // Read-only filesystem access
    Write,     // Write filesystem access
    Execute,   // Command execution
    Network,   // Network access
}
```

**Permission hierarchy:** None < Read < Write < Execute < Network

### ToolOutput

Output produced by tool execution.

```rust
pub struct ToolOutput {
    pub text: String,                    // Plain-text result
    pub structured: Option<Value>,       // Optional JSON data
}
```

**Methods:**
- `text(text: impl Into<String>) -> Self` - Create text-only output
- `with_structured(text: impl Into<String>, structured: Value) -> Self` - Create output with metadata

### Tool Trait

Core trait that all tools must implement.

```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn permission(&self) -> ToolPermission;
    fn parameters_schema(&self) -> Value;
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput>;
}
```

---

## Tool Registry

### ToolRegistry

Central registry for tool registration and dispatch.

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    rate_limiter: Arc<RateLimiter>,
}
```

**Methods:**

#### `new() -> Self`
Create a new registry with default rate limiting (10 req/s, burst 20).

#### `with_rate_limiting(max_per_second: NonZeroU32, max_burst: NonZeroU32) -> Self`
Create registry with custom rate limiting.

#### `register(&mut self, tool: impl Tool + 'static)`
Register a tool with the registry.

```rust
let mut registry = ToolRegistry::new();
registry.register(ReadFileTool);
registry.register(CustomTool);
```

#### `list(&self) -> Vec<ToolInfo>`
List all registered tools.

```rust
let tools = registry.list();
for tool in tools {
    println!("{}: {}", tool.name, tool.description);
}
```

#### `get(&self, name: &str) -> Option<&dyn Tool>`
Get a tool by name.

#### `execute(&self, call: &ToolCall, ctx: &ToolContext) -> ToolResult`
Execute a tool call.

### ToolInfo

Metadata about a registered tool.

```rust
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
    pub permission: ToolPermission,
    pub defer_loading: Option<bool>,
}
```

---

## Tool Executor

### ToolExecutor

High-level executor with caching and event bus integration.

```rust
pub struct ToolExecutor {
    registry: ToolRegistry,
    context: ToolContext,
    bus: Option<Arc<EventBus>>,
    cache: Arc<ToolCache>,
}
```

**Constructors:**

#### `new(cwd: PathBuf) -> Self`
Create executor with default settings.

#### `with_cache(cwd: PathBuf, cache_config: CacheConfig) -> Self`
Create executor with custom cache configuration.

#### `with_event_bus(cwd: PathBuf, bus: Arc<EventBus>) -> Self`
Create executor with event bus integration.

#### `with_todo_state(cwd: PathBuf, todo_state: TodoState) -> Self`
Create executor with todo state management.

**Methods:**

#### `list(&self) -> Vec<ToolInfo>`
List available tools.

#### `execute(&self, call: &ToolCall) -> ToolResult`
Execute a tool call.

#### `execute_with_session(&self, call: &ToolCall, session_id: Option<SessionId>) -> ToolResult`
Execute with optional session tracking and event publishing.

#### `execute_cached_with_session(&self, call: &ToolCall, session_id: Option<SessionId>) -> ToolResult`
Execute with caching support.

---

## Built-in Tools

### File Operations

#### ReadFileTool

**Permission:** Read
**Description:** Read UTF-8 text files with optional line ranges

**Parameters:**
```json
{
  "type": "object",
  "required": ["path"],
  "properties": {
    "path": { "type": "string" },
    "start_line": { "type": "integer", "description": "First line to return (1-indexed, inclusive)" },
    "end_line": { "type": "integer", "description": "Last line to return (1-indexed, inclusive)" }
  }
}
```

**Metadata Fields:**
- `path` (string) - Absolute file path
- `total_bytes` (number) - File size in bytes
- `shown_bytes` (number) - Bytes included in output
- `total_lines` (number) - Total line count
- `shown_lines` (number) - Lines included in output
- `binary` (boolean) - Whether file is binary
- `content_hash` (string) - SHA-256 hash for caching
- `language` (string) - Detected programming language
- `truncated` (boolean) - Whether output was truncated

#### WriteFileTool

**Permission:** Write
**Description:** Write content to files

**Parameters:**
```json
{
  "type": "object",
  "required": ["path", "content"],
  "properties": {
    "path": { "type": "string" },
    "content": { "type": "string" }
  }
}
```

**Metadata Fields:**
- `path` (string) - Absolute file path
- `bytes` (number) - Bytes written
- `lines` (number) - Lines written

#### ListDirTool

**Permission:** Read
**Description:** List directory contents with filtering

**Parameters:**
```json
{
  "type": "object",
  "required": [],
  "properties": {
    "path": { "type": "string", "description": "Directory path (default: \".\")" },
    "recursive": { "type": "boolean", "description": "List recursively (default: false)" },
    "max_depth": { "type": "integer", "description": "Maximum depth (default: 3)" },
    "filter": { "type": "string", "description": "\"file\", \"dir\", \"all\", or \".ext\"" }
  }
}
```

**Metadata Fields:**
- `path` (string) - Directory path
- `total_items` (number) - Total entries
- `recursive` (boolean) - Whether listing was recursive
- `max_depth` (number) - Maximum depth
- `filter` (string) - Applied filter

### Search Operations

#### GrepTool

**Permission:** Read
**Description:** Search files with regex patterns

**Parameters:**
```json
{
  "type": "object",
  "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string" },
    "path": { "type": "string" },
    "before_context": { "type": "integer", "description": "Lines before match" },
    "after_context": { "type": "integer", "description": "Lines after match" },
    "max_matches_per_file": { "type": "integer", "description": "Limit matches per file" }
  }
}
```

**Metadata Fields:**
- `pattern` (string) - Search pattern
- `total_matches` (number) - Total matches found
- `files_with_matches` (number) - Files containing pattern
- `top_files` (array) - Up to 10 files with most matches
- `truncated` (boolean) - Whether output was truncated

#### GlobTool

**Permission:** Read
**Description:** Pattern-based file matching

**Parameters:**
```json
{
  "type": "object",
  "required": ["pattern"],
  "properties": {
    "pattern": { "type": "string" }
  }
}
```

**Metadata Fields:**
- `pattern` (string) - Search pattern
- `total_matches` (number) - Files matching pattern
- `extensions` (array) - Breakdown by extension
- `truncated` (boolean) - Whether output was truncated

### Command Execution

#### BashTool

**Permission:** Execute
**Description:** Execute shell commands with timeout

**Parameters:**
```json
{
  "type": "object",
  "required": ["command"],
  "properties": {
    "command": { "type": "string" },
    "cwd": { "type": "string", "description": "Working directory override" },
    "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default: 30)" },
    "transform": { "type": "string", "description": "Output transformation name" }
  }
}
```

**Metadata Fields:**
- `exit_code` (number) - Process exit code
- `command` (string) - Executed command
- `execution_time_ms` (number) - Execution duration
- `timeout_secs` (number) - Timeout limit
- `failed` (boolean) - True if exit_code != 0
- `total_lines` (number) - Output line count
- `total_bytes` (number) - Output byte count
- `truncated` (boolean) - Whether output was truncated

### Web Operations

#### WebFetchTool

**Permission:** Network
**Description:** Fetch URLs with response metadata

**Parameters:**
```json
{
  "type": "object",
  "required": ["url"],
  "properties": {
    "url": { "type": "string" },
    "convert_markdown": { "type": "boolean", "description": "Convert HTML to markdown" }
  }
}
```

**Metadata Fields:**
- `url` (string) - Fetched URL
- `chars` (number) - Characters returned
- `truncated` (boolean) - Whether content was truncated
- `converted` (boolean) - Whether HTML was converted
- `status_code` (number) - HTTP status code
- `time_to_first_byte_ms` (number) - Time to first byte
- `total_time_ms` (number) - Total request time
- `headers` (object) - Response headers

### Version Control

#### GitStatusTool

**Permission:** Read
**Description:** Get git status information

**Metadata Fields:**
- `branch` (string) - Current branch
- `ahead` (number) - Commits ahead of remote
- `behind` (number) - Commits behind remote
- `staged` (number) - Staged changes
- `unstaged` (number) - Unstaged changes
- `untracked` (number) - Untracked files

#### GitDiffTool

**Permission:** Read
**Description:** Get git diff with statistics

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "path_spec": { "type": "string" },
    "cached": { "type": "boolean" },
    "color_words": { "type": "boolean" }
  }
}
```

**Metadata Fields:**
- `files_changed` (number) - Files changed
- `additions` (number) - Lines added
- `deletions` (number) - Lines deleted
- `path_spec` (string) - Path spec used

#### GitLogTool

**Permission:** Read
**Description:** Get git commit history

**Parameters:**
```json
{
  "type": "object",
  "properties": {
    "limit": { "type": "integer" },
    "path_spec": { "type": "string" }
  }
}
```

**Metadata Fields:**
- `commit_count` (number) - Commits returned
- `limit` (number) - Applied limit
- `path_spec` (string) - Path spec used

#### GitCommitTool

**Permission:** Write
**Description:** Create git commits

**Parameters:**
```json
{
  "type": "object",
  "required": ["message"],
  "properties": {
    "message": { "type": "string" },
    "paths": { "type": "array", "items": { "type": "string" } }
  }
}
```

**Metadata Fields:**
- `commit_hash` (string) - Created commit SHA
- `branch` (string) - Branch committed to
- `files_changed` (number) - Files in commit
- `message` (string) - Commit message

### LSP Tools

#### LspDiagnosticsTool

**Permission:** Read
**Description:** Get LSP diagnostics

**Metadata Fields:**
- `file_count` (number) - Files with diagnostics
- `error_count` (number) - Total errors
- `warning_count` (number) - Total warnings
- `info_count` (number) - Total info messages
- `hint_count` (number) - Total hints

#### LspHoverTool

**Permission:** Read
**Description:** Get hover information

**Parameters:**
```json
{
  "type": "object",
  "required": ["file", "line", "column"],
  "properties": {
    "file": { "type": "string" },
    "line": { "type": "integer" },
    "column": { "type": "integer" }
  }
}
```

**Metadata Fields:**
- `file` (string) - File path
- `line` (number) - Line number
- `column` (number) - Column number
- `language` (string) - Language ID

#### LspDefinitionTool

**Permission:** Read
**Description:** Go to definition

**Parameters:**
```json
{
  "type": "object",
  "required": ["file", "line", "column"],
  "properties": {
    "file": { "type": "string" },
    "line": { "type": "integer" },
    "column": { "type": "integer" }
  }
}
```

**Metadata Fields:**
- `file` (string) - Target file path
- `line` (number) - Target line number
- `column` (number) - Target column number
- `definition_kind` (string) - Type of definition

#### LspCompletionTool

**Permission:** Read
**Description:** Get code completions

**Parameters:**
```json
{
  "type": "object",
  "required": ["file", "line", "column"],
  "properties": {
    "file": { "type": "string" },
    "line": { "type": "integer" },
    "column": { "type": "integer" }
  }
}
```

**Metadata Fields:**
- `file` (string) - File path
- `line` (number) - Line number
- `column` (number) - Column number
- `completion_count` (number) - Completions provided
- `incomplete` (boolean) - Whether results are incomplete

---

## Plugin System

### ToolPlugin Trait

Trait for implementing custom tools.

```rust
pub trait ToolPlugin: Send + Sync {
    /// Get plugin name
    fn name(&self) -> &str;

    /// Get plugin description
    fn description(&self) -> &str;

    /// Get required capabilities
    fn capabilities(&self) -> PluginCapabilities;

    /// Execute the tool
    fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput>;

    /// Get parameter schema
    fn parameters_schema(&self) -> Value;

    /// Initialize plugin state
    fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Cleanup resources
    fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }
}
```

### PluginCapabilities

Declare what capabilities a plugin needs.

```rust
pub struct PluginCapabilities {
    pub max_permission: ToolPermission,
    pub filesystem: bool,
    pub network: bool,
    pub execute: bool,
    pub max_memory_mb: Option<usize>,
    pub max_cpu_secs: Option<u64>,
}
```

**Methods:**
- `read_only() -> Self` - Create read-only capabilities
- `full_access() -> Self` - Create full-access capabilities
- `validate(&self, allowed: &PluginCapabilities) -> Result<()>` - Validate against allowed capabilities

### PluginManager

Synchronous plugin manager.

```rust
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn ToolPlugin>>,
}
```

**Methods:**
- `new() -> Self` - Create new manager
- `register(&mut self, plugin: Box<dyn ToolPlugin>) -> Result<()>` - Register plugin
- `execute(&self, name: &str, params: Value, ctx: &ToolContext) -> Result<ToolOutput>` - Execute plugin
- `list(&self) -> Vec<PluginInfo>` - List plugins
- `cleanup(&mut self)` - Cleanup all plugins

### AsyncPluginManager

Async plugin manager for long-running operations.

```rust
pub struct AsyncPluginManager {
    plugins: HashMap<String, Box<dyn ToolPlugin>>,
}
```

**Methods:**
- `new() -> Self` - Create new manager
- `register(&mut self, plugin: Box<dyn ToolPlugin>) -> Result<()>` - Register plugin
- `execute_async(&self, name: &str, params: Value, ctx: &ToolContext) -> Pin<Box<dyn Future<Output = Result<ToolOutput>>>` - Execute plugin asynchronously

### PluginInfo

Metadata about a registered plugin.

```rust
pub struct PluginInfo {
    pub name: String,
    pub description: String,
    pub capabilities: PluginCapabilities,
}
```

---

## Compile-Time Tools

### Tool Trait (Compile-Time)

Type-safe tool trait with associated types.

```rust
pub trait Tool {
    type Input: Send + Sync;
    type Output: Send + Sync;
    type Error: std::error::Error + Send + Sync + 'static;

    const METADATA: ToolMetadata;

    fn execute(input: Self::Input) -> Result<Self::Output, Self::Error>;
    fn validate(input: &Self::Input) -> Result<(), ToolValidationError>;
    fn parameters_schema() -> Option<serde_json::Value>;
}
```

### ToolMetadata

Static tool metadata (const-friendly).

```rust
pub struct ToolMetadata {
    pub name: &'static str,
    pub description: &'static str,
    pub permission: ToolPermission,
    pub category: ToolCategory,
}
```

### ToolCategory

Tool categorization.

```rust
pub enum ToolCategory {
    ReadOnly,
    Write,
    Execute,
    Network,
    Stateful,
}
```

### ToolDispatcher

Zero-cost dispatcher for compile-time tools.

```rust
pub struct ToolDispatcher<T: Tool> {
    _phantom: PhantomData<T>,
}
```

**Methods:**
- `dispatch(input: T::Input) -> Result<T::Output, T::Error>` - Execute tool with zero-cost dispatch

### Available Compile-Time Tools

#### CompileTimeReadFile

```rust
pub struct CompileTimeReadFile;

impl Tool for CompileTimeReadFile {
    type Input = ReadFileInput;
    type Output = ReadFileOutput;
    type Error = ReadFileError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "read_file",
        description: "Read a file with type safety",
        permission: ToolPermission::Read,
        category: ToolCategory::ReadOnly,
    };
}
```

**Input:**
```rust
pub struct ReadFileInput {
    pub path: PathBuf,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}
```

**Output:**
```rust
pub struct ReadFileOutput {
    pub content: String,
    pub line_count: usize,
    pub byte_count: usize,
    pub language: Option<String>,
}
```

#### CompileTimeWriteFile

```rust
pub struct CompileTimeWriteFile;

impl Tool for CompileTimeWriteFile {
    type Input = WriteFileInput;
    type Output = WriteFileOutput;
    type Error = WriteFileError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "write_file",
        description: "Write a file with type safety",
        permission: ToolPermission::Write,
        category: ToolCategory::Write,
    };
}
```

#### CompileTimeGrep

```rust
pub struct CompileTimeGrep;

impl Tool for CompileTimeGrep {
    type Input = GrepInput;
    type Output = GrepOutput;
    type Error = GrepError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "grep",
        description: "Search files with regex",
        permission: ToolPermission::Read,
        category: ToolCategory::ReadOnly,
    };
}
```

#### CompileTimeGlob

```rust
pub struct CompileTimeGlob;

impl Tool for CompileTimeGlob {
    type Input = GlobInput;
    type Output = GlobOutput;
    type Error = GlobError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "glob",
        description: "Match file patterns",
        permission: ToolPermission::Read,
        category: ToolCategory::ReadOnly,
    };
}
```

#### CompileTimeBash

```rust
pub struct CompileTimeBash;

impl Tool for CompileTimeBash {
    type Input = BashInput;
    type Output = BashOutput;
    type Error = BashError;

    const METADATA: ToolMetadata = ToolMetadata {
        name: "bash",
        description: "Execute shell commands",
        permission: ToolPermission::Execute,
        category: ToolCategory::Execute,
    };
}
```

---

## Caching System

### ToolCache

Thread-safe LRU cache with file-based invalidation.

```rust
pub struct ToolCache {
    cache: Arc<RwLock<LruCache<CacheKey, CacheEntry>>>,
    config: CacheConfig,
    metrics: Arc<RwLock<CacheMetrics>>,
}
```

**Methods:**
- `new(config: CacheConfig) -> Self` - Create cache with configuration
- `new_with_defaults() -> Self` - Create cache with default config
- `get(&self, key: &CacheKey) -> Option<CachedToolResult>` - Get cached result
- `put(&self, key: CacheKey, result: CachedToolResult, dependencies: Vec<PathBuf>, ttl: Option<Duration>)` - Cache result
- `invalidate(&self, key: &CacheKey)` - Invalidate specific entry
- `invalidate_path(&self, path: &PathBuf)` - Invalidate entries depending on path
- `clear(&self)` - Clear all cache entries
- `get_metrics(&self) -> CacheMetrics` - Get cache metrics
- `reset_metrics(&self)` - Reset metrics

### CacheConfig

Cache configuration.

```rust
pub struct CacheConfig {
    pub default_ttl: Duration,           // Default TTL
    pub max_entries: usize,              // Maximum entries
    pub track_file_dependencies: bool,   // Track file dependencies
    pub max_memory_bytes: Option<usize>, // Memory limit
    pub enable_metrics: bool,            // Enable metrics
}
```

**Default values:**
- `default_ttl`: 300 seconds (5 minutes)
- `max_entries`: 1000
- `track_file_dependencies`: true
- `max_memory_bytes`: 100 MB
- `enable_metrics`: true

### CacheKey

Cache entry key.

```rust
pub struct CacheKey {
    pub tool_name: String,
    pub arguments_hash: u64,
}
```

**Methods:**
- `new(tool_name: String, arguments: &Value) -> Self` - Create cache key

### CachedToolResult

Cached tool result.

```rust
pub struct CachedToolResult {
    pub output: String,
    pub structured: Option<Value>,
    pub success: bool,
    pub error: Option<String>,
}
```

### CacheMetrics

Cache performance metrics.

```rust
pub struct CacheMetrics {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub invalidations: usize,
    pub memory_usage_bytes: usize,
    pub entry_count: usize,
}
```

**Methods:**
- `hit_rate(&self) -> f64` - Calculate cache hit rate
- `total_requests(&self) -> usize` - Total cache requests

---

## Rate Limiting

### RateLimiter

Token-bucket rate limiter for DoS protection.

```rust
pub struct RateLimiter {
    limiters: Arc<TokioMutex<HashMap<String, GovernorRateLimiter>>>,
    global: GovernorRateLimiter,
    max_per_second: NonZeroU32,
    max_burst: NonZeroU32,
}
```

**Methods:**
- `new(max_per_second: NonZeroU32, max_burst: NonZeroU32) -> Self` - Create rate limiter
- `check_limit(&self, key: &str) -> Result<()>` - Check if request is allowed
- `quota(&self) -> (NonZeroU32, NonZeroU32)` - Get current quota

**Default values:**
- `max_per_second`: 10
- `max_burst`: 20

---

## Security

### Path Validation Functions

```rust
// Validate read operations
fn validate_read_path(path: &str, cwd: &PathBuf) -> Result<PathBuf>

// Validate write operations
fn validate_write_path(path: &str, cwd: &PathBuf) -> Result<PathBuf>

// Validate list operations
fn validate_list_path(path: &str, cwd: &PathBuf) -> Result<PathBuf>

// Validate URLs
fn validate_url(url: &str) -> Result<String>

// Validate regex patterns
fn validate_regex_pattern(pattern: &str) -> Result<()>

// Validate command safety
fn validate_command_safety(command: &str) -> Result<()>
```

### Blocked Extensions

Security-sensitive file extensions that are blocked:

- **Executables:** exe, dll, so, dylib, app, bin
- **Databases:** db, sqlite, mdb

### Binary File Detection

The following 70+ extensions are detected as binary:

**Images:** png, jpg, jpeg, gif, bmp, ico, webp, svg, tiff, psd, ai, eps
**Audio:** mp3, wav, ogg, flac, aac, m4a, wma
**Video:** mp4, avi, mkv, mov, wmv, flv, webm
**Archives:** zip, tar, gz, bz2, rar, 7z, xz, zst
**Documents:** pdf, doc, docx, xls, xlsx, ppt, pptx
**Fonts:** ttf, otf, woff, woff2, eot

---

## Error Handling

### ToolResult

Result structure returned by tool execution.

```rust
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub structured: Option<Value>,
}
```

### AuditLogEntry

Audit log entry for tool execution.

```rust
pub struct AuditLogEntry {
    pub tool_name: String,
    pub timestamp: u64,
    pub duration_ms: Option<u128>,
    pub success: bool,
    pub error: Option<String>,
    pub output_size: usize,
}
```

**Methods:**
- `new(tool_name: String, duration_ms: Option<u128>, success: bool, error: Option<String>, output_size: usize) -> Self` - Create audit log entry

---

## Helper Functions

### Tool Permission Functions

```rust
/// Get protocol-level permission for a tool
pub fn get_tool_permission(tool_name: &str) -> Option<ProtocolToolPermission>

/// Check if tool is allowed in session mode
pub fn check_tool_permission(tool_name: &str, mode: SessionMode) -> bool

/// Get all tools allowed in a mode
pub fn get_allowed_tools(mode: SessionMode) -> Vec<String>
```

### Default Registry

```rust
/// Build registry pre-loaded with all built-in tools
pub fn default_registry() -> ToolRegistry
```

### Truncation Constants

```rust
pub const READ_MAX_LINES: usize = 80;
pub const READ_MAX_BYTES: usize = 10_240;
pub const BASH_MAX_LINES: usize = 30;
pub const BASH_MAX_BYTES: usize = 51_200;
pub const GREP_MAX_MATCHES: usize = 15;
pub const LIST_MAX_ITEMS: usize = 30;
```

### Truncation Functions

```rust
pub fn truncate_lines(text: &str, max_lines: usize) -> TruncatedOutput
pub fn truncate_bytes(text: &str, max_bytes: usize) -> TruncatedOutput
pub fn truncate_items<T>(items: &[T], max_items: usize) -> (Vec<T>, bool)
pub fn truncate_bash_output(stdout: &str, stderr: &str) -> (String, String, bool)
```

---

## See Also

- [EXAMPLES.md](./EXAMPLES.md) - Usage examples and patterns
- [INTEGRATION.md](./INTEGRATION.md) - Integration guide
- [TOOL_METADATA.md](./TOOL_METADATA.md) - Metadata specification
- [LLM_METADATA_SPEC.md](./LLM_METADATA_SPEC.md) - LLM metadata format
