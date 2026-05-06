#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cloned_instead_of_copied,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::manual_string_new,
    clippy::match_same_arms,
    clippy::missing_const_for_fn,
    clippy::redundant_clone,
    clippy::similar_names,
    clippy::single_char_pattern,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unwrap_used
)]

//! Tests for Anthropic Agent Skills API support

use rustycode_llm::provider::{CompletionRequest, SkillRef};
use rustycode_llm::tools::{anthropic_skills_beta_headers, skills_tools, to_anthropic_tools};

#[test]
fn test_skill_ref_serialization() {
    let skill = SkillRef {
        skill_type: "anthropic".into(),
        skill_id: "pptx".into(),
        version: "latest".into(),
    };
    let json = serde_json::to_value(&skill).unwrap();
    assert_eq!(json["type"], "anthropic");
    assert_eq!(json["skill_id"], "pptx");
    assert_eq!(json["version"], "latest");
}

#[test]
fn test_completion_request_with_skills() {
    let request = CompletionRequest::new("claude-opus-4-7", vec![]).with_skills(vec![SkillRef {
        skill_type: "anthropic".into(),
        skill_id: "pptx".into(),
        version: "latest".into(),
    }]);

    let container = request.container.as_ref().unwrap();
    let skills = container["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0]["type"], "anthropic");
    assert_eq!(skills[0]["skill_id"], "pptx");
    assert_eq!(skills[0]["version"], "latest");
}

#[test]
fn test_completion_request_without_skills() {
    let request = CompletionRequest::new("claude-sonnet-4-6", vec![]);
    assert!(request.container.is_none());
}

#[test]
fn test_skills_tools() {
    let tools = skills_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "code_execution");
    assert!(tools[0].is_server_tool);
    assert_eq!(
        tools[0].anthropic_type.as_deref(),
        Some("code_execution_20250825")
    );
}

#[test]
fn test_code_execution_tool_anthropic_format() {
    let tools = skills_tools();
    let anthropic_tools = to_anthropic_tools(&tools);
    assert_eq!(anthropic_tools.len(), 1);
    assert_eq!(anthropic_tools[0]["type"], "code_execution_20250825");
    assert_eq!(anthropic_tools[0]["name"], "code_execution");
}

#[test]
fn test_skills_beta_headers() {
    let headers = anthropic_skills_beta_headers();
    assert_eq!(headers.len(), 2);
    assert!(headers.contains(&"code-execution-2025-08-25"));
    assert!(headers.contains(&"skills-2025-10-02"));
}

#[test]
fn test_container_omitted_from_serialization_when_none() {
    let request = CompletionRequest::new("claude-sonnet-4-6", vec![]);
    let json = serde_json::to_value(&request).unwrap();
    assert!(json.get("container").is_none());
}

#[test]
fn test_container_present_in_serialization_when_set() {
    let request = CompletionRequest::new("claude-opus-4-7", vec![]).with_skills(vec![SkillRef {
        skill_type: "anthropic".into(),
        skill_id: "xlsx".into(),
        version: "latest".into(),
    }]);
    let json = serde_json::to_value(&request).unwrap();
    assert!(json.get("container").is_some());
    assert!(json["container"]["skills"].is_array());
}

#[test]
fn test_multiple_skills_in_container() {
    let request = CompletionRequest::new("claude-opus-4-7", vec![]).with_skills(vec![
        SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "pptx".into(),
            version: "latest".into(),
        },
        SkillRef {
            skill_type: "anthropic".into(),
            skill_id: "xlsx".into(),
            version: "latest".into(),
        },
    ]);

    let container = request.container.as_ref().unwrap();
    let skills = container["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 2);
    assert_eq!(skills[0]["skill_id"], "pptx");
    assert_eq!(skills[1]["skill_id"], "xlsx");
}
