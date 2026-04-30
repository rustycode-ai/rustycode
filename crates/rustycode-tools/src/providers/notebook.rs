use crate::{Tool, ToolContext, ToolOutput, ToolPermission};
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::fs;

pub struct NotebookEditTool;

impl Tool for NotebookEditTool {
    fn name(&self) -> &'static str {
        "notebook_edit"
    }

    fn description(&self) -> &'static str {
        r#"Completely replaces the contents of a specific cell in a Jupyter notebook (.ipynb file) with new source.
Jupyter notebooks are interactive documents that combine code, text, and visualizations.
The notebook_path parameter must be an absolute path, not a relative path.
The cell_number is 0-indexed. Use edit_mode=insert to add a new cell at the index specified by cell_number.
Use edit_mode=delete to delete the cell at the index specified by cell_number."#
    }

    fn permission(&self) -> ToolPermission {
        ToolPermission::Write
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "notebook_path": {
                    "type": "string",
                    "description": "The absolute path to the Jupyter notebook file to edit"
                },
                "cell_id": {
                    "type": "string",
                    "description": "The ID of the cell to edit. When inserting, the new cell is inserted after this cell, or at the beginning if not specified."
                },
                "new_source": {
                    "type": "string",
                    "description": "The new source for the cell"
                },
                "cell_type": {
                    "type": "string",
                    "enum": ["code", "markdown"],
                    "description": "The type of the cell. Defaults to current cell type. Required for edit_mode=insert."
                },
                "edit_mode": {
                    "type": "string",
                    "enum": ["replace", "insert", "delete"],
                    "description": "The type of edit to make. Defaults to replace."
                }
            },
            "required": ["notebook_path", "new_source"]
        })
    }

    fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let path = params["notebook_path"]
            .as_str()
            .ok_or_else(|| anyhow!("missing notebook_path"))?;

        if !path.starts_with('/') {
            return Err(anyhow!("notebook_path must be absolute, got: {path}"));
        }

        let edit_mode = params
            .get("edit_mode")
            .and_then(Value::as_str)
            .unwrap_or("replace");
        let cell_id = params.get("cell_id").and_then(Value::as_str);
        let new_source = params["new_source"]
            .as_str()
            .ok_or_else(|| anyhow!("missing new_source"))?;
        let cell_type = params.get("cell_type").and_then(Value::as_str);

        let content =
            fs::read_to_string(path).with_context(|| format!("Failed to read notebook: {path}"))?;
        let mut nb: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse notebook JSON: {path}"))?;

        let cells = nb
            .get_mut("cells")
            .ok_or_else(|| anyhow!("Notebook has no 'cells' array"))?
            .as_array_mut()
            .ok_or_else(|| anyhow!("'cells' is not an array"))?;

        match edit_mode {
            "replace" => {
                let idx = find_cell_index(cells, cell_id)?;
                let cell = &mut cells[idx];
                if let Some(ct) = cell_type {
                    cell["cell_type"] = json!(ct);
                }
                cell["source"] = json!(new_source);
                Ok(ToolOutput::text(format!(
                    "Replaced cell {} in {}",
                    cell_id.unwrap_or(&idx.to_string()),
                    path
                )))
            }
            "insert" => {
                let ct = cell_type.unwrap_or("code");
                let new_cell = json!({
                    "cell_type": ct,
                    "source": new_source,
                    "metadata": {},
                    "id": generate_cell_id(),
                    "outputs": [],
                    "execution_count": null,
                });
                if let Some(id) = cell_id {
                    let idx = find_cell_index(cells, Some(id))?;
                    cells.insert(idx + 1, new_cell);
                } else {
                    cells.insert(0, new_cell);
                }
                Ok(ToolOutput::text(format!(
                    "Inserted {} cell in {}",
                    ct, path
                )))
            }
            "delete" => {
                let idx = find_cell_index(cells, cell_id)?;
                cells.remove(idx);
                Ok(ToolOutput::text(format!(
                    "Deleted cell {} from {}",
                    cell_id.unwrap_or(&idx.to_string()),
                    path
                )))
            }
            _ => Err(anyhow!("Unknown edit_mode: {edit_mode}")),
        }?;

        // Preserve notebook format: trailing newline
        let output =
            serde_json::to_string_pretty(&nb).with_context(|| "Failed to serialize notebook")?;
        fs::write(path, format!("{output}\n"))
            .with_context(|| format!("Failed to write notebook: {path}"))?;

        Ok(ToolOutput::text(format!(
            "Successfully edited notebook: {path} (mode: {edit_mode})"
        )))
    }
}

fn find_cell_index(cells: &[Value], cell_id: Option<&str>) -> Result<usize> {
    let id = cell_id.ok_or_else(|| anyhow!("cell_id is required"))?;
    cells
        .iter()
        .position(|c| c.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| anyhow!("Cell '{id}' not found"))
}

fn generate_cell_id() -> String {
    format!(
        "cell-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_notebook(cells: Vec<Value>) -> String {
        serde_json::to_string_pretty(&json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {},
            "cells": cells
        }))
        .unwrap()
    }

    fn test_ctx() -> ToolContext {
        ToolContext::new("/tmp")
    }

    #[test]
    fn test_notebook_replace_cell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        let nb = make_notebook(vec![json!({
            "id": "cell-1",
            "cell_type": "code",
            "source": "print('old')",
            "metadata": {},
            "outputs": [],
            "execution_count": null,
        })]);
        fs::write(&path, nb).unwrap();

        let tool = NotebookEditTool;
        let result = tool.execute(
            json!({
                "notebook_path": path.to_str().unwrap(),
                "cell_id": "cell-1",
                "new_source": "print('new')"
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());

        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(updated["cells"][0]["source"], "print('new')");
    }

    #[test]
    fn test_notebook_insert_cell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        let nb = make_notebook(vec![json!({
            "id": "cell-1",
            "cell_type": "code",
            "source": "old",
            "metadata": {},
        })]);
        fs::write(&path, nb).unwrap();

        let tool = NotebookEditTool;
        let result = tool.execute(
            json!({
                "notebook_path": path.to_str().unwrap(),
                "cell_id": "cell-1",
                "new_source": "# inserted",
                "cell_type": "markdown",
                "edit_mode": "insert"
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());

        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(updated["cells"].as_array().unwrap().len(), 2);
        assert_eq!(updated["cells"][1]["cell_type"], "markdown");
    }

    #[test]
    fn test_notebook_delete_cell() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        let nb = make_notebook(vec![
            json!({"id": "cell-1", "cell_type": "code", "source": "a"}),
            json!({"id": "cell-2", "cell_type": "code", "source": "b"}),
        ]);
        fs::write(&path, nb).unwrap();

        let tool = NotebookEditTool;
        let result = tool.execute(
            json!({
                "notebook_path": path.to_str().unwrap(),
                "cell_id": "cell-1",
                "edit_mode": "delete",
                "new_source": ""
            }),
            &test_ctx(),
        );
        assert!(result.is_ok());

        let updated: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let cells = updated["cells"].as_array().unwrap();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0]["id"], "cell-2");
    }

    #[test]
    fn test_notebook_rejects_relative_path() {
        let tool = NotebookEditTool;
        let result = tool.execute(
            json!({
                "notebook_path": "relative.ipynb",
                "cell_id": "cell-1",
                "new_source": "x"
            }),
            &test_ctx(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("absolute"));
    }

    #[test]
    fn test_notebook_missing_cell_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ipynb");
        fs::write(&path, make_notebook(vec![])).unwrap();

        let tool = NotebookEditTool;
        let result = tool.execute(
            json!({
                "notebook_path": path.to_str().unwrap(),
                "new_source": "x"
            }),
            &test_ctx(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_metadata() {
        let tool = NotebookEditTool;
        assert_eq!(tool.name(), "notebook_edit");
        assert_eq!(tool.permission(), ToolPermission::Write);
        assert!(tool.description().contains("Jupyter"));
    }
}
