//! Skill adapter: converts `SkillDefinition` to provider-specific tool schemas.
//!
//! Supported providers:
//! - Anthropic (Claude API `tool_use` format)
//! - `OpenAI` (function-calling format)
//! - AWS Bedrock (Converse API toolSpec format)
//!
//! # Tool name namespacing
//!
//! MCP-sourced skills get an `mcp__{server}__{tool}` prefix so the provider
//! and the model can distinguish them from built-in tools. Other sources get
//! shorter prefixes (`skill__`, `user__`, `plugin__`).
//!
//! All tool names are enforced to stay within the provider's character limit
//! (64 for Anthropic). Names that exceed the limit are truncated with a hash
//! suffix to preserve uniqueness.

use crate::types::{ProcedureKind, SkillDefinition, SkillSource};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

const MAX_DESCRIPTION_LEN: usize = 1024;

/// Anthropic enforces a 64-character limit on tool names.
pub const MAX_TOOL_NAME_LEN: usize = 64;

// ── Name registry ──────────────────────────────────────────────────────────

/// Bidirectional name registry for mapping between short API names and
/// original skill IDs. Needed because long names get hashed.
static NAME_REGISTRY: Mutex<Option<NameRegistry>> = Mutex::new(None);

#[derive(Default)]
struct NameRegistry {
    /// `api_name` → original skill id
    api_to_original: HashMap<String, String>,
    /// original skill id → `api_name`
    original_to_api: HashMap<String, String>,
}

impl NameRegistry {
    fn register(&mut self, api_name: String, original_id: String) {
        self.api_to_original
            .insert(api_name.clone(), original_id.clone());
        self.original_to_api.insert(original_id, api_name);
    }
}

#[allow(clippy::significant_drop_tightening)]
fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut NameRegistry) -> R,
{
    let mut guard = NAME_REGISTRY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let reg = guard.get_or_insert_with(NameRegistry::default);
    f(reg)
}

// ── Public name API ────────────────────────────────────────────────────────

/// Create a namespaced tool name for the given skill.
///
/// Prefixes by source:
/// - `Mcp`          → `mcp__{server}__{tool}` (expects id in `server__tool` form)
/// - `Bundled`/`Managed` → `skill__{id}`
/// - `User`/`Project`    → `user__{id}`
/// - `Plugin`/`Dynamic`  → `plugin__{id}` / `dyn__{id}`
///
/// Names exceeding `MAX_TOOL_NAME_LEN` are truncated with a hash suffix.
pub fn namespaced_name(skill: &SkillDefinition) -> String {
    let raw = match skill.source {
        SkillSource::Mcp => format!("mcp__{}", skill.id),
        SkillSource::Bundled | SkillSource::Managed => format!("skill__{}", skill.id),
        SkillSource::User | SkillSource::Project => format!("user__{}", skill.id),
        SkillSource::Plugin => format!("plugin__{}", skill.id),
        SkillSource::Dynamic => format!("dyn__{}", skill.id),
    };

    let api_name = enforce_name_length(&raw);
    with_registry(|reg| reg.register(api_name.clone(), skill.id.clone()));
    api_name
}

/// Reverse a namespaced API name back to the original skill ID.
///
/// Returns `None` if the name was never registered (e.g., not a skill tool).
pub fn original_id(api_name: &str) -> Option<String> {
    with_registry(|reg| reg.api_to_original.get(api_name).cloned())
}

/// Check whether a tool name came from the skill adapter (vs a built-in tool).
pub fn is_skill_tool(api_name: &str) -> bool {
    api_name.starts_with("mcp__")
        || api_name.starts_with("skill__")
        || api_name.starts_with("user__")
        || api_name.starts_with("plugin__")
        || api_name.starts_with("dyn__")
}

/// Check whether a tool name originated from an MCP server.
pub fn is_mcp_tool(api_name: &str) -> bool {
    api_name.starts_with("mcp__")
}

/// Enforce the provider tool name length limit.
///
/// Names under the limit are returned as-is. Names exceeding it are truncated
/// with a hash suffix for uniqueness. The result is registered so `original_id()`
/// can reverse it.
pub fn sanitize_name(name: &str) -> String {
    let api_name = enforce_name_length(name);
    with_registry(|reg| reg.register(api_name.clone(), name.to_string()));
    api_name
}

// ── Internal ───────────────────────────────────────────────────────────────

fn enforce_name_length(name: &str) -> String {
    if name.len() <= MAX_TOOL_NAME_LEN {
        return name.to_string();
    }

    let hash = hash_name(name);
    let max_prefix = MAX_TOOL_NAME_LEN - 10;
    let mut end = max_prefix;
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}__{}", &name[..end], hash)
}

fn hash_name(name: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    let full = format!("{:016x}", hasher.finish());
    full[full.len() - 8..].to_string()
}

// ── Provider conversion (SkillDefinition) ──────────────────────────────────

/// Adapt a `SkillDefinition` into Anthropic `tool_use` JSON.
pub fn to_anthropic(skill: &SkillDefinition) -> Value {
    let (description, input_schema) = build_parts(skill);
    json!({
        "name": namespaced_name(skill),
        "description": description,
        "input_schema": input_schema,
    })
}

/// Adapt a `SkillDefinition` into `OpenAI` function-calling JSON.
pub fn to_openai(skill: &SkillDefinition) -> Value {
    let (description, input_schema) = build_parts(skill);
    json!({
        "type": "function",
        "function": {
            "name": namespaced_name(skill),
            "description": description,
            "parameters": input_schema,
        }
    })
}

/// Adapt a `SkillDefinition` into AWS Bedrock toolSpec JSON.
pub fn to_bedrock(skill: &SkillDefinition) -> Value {
    let (description, input_schema) = build_parts(skill);
    json!({
        "toolSpec": {
            "name": namespaced_name(skill),
            "description": description,
            "inputSchema": {
                "json": input_schema,
            }
        }
    })
}

// ── Provider conversion (raw — for plugins, MCP, etc.) ─────────────────────

/// Convert raw tool info into Anthropic `tool_use` JSON.
///
/// Use this for plugin tools, MCP tools, or any tool that doesn't come from a
/// `SkillDefinition`. The caller is responsible for namespacing the name
/// (e.g., `format!("plugin__{plugin}__{tool}")`) and calling `sanitize_name()`
/// if the name might exceed the 64-char limit.
pub fn to_anthropic_raw(name: &str, description: &str, input_schema: &Value) -> Value {
    let desc = truncate_description(description);
    json!({
        "name": name,
        "description": desc,
        "input_schema": input_schema,
    })
}

/// Convert raw tool info into `OpenAI` function-calling JSON.
pub fn to_openai_raw(name: &str, description: &str, input_schema: &Value) -> Value {
    let desc = truncate_description(description);
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": desc,
            "parameters": input_schema,
        }
    })
}

/// Convert raw tool info into AWS Bedrock toolSpec JSON.
pub fn to_bedrock_raw(name: &str, description: &str, input_schema: &Value) -> Value {
    let desc = truncate_description(description);
    json!({
        "toolSpec": {
            "name": name,
            "description": desc,
            "inputSchema": {
                "json": input_schema,
            }
        }
    })
}

fn truncate_description(desc: &str) -> String {
    if desc.len() <= MAX_DESCRIPTION_LEN {
        return desc.to_string();
    }
    let mut end = MAX_DESCRIPTION_LEN;
    while !desc.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &desc[..end])
}

fn build_parts(skill: &SkillDefinition) -> (String, Value) {
    (build_description(skill), build_input_schema(skill))
}

fn build_description(skill: &SkillDefinition) -> String {
    let mut parts = Vec::new();

    parts.push(skill.description.clone());

    if !skill.when_to_use.is_empty() {
        parts.push(format!("When to use: {}", skill.when_to_use));
    }

    if !skill.allowed_tools.is_empty() {
        parts.push(format!("Tools: {}", skill.allowed_tools.join(", ")));
    }

    let desc = parts.join("\n\n");

    if desc.len() <= MAX_DESCRIPTION_LEN {
        return desc;
    }

    let mut end = MAX_DESCRIPTION_LEN;
    while !desc.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &desc[..end])
}

fn build_input_schema(skill: &SkillDefinition) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<String> = Vec::new();

    if let Some(ref hint) = skill.argument_hint {
        properties.insert(
            "args".to_string(),
            json!({ "type": "string", "description": hint }),
        );
        required.push("args".to_string());
    } else {
        properties.insert(
            "args".to_string(),
            json!({ "type": "string", "description": "Optional arguments for the skill" }),
        );
    }

    if let Some(ProcedureKind::Pipeline(ref pipeline)) = skill.procedure {
        if !pipeline.stages.is_empty() {
            let stages: Vec<Value> = pipeline.stages.iter().map(|s| json!(s.name)).collect();
            properties.insert(
                "stage".to_string(),
                json!({
                    "type": "string",
                    "description": "Pipeline stage to execute",
                    "enum": stages,
                }),
            );
        }
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::path::PathBuf;

    fn test_skill() -> SkillDefinition {
        SkillDefinition {
            id: "tdd-guide".to_string(),
            name: "TDD Guide".to_string(),
            description: "Enforces test-driven development workflow.".to_string(),
            when_to_use: "When writing new features or fixing bugs.".to_string(),
            source: SkillSource::Bundled,
            version: "1.0".to_string(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::High,
            context: ExecutionContext::Inline,
            procedure: None,
            allowed_tools: vec!["bash".to_string(), "read_file".to_string()],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: Some("feature description or bug description".to_string()),
            categories: vec!["testing".to_string()],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::from("/skills/tdd/SKILL.md"),
            content: None,
        }
    }

    fn mcp_skill() -> SkillDefinition {
        SkillDefinition {
            id: "claude_ai_Zapier__google_sheets_lookup_spreadsheet_rows_advanced".to_string(),
            name: "Google Sheets Lookup".to_string(),
            description: "Look up spreadsheet rows.".to_string(),
            when_to_use: String::new(),
            source: SkillSource::Mcp,
            version: String::new(),
            activation: ActivationSpec::always(),
            effort: SkillEffortLevel::Medium,
            context: ExecutionContext::Inline,
            procedure: None,
            allowed_tools: vec![],
            user_invocable: true,
            model_invocable: true,
            agent: None,
            model_override: None,
            argument_hint: None,
            categories: vec![],
            excludes: vec![],
            gotchas: vec![],
            quality: SkillQuality::default(),
            lifecycle_state: LifecycleState::Active,
            content_path: PathBuf::from("/mcp/zapier/SKILL.md"),
            content: None,
        }
    }

    fn pipeline_skill() -> SkillDefinition {
        let mut skill = test_skill();
        skill.id = "code-review".to_string();
        skill.name = "Code Review".to_string();
        skill.procedure = Some(ProcedureKind::Pipeline(Pipeline {
            stages: vec![
                PipelineStage {
                    name: "Analyze".to_string(),
                    description: "Analyze changed files".to_string(),
                    required_tools: vec!["read_file".to_string()],
                    parallel: false,
                },
                PipelineStage {
                    name: "Report".to_string(),
                    description: "Generate review report".to_string(),
                    required_tools: vec![],
                    parallel: false,
                },
            ],
        }));
        skill
    }

    // -- Provider format tests --

    #[test]
    fn anthropic_format_has_required_fields() {
        let tool = to_anthropic(&test_skill());
        assert_eq!(tool["name"], "skill__tdd-guide");
        assert!(tool["description"].is_string());
        assert_eq!(tool["input_schema"]["type"], "object");
    }

    #[test]
    fn anthropic_includes_when_to_use() {
        let tool = to_anthropic(&test_skill());
        let desc = tool["description"].as_str().unwrap();
        assert!(desc.contains("When to use:"));
    }

    #[test]
    fn anthropic_includes_allowed_tools() {
        let tool = to_anthropic(&test_skill());
        let desc = tool["description"].as_str().unwrap();
        assert!(desc.contains("bash"));
        assert!(desc.contains("read_file"));
    }

    #[test]
    fn openai_wraps_in_function() {
        let tool = to_openai(&test_skill());
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "skill__tdd-guide");
        assert_eq!(tool["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn bedrock_wraps_in_tool_spec() {
        let tool = to_bedrock(&test_skill());
        assert_eq!(tool["toolSpec"]["name"], "skill__tdd-guide");
        assert_eq!(tool["toolSpec"]["inputSchema"]["json"]["type"], "object");
    }

    // -- Schema tests --

    #[test]
    fn argument_hint_makes_args_required() {
        let tool = to_anthropic(&test_skill());
        let schema = &tool["input_schema"];
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("args")));
        assert_eq!(
            schema["properties"]["args"]["description"],
            "feature description or bug description"
        );
    }

    #[test]
    fn no_argument_hint_makes_args_optional() {
        let mut skill = test_skill();
        skill.argument_hint = None;
        let tool = to_anthropic(&skill);
        let required = tool["input_schema"]["required"].as_array().unwrap();
        assert!(!required.contains(&json!("args")));
    }

    #[test]
    fn pipeline_adds_stage_enum() {
        let tool = to_anthropic(&pipeline_skill());
        let stage = &tool["input_schema"]["properties"]["stage"];
        assert_eq!(stage["type"], "string");
        let enum_vals = stage["enum"].as_array().unwrap();
        assert_eq!(enum_vals.len(), 2);
        assert!(enum_vals.contains(&json!("Analyze")));
        assert!(enum_vals.contains(&json!("Report")));
    }

    #[test]
    fn instruction_procedure_no_stage_enum() {
        let mut skill = test_skill();
        skill.procedure = Some(ProcedureKind::Instruction);
        let tool = to_anthropic(&skill);
        assert!(tool["input_schema"]["properties"].get("stage").is_none());
    }

    // -- Description tests --

    #[test]
    fn description_truncation() {
        let mut skill = test_skill();
        skill.description = "x".repeat(2000);
        let tool = to_anthropic(&skill);
        let desc = tool["description"].as_str().unwrap();
        assert!(desc.len() <= MAX_DESCRIPTION_LEN + 3);
        assert!(desc.ends_with("..."));
    }

    #[test]
    fn empty_allowed_tools_omits_from_description() {
        let mut skill = test_skill();
        skill.allowed_tools = vec![];
        let tool = to_anthropic(&skill);
        let desc = tool["description"].as_str().unwrap();
        assert!(!desc.contains("Tools:"));
    }

    #[test]
    fn empty_when_to_use_omits_from_description() {
        let mut skill = test_skill();
        skill.when_to_use = String::new();
        let tool = to_anthropic(&skill);
        let desc = tool["description"].as_str().unwrap();
        assert!(!desc.contains("When to use:"));
    }

    // -- Cross-format consistency --

    #[test]
    fn all_three_formats_share_same_schema() {
        let skill = test_skill();
        let a = to_anthropic(&skill);
        let o = to_openai(&skill);
        let b = to_bedrock(&skill);

        let a_schema = &a["input_schema"];
        let o_schema = &o["function"]["parameters"];
        let b_schema = &b["toolSpec"]["inputSchema"]["json"];

        assert_eq!(a_schema, o_schema);
        assert_eq!(a_schema, b_schema);
    }

    #[test]
    fn batch_conversion_via_iterator() {
        let skills = [test_skill(), pipeline_skill()];
        let tools: Vec<Value> = skills.iter().map(to_anthropic).collect();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "skill__tdd-guide");
        assert_eq!(tools[1]["name"], "skill__code-review");
    }

    // -- Namespace tests --

    #[test]
    fn mcp_skill_gets_mcp_prefix() {
        let name = namespaced_name(&mcp_skill());
        assert!(name.starts_with("mcp__"));
        assert!(name.contains("claude_ai_Zapier"));
    }

    #[test]
    fn bundled_skill_gets_skill_prefix() {
        let name = namespaced_name(&test_skill());
        assert_eq!(name, "skill__tdd-guide");
    }

    #[test]
    fn user_skill_gets_user_prefix() {
        let mut skill = test_skill();
        skill.source = SkillSource::User;
        let name = namespaced_name(&skill);
        assert_eq!(name, "user__tdd-guide");
    }

    #[test]
    fn plugin_skill_gets_plugin_prefix() {
        let mut skill = test_skill();
        skill.source = SkillSource::Plugin;
        let name = namespaced_name(&skill);
        assert_eq!(name, "plugin__tdd-guide");
    }

    #[test]
    fn dynamic_skill_gets_dyn_prefix() {
        let mut skill = test_skill();
        skill.source = SkillSource::Dynamic;
        let name = namespaced_name(&skill);
        assert_eq!(name, "dyn__tdd-guide");
    }

    // -- 64-char limit enforcement --

    #[test]
    fn long_mcp_name_enforces_64_char_limit() {
        let name = namespaced_name(&mcp_skill());
        assert!(
            name.len() <= MAX_TOOL_NAME_LEN,
            "name is {} chars: {name}",
            name.len()
        );
    }

    #[test]
    fn truncated_name_preserves_hash_suffix() {
        let name = namespaced_name(&mcp_skill());
        if name.len() == MAX_TOOL_NAME_LEN {
            // Last 8 chars should be the hash, preceded by "__"
            let tail = &name[name.len() - 10..];
            assert!(tail.starts_with("__"));
        }
    }

    #[test]
    fn short_names_are_not_modified() {
        let name = namespaced_name(&test_skill());
        assert_eq!(name, "skill__tdd-guide");
        assert!(name.len() <= MAX_TOOL_NAME_LEN);
    }

    // -- Name registry --

    #[test]
    fn registry_reverses_api_name_to_original() {
        let _ = namespaced_name(&test_skill());
        let orig = original_id("skill__tdd-guide").unwrap();
        assert_eq!(orig, "tdd-guide");
    }

    #[test]
    fn registry_reverses_truncated_name() {
        let api_name = namespaced_name(&mcp_skill());
        let orig = original_id(&api_name).unwrap();
        assert_eq!(
            orig,
            "claude_ai_Zapier__google_sheets_lookup_spreadsheet_rows_advanced"
        );
    }

    #[test]
    fn registry_returns_none_for_unknown() {
        assert!(original_id("nonexistent_tool").is_none());
    }

    // -- is_skill_tool / is_mcp_tool --

    #[test]
    fn identifies_skill_tools() {
        assert!(is_skill_tool("skill__tdd-guide"));
        assert!(is_skill_tool("user__my-skill"));
        assert!(is_skill_tool("plugin__my-plugin"));
        assert!(is_skill_tool("dyn__my-dynamic"));
        assert!(is_skill_tool("mcp__server__tool"));
        assert!(!is_skill_tool("bash"));
        assert!(!is_skill_tool("read_file"));
    }

    #[test]
    fn identifies_mcp_tools() {
        assert!(is_mcp_tool("mcp__server__tool"));
        assert!(!is_mcp_tool("skill__tdd-guide"));
        assert!(!is_mcp_tool("bash"));
    }

    // -- sanitize_name --

    #[test]
    fn sanitize_short_name_unchanged() {
        assert_eq!(sanitize_name("bash"), "bash");
    }

    #[test]
    fn sanitize_long_name_truncated() {
        let long = format!("plugin__my_plugin__{}", "x".repeat(80));
        let sanitized = sanitize_name(&long);
        assert!(sanitized.len() <= MAX_TOOL_NAME_LEN, "{}", sanitized.len());
    }

    #[test]
    fn sanitize_registers_for_reverse_lookup() {
        let long = format!("plugin__my_plugin__{}", "x".repeat(80));
        let sanitized = sanitize_name(&long);
        assert_eq!(original_id(&sanitized).unwrap(), long);
    }

    // -- Raw conversion (plugin bridge path) --

    #[test]
    fn raw_anthropic_format() {
        let input = json!({ "type": "object", "properties": { "path": { "type": "string" } } });
        let tool = to_anthropic_raw(
            "plugin__reviewer__check_code",
            "Run code review checks",
            &input,
        );
        assert_eq!(tool["name"], "plugin__reviewer__check_code");
        assert_eq!(tool["description"], "Run code review checks");
        assert_eq!(tool["input_schema"]["type"], "object");
    }

    #[test]
    fn raw_openai_format() {
        let input = json!({ "type": "object" });
        let tool = to_openai_raw(
            "plugin__reviewer__check_code",
            "Run code review checks",
            &input,
        );
        assert_eq!(tool["type"], "function");
        assert_eq!(tool["function"]["name"], "plugin__reviewer__check_code");
    }

    #[test]
    fn raw_bedrock_format() {
        let input = json!({ "type": "object" });
        let tool = to_bedrock_raw(
            "plugin__reviewer__check_code",
            "Run code review checks",
            &input,
        );
        assert_eq!(tool["toolSpec"]["name"], "plugin__reviewer__check_code");
    }

    #[test]
    fn raw_truncates_long_description() {
        let long_desc = "a".repeat(2000);
        let input = json!({});
        let tool = to_anthropic_raw("tool", &long_desc, &input);
        let desc = tool["description"].as_str().unwrap();
        assert!(desc.len() <= MAX_DESCRIPTION_LEN + 3);
        assert!(desc.ends_with("..."));
    }

    #[test]
    fn plugin_bridge_end_to_end() {
        // Simulate what a plugin bridge would do:
        // 1. Get tools from plugin
        // 2. Namespace: plugin__{plugin_name}__{tool_name}
        // 3. Sanitize name
        // 4. Convert to provider format
        let plugin_name = "code-reviewer";
        let tool_name = "check_security";
        let full_name = format!("plugin__{plugin_name}__{tool_name}");
        let api_name = sanitize_name(&full_name);

        let input = json!({ "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] });
        let tool = to_anthropic_raw(&api_name, "Run security checks on code", &input);

        assert_eq!(tool["name"], "plugin__code-reviewer__check_security");
        assert_eq!(original_id(&api_name).unwrap(), full_name);
    }
}
