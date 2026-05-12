use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

use super::languages::Lang;
use super::FileSummary;
use super::{CHARS_PER_TOKEN, INDEXED_EXTENSIONS, REGEX_EXTENSIONS};

mod generic;
mod javascript;
mod python;
mod rust;

use generic::extract_go_symbols_ts;
pub use generic::parse_with_regex;
use javascript::extract_js_ts_symbols_ts;
use python::extract_python_symbols_ts;
use rust::extract_rust_symbols_ts;

pub fn parse_with_treesitter(
    parser: &mut Parser,
    lang: Lang,
    path: &Path,
    content: &str,
) -> FileSummary {
    let language = lang.language();
    parser.set_language(&language).ok();

    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => {
            return FileSummary {
                path: path.to_path_buf(),
                symbols: Vec::new(),
                imports: Vec::new(),
            }
        }
    };

    let root = tree.root_node();
    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    match lang {
        Lang::Rust => extract_rust_symbols_ts(&root, content, &mut symbols, &mut imports),
        Lang::Python => extract_python_symbols_ts(&root, content, &mut symbols, &mut imports),
        Lang::JavaScript | Lang::TypeScript => {
            extract_js_ts_symbols_ts(&root, content, &mut symbols, &mut imports)
        }
        Lang::Go => extract_go_symbols_ts(&root, content, &mut symbols, &mut imports),
    }

    FileSummary {
        path: path.to_path_buf(),
        symbols,
        imports,
    }
}

pub fn format_map(
    file_summaries: &HashMap<PathBuf, FileSummary>,
    token_budget: usize,
) -> (String, usize) {
    let mut output = String::new();
    let budget_chars = token_budget * CHARS_PER_TOKEN;
    let mut entries: Vec<(&PathBuf, &FileSummary)> = file_summaries.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (rel_path, summary) in &entries {
        let file_block = format_file_entry(rel_path, summary);
        if output.len().saturating_add(file_block.len()) > budget_chars {
            if output.is_empty() {
                let trunc_len = budget_chars.saturating_sub(20);
                if trunc_len > 0 {
                    output.push_str(truncate_str_safe(&file_block, trunc_len));
                    output.push_str("\n... (truncated)\n");
                }
            } else {
                let remaining = budget_chars.saturating_sub(output.len()).saturating_sub(20);
                if remaining > 30 {
                    output.push_str(truncate_str_safe(&file_block, remaining));
                    output.push_str("\n... (truncated)\n");
                }
            }
            break;
        }
        output.push_str(&file_block);
    }
    let total_tokens = output.len() / CHARS_PER_TOKEN;
    (output, total_tokens)
}

fn format_file_entry(rel_path: &Path, summary: &FileSummary) -> String {
    let mut out = String::new();
    let path_str = rel_path.to_string_lossy();
    out.push_str(&format!("{path_str}:\n"));
    for symbol in &summary.symbols {
        let sig = if symbol.signature.is_empty() {
            symbol.name.clone()
        } else {
            let sig_line = symbol.signature.lines().next().unwrap_or(&symbol.name);
            if sig_line.len() > 120 {
                let truncated = truncate_str_safe(sig_line, 117);
                format!("{truncated}...")
            } else {
                sig_line.to_string()
            }
        };
        out.push_str(&format!("  {} {}\n", symbol.kind, sig));
    }
    out
}

pub fn node_text(node: &Node, source: &str) -> Option<String> {
    let start = node.start_byte();
    let end = node.end_byte();
    if start <= end && end <= source.len() {
        source.get(start..end).map(ToString::to_string)
    } else {
        None
    }
}

pub(crate) fn truncate_str_safe(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    if s.is_char_boundary(max_bytes) {
        &s[..max_bytes]
    } else {
        let mut boundary = max_bytes;
        while boundary > 0 && !s.is_char_boundary(boundary) {
            boundary -= 1;
        }
        &s[..boundary]
    }
}

pub(crate) fn extract_doc_comment(node: &Node, source: &str) -> Option<String> {
    let prev = node.prev_named_sibling()?;
    if prev.kind() == "block_comment" || prev.kind() == "line_comment" {
        let text = node_text(&prev, source)?;
        let cleaned = text
            .trim_start_matches("///")
            .trim_start_matches("//")
            .trim_start_matches("/*")
            .trim_end_matches("*/")
            .trim();
        if cleaned.len() < 200 {
            Some(cleaned.to_string())
        } else {
            Some(format!("{}...", truncate_str_safe(cleaned, 197)))
        }
    } else {
        None
    }
}

pub fn collect_source_files(project_root: &Path) -> Result<Vec<PathBuf>> {
    let ext_set: std::collections::HashSet<&str> = INDEXED_EXTENSIONS
        .iter()
        .chain(REGEX_EXTENSIONS.iter())
        .cloned()
        .collect();
    let mut files = Vec::new();
    let mut builder = ignore::WalkBuilder::new(project_root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .add_custom_ignore_filename(".rustycodeignore");
    for result in builder.build() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext_set.contains(ext) {
            files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

pub fn file_modified_time(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}
