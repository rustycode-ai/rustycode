use crate::security::{create_file_symlink_safe, open_file_symlink_safe, validate_write_path};
use crate::{ToolOutput, ToolPermission, ToolTag};
use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;

struct MismatchDetail {
    offset: usize,
    expected: u8,
    actual: u8,
}

fn verify_written(path: &Path, expected: &[u8]) -> Result<(), MismatchDetail> {
    use std::io::Read;

    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return Err(MismatchDetail {
                offset: 0,
                expected: expected.first().copied().unwrap_or(0),
                actual: 0,
            });
        }
    };

    let mut actual = Vec::with_capacity(expected.len());
    if f.read_to_end(&mut actual).is_err() {
        return Err(MismatchDetail {
            offset: 0,
            expected: expected.first().copied().unwrap_or(0),
            actual: 0,
        });
    }

    if actual.len() != expected.len() {
        let offset = expected.len().min(actual.len());
        return Err(MismatchDetail {
            offset,
            expected: expected.get(offset).copied().unwrap_or(0),
            actual: actual.get(offset).copied().unwrap_or(0),
        });
    }

    for (i, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
        if e != a {
            return Err(MismatchDetail {
                offset: i,
                expected: *e,
                actual: *a,
            });
        }
    }

    Ok(())
}

fn execute_append(
    path: &Path,
    content: &str,
    appended_bytes: usize,
    path_display: String,
    ctx: &crate::ToolContext,
) -> Result<ToolOutput> {
    use std::io::Write;

    let existed = path.exists();
    let old_size = if existed {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .with_context(|| format!("Failed to open {path_display} for append"))?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;

    let new_metadata = fs::metadata(path)?;
    let total_bytes = new_metadata.len();
    let total_lines = fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0);

    let summary = if existed {
        format!(
            "appended {appended_bytes} bytes to {path_display} (total: {total_bytes} bytes, {total_lines} lines)"
        )
    } else {
        format!("created and wrote {appended_bytes} bytes to {path_display} ({total_lines} lines)")
    };

    let mut output_text = summary;
    if let Some(formatter_diff) = crate::file_formatter::format_file(path, &ctx.cwd) {
        output_text.push_str(&formatter_diff);
    }

    Ok(ToolOutput::with_structured(
        output_text,
        json!({
            "path": path_display,
            "appended_bytes": appended_bytes,
            "total_bytes": total_bytes,
            "total_lines": total_lines,
            "old_size": old_size,
        }),
    ))
}

#[derive(Deserialize, JsonSchema)]
struct WriteFileParams {
    /// File path to write (alias: file_path). Parent directories are created automatically.
    #[serde(alias = "file_path")]
    path: std::path::PathBuf,
    /// UTF-8 text content. Completely replaces existing file content unless append=true.
    content: Option<String>,
    /// Base64-encoded binary content to write.
    content_base64: Option<String>,
    /// If true, append content to the end of the existing file instead of overwriting.
    append: Option<bool>,
}

rustycode_tools_api::define_tool! {
    pub struct WriteFileTool;

    name: "write_file",
    description: "Write UTF-8 text to a file. Creates parent directories if needed. Set append=true to add content to the end of an existing file (useful for writing large files in multiple chunks). Returns a diff showing what changed vs the previous file content.",
    permission: ToolPermission::Write,
    tags: [ToolTag::Implement, ToolTag::Refactor],

    execute(params: WriteFileParams, ctx) {
        if let Some(gate) = &ctx.plan_gate {
            gate.check_access(ctx.role, "write_file")?;
        }
        crate::check_permission(ToolPermission::Write, ctx)?;

        let path_str = params.path.to_str()
            .ok_or_else(|| anyhow!("path contains invalid UTF-8"))?;
        let text_content = params.content.as_deref();
        let binary_content = params.content_base64.as_deref();
        let append = params.append.unwrap_or(false);

        if text_content.is_some() && binary_content.is_some() {
            return Err(anyhow!("use either `content` or `content_base64`, not both"));
        }
        if append && binary_content.is_some() {
            return Err(anyhow!("append mode is not supported for binary content"));
        }

        let binary_bytes = if let Some(encoded) = binary_content {
            Some(
                STANDARD
                    .decode(encoded)
                    .map_err(|e| anyhow!("invalid base64 content: {e}"))?,
            )
        } else {
            None
        };
        let content = text_content.unwrap_or("");

        let write_size = binary_bytes
            .as_ref()
            .map(|b: &Vec<u8>| b.len())
            .unwrap_or(content.len());
        let path =
            validate_write_path(path_str, &ctx.cwd, write_size, !ctx.allow_outside_workspace)?;

        // Staleness check (was validate_input)
        {
            let canonical = path.canonicalize().ok();
            if let (Some(state), Some(canonical)) = (&ctx.file_read_state, &canonical) {
                let current_mtime = fs::metadata(canonical).ok().and_then(|m| m.modified().ok());
                if let Err(reason) = state.check_stale(canonical, current_mtime) {
                    return Err(anyhow!("{reason}"));
                }
            }
        }

        crate::check_sandbox_path(&path, ctx)?;

        if super::fs::is_blocked_extension(&path) {
            return Err(anyhow::anyhow!(
                "File extension is blocked for writing: {}",
                path.extension().unwrap_or_default().to_string_lossy()
            ));
        }
        if super::fs::is_blocked_filename(&path) {
            return Err(anyhow::anyhow!(
                "File is blocked for writing: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }

        // Second staleness check after resolving path (belt-and-suspenders)
        if let Some(state) = &ctx.file_read_state {
            let canonical = path.canonicalize().ok();
            let current_mtime = canonical
                .as_ref()
                .and_then(|p| fs::metadata(p).ok())
                .and_then(|m| m.modified().ok());
            let check_path = canonical.as_ref().unwrap_or(&path);
            if let Err(reason) = state.check_stale(check_path, current_mtime) {
                return Err(anyhow!("{reason}"));
            }
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let path_display = path.display().to_string();

        if append {
            return execute_append(&path, content, write_size, path_display, ctx);
        }

        // Non-append (overwrite) mode
        let old_content = if binary_bytes.is_some() {
            Vec::new()
        } else if let Ok(mut f) = open_file_symlink_safe(&path) {
            use std::io::Read;
            let mut buf = String::new();
            let bytes_read = f.read_to_string(&mut buf).unwrap_or_else(|e| {
                tracing::debug!("Failed to read existing file for diff: {}", e);
                0
            });
            if bytes_read == 0 && !buf.is_empty() {
                tracing::debug!(
                    "Read returned 0 bytes but buffer is non-empty for {}",
                    path.display()
                );
            }
            buf.into_bytes()
        } else {
            Vec::new()
        };

        let expected_bytes: &[u8] = binary_bytes.as_deref().unwrap_or(content.as_bytes());
        let pre_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(expected_bytes);
            let hash = hasher.finalize();
            let mut hex = String::with_capacity(8);
            for b in &hash[..4] {
                use std::fmt::Write;
                let _ = write!(hex, "{b:02x}");
            }
            hex
        };
        tracing::debug!(
            "write_file: preparing to write {} bytes to {} (sha256 prefix: {pre_hash})",
            write_size,
            path_display,
        );

        let mut file = create_file_symlink_safe(&path)?;
        use std::io::Write;
        file.write_all(expected_bytes)?;
        file.sync_all()?;
        drop(file);

        if write_size <= 1_048_576 {
            match verify_written(&path, expected_bytes) {
                Ok(()) => {
                    tracing::debug!(
                        "write_file: verified {write_size} bytes for {path_display} (sha256 prefix: {pre_hash})"
                    );
                }
                Err(first_mismatch) => {
                    tracing::warn!(
                        "write_file: readback mismatch for {path_display} at byte offset {} \
                         (expected 0x{:02x}, got 0x{:02x}) — retrying write",
                        first_mismatch.offset,
                        first_mismatch.expected,
                        first_mismatch.actual,
                    );
                    let mut retry_file = create_file_symlink_safe(&path)?;
                    retry_file.write_all(expected_bytes)?;
                    retry_file.sync_all()?;
                    drop(retry_file);

                    if let Err(retry_mismatch) = verify_written(&path, expected_bytes) {
                        return Err(anyhow!(
                            "write_file: persistent readback mismatch for {path_display} \
                             after retry at byte offset {} (expected 0x{:02x}, got 0x{:02x})",
                            retry_mismatch.offset,
                            retry_mismatch.expected,
                            retry_mismatch.actual,
                        ));
                    }
                    tracing::debug!(
                        "write_file: retry verified {write_size} bytes for {path_display}"
                    );
                }
            }
        }

        let bytes = write_size;
        let lines = content.lines().count();

        let diff = if binary_bytes.is_some() {
            format!("Wrote binary file ({bytes} bytes)")
        } else if old_content.is_empty() {
            format!("Created new file ({bytes} bytes, {lines} lines)")
        } else {
            let old_text = String::from_utf8_lossy(&old_content);
            crate::line_endings::generate_diff(&old_text, content, &path_display, 50)
        };

        let mut output_text =
            format!("wrote {path_display} ({bytes} bytes, {lines} lines)\n{diff}");

        if binary_bytes.is_none() {
            if let Some(formatter_diff) = crate::file_formatter::format_file(&path, &ctx.cwd) {
                output_text.push_str(&formatter_diff);
            }
        }

        if let Some(state) = &ctx.file_read_state {
            state.invalidate(&path);
        }

        Ok(ToolOutput::with_structured(
            output_text,
            json!({
                "path": path_display,
                "bytes": bytes,
                "lines": lines
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::list_dir::ListDirTool;
    use crate::{Tool, ToolContext};
    use std::os::unix::fs::symlink as symlink_file;
    use tempfile::tempdir;

    #[test]
    fn write_file_supports_base64_binary() {
        let workspace = tempdir().expect("workspace tempdir");
        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());

        let bytes = [0x89u8, 0x50, 0x4e, 0x47, 0x00, 0x01];
        let encoded = STANDARD.encode(bytes);
        let res = tool.execute(
            serde_json::json!({
                "path": "out.dat",
                "content_base64": encoded
            }),
            &ctx,
        );
        assert!(res.is_ok(), "write failed: {:?}", res.err());
        let written = fs::read(workspace.path().join("out.dat")).expect("read written binary");
        assert_eq!(written.as_slice(), &bytes);
    }

    #[test]
    fn write_file_blocks_symlink() {
        let workspace = tempdir().expect("workspace tempdir");
        let test_file = workspace.path().join("test.txt");
        fs::write(&test_file, "original").expect("write test file");

        let symlink_path = workspace.path().join("symlink.txt");
        symlink_file(&test_file, &symlink_path).expect("create symlink");

        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({ "path": "symlink.txt", "content": "modified" }),
            &ctx,
        );
        match res {
            Ok(_) => panic!("Expected error for symlink path, but got Ok"),
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
            }
        }
    }

    #[test]
    fn write_file_normal_path_works() {
        let workspace = tempdir().expect("workspace tempdir");

        let tool = WriteFileTool;
        let ctx = ToolContext::new(workspace.path());
        let res = tool.execute(
            serde_json::json!({ "path": "newfile.txt", "content": "test content" }),
            &ctx,
        );
        assert!(res.is_ok());
    }

    #[test]
    fn list_dir_blocks_symlink() {
        let workspace = tempdir().expect("workspace tempdir");
        let outside = tempdir().expect("outside tempdir");

        let symlink_path = workspace.path().join("symlinkdir");

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &symlink_path).expect("create dir symlink");

        let tool = ListDirTool;
        let ctx = ToolContext::new(workspace.path());

        #[cfg(unix)]
        {
            let res = tool.execute(serde_json::json!({ "path": "symlinkdir" }), &ctx);
            match res {
                Ok(_) => panic!("Expected error for symlink path, but got Ok"),
                Err(e) => {
                    let msg = e.to_string();
                    assert!(msg.contains("symbolic link"), "Unexpected error: {}", msg);
                }
            }
        }
    }
}
