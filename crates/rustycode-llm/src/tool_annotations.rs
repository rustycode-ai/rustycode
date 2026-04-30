use serde_json::{json, Value};

const READ_ONLY_TOOL_NAMES: &[&str] = &[
    "read_file",
    "list_dir",
    "grep",
    "glob",
    "git_status",
    "git_diff",
    "git_log",
    "web_fetch",
    "memory_search",
    "memory_list",
    "skill_list",
    "doctor",
    "reasoning_research",
];

#[must_use]
pub fn read_only_annotation() -> Value {
    json!({ "readOnlyHint": true })
}

#[must_use]
pub fn is_read_only_tool_name(name: &str) -> bool {
    READ_ONLY_TOOL_NAMES.contains(&name)
}

#[must_use]
pub fn anthropic_annotations_for_tool_name(name: &str) -> Option<Value> {
    if is_read_only_tool_name(name) {
        Some(read_only_annotation())
    } else {
        None
    }
}

#[must_use]
pub fn anthropic_annotations_for_tool_info(name: &str, is_read_permission: bool) -> Option<Value> {
    if is_read_permission || is_read_only_tool_name(name) {
        Some(read_only_annotation())
    } else {
        None
    }
}
