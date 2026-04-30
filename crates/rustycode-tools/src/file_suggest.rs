//! File suggestion utilities for "did you mean?" error messages.

/// Find files with similar names in the workspace.
/// Returns up to `max` candidates sorted by similarity.
pub fn suggest_similar_files(
    target: &std::path::Path,
    cwd: &std::path::Path,
    max: usize,
) -> Vec<String> {
    let target_str = target.to_string_lossy();
    let target_name = target.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let target_stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    let mut candidates: Vec<(usize, String)> = Vec::new();
    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let entry_stem = std::path::Path::new(&name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let score = score_similarity(entry_stem, target_stem, &name, target_name, &target_str);
        if score > 0 {
            if let Ok(full) = entry.path().strip_prefix(cwd) {
                candidates.push((score, full.to_string_lossy().to_string()));
            } else {
                candidates.push((score, name));
            }
        }
    }

    // Also check src/ directory if target is under src/
    if target_str.starts_with("src/") || target_str.starts_with("src\\") {
        let src_dir = cwd.join("src");
        if src_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&src_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let entry_stem = std::path::Path::new(&name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    let score =
                        score_similarity(entry_stem, target_stem, &name, target_name, &target_str);
                    if score > 0 {
                        candidates.push((score, format!("src/{name}")));
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.truncate(max);
    candidates.into_iter().map(|(_, p)| p).collect()
}

/// Format suggestion list as a "did you mean?" message suffix.
/// Returns empty string if no suggestions found.
pub fn format_suggestions(suggestions: &[String]) -> String {
    if suggestions.is_empty() {
        return String::new();
    }
    format!(
        "\n\nDid you mean one of these files?\n{}",
        suggestions
            .iter()
            .map(|s| format!("  - {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Score how similar a candidate file is to the target.
fn score_similarity(
    entry_stem: &str,
    target_stem: &str,
    entry_name: &str,
    target_name: &str,
    target_str: &str,
) -> usize {
    if entry_stem == target_stem && !entry_stem.is_empty() {
        100
    } else if entry_name == target_name {
        90
    } else if target_str.contains(entry_name) || entry_name.contains(target_name) {
        70
    } else if entry_stem.starts_with(target_stem) || target_stem.starts_with(entry_stem) {
        50
    } else if char_overlap(entry_stem, target_stem) >= 0.6 {
        30
    } else {
        0
    }
}

/// Character-overlap ratio between two strings (0.0..1.0).
fn char_overlap(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut common = 0usize;
    let mut b_used = vec![false; b_chars.len()];
    for &ac in &a_chars {
        for (j, &bc) in b_chars.iter().enumerate() {
            if !b_used[j] && ac == bc {
                common += 1;
                b_used[j] = true;
                break;
            }
        }
    }
    let max_len = a_chars.len().max(b_chars.len()) as f64;
    common as f64 / max_len
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn suggest_finds_matching_stem() {
        let workspace = tempdir().unwrap();
        fs::write(workspace.path().join("config.toml"), "").unwrap();
        fs::write(workspace.path().join("config.yaml"), "").unwrap();
        fs::write(workspace.path().join("other.txt"), "").unwrap();

        let target = std::path::Path::new("config.json");
        let suggestions = suggest_similar_files(target, workspace.path(), 3);
        assert!(!suggestions.is_empty(), "should find similar files");
    }

    #[test]
    fn suggest_returns_empty_when_no_matches() {
        let workspace = tempdir().unwrap();
        let target = std::path::Path::new("completely_unique_name.xyz");
        let suggestions = suggest_similar_files(target, workspace.path(), 3);
        assert!(suggestions.is_empty(), "should find no matches");
    }

    #[test]
    fn char_overlap_scoring() {
        assert!(char_overlap("main", "man") >= 0.6, "man~main should match");
        assert!(char_overlap("config", "config") > 0.99, "identical");
        assert_eq!(char_overlap("", "foo"), 0.0, "empty string");
        assert!(char_overlap("abc", "xyz") < 0.4, "no overlap");
    }

    #[test]
    fn format_suggestions_empty() {
        assert!(format_suggestions(&[]).is_empty());
    }

    #[test]
    fn format_suggestions_nonempty() {
        let msg = format_suggestions(&["main.rs".to_string(), "config.rs".to_string()]);
        assert!(msg.contains("Did you mean"));
        assert!(msg.contains("main.rs"));
    }
}
