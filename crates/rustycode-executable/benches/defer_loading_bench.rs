// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::semicolon_if_nothing_returned
)]

//! Benchmarks for `defer_loading` performance in `ExecutableRegistry`
//!
//! Measures:
//! - Registration throughput with 50 units (mix of deferred and fully loaded)
//! - Search query across registered units via `ToolSearchService`
//! - Relevance-scored search with deferred-loading units

use std::sync::Arc;

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rustycode_executable::{
    AdvancedToolMetadata, Callable, ExecutableError, ExecutableRegistry, ExecutableUnit,
    ExecutionContext, ExecutionInput, ExecutionMetadata, ExecutionMode, ExecutionOutput,
    ToolSearchService, UnitCapabilities, UnitSource,
};

struct BenchCallable;

#[async_trait::async_trait]
impl Callable for BenchCallable {
    async fn execute(
        &self,
        input: ExecutionInput,
        _context: ExecutionContext,
    ) -> Result<ExecutionOutput, ExecutableError> {
        Ok(ExecutionOutput {
            data: input.data,
            metadata: ExecutionMetadata {
                duration_ms: 0,
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Tool names and descriptions used to populate the registry.
/// Covers a realistic mix of file ops, search, VCS, and LSP tools.
const TOOL_DEFINITIONS: &[(&str, &str)] = &[
    ("read_file", "Read file contents from the filesystem"),
    ("write_file", "Write content to a file on disk"),
    ("edit_file", "Apply targeted edits to an existing file"),
    ("bash", "Execute shell commands in a subprocess"),
    ("grep", "Search file contents with regex patterns"),
    ("glob", "Find files matching a glob pattern"),
    ("git_status", "Show working tree status"),
    (
        "git_diff",
        "Show changes between commits or the working tree",
    ),
    ("git_log", "Show commit history"),
    ("git_add", "Stage file changes for the next commit"),
    ("git_commit", "Record changes to the repository"),
    ("git_push", "Push local commits to a remote"),
    ("git_pull", "Fetch and merge from a remote"),
    ("lsp_hover", "Show type information at a position"),
    ("lsp_goto_definition", "Jump to the definition of a symbol"),
    ("lsp_references", "Find all references to a symbol"),
    ("lsp_rename", "Rename a symbol across the workspace"),
    ("lsp_diagnostics", "Get diagnostics for a file"),
    ("web_fetch", "Fetch content from a URL"),
    ("notebook_edit", "Edit Jupyter notebook cells"),
    ("apply_patch", "Apply a unified diff patch to a file"),
    ("search", "Full-text search across the codebase"),
    ("search_replace", "Search and replace across files"),
    ("multiedit", "Apply multiple edits in a single operation"),
    ("list_directory", "List directory contents"),
    ("create_directory", "Create a directory tree"),
    ("move_file", "Move or rename a file"),
    ("delete_file", "Delete a file from the filesystem"),
    ("file_info", "Get file metadata (size, permissions, mtime)"),
    ("skill_install", "Install a skill from a registry"),
    ("skill_list", "List installed skills"),
    ("skill_execute", "Execute a named skill"),
    ("skill_bundler", "Bundle knowledge for skill execution"),
    ("agent_delegate", "Delegate a subtask to a specialist agent"),
    ("agent_status", "Query agent execution status"),
    ("agent_cancel", "Cancel a running agent"),
    ("mcp_connect", "Connect to an MCP server"),
    ("mcp_call_tool", "Call a tool exposed by an MCP server"),
    ("memory_store", "Store a key-value pair in session memory"),
    ("memory_recall", "Recall a value from session memory"),
    ("context_pack", "Pack context for an LLM call"),
    (
        "context_expand",
        "Expand packed context into conversation form",
    ),
    ("ast_parse", "Parse source code into an AST"),
    ("ast_query", "Query AST nodes with a pattern"),
    ("ast_transform", "Apply a transformation to an AST"),
    ("prompt_render", "Render a prompt template with variables"),
    ("plan_create", "Create an execution plan from a goal"),
    ("plan_execute", "Execute a plan step by step"),
    ("plan_status", "Get the current status of a plan"),
    ("tool_registry", "Inspect registered tool metadata and capabilities"),
];

/// Build an `ExecutableUnit` with the given ID, description, and defer_loading flag.
fn make_unit(id: &str, description: &str, defer_loading: bool) -> ExecutableUnit {
    ExecutableUnit {
        id: id.to_string(),
        name: id.to_string(),
        description: description.to_string(),
        capabilities: UnitCapabilities {
            can_execute_directly: true,
            can_bundle_knowledge: !defer_loading,
            can_reason_autonomously: false,
        },
        advanced_metadata: AdvancedToolMetadata {
            examples: vec![],
            defer_loading,
            search_hints: vec![id.to_string()],
            execution_strategy: if defer_loading {
                ExecutionMode::Bundled
            } else {
                ExecutionMode::Direct
            },
            result_processor: None,
        },
        handler: Arc::new(BenchCallable),
        source: UnitSource::NativeTool {
            path: format!("tools/{id}"),
        },
        schema: None,
        tags: vec![],
        version: None,
    }
}

/// Populate a fresh registry with `count` units.
/// Even-indexed units are fully loaded; odd-indexed units have defer_loading = true.
fn populate_registry(count: usize) -> ExecutableRegistry {
    let registry = ExecutableRegistry::new();
    for (i, (name, desc)) in TOOL_DEFINITIONS.iter().take(count).enumerate() {
        let defer = i % 2 == 1;
        let unit = make_unit(name, desc, defer);
        // register() is sync internally (uses block_on)
        registry
            .register(unit)
            .expect("registration should succeed");
    }
    registry
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Benchmark: how long to register 50 units one-by-one.
fn bench_registration_50_units(c: &mut Criterion) {
    let mut group = c.benchmark_group("defer_loading/registration");

    group.bench_function("register_50_units", |b| {
        b.iter(|| {
            let registry = ExecutableRegistry::new();
            for (i, (name, desc)) in TOOL_DEFINITIONS.iter().take(50).enumerate() {
                let defer = i % 2 == 1;
                let unit = make_unit(name, desc, defer);
                registry
                    .register(unit)
                    .expect("registration should succeed");
            }
            black_box(registry);
        })
    });

    group.finish();
}

/// Benchmark: how long to register N units for varying N.
fn bench_registration_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("defer_loading/registration_scaling");

    for count in [10, 25, 50] {
        group.bench_with_input(BenchmarkId::new("register", count), &count, |b, &count| {
            b.iter(|| {
                let registry = ExecutableRegistry::new();
                for (i, (name, desc)) in TOOL_DEFINITIONS.iter().take(count).enumerate() {
                    let defer = i % 2 == 1;
                    let unit = make_unit(name, desc, defer);
                    registry
                        .register(unit)
                        .expect("registration should succeed");
                }
                black_box(registry);
            })
        });
    }

    group.finish();
}

/// Benchmark: search across 50 registered units using ToolSearchService.
fn bench_search_50_units(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let registry = populate_registry(50);
    let search_service = ToolSearchService::new(Arc::new(registry));

    let mut group = c.benchmark_group("defer_loading/search_50");

    group.bench_function("exact_name_match", |b| {
        b.iter(|| {
            rt.block_on(async {
                let opts = rustycode_executable::discovery::ToolSearchOptions {
                    include_full_definitions: false,
                    limit: 10,
                };
                let results = search_service
                    .search("grep", opts)
                    .await
                    .expect("search should succeed");
                black_box(results);
            })
        })
    });

    group.bench_function("partial_name_match", |b| {
        b.iter(|| {
            rt.block_on(async {
                let opts = rustycode_executable::discovery::ToolSearchOptions {
                    include_full_definitions: false,
                    limit: 10,
                };
                let results = search_service
                    .search("git", opts)
                    .await
                    .expect("search should succeed");
                black_box(results);
            })
        })
    });

    group.bench_function("description_match", |b| {
        b.iter(|| {
            rt.block_on(async {
                let opts = rustycode_executable::discovery::ToolSearchOptions {
                    include_full_definitions: false,
                    limit: 10,
                };
                let results = search_service
                    .search("filesystem", opts)
                    .await
                    .expect("search should succeed");
                black_box(results);
            })
        })
    });

    group.finish();
}

/// Benchmark: search with full definition resolution (deferred loading path).
fn bench_search_with_definitions(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let registry = populate_registry(50);
    let search_service = ToolSearchService::new(Arc::new(registry));

    let mut group = c.benchmark_group("defer_loading/search_with_definitions");

    group.bench_function("include_definitions", |b| {
        b.iter(|| {
            rt.block_on(async {
                let opts = rustycode_executable::discovery::ToolSearchOptions {
                    include_full_definitions: true,
                    limit: 10,
                };
                let results = search_service
                    .search("file", opts)
                    .await
                    .expect("search should succeed");
                black_box(results);
            })
        })
    });

    group.finish();
}

/// Benchmark: registry discover() method directly (no search service overhead).
fn bench_discover_direct(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let registry = populate_registry(50);

    let mut group = c.benchmark_group("defer_loading/discover_direct");

    group.bench_function("broad_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = registry.discover("file", None).await;
                black_box(results);
            })
        })
    });

    group.bench_function("narrow_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = registry.discover("git_status", None).await;
                black_box(results);
            })
        })
    });

    group.bench_function("no_match_query", |b| {
        b.iter(|| {
            rt.block_on(async {
                let results = registry.discover("nonexistent_tool_xyz", None).await;
                black_box(results);
            })
        })
    });

    group.finish();
}

/// Benchmark: mixed registration and lookup pattern (simulates startup).
fn bench_registration_and_lookup(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

    let mut group = c.benchmark_group("defer_loading/startup_pattern");

    group.bench_function("register_then_lookup_50", |b| {
        b.iter(|| {
            let registry = ExecutableRegistry::new();
            for (i, (name, desc)) in TOOL_DEFINITIONS.iter().take(50).enumerate() {
                let defer = i % 2 == 1;
                let unit = make_unit(name, desc, defer);
                registry
                    .register(unit)
                    .expect("registration should succeed");
            }
            // Sync lookups to simulate cold-start lookup path
            for (name, _) in TOOL_DEFINITIONS.iter().take(50) {
                let result = registry.get_sync(name);
                black_box(result);
            }
            black_box(registry);
        })
    });

    group.bench_function("register_then_async_lookup_50", |b| {
        b.iter(|| {
            let registry = ExecutableRegistry::new();
            for (i, (name, desc)) in TOOL_DEFINITIONS.iter().take(50).enumerate() {
                let defer = i % 2 == 1;
                let unit = make_unit(name, desc, defer);
                registry
                    .register(unit)
                    .expect("registration should succeed");
            }
            // Async lookups
            rt.block_on(async {
                for (name, _) in TOOL_DEFINITIONS.iter().take(50) {
                    let result = registry.get(name).await;
                    black_box(result);
                }
            });
            black_box(registry);
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_registration_50_units,
    bench_registration_scaling,
    bench_search_50_units,
    bench_search_with_definitions,
    bench_discover_direct,
    bench_registration_and_lookup,
);
criterion_main!(benches);
