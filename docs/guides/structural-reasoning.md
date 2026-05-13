# Guide: Efficient Structural Reasoning with Unified Symbol Engine

To use this engine effectively, follow the "Explore-Target-Patch-Verify" (ETPV) loop.

## 1. Explore (Minimal Context)
**Avoid**: `read_file` or `list_dir` for large projects.
**Use**: `outline_file(detail="condensed")` for a bird's eye view.
**Use**: `outline_file(detail="signatures")` to understand APIs and data types.

## 2. Target (Focused Context)
**Avoid**: Reading the whole file to find a method.
**Use**: `find_symbol(name="MyMethod")` to locate it across the project.
**Use**: `code_context(symbol="MyMethod")` to read only the relevant implementation.

## 3. Patch (Atomic Modification)
**Avoid**: `replace_file_content` with line ranges (fragile).
**Use**: `structural_patch(symbol="MyMethod", content="...")`. 
*   **Why?** It uses byte-offsets from the symbol engine. It is immune to line shifts and concurrent changes.

## 4. Verify (Structural Integrity)
**Avoid**: Just hoping it still compiles.
**Use**: `check_drift(path="file.rs")`.
*   **Why?** It ensures you haven't accidentally deleted a method or changed a public signature that other modules depend on.

---

## Token Budgeting Tips

| Task | Legacy Tool | Symbol Tool | Context Saving |
|------|-------------|-------------|----------------|
| Map project | `read_file` * N | `RepoMap` | **95%** |
| Understand class | `read_file` | `outline_file` | **70%** |
| Refactor method | `read_file` + `edit` | `code_context` + `patch` | **50%** |

## Example Flow: Refactoring a Method

1.  `outline_file(path="src/lib.rs", detail="signatures")` -> Get the structure.
2.  `code_context(symbol="process_data")` -> Get the body.
3.  `structural_patch(symbol="process_data", content="new body")` -> Atomic update.
4.  `check_drift(path="src/lib.rs")` -> Verify.
