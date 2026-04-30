//! Topic file loader: on-demand loading of topic-specific memory files.
//!
//! Topic files are markdown files stored in a `topics/` subdirectory of the
//! memory directory. They are discovered by scanning the directory, matched
//! by keyword, and loaded on demand with LRU caching.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Maximum number of topic files kept in the LRU cache.
const MAX_CACHE_SIZE: usize = 10;

/// A loaded topic file with metadata.
#[derive(Debug, Clone)]
pub struct TopicFile {
    /// Human-readable name derived from the filename.
    pub name: String,
    /// Absolute path to the topic file.
    pub path: PathBuf,
    /// Full content of the topic file.
    pub content: String,
    /// Keywords extracted from the file (first line keywords comment, or filename).
    pub keywords: Vec<String>,
    /// Number of lines in the file.
    pub line_count: usize,
}

/// Loader for topic files with LRU caching.
///
/// Scans the topics directory once, then loads files on demand.
/// Keeps at most `MAX_CACHE_SIZE` files in memory.
#[derive(Debug)]
pub struct TopicLoader {
    /// Root directory containing topic files.
    topics_dir: PathBuf,
    /// Cache of loaded topic files keyed by filename stem.
    cache: HashMap<String, TopicFile>,
    /// Order of access for LRU eviction (most recently used at the back).
    access_order: Vec<String>,
}

impl TopicLoader {
    /// Create a new topic loader pointing at the given topics directory.
    pub fn new(topics_dir: &Path) -> Self {
        Self {
            topics_dir: topics_dir.to_path_buf(),
            cache: HashMap::new(),
            access_order: Vec::new(),
        }
    }

    /// Scan the topics directory and return discovered file stems.
    ///
    /// Returns a list of (name, path) pairs for all `.md` files found.
    /// Does not load file contents.
    pub fn scan_directory(&self) -> Result<Vec<(String, PathBuf)>> {
        if !self.topics_dir.exists() {
            return Ok(Vec::new());
        }

        let mut entries = Vec::new();
        let entries_iter = fs::read_dir(&self.topics_dir).with_context(|| {
            format!(
                "Failed to read topics directory {}",
                self.topics_dir.display()
            )
        })?;

        for entry in entries_iter {
            let entry = entry.with_context(|| "Failed to read directory entry")?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                entries.push((name, path));
            }
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }

    /// Load a topic file by keyword match.
    ///
    /// Searches all topic files for ones whose name or keywords contain the
    /// given keyword (case-insensitive). Returns the first match.
    /// Results are cached.
    pub fn load_by_keyword(&mut self, keyword: &str) -> Result<Option<TopicFile>> {
        let keyword_lower = keyword.to_lowercase();

        // Check cache first — collect matches to avoid borrow conflicts
        let cache_match: Option<(String, TopicFile)> = self
            .cache
            .iter()
            .find(|(name, cached)| {
                name.to_lowercase().contains(&keyword_lower)
                    || cached
                        .keywords
                        .iter()
                        .any(|k| k.to_lowercase().contains(&keyword_lower))
            })
            .map(|(name, cached)| (name.clone(), cached.clone()));

        if let Some((name, cached)) = cache_match {
            self.touch_cache(&name);
            return Ok(Some(cached));
        }

        // Scan directory for matches
        let discovered = self.scan_directory()?;
        for (name, path) in &discovered {
            if name.to_lowercase().contains(&keyword_lower) {
                let topic = self.load_file(name, path)?;
                return Ok(Some(topic));
            }
        }

        // Try loading each undiscovered file and checking keywords
        for (name, path) in &discovered {
            if self.cache.contains_key(name) {
                continue;
            }
            let topic = self.load_file(name, path)?;
            if topic
                .keywords
                .iter()
                .any(|k| k.to_lowercase().contains(&keyword_lower))
            {
                return Ok(Some(topic));
            }
        }

        Ok(None)
    }

    /// Load a topic file by exact name (filename stem).
    pub fn load_by_name(&mut self, name: &str) -> Result<Option<TopicFile>> {
        if let Some(cached) = self.cache.get(name).cloned() {
            self.touch_cache(name);
            return Ok(Some(cached));
        }

        let path = self.topics_dir.join(format!("{name}.md"));
        if !path.exists() {
            return Ok(None);
        }

        let topic = self.load_file(name, &path)?;
        Ok(Some(topic))
    }

    /// Load a single file into the cache.
    fn load_file(&mut self, name: &str, path: &Path) -> Result<TopicFile> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read topic file {}", path.display()))?;

        let keywords = extract_keywords(&content, name);
        let line_count = content.lines().count();

        let topic = TopicFile {
            name: name.to_string(),
            path: path.to_path_buf(),
            content,
            keywords,
            line_count,
        };

        self.insert_cache(name.to_string(), topic.clone());

        Ok(topic)
    }

    /// Insert into cache, evicting the least recently used entry if at capacity.
    fn insert_cache(&mut self, name: String, topic: TopicFile) {
        if self.cache.len() >= MAX_CACHE_SIZE && !self.cache.contains_key(&name) {
            if let Some(evict_name) = self.access_order.first().cloned() {
                self.cache.remove(&evict_name);
                self.access_order.retain(|n| n != &evict_name);
                debug!("evicted topic from cache: {evict_name}");
            }
        }

        self.access_order.retain(|n| n != &name);
        self.access_order.push(name.clone());
        self.cache.insert(name, topic);
    }

    /// Touch a cache entry to mark it as recently used.
    fn touch_cache(&mut self, name: &str) {
        self.access_order.retain(|n| n != name);
        self.access_order.push(name.to_string());
    }

    /// Clear the cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }

    /// Return the number of cached files.
    #[must_use]
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

/// Extract keywords from a topic file.
///
/// Looks for a line starting with `<!-- keywords: ... -->` at the top of the file.
/// Falls back to deriving keywords from the filename.
fn extract_keywords(content: &str, fallback_name: &str) -> Vec<String> {
    for line in content.lines().take(5) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("<!-- keywords: ") {
            if let Some(kw_str) = rest.strip_suffix(" -->") {
                return kw_str
                    .split(',')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect();
            }
        }
    }

    fallback_name
        .split(&['-', '_', ' '])
        .map(String::from)
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("rustycode-topic-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn make_topics_dir() -> PathBuf {
        let dir = temp_dir();
        let topics = dir.join("topics");
        std::fs::create_dir_all(&topics).unwrap();
        topics
    }

    fn write_topic(dir: &Path, name: &str, content: &str) {
        std::fs::write(dir.join(format!("{name}.md")), content).unwrap();
    }

    #[test]
    fn scan_empty_directory() {
        let dir = make_topics_dir();
        let loader = TopicLoader::new(&dir);
        let entries = loader.scan_directory().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_nonexistent_directory() {
        let dir = temp_dir().join("nonexistent-topics");
        let loader = TopicLoader::new(&dir);
        let entries = loader.scan_directory().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_finds_markdown_files() {
        let dir = make_topics_dir();
        write_topic(&dir, "api-patterns", "# API Patterns\n\nContent here.");
        write_topic(&dir, "testing", "# Testing\n\nTest content.");

        let loader = TopicLoader::new(&dir);
        let entries = loader.scan_directory().unwrap();
        assert_eq!(entries.len(), 2);

        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"api-patterns"));
        assert!(names.contains(&"testing"));
    }

    #[test]
    fn scan_ignores_non_markdown() {
        let dir = make_topics_dir();
        write_topic(&dir, "good", "# Good");
        std::fs::write(dir.join("ignore.txt"), "not a topic").unwrap();

        let loader = TopicLoader::new(&dir);
        let entries = loader.scan_directory().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, "good");
    }

    #[test]
    fn load_by_name_returns_content() {
        let dir = make_topics_dir();
        write_topic(&dir, "api", "# API Patterns\n\nUse REST for external APIs.");

        let mut loader = TopicLoader::new(&dir);
        let topic = loader.load_by_name("api").unwrap().unwrap();
        assert_eq!(topic.name, "api");
        assert!(topic.content.contains("REST"));
        assert!(topic.line_count > 0);
    }

    #[test]
    fn load_by_name_missing_returns_none() {
        let dir = make_topics_dir();
        let mut loader = TopicLoader::new(&dir);
        let result = loader.load_by_name("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_by_keyword_matches_name() {
        let dir = make_topics_dir();
        write_topic(&dir, "api-patterns", "# API Patterns\n\nContent.");

        let mut loader = TopicLoader::new(&dir);
        let topic = loader.load_by_keyword("api").unwrap().unwrap();
        assert_eq!(topic.name, "api-patterns");
    }

    #[test]
    fn load_by_keyword_matches_explicit_keywords() {
        let dir = make_topics_dir();
        write_topic(
            &dir,
            "deploy",
            "<!-- keywords: deployment, kubernetes, k8s -->\n# Deploy\n\nDeploy content.",
        );

        let mut loader = TopicLoader::new(&dir);
        let topic = loader.load_by_keyword("kubernetes").unwrap().unwrap();
        assert_eq!(topic.name, "deploy");
    }

    #[test]
    fn load_by_keyword_no_match_returns_none() {
        let dir = make_topics_dir();
        write_topic(&dir, "api", "# API\n\nContent.");

        let mut loader = TopicLoader::new(&dir);
        let result = loader.load_by_keyword("quantum").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn cache_stores_loaded_files() {
        let dir = make_topics_dir();
        write_topic(&dir, "api", "# API");
        write_topic(&dir, "test", "# Test");

        let mut loader = TopicLoader::new(&dir);
        assert_eq!(loader.cache_size(), 0);

        loader.load_by_name("api").unwrap();
        assert_eq!(loader.cache_size(), 1);

        loader.load_by_name("test").unwrap();
        assert_eq!(loader.cache_size(), 2);
    }

    #[test]
    fn cache_hits_avoid_disk_read() {
        let dir = make_topics_dir();
        write_topic(&dir, "cached", "# Cached Topic\n\nOriginal content.");

        let mut loader = TopicLoader::new(&dir);
        let first = loader.load_by_name("cached").unwrap().unwrap();

        write_topic(&dir, "cached", "# Cached Topic\n\nUpdated content.");

        let second = loader.load_by_name("cached").unwrap().unwrap();
        assert_eq!(second.content, first.content);
        assert!(second.content.contains("Original"));
    }

    #[test]
    fn cache_evicts_at_max_capacity() {
        let dir = make_topics_dir();
        for i in 0..12 {
            write_topic(&dir, &format!("topic-{i}"), &format!("# Topic {i}"));
        }

        let mut loader = TopicLoader::new(&dir);
        for i in 0..12 {
            loader.load_by_name(&format!("topic-{i}")).unwrap();
        }

        assert!(
            loader.cache_size() <= MAX_CACHE_SIZE,
            "cache should not exceed {MAX_CACHE_SIZE}, got {}",
            loader.cache_size(),
        );
    }

    #[test]
    fn clear_cache_resets_state() {
        let dir = make_topics_dir();
        write_topic(&dir, "api", "# API");

        let mut loader = TopicLoader::new(&dir);
        loader.load_by_name("api").unwrap();
        assert_eq!(loader.cache_size(), 1);

        loader.clear_cache();
        assert_eq!(loader.cache_size(), 0);
    }

    #[test]
    fn extract_keywords_from_html_comment() {
        let content = "<!-- keywords: rust, cargo, build -->\n# Rust\n\nContent";
        let kws = extract_keywords(content, "fallback");
        assert_eq!(kws, vec!["rust", "cargo", "build"]);
    }

    #[test]
    fn extract_keywords_fallback_from_filename() {
        let content = "# API Patterns\n\nNo keyword comment.";
        let kws = extract_keywords(content, "api-patterns");
        assert_eq!(kws, vec!["api", "patterns"]);
    }

    #[test]
    fn topic_file_line_count() {
        let dir = make_topics_dir();
        write_topic(&dir, "multi", "line1\nline2\nline3\nline4\nline5");

        let mut loader = TopicLoader::new(&dir);
        let topic = loader.load_by_name("multi").unwrap().unwrap();
        assert_eq!(topic.line_count, 5);
    }
}
