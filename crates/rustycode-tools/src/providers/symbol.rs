//! Symbol-level editing utilities for LSP-based code manipulation.
//!
//! This module provides language-agnostic tools for working with code symbols.
//! It leverages LSP's `documentSymbol` to understand code structure and provides
//! primitives for finding, analyzing, and editing symbols by name.

use anyhow::{anyhow, Result};
use lsp_types::{DocumentSymbol, Position, Range, SymbolKind};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Parsed symbol path pattern for navigation and matching.
///
/// A symbol path is a `/`-separated sequence of symbol names to navigate
/// a symbol tree. Example: `MyClass/my_method` or `/root/child`.
#[derive(Debug, Clone)]
pub struct SymbolPath {
    /// Parsed components: ["`MyClass`", "`my_method`"]
    components: Vec<String>,
    /// Whether the pattern began with "/" (absolute match from root only)
    absolute: bool,
    /// Optional overload index: 0-based, from "name[N]" syntax
    #[allow(dead_code)]
    overload_idx: Option<usize>,
    /// If true, the last component matches by substring instead of exact match
    substring_matching: bool,
}

impl SymbolPath {
    /// Parse a symbol path pattern.
    ///
    /// # Format
    ///
    /// - Separator: `/`
    /// - Absolute (from root): leading `/` e.g. `/MyClass/method`
    /// - Relative (suffix match): `MyClass/method` matches any ancestor chain
    /// - Simple name: `method` matches any symbol named "method"
    /// - Overload index: `name[0]`, `name[1]` — 0-based
    ///
    /// # Example
    ///
    /// ```ignore
    /// let p2 = SymbolPath::parse("/root/child");
    /// let p3 = SymbolPath::parse("method[1]");  // second overload
    /// ```
    pub fn parse(pattern: &str) -> Self {
        let absolute = pattern.starts_with('/');
        let pattern = pattern.trim_start_matches('/');

        let mut components = Vec::new();
        let mut overload_idx = None;

        for component in pattern.split('/') {
            if component.is_empty() {
                continue;
            }

            // Parse overload index from "name[N]" syntax
            if let Some(bracket_pos) = component.find('[') {
                let name = component[..bracket_pos].to_string();
                let idx_str = component[bracket_pos + 1..].trim_end_matches(']');
                if let Ok(idx) = idx_str.parse::<usize>() {
                    overload_idx = Some(idx);
                    components.push(name);
                } else {
                    components.push(component.to_string());
                }
            } else {
                components.push(component.to_string());
            }
        }

        Self {
            components,
            absolute,
            overload_idx,
            substring_matching: false,
        }
    }

    /// Check if a symbol name matches this path component.
    ///
    /// - For non-final components: exact match required
    /// - For final component: substring match if enabled, else exact
    fn matches_component(&self, component_idx: usize, symbol_name: &str) -> bool {
        if component_idx >= self.components.len() {
            return false;
        }

        let pattern = &self.components[component_idx];
        let is_last = component_idx == self.components.len() - 1;

        if is_last && self.substring_matching {
            symbol_name.contains(pattern)
        } else {
            pattern == symbol_name
        }
    }
}

/// Result of finding a symbol in the tree.
#[derive(Debug, Clone)]
pub struct FoundSymbol<'a> {
    /// Reference to the matched `DocumentSymbol`
    pub symbol: &'a DocumentSymbol,
    /// Full qualified path: "Class/method"
    pub qualified_path: String,
}

/// Find all symbols matching a name path pattern in a symbol tree.
///
/// Returns a vec of all matches found by DFS traversal.
pub fn find_symbols<'a>(roots: &'a [DocumentSymbol], path: &SymbolPath) -> Vec<FoundSymbol<'a>> {
    let mut results = Vec::new();
    for root in roots {
        dfs_find(root, path, &mut Vec::new(), &mut results);
    }
    results
}

/// Recursively traverse symbol tree by DFS.
fn dfs_find<'a>(
    symbol: &'a DocumentSymbol,
    path: &SymbolPath,
    ancestor_path: &mut Vec<&'a str>,
    results: &mut Vec<FoundSymbol<'a>>,
) {
    let symbol_name = &symbol.name;

    // Build the current path by adding this symbol
    ancestor_path.push(symbol_name);

    // Check if this symbol matches the search pattern
    if matches_symbol_path(ancestor_path, path) {
        let qualified = ancestor_path.join("/");
        results.push(FoundSymbol {
            symbol,
            qualified_path: qualified,
        });
    }

    // Recurse into children
    if let Some(children) = &symbol.children {
        for child in children {
            dfs_find(child, path, ancestor_path, results);
        }
    }

    // Pop before returning (for next sibling)
    ancestor_path.pop();
}

/// Check if a symbol ancestor chain matches a search pattern.
fn matches_symbol_path(ancestor_chain: &[&str], path: &SymbolPath) -> bool {
    if path.components.is_empty() {
        return false;
    }

    if path.absolute {
        // Absolute match: ancestor chain must match exactly from root
        if ancestor_chain.len() != path.components.len() {
            return false;
        }
        for (i, &name) in ancestor_chain.iter().enumerate() {
            if !path.matches_component(i, name) {
                return false;
            }
        }
        true
    } else {
        // Relative match: the last N components of ancestor_chain
        // must match the path components
        if ancestor_chain.len() < path.components.len() {
            return false;
        }

        let offset = ancestor_chain.len() - path.components.len();
        for (i, &name) in ancestor_chain[offset..].iter().enumerate() {
            if !path.matches_component(i, name) {
                return false;
            }
        }
        true
    }
}

/// Find a unique symbol matching the pattern.
///
/// If multiple symbols match, attempt disambiguation:
/// - Accept if exactly one has its full qualified path equal to the pattern string
/// - Otherwise return an error
pub fn find_unique<'a>(
    roots: &'a [DocumentSymbol],
    path: &SymbolPath,
) -> Result<&'a DocumentSymbol> {
    let matches = find_symbols(roots, path);

    match matches.len() {
        0 => Err(anyhow!(
            "symbol not found: {}",
            path.components.join(&std::path::MAIN_SEPARATOR.to_string())
        )),
        1 => Ok(matches[0].symbol),
        _ => {
            // Try to disambiguate by exact path match
            let pattern_str = path.components.join(&std::path::MAIN_SEPARATOR.to_string());
            let exact_matches: Vec<_> = matches
                .iter()
                .filter(|m| m.qualified_path == pattern_str)
                .collect();

            if exact_matches.len() == 1 {
                Ok(exact_matches[0].symbol)
            } else {
                let mut paths = matches
                    .iter()
                    .map(|m| m.qualified_path.clone())
                    .collect::<Vec<_>>();
                paths.sort();
                Err(anyhow!(
                    "ambiguous symbol '{}': {} matches found:\n  {}",
                    pattern_str,
                    matches.len(),
                    paths.join("\n  ")
                ))
            }
        }
    }
}

/// Convert a line+character position to a string byte index.
///
/// Positions are 0-based (line and character).
/// This handles UTF-8 text by walking line by line.
pub fn position_to_byte_index(text: &str, pos: Position) -> Result<usize> {
    let mut line_num = 0u32;
    let mut byte_idx = 0usize;
    let target_line = pos.line;
    let target_char = pos.character as usize;

    for line in text.lines() {
        if line_num == target_line {
            // Found the target line; now count characters
            let mut char_count = 0;
            for ch in line.chars() {
                if char_count == target_char {
                    return Ok(byte_idx);
                }
                byte_idx += ch.len_utf8();
                char_count += 1;
            }
            // If we've consumed all chars and target_char is at the end
            if char_count == target_char {
                return Ok(byte_idx);
            }
            return Err(anyhow!(
                "character {target_char} out of range for line {target_line} (max {char_count})"
            ));
        }
        // Advance past the line and its ending (\n or \r\n).
        // `line.len()` is the content length (no line ending). We must add
        // the actual number of bytes used by the line ending in the source text.
        byte_idx += line.len();
        let rest = &text.as_bytes()[byte_idx..];
        if rest.starts_with(b"\r\n") {
            byte_idx += 2;
        } else if rest.first() == Some(&b'\n') {
            byte_idx += 1;
        }
        line_num += 1;
    }

    Err(anyhow!("line {target_line} out of range (max {line_num})"))
}

/// Replace text between two positions with new content.
pub fn replace_range(text: &str, range: &Range, replacement: &str) -> Result<String> {
    let start_idx = position_to_byte_index(text, range.start)?;
    let end_idx = position_to_byte_index(text, range.end)?;

    if start_idx > end_idx {
        return Err(anyhow!("invalid range: start > end"));
    }

    Ok(format!(
        "{}{}{}",
        &text[..start_idx],
        replacement,
        &text[end_idx..]
    ))
}

/// Insert text at the beginning of a specific line.
pub fn insert_at_line(text: &str, line: u32, insertion: &str) -> Result<String> {
    let pos = Position { line, character: 0 };
    let byte_idx = position_to_byte_index(text, pos)?;
    Ok(format!(
        "{}{}{}",
        &text[..byte_idx],
        insertion,
        &text[byte_idx..]
    ))
}

/// Format a symbol tree as compact JSON grouped by kind.
///
/// - Groups symbols by their kind
/// - Optionally includes children up to specified depth
/// - Excludes low-level symbols (Variable, Field, etc.) from the primary output
pub fn symbols_overview(symbols: &[DocumentSymbol], depth: usize) -> Value {
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();

    for symbol in symbols {
        // Skip low-level symbols in the overview
        if is_low_level_symbol(&symbol.kind) {
            continue;
        }

        let kind_str = format_symbol_kind(&symbol.kind);
        grouped
            .entry(kind_str)
            .or_default()
            .push(symbol.name.clone());

        // Add children if depth allows
        if depth > 1 {
            if let Some(children) = &symbol.children {
                let child_overview = symbols_overview(children, depth - 1);
                if let Value::Object(child_obj) = child_overview {
                    for (kind, names) in child_obj {
                        if let Value::Array(name_array) = names {
                            grouped.entry(kind).or_default().extend(
                                name_array
                                    .iter()
                                    .filter_map(|v| v.as_str().map(String::from)),
                            );
                        }
                    }
                }
            }
        }
    }

    let mut result = json!({});
    for (kind, mut names) in grouped {
        names.sort();
        names.dedup();
        result[kind] = Value::Array(names.into_iter().map(Value::String).collect());
    }

    result
}

/// Check if a symbol kind is low-level (shouldn't appear in overviews).
const fn is_low_level_symbol(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        &SymbolKind::VARIABLE
            | &SymbolKind::FIELD
            | &SymbolKind::ENUM_MEMBER
            | &SymbolKind::TYPE_PARAMETER
            | &SymbolKind::CONSTANT
    )
}

/// Format a symbol kind as a human-readable string.
#[allow(clippy::all)]
pub fn format_symbol_kind(kind: &SymbolKind) -> String {
    match kind {
        &SymbolKind::FILE => "File",
        &SymbolKind::MODULE => "Module",
        &SymbolKind::NAMESPACE => "Namespace",
        &SymbolKind::PACKAGE => "Package",
        &SymbolKind::CLASS => "Class",
        &SymbolKind::METHOD => "Method",
        &SymbolKind::PROPERTY => "Property",
        &SymbolKind::FIELD => "Field",
        &SymbolKind::CONSTRUCTOR => "Constructor",
        &SymbolKind::ENUM => "Enum",
        &SymbolKind::INTERFACE => "Interface",
        &SymbolKind::FUNCTION => "Function",
        &SymbolKind::VARIABLE => "Variable",
        &SymbolKind::CONSTANT => "Constant",
        &SymbolKind::STRING => "String",
        &SymbolKind::NUMBER => "Number",
        &SymbolKind::BOOLEAN => "Boolean",
        &SymbolKind::ARRAY => "Array",
        &SymbolKind::OBJECT => "Object",
        &SymbolKind::ENUM_MEMBER => "EnumMember",
        &SymbolKind::STRUCT => "Struct",
        &SymbolKind::EVENT => "Event",
        &SymbolKind::OPERATOR => "Operator",
        &SymbolKind::TYPE_PARAMETER => "TypeParameter",
        _ => "Unknown",
    }
    .to_string()
}

/// Extract parameter names from a function signature.
///
/// Given text from after the function name (e.g., "(a: i32, b: &str) -> bool {"),
/// extracts the list of parameter identifiers: ["a", "b"].
///
/// Handles:
/// - Default parameters with `=`
/// - Type annotations with `:`
/// - Nested generics and lifetimes
/// - Empty parameter lists
pub fn extract_param_names(sig_text: &str) -> Result<Vec<String>> {
    let sig_text = sig_text.trim_start();

    // Find opening paren
    if !sig_text.starts_with('(') {
        return Err(anyhow!("signature does not start with '('"));
    }

    // Find matching closing paren by tracking nesting
    let mut depth = 0;
    let mut paren_end = 0;
    for (i, ch) in sig_text.chars().enumerate() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                if depth == 0 {
                    paren_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if paren_end == 0 {
        return Err(anyhow!("unclosed parameter list"));
    }

    let params_str = &sig_text[1..paren_end];
    if params_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut params = Vec::new();

    // Split by comma at depth 0
    let mut current = String::new();
    let mut depth = 0;
    for ch in params_str.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let param_name = extract_param_identifier(&current)?;
                if !param_name.is_empty() {
                    params.push(param_name);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        let param_name = extract_param_identifier(&current)?;
        if !param_name.is_empty() {
            params.push(param_name);
        }
    }

    Ok(params)
}

/// Extract the identifier from a parameter declaration.
///
/// Handles: `a`, `a: Type`, `a: Type = default`, `&mut a`, `*const a`, etc.
fn extract_param_identifier(param: &str) -> Result<String> {
    let param = param.trim();

    // Handle patterns like "&mut", "*const", "*mut", "&"
    let param = if param.starts_with('&') || param.starts_with('*') {
        let chars: Vec<char> = param.chars().collect();
        let mut i = 0;
        while i < chars.len() && (chars[i] == '&' || chars[i] == '*') {
            i += 1;
        }
        // Skip "mut"
        let rest = param[i..].trim();
        rest.strip_prefix("mut ").unwrap_or(rest)
    } else {
        param
    };

    // Split on ':' to separate name from type
    let name = if let Some(colon_idx) = param.find(':') {
        param.get(..colon_idx).map_or(param, |s| s.trim())
    } else {
        param
    };

    // Extract identifier (stop at whitespace or special chars)
    let name = name
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

    if name.is_empty() {
        Ok(String::new())
    } else {
        Ok(name.to_string())
    }
}

/// Extract the body of a function without surrounding braces.
///
/// Returns `(body_text, is_single_expression)` where:
/// - `body_text` is the content between `{` and `}`
/// - `is_single_expression` is true if the body is a single expression (no semicolons at depth 0)
pub fn extract_function_body(body_text: &str) -> Result<(String, bool)> {
    let body_text = body_text.trim_start();

    if !body_text.starts_with('{') {
        return Err(anyhow!("body does not start with '{{'"));
    }

    // Find matching closing brace
    let mut depth = 0;
    let mut brace_end = 0;
    for (i, ch) in body_text.chars().enumerate() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    brace_end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if brace_end == 0 {
        return Err(anyhow!("unclosed function body"));
    }

    let body = &body_text[1..brace_end];

    // Check if it's a single expression by looking for semicolons at depth 0
    let mut depth = 0;
    let mut is_single_expr = true;
    for ch in body.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ';' if depth == 0 => {
                is_single_expr = false;
                break;
            }
            _ => {}
        }
    }

    Ok((body.to_string(), is_single_expr))
}

/// Find the byte range of a function call's argument list.
///
/// Given the end byte offset of a function name, scans forward to find
/// the opening and closing parentheses of the argument list.
///
/// Returns `Some((open_paren_byte, close_paren_byte))` if found, else `None`.
pub fn find_call_args_range(text: &str, name_end_byte: usize) -> Option<(usize, usize)> {
    if name_end_byte >= text.len() {
        return None;
    }

    let bytes = text.as_bytes();

    // Skip whitespace and find opening paren
    let mut i = name_end_byte;
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }

    if i >= bytes.len() || bytes[i] as char != '(' {
        return None;
    }

    let open_paren = i;

    // Find matching closing paren
    let mut depth = 0;
    let mut in_string = false;
    let mut string_delim = ' ';
    let mut escape_next = false;

    while i < bytes.len() {
        let ch = bytes[i] as char;

        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }

        match ch {
            '\\' if in_string => escape_next = true,
            '"' | '\'' if !in_string => {
                in_string = true;
                string_delim = ch;
            }
            '"' | '\'' if in_string && ch == string_delim => {
                in_string = false;
            }
            '(' | '[' | '{' if !in_string => depth += 1,
            ')' | ']' | '}' if !in_string => {
                depth -= 1;
                if depth == 0 && (bytes[i] as char) == ')' {
                    return Some((open_paren, i));
                }
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// Split a comma-separated argument string respecting nesting depth.
///
/// Handles nested parentheses, brackets, braces, and string literals.
/// Returns a vector of trimmed argument strings.
pub fn split_args(args_text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_delim = ' ';
    let mut escape_next = false;

    for ch in args_text.chars() {
        if escape_next {
            escape_next = false;
            current.push(ch);
            continue;
        }

        match ch {
            '\\' if in_string => {
                escape_next = true;
                current.push(ch);
            }
            '"' | '\'' if !in_string => {
                in_string = true;
                string_delim = ch;
                current.push(ch);
            }
            '"' | '\'' if in_string && ch == string_delim => {
                in_string = false;
                current.push(ch);
            }
            '(' | '[' | '{' if !in_string => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' if !in_string => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 && !in_string => {
                let arg = current.trim();
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        let arg = current.trim();
        if !arg.is_empty() {
            args.push(arg.to_string());
        }
    }

    args
}

/// Substitute parameters in function body with argument values.
///
/// Performs word-boundary-aware substitution. If an argument contains
/// top-level binary operators (+, -, *, /, |, &, ^, ?), wraps it in parentheses.
///
/// This is a naive substitution that works without AST parsing by ensuring
/// we only replace complete identifiers (word boundaries).
pub fn substitute_params(body: &str, params: &[&str], args: &[&str]) -> String {
    if params.len() != args.len() {
        return body.to_string();
    }

    let mut result = body.to_string();

    for (param, arg) in params.iter().zip(args.iter()) {
        // If arg contains binary operators at depth 0, wrap in parens
        let wrapped_arg = if needs_parentheses(arg) {
            format!("({arg})")
        } else {
            arg.to_string()
        };

        // Replace all occurrences of the parameter name with word-boundary checks
        result = substitute_with_word_boundaries(&result, param, &wrapped_arg);
    }

    result
}

/// Check if an argument needs parentheses when substituted.
///
/// Needs parens if it contains binary operators at depth 0.
fn needs_parentheses(arg: &str) -> bool {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_delim = ' ';
    let mut escape_next = false;

    let dangerous_ops = ['+', '-', '*', '/', '|', '&', '^', '?'];

    for ch in arg.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_string => escape_next = true,
            '"' | '\'' if !in_string => {
                in_string = true;
                string_delim = ch;
            }
            '"' | '\'' if in_string && ch == string_delim => {
                in_string = false;
            }
            '(' | '[' | '{' if !in_string => depth += 1,
            ')' | ']' | '}' if !in_string => depth -= 1,
            _ if depth == 0 && !in_string && dangerous_ops.contains(&ch) => {
                return true;
            }
            _ => {}
        }
    }

    false
}

/// Replace all occurrences of `param` with `replacement` using word boundaries.
fn substitute_with_word_boundaries(text: &str, param: &str, replacement: &str) -> String {
    let mut result = String::new();
    let param_bytes = param.as_bytes();
    let text_bytes = text.as_bytes();
    let mut i = 0;

    while i < text_bytes.len() {
        // Check if we have a match starting at position i
        if i + param_bytes.len() <= text_bytes.len()
            && &text_bytes[i..i + param_bytes.len()] == param_bytes
        {
            // Check word boundaries
            let before_ok = i == 0 || !is_identifier_char(text_bytes[i - 1] as char);
            let after_ok = i + param_bytes.len() >= text_bytes.len()
                || !is_identifier_char(text_bytes[i + param_bytes.len()] as char);

            if before_ok && after_ok {
                result.push_str(replacement);
                i += param_bytes.len();
                continue;
            }
        }

        result.push(text_bytes[i] as char);
        i += 1;
    }

    result
}

/// Check if a character can be part of an identifier.
fn is_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_path_parse_relative() {
        let p = SymbolPath::parse("MyClass/my_method");
        assert!(!p.absolute);
        assert_eq!(p.components, vec!["MyClass", "my_method"]);
        assert!(p.overload_idx.is_none());
    }

    #[test]
    fn test_symbol_path_parse_absolute() {
        let p = SymbolPath::parse("/MyClass/my_method");
        assert!(p.absolute);
        assert_eq!(p.components, vec!["MyClass", "my_method"]);
    }

    #[test]
    fn test_symbol_path_parse_overload() {
        let p = SymbolPath::parse("method[1]");
        assert_eq!(p.components, vec!["method"]);
        assert_eq!(p.overload_idx, Some(1));
    }

    #[test]
    fn test_position_to_byte_index_simple() {
        let text = "hello\nworld";
        let pos = Position {
            line: 0,
            character: 2,
        };
        let idx = position_to_byte_index(text, pos).unwrap();
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_position_to_byte_index_second_line() {
        let text = "hello\nworld";
        let pos = Position {
            line: 1,
            character: 2,
        };
        let idx = position_to_byte_index(text, pos).unwrap();
        assert_eq!(idx, 8); // 5 chars + newline + 2 = 8
    }

    #[test]
    fn test_position_to_byte_index_crlf() {
        // CRLF line endings: \r\n is 2 bytes per newline
        let text = "hello\r\nworld";
        let pos = Position {
            line: 1,
            character: 2,
        };
        let idx = position_to_byte_index(text, pos).unwrap();
        assert_eq!(idx, 9); // 5 chars + 2 byte CRLF + 2 = 9
    }

    #[test]
    fn test_position_to_byte_index_crlf_third_line() {
        let text = "one\r\ntwo\r\nthree";
        let pos = Position {
            line: 2,
            character: 3,
        };
        let idx = position_to_byte_index(text, pos).unwrap();
        // "one" (3) + CRLF (2) + "two" (3) + CRLF (2) + 3 chars = 13
        assert_eq!(idx, 13);
    }

    #[test]
    fn test_replace_range_basic() {
        let text = "fn foo() {\n    println!(\"hi\");\n}";
        let range = Range {
            start: Position {
                line: 1,
                character: 4,
            },
            end: Position {
                line: 1,
                character: 19,
            },
        };
        let result = replace_range(text, &range, "// replaced").unwrap();
        assert!(result.contains("// replaced"));
    }

    #[test]
    fn test_insert_at_line_start() {
        let text = "line1\nline2\nline3";
        let result = insert_at_line(text, 1, "inserted\n").unwrap();
        assert!(result.contains("inserted"));
    }

    #[test]
    #[allow(deprecated)]
    fn test_symbols_overview_filters_low_level() {
        let symbols = vec![DocumentSymbol {
            name: "MyVar".to_string(),
            kind: SymbolKind::VARIABLE,
            detail: None,
            deprecated: None,
            tags: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
            children: None,
        }];

        let overview = symbols_overview(&symbols, 1);
        // Low-level Variable should be filtered out
        assert!(overview.as_object().unwrap().is_empty());
    }

    #[test]
    #[allow(deprecated)]
    fn test_symbols_overview_includes_high_level() {
        let symbols = vec![DocumentSymbol {
            name: "MyClass".to_string(),
            kind: SymbolKind::CLASS,
            detail: None,
            deprecated: None,
            tags: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 0,
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 7,
                },
            },
            children: None,
        }];

        let overview = symbols_overview(&symbols, 1);
        let obj = overview.as_object().unwrap();
        assert!(obj.contains_key("Class"));
    }

    // Tests for extract_param_names
    #[test]
    fn test_extract_param_names_simple() {
        let sig = "(a: i32, b: String)";
        let params = extract_param_names(sig).unwrap();
        assert_eq!(params, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_param_names_with_references() {
        let sig = "(&mut a: i32, &b: String)";
        let params = extract_param_names(sig).unwrap();
        assert_eq!(params, vec!["a", "b"]);
    }

    #[test]
    fn test_extract_param_names_empty() {
        let sig = "()";
        let params = extract_param_names(sig).unwrap();
        assert!(params.is_empty());
    }

    #[test]
    fn test_extract_param_names_nested_generics() {
        let sig = "(a: Vec<HashMap<String, i32>>, b: Fn(i32) -> String)";
        let params = extract_param_names(sig).unwrap();
        assert_eq!(params, vec!["a", "b"]);
    }

    // Tests for extract_function_body
    #[test]
    fn test_extract_function_body_single_expr() {
        let body = "{ x + y }";
        let (_content, is_single) = extract_function_body(body).unwrap();
        assert!(is_single);
    }

    #[test]
    fn test_extract_function_body_multiple_stmts() {
        let body = "{ let x = 1; x + 2 }";
        let (_content, is_single) = extract_function_body(body).unwrap();
        assert!(!is_single);
    }

    // Tests for find_call_args_range
    #[test]
    fn test_find_call_args_range_simple() {
        let text = "foo(a, b)";
        let range = find_call_args_range(text, 3).unwrap();
        assert_eq!(range, (3, 8));
    }

    #[test]
    fn test_find_call_args_range_with_whitespace() {
        let text = "foo  (a, b)";
        let range = find_call_args_range(text, 3).unwrap();
        assert_eq!(range, (5, 10));
    }

    #[test]
    fn test_find_call_args_range_nested() {
        let text = "foo(bar(x), y)";
        let range = find_call_args_range(text, 3).unwrap();
        assert_eq!(range, (3, 13));
    }

    // Tests for split_args
    #[test]
    fn test_split_args_simple() {
        let args = "a, b, c";
        let split = split_args(args);
        assert_eq!(split, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_split_args_with_nesting() {
        let args = "foo(x, y), bar[z]";
        let split = split_args(args);
        assert_eq!(split, vec!["foo(x, y)", "bar[z]"]);
    }

    // Tests for substitute_params
    #[test]
    fn test_substitute_params_simple() {
        let body = "x + y";
        let params = vec!["x", "y"];
        let args = vec!["2", "3"];
        let result = substitute_params(body, &params, &args);
        assert_eq!(result, "2 + 3");
    }

    #[test]
    fn test_substitute_params_with_parentheses() {
        let body = "x + y";
        let params = vec!["x", "y"];
        let args = vec!["a + b", "c"];
        let result = substitute_params(body, &params, &args);
        assert_eq!(result, "(a + b) + c");
    }

    #[test]
    fn test_substitute_params_word_boundary() {
        let body = "xxx + x";
        let params = vec!["x"];
        let args = vec!["2"];
        let result = substitute_params(body, &params, &args);
        assert_eq!(result, "xxx + 2");
    }

    #[test]
    fn test_needs_parentheses_with_operators() {
        assert!(needs_parentheses("a + b"));
        assert!(needs_parentheses("x - y"));
        assert!(needs_parentheses("p & q"));
        assert!(!needs_parentheses("simple"));
        assert!(!needs_parentheses("func()"));
    }
}
