// Copyright 2025 The RustyCode Authors. All rights reserved.
// Use of this source code is governed by an MIT-style license.

//! Tool dispatch benchmarks
//!
//! Benchmarks runtime tool dispatch performance:
//! - Runtime dispatch (dynamic trait objects)
//! - Tool execution overhead
//! - Parameter validation
//! - JSON parsing
//! - Registry operations

use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use rustycode_tools::{
    BashInput, BashTool, GlobInput, GrepInput, ReadFileInput, ReadFileTool, Tool, ToolContext,
};
use std::path::PathBuf;
use tempfile::TempDir;

// Dispatch Benchmarks

/// Benchmark runtime tool dispatch
fn bench_runtime_dispatch(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    let test_file = dir.path().join("test.txt");
    std::fs::write(&test_file, "Hello, World!").unwrap();

    let tool = ReadFileTool;
    let ctx = ToolContext::new(dir.path());
    let params = serde_json::json!({"path": "test.txt"});

    c.bench_function("runtime_dispatch", |b| {
        b.iter(|| {
            let result = tool.execute(black_box(params.clone()), black_box(&ctx));
            black_box(result)
        })
    });
}

/// Benchmark tool metadata access
fn bench_tool_metadata(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_metadata");

    group.bench_function("runtime_metadata", |b| {
        let tool = ReadFileTool;
        b.iter(|| {
            let name = black_box(tool.name());
            let desc = black_box(tool.description());
            black_box((name, desc))
        })
    });

    group.finish();
}

/// Benchmark permission checking
fn bench_permission_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("permission_check");

    let read_tool = ReadFileTool;
    let bash_tool = BashTool;

    group.bench_function("runtime_read_tool", |b| {
        b.iter(|| {
            let perm = black_box(read_tool.permission());
            black_box(perm)
        })
    });

    group.bench_function("runtime_bash_tool", |b| {
        b.iter(|| {
            let perm = black_box(bash_tool.permission());
            black_box(perm)
        })
    });

    group.finish();
}

// Throughput Benchmarks

/// Benchmark high-throughput dispatch
fn bench_dispatch_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_throughput");

    let dir = TempDir::new().unwrap();
    let test_file = dir.path().join("test.txt");
    std::fs::write(&test_file, "Hello, World!").unwrap();

    let throughput = Throughput::Elements(1000);

    // Runtime dispatch throughput
    group.throughput(throughput);
    group.bench_function("runtime_1000", |b| {
        let tool = ReadFileTool;
        let ctx = ToolContext::new(dir.path());
        let params = serde_json::json!({"path": "test.txt"});

        b.iter(|| {
            for _ in 0..1000 {
                let result = tool.execute(black_box(params.clone()), black_box(&ctx));
                let _ = black_box(result);
            }
        })
    });

    group.finish();
}

// JSON Parsing Benchmarks

/// Benchmark JSON parameter parsing overhead
fn bench_json_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_parsing");

    let json_str = r#"{"path": "/etc/hosts", "start_line": 1, "end_line": 10}"#;

    group.bench_function("parse_json", |b| {
        b.iter(|| {
            let value: serde_json::Value = serde_json::from_str(black_box(json_str)).unwrap();
            black_box(value)
        })
    });

    // Compare with struct construction
    group.bench_function("construct_struct", |b| {
        b.iter(|| {
            let input = ReadFileInput {
                path: PathBuf::from("/etc/hosts"),
                start_line: Some(1),
                end_line: Some(10),
            };
            black_box(input)
        })
    });

    group.finish();
}

// Tool-Specific Benchmarks

/// Benchmark Grep tool
fn bench_grep_tool(c: &mut Criterion) {
    let mut group = c.benchmark_group("grep_tool");

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("test1.txt"), "Hello World").unwrap();
    std::fs::write(dir.path().join("test2.txt"), "Hello Again").unwrap();
    std::fs::write(dir.path().join("test3.txt"), "Goodbye").unwrap();

    let input = GrepInput {
        pattern: "Hello".to_string(),
        path: Some(dir.path().to_path_buf()),
        max_depth: Some(1),
        case_insensitive: Some(false),
    };

    group.bench_function("runtime_grep", |b| {
        b.iter(|| {
            let result = rustycode_tools::GrepTool
                .execute(black_box(serde_json::to_value(&input).unwrap()), black_box(&ToolContext::new(dir.path())));
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark Glob tool
fn bench_glob_tool(c: &mut Criterion) {
    let mut group = c.benchmark_group("glob_tool");

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("test1.rs"), "content").unwrap();
    std::fs::write(dir.path().join("test2.rs"), "content").unwrap();
    std::fs::write(dir.path().join("test3.txt"), "content").unwrap();

    let input = GlobInput {
        pattern: "*.rs".to_string(),
        path: Some(dir.path().to_path_buf()),
        max_depth: Some(1),
        case_insensitive: Some(false),
    };

    group.bench_function("runtime_glob", |b| {
        b.iter(|| {
            let result = rustycode_tools::GlobTool
                .execute(black_box(serde_json::to_value(&input).unwrap()), black_box(&ToolContext::new(dir.path())));
            black_box(result)
        })
    });

    group.finish();
}

/// Benchmark Bash tool
fn bench_bash_tool(c: &mut Criterion) {
    let mut group = c.benchmark_group("bash_tool");

    let input = BashInput {
        command: "echo".to_string(),
        args: Some(vec!["test".to_string()]),
        working_dir: None,
        timeout_secs: Some(5),
    };

    group.bench_function("runtime_bash", |b| {
        b.iter(|| {
            let result = BashTool
                .execute(black_box(serde_json::to_value(&input).unwrap()), black_box(&ToolContext::new(std::env::temp_dir())));
            black_box(result)
        })
    });

    group.finish();
}

// Error Handling Benchmarks

/// Benchmark error handling
fn bench_error_handling(c: &mut Criterion) {
    let mut group = c.benchmark_group("error_handling");

    let tool = ReadFileTool;
    let ctx = ToolContext::new(std::env::temp_dir());

    // Valid parameters
    let dir = TempDir::new().unwrap();
    let test_file = dir.path().join("test.txt");
    std::fs::write(&test_file, "content").unwrap();

    let valid_params = serde_json::json!({"path": test_file.to_str().unwrap()});

    // Invalid parameters (missing path)
    let invalid_params = serde_json::json!({});

    group.bench_function("successful_call", |b| {
        b.iter(|| {
            let result = tool.execute(black_box(valid_params.clone()), black_box(&ctx));
            black_box(result)
        })
    });

    group.bench_function("error_call", |b| {
        b.iter(|| {
            let result = tool.execute(black_box(invalid_params.clone()), black_box(&ctx));
            black_box(result)
        })
    });

    group.finish();
}

// Different Tools Benchmarks

/// Benchmark different tool types
fn bench_different_tools(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_types");

    let dir = TempDir::new().unwrap();
    let ctx = ToolContext::new(dir.path());

    group.bench_function("runtime_read", |b| {
        let tool = ReadFileTool;
        let test_file = dir.path().join("test.txt");
        std::fs::write(&test_file, "content").unwrap();
        let params = serde_json::json!({"path": test_file.to_str().unwrap()});
        b.iter(|| {
            let result = tool.execute(black_box(params.clone()), black_box(&ctx));
            black_box(result)
        })
    });

    group.bench_function("runtime_bash", |b| {
        let tool = BashTool;
        let params = serde_json::json!({"command": "echo", "args": ["test"]});
        b.iter(|| {
            let result = tool.execute(black_box(params.clone()), black_box(&ctx));
            black_box(result)
        })
    });

    group.finish();
}

// Registry Operations

/// Benchmark tool registry operations
fn bench_registry_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_operations");

    group.bench_function("create_registry", |b| {
        b.iter(|| {
            let registry = rustycode_tools::ToolRegistry::new();
            black_box(registry)
        })
    });

    group.bench_function("default_registry", |b| {
        b.iter(|| {
            let registry = rustycode_tools::default_registry();
            black_box(registry)
        })
    });

    group.finish();
}

// Main Benchmark Group

criterion_group!(
    benches,
    bench_runtime_dispatch,
    bench_tool_metadata,
    bench_permission_check,
    bench_dispatch_throughput,
    bench_json_parsing,
    bench_grep_tool,
    bench_glob_tool,
    bench_bash_tool,
    bench_error_handling,
    bench_different_tools,
    bench_registry_operations,
);
criterion_main!(benches);
