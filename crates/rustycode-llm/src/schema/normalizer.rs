//! Per-format schema normalization profiles.

/// Describes what JSON Schema features a wire format supports.
#[derive(Debug, Clone, Copy)]
pub struct SchemaNormalizationProfile {
    pub supports_ref: bool,
    pub supports_defs: bool,
    pub supports_schema_keyword: bool,
    pub supports_default_values: bool,
    pub supports_enum: bool,
    pub supports_type_unions: bool,
    pub supports_additional_properties: bool,
    pub supports_min_max: bool,
    pub supports_pattern: bool,
    pub supports_format: bool,
    pub supports_examples: bool,
    pub requires_strict: bool,
}

/// Result of normalizing a schema for a specific format.
#[derive(Debug, Clone)]
pub struct NormalizedSchema {
    pub schema: serde_json::Value,
    pub warnings: Vec<String>,
    pub removed_features: Vec<&'static str>,
}

/// Wire format identifier for schema normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireFormat {
    Anthropic,
    OpenAIChat,
    OpenAIResponses,
    Gemini,
    Bedrock,
    Cohere,
    LiteRT,
}

/// Return the normalization profile for a wire format.
pub fn profile_for_format(format: WireFormat) -> SchemaNormalizationProfile {
    match format {
        WireFormat::Anthropic => SchemaNormalizationProfile {
            supports_ref: true,
            supports_defs: true,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: true,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::OpenAIChat => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::OpenAIResponses => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: true,
        },
        WireFormat::Gemini => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: false,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: false,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::Bedrock => SchemaNormalizationProfile {
            supports_ref: true,
            supports_defs: true,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: true,
            supports_additional_properties: true,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::Cohere => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: true,
            supports_enum: true,
            supports_type_unions: false,
            supports_additional_properties: false,
            supports_min_max: true,
            supports_pattern: true,
            supports_format: true,
            supports_examples: false,
            requires_strict: false,
        },
        WireFormat::LiteRT => SchemaNormalizationProfile {
            supports_ref: false,
            supports_defs: false,
            supports_schema_keyword: false,
            supports_default_values: false,
            supports_enum: false,
            supports_type_unions: false,
            supports_additional_properties: false,
            supports_min_max: false,
            supports_pattern: false,
            supports_format: false,
            supports_examples: false,
            requires_strict: false,
        },
    }
}

/// Normalize a JSON schema value for a specific wire format.
///
/// Removes unsupported features and logs warnings for anything stripped.
pub fn normalize_schema(schema: &serde_json::Value, format: WireFormat) -> NormalizedSchema {
    let profile = profile_for_format(format);
    let mut normalized = schema.clone();
    let mut warnings = Vec::new();
    let mut removed: Vec<&'static str> = Vec::new();

    // Remove $schema
    if !profile.supports_schema_keyword {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$schema").is_some() {
                removed.push("$schema");
            }
        }
    }

    // Remove $defs
    if !profile.supports_defs {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$defs").is_some() {
                removed.push("$defs");
                warnings.push("$defs not supported; consider expanding inline".into());
            }
        }
    }

    // Remove $ref
    if !profile.supports_ref {
        if let Some(obj) = normalized.as_object_mut() {
            if obj.remove("$ref").is_some() {
                removed.push("$ref");
                warnings.push("$ref not supported; consider expanding inline".into());
            }
        }
    }

    // Remove default: null (Gemini can't handle it)
    if !profile.supports_default_values {
        if let Some(obj) = normalized.as_object_mut() {
            if let Some(default) = obj.get("default") {
                if default.is_null() {
                    obj.remove("default");
                    removed.push("default:null");
                }
            }
        }
    }

    // Flatten type unions: ["string", "null"] → "string"
    if !profile.supports_type_unions {
        if let Some(obj) = normalized.as_object_mut() {
            if let Some(type_val) = obj.get_mut("type") {
                if let Some(arr) = type_val.as_array_mut() {
                    if !arr.is_empty() {
                        warnings.push(format!(
                            "type union {:?} flattened to first variant",
                            arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()
                        ));
                        *type_val = arr[0].clone();
                        removed.push("type_union");
                    }
                }
            }
        }
    }

    // Add strict: true for OpenAI Responses
    if profile.requires_strict {
        if let Some(obj) = normalized.as_object_mut() {
            obj.insert("strict".into(), serde_json::Value::Bool(true));
        }
    }

    // Recurse into properties
    if let Some(obj) = normalized.as_object_mut() {
        if let Some(properties) = obj.get_mut("properties") {
            if let Some(props) = properties.as_object_mut() {
                for (_key, value) in props.iter_mut() {
                    let sub = normalize_schema(value, format);
                    warnings.extend(sub.warnings);
                    // Don't add duplicate removed features
                    for feat in sub.removed_features {
                        if !removed.contains(&feat) {
                            removed.push(feat);
                        }
                    }
                    *value = sub.schema;
                }
            }
        }
        // Recurse into items
        if let Some(items) = obj.get_mut("items") {
            let sub = normalize_schema(items, format);
            warnings.extend(sub.warnings);
            for feat in sub.removed_features {
                if !removed.contains(&feat) {
                    removed.push(feat);
                }
            }
            *items = sub.schema;
        }
        // Recurse into anyOf
        if let Some(any_of) = obj.get_mut("anyOf") {
            if let Some(arr) = any_of.as_array_mut() {
                for item in arr.iter_mut() {
                    let sub = normalize_schema(item, format);
                    warnings.extend(sub.warnings);
                    for feat in sub.removed_features {
                        if !removed.contains(&feat) {
                            removed.push(feat);
                        }
                    }
                    *item = sub.schema;
                }
            }
        }
    }

    NormalizedSchema {
        schema: normalized,
        warnings,
        removed_features: removed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gemini_strips_schema_and_defs() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": { "Error": { "type": "string" } },
            "type": "object",
            "properties": {}
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.schema.get("$schema").is_none());
        assert!(result.schema.get("$defs").is_none());
        assert!(result.removed_features.contains(&"$schema"));
        assert!(result.removed_features.contains(&"$defs"));
    }

    #[test]
    fn gemini_flattens_type_unions() {
        let schema = json!({
            "type": ["string", "null"]
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert_eq!(result.schema["type"], "string");
        assert!(result.removed_features.contains(&"type_union"));
    }

    #[test]
    fn gemini_removes_default_null() {
        let schema = json!({
            "type": "string",
            "default": null
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.schema.get("default").is_none());
        assert!(result.removed_features.contains(&"default:null"));
    }

    #[test]
    fn openai_responses_adds_strict() {
        let schema = json!({
            "type": "object",
            "properties": {}
        });
        let result = normalize_schema(&schema, WireFormat::OpenAIResponses);
        assert_eq!(result.schema["strict"], true);
    }

    #[test]
    fn anthropic_preserves_ref() {
        let schema = json!({
            "$ref": "#/definitions/Error",
            "type": "object"
        });
        let result = normalize_schema(&schema, WireFormat::Anthropic);
        assert!(result.schema.get("$ref").is_some());
        assert!(!result.removed_features.contains(&"$ref"));
    }

    #[test]
    fn openai_chat_strips_ref() {
        let schema = json!({
            "$ref": "#/definitions/Error",
            "type": "object"
        });
        let result = normalize_schema(&schema, WireFormat::OpenAIChat);
        assert!(result.schema.get("$ref").is_none());
        assert!(result.removed_features.contains(&"$ref"));
    }

    #[test]
    fn normalization_is_recursive() {
        let schema = json!({
            "type": "object",
            "properties": {
                "nested": {
                    "type": "object",
                    "properties": {
                        "deep": {
                            "$schema": "http://json-schema.org/draft-07/schema#",
                            "type": "string"
                        }
                    }
                }
            }
        });
        let result = normalize_schema(&schema, WireFormat::Gemini);
        assert!(result.removed_features.contains(&"$schema"));
        // The deeply nested $schema should be removed too
        let deep = &result.schema["properties"]["nested"]["properties"]["deep"];
        assert!(deep.get("$schema").is_none());
    }
}
