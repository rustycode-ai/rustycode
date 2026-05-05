use std::collections::HashMap;

// A minimal, pure-Rust frontmatter representation and parser.
// This module intentionally avoids any external YAML dependency and
// supports the subset of frontmatter needed by RustyCode: strings, booleans,
// numbers, arrays (inline and multi-line), and simple nesting via objects.

#[derive(Debug, Clone, PartialEq)]
pub enum FrontmatterValue {
    String(String),
    Bool(bool),
    Number(i64),
    Array(Vec<FrontmatterValue>),
    Object(FrontmatterMap),
}

pub type FrontmatterMap = HashMap<String, FrontmatterValue>;

/// Splits a content blob into optional (YAML frontmatter, body).
/// Body is everything after the closing `---` delimiter.
/// Returns `None` if no valid frontmatter block is found.
pub fn split_frontmatter(content: &str) -> Option<(String, String)> {
    let mut lines = content.lines();
    // First line must be a delimiter
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }
    let mut yaml_lines: Vec<String> = Vec::new();
    let mut body_lines: Vec<String> = Vec::new();
    let mut found_close = false;

    for line in lines {
        if found_close {
            body_lines.push(line.to_string());
        } else if line.trim() == "---" {
            found_close = true;
        } else {
            yaml_lines.push(line.to_string());
        }
    }

    if !found_close {
        return None;
    }

    Some((yaml_lines.join("\n"), body_lines.join("\n")))
}

/// Minimal frontmatter parser.
/// Supports:
/// - top-level key: value (string/bool/number)
/// - multi-line arrays with "- item" syntax
/// - inline arrays like ["a", "b"]
pub fn parse_frontmatter_map(yaml: &str) -> FrontmatterMap {
    let mut map: FrontmatterMap = FrontmatterMap::new();
    let mut current_key: Option<String> = None;
    for raw in yaml.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Array item continuation
        if let Some(stripped) = trimmed.strip_prefix("- ") {
            if let Some(ref key) = current_key {
                // Ensure an Array exists for this key
                map.entry(key.clone())
                    .or_insert(FrontmatterValue::Array(Vec::new()));
                if let Some(FrontmatterValue::Array(arr)) = map.get_mut(key) {
                    arr.push(parse_scalar(stripped.trim()));
                }
            }
            continue;
        }
        // Key: value
        if let Some(pos) = trimmed.find(':') {
            let key = trimmed[..pos].trim().to_string();
            let val = trimmed[pos + 1..].trim();
            // Empty value means array/map follows on next lines
            if val.is_empty() {
                current_key = Some(key);
                continue;
            }
            // Inline array
            if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len() - 1];
                let mut items: Vec<FrontmatterValue> = Vec::new();
                for part in inner.split(',') {
                    let part = part.trim();
                    if part.is_empty() {
                        continue;
                    }
                    items.push(parse_scalar(part));
                }
                map.insert(key.clone(), FrontmatterValue::Array(items));
                current_key = Some(key);
                continue;
            } else {
                let value = parse_scalar(val);
                map.insert(key.clone(), value);
                current_key = Some(key);
                continue;
            }
        }
        // Lines without a colon are ignored in this lightweight parser.
    }
    map
}

fn parse_scalar(token: &str) -> FrontmatterValue {
    let t = token.trim();
    if t.eq_ignore_ascii_case("true") {
        FrontmatterValue::Bool(true)
    } else if t.eq_ignore_ascii_case("false") {
        FrontmatterValue::Bool(false)
    } else if let Ok(n) = t.parse::<i64>() {
        FrontmatterValue::Number(n)
    } else {
        // Strip surrounding quotes if present
        let mut s = t.to_string();
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            s = s[1..s.len() - 1].to_string();
        }
        FrontmatterValue::String(s)
    }
}

/// Expand brace patterns in a path string.
/// `"src/*.{ts,tsx}"` → `["src/*.ts", "src/*.tsx"]`
/// `"a{b,c}d{e,f}"` → `["abde", "abdf", "acde", "acdf"]`
/// Nested braces are supported.
fn expand_braces(pattern: &str) -> Vec<String> {
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };

    // Find matching closing brace (respecting nesting)
    let mut depth = 0i32;
    let mut close = None;
    for (i, ch) in pattern[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }

    let Some(close) = close else {
        // Unmatched brace — return as-is
        return vec![pattern.to_string()];
    };

    let prefix = &pattern[..open];
    let suffix = &pattern[close + '}'.len_utf8()..];
    let inner = &pattern[open + '{'.len_utf8()..close];

    let alternatives = split_by_comma(inner);

    let mut results = Vec::new();
    for alt in alternatives {
        let combined = format!("{prefix}{alt}{suffix}");
        results.extend(expand_braces(&combined));
    }

    results
}

/// Split a string by commas, respecting nested brace groups.
fn split_by_comma(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;

    for (i, ch) in s.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                result.push(s[start..i].trim());
                start = i + ','.len_utf8();
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        result.push(last);
    }
    result
}

/// Normalize frontmatter paths: expand brace patterns and split comma-separated entries.
///
/// Handles:
/// - Brace expansion: `"src/*.{ts,tsx}"` → `["src/*.ts", "src/*.tsx"]`
/// - Comma-separated: `"*.rs, *.ts"` → `["*.rs", "*.ts"]`
/// - Combined: `"src/*.{ts,tsx}, *.json"` → `["src/*.ts", "src/*.tsx", "*.json"]`
pub fn normalize_paths(paths: &[String]) -> Vec<String> {
    let mut expanded = Vec::new();
    for path in paths {
        for part in split_by_comma(path) {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            expanded.extend(expand_braces(trimmed));
        }
    }
    expanded
}

/// Convenience helpers for consumers of FrontmatterValue.
pub fn as_string(v: &FrontmatterValue) -> Option<String> {
    if let FrontmatterValue::String(s) = v {
        Some(s.clone())
    } else {
        None
    }
}

pub fn as_bool(v: &FrontmatterValue) -> Option<bool> {
    if let FrontmatterValue::Bool(b) = v {
        Some(*b)
    } else {
        None
    }
}

pub fn as_array(v: &FrontmatterValue) -> Option<Vec<FrontmatterValue>> {
    if let FrontmatterValue::Array(arr) = v {
        Some(arr.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter_basic() {
        let input = "---\nname: test\nversion: 1\n---\nbody content";
        let result = split_frontmatter(input);
        assert_eq!(
            result,
            Some((
                "name: test\nversion: 1".to_string(),
                "body content".to_string()
            ))
        );
    }

    #[test]
    fn test_split_frontmatter_extracts_body() {
        let input = "---\nname: test\n---\n# Title\n\nSome body text.\n";
        let (fm, body) = split_frontmatter(input).unwrap();
        assert_eq!(fm, "name: test");
        assert!(body.contains("# Title"));
        assert!(body.contains("Some body text."));
        assert!(!body.contains("name:"));
    }

    #[test]
    fn test_split_frontmatter_no_delimiter() {
        let input = "no frontmatter here\njust content";
        assert!(split_frontmatter(input).is_none());
    }

    #[test]
    fn test_split_frontmatter_unclosed() {
        let input = "---\nname: test\nno closing delimiter";
        assert!(split_frontmatter(input).is_none());
    }

    #[test]
    fn test_parse_simple_key_value() {
        let yaml = "name: hello\nversion: 42";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(
            as_string(map.get("name").unwrap()).as_deref(),
            Some("hello")
        );
        assert_eq!(map.get("version").unwrap(), &FrontmatterValue::Number(42));
    }

    #[test]
    fn test_parse_quoted_string() {
        let yaml = "title: \"hello world\"";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(
            as_string(map.get("title").unwrap()).as_deref(),
            Some("hello world")
        );
    }

    #[test]
    fn test_parse_single_quoted_string() {
        let yaml = "title: 'single quoted'";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(
            as_string(map.get("title").unwrap()).as_deref(),
            Some("single quoted")
        );
    }

    #[test]
    fn test_parse_glob_pattern_unquoted() {
        let yaml = "paths:\n  - \"**/*.rs\"\n  - \"*.ts\"";
        let map = parse_frontmatter_map(yaml);
        let arr = as_array(map.get("paths").unwrap()).unwrap();
        assert_eq!(as_string(&arr[0]).as_deref(), Some("**/*.rs"));
        assert_eq!(as_string(&arr[1]).as_deref(), Some("*.ts"));
    }

    #[test]
    fn test_parse_inline_array() {
        let yaml = "tags: [\"a\", \"b\", \"c\"]";
        let map = parse_frontmatter_map(yaml);
        let arr = as_array(map.get("tags").unwrap()).unwrap();
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn test_parse_multiline_array() {
        let yaml = "items:\n  - first\n  - second\n  - third";
        let map = parse_frontmatter_map(yaml);
        let arr = as_array(map.get("items").unwrap()).unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(as_string(&arr[0]).as_deref(), Some("first"));
    }

    #[test]
    fn test_parse_booleans() {
        let yaml = "enabled: true\ndisabled: false";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(as_bool(map.get("enabled").unwrap()), Some(true));
        assert_eq!(as_bool(map.get("disabled").unwrap()), Some(false));
    }

    #[test]
    fn test_parse_comments_ignored() {
        let yaml = "# comment\nkey: value";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(map.len(), 1);
        assert_eq!(as_string(map.get("key").unwrap()).as_deref(), Some("value"));
    }

    #[test]
    fn test_parse_empty_input() {
        let map = parse_frontmatter_map("");
        assert!(map.is_empty());
    }

    #[test]
    fn test_parse_negative_number() {
        let yaml = "offset: -42";
        let map = parse_frontmatter_map(yaml);
        assert_eq!(map.get("offset").unwrap(), &FrontmatterValue::Number(-42));
    }

    #[test]
    fn test_as_string_on_non_string() {
        let v = FrontmatterValue::Bool(true);
        assert!(as_string(&v).is_none());
    }

    #[test]
    fn test_as_bool_on_non_bool() {
        let v = FrontmatterValue::String("true".to_string());
        assert!(as_bool(&v).is_none());
    }

    #[test]
    fn test_as_array_on_non_array() {
        let v = FrontmatterValue::Number(42);
        assert!(as_array(&v).is_none());
    }

    // --- Brace expansion tests ---

    #[test]
    fn test_expand_braces_single_group() {
        let result = expand_braces("src/*.{ts,tsx}");
        assert_eq!(result, vec!["src/*.ts", "src/*.tsx"]);
    }

    #[test]
    fn test_expand_braces_multiple_groups() {
        let result = expand_braces("a{b,c}d{e,f}");
        assert_eq!(result, vec!["abde", "abdf", "acde", "acdf"]);
    }

    #[test]
    fn test_expand_braces_no_braces() {
        let result = expand_braces("**/*.rs");
        assert_eq!(result, vec!["**/*.rs"]);
    }

    #[test]
    fn test_expand_braces_unmatched() {
        let result = expand_braces("src/*.{ts");
        assert_eq!(result, vec!["src/*.{ts"]);
    }

    #[test]
    fn test_expand_braces_nested() {
        let result = expand_braces("{a,{b,c}}");
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_expand_braces_single_alt() {
        let result = expand_braces("src/{ts}");
        assert_eq!(result, vec!["src/ts"]);
    }

    #[test]
    fn test_expand_braces_three_alts() {
        let result = expand_braces("*.{ts,tsx,js}");
        assert_eq!(result, vec!["*.ts", "*.tsx", "*.js"]);
    }

    // --- Path normalization tests ---

    #[test]
    fn test_normalize_paths_brace_expansion() {
        let paths = vec!["src/*.{ts,tsx}".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["src/*.ts", "src/*.tsx"]);
    }

    #[test]
    fn test_normalize_paths_comma_separated() {
        let paths = vec!["*.rs, *.ts".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["*.rs", "*.ts"]);
    }

    #[test]
    fn test_normalize_paths_mixed() {
        let paths = vec!["src/*.{ts,tsx}, *.json".to_string(), "**/*.rs".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["src/*.ts", "src/*.tsx", "*.json", "**/*.rs"]);
    }

    #[test]
    fn test_normalize_paths_no_expansion_needed() {
        let paths = vec!["**/*.rs".to_string(), "*.toml".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["**/*.rs", "*.toml"]);
    }

    #[test]
    fn test_normalize_paths_empty() {
        let result = normalize_paths(&[]);
        assert!(result.is_empty());
    }

    // --- Edge case tests ---

    #[test]
    fn test_expand_braces_empty_alternative() {
        let result = expand_braces("src/{ts,}");
        assert_eq!(result, vec!["src/ts"]);
    }

    #[test]
    fn test_expand_braces_consecutive() {
        let result = expand_braces("a{b}c{d}e");
        assert_eq!(result, vec!["abcde"]);
    }

    #[test]
    fn test_normalize_paths_whitespace_handling() {
        let paths = vec!["  *.rs  ,  *.ts  ".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["*.rs", "*.ts"]);
    }

    #[test]
    fn test_normalize_paths_trailing_comma() {
        let paths = vec!["*.rs,".to_string()];
        let result = normalize_paths(&paths);
        assert_eq!(result, vec!["*.rs"]);
    }

    #[test]
    fn test_split_frontmatter_empty_body() {
        let input = "---\nkey: val\n---\n";
        let (fm, body) = split_frontmatter(input).unwrap();
        assert_eq!(fm, "key: val");
        assert!(body.trim().is_empty());
    }

    #[test]
    fn test_split_frontmatter_body_with_dashes() {
        let input = "---\nkey: val\n---\nSome --- text\n";
        let (fm, body) = split_frontmatter(input).unwrap();
        assert_eq!(fm, "key: val");
        assert!(body.contains("Some --- text"));
    }

    #[test]
    fn test_parse_number_in_array() {
        let yaml = "items:\n  - 1\n  - 2\n  - 3";
        let map = parse_frontmatter_map(yaml);
        let arr = as_array(map.get("items").unwrap()).unwrap();
        assert_eq!(arr[0], FrontmatterValue::Number(1));
        assert_eq!(arr[2], FrontmatterValue::Number(3));
    }

    #[test]
    fn test_parse_mixed_array() {
        let yaml = "items:\n  - hello\n  - 42\n  - true";
        let map = parse_frontmatter_map(yaml);
        let arr = as_array(map.get("items").unwrap()).unwrap();
        assert_eq!(arr[0], FrontmatterValue::String("hello".to_string()));
        assert_eq!(arr[1], FrontmatterValue::Number(42));
        assert_eq!(arr[2], FrontmatterValue::Bool(true));
    }
}
