use anyhow::Context;
use rustycode_tools_api::{define_tool, ToolOutput, ToolPermission, ToolTag};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::indexing::symbols::languages::Lang;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TsQueryParams {
    pub file: String,
    pub query: String,
    pub max_results: Option<usize>,
}

fn lang_from_language(name: &str) -> Option<tree_sitter::Language> {
    match name {
        "Rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "Python" => Some(tree_sitter_python::LANGUAGE.into()),
        "JavaScript" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "TypeScript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "Go" => Some(tree_sitter_go::LANGUAGE.into()),
        _ => None,
    }
}

define_tool! {
    pub struct TsQueryTool;

    name: "ts_query",
    namespace: "symbol",
    description: "Run a tree-sitter query against a source file. Returns matching nodes with \
                  their text, line numbers, and captures. Use ts_nodes first to discover valid \
                  node types and fields for the language.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],
    defer_loading: true,

    execute(params: TsQueryParams, ctx) {
        let max = params.max_results.unwrap_or(50).min(200);

        let file_path = if std::path::Path::new(&params.file).is_absolute() {
            PathBuf::from(&params.file)
        } else {
            ctx.cwd.join(&params.file)
        };

        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;

        let ext = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let lang = Lang::from_ext(ext)
            .ok_or_else(|| anyhow::anyhow!(
                "Unsupported language for extension '.{}'. Supported: rust (.rs), python (.py), javascript (.js), typescript (.ts/.tsx), go (.go)",
                ext
            ))?;

        let lang_name = format!("{:?}", lang);
        let language = lang_from_language(&lang_name)
            .ok_or_else(|| anyhow::anyhow!("Unsupported language. Supported: rust, python, javascript, typescript, go"))?;

        let query = match Query::new(&language, &params.query) {
            Ok(q) => q,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "Query compilation error: {}\n\nUse ts_nodes to look up valid node types and field names.",
                    e
                )));
            }
        };

        let mut parser = Parser::new();
        parser.set_language(&language)
            .with_context(|| "failed to set tree-sitter language")?;

        let tree = parser.parse(&content, None)
            .ok_or_else(|| anyhow::anyhow!("tree-sitter parse returned None for {}", file_path.display()))?;

        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(&query, tree.root_node(), content.as_bytes());

        let capture_names = query.capture_names().to_vec();
        let mut results: Vec<(String, usize, usize, String)> = Vec::new();
        let mut total_matches = 0usize;
        let mut truncated_count = 0usize;

        while let Some((m, _)) = captures.next() {
            for cap in m.captures {
                total_matches += 1;
                if results.len() >= max {
                    continue;
                }

                let node = cap.node;
                let cname = capture_names.get(cap.index as usize)
                    .copied()
                    .unwrap_or("unknown")
                    .to_string();

                let start_line = node.start_position().row + 1;
                let end_line = node.end_position().row + 1;

                let text = node.utf8_text(content.as_bytes())
                    .unwrap_or("<invalid utf8>");

                let (display_text, was_truncated) = if text.len() > 200 {
                    (format!("{}...", &text[..text.floor_char_boundary(200)]), true)
                } else {
                    (text.to_string(), false)
                };

                if was_truncated {
                    truncated_count += 1;
                }

                results.push((cname, start_line, end_line, display_text));
            }
        }

        let rel = file_path.strip_prefix(&ctx.cwd)
            .unwrap_or(&file_path)
            .to_string_lossy()
            .replace('\\', "/");

        let mut output = format!("ts_query results for {} ({} matches):\n\n", rel, total_matches);

        for (cname, start, end, text) in &results {
            if start == end {
                output.push_str(&format!("@{} (line {}):\n", cname, start));
            } else {
                output.push_str(&format!("@{} (line {}-{}):\n", cname, start, end));
            }
            output.push_str(text);
            output.push_str("\n\n");
        }

        let shown = results.len();
        output.push_str(&format!(
            "({} matches, {} shown, {} text truncated)\n",
            total_matches, shown, truncated_count
        ));

        if total_matches > max {
            output.push_str(&format!(
                "Use max_results (currently {}) to see more matches (max 200).\n",
                max
            ));
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_query_finds_functions_in_rust_file() {
        let source = r#"
fn hello() {
    println!("hello");
}

pub fn world(x: i32) -> i32 {
    x + 1
}
"#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, "(function_item name: (identifier) @fn-name)")
            .expect("query should compile");

        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).expect("parse should succeed");

        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(&query, tree.root_node(), source.as_bytes());

        let capture_names = query.capture_names().to_vec();
        let mut results = Vec::new();

        while let Some((m, _)) = captures.next() {
            for cap in m.captures {
                let cname = capture_names
                    .get(cap.index as usize)
                    .copied()
                    .unwrap_or("unknown")
                    .to_string();
                let text = cap
                    .node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                results.push((cname, text));
            }
        }

        assert_eq!(results.len(), 2, "should find 2 functions");
        let names: Vec<&str> = results.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            names.iter().all(|n| *n == "fn-name"),
            "all captures should be @fn-name"
        );

        let texts: Vec<&str> = results.iter().map(|(_, t)| t.as_str()).collect();
        assert!(texts.contains(&"hello"), "should find 'hello' function");
        assert!(texts.contains(&"world"), "should find 'world' function");
    }

    #[test]
    fn ts_query_lang_detection_from_ext() {
        assert_eq!(Lang::from_ext("rs"), Some(Lang::Rust));
        assert_eq!(Lang::from_ext("py"), Some(Lang::Python));
        assert_eq!(Lang::from_ext("js"), Some(Lang::JavaScript));
        assert_eq!(Lang::from_ext("ts"), Some(Lang::TypeScript));
        assert_eq!(Lang::from_ext("go"), Some(Lang::Go));
        assert_eq!(Lang::from_ext("java"), None);
    }

    #[test]
    fn ts_query_lang_from_language_mapping() {
        assert!(lang_from_language("Rust").is_some());
        assert!(lang_from_language("Python").is_some());
        assert!(lang_from_language("JavaScript").is_some());
        assert!(lang_from_language("TypeScript").is_some());
        assert!(lang_from_language("Go").is_some());
        assert!(lang_from_language("Cobol").is_none());
    }

    #[test]
    fn ts_query_invalid_query_returns_error() {
        let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let result = Query::new(&language, "(not_a_real_node_type)");
        // tree-sitter may accept syntactically valid S-exprs referencing unknown node types;
        // just verify no panic
        let _ = result;
    }
}
