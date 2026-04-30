// Configuration loading performance benchmarks
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::semicolon_if_nothing_returned,
    clippy::format_push_string,
    clippy::uninlined_format_args,
    clippy::explicit_iter_loop,
    clippy::literal_string_with_formatting_args,
    deprecated
)]

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rustycode_config::ConfigLoader;
use std::hint::black_box;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_test_config(dir: &std::path::Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).unwrap();
    path
}

fn bench_config_load_single(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    let config_content = r#"{
        "model": "claude-3-5-sonnet-latest",
        "temperature": 0.1,
        "max_tokens": 4096,
        "providers": {
            "anthropic": {
                "api_key": "sk-ant-test-benchmark-key-not-real-but-long-enough-to-pass-validation"
            }
        }
    }"#;

    create_test_config(dir, "config.json", config_content);

    let mut loader = ConfigLoader::new();

    c.bench_function("config_load_single", |b| {
        b.iter(|| {
            let path = dir.join("config.json");
            black_box(loader.load_from_path(black_box(&path)).unwrap());
        })
    });
}

fn bench_config_load_hierarchical(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path().to_path_buf();

    let global_content = r#"{
        "model": "claude-3-5-sonnet-latest",
        "temperature": 0.1,
        "providers": {
            "anthropic": {
                "api_key": "sk-ant-test-benchmark-key-not-real-but-long-enough"
            }
        }
    }"#;

    let project_content = r#"{
        "temperature": 0.3,
        "max_tokens": 8192
    }"#;

    create_test_config(&dir, "global.json", global_content);
    create_test_config(&dir, "project.json", project_content);

    let mut loader = ConfigLoader::new();

    c.bench_function("config_load_hierarchical", |b| {
        b.iter(|| {
            black_box(loader.load(black_box(&dir))).unwrap();
        })
    });
}

fn bench_config_merge(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path().to_path_buf();

    // Create multiple config files
    for i in 0..5 {
        let content = format!(r#"{{"layer_{i}": "value"{i} }}"#);
        create_test_config(&dir, &format!("config_{i}.json"), &content);
    }

    let mut loader = ConfigLoader::new();

    c.bench_function("config_merge_5_files", |b| {
        b.iter(|| {
            black_box(loader.load(black_box(&dir))).unwrap();
        })
    });
}

fn bench_config_with_substitutions(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let dir = temp_dir.path();

    std::env::set_var("TEST_VAR", "test_value");
    std::env::set_var("TEST_VAR2", "substituted_path_value");

    let config_content = r#"{
        "model": "{env:TEST_VAR}",
        "api_key": "sk-ant-test-bench-key-placeholder",
        "path": "{env:TEST_VAR2}"
    }"#;

    let config_path = create_test_config(dir, "config.json", config_content);

    let mut loader = ConfigLoader::new();

    c.bench_function("config_with_substitutions", |b| {
        b.iter(|| {
            black_box(loader.load_from_path(black_box(&config_path)).unwrap());
        })
    });
}

fn bench_config_size_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("config_size_scaling");

    for size in &[100, 500, 1000, 5000] {
        let temp_dir = TempDir::new().unwrap();
        let dir = temp_dir.path();

        // Create config with many fields
        let mut config = String::from("{");
        for i in 0..*size {
            if i > 0 {
                config.push(',');
            }
            config.push_str(&format!(r#""field_{i}": "value_{i}""#));
        }
        config.push('}');

        let config_path = create_test_config(dir, "large_config.json", &config);

        let mut loader = ConfigLoader::new();

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                black_box(loader.load_from_path(black_box(&config_path)).unwrap());
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_config_load_single,
    bench_config_load_hierarchical,
    bench_config_merge,
    bench_config_with_substitutions,
    bench_config_size_scaling
);
criterion_main!(benches);
