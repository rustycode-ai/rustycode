//! Artifact collection from trial output directories.
//!
//! Uses the `ignore` crate for gitignore-style glob exclusion.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Configuration for which files to collect.
#[derive(Debug, Clone)]
pub struct ArtifactFilter {
    /// Root directory to scan.
    pub root: PathBuf,
    /// Glob patterns to exclude (gitignore-style).
    pub exclude_patterns: Vec<String>,
}

/// Collected artifact with metadata.
#[derive(Debug, Clone)]
pub struct Artifact {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub size_bytes: u64,
}

impl ArtifactFilter {
    /// Collect all non-excluded files under root.
    pub fn collect(&self) -> Result<Vec<Artifact>> {
        let mut artifacts = Vec::new();

        let mut builder = ignore::WalkBuilder::new(&self.root);
        builder.hidden(false);
        builder.git_ignore(false);
        builder.git_global(false);
        builder.git_exclude(false);

        // Add custom exclude patterns via a temporary ignore file
        // The ignore crate's add_custom_ignore_filename expects a filename,
        // not a glob pattern. For glob-based exclusion, we filter post-walk.
        for entry in builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::debug!("Skipping artifact entry: {e}");
                    continue;
                }
            };

            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let path = entry.path();
            let relative = path.strip_prefix(&self.root).unwrap_or(path).to_path_buf();

            // Apply glob-based exclusion patterns
            let rel_str = relative.to_string_lossy();
            let excluded = self.exclude_patterns.iter().any(|pattern| {
                if let Ok(glob) = globset::GlobBuilder::new(pattern)
                    .literal_separator(true)
                    .build()
                {
                    let matcher = glob.compile_matcher();
                    matcher.is_match(rel_str.as_ref())
                        || matcher.is_match(path.file_name().unwrap_or_default())
                } else {
                    // Simple string match for directory names
                    rel_str.contains(pattern)
                        || path
                            .file_name()
                            .is_some_and(|f| f == std::ffi::OsStr::new(pattern))
                }
            });

            if excluded {
                continue;
            }

            let metadata = entry
                .metadata()
                .with_context(|| format!("reading metadata for {}", path.display()))
                .ok();

            artifacts.push(Artifact {
                relative_path: relative,
                absolute_path: path.to_path_buf(),
                size_bytes: metadata.map(|m| m.len()).unwrap_or(0),
            });
        }

        artifacts.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        Ok(artifacts)
    }
}

/// Collect artifacts from a trial directory, excluding common noise.
pub fn collect_trial_artifacts(trial_dir: &Path) -> Result<Vec<Artifact>> {
    let filter = ArtifactFilter {
        root: trial_dir.to_path_buf(),
        exclude_patterns: vec![
            "*.log".to_string(),
            "target".to_string(),
            "__pycache__".to_string(),
            "node_modules".to_string(),
            ".git".to_string(),
        ],
    };
    filter.collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn collect_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec![],
        };
        let artifacts = filter.collect().unwrap();
        assert!(artifacts.is_empty());
    }

    #[test]
    fn collect_with_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("output.txt"), "hello").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/data.json"), "{}").unwrap();

        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec![],
        };
        let artifacts = filter.collect().unwrap();
        assert_eq!(artifacts.len(), 2);
        assert!(artifacts[0]
            .relative_path
            .to_string_lossy()
            .contains("output"));
    }

    #[test]
    fn collect_excludes_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), "data").unwrap();
        std::fs::write(dir.path().join("noise.log"), "log").unwrap();

        let filter = ArtifactFilter {
            root: dir.path().to_path_buf(),
            exclude_patterns: vec!["*.log".to_string()],
        };
        let artifacts = filter.collect().unwrap();
        assert_eq!(artifacts.len(), 1);
        assert!(artifacts[0]
            .relative_path
            .to_string_lossy()
            .contains("keep"));
    }
}
