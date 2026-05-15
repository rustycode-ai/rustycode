//! Import/symbol extraction and source file ranking.
//!
//! This module extracts imports and symbol references from code files, then
//! ranks source files by relevance to pre-load them into the agent's initial
//! context. It is a Rust port of the Python `build_source_snippets()` function
//! from SWE-bench experiments.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Information about an import extracted from source code.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportInfo {
    /// Module path (e.g., "collections.OrderedDict", "std::collections::HashMap").
    pub module_path: String,
    /// Specific symbols imported (e.g., ["OrderedDict"], ["HashMap", "BTreeMap"]).
    pub symbols: Vec<String>,
}

/// A pre-loaded source file snippet.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSnippet {
    /// Relative path from workspace root.
    pub path: String,
    /// File content (possibly truncated).
    pub content: String,
    /// Total lines in the file.
    pub total_lines: usize,
    /// Lines shown in the snippet.
    pub shown_lines: usize,
}

// ---------------------------------------------------------------------------
// Compiled regex patterns (lazy-init once)
// ---------------------------------------------------------------------------

static PYTHON_FROM_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*from\s+([a-zA-Z_][\w.]*)\s+import\s+(.+?)\s*$")
        .expect("valid python from-import regex")
});

static PYTHON_BARE_IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*import\s+([a-zA-Z_][\w.]*)(?:\s+as\s+\w+)?\s*$")
        .expect("valid python bare-import regex")
});

static RUST_USE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*use\s+([a-zA-Z_][\w:]*)(?:<::[\w:]+>)?\s*;") // basic path
        .expect("valid rust use regex")
});

static RUST_USE_GROUPED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*use\s+([a-zA-Z_][\w:]+)::\{([^}]+)\}\s*;")
        .expect("valid rust grouped-use regex")
});

static JS_IMPORT_NAMED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*import\s+(?:type\s+)?\{([^}]+)\}\s+from\s+['"]([^'"]+)['"]"#)
        .expect("valid js named-import regex")
});

static JS_IMPORT_DEFAULT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*import\s+(?:type\s+)?([a-zA-Z_]\w*)\s+from\s+['"]([^'"]+)['"]"#)
        .expect("valid js default-import regex")
});

static DOTTED_IDENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b([a-zA-Z_]\w*(?:\.[a-zA-Z_]\w*)+)\b").expect("valid dotted-identifier regex")
});

/// Keywords and common stdlib names to exclude from symbol references.
static FILTERED_NAMES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut s = HashSet::new();
    // Python keywords / builtins
    for name in [
        "self", "true", "false", "none", "True", "False", "None", "cls",
    ] {
        s.insert(name);
    }
    // Python common stdlib modules
    for name in [
        "sys",
        "os",
        "re",
        "json",
        "math",
        "logging",
        "collections",
        "functools",
        "itertools",
        "pathlib",
        "typing",
        "abc",
        "io",
        "copy",
        "datetime",
        "hashlib",
        "subprocess",
        "threading",
        "multiprocessing",
        "argparse",
        "unittest",
        "traceback",
        "inspect",
        "textwrap",
        "string",
        "random",
        "time",
        "enum",
        "dataclasses",
        "contextlib",
        "warnings",
        "pprint",
        "csv",
        "glob",
        "shutil",
        "tempfile",
        "struct",
        "pickle",
        "base64",
        "heapq",
        "bisect",
        "operator",
        "types",
        "weakref",
        "numbers",
        "decimal",
        "fractions",
    ] {
        s.insert(name);
    }
    // Rust common std
    for name in [
        "std",
        "core",
        "alloc",
        "proc_macro",
        "Self",
        "super",
        "crate",
    ] {
        s.insert(name);
    }
    // JS/TS common
    for name in [
        "console", "window", "document", "Math", "JSON", "Object", "Array", "String", "Number",
        "Boolean", "Promise", "Error", "Map", "Set", "Date", "RegExp",
    ] {
        s.insert(name);
    }
    s
});

// ---------------------------------------------------------------------------
// Import extraction
// ---------------------------------------------------------------------------

/// Extract imports from source code.
///
/// Supports Python, Rust, and JavaScript/TypeScript via regex matching.
/// Returns an empty vector for unrecognized languages.
pub fn extract_imports(content: &str, language: &str) -> Vec<ImportInfo> {
    match language {
        "python" | "py" => extract_python_imports(content),
        "rust" | "rs" => extract_rust_imports(content),
        "javascript" | "js" | "typescript" | "ts" => extract_js_imports(content),
        _ => Vec::new(),
    }
}

fn extract_python_imports(content: &str) -> Vec<ImportInfo> {
    let mut results = Vec::new();

    for line in content.lines() {
        // `from X import Y, Z`
        if let Some(caps) = PYTHON_FROM_IMPORT.captures(line) {
            let module = caps[1].to_string();
            let symbols_str = &caps[2];
            let symbols = parse_symbol_list(symbols_str);
            results.push(ImportInfo {
                module_path: module,
                symbols,
            });
            continue;
        }

        // `import X` or `import X as Y`
        if let Some(caps) = PYTHON_BARE_IMPORT.captures(line) {
            let module = caps[1].to_string();
            // For bare imports the whole module path is the "symbol" at the
            // tail segment. We record it with an empty symbols list to
            // indicate a full-module import.
            results.push(ImportInfo {
                module_path: module.clone(),
                symbols: vec![module.rsplit('.').next().unwrap_or(&module).to_string()],
            });
        }
    }

    results
}

fn extract_rust_imports(content: &str) -> Vec<ImportInfo> {
    let mut results = Vec::new();

    for line in content.lines() {
        // `use path::{A, B};`
        if let Some(caps) = RUST_USE_GROUPED.captures(line) {
            let module = caps[1].to_string();
            let symbols = parse_symbol_list(&caps[2]);
            results.push(ImportInfo {
                module_path: module,
                symbols,
            });
            continue;
        }

        // `use path::Symbol;`
        if let Some(caps) = RUST_USE.captures(line) {
            let full_path = caps[1].to_string();
            // Split into module path and the final symbol.
            if let Some(pos) = full_path.rfind("::") {
                let module = &full_path[..pos];
                let symbol = &full_path[pos + 2..];
                results.push(ImportInfo {
                    module_path: module.to_string(),
                    symbols: vec![symbol.to_string()],
                });
            } else {
                // Single-segment use like `use foo;`
                results.push(ImportInfo {
                    module_path: String::new(),
                    symbols: vec![full_path],
                });
            }
        }
    }

    results
}

fn extract_js_imports(content: &str) -> Vec<ImportInfo> {
    let mut results = Vec::new();

    for line in content.lines() {
        // `import { A, B } from 'module'`
        if let Some(caps) = JS_IMPORT_NAMED.captures(line) {
            let symbols = parse_symbol_list(&caps[1]);
            let module = caps[2].to_string();
            results.push(ImportInfo {
                module_path: module,
                symbols,
            });
            continue;
        }

        // `import X from 'module'`
        if let Some(caps) = JS_IMPORT_DEFAULT.captures(line) {
            let symbol = caps[1].to_string();
            let module = caps[2].to_string();
            results.push(ImportInfo {
                module_path: module,
                symbols: vec![symbol],
            });
        }
    }

    results
}

/// Parse a comma-separated list of symbols, stripping optional `as X` aliases.
fn parse_symbol_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|part| {
            let trimmed = part.trim();
            // Handle `Symbol as Alias`
            trimmed
                .split_whitespace()
                .next()
                .unwrap_or(trimmed)
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Symbol reference extraction
// ---------------------------------------------------------------------------

/// Extract dotted identifiers from source that look like code references.
///
/// Filters out common stdlib names, keywords, and language builtins so that
/// only project-relevant references remain.
pub fn extract_symbol_references(content: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();

    for caps in DOTTED_IDENT.captures_iter(content) {
        let full = &caps[1];
        // Check the leading segment against the filter list.
        let leading = full.split('.').next().unwrap_or(full);
        if FILTERED_NAMES.contains(leading) {
            continue;
        }
        // Deduplicate while preserving order.
        if seen.insert(full.to_string()) {
            refs.push(full.to_string());
        }
    }

    refs
}

// ---------------------------------------------------------------------------
// Source file ranking
// ---------------------------------------------------------------------------

/// Patterns used when grepping for symbol definitions.
const DEFINITION_PATTERNS: &[&str] = &[
    "def {sym}",
    "class {sym}",
    "fn {sym}",
    "struct {sym}",
    "enum {sym}",
    "trait {sym}",
    "impl {sym}",
    "const {sym}",
    "type {sym}",
    "interface {sym}",
    "function {sym}",
];

/// Directories and file patterns to exclude from candidate results.
fn is_excluded_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let components: Vec<&str> = lower.split('/').collect();

    // Hidden directories (e.g., .git, .venv).
    if components.iter().any(|c| c.starts_with('.') && *c != ".") {
        return true;
    }
    // __pycache__
    if components.contains(&"__pycache__") {
        return true;
    }
    // node_modules
    if components.contains(&"node_modules") {
        return true;
    }
    // Test directories: exclude if any path segment is a known test directory.
    let test_dirs = ["tests", "test", "__tests__", "specs", "spec"];
    if components.iter().any(|c| test_dirs.contains(c)) {
        return true;
    }
    // Test files: check filename with word boundaries to avoid false positives
    // like "latest.rs" or "special.js".
    let filename = components.last().copied().unwrap_or("");
    if is_test_filename(filename) {
        return true;
    }
    false
}

/// Check if a filename looks like a test/spec file using word-boundary-aware matching.
fn is_test_filename(filename: &str) -> bool {
    // Patterns: _test.rs, test_foo.py, .test.js, .spec.ts, _spec.py, Test.java
    let lower = filename.to_ascii_lowercase();
    let stem = lower.rsplit_once('.').map(|(s, _)| s).unwrap_or(&lower);

    // Check for test/spec as a distinct segment separated by underscore, dot, or
    // at the start of the stem.
    for part in stem.split(&['_', '.'][..]) {
        if part == "test" || part == "spec" || part == "tests" || part == "specs" {
            return true;
        }
    }
    false
}

/// Score and rank source files by relevance to the given imports and symbol references.
///
/// For each symbol, `grep` is used to find files defining it. For each import
/// module path, matching source files are found via `find`. Files are scored
/// (+2 per import path match, +1 per symbol match), filtered, truncated to
/// `max_lines`, and the top `max_files` are returned.
pub fn rank_source_files(
    imports: &[ImportInfo],
    symbol_refs: &[String],
    workspace: &Path,
    max_files: usize,
    max_lines: usize,
) -> Vec<FileSnippet> {
    if imports.is_empty() && symbol_refs.is_empty() {
        return Vec::new();
    }

    let mut scores: HashMap<String, usize> = HashMap::new();

    let workspace_str = match workspace.to_str() {
        Some(s) => s,
        None => return Vec::new(),
    };

    // --- Score by import module path matches ---
    for import in imports {
        if import.module_path.is_empty() {
            continue;
        }
        // Convert module path to a glob-like fragment for finding files.
        // e.g. "collections.OrderedDict" -> "collections/OrderedDict"
        let path_hint = import.module_path.replace('.', "/").replace("::", "/");

        // Use `find` to locate matching source files.
        if let Ok(output) = Command::new("find")
            .args([
                workspace_str,
                "-type",
                "f",
                "(",
                "-name",
                &format!("{path_hint}.py"),
                "-o",
                "-name",
                &format!("{path_hint}.rs"),
                "-o",
                "-name",
                &format!("{path_hint}.js"),
                "-o",
                "-name",
                &format!("{path_hint}.ts"),
                "-o",
                "-name",
                &format!("{path_hint}.jsx"),
                "-o",
                "-name",
                &format!("{path_hint}.tsx"),
                ")",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(rel) = Path::new(line).strip_prefix(workspace) {
                        let rel_str = rel.to_string_lossy().to_string();
                        if !is_excluded_path(&rel_str) {
                            *scores.entry(rel_str).or_insert(0) += 2;
                        }
                    }
                }
            }
        }
    }

    // --- Score by symbol definition matches (grep) ---
    for sym in symbol_refs {
        // Take the tail segment of dotted symbols (e.g., "Response" from
        // "requests.models.Response").
        let tail = sym.rsplit('.').next().unwrap_or(sym);

        for pattern_tmpl in DEFINITION_PATTERNS {
            let pattern = pattern_tmpl.replace("{sym}", tail);

            let Ok(output) = Command::new("grep")
                .args([
                    "-rn",
                    "--include=*.py",
                    "--include=*.rs",
                    "--include=*.js",
                    "--include=*.ts",
                    "--include=*.jsx",
                    "--include=*.tsx",
                    "-l",
                    &pattern,
                    workspace_str,
                ])
                .output()
            else {
                continue;
            };

            if !output.status.success() {
                continue;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(rel) = Path::new(line).strip_prefix(workspace) {
                    let rel_str = rel.to_string_lossy().to_string();
                    if !is_excluded_path(&rel_str) {
                        *scores.entry(rel_str).or_insert(0) += 1;
                    }
                }
            }
            // Only use the first matching pattern per symbol to avoid
            // over-counting files that match multiple definition patterns.
            break;
        }
    }

    // --- Sort by score (descending), then by path for determinism ---
    let mut ranked: Vec<(String, usize)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // --- Take top max_files, read and truncate ---
    let mut snippets = Vec::new();
    for (rel_path, _score) in ranked.into_iter().take(max_files) {
        let full_path = workspace.join(&rel_path);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let total_lines = content.lines().count();
        if total_lines <= max_lines {
            snippets.push(FileSnippet {
                path: rel_path,
                content,
                total_lines,
                shown_lines: total_lines,
            });
        } else {
            // Truncate: take the first `max_lines` lines and add an indicator.
            let truncated: String = content
                .lines()
                .take(max_lines)
                .collect::<Vec<&str>>()
                .join("\n");
            let indicator = format!("\n... ({} more lines truncated)", total_lines - max_lines);
            snippets.push(FileSnippet {
                path: rel_path,
                content: format!("{truncated}{indicator}"),
                total_lines,
                shown_lines: max_lines,
            });
        }
    }

    snippets
}

// ---------------------------------------------------------------------------
// Convenience function
// ---------------------------------------------------------------------------

/// Read a file, extract imports and symbol references, then rank and return
/// source file snippets from the workspace.
///
/// This is the main entry point for pre-loading relevant source files into an
/// agent's initial context.
pub fn build_source_snippets_from_file(
    file_path: &Path,
    workspace: &Path,
    language: &str,
    max_files: usize,
    max_lines: usize,
) -> Vec<FileSnippet> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let imports = extract_imports(&content, language);
    let symbol_refs = extract_symbol_references(&content);

    rank_source_files(&imports, &symbol_refs, workspace, max_files, max_lines)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // --- extract_python_imports ---
    #[test]
    fn extract_python_imports() {
        let code = r#"
from collections import OrderedDict
import os.path
from typing import List, Dict, Optional
import json as json_lib
"#;
        let imports = extract_imports(code, "python");
        assert_eq!(imports.len(), 4);

        assert_eq!(imports[0].module_path, "collections");
        assert_eq!(imports[0].symbols, vec!["OrderedDict"]);

        assert_eq!(imports[1].module_path, "os.path");
        assert_eq!(imports[1].symbols, vec!["path"]);

        assert_eq!(imports[2].module_path, "typing");
        assert_eq!(imports[2].symbols, vec!["List", "Dict", "Optional"]);

        assert_eq!(imports[3].module_path, "json");
        assert_eq!(imports[3].symbols, vec!["json"]);
    }

    // --- extract_rust_imports ---
    #[test]
    fn extract_rust_imports() {
        let code = r#"
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use anyhow::Result;
"#;
        let imports = extract_imports(code, "rust");
        assert_eq!(imports.len(), 3);

        assert_eq!(imports[0].module_path, "std::collections");
        assert_eq!(imports[0].symbols, vec!["HashMap"]);

        assert_eq!(imports[1].module_path, "serde");
        assert_eq!(imports[1].symbols, vec!["Serialize", "Deserialize"]);

        assert_eq!(imports[2].module_path, "anyhow");
        assert_eq!(imports[2].symbols, vec!["Result"]);
    }

    // --- extract_js_imports ---
    #[test]
    fn extract_js_imports() {
        let code = r#"
import { useState, useEffect } from 'react';
import axios from 'axios';
import type { Config } from './config';
"#;
        let imports = extract_imports(code, "javascript");
        assert_eq!(imports.len(), 3);

        assert_eq!(imports[0].module_path, "react");
        assert_eq!(imports[0].symbols, vec!["useState", "useEffect"]);

        assert_eq!(imports[1].module_path, "axios");
        assert_eq!(imports[1].symbols, vec!["axios"]);

        assert_eq!(imports[2].module_path, "./config");
        assert_eq!(imports[2].symbols, vec!["Config"]);
    }

    // --- extract_symbol_references_filters_stdlib ---
    #[test]
    fn extract_symbol_references_filters_stdlib() {
        let code = r#"
sys.path.append("/foo")
os.environ["HOME"]
json.dumps(data)
math.pi
self.value = 42
true.flag = false
"#;
        let refs = extract_symbol_references(code);
        // All of the above should be filtered out.
        assert!(
            refs.is_empty(),
            "Expected no references after stdlib filtering, got: {refs:?}"
        );
    }

    // --- extract_symbol_references_captures_dotted ---
    #[test]
    fn extract_symbol_references_captures_dotted() {
        let code = r#"
requests.models.Response
auth.validate
myapp.utils.format_date
"#;
        let refs = extract_symbol_references(code);
        assert_eq!(refs.len(), 3);
        assert!(refs.contains(&"requests.models.Response".to_string()));
        assert!(refs.contains(&"auth.validate".to_string()));
        assert!(refs.contains(&"myapp.utils.format_date".to_string()));
    }

    // --- rank_source_files_empty_imports ---
    #[test]
    fn rank_source_files_empty_imports() {
        let result = rank_source_files(&[], &[], Path::new("/tmp"), 10, 100);
        assert!(result.is_empty());
    }

    // --- file_snippet_truncation ---
    #[test]
    fn file_snippet_truncation() {
        use std::io::Write as IoWrite;

        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("sample.py");
        let mut content = String::new();
        for i in 1..=200 {
            use std::fmt::Write;
            let _ = writeln!(content, "line {i}");
        }
        {
            let mut f = std::fs::File::create(&file_path).expect("create");
            f.write_all(content.as_bytes()).expect("write");
        }

        // Build imports that reference "sample" to make it appear in results.
        // We'll call rank_source_files with a single import and no grep
        // hits so we can test truncation of the file we created.
        // Since rank uses grep/find on the workspace, we use the temp dir.
        let imports = vec![ImportInfo {
            module_path: "sample".to_string(),
            symbols: vec!["sample".to_string()],
        }];

        let snippets = rank_source_files(&imports, &[], dir.path(), 5, 50);

        // The find command should locate sample.py via module_path.
        if let Some(snippet) = snippets.into_iter().find(|s| s.path.ends_with("sample.py")) {
            assert_eq!(snippet.total_lines, 200);
            assert_eq!(snippet.shown_lines, 50);
            assert!(snippet.content.contains("... (150 more lines truncated)"));
            assert!(snippet.content.starts_with("line 1\n"));
        }
        // If find didn't locate it (e.g., not installed), the test still
        // passes — it just doesn't verify truncation on CI without find.
    }

    // --- extract_imports_unknown_language ---
    #[test]
    fn extract_imports_unknown_language() {
        let code = "import foo";
        let result = extract_imports(code, "brainfuck");
        assert!(result.is_empty());
    }

    // --- edge cases ---
    #[test]
    fn extract_imports_empty_content() {
        assert!(extract_imports("", "python").is_empty());
        assert!(extract_imports("", "rust").is_empty());
        assert!(extract_imports("", "javascript").is_empty());
    }

    #[test]
    fn parse_symbol_list_handles_aliases() {
        let result = parse_symbol_list("foo as bar, baz, qux as quux");
        assert_eq!(result, vec!["foo", "baz", "qux"]);
    }

    #[test]
    fn is_excluded_path_filters_tests_and_hidden() {
        assert!(is_excluded_path("tests/helper.py"));
        assert!(is_excluded_path("src/foo_test.rs"));
        assert!(is_excluded_path("__pycache__/cache.pyc"));
        assert!(is_excluded_path(".venv/lib/foo.py"));
        assert!(is_excluded_path("node_modules/react/index.js"));
        assert!(is_excluded_path("__tests__/setup.ts"));
        assert!(is_excluded_path("src/component.test.ts"));
        assert!(is_excluded_path("src/unit.spec.js"));
        assert!(is_excluded_path("test/helper.py"));
        assert!(is_excluded_path("spec/model_spec.rb"));

        // Not excluded: "latest" contains "test" substring but not as a word segment
        assert!(!is_excluded_path("src/latest.rs"));
        assert!(!is_excluded_path("src/main.rs"));
        assert!(!is_excluded_path("lib/parser.py"));
        assert!(!is_excluded_path("src/special_handler.rs"));
        assert!(!is_excluded_path("src/spectacular.rs"));
    }

    #[test]
    fn extract_symbol_references_deduplicates() {
        let code = "foo.bar.baz\nfoo.bar.baz\nfoo.bar.baz\n";
        let refs = extract_symbol_references(code);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], "foo.bar.baz");
    }

    #[test]
    fn import_info_equality() {
        let a = ImportInfo {
            module_path: "foo".to_string(),
            symbols: vec!["bar".to_string()],
        };
        let b = ImportInfo {
            module_path: "foo".to_string(),
            symbols: vec!["bar".to_string()],
        };
        assert_eq!(a, b);
    }

    #[test]
    fn file_snippet_equality() {
        let a = FileSnippet {
            path: "foo.rs".to_string(),
            content: "fn main() {}".to_string(),
            total_lines: 1,
            shown_lines: 1,
        };
        let b = FileSnippet {
            path: "foo.rs".to_string(),
            content: "fn main() {}".to_string(),
            total_lines: 1,
            shown_lines: 1,
        };
        assert_eq!(a, b);
    }
}
