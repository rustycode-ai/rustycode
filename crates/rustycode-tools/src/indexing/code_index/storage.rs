use super::{Symbol, SymbolKind};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

// ── Trigram Index ─────────────────────────────────────────────────────────────

/// Trigram index for fast substring search
///
/// For each trigram (3-char substring), stores the set of (file, line) pairs
/// that contain it. Query intersection gives O(1) lookups for multi-trigram patterns.
pub(crate) struct TrigramIndex {
    /// trigram -> set of (`file_index`, `line_number`)
    pub index: HashMap<[u8; 3], HashSet<(usize, usize)>>,
    /// file index -> path
    pub files: Vec<PathBuf>,
}

impl TrigramIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            files: Vec::new(),
        }
    }

    pub fn insert_file(&mut self, file_idx: usize, content: &str) {
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            let line_lower: Vec<u8> = line.to_lowercase().bytes().collect();
            if line_lower.len() < 3 {
                continue;
            }
            for window in line_lower.windows(3) {
                let trigram: [u8; 3] = [window[0], window[1], window[2]];
                self.index
                    .entry(trigram)
                    .or_default()
                    .insert((file_idx, line_num));
            }
        }
    }

    pub fn search(&self, pattern: &str, files: &[PathBuf]) -> Vec<(PathBuf, usize)> {
        let pattern_lower: Vec<u8> = pattern.to_lowercase().bytes().collect();
        if pattern_lower.len() < 3 {
            return Vec::new();
        }

        // Extract trigrams from pattern
        let pattern_trigrams: Vec<[u8; 3]> = pattern_lower
            .windows(3)
            .map(|w| [w[0], w[1], w[2]])
            .collect();

        if pattern_trigrams.is_empty() {
            return Vec::new();
        }

        // Intersect results from all trigrams
        let mut candidates: Option<HashSet<(usize, usize)>> = None;
        for trigram in &pattern_trigrams {
            if let Some(matches) = self.index.get(trigram) {
                match candidates {
                    None => candidates = Some(matches.clone()),
                    Some(ref mut existing) => {
                        let intersection: HashSet<_> =
                            existing.intersection(matches).cloned().collect();
                        *existing = intersection;
                    }
                }
            } else {
                return Vec::new(); // Trigram not found = no matches
            }
        }

        candidates
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(file_idx, line)| files.get(file_idx).map(|p| (p.clone(), line)))
            .collect()
    }
}

// ── Word Index ────────────────────────────────────────────────────────────────

/// Inverted word index for exact identifier lookups
pub(crate) struct WordIndex {
    /// word (lowercase) -> set of (`file_index`, `line_number`)
    pub index: HashMap<String, HashSet<(usize, usize)>>,
}

impl WordIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    pub fn insert_file(&mut self, file_idx: usize, content: &str) {
        let mut line_num = 0;
        for line in content.lines() {
            line_num += 1;
            for word in extract_words(line) {
                self.index
                    .entry(word.to_lowercase())
                    .or_default()
                    .insert((file_idx, line_num));
            }
        }
    }

    pub fn lookup(&self, word: &str) -> Vec<(usize, usize)> {
        self.index
            .get(&word.to_lowercase())
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn prefix_search(&self, prefix: &str, limit: usize) -> Vec<String> {
        let prefix_lower = prefix.to_lowercase();
        let mut results: Vec<String> = self
            .index
            .keys()
            .filter(|w| w.starts_with(&prefix_lower))
            .cloned()
            .collect();
        results.truncate(limit);
        results
    }
}

/// Extract identifier-like words from a line of code
pub(crate) fn extract_words(line: &str) -> Vec<&str> {
    line.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .collect()
}

// ── Symbol Index ──────────────────────────────────────────────────────────────

/// Index of extracted code symbols
pub(crate) struct SymbolIndex {
    /// name (lowercase) -> list of symbols
    pub by_name: HashMap<String, Vec<Symbol>>,
    /// all symbols
    pub all: Vec<Symbol>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            by_name: HashMap::new(),
            all: Vec::new(),
        }
    }

    pub fn add(&mut self, symbol: Symbol) {
        let key = symbol.name.to_lowercase();
        self.by_name.entry(key).or_default().push(symbol.clone());
        self.all.push(symbol);
    }

    pub fn lookup(&self, name: &str) -> Vec<&Symbol> {
        self.by_name
            .get(&name.to_lowercase())
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn lookup_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.all.iter().filter(|s| s.kind == kind).collect()
    }

    pub fn all_symbols(&self) -> &[Symbol] {
        &self.all
    }
}

// ── Dependency Index ──────────────────────────────────────────────────────────

/// Tracks file dependencies for impact analysis
pub(crate) struct DependencyIndex {
    /// file -> set of files it imports/depends on
    pub imports: HashMap<PathBuf, HashSet<PathBuf>>,
    /// file -> set of files that import it (reverse deps)
    pub imported_by: HashMap<PathBuf, HashSet<PathBuf>>,
}

impl DependencyIndex {
    pub fn new() -> Self {
        Self {
            imports: HashMap::new(),
            imported_by: HashMap::new(),
        }
    }

    pub fn add_import(&mut self, from: PathBuf, to: PathBuf) {
        self.imports
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.imported_by.entry(to).or_default().insert(from);
    }

    pub fn get_dependents(&self, file: &Path) -> Vec<PathBuf> {
        self.imported_by
            .get(file)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
}
