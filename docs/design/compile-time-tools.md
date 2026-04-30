# Compile-Time Tool System Design

**Status:** Design Draft
**Author:** RustyCode Ensemble
**Created:** 2025-03-12

## Executive Summary

This document outlines a Rust-native compile-time tool system that maximizes type safety through zero-cost abstractions. Unlike typical agent tool systems that rely on runtime JSON serialization and dynamic dispatch, this design leverages Rust's type system to provide compile-time guarantees for tool registration, parameter validation, and permission checking.

**Key Benefits:**
- **Compile-time type safety** - Invalid tool calls fail to compile
- **Zero-cost abstractions** - Monomorphized calls with no dynamic dispatch overhead
- **Type-safe parameters** - Native Rust types, not JSON serde
- **Permission guarantees** - Compile-time permission checking where possible
- **Ergonomic authoring** - Declarative macros for tool definitions

## Motivation

### Current State: Runtime Tool Systems

Most AI coding agents use runtime tool registration:

```python
# Typical runtime tool system (Python-like pseudocode)
tools = {
    "read_file": {
        "parameters": {"path": "string"},
        "permission": "read"
    }
}

result = call_tool("read_file", {"path": "file.txt"})  # Runtime validation
```

**Problems:**
- Parameter type mismatches discovered at runtime
- No compile-time guarantee that tool exists
- Permission checks happen during execution
- JSON serialization overhead on every call
- String-based tool names (typos silently fail)

### Our Goal: Compile-Time Safety

```rust
// Compile-time tool system
call_tool!(read_file, path: "file.txt")?;  // Type-checked at compile time
```

**Benefits:**
- Wrong parameter types = compile error
- Tool name typos = compile error
- Missing required parameters = compile error
- Zero runtime overhead (monomorphization)
- Native Rust types throughout

## Design Overview

### Core Principles

1. **Types Over Strings** - Tool names and parameters are Rust types
2. **Compile-Time Registration** - Tools registered at compile time via macros
3. **Zero-Cost Execution** - Static dispatch where possible
4. **Progressive Enhancement** - Runtime checks only where compile-time impossible
5. **Ergonomic Macros** - `#[tool]` attribute for tool definitions

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Tool Definition Layer                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │  #[tool]    │  │  #[tool]    │  │  #[tool]    │          │
│  │  read_file  │  │  write_file │  │  exec_cmd   │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                   Compile-Time Layer (Macros)                │
│  ┌─────────────────┐  ┌─────────────────┐                   │
│  │  tool_registry! │  │  permission_check│                   │
│  │  (declarative)  │  │  (const eval)    │                   │
│  └─────────────────┘  └─────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                      Type System Layer                       │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │  Tool trait │  │  Permission │  │  Parameters │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    Execution Layer                           │
│  ┌─────────────────┐  ┌─────────────────┐                   │
│  │  Static Dispatch│  │  Runtime Fallback│                  │
│  │  (zero-cost)    │  │  (when needed)   │                   │
│  └─────────────────┘  └─────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Tool Trait with Associated Types

```rust
/// Core tool trait with associated types for compile-time type safety
pub trait Tool {
    /// Input parameter type (must be a struct)
    type Input: Serialize;

    /// Output type (must be a struct)
    type Output: DeserializeOwned;

    /// Error type (must implement std::error::Error)
    type Error: std::error::Error + Send + Sync + 'static;

    /// Tool metadata (const evaluable)
    const METADATA: ToolMetadata;

    /// Execute the tool
    fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error>;

    /// Validate parameters (compile-time checked)
    fn validate(input: &Self::Input) -> Result<(), ToolValidationError> {
        // Default implementation: no validation
        Ok(())
    }
}

/// Tool metadata (const-friendly)
#[derive(Clone, Debug)]
pub struct ToolMetadata {
    pub name: &'static str,
    pub category: ToolCategory,
    pub permission: Permission,
    pub description: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolCategory {
    ReadOnly,
    Write,
    Execute,
    Network,
    Stateful,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Permission {
    /// No restrictions
    None,
    /// Read-only filesystem access
    Read,
    /// Write filesystem access
    Write,
    /// Execute commands
    Execute,
    /// Network access
    Network,
    /// Custom permission set
    Custom(&'static [&'static str]),
}
```

### 2. Declarative Tool Registration Macro

```rust
/// Declarative macro for tool registration
#[macro_export]
macro_rules! tool_registry {
    (
        $(
            #[tool]
            #[permission($perm:ident)]
            #[category($cat:ident)]
            fn $name:ident($($param:ident : $param_ty:ty),* $(,)?)
                -> Result<$output:ty, $error:ty>
            $body:block
        )*
    ) => {
        // Generate tool enum
        #[derive(Debug, Clone)]
        pub enum Tool {
            $(
                $name($name),
            )*
        }

        // Generate tool registry
        pub static TOOLS: &[&'static ToolMetadata] = &[
            $(
                <$name as Tool>::METADATA,
            )*
        ];

        // Generate tool execution dispatcher
        impl ToolRegistry {
            pub fn execute<T: Tool>(
                &self,
                tool: T,
                input: T::Input,
            ) -> Result<T::Output, T::Error> {
                tool.validate(&input)?;
                tool.execute(input)
            }
        }

        $(
            // Generate tool struct
            #[derive(Debug, Clone)]
            pub struct $name;

            // Implement Tool trait
            impl Tool for $name {
                type Input = $name##Input;
                type Output = $output;
                type Error = $error;

                const METADATA: ToolMetadata = ToolMetadata {
                    name: stringify!($name),
                    category: ToolCategory::$cat,
                    permission: Permission::$perm,
                    description: concat!("Tool: ", stringify!($name)),
                };

                fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
                    $body
                }
            }

            // Generate input struct
            #[derive(Debug, Clone, Serialize)]
            pub struct $name##Input {
                $(
                    pub $param: $param_ty,
                )*
            }
        )*
    };
}

// Example usage:
tool_registry! {
    #[tool]
    #[permission(Read)]
    #[category(ReadOnly)]
    fn read_file(path: PathBuf) -> Result<String, io::Error> {
        fs::read_to_string(&path)
    }

    #[tool]
    #[permission(Write)]
    #[category(Write)]
    fn write_file(path: PathBuf, content: String) -> Result<(), io::Error> {
        fs::write(&path, &content)
    }
}
```

### 3. Type-Safe Parameter System

```rust
/// Type-safe parameter builder using labelled generics pattern
/// Inspired by frunk but simplified for tool parameters
pub trait Param {
    type Value;
    fn name(&self) -> &'static str;
    fn value(&self) -> &Self::Value;
}

/// Labelled parameter type
pub struct Labelled<N, V> {
    _name: PhantomData<N>,
    value: V,
}

impl<N, V> Labelled<N, V> {
    pub fn new(value: V) -> Self {
        Self {
            _name: PhantomData,
            value,
        }
    }
}

/// Type-safe parameter list
pub struct Params<P> {
    params: P,
}

/// Example: Building parameters with compile-time type checking
struct PathParam;
struct ContentParam;

impl Param for Labelled<PathParam, PathBuf> {
    type Value = PathBuf;
    fn name(&self) -> &'static str { "path" }
    fn value(&self) -> &Self::Value { &self.value }
}

impl Param for Labelled<ContentParam, String> {
    type Value = String;
    fn name(&self) -> &'static str { "content" }
    fn value(&self) -> &Self::Value { &self.value }
}

/// Macro for building parameter lists
#[macro_export]
macro_rules! params {
    ($($name:ident : $value:expr),* $(,)?) => {
        {
            use crate::tools::params::Labelled;

            $(
                let $name = Labelled::<ParamName::$name, _>::new($value);
            )*

            // Type-check that all required parameters are present
            // at compile time
        }
    };
}
```

### 4. Compile-Time Permission Checking

```rust
/// Permission checking with const evaluation where possible
pub trait PermissionChecker {
    /// Check if permission is granted (const when possible)
    const ALLOWED: bool;

    /// Runtime check for dynamic permissions
    fn check_runtime(&self) -> bool {
        Self::ALLOWED
    }
}

/// Compile-time permission macro
#[macro_export]
macro_rules! assert_permission {
    ($tool:ty, $perm:expr) => {
        const _: () = {
            assert!(
                <$perm as Permission>::includes(<$tool as Tool>::METADATA.permission),
                "Tool {:?} does not have permission {:?}",
                <$tool as Tool>::METADATA.name,
                $perm
            );
        };
    };
}

/// Example: Permission-gated tool execution
pub struct PermissionGuard<T: Tool, P: Permission> {
    tool: T,
    _permission: PhantomData<P>,
}

impl<T: Tool, P: Permission> PermissionGuard<T, P> {
    pub fn new(tool: T) -> Self {
        // Compile-time assertion
        assert_permission!(T, P);
        Self {
            tool,
            _permission: PhantomData,
        }
    }

    pub fn execute(&self, input: T::Input) -> Result<T::Output, T::Error> {
        // Runtime check (optimized out if compile-time proven)
        if P::check_runtime() {
            self.tool.execute(input)
        } else {
            Err(ToolError::PermissionDenied)?
        }
    }
}
```

### 5. Zero-Cost Execution Model

```rust
/// Static dispatcher for tool execution (zero-cost)
pub struct ToolDispatcher<T: Tool> {
    _marker: PhantomData<T>,
}

impl<T: Tool> ToolDispatcher<T> {
    pub fn dispatch(input: T::Input) -> Result<T::Output, T::Error> {
        // Direct call - monomorphized, inlined
        T::default().execute(input)
    }
}

/// Dynamic dispatcher for when tool type is unknown at compile time
/// (only used when absolutely necessary)
pub struct DynamicToolDispatcher {
    tools: HashMap<TypeId, Box<dyn AnyTool>>,
}

trait AnyTool: Send + Sync {
    fn execute_boxed(&self, input: Box<dyn Any>) -> Result<Box<dyn Any>, Box<dyn Error>>;
}

impl<T: Tool> AnyTool for T {
    fn execute_boxed(&self, input: Box<dyn Any>) -> Result<Box<dyn Any>, Box<dyn Error>> {
        let input = input.downcast::<T::Input>()
            .map_err(|_| "Invalid input type")?;
        let output = self.execute(*input)?;
        Ok(Box::new(output))
    }
}

/// Usage: Static dispatch (preferred)
let result = ToolDispatcher::<ReadFile>::dispatch(ReadFileInput {
    path: PathBuf::from("file.txt"),
})?;

/// Usage: Dynamic dispatch (when needed)
let result = dispatcher.execute("read_file", input)?;
```

## Macro-Based Tool Authoring

### Attribute Macro Design

```rust
/// Proc macro for tool definition
#[proc_macro_attribute]
pub fn tool(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the function
    let input_fn = parse_macro_input!(input as ItemFn);

    // Extract function metadata
    let name = &input_fn.sig.ident;
    let inputs = &input_fn.sig.inputs;
    let output = &input_fn.sig.output;
    let block = &input_fn.block;

    // Parse attributes
    let permission = extract_permission(&args);
    let category = extract_category(&args);

    // Generate:
    // 1. Tool struct
    // 2. Input struct from parameters
    // 3. Tool trait implementation
    // 4. Type-safe execute wrapper

    quote! {
        // Generated tool struct
        #[derive(Debug, Clone)]
        pub struct #name;

        // Generated input struct
        #[derive(Debug, Clone, Serialize)]
        pub struct #nameInput {
            // Extracted parameters...
        }

        // Tool implementation
        impl Tool for #name {
            type Input = #nameInput;
            type Output = #output;
            type Error = ToolError;

            const METADATA: ToolMetadata = ToolMetadata {
                name: stringify!(#name),
                category: ToolCategory::#category,
                permission: Permission::#permission,
                description: "", // From doc comment
            };

            fn execute(&self, input: Self::Input) -> Result<Self::Output, Self::Error> {
                // Call original function
                #block
            }
        }
    }.into()
}
```

### Example: Tool Definitions

```rust
/// Read file contents
#[tool]
#[permission(Read)]
#[category(ReadOnly)]
fn read_file(path: PathBuf) -> Result<String, io::Error> {
    fs::read_to_string(&path)
}

/// Write file contents
#[tool]
#[permission(Write)]
#[category(Write)]
fn write_file(path: PathBuf, content: String, create_parents: bool) -> Result<(), io::Error> {
    if create_parents {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&path, &content)
}

/// Execute shell command
#[tool]
#[permission(Execute)]
#[category(Execute)]
fn execute_command(
    command: String,
    args: Vec<String>,
    working_dir: Option<PathBuf>,
) -> Result<CommandOutput, ExecuteError> {
    let mut cmd = Command::new(&command);
    cmd.args(&args);
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    let output = cmd.output()?;
    Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into(),
        stderr: String::from_utf8_lossy(&output.stderr).into(),
        status: output.status,
    })
}

/// Git status with type-safe output
#[tool]
#[permission(Read)]
#[category(ReadOnly)]
fn git_status(repository_path: PathBuf) -> Result<GitStatus, GitError> {
    let repo = git2::Repository::open(&repository_path)?;
    let statuses = repo.statuses(None)?;

    let mut modified = Vec::new();
    let mut added = Vec::new();
    let mut untracked = Vec::new();

    for entry in statuses.iter() {
        let path = entry.path().ok_or(GitError::InvalidPath)?;
        match entry.status() {
            s if s.contains(git2::Status::WT_MODIFIED) => modified.push(path.into()),
            s if s.contains(git2::Status::INDEX_NEW) => added.push(path.into()),
            s if s.contains(git2::Status::WT_NEW) => untracked.push(path.into()),
            _ => {}
        }
    }

    Ok(GitStatus {
        modified,
        added,
        untracked,
    })
}
```

## Type-Safe Tool Execution

### Call-Site API

```rust
/// Type-safe tool call macro
#[macro_export]
macro_rules! call_tool {
    // Static dispatch (preferred)
    ($tool:ty, $($param:ident : $value:expr),*) => {{
        use crate::tools::{Tool, ToolDispatcher};

        let input = <$tool as Tool>::Input {
            $(
                $param: $value,
            )*
        };

        ToolDispatcher::<$tool>::dispatch(input)
    }};

    // Dynamic dispatch with type hint
    (dynamic, $tool_name:expr, $($param:ident : $value:expr),*) => {{
        use crate::tools::DynamicToolDispatcher;

        // Runtime lookup but type-checked input
        let registry = TOOL_REGISTRY.get();
        registry.execute($tool_name, params!($($param: $value),*))
    }};
}

// Usage examples
fn examples() -> Result<()> {
    // Static dispatch: compile-time type checking
    let contents = call_tool!(ReadFile, path: PathBuf::from("Cargo.toml"))?;

    // Dynamic dispatch: runtime tool lookup
    let contents = call_tool!(dynamic, "read_file", path: PathBuf::from("Cargo.toml"))?;

    // Type error: wrong parameter type
    // let contents = call_tool!(ReadFile, path: 123)?;
    //                    ^^^^^^^ expected PathBuf, found integer

    // Type error: missing required parameter
    // let contents = call_tool!(ReadFile)?;
    //                    ^^^^^^^ missing `path`

    Ok(())
}
```

### Result-Based Error Handling

```rust
/// Tool error with context
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Invalid parameter: {field_name} - {reason}")]
    InvalidParameter {
        field_name: String,
        reason: String,
    },

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Tool-specific error: {0}")]
    Custom(Box<dyn std::error::Error + Send + Sync>),
}

/// Result type with context
pub type ToolResult<T> = Result<T, ToolError>;

/// Extension trait for ergonomic error handling
pub trait ToolResultExt<T> {
    fn with_context<F>(self, f: F) -> ToolResult<T>
    where
        F: FnOnce() -> String;
}

impl<T> ToolResultExt<T> for ToolResult<T> {
    fn with_context<F>(self, f: F) -> ToolResult<T>
    where
        F: FnOnce() -> String,
    {
        self.map_err(|e| ToolError::Custom(
            format!("{}: {}", f(), e).into()
        ))
    }
}

// Usage
fn example() -> ToolResult<String> {
    call_tool!(ReadFile, path: PathBuf::from("config.toml"))
        .with_context(|| "Failed to read configuration file")
}
```

## Zero-Cost Abstractions

### Monomorphization Example

```rust
// Define a generic tool executor
pub fn execute_tool<T: Tool>(input: T::Input) -> Result<T::Output, T::Error> {
    T::default().execute(input)
}

// Compiler generates specialized versions:
// - execute_tool::<ReadFile> -> direct call to ReadFile::execute
// - execute_tool::<WriteFile> -> direct call to WriteFile::execute
// - No vtable lookup, possible inlining

// Assembly comparison:

// With dynamic dispatch:
// mov rax, [rdi + 8]      ; Load vtable
// mov rax, [rax + 16]     ; Load function pointer
// call rax                ; Indirect call

// With monomorphization:
// call read_file::execute ; Direct call
//                         ; (possibly inlined)

// Performance difference: ~2-5ns per call (vtable overhead)
```

### Compile-Time Tool Discovery

```rust
/// Tool registry built at compile time
pub struct ToolRegistry;

#[macro_export]
macro_rules! impl_tool_registry {
    ($($tool:ty),*) => {
        impl ToolRegistry {
            pub const ALL_TOOLS: &[ToolMetadata] = &[
                $( <$tool as Tool>::METADATA ),*
            ];

            pub fn get_tool(name: &str) -> Option<ToolMetadata> {
                Self::ALL_TOOLS.iter()
                    .find(|m| m.name == name)
                    .copied()
            }

            pub fn tools_by_category(category: ToolCategory) -> Vec<&'static ToolMetadata> {
                Self::ALL_TOOLS.iter()
                    .filter(|m| m.category == category)
                    .collect()
            }
        }
    };
}

// Usage
impl_tool_registry!(
    ReadFile,
    WriteFile,
    ExecuteCommand,
    GitStatus,
);

// At compile time:
// - All tools must exist and implement Tool
// - Tool names are validated
// - Categories are checked
```

## Comparison: Runtime vs Compile-Time

### Runtime Tool System (Typical Agent)

```python
# Definition
class ReadFileTool:
    name = "read_file"
    parameters = {"path": "string"}
    permission = "read"

    def execute(self, params):
        return read_file(params["path"])

# Registration
registry = {"read_file": ReadFileTool()}

# Execution (all runtime checks)
def call_tool(name, params):
    tool = registry.get(name)
    if not tool:
        raise ToolNotFound(name)

    # Validate parameters at runtime
    for param, typ in tool.parameters.items():
        if param not in params:
            raise MissingParameter(param)
        if not isinstance(params[param], eval(typ)):
            raise InvalidParameterType(param)

    # Check permission at runtime
    if not has_permission(tool.permission):
        raise PermissionDenied(tool.permission)

    return tool.execute(params)

# Problems:
# 1. "red_fle" typo won't be caught until runtime
# 2. Wrong parameter type discovered at runtime
# 3. Permission check happens every call
# 4. JSON serialization overhead
```

### Compile-Time Tool System (Our Design)

```rust
// Definition (once)
#[tool]
#[permission(Read)]
fn read_file(path: PathBuf) -> Result<String, io::Error> {
    fs::read_to_string(&path)
}

// Registration (compile-time)
tool_registry!(read_file);

// Execution (compile-time checked)
let result = call_tool!(ReadFile, path: PathBuf::from("file.txt"))?;

// Benefits:
// 1. "red_fle" would be compile error
// 2. Wrong parameter type = compile error
// 3. Permission checked at compile time (when possible)
// 4. No serialization, native Rust types
```

### Performance Comparison

| Aspect | Runtime (Python) | Runtime (Rust) | Compile-Time (Rust) |
|--------|------------------|----------------|---------------------|
| Tool lookup | HashMap (hash) | HashMap (hash) | Direct (monomorphized) |
| Parameter validation | Runtime JSON | Runtime serde | Compile-time type check |
| Permission check | Runtime | Runtime | Compile-time (when possible) |
| Call overhead | ~200ns | ~50ns | ~5ns (or inlined) |
| Memory usage | Boxed traits | Boxed traits | Static (zero allocation) |
| Binary size | N/A | +100KB | +200KB (code bloat) |

## Advanced Patterns

### Tool Composition

```rust
/// Compose multiple tools into a workflow
pub trait ToolChain {
    type Input;
    type Output;

    fn execute_chain(&self, input: Self::Input) -> Result<Self::Output, ToolError>;
}

/// Example: Git commit workflow
#[derive(Debug)]
pub struct GitCommitWorkflow {
    repo_path: PathBuf,
    message: String,
}

impl ToolChain for GitCommitWorkflow {
    type Input = ();
    type Output = git2::Oid;

    fn execute_chain(&self, _input: Self::Input) -> Result<Self::Output, ToolError> {
        // Step 1: Check status
        let status = call_tool!(GitStatus, repository_path: self.repo_path.clone())?;

        // Step 2: Stage files
        for file in &status.modified {
            call_tool!(GitAdd,
                repository_path: self.repo_path.clone(),
                path: file.clone()
            )?;
        }

        // Step 3: Commit
        let result = call_tool!(GitCommit,
            repository_path: self.repo_path.clone(),
            message: self.message.clone()
        )?;

        Ok(result.oid)
    }
}
```

### Conditional Tool Execution

```rust
/// Conditional tool execution based on permissions
pub fn execute_with_permission<T: Tool, P: Permission>(
    input: T::Input,
) -> Result<T::Output, ToolError>
where
    P: PermissionChecker,
{
    if P::check_runtime() {
        Ok(T::default().execute(input)?)
    } else {
        Err(ToolError::PermissionDenied(format!(
            "{} requires {:?} permission",
            T::METADATA.name,
            T::METADATA.permission
        )))
    }
}

// Usage
type ReadPermission = Permission::Read;

let contents = execute_with_permission::<ReadFile, ReadPermission>(input)?;
```

### Tool Aliases and Overloads

```rust
/// Create tool aliases with different defaults
pub struct ToolAlias<T: Tool, F: Fn(&T::Input) -> T::Input> {
    _tool: PhantomData<T>,
    _modifier: F,
}

impl<T: Tool, F: Fn(&T::Input) -> T::Input> ToolAlias<T, F> {
    pub fn execute(&self, mut input: T::Input) -> Result<T::Output, T::Error> {
        // Apply modifier
        let modified = input;
        T::default().execute(modified)
    }
}

// Example: ReadFile with UTF-8 validation
type ReadFileUtf8 = ToolAlias<ReadFile, fn(&ReadFileInput) -> ReadFileInput>;

fn validate_utf8(input: &ReadFileInput) -> ReadFileInput {
    // Add validation flag
    ReadFileInput {
        path: input.path.clone(),
        validate_utf8: true,
    }
}
```

## Implementation Strategy

### Phase 1: Core Traits and Types

```rust
// File: rustycode-tools/src/lib.rs
pub mod tool;
pub mod permission;
pub mod params;
pub mod registry;

// Re-export key types
pub use tool::{Tool, ToolMetadata, ToolCategory};
pub use permission::{Permission, PermissionChecker};
pub use registry::ToolRegistry;
```

### Phase 2: Declarative Macros

```rust
// File: rustycode-tools-macros/src/lib.rs
proc_macro::decl_tool! {
    // Implement #[tool] attribute
}

proc_macro::tool_registry! {
    // Implement tool_registry! macro
}

proc_macro::call_tool! {
    // Implement call_tool! macro
}
```

### Phase 3: Standard Tool Library

```rust
// File: rustycode-tools/src/tools/
pub mod fs {
    pub use read_file::ReadFile;
    pub use write_file::WriteFile;
    pub use delete_file::DeleteFile;
}

pub mod git {
    pub use git_status::GitStatus;
    pub use git_commit::GitCommit;
}

pub mod exec {
    pub use execute_command::ExecuteCommand;
}
```

### Phase 4: Integration with Core

```rust
// File: rustycode-core/src/tools.rs
use rustycode_tools::{Tool, ToolRegistry};

pub struct ToolExecutor {
    registry: ToolRegistry,
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::with_default_tools(),
        }
    }

    pub fn execute<T: Tool>(&self, tool: T, input: T::Input) -> Result<T::Output, T::Error> {
        tool.validate(&input)?;
        tool.execute(input)
    }
}
```

## Trade-offs and Limitations

### What Can Be Checked at Compile Time

✅ **Compile-Time Checks:**
- Tool exists and implements `Tool` trait
- Parameter types match function signature
- Required parameters are present
- Basic permission compatibility
- Return type compatibility

### What Requires Runtime Checks

⚠️ **Runtime Checks:**
- Dynamic permission revocation
- File existence and accessibility
- Command availability in PATH
- Network connectivity
- User-provided input validation
- Sandbox constraints

### Binary Size Considerations

**Monomorphization Cost:**
- Each tool instantiation generates specialized code
- ~2-5KB per tool (depends on complexity)
- Mitigation: LTO (Link-Time Optimization) can deduplicate

**Example:**
```
Without LTO:
  100 tools × 3KB = 300KB

With LTO:
  100 tools × 1KB = 100KB (shared code deduplicated)
```

### Compile-Time Impact

**Macro Expansion:**
- Initial compilation: +5-10 seconds (1000 tools)
- Incremental compilation: negligible (changed tools only)
- Debug builds: slower due to monomorphization
- Release builds: faster due to optimization

## Future Enhancements

### 1. Const Generic Tool Parameters

```rust
// Future: Const generics for tool parameters
pub trait Tool<const N: usize> {
    type Input: ArrayLength<N>;
}

// Compile-time parameter validation
const MAX_PATH_LEN: usize = 260;

#[tool]
fn read_file(path: PathBuf) -> Result<String, io::Error>
where
    PathBuf: PathLength<{MAX_PATH_LEN}>,
{
    // Compiler ensures path length <= 260
}
```

### 2. Type-Level Tool Dependencies

```rust
// Future: Type-level dependency graph
pub trait ToolDeps {
    type Dependencies: ToolList;
}

// Compile-time dependency checking
impl ToolDeps for GitCommit {
    type Dependencies = (GitStatus, GitAdd);
}

// Compiler verifies all dependencies are available
```

### 3. Async Tool Support

```rust
// Future: Async tools with compile-time checking
pub trait AsyncTool {
    type Input;
    type Output;

    async fn execute(&self, input: Self::Input) -> Result<Self::Output, ToolError>;
}

#[tool]
async fn fetch_url(url: String) -> Result<String, reqwest::Error> {
    reqwest::get(&url).await?.text().await
}
```

## References

### Inspirations

1. **Frunk** - Labelled generics and HList
   - https://github.com/lloydmeta/frunk
   - Type-safe struct transformations

2. **Diesel** - Compile-time SQL queries
   - https://diesel.rs
   - DSL that prevents invalid queries at compile time

3. **Clap** - Derive macros for CLI
   - https://github.com/clap-rs/clap
   - Ergonomic derive-based API design

4. **Typenum** - Type-level numbers
   - https://github.com/paholg/typenum
   - Compile-time numeric computations

### Related Work

- **Serde** - Serialization framework (derive-based)
- **ThisError** - Error handling (derive-based)
- **Anyhow** - Error context (ergonomic)
- **Tower** - Service abstraction (middleware)

## Conclusion

This design demonstrates how Rust's type system can provide unprecedented compile-time safety for tool systems in AI agents. By leveraging:

1. **Associated types** for input/output contracts
2. **Declarative macros** for ergonomics
3. **Monomorphization** for zero-cost execution
4. **Const evaluation** for permission checks

We can create a tool system that is:
- **Type-safe** (wrong calls don't compile)
- **Fast** (no dynamic dispatch overhead)
- **Ergonomic** (simple macro-based API)
- **Maintainable** (clear error messages)

The result is a system that catches more errors at compile time while maintaining runtime flexibility where needed. This approach pushes the boundaries of what's possible with compile-time guarantees in agent tool systems.

**Next Steps:**
1. Implement core traits (`Tool`, `Permission`, `Param`)
2. Build declarative macros (`#[tool]`, `tool_registry!`)
3. Create standard tool library (fs, git, exec)
4. Benchmark against runtime systems
5. Integrate with rustycode-core
