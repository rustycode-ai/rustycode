//! Code utility functions for token estimation, code excerpt selection, and source filtering.

use anyhow::Result;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use crate::runtime::CodeExcerpt;

pub fn estimate_tokens(value: &str) -> usize {
    crate::context::TokenCounter::estimate_tokens(value)
}

/// Select code excerpts relevant to a task from the workspace.
pub fn select_code_excerpts(cwd: &Path, task: &str, limit: usize) -> Result<Vec<CodeExcerpt>> {
    let terms = task_terms(task);
    let mut matches = Vec::new();
    let mut fallback = Vec::new();
    for entry in WalkDir::new(cwd)
        .max_depth(4)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if should_skip_path(path) || !is_supported_source(path) {
            continue;
        }
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let preview = content
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("")
            .trim()
            .chars()
            .take(120)
            .collect::<String>();
        let path_text = path.display().to_string().to_lowercase();
        let content_text = content.to_lowercase();
        let mut score = 0;
        for term in &terms {
            if path_text.contains(term) {
                score += 5;
            }
            if content_text.contains(term) {
                score += 2;
            }
        }
        if score == 0 && terms.is_empty() {
            score = 1;
        }
        if score > 0 {
            matches.push(CodeExcerpt {
                path: path.display().to_string(),
                preview,
                score,
            });
        } else {
            fallback.push(CodeExcerpt {
                path: path.display().to_string(),
                preview,
                score: 1,
            });
        }
    }
    matches.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
    fallback.sort_by(|a, b| a.path.cmp(&b.path));
    for excerpt in fallback {
        if matches.len() >= limit {
            break;
        }
        matches.push(excerpt);
    }
    matches.truncate(limit);
    Ok(matches)
}

/// Extract search terms from a task description.
pub fn task_terms(task: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for raw in task.split(|char: char| !char.is_alphanumeric()) {
        let term = raw.trim().to_lowercase();
        if term.len() >= 3 && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

/// Check if a file extension is a supported source file type.
pub fn is_supported_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("rs" | "md" | "toml" | "json" | "yaml" | "yml" | "ts" | "js" | "py")
    )
}

/// Check if a path should be skipped (target, .git, node_modules).
pub fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == "target" || value == ".git" || value == "node_modules"
    })
}
