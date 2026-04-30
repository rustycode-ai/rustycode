use crate::registry::SkillRegistry;
use crate::types::SkillSource;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tracing::warn;

pub struct SkillWatcher {
    watched_dirs: HashSet<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: PathBuf,
    pub kind: WatchEventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    Created,
    Modified,
    Removed,
}

impl SkillWatcher {
    pub fn new() -> Self {
        Self {
            watched_dirs: HashSet::new(),
        }
    }

    pub fn watch_dir(&mut self, dir: PathBuf) -> bool {
        if dir.exists() && dir.is_dir() {
            self.watched_dirs.insert(dir)
        } else {
            false
        }
    }

    pub const fn watched_dirs(&self) -> &HashSet<PathBuf> {
        &self.watched_dirs
    }

    pub fn watch_count(&self) -> usize {
        self.watched_dirs.len()
    }

    pub fn poll_changes(&self) -> Vec<WatchEvent> {
        let mut changes = Vec::new();

        for dir in &self.watched_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let skill_dir = entry.path();
                    if skill_dir.is_dir() {
                        let skill_md = skill_dir.join("SKILL.md");
                        if skill_md.exists() {
                            if let Ok(metadata) = std::fs::metadata(&skill_md) {
                                if let Ok(modified) = metadata.modified() {
                                    if let Ok(since) = modified.elapsed() {
                                        if since < Duration::from_millis(300) {
                                            changes.push(WatchEvent {
                                                path: skill_md,
                                                kind: WatchEventKind::Modified,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        changes
    }

    pub fn reload_on_changes(&self, registry: &mut SkillRegistry, source: SkillSource) -> usize {
        let changes = self.poll_changes();
        if changes.is_empty() {
            return 0;
        }

        let mut reloaded = 0;
        let mut parent_dirs: HashSet<PathBuf> = HashSet::new();

        for change in &changes {
            if let Some(parent) = change.path.parent() {
                parent_dirs.insert(parent.to_path_buf());
            }
        }

        for dir in &parent_dirs {
            let before = registry.active_count() + registry.conditional_count();
            if let Err(e) = registry.load_from_dir(dir, source) {
                warn!("Failed to reload skills from {:?}: {}", dir, e);
                continue;
            }
            let after = registry.active_count() + registry.conditional_count();
            if after > before {
                reloaded += after - before;
            }
        }

        reloaded
    }
}

impl Default for SkillWatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub struct DebouncedWatcher {
    inner: SkillWatcher,
    debounce_ms: u64,
}

impl DebouncedWatcher {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            inner: SkillWatcher::new(),
            debounce_ms,
        }
    }

    pub fn watch_dir(&mut self, dir: PathBuf) -> bool {
        self.inner.watch_dir(dir)
    }

    pub fn poll_with_debounce(&self) -> Vec<WatchEvent> {
        std::thread::sleep(Duration::from_millis(self.debounce_ms));
        self.inner.poll_changes()
    }

    pub fn reload_on_changes(&self, registry: &mut SkillRegistry, source: SkillSource) -> usize {
        let changes = self.poll_with_debounce();
        if changes.is_empty() {
            return 0;
        }

        let mut parent_dirs: HashSet<PathBuf> = HashSet::new();
        for change in &changes {
            if let Some(parent) = change.path.parent() {
                parent_dirs.insert(parent.to_path_buf());
            }
        }

        let mut reloaded = 0;
        for dir in &parent_dirs {
            let before = registry.active_count() + registry.conditional_count();
            if let Err(e) = registry.load_from_dir(dir, source) {
                warn!("Failed to reload skills from {:?}: {}", dir, e);
                continue;
            }
            let after = registry.active_count() + registry.conditional_count();
            if after > before {
                reloaded += after - before;
            }
        }

        reloaded
    }

    pub fn watch_count(&self) -> usize {
        self.inner.watch_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-watcher-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn new_watcher_is_empty() {
        let w = SkillWatcher::new();
        assert_eq!(w.watch_count(), 0);
    }

    #[test]
    fn default_watcher_is_empty() {
        let w = SkillWatcher::default();
        assert_eq!(w.watch_count(), 0);
    }

    #[test]
    fn watch_existing_dir() {
        let dir = temp_dir();
        let mut w = SkillWatcher::new();
        assert!(w.watch_dir(dir.clone()));
        assert_eq!(w.watch_count(), 1);
    }

    #[test]
    fn watch_nonexistent_dir_ignored() {
        let mut w = SkillWatcher::new();
        assert!(!w.watch_dir(PathBuf::from("/nonexistent")));
        assert_eq!(w.watch_count(), 0);
    }

    #[test]
    fn watch_duplicate_dir() {
        let dir = temp_dir();
        let mut w = SkillWatcher::new();
        w.watch_dir(dir.clone());
        w.watch_dir(dir.clone());
        assert_eq!(w.watch_count(), 1);
    }

    #[test]
    fn poll_changes_empty() {
        let dir = temp_dir();
        let mut w = SkillWatcher::new();
        w.watch_dir(dir);
        let changes = w.poll_changes();
        assert!(changes.is_empty());
    }

    #[test]
    fn reload_on_changes_no_changes() {
        let dir = temp_dir();
        let mut w = SkillWatcher::new();
        w.watch_dir(dir);
        let mut reg = SkillRegistry::new();
        let count = w.reload_on_changes(&mut reg, SkillSource::User);
        assert_eq!(count, 0);
    }

    #[test]
    fn debounced_watcher_new() {
        let w = DebouncedWatcher::new(300);
        assert_eq!(w.watch_count(), 0);
    }

    #[test]
    fn debounced_watcher_watch_dir() {
        let dir = temp_dir();
        let mut w = DebouncedWatcher::new(300);
        assert!(w.watch_dir(dir));
        assert_eq!(w.watch_count(), 1);
    }
}
