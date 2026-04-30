//! Path utilities for orchestra directory resolution and app paths.

use std::fs;
use std::path::{Path, PathBuf};

#[allow(clippy::implicit_hasher)]
#[allow(clippy::option_if_let_else)]
pub fn app_root() -> PathBuf {
    if let Ok(path) = std::env::var("CLAUDE_APP_ROOT") {
        PathBuf::from(path)
    } else if let Some(home) = dirs::home_dir() {
        #[allow(clippy::useless_let_if_seq)]
        let result = home.join(".claude");
        result
    } else {
        PathBuf::from(".")
    }
}

pub fn agent_dir() -> PathBuf {
    app_root().join("agents")
}

pub fn orchestra_root(base_path: &Path) -> PathBuf {
    base_path.join(".orchestra")
}

pub fn milestones_dir(base_path: &Path) -> PathBuf {
    orchestra_root(base_path).join("milestones")
}

pub fn resolve_milestone_path(base_path: &Path, milestone_id: &str) -> PathBuf {
    milestones_dir(base_path).join(milestone_id)
}

pub fn build_milestone_file_name(milestone_id: &str, kind: &str) -> String {
    format!("{milestone_id}-{kind}.md")
}

pub fn resolve_milestone_file(base_path: &Path, milestone_id: &str, kind: &str) -> Option<PathBuf> {
    let path = resolve_milestone_path(base_path, milestone_id)
        .join(build_milestone_file_name(milestone_id, kind));
    path.exists().then_some(path)
}

pub fn resolve_tasks_dir(base_path: &Path, milestone_id: &str, slice_id: &str) -> Option<PathBuf> {
    let tasks_dir = orchestra_root(base_path)
        .join("milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id)
        .join("tasks");

    tasks_dir.exists().then_some(tasks_dir)
}

pub fn resolve_slice_file(
    base_path: &Path,
    milestone_id: &str,
    slice_id: &str,
    kind: &str,
) -> Option<PathBuf> {
    let file_name = match kind {
        "PLAN" => "PLAN.md".to_string(),
        "SUMMARY" => format!("{slice_id}-SUMMARY.md"),
        other => format!("{slice_id}-{other}.md"),
    };

    let path = orchestra_root(base_path)
        .join("milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id)
        .join(file_name);

    path.exists().then_some(path)
}

pub fn resolve_task_file(
    base_path: &Path,
    milestone_id: &str,
    slice_id: &str,
    task_id: &str,
    kind: &str,
) -> Option<PathBuf> {
    let file_name = format!("{task_id}-{kind}.md");
    let path = orchestra_root(base_path)
        .join("milestones")
        .join(milestone_id)
        .join("slices")
        .join(slice_id)
        .join("tasks")
        .join(task_id)
        .join(file_name);

    path.exists().then_some(path)
}

pub fn resolve_task_files(tasks_dir: &Path, kind: &str) -> Vec<PathBuf> {
    let suffix = format!("-{kind}.md");
    let mut files = Vec::new();

    let Ok(entries) = fs::read_dir(tasks_dir) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Some(task_id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let file_path = path.join(format!("{task_id}-{kind}.md"));
            if file_path.exists() {
                files.push(file_path);
            }
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.ends_with(&suffix))
        {
            files.push(path);
        }
    }

    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestra_root() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            orchestra_root(base),
            PathBuf::from("/tmp/project/.orchestra")
        );
    }

    #[test]
    fn test_milestones_dir() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            milestones_dir(base),
            PathBuf::from("/tmp/project/.orchestra/milestones")
        );
    }

    #[test]
    fn test_resolve_milestone_path() {
        let base = Path::new("/tmp/project");
        assert_eq!(
            resolve_milestone_path(base, "m1"),
            PathBuf::from("/tmp/project/.orchestra/milestones/m1")
        );
    }

    #[test]
    fn test_build_milestone_file_name() {
        assert_eq!(build_milestone_file_name("m1", "PLAN"), "m1-PLAN.md");
        assert_eq!(build_milestone_file_name("m1", "SUMMARY"), "m1-SUMMARY.md");
    }

    #[test]
    fn test_resolve_milestone_file_missing() {
        let base = Path::new("/nonexistent");
        assert!(resolve_milestone_file(base, "m1", "PLAN").is_none());
    }

    #[test]
    fn test_resolve_tasks_dir_missing() {
        let base = Path::new("/nonexistent");
        assert!(resolve_tasks_dir(base, "m1", "s1").is_none());
    }

    #[test]
    fn test_resolve_slice_file_missing() {
        let base = Path::new("/nonexistent");
        assert!(resolve_slice_file(base, "m1", "s1", "PLAN").is_none());
    }

    #[test]
    fn test_resolve_task_file_missing() {
        let base = Path::new("/nonexistent");
        assert!(resolve_task_file(base, "m1", "s1", "t1", "IMPL").is_none());
    }

    #[test]
    fn test_resolve_slice_file_plan_kind() {
        let base = Path::new("/nonexistent");
        assert!(resolve_slice_file(base, "m1", "s1", "PLAN").is_none());
    }

    #[test]
    fn test_resolve_task_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let files = resolve_task_files(dir.path(), "IMPL");
        assert!(files.is_empty());
    }

    #[test]
    fn test_resolve_task_files_with_files() {
        let dir = tempfile::tempdir().unwrap();
        let tasks_dir = dir.path().join("t1");
        fs::create_dir_all(&tasks_dir).unwrap();
        let task_file = tasks_dir.join("t1-IMPL.md");
        fs::write(&task_file, "content").unwrap();
        let files = resolve_task_files(dir.path(), "IMPL");
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("t1-IMPL.md"));
    }
}
