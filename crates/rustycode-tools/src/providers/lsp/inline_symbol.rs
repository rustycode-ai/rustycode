use super::*;
use crate::{ToolOutput, ToolPermission, ToolTag};
use lsp_types::Position;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct LspInlineSymbolParams {
    #[serde(default)]
    relative_path: String,
    pub name_path: String,
    #[serde(default)]
    pub force: bool,
    #[serde(default = "default_remove_definition")]
    pub remove_definition: bool,
    #[serde(default)]
    pub language: Option<String>,
}

fn default_remove_definition() -> bool {
    true
}

rustycode_tools_api::define_tool! {
    pub struct LspInlineSymbolTool;

    name: "LspInlineSymbol",
    description: "Inline a symbol definition at its usage sites",
    permission: ToolPermission::Write,
    tags: [ToolTag::Refactor],

    execute(params: LspInlineSymbolParams, ctx) {
        let file_path = resolve_file_path_from_str(ctx, &params.relative_path)?;
        let name_path_str = &params.name_path;
        let force = params.force;
        let remove_definition = params.remove_definition;

        let language_id = if let Some(lang_str) = &params.language {
            LanguageId::from_path(&PathBuf::from(lang_str))
        } else {
            language_for_path(&file_path)
        };

        let uri = Url::from_file_path(&file_path)
            .map_err(|()| anyhow!("invalid file path: {file_path:?}"))?;
        let mut text = read_file_blocking(&file_path)?;
        let language_str = language_id.language_id_str().to_string();
        let lsp_config = get_lsp_config_for_project(&ctx.cwd);

        let symbols = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let text = text.clone();
            let language_str = language_str.clone();
            run_async_result(async {
                client
                    .open_document(uri.clone(), &language_str, 1, &text)
                    .await?;
                client.document_symbols(uri.clone()).await
            })
        })?;

        let sym_path = crate::providers::symbol::SymbolPath::parse(name_path_str);
        let target_symbol = crate::providers::symbol::find_unique(&symbols, &sym_path)?;

        // Extract function signature and body
        let sel_start = crate::providers::symbol::position_to_byte_index(
            &text,
            target_symbol.selection_range.start,
        )?;
        let body_end =
            crate::providers::symbol::position_to_byte_index(&text, target_symbol.range.end)?;

        // Get parameter names from the signature
        let sig_text = &text[sel_start..body_end];
        let params_list =
            crate::providers::symbol::extract_param_names(sig_text).unwrap_or_default();

        // Get function body
        let body_text = &text[sel_start..body_end];
        let (body_content, is_single_expr) =
            crate::providers::symbol::extract_function_body(body_text)?;

        // Check if inlining is safe (multi-statement requires force=true)
        if !is_single_expr && !force {
            return Err(anyhow!(
                "function body has multiple statements; use force=true to inline anyway"
            ));
        }

        // Get references
        let references = with_lsp_client(ctx, language_id, lsp_config.as_ref(), |client| {
            let uri = uri.clone();
            let pos = target_symbol.selection_range.start;
            run_async_result(async { client.references(uri.clone(), pos).await })
        })?;

        // Filter to references in the same file (excluding the definition itself)
        let same_file_refs: Vec<_> = references
            .iter()
            .filter(|r| {
                r.uri.path().as_str() == file_path.as_os_str()
                    && r.range != target_symbol.selection_range
            })
            .collect();

        if same_file_refs.is_empty() {
            return Ok(ToolOutput::text("No references found to inline".to_string()).with_metadata(ctx, || json!({
                    "symbol": name_path_str,
                    "status": "no_references"
                })));
        }

        // Process each reference from end to start (to avoid index shifts)
        let mut call_sites: Vec<_> = same_file_refs
            .iter()
            .map(|r| crate::providers::symbol::position_to_byte_index(&text, r.range.start))
            .collect::<Result<Vec<_>>>()?;
        call_sites.sort_by_key(|a| std::cmp::Reverse(*a)); // Sort in reverse order

        let mut inlined_count = 0;
        let mut errors = Vec::new();

        for call_byte_idx in call_sites {
            // Find the argument list for this call
            if let Some((arg_start, arg_end)) =
                crate::providers::symbol::find_call_args_range(&text, call_byte_idx)
            {
                let args_str = &text[arg_start + 1..arg_end];
                let args = crate::providers::symbol::split_args(args_str);

                // Validate arity
                if args.len() != params_list.len() {
                    errors.push(format!(
                        "arity mismatch at byte {}: expected {} args, got {}",
                        call_byte_idx,
                        params_list.len(),
                        args.len()
                    ));
                    continue;
                }

                // Perform substitution
                let param_refs: Vec<&str> = params_list.iter().map(String::as_str).collect();
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let inlined_body = crate::providers::symbol::substitute_params(
                    &body_content,
                    &param_refs,
                    &arg_refs,
                );

                // Replace the call with the inlined body
                let replacement = format!("({inlined_body})");
                text = crate::providers::symbol::replace_range(
                    &text,
                    &lsp_types::Range {
                        start: Position {
                            line: 0,
                            character: call_byte_idx as u32,
                        },
                        end: Position {
                            line: 0,
                            character: (arg_end + 1) as u32,
                        },
                    },
                    &replacement,
                )?;

                inlined_count += 1;
            } else {
                errors.push(format!(
                    "could not find argument list at byte {call_byte_idx}"
                ));
            }
        }

        // Optionally remove the definition
        if remove_definition && inlined_count > 0 {
            text = crate::providers::symbol::replace_range(&text, &target_symbol.range, "")?;
        }

        // Write the file back
        safe_write_file(&file_path, text.as_bytes())
            .with_context(|| format!("failed to write file {}", file_path.display()))?;

        Ok(ToolOutput::text(serde_json::to_string_pretty(&json!({
                "symbol": name_path_str,
                "inlined_count": inlined_count,
                "definition_removed": remove_definition && inlined_count > 0,
                "errors": errors,
                "status": if errors.is_empty() {
                    format!("Successfully inlined {inlined_count} call site(s)")
                } else {
                    format!("Inlined {} call site(s) with {} error(s)", inlined_count, errors.len())
                }
            }))?).with_metadata(ctx, || json!({
                "symbol": name_path_str,
                "inlined_count": inlined_count,
                "definition_removed": remove_definition && inlined_count > 0,
                "errors": errors
            })))
    }
}
