use crate::registry::SkillRegistry;
use crate::types::SkillSource;
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::debug;

pub struct Discovery {
    checked_paths: HashSet<String>,
    skills_dir_name: &'static str,
}

impl Discovery {
    pub fn new() -> Self {
        Self {
            checked_paths: HashSet::new(),
            skills_dir_name: ".rustycode",
        }
    }

    pub const fn with_skills_dir_name(mut self, name: &'static str) -> Self {
        self.skills_dir_name = name;
        self
    }

    /// Walk up from each file path toward the filesystem root,
    /// looking for `<skills_dir_name>/skills/` directories.
    /// Returns the list of skill directories found, deepest-first.
    pub fn discover_for_paths(&mut self, file_paths: &[&Path], cwd: &Path) -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        for file_path in file_paths {
            let start = if file_path.is_absolute() {
                file_path.to_path_buf()
            } else {
                cwd.join(file_path)
            };

            let ancestor_dirs = self.walk_up(&start);
            dirs.extend(ancestor_dirs);
        }

        dirs.sort_by(|a, b| {
            let a_depth = a.components().count();
            let b_depth = b.components().count();
            b_depth.cmp(&a_depth)
        });

        dirs.dedup();
        dirs
    }

    /// Load skills from all discovered directories into the registry.
    pub fn load_discovered(
        &mut self,
        registry: &mut SkillRegistry,
        file_paths: &[&Path],
        cwd: &Path,
    ) -> Result<Vec<String>> {
        let dirs = self.discover_for_paths(file_paths, cwd);
        let mut loaded = Vec::new();

        for dir in &dirs {
            let key = dir.to_string_lossy().to_string();
            if self.checked_paths.contains(&key) {
                continue;
            }

            let before = registry.active_count() + registry.conditional_count();
            registry.load_from_dir(dir, SkillSource::Project)?;
            let after = registry.active_count() + registry.conditional_count();

            let count = after - before;
            if count > 0 {
                debug!("Discovered {} skills from {}", count, key);
            }

            self.checked_paths.insert(key);
            loaded.push(dir.to_string_lossy().to_string());
        }

        Ok(loaded)
    }

    /// Reset memoized paths (e.g., when changing projects).
    pub fn reset(&mut self) {
        self.checked_paths.clear();
    }

    fn walk_up(&self, start: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut current = start;

        loop {
            let skills_dir = current.join(self.skills_dir_name).join("skills");
            if skills_dir.is_dir() {
                let key = skills_dir.to_string_lossy().to_string();
                if !self.checked_paths.contains(&key) {
                    found.push(skills_dir);
                }
            }

            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }

        found
    }
}

impl Default for Discovery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("rustycode-discovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn new_discovery_is_empty() {
        let d = Discovery::new();
        assert!(d.checked_paths.is_empty());
    }

    #[test]
    fn default_discovery_is_empty() {
        let d = Discovery::default();
        assert!(d.checked_paths.is_empty());
    }

    #[test]
    fn walk_up_finds_skills_dir() {
        let root = temp_dir();
        let project = root.join("project");
        let src = project.join("src");
        fs::create_dir_all(src.join("module")).unwrap();
        fs::create_dir_all(project.join(".rustycode").join("skills")).unwrap();

        let disc = Discovery::new();
        let file_path = src.join("module").join("main.rs");
        let found = disc.walk_up(&file_path);

        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("skills"));
    }

    #[test]
    fn walk_up_no_skills_dir() {
        let root = temp_dir();
        let project = root.join("empty-project");
        fs::create_dir_all(project.join("src")).unwrap();

        let disc = Discovery::new();
        let file_path = project.join("src").join("main.rs");
        let found = disc.walk_up(&file_path);

        assert!(found.is_empty());
    }

    #[test]
    fn discover_for_paths_deduplicates() {
        let root = temp_dir();
        let project = root.join("dedup-project");
        fs::create_dir_all(project.join(".rustycode").join("skills")).unwrap();
        fs::create_dir_all(project.join("src")).unwrap();

        let mut disc = Discovery::new();
        let cwd = project.clone();
        let found = disc.discover_for_paths(
            &[
                project.join("src/main.rs").as_path(),
                project.join("src/lib.rs").as_path(),
            ],
            &cwd,
        );

        assert_eq!(found.len(), 1);
    }

    #[test]
    fn load_discovered_loads_skills() {
        let root = temp_dir();
        let project = root.join("load-project");
        let skills_dir = project.join(".rustycode").join("skills");
        let skill_dir = skills_dir.join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: my-skill\n---\n# My Skill\n\nTest.\n",
        )
        .unwrap();

        let mut disc = Discovery::new();
        let mut reg = SkillRegistry::new();
        let loaded = disc
            .load_discovered(&mut reg, &[project.join("main.rs").as_path()], &project)
            .unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(reg.active_count(), 1);
        assert!(reg.get("my-skill").is_some());
    }

    #[test]
    fn load_discovered_memoizes() {
        let root = temp_dir();
        let project = root.join("memo-project");
        let skills_dir = project.join(".rustycode").join("skills");
        let skill_dir = skills_dir.join("s1");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: s1\n---\n# S1\n\nTest.\n",
        )
        .unwrap();

        let mut disc = Discovery::new();
        let mut reg = SkillRegistry::new();

        let first = disc
            .load_discovered(&mut reg, &[project.join("a.rs").as_path()], &project)
            .unwrap();
        assert_eq!(first.len(), 1);

        let second = disc
            .load_discovered(&mut reg, &[project.join("b.rs").as_path()], &project)
            .unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn reset_clears_memoized() {
        let root = temp_dir();
        let project = root.join("reset-project");
        let skills_dir = project.join(".rustycode").join("skills");
        fs::create_dir_all(skills_dir).unwrap();

        let mut disc = Discovery::new();
        let mut reg = SkillRegistry::new();
        let _ = disc.load_discovered(&mut reg, &[project.join("x.rs").as_path()], &project);
        assert!(!disc.checked_paths.is_empty());

        disc.reset();
        assert!(disc.checked_paths.is_empty());
    }

    #[test]
    fn deeper_overrides_shallower() {
        let root = temp_dir();
        let project = root.join("nested-project");
        let outer_skills = project.join(".rustycode").join("skills");
        let inner_dir = project.join("packages").join("lib");
        let inner_skills = inner_dir.join(".rustycode").join("skills");
        fs::create_dir_all(outer_skills).unwrap();
        fs::create_dir_all(&inner_skills).unwrap();

        let mut disc = Discovery::new();
        let found = disc.discover_for_paths(&[inner_dir.join("src/main.rs").as_path()], &project);

        assert_eq!(found.len(), 2);
        assert!(found[0].components().count() > found[1].components().count());
    }
}
