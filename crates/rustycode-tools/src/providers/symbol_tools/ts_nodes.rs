use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TsNodesParams {
    pub language: String,
    pub filter: Option<String>,
}

fn get_language(name: &str) -> anyhow::Result<tree_sitter::Language> {
    match name.to_lowercase().as_str() {
        "rust" => Ok(tree_sitter_rust::LANGUAGE.into()),
        "python" => Ok(tree_sitter_python::LANGUAGE.into()),
        "javascript" | "js" => Ok(tree_sitter_javascript::LANGUAGE.into()),
        "typescript" | "ts" => Ok(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "go" => Ok(tree_sitter_go::LANGUAGE.into()),
        other => anyhow::bail!(
            "Unsupported language '{}'. Supported: rust, python, javascript, typescript, go",
            other
        ),
    }
}

define_tool! {
    pub struct TsNodesTool;

    name: "ts_nodes",
    namespace: "symbol",
    description: "List available tree-sitter node types and fields for a language. \
                  Use before writing ts_query calls to discover valid node names, field names, \
                  and named vs anonymous types.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: TsNodesParams, _ctx) {
        let language = get_language(&params.language)?;

        let kind_count = language.node_kind_count();
        let field_count = language.field_count();

        let filter_lower = params.filter.as_deref().map(|f| f.to_lowercase());

        let mut node_types: Vec<(String, bool)> = Vec::new();
        for id in 0u16..u16::try_from(kind_count).unwrap_or(u16::MAX) {
            if let Some(kind) = language.node_kind_for_id(id) {
                let named = language.node_kind_is_named(id);
                let visible = language.node_kind_is_visible(id);
                if visible {
                    if let Some(ref fl) = filter_lower {
                        if !kind.to_lowercase().contains(fl.as_str()) {
                            continue;
                        }
                    }
                    node_types.push((kind.to_string(), named));
                }
            }
        }

        let mut field_names: Vec<String> = Vec::new();
        for fid in 0u16..u16::try_from(field_count).unwrap_or(u16::MAX) {
            if let Some(fname) = language.field_name_for_id(fid) {
                if let Some(ref fl) = filter_lower {
                    if !fname.to_lowercase().contains(fl.as_str()) {
                        continue;
                    }
                }
                field_names.push(fname.to_string());
            }
        }

        let total_visible = node_types.len();
        let truncated = total_visible > 200;
        let shown = if truncated { &node_types[..200] } else { &node_types };

        let mut output = format!(
            "Tree-sitter nodes for {} ({} types, showing {}{}):\n\n",
            params.language,
            kind_count,
            if filter_lower.is_some() {
                format!("{} matching", total_visible)
            } else {
                format!("{}", total_visible)
            },
            if truncated { ", truncated to 200" } else { "" }
        );

        for (name, named) in shown {
            if *named {
                output.push_str(&format!("{} (named)\n", name));
            } else {
                output.push_str(&format!("{} [keyword/punct]\n", name));
            }
        }

        if truncated {
            output.push_str(&format!(
                "\n... and {} more types not shown\n",
                total_visible - 200
            ));
        }

        if !field_names.is_empty() {
            output.push_str(&format!("\nFields ({} total):\n", field_names.len()));
            for fname in &field_names {
                output.push_str(&format!("  {}\n", fname));
            }
        }

        output.push_str("\nUse these names in ts_query S-expressions, e.g.: (function_item name: (_) @fn-name)");

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_nodes_rust_returns_function_item() {
        let language = get_language("rust").unwrap();
        let kind_count = language.node_kind_count();
        assert!(kind_count > 0, "Rust language should have node types");

        let mut found_function = false;
        for id in 0u16..u16::try_from(kind_count).unwrap_or(u16::MAX) {
            if let Some(kind) = language.node_kind_for_id(id) {
                if kind == "function_item" {
                    found_function = true;
                    break;
                }
            }
        }
        assert!(
            found_function,
            "Rust grammar should have 'function_item' node type"
        );
    }

    #[test]
    fn ts_nodes_case_insensitive() {
        assert!(get_language("Rust").is_ok());
        assert!(get_language("RUST").is_ok());
        assert!(get_language("rust").is_ok());
        assert!(get_language("TypeScript").is_ok());
        assert!(get_language("ts").is_ok());
    }

    #[test]
    fn ts_nodes_unsupported_rejected() {
        assert!(get_language("brainfuck").is_err());
    }

    #[test]
    fn ts_nodes_fields_exist() {
        let language = get_language("rust").unwrap();
        let field_count = language.field_count();
        assert!(field_count > 0, "Rust language should have field names");

        let mut found_name = false;
        for fid in 0u16..u16::try_from(field_count).unwrap_or(u16::MAX) {
            if let Some(fname) = language.field_name_for_id(fid) {
                if fname == "name" {
                    found_name = true;
                    break;
                }
            }
        }
        assert!(found_name, "Rust grammar should have a 'name' field");
    }
}
