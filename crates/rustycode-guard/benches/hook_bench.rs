use criterion::{criterion_group, criterion_main, Criterion};
use rustycode_guard::codec::HookInput;
use serde_json::json;

fn bench_pre_tool_evaluate(c: &mut Criterion) {
    let input = HookInput {
        session_id: None,
        tool_name: "Bash".to_string(),
        tool_input: json!({"command": "echo hello"}),
        cwd: Some("/workspace/project".to_string()),
        hook_event_name: None,
    };
    c.bench_function("pre_tool_evaluate", |b| {
        b.iter(|| rustycode_guard::pre_tool::evaluate(&input));
    });
}

criterion_group!(benches, bench_pre_tool_evaluate);
criterion_main!(benches);
