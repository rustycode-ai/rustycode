#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustycode_guard::codec::{HookInput, HookResult};
use rustycode_guard::pre_tool;
use serde_json::json;

#[test]
fn stdin_to_stdout_integration() {
    let input = HookInput {
        session_id: None,
        tool_name: "Bash".to_string(),
        tool_input: json!({"command": "sudo ls"}),
        cwd: Some("/workspace/project".to_string()),
        hook_event_name: None,
    };
    let input_json = serde_json::to_string(&input).expect("serializing HookInput should not fail");

    let result = pre_tool::evaluate(&input);
    let _expected_prefix = result
        .permission_decision
        .unwrap_or_else(|| "allow".to_string());
    let output = rustycode_guard::process_hook(&input_json, "pre-tool")
        .expect("process_hook should succeed");
    assert!(output.contains("permissionDecision"));

    let _: HookResult =
        serde_json::from_str(&output).expect("output should be valid HookResult JSON");
}
