use serde_json::{json, Value};

use crate::tool_names as tn;

const READ_ONLY_TOOL_NAMES: &[&str] = &[
    tn::READ,
    tn::LIST_DIR,
    tn::GREP,
    tn::GLOB,
    tn::GIT_STATUS,
    tn::GIT_DIFF,
    tn::GIT_LOG,
    tn::WEB_FETCH,
    tn::MEMORY_SEARCH,
    tn::MEMORY_LIST,
    tn::SKILL_LIST,
    tn::DOCTOR,
    tn::REASONING_RESEARCH,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tools_get_annotation() {
        assert!(anthropic_annotations_for_tool_name(tn::READ).is_some());
        assert!(anthropic_annotations_for_tool_name(tn::GREP).is_some());
        assert!(anthropic_annotations_for_tool_name(tn::GLOB).is_some());
    }

    #[test]
    fn write_tools_no_annotation() {
        assert!(anthropic_annotations_for_tool_name(tn::WRITE).is_none());
        assert!(anthropic_annotations_for_tool_name(tn::EDIT).is_none());
        assert!(anthropic_annotations_for_tool_name(tn::BASH).is_none());
    }

    #[test]
    fn permission_overrides_name() {
        // Even if name isn't in the list, read permission grants annotation
        assert!(anthropic_annotations_for_tool_info("custom_tool", true).is_some());
        assert!(anthropic_annotations_for_tool_info("custom_tool", false).is_none());
    }
}
