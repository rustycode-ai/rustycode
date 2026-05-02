use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustycode_executable::{ExecutableRegistry, ToolSearchService, ToolSearchOptions};
use std::sync::Arc;

// Helper to create a test tool unit
fn make_test_unit(id: &str) -> rustycode_executable::ExecutableUnit {
    rustycode_executable::ExecutableUnit {
        id: id.to_string(),
        name: format!("Tool {}", id),
        description: Some(format!("Test tool {} with comprehensive description for token counting", id)),
        source: rustycode_executable::UnitSource::NativeTool,
        callable: Arc::new(rustycode_executable::types::NoOpCallable),
        metadata: Some(rustycode_executable::AdvancedToolMetadata {
            examples: vec![
                rustycode_executable::ExecutionExample {
                    scenario: "Example 1".to_string(),
                    input: r#"{"param": "value"}"#.to_string(),
                    output: r#"{"result": "done"}"#.to_string(),
                    explanation: "This is a detailed explanation of the first example".to_string(),
                };
                5 // 5 examples per tool
            ],
            ..Default::default()
        }),
    }
}

fn bench_defer_loading_enabled(c: &mut Criterion) {
    c.bench_function("search_with_defer_loading_enabled", |b| {
        b.iter(|| {
            let registry = Arc::new(ExecutableRegistry::new());
            for i in 0..50 {
                registry.register(black_box(make_test_unit(&format!("tool_{}", i)))).unwrap();
            }

            let search = ToolSearchService::new(registry);
            let opts = ToolSearchOptions {
                defer_loading: true,
                include_full_definitions: false,
                limit: Some(10),
            };
            search.search("tool", opts).unwrap()
        });
    });
}

fn bench_defer_loading_disabled(c: &mut Criterion) {
    c.bench_function("search_with_defer_loading_disabled", |b| {
        b.iter(|| {
            let registry = Arc::new(ExecutableRegistry::new());
            for i in 0..50 {
                registry.register(black_box(make_test_unit(&format!("tool_{}", i)))).unwrap();
            }

            let search = ToolSearchService::new(registry);
            let opts = ToolSearchOptions {
                defer_loading: false,
                include_full_definitions: true,
                limit: Some(10),
            };
            search.search("tool", opts).unwrap()
        });
    });
}

criterion_group!(benches, bench_defer_loading_enabled, bench_defer_loading_disabled);
criterion_main!(benches);
