use crate::indexing::CodeIndex;
use crate::providers::lsp::{get_lsp_config_for_project, read_file_blocking, with_lsp_client};
use crate::providers::symbol::symbols_overview;
use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use lsp_types::{GotoDefinitionResponse, Location, Position, SymbolInformation, Uri as LspUrl};
use rustycode_lsp::LanguageId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use url::Url as FileUrl;

static CODE_INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<CodeIndex>>>> = OnceLock::new();

fn code_indexes() -> &'static Mutex<HashMap<PathBuf, Arc<CodeIndex>>> {
    CODE_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn workspace_root(ctx: &ToolContext) -> PathBuf {
    std::fs::canonicalize(&ctx.cwd).unwrap_or_else(|_| ctx.cwd.clone())
}

fn build_code_index(ctx: &ToolContext) -> Result<Arc<CodeIndex>> {
    let root = workspace_root(ctx);
    let mut guard = code_indexes()
        .lock()
        .map_err(|_| anyhow!("failed to lock exploration index cache"))?;

    if let Some(index) = guard.get(&root) {
        return Ok(Arc::clone(index));
    }

    let mut index = CodeIndex::new(root.clone());
    index.build().with_context(|| {
        format!(
            "failed to build exploration code index for {}",
            root.display()
        )
    })?;

    let index = Arc::new(index);
    guard.insert(root, Arc::clone(&index));
    Ok(index)
}

fn scope_matches(path: &Path, scope: Option<&str>, cwd: &Path) -> bool {
    let Some(scope) = scope.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };

    let normalized_scope = scope.replace('\\', "/");
    let rel = path.strip_prefix(cwd).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace('\\', "/");
    let abs_str = path.to_string_lossy().replace('\\', "/");

    rel_str.contains(&normalized_scope) || abs_str.contains(&normalized_scope)
}

fn file_uri(path: &Path) -> Result<LspUrl> {
    let file_url = FileUrl::from_file_path(path)
        .map_err(|()| anyhow!("failed to convert path to file URI: {}", path.display()))?;
    file_url.as_str().parse().map_err(|_| {
        anyhow!(
            "failed to convert file URL into LSP URI: {}",
            path.display()
        )
    })
}

fn uri_to_path(uri: &LspUrl) -> Option<PathBuf> {
    FileUrl::parse(uri.as_str()).ok()?.to_file_path().ok()
}

fn rel_path_string(path: &Path, cwd: &Path) -> String {
    path.strip_prefix(cwd)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn format_location(location: &Location, cwd: &Path) -> String {
    let path = uri_to_path(&location.uri).unwrap_or_else(|| PathBuf::from(location.uri.as_str()));
    format!(
        "{}:{}:{}",
        rel_path_string(&path, cwd),
        location.range.start.line + 1,
        location.range.start.character + 1
    )
}

fn format_definition_response(response: &GotoDefinitionResponse, cwd: &Path) -> String {
    match response {
        GotoDefinitionResponse::Scalar(location) => format_location(location, cwd),
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| format_location(location, cwd))
            .collect::<Vec<_>>()
            .join("\n"),
        GotoDefinitionResponse::Link(links) => serde_json::to_string_pretty(links)
            .unwrap_or_else(|_| "Unable to format definition links".to_string()),
    }
}

fn symbol_candidates<'a>(
    index: &'a CodeIndex,
    symbol: &str,
    scope: Option<&str>,
    cwd: &Path,
) -> Vec<&'a crate::indexing::code_index::Symbol> {
    index
        .find_symbols(symbol)
        .into_iter()
        .filter(|entry| scope_matches(&entry.file_path, scope, cwd))
        .collect()
}

fn nearby_matches(index: &CodeIndex, query: &str, scope: Option<&str>, cwd: &Path) -> Vec<Value> {
    index
        .search(query)
        .into_iter()
        .filter(|entry| scope_matches(&entry.file_path, scope, cwd))
        .map(|entry| {
            json!({
                "path": rel_path_string(&entry.file_path, cwd),
                "line": entry.line,
                "match_type": format!("{:?}", entry.match_type),
                "score": entry.score,
                "context": entry.context,
            })
        })
        .collect()
}

fn choose_symbol_information<'a>(
    candidates: &'a [SymbolInformation],
    path: &Path,
    symbol_name: &str,
) -> Option<&'a SymbolInformation> {
    candidates
        .iter()
        .find(|candidate| {
            candidate.name == symbol_name
                && uri_to_path(&candidate.location.uri)
                    .is_some_and(|candidate_path| candidate_path == path)
        })
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| candidate.name == symbol_name)
        })
        .or_else(|| candidates.first())
}

/// Parameters for the find tool.
#[derive(Deserialize, JsonSchema)]
pub struct FindParams {
    /// What to look for in the codebase
    query: String,
    /// Optional scope path or prefix to narrow the search
    #[serde(rename = "in")]
    #[schemars(rename = "in")]
    scope: Option<String>,
    /// Maximum number of results to return
    limit: Option<u64>,
}

/// Parameters for the inspect tool.
#[derive(Deserialize, JsonSchema)]
pub struct InspectParams {
    /// Symbol name or path to inspect
    symbol: String,
    /// What aspect to inspect
    aspect: Option<String>,
    /// Optional scope path or prefix to disambiguate
    #[serde(rename = "in")]
    #[schemars(rename = "in")]
    scope: Option<String>,
}

rustycode_tools_api::define_tool! {
    pub struct FindTool;

    name: "find",
    description: "Find relevant code locations for a query. Use this first for broad discovery, then inspect the best match with inspect.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: FindParams, ctx) {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "find")?;
        }

        let query = &params.query;
        let scope = params.scope.as_deref();
        let limit = params.limit.unwrap_or(8).clamp(1, 20) as usize;

        let index = build_code_index(ctx)?;
        let mut exact_symbols: Vec<Value> = symbol_candidates(&index, query, scope, &ctx.cwd)
            .into_iter()
            .take(limit)
            .map(|symbol| {
                json!({
                    "path": rel_path_string(&symbol.file_path, &ctx.cwd),
                    "line": symbol.line,
                    "kind": symbol.kind.to_string(),
                    "name": symbol.name,
                    "signature": symbol.signature,
                    "parent": symbol.parent,
                })
            })
            .collect();

        let mut seen: HashSet<(String, usize)> = exact_symbols
            .iter()
            .filter_map(|entry| {
                Some((
                    entry.get("path")?.as_str()?.to_string(),
                    entry.get("line")?.as_u64()? as usize,
                ))
            })
            .collect();

        let mut results = nearby_matches(&index, query, scope, &ctx.cwd)
            .into_iter()
            .filter(|entry| {
                let path = entry
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let line = entry
                    .get("line")
                    .and_then(Value::as_u64)
                    .unwrap_or_default() as usize;
                seen.insert((path.to_string(), line))
            })
            .take(limit.saturating_sub(exact_symbols.len()))
            .collect::<Vec<_>>();

        let mut all_results = Vec::new();
        all_results.append(&mut exact_symbols);
        all_results.append(&mut results);

        let summary = if all_results.is_empty() {
            format!("No matches found for `{query}`")
        } else {
            let mut output = format!("Found {} matches for `{query}`\n", all_results.len());
            for (idx, entry) in all_results.iter().enumerate() {
                let line = entry
                    .get("line")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let path = entry.get("path").and_then(Value::as_str).unwrap_or("?");
                let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("match");
                let context = entry.get("context").and_then(Value::as_str).unwrap_or("");
                output.push_str(&format!(
                    "{}. {}:{} [{}] {}\n",
                    idx + 1,
                    path,
                    line,
                    kind,
                    context.lines().next().unwrap_or(context).trim()
                ));
            }
            output
        };

        Ok(ToolOutput::text(summary).with_metadata(ctx, || json!({
                "query": query,
                "scope": scope,
                "limit": limit,
                "results": all_results,
            })))
    }
}

rustycode_tools_api::define_tool! {
    pub struct InspectTool;

    name: "inspect",
    description: "Inspect a symbol deeply. Use after find to look at definition, hover info, references, outline, or dependencies.",
    permission: ToolPermission::Read,
    tags: [ToolTag::Explore],

    execute(params: InspectParams, ctx) {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "inspect")?;
        }

        let symbol = &params.symbol;
        let aspect = params.aspect.as_deref().unwrap_or("definition");
        let scope = params.scope.as_deref();

        let index = build_code_index(ctx)?;
        let candidates = symbol_candidates(&index, symbol, scope, &ctx.cwd);

        if candidates.is_empty() {
            let nearby = nearby_matches(&index, symbol, scope, &ctx.cwd);
            return Ok(ToolOutput::text(format!("No exact symbol named `{symbol}` was found")).with_metadata(ctx, || json!({
                    "symbol": symbol,
                    "aspect": aspect,
                    "scope": scope,
                    "matches": nearby,
                })));
        }

        if candidates.len() > 1 {
            let matches: Vec<Value> = candidates
                .into_iter()
                .map(|entry| {
                    json!({
                        "path": rel_path_string(&entry.file_path, &ctx.cwd),
                        "line": entry.line,
                        "kind": entry.kind.to_string(),
                        "name": entry.name,
                        "signature": entry.signature,
                        "parent": entry.parent,
                    })
                })
                .collect();
            return Ok(ToolOutput::text(format!("`{symbol}` matches multiple symbols; pick one and inspect again")).with_metadata(ctx, || json!({
                    "symbol": symbol,
                    "aspect": aspect,
                    "scope": scope,
                    "matches": matches,
                })));
        }

        let symbol_info = candidates[0];
        let file_path = symbol_info.file_path.clone();
        let file_text = read_file_blocking(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        let language = LanguageId::from_path(&file_path);
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);
        let uri = file_uri(&file_path)?;
        let rel_path = rel_path_string(&file_path, &ctx.cwd);

        let result = with_lsp_client(ctx, language, lsp_config.as_ref(), |client| {
            crate::providers::lsp::run_async_result(async {
                client
                    .open_document(uri.clone(), language.language_id_str(), 1, &file_text)
                    .await?;

                let position = if let Some(location) = choose_symbol_information(
                    &client.workspace_symbols(symbol_info.name.as_str()).await?,
                    &file_path,
                    symbol_info.name.as_str(),
                ) {
                    location.location.range.start
                } else {
                    Position::new(symbol_info.line.saturating_sub(1) as u32, 0)
                };

                let payload = match aspect {
                    "hover" => json!({
                        "symbol": symbol_info.name,
                        "path": rel_path,
                        "line": symbol_info.line,
                        "kind": symbol_info.kind.to_string(),
                        "hover": client.hover(uri.clone(), position).await?.map(|hover| {
                            serde_json::to_value(hover).unwrap_or(Value::Null)
                        }),
                    }),
                    "references" => json!({
                        "symbol": symbol_info.name,
                        "path": rel_path,
                        "line": symbol_info.line,
                        "kind": symbol_info.kind.to_string(),
                        "references": client
                            .references(uri.clone(), position)
                            .await?
                            .into_iter()
                            .map(|location| json!(format_location(&location, &ctx.cwd)))
                            .collect::<Vec<_>>(),
                    }),
                    "outline" => {
                        let symbols = client.document_symbols(uri.clone()).await?;
                        json!({
                            "symbol": symbol_info.name,
                            "path": rel_path,
                            "line": symbol_info.line,
                            "kind": symbol_info.kind.to_string(),
                            "outline": symbols_overview(&symbols, 2),
                        })
                    }
                    "dependencies" => json!({
                        "symbol": symbol_info.name,
                        "path": rel_path.clone(),
                        "line": symbol_info.line,
                        "kind": symbol_info.kind.to_string(),
                        "dependents": index
                            .get_dependents(&file_path)
                            .into_iter()
                            .map(|path| rel_path_string(&path, &ctx.cwd))
                            .collect::<Vec<_>>(),
                    }),
                    "overview" | "definition" => {
                        let definition = client.goto_definition(uri.clone(), position).await?;
                        json!({
                            "symbol": symbol_info.name,
                            "path": rel_path,
                            "line": symbol_info.line,
                            "kind": symbol_info.kind.to_string(),
                            "signature": symbol_info.signature,
                            "parent": symbol_info.parent,
                            "definition": definition
                                .as_ref()
                                .map(|resp| format_definition_response(resp, &ctx.cwd)),
                        })
                    }
                    other => return Err(anyhow!("unsupported aspect: {other}")),
                };

                Ok(payload)
            })
        })?;

        let summary = match aspect {
            "hover" => format!("Hover info for `{}`", symbol_info.name),
            "references" => format!("References for `{}`", symbol_info.name),
            "outline" => format!("Outline for `{}`", symbol_info.name),
            "dependencies" => format!("Dependencies for `{}`", symbol_info.name),
            _ => format!("Definition for `{}`", symbol_info.name),
        };

        Ok(ToolOutput::text(summary).with_metadata(ctx, || result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Tool;

    #[test]
    fn scope_matches_accepts_narrowing_prefixes() {
        let cwd = PathBuf::from("/workspace/project");
        let file = cwd.join("src/services/conversation_service.rs");

        assert!(scope_matches(&file, Some("src/services"), &cwd));
        assert!(scope_matches(&file, Some("conversation_service"), &cwd));
        assert!(!scope_matches(&file, Some("src/ui"), &cwd));
    }

    #[test]
    fn find_and_inspect_names_are_stable() {
        assert_eq!(FindTool.name(), "find");
        assert_eq!(InspectTool.name(), "inspect");
    }
}
