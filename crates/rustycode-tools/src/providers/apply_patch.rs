//! Apply Patch tool — apply unified diffs directly from LLM output.
//!
//! Supports inline patch text (preferred) or patch file paths.
//! Falls back to `git apply --3way` when direct parsing fails.

use crate::file_formatter;
use crate::line_endings::generate_diff;
use crate::security::{
    create_file_symlink_safe, open_file_symlink_safe, validate_read_path, validate_write_path,
};
use crate::{ToolContext, ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use schemars::JsonSchema;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

// Input

#[derive(serde::Deserialize, JsonSchema)]
pub struct ApplyPatchParams {
    /// The unified diff patch text to apply. Use standard `diff -u` format with `--- a/file` and `+++ b/file` headers.
    patch: Option<String>,
    /// Path to a .patch or .diff file (alternative to inline `patch`)
    patch_file: Option<String>,
    /// Path components to strip (default: 1)
    #[serde(default = "default_strip")]
    strip: u64,
}

fn default_strip() -> u64 {
    1
}

#[derive(Debug)]
enum PatchSource {
    Inline(String),
    File(PathBuf),
}

// Patch data structures

#[derive(Debug, Clone)]
enum PatchOperation {
    Add,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
struct FilePatch {
    path: String,
    operation: PatchOperation,
    hunks: Vec<Hunk>,
}

#[derive(Debug, Clone)]
struct Hunk {
    old_start: usize,
    #[allow(dead_code)]
    old_count: usize,
    #[allow(dead_code)]
    new_start: usize,
    #[allow(dead_code)]
    new_count: usize,
    lines: Vec<PatchLine>,
}

#[derive(Debug, Clone)]
enum PatchLine {
    #[allow(dead_code)]
    Context(String),
    Remove(String),
    Add(String),
}

// Tool implementation

rustycode_tools_api::define_tool! {
    pub struct ApplyPatchTool;

    name: "apply_patch",
    description: "Apply a unified diff patch to one or more files. \
     Supports adding new files, updating existing files, and deleting files. \
     Prefer this over edit_file for multi-file or multi-hunk changes.",
    permission: ToolPermission::Write,
    tags: [ToolTag::Implement],

    execute(params: ApplyPatchParams, ctx) {
        let source = resolve_source(&params, ctx)?;
        let strip = params.strip as usize;
        let patch_text = read_source(&source, ctx)?;

        if patch_text.trim().is_empty() {
            return Err(anyhow!("empty patch text"));
        }

        // Try direct parsing first.
        match parse_and_apply(&patch_text, strip, ctx) {
            Ok(output) => Ok(output),
            Err(e) => {
                // Fall back to git apply.
                match git_apply_fallback(&patch_text, strip, ctx) {
                    Ok(output) => Ok(output),
                    Err(git_err) => Err(anyhow!(
                        "direct parse failed: {e}\ngit apply fallback also failed: {git_err}"
                    )),
                }
            }
        }
    }
}

// Source resolution

fn resolve_source(params: &ApplyPatchParams, ctx: &crate::ToolContext) -> Result<PatchSource> {
    if let Some(ref text) = params.patch {
        if !text.is_empty() {
            return Ok(PatchSource::Inline(text.clone()));
        }
    }
    if let Some(ref path) = params.patch_file {
        let validated = validate_read_path(path, &ctx.cwd, !ctx.allow_outside_workspace)?;
        return Ok(PatchSource::File(validated));
    }
    Err(anyhow!(
        "provide either `patch` (inline text) or `patch_file` (path)"
    ))
}

fn read_source(source: &PatchSource, _ctx: &ToolContext) -> Result<String> {
    match source {
        PatchSource::Inline(text) => Ok(text.clone()),
        PatchSource::File(path) => {
            let mut file = open_file_symlink_safe(path)
                .with_context(|| format!("failed to open patch file: {}", path.display()))?;
            let mut content = String::new();
            file.read_to_string(&mut content)
                .with_context(|| format!("failed to read patch file: {}", path.display()))?;
            Ok(content)
        }
    }
}

// Unified diff parser

fn parse_multi_file_patch(patch_text: &str) -> Result<Vec<FilePatch>> {
    let normalized = patch_text.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    let mut patches = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Look for --- header.
        if lines[i].starts_with("--- ") {
            let old_path = extract_path(lines[i], 4);
            i += 1;

            if i >= lines.len() || !lines[i].starts_with("+++ ") {
                continue;
            }
            let new_path = extract_path(lines[i], 4);
            i += 1;

            // Determine operation.
            let operation = if old_path == "/dev/null" {
                PatchOperation::Add
            } else if new_path == "/dev/null" {
                PatchOperation::Delete
            } else {
                PatchOperation::Update
            };

            let target_path = if matches!(operation, PatchOperation::Add) {
                strip_path_components(new_path, 1)
            } else {
                strip_path_components(old_path, 1)
            };

            // Parse hunks.
            let mut hunks = Vec::new();
            while i < lines.len() && lines[i].starts_with("@@") {
                let hunk = parse_hunk(lines[i])?;
                i += 1;

                let mut hunk_lines = Vec::new();
                while i < lines.len()
                    && !lines[i].starts_with("@@")
                    && !lines[i].starts_with("--- ")
                {
                    let line = lines[i];
                    if let Some(content) = line.strip_prefix('+') {
                        hunk_lines.push(PatchLine::Add(content.to_string()));
                    } else if let Some(content) = line.strip_prefix('-') {
                        hunk_lines.push(PatchLine::Remove(content.to_string()));
                    } else if let Some(content) = line.strip_prefix(' ') {
                        hunk_lines.push(PatchLine::Context(content.to_string()));
                    } else if line.starts_with('\\') {
                        // "\ No newline at end of file" — skip.
                    } else {
                        // Treat as context.
                        hunk_lines.push(PatchLine::Context(line.to_string()));
                    }
                    i += 1;
                }

                hunks.push(Hunk {
                    old_start: hunk.0,
                    old_count: hunk.1,
                    new_start: hunk.2,
                    new_count: hunk.3,
                    lines: hunk_lines,
                });
            }

            patches.push(FilePatch {
                path: target_path,
                operation,
                hunks,
            });
        } else {
            i += 1;
        }
    }

    if patches.is_empty() {
        return Err(anyhow!("no valid unified diff hunks found in patch text"));
    }
    Ok(patches)
}

fn extract_path(header: &str, prefix_len: usize) -> &str {
    let rest = &header[prefix_len..];
    let path = rest.trim();
    // Strip tab-separated timestamps: "--- a/file.ts\t2024-01-01"
    if let Some(idx) = path.find('\t') {
        &path[..idx]
    } else {
        path
    }
}

fn strip_path_components(path: &str, n: usize) -> String {
    // Strip a/ or b/ prefix.
    let stripped = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);
    // Strip additional components.
    stripped
        .split('/')
        .skip(n.saturating_sub(1))
        .collect::<Vec<_>>()
        .join("/")
}

fn parse_hunk(header: &str) -> Result<(usize, usize, usize, usize)> {
    // @@ -old_start[,old_count] +new_start[,new_count] @@
    let text = header.trim_start_matches('@').trim_start_matches(' ');
    let text = text.split('@').next().unwrap_or("").trim();

    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(anyhow!("invalid hunk header: {header}"));
    }

    let old = parse_range(parts[0])?;
    let new = parse_range(parts[1])?;
    Ok((old.0, old.1, new.0, new.1))
}

fn parse_range(s: &str) -> Result<(usize, usize)> {
    let s = s.trim_start_matches('-').trim_start_matches('+');
    let mut parts = s.splitn(2, ',');
    let start: usize = parts.next().unwrap_or("1").parse().unwrap_or(1);
    let count: usize = parts.next().map(|c| c.parse().unwrap_or(1)).unwrap_or(1);
    Ok((start.max(1), count))
}

// Patch application

fn parse_and_apply(patch_text: &str, strip: usize, ctx: &ToolContext) -> Result<ToolOutput> {
    let patches = parse_multi_file_patch(patch_text)?;
    let mut results = Vec::new();

    for fp in &patches {
        let result = apply_file_patch(fp, strip, ctx)?;
        results.push(result);
    }

    let output = results.join("\n");
    Ok(ToolOutput::text(output))
}

fn apply_file_patch(fp: &FilePatch, strip: usize, ctx: &ToolContext) -> Result<String> {
    // Re-strip with the user-provided strip count if > 1.
    let path_str = if strip > 1 {
        strip_path_components(&fp.path, strip)
    } else {
        fp.path.clone()
    };

    let validated = validate_write_path(&path_str, &ctx.cwd, 0, !ctx.allow_outside_workspace)?;

    match &fp.operation {
        PatchOperation::Add => apply_add(&validated, fp),
        PatchOperation::Delete => apply_delete(&validated, fp),
        PatchOperation::Update => apply_update(&validated, fp, ctx),
    }
}

fn apply_add(path: &Path, fp: &FilePatch) -> Result<String> {
    if path.exists() {
        return Err(anyhow!("file already exists: {}", path.display()));
    }

    // Collect all Add lines from hunks.
    let mut content = String::new();
    for hunk in &fp.hunks {
        for line in &hunk.lines {
            if let PatchLine::Add(text) = line {
                content.push_str(text);
                content.push('\n');
            }
        }
    }

    // Create parent dirs.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let mut file = create_file_symlink_safe(path)
        .with_context(|| format!("failed to create file: {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write file: {}", path.display()))?;
    file.sync_all()?;

    let diff = generate_diff("", &content, &path.display().to_string(), 30);
    Ok(format!(
        "Created {} (+{} lines)\n{}",
        path.display(),
        content.lines().count(),
        diff
    ))
}

fn apply_delete(path: &Path, _fp: &FilePatch) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!("file not found: {}", path.display()));
    }

    let old_content = read_file(path)?;
    std::fs::remove_file(path).with_context(|| format!("failed to delete: {}", path.display()))?;

    Ok(format!(
        "Deleted {} (was {} lines)",
        path.display(),
        old_content.lines().count()
    ))
}

fn apply_update(path: &Path, fp: &FilePatch, ctx: &ToolContext) -> Result<String> {
    if !path.exists() {
        return Err(anyhow!("file not found: {}", path.display()));
    }

    let old_content = read_file(path)?;
    let new_content = apply_hunks(&old_content, &fp.hunks)?;

    if old_content == new_content {
        return Ok(format!("No changes in {}", path.display()));
    }

    let mut file = create_file_symlink_safe(path)
        .with_context(|| format!("failed to write: {}", path.display()))?;
    file.write_all(new_content.as_bytes())
        .with_context(|| format!("failed to write: {}", path.display()))?;
    file.sync_all()?;

    let diff = generate_diff(&old_content, &new_content, &path.display().to_string(), 30);

    let mut output = diff;

    // Auto-format if configured.
    if let Some(formatter_diff) = file_formatter::format_file(path, &ctx.cwd) {
        output.push_str(&formatter_diff);
    }

    Ok(output)
}

fn apply_hunks(content: &str, hunks: &[Hunk]) -> Result<String> {
    let mut result: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut offset: isize = 0;

    for hunk in hunks {
        let insert_pos = (hunk.old_start as isize + offset - 1).max(0) as usize;

        // Collect what this hunk expects to remove vs what we find.
        let removes: Vec<&str> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                PatchLine::Remove(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();

        let adds: Vec<String> = hunk
            .lines
            .iter()
            .filter_map(|l| match l {
                PatchLine::Add(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        let add_count = adds.len();

        // Verify context matches.
        let end = (insert_pos + removes.len()).min(result.len());
        let actual: Vec<&str> = result
            .get(insert_pos..end)
            .unwrap_or(&[])
            .iter()
            .map(|s| s.as_str())
            .collect();

        // Allow fuzzy: if exact match fails, try to find the removed block nearby.
        let match_pos = if !removes.is_empty() && actual != removes {
            let result_refs: Vec<&str> = result.iter().map(|s| s.as_str()).collect();
            find_fuzzy_match(&result_refs, &removes, insert_pos, 10).ok_or_else(|| {
                anyhow!(
                    "hunk at line {} does not match.\nExpected:\n  {}\nFound:\n  {}",
                    hunk.old_start,
                    removes.join("\n  "),
                    actual.join("\n  ")
                )
            })?
        } else {
            insert_pos
        };

        // Remove old lines and insert new ones.
        let remove_count = removes.len().min(result.len().saturating_sub(match_pos));
        result.splice(match_pos..match_pos + remove_count, adds);
        offset += add_count as isize - remove_count as isize;
    }

    let mut output = result.join("\n");
    // Preserve trailing newline if original had one.
    if content.ends_with('\n') && !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Search for the removed block within `radius` lines of `start`.
#[allow(clippy::needless_lifetimes)]
fn find_fuzzy_match<'a>(
    lines: &[&'a str],
    removes: &[&'a str],
    start: usize,
    radius: usize,
) -> Option<usize> {
    let lo = start.saturating_sub(radius);
    let hi = (start + radius).min(lines.len().saturating_sub(removes.len()));
    for pos in lo..=hi {
        let end = pos + removes.len();
        if end > lines.len() {
            continue;
        }
        let slice = &lines[pos..end];
        if slice
            .iter()
            .zip(removes.iter())
            .all(|(a, b)| a.trim() == b.trim())
        {
            return Some(pos);
        }
    }
    None
}

fn read_file(path: &Path) -> Result<String> {
    let mut file = open_file_symlink_safe(path)
        .with_context(|| format!("failed to open: {}", path.display()))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .with_context(|| format!("failed to read: {}", path.display()))?;
    Ok(content)
}

// Git apply fallback

fn git_apply_fallback(patch_text: &str, strip: usize, ctx: &ToolContext) -> Result<ToolOutput> {
    // Write patch to temp file.
    let tmp_dir = std::env::temp_dir().join("rustycode-patches");
    std::fs::create_dir_all(&tmp_dir).ok();
    let tmp_path = tmp_dir.join(format!("patch-{}", uuid::Uuid::new_v4().as_simple()));
    {
        let mut f =
            std::fs::File::create(&tmp_path).with_context(|| "failed to create temp patch file")?;
        f.write_all(patch_text.as_bytes())?;
    }

    let output = std::process::Command::new("git")
        .arg("apply")
        .arg("--3way")
        .arg("--stat")
        .arg(format!("-p{strip}"))
        .arg(&tmp_path)
        .current_dir(&ctx.cwd)
        .output()
        .with_context(|| "failed to run git apply")?;

    // Clean up temp file.
    std::fs::remove_file(&tmp_path).ok();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        return Err(anyhow!("git apply failed:\n{stderr}"));
    }

    let mut result = String::from("Applied via git apply:\n");
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() && !stderr.contains("cyclic") {
        result.push_str(&stderr);
    }

    Ok(ToolOutput::text(result))
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Tool, ToolContext};
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn test_apply_patch_tool_metadata() {
        let tool = ApplyPatchTool;
        assert_eq!(tool.name(), "apply_patch");
        assert_eq!(tool.permission(), ToolPermission::Write);
    }

    #[test]
    fn test_parse_simple_update() {
        let patch = "\
--- a/hello.txt
+++ b/hello.txt
@@ -1,3 +1,3 @@
 hello
-world
+rust
 goodbye";

        let patches = parse_multi_file_patch(patch).unwrap();
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].path, "hello.txt");
        assert!(matches!(patches[0].operation, PatchOperation::Update));
        assert_eq!(patches[0].hunks.len(), 1);
        assert_eq!(patches[0].hunks[0].lines.len(), 4);
    }

    #[test]
    fn test_parse_new_file() {
        let patch = "\
--- /dev/null
+++ b/new_file.rs
@@ -0,0 +1,3 @@
+fn main() {
+    println!(\"hello\");
+}";

        let patches = parse_multi_file_patch(patch).unwrap();
        assert_eq!(patches.len(), 1);
        assert!(matches!(patches[0].operation, PatchOperation::Add));
        assert_eq!(patches[0].path, "new_file.rs");
    }

    #[test]
    fn test_parse_delete_file() {
        let patch = "\
--- a/old_file.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-line1
-line2";

        let patches = parse_multi_file_patch(patch).unwrap();
        assert_eq!(patches.len(), 1);
        assert!(matches!(patches[0].operation, PatchOperation::Delete));
    }

    #[test]
    fn test_parse_multi_file() {
        let patch = "\
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old
+new
--- a/b.txt
+++ b/b.txt
@@ -1 +1 @@
-foo
+bar";

        let patches = parse_multi_file_patch(patch).unwrap();
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].path, "a.txt");
        assert_eq!(patches[1].path, "b.txt");
    }

    #[test]
    fn test_apply_inline_update() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("hello.txt");
        std::fs::write(&test_file, "hello\nworld\ngoodbye\n").unwrap();

        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- a/hello.txt\n+++ b/hello.txt\n@@ -1,3 +1,3 @@\n hello\n-world\n+rust\n goodbye\n"
            }),
            &ctx,
        );

        assert!(result.is_ok(), "apply failed: {:?}", result.err());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "hello\nrust\ngoodbye\n");
    }

    #[test]
    fn test_apply_inline_new_file() {
        let workspace = tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,2 @@\n+hello\n+world\n"
            }),
            &ctx,
        );

        assert!(result.is_ok(), "apply failed: {:?}", result.err());
        let content = std::fs::read_to_string(workspace.path().join("new.txt")).unwrap();
        assert_eq!(content, "hello\nworld\n");
    }

    #[test]
    fn test_apply_inline_delete() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("old.txt");
        std::fs::write(&test_file, "line1\nline2\n").unwrap();

        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- a/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-line1\n-line2\n"
            }),
            &ctx,
        );

        assert!(result.is_ok(), "apply failed: {:?}", result.err());
        assert!(!test_file.exists());
    }

    #[test]
    fn test_apply_empty_patch_rejected() {
        let workspace = tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({"patch": ""}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_no_source_rejected() {
        let workspace = tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({}), &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_blocks_path_traversal() {
        let workspace = tempdir().unwrap();
        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- a/../../../etc/passwd\n+++ b/../../../etc/passwd\n@@ -1 +1 @@\n-root\n+hacked\n"
            }),
            &ctx,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_output_contains_diff() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("diff_test.txt");
        std::fs::write(&test_file, "hello\nworld\n").unwrap();

        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- a/diff_test.txt\n+++ b/diff_test.txt\n@@ -1,2 +1,2 @@\n-hello\n-world\n+foo\n+bar\n"
            }),
            &ctx,
        );

        assert!(result.is_ok());
        let output = result.unwrap().text;
        assert!(
            output.contains("+foo"),
            "diff output should contain +foo, got: {output}"
        );
        assert!(
            output.contains("-hello"),
            "diff output should contain -hello, got: {output}"
        );
    }

    #[test]
    fn test_apply_multi_hunk() {
        let workspace = tempdir().unwrap();
        let test_file = workspace.path().join("multi.txt");
        std::fs::write(
            &test_file,
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\n",
        )
        .unwrap();

        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(
            json!({
                "patch": "--- a/multi.txt\n+++ b/multi.txt\n@@ -1,3 +1,3 @@\n-line1\n+CHANGED1\n line2\n line3\n@@ -5,3 +5,3 @@\n line5\n-line6\n+CHANGED6\n line7\n"
            }),
            &ctx,
        );

        assert!(result.is_ok(), "apply failed: {:?}", result.err());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("CHANGED1"));
        assert!(content.contains("CHANGED6"));
        assert!(!content.contains("line1"));
        assert!(!content.contains("line6"));
    }

    #[test]
    fn test_apply_patch_file_mode() {
        let workspace = tempdir().unwrap();
        // Create a patch file.
        let patch_file = workspace.path().join("changes.patch");
        let test_file = workspace.path().join("target.txt");
        std::fs::write(&test_file, "old content\n").unwrap();
        std::fs::write(
            &patch_file,
            "--- a/target.txt\n+++ b/target.txt\n@@ -1 +1 @@\n-old content\n+new content\n",
        )
        .unwrap();

        let tool = ApplyPatchTool;
        let ctx = ToolContext::new(workspace.path());

        let result = tool.execute(json!({"patch_file": "changes.patch"}), &ctx);

        assert!(result.is_ok(), "apply failed: {:?}", result.err());
        let content = std::fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "new content\n");
    }
}
