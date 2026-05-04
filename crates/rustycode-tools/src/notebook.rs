//! Jupyter Notebook Parser
//!
//! Parses `.ipynb` files (JSON-based Jupyter notebook format) into a
//! human-readable text representation suitable for LLM consumption.
//!
//! Uses `serde_json` for parsing — no additional notebook crate dependency.

use anyhow::{Context, Result};

/// Parse a Jupyter notebook (`.ipynb`) file and return formatted text.
///
/// Extracts cells from the notebook JSON and formats each one based on
/// its type (markdown, code, raw). Code cell outputs (stdout, stderr,
/// execution results, errors) are included after each code cell.
pub fn parse_notebook(content: &str) -> Result<String> {
    let nb: serde_json::Value = serde_json::from_str(content)
        .with_context(|| "failed to parse notebook JSON")?;

    let cells = nb
        .get("cells")
        .and_then(|c| c.as_array())
        .with_context(|| "notebook missing 'cells' array")?;

    let mut output = Vec::new();
    let mut code_cell_index = 0;

    for cell in cells {
        let cell_type = cell
            .get("cell_type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");

        let source_lines = extract_source_lines(cell);

        match cell_type {
            "markdown" => {
                output.push("### Markdown Cell ###".to_string());
                output.push(source_lines.join("\n"));
            }
            "code" => {
                code_cell_index += 1;
                let language = extract_language(cell);
                let header = format!("### Code Cell [{code_cell_index}] ###");

                let fenced = if language.is_empty() {
                    format!("```\n{}\n```", source_lines.join("\n"))
                } else {
                    format!("```{language}\n{}\n```", source_lines.join("\n"))
                };

                output.push(header);
                output.push(fenced);

                if let Some(outputs_text) = format_outputs(cell) {
                    output.push(outputs_text);
                }
            }
            "raw" => {
                output.push("### Raw Cell ###".to_string());
                output.push(source_lines.join("\n"));
            }
            _ => {
                output.push(format!("### {cell_type} Cell ###"));
                output.push(source_lines.join("\n"));
            }
        }
    }

    Ok(output.join("\n\n"))
}

/// Extract source lines from a cell's `source` field.
///
/// The `source` field can be either a string or an array of strings.
fn extract_source_lines(cell: &serde_json::Value) -> Vec<String> {
    cell.get("source")
        .map(|src| match src.as_str() {
            Some(s) => s.lines().map(|l| l.to_string()).collect(),
            None => src
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.trim_end_matches('\n').to_string()))
                        .collect()
                })
                .unwrap_or_default(),
        })
        .unwrap_or_default()
}

/// Extract the language identifier from a code cell's metadata.
fn extract_language(cell: &serde_json::Value) -> String {
    cell.get("metadata")
        .and_then(|m| m.get("language_info"))
        .and_then(|li| li.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            cell.get("metadata")
                .and_then(|m| m.get("pygments_lexer"))
                .and_then(|l| l.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

/// Format the outputs of a code cell into readable text.
fn format_outputs(cell: &serde_json::Value) -> Option<String> {
    let outputs = cell.get("outputs")?.as_array()?;
    if outputs.is_empty() {
        return None;
    }

    let mut parts = Vec::new();

    for output in outputs {
        let output_type = output
            .get("output_type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        match output_type {
            "stream" => {
                let name = output
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("stdout");
                let text = output
                    .get("text")
                    .map(extract_text_value)
                    .unwrap_or_default();
                if !text.is_empty() {
                    let prefix = if name == "stderr" {
                        "[stderr]"
                    } else {
                        "[stdout]"
                    };
                    parts.push(format!("{prefix} {text}"));
                }
            }
            "execute_result" | "display_data" => {
                if let Some(data) = output.get("data") {
                    if let Some(text_plain) = data.get("text/plain") {
                        let text = extract_text_value(text_plain);
                        if !text.is_empty() {
                            parts.push(text);
                        }
                    }
                }
            }
            "error" => {
                let ename = output
                    .get("ename")
                    .and_then(|e| e.as_str())
                    .unwrap_or("Error");
                let evalue = output
                    .get("evalue")
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                parts.push(format!("[error] {ename}: {evalue}"));

                if let Some(traceback) = output.get("traceback").and_then(|t| t.as_array()) {
                    let tb_lines: Vec<&str> = traceback
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect();
                    if !tb_lines.is_empty() {
                        parts.push(format!("[error] {}", tb_lines.join("\n")));
                    }
                }
            }
            _ => {}
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Extract text from a JSON value that may be a string or array of strings.
fn extract_text_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<&str>>()
            .join(""),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_notebook(cells: &[(&str, &str, Option<serde_json::Value>)]) -> String {
        let cells_json: Vec<serde_json::Value> = cells
            .iter()
            .map(|(cell_type, source, outputs)| {
                let mut cell = serde_json::json!({
                    "cell_type": cell_type,
                    "source": [source],
                    "metadata": {}
                });
                if let Some(out) = outputs {
                    cell["outputs"] = out.clone();
                }
                cell
            })
            .collect();

        serde_json::json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": cells_json
        })
        .to_string()
    }

    #[test]
    fn test_parse_simple_notebook() {
        let notebook = make_notebook(&[(
            "code",
            "print('hello')",
            Some(serde_json::json!([
                {
                    "output_type": "stream",
                    "name": "stdout",
                    "text": ["hello\n"]
                }
            ])),
        )]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("### Code Cell [1] ###"));
        assert!(result.contains("print('hello')"));
        assert!(result.contains("[stdout] hello"));
    }

    #[test]
    fn test_parse_markdown_cell() {
        let notebook = make_notebook(&[("markdown", "# Title\nSome text", None)]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("### Markdown Cell ###"));
        assert!(result.contains("# Title"));
    }

    #[test]
    fn test_parse_error_output() {
        let notebook = make_notebook(&[(
            "code",
            "1/0",
            Some(serde_json::json!([
                {
                    "output_type": "error",
                    "ename": "ZeroDivisionError",
                    "evalue": "division by zero",
                    "traceback": ["ZeroDivisionError: division by zero"]
                }
            ])),
        )]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("[error] ZeroDivisionError: division by zero"));
    }

    #[test]
    fn test_parse_empty_notebook() {
        let notebook = serde_json::json!({
            "nbformat": 4,
            "cells": []
        })
        .to_string();

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = parse_notebook("not valid json {{{");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("failed to parse notebook JSON"));
    }

    #[test]
    fn test_parse_execute_result() {
        let notebook = make_notebook(&[(
            "code",
            "2 + 2",
            Some(serde_json::json!([
                {
                    "output_type": "execute_result",
                    "data": {"text/plain": ["4"]},
                    "metadata": {},
                    "execution_count": 1
                }
            ])),
        )]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("4"));
    }

    #[test]
    fn test_parse_raw_cell() {
        let notebook = make_notebook(&[("raw", "Some raw content", None)]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("### Raw Cell ###"));
        assert!(result.contains("Some raw content"));
    }

    #[test]
    fn test_parse_multiple_code_cells_numbered() {
        let notebook = make_notebook(&[
            ("code", "a = 1", None),
            ("code", "b = 2", None),
            ("code", "c = a + b", None),
        ]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("### Code Cell [1] ###"));
        assert!(result.contains("### Code Cell [2] ###"));
        assert!(result.contains("### Code Cell [3] ###"));
    }

    #[test]
    fn test_parse_with_language() {
        let notebook = serde_json::json!({
            "nbformat": 4,
            "cells": [{
                "cell_type": "code",
                "source": ["x = 42"],
                "metadata": {
                    "language_info": {"name": "python"}
                },
                "outputs": []
            }]
        })
        .to_string();

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("```python"));
    }

    #[test]
    fn test_parse_missing_cells_field() {
        let notebook = serde_json::json!({
            "nbformat": 4,
            "metadata": {}
        })
        .to_string();

        let result = parse_notebook(&notebook);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing 'cells'"));
    }

    #[test]
    fn test_parse_stderr_output() {
        let notebook = make_notebook(&[(
            "code",
            "import sys; sys.stderr.write('warning\\n')",
            Some(serde_json::json!([
                {
                    "output_type": "stream",
                    "name": "stderr",
                    "text": ["warning\n"]
                }
            ])),
        )]);

        let result = parse_notebook(&notebook).unwrap();
        assert!(result.contains("[stderr] warning"));
    }
}
