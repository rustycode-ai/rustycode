use std::path::Path;

/// Find files with similar names in the workspace (for "did you mean?" suggestions).
pub fn suggest_similar_files(target: &Path, cwd: &Path) -> Vec<String> {
    crate::file_suggest::suggest_similar_files(target, cwd, 3)
}

/// Helper to generate the final edited message with diff and optional formatting results.
pub fn format_edit_output(
    path_display: &str,
    diff: &str,
    formatter_diff: Option<String>,
) -> String {
    let mut output = format!("Edited {path_display}:\n{diff}");
    if let Some(fd) = formatter_diff {
        output.push_str(&fd);
    }
    output
}
