use rustycode_protocol::frontmatter::{FrontmatterMap, FrontmatterValue};

/// Extract a string value from frontmatter map if present
pub fn extract_string(map: &FrontmatterMap, key: &str) -> Option<String> {
    map.get(key).and_then(|v| {
        if let FrontmatterValue::String(s) = v {
            Some(s.clone())
        } else {
            None
        }
    })
}

/// Extract an array of strings from frontmatter map.
/// If the value is a single string, treats it as a single-element array.
pub fn extract_string_array(map: &FrontmatterMap, key: &str) -> Vec<String> {
    match map.get(key) {
        Some(FrontmatterValue::Array(arr)) => arr
            .iter()
            .filter_map(|fv| {
                if let FrontmatterValue::String(s) = fv {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect(),
        Some(FrontmatterValue::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Extract a boolean value from frontmatter map with a default
pub fn extract_bool(map: &FrontmatterMap, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| {
            if let FrontmatterValue::Bool(b) = v {
                Some(*b)
            } else {
                None
            }
        })
        .unwrap_or(default)
}

/// Parsed frontmatter fields for the new skill definition model.
#[derive(Debug, Clone)]
pub struct ParsedFrontmatter {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    pub version: String,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub arguments: Vec<String>,
    pub effort: Option<String>,
    pub model_override: Option<String>,
    pub model_invocable: bool,
    pub user_invocable: bool,
    pub paths: Vec<String>,
    pub context: Option<String>,
    pub agent: Option<String>,
    pub categories: Vec<String>,
    pub excludes: Vec<String>,
    pub gotchas: Vec<String>,
}

pub fn parse_frontmatter_fields(
    fm: &FrontmatterMap,
    fallback_description: &str,
) -> ParsedFrontmatter {
    ParsedFrontmatter {
        name: extract_string(fm, "name").unwrap_or_default(),
        description: extract_string(fm, "description")
            .unwrap_or_else(|| fallback_description.to_string()),
        when_to_use: extract_string(fm, "when-to-use").or_else(|| extract_string(fm, "whenToUse")),
        version: extract_string(fm, "version").unwrap_or_default(),
        allowed_tools: extract_string_array(fm, "allowed-tools")
            .into_iter()
            .chain(extract_string_array(fm, "allowedTools"))
            .collect(),
        argument_hint: extract_string(fm, "argument-hint")
            .or_else(|| extract_string(fm, "argumentHint")),
        arguments: extract_string_array(fm, "arguments"),
        effort: extract_string(fm, "effort"),
        model_override: extract_string(fm, "model")
            .or_else(|| extract_string(fm, "model-override")),
        model_invocable: extract_bool(fm, "model-invocable", true)
            && extract_bool(fm, "modelInvocable", true),
        user_invocable: extract_bool(fm, "user-invocable", true)
            && extract_bool(fm, "userInvocable", true),
        paths: rustycode_protocol::frontmatter::normalize_paths(&extract_string_array(fm, "paths")),
        context: extract_string(fm, "context"),
        agent: extract_string(fm, "agent"),
        categories: extract_string_array(fm, "categories"),
        excludes: extract_string_array(fm, "excludes"),
        gotchas: extract_string_array(fm, "gotchas"),
    }
}

pub mod __private_dummy {}
