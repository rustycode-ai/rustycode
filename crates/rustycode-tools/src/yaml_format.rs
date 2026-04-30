//! YAML multiline string formatting utilities.
//!
//! Ported from goose's `recipe/yaml_format_utils.rs`.
//!
//! When `serde_yaml` serializes multi-line strings, it may use quoted format
//! with escaped `\n` and `\"` characters. This is problematic for template
//! engines like Handlebars or `MiniJinja`, which expect literal block scalars
//! (using `|` syntax).
//!
//! This module provides a function to reformat specific YAML fields from
//! escaped quoted format to literal block scalar format.
//!
//! ## Example
//!
//! ```
//! use rustycode_tools::yaml_format::reformat_fields_with_multiline_values;
//!
//! let yaml = "prompt: \"line1\\\\nline2\"";
//! let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
//! assert!(result.contains("prompt: |\n"));
//! assert!(result.contains("  line1\n"));
//! ```

use std::fmt::Write;

/// Normalizes YAML multi-line string fields from quoted escaped format
/// to literal block scalar format.
///
/// `serde_yaml` may serialize multi-line strings as quoted values with
/// escaped newlines (`\\n`) and quotes (`\\\"`). This function converts
/// specified fields to use YAML's literal block scalar (`|`) syntax instead,
/// which is more readable and compatible with template engines.
///
/// # Arguments
///
/// * `yaml` - The YAML string to reformat
/// * `multiline_fields` - Field names that should be converted to block scalars
///
/// # Example
///
/// Before:
/// ```text
/// prompt: "Hello \"World\"\n{% if admin %}Welcome{% endif %}"
/// ```
///
/// After:
/// ```text
/// prompt: |
///   Hello "World"
///   {% if admin %}Welcome{% endif %}
/// ```
pub fn reformat_fields_with_multiline_values(yaml: &str, multiline_fields: &[&str]) -> String {
    let mut result = String::new();

    for line in yaml.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            writeln!(result).unwrap();
            continue;
        }

        let indent = line.len() - trimmed.len();
        let indent_str = " ".repeat(indent);

        let matched_field = multiline_fields
            .iter()
            .find(|&f| trimmed.starts_with(&format!("{f}: ")));

        if let Some(field) = matched_field {
            if let Some((_, raw_val)) = trimmed.split_once(": ") {
                if raw_val.contains("\\n") {
                    // Clean escaped content and unescape quotes
                    let mut value = raw_val.trim_matches('"').to_string();

                    // Unescape quotes and double backslashes
                    value = value.replace("\\\"", "\"").replace("\\\\n", "\\n");

                    writeln!(result, "{indent_str}{field}: |").unwrap();
                    for l in value.split("\\n") {
                        writeln!(result, "{indent_str}  {l}").unwrap();
                    }
                    continue;
                }
            }
        }

        writeln!(result, "{line}").unwrap();
    }

    let mut output = result.trim_end_matches('\n').to_string();
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_simple_fields_unchanged() {
        let yaml = "version: \"1.0\"\ntitle: \"Simple\"\nprompt: \"Hello\"";
        let expected = "version: \"1.0\"\ntitle: \"Simple\"\nprompt: \"Hello\"\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn converts_multiline_prompt_to_literal_block() {
        let yaml = "version: \"1.0\"\nprompt: \"line1\\\\nline2\"";
        let expected = "version: \"1.0\"\nprompt: |\n  line1\n  line2\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn unescapes_quotes_inside_block() {
        let yaml = "prompt: \"Hello \\\"World\\\"\\nHow are you?\"";
        let expected = "prompt: |\n  Hello \"World\"\n  How are you?\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn preserves_unlisted_fields() {
        let yaml = "version: \"1.0\"\nprompt: \"line1\\\\nline2\"\nnotes: \"note1\\\\nnote2\"";
        let expected =
            "version: \"1.0\"\nprompt: |\n  line1\n  line2\nnotes: \"note1\\\\nnote2\"\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn handles_indented_nested_field() {
        let yaml = "settings:\n  prompt: \"line1\\\\nline2\"";
        let expected = "settings:\n  prompt: |\n    line1\n    line2\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn ignores_existing_literal_blocks() {
        let yaml = "prompt: |\n  already good\n  block";
        let expected = "prompt: |\n  already good\n  block\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn ignores_fields_without_newlines() {
        let yaml = "prompt: \"single line text\"";
        let expected = "prompt: \"single line text\"\n";

        let result = reformat_fields_with_multiline_values(yaml, &["prompt"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn handles_empty_yaml() {
        let result = reformat_fields_with_multiline_values("", &["prompt"]);
        assert_eq!(result, "\n");
    }

    #[test]
    fn handles_multiple_multiline_fields() {
        let yaml = "prompt: \"line1\\\\nline2\"\ninstructions: \"step1\\\\nstep2\"";
        let result = reformat_fields_with_multiline_values(yaml, &["prompt", "instructions"]);

        assert!(result.contains("prompt: |"));
        assert!(result.contains("instructions: |"));
        assert!(result.contains("  line1"));
        assert!(result.contains("  step1"));
    }
}
