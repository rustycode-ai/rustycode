use super::{CodeIndex, MatchType, SearchResult, Symbol};
use std::collections::HashSet;
use std::path::Path;

impl CodeIndex {
    /// Search for code matching a pattern using trigram index
    pub fn search(&self, pattern: &str) -> Vec<SearchResult> {
        let trigram_results = self
            .trigram_index
            .search(pattern, &self.trigram_index.files);
        let mut results = Vec::new();

        for (file_path, line_num) in trigram_results {
            let context = self.get_line_context(&file_path, line_num, 1);
            results.push(SearchResult {
                file_path,
                line: line_num,
                column: 0,
                context,
                match_type: MatchType::TrigramMatch,
                score: 1.0,
            });
        }

        // Also check exact word matches (higher priority)
        let words: Vec<&str> = pattern.split_whitespace().collect();
        for word in words {
            if word.len() >= 2 {
                let word_results = self.word_index.lookup(word);
                for (file_idx, line_num) in word_results {
                    if let Some(file_path) = self.trigram_index.files.get(file_idx) {
                        let context = self.get_line_context(file_path, line_num, 1);
                        results.push(SearchResult {
                            file_path: file_path.clone(),
                            line: line_num,
                            column: 0,
                            context,
                            match_type: MatchType::WordMatch,
                            score: 2.0,
                        });
                    }
                }
            }
        }

        // Also check symbol matches (highest priority)
        let symbol_results = self.symbol_index.lookup(pattern);
        for symbol in symbol_results {
            let context = self.get_line_context(&symbol.file_path, symbol.line, 1);
            results.push(SearchResult {
                file_path: symbol.file_path.clone(),
                line: symbol.line,
                column: 0,
                context,
                match_type: MatchType::ExactSymbol,
                score: 3.0,
            });
        }

        // Sort by score (highest first) and deduplicate
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate by (file, line)
        let mut seen = HashSet::new();
        results.retain(|r| seen.insert((r.file_path.clone(), r.line)));

        results
    }

    /// Format results as token-efficient structured text for LLM consumption
    pub fn format_results(&self, results: &[SearchResult]) -> String {
        if results.is_empty() {
            return "No results found.".to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} results:\n", results.len()));

        for result in results {
            let rel_path = result
                .file_path
                .strip_prefix(&self.root)
                .unwrap_or(&result.file_path);
            let match_icon = match result.match_type {
                MatchType::ExactSymbol => "S",
                MatchType::WordMatch => "W",
                MatchType::TrigramMatch => "T",
                MatchType::PrefixMatch => "P",
            };
            output.push_str(&format!(
                " {}:{}:{} | {}\n",
                match_icon,
                rel_path.display(),
                result.line,
                result.context.trim()
            ));
        }

        output
    }

    /// Format symbols as token-efficient structured text
    pub fn format_symbols(&self, symbols: &[&Symbol]) -> String {
        if symbols.is_empty() {
            return "No symbols found.".to_string();
        }

        let mut output = String::new();
        output.push_str(&format!("Found {} symbols:\n", symbols.len()));

        for symbol in symbols {
            let rel_path = symbol
                .file_path
                .strip_prefix(&self.root)
                .unwrap_or(&symbol.file_path);
            let parent_str = symbol
                .parent
                .as_ref()
                .map(|p| format!(" in {p}"))
                .unwrap_or_default();
            let sig = symbol
                .signature
                .as_ref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            output.push_str(&format!(
                " {} {} @ {}:{}{}{}\n",
                symbol.kind,
                symbol.name,
                rel_path.display(),
                symbol.line,
                parent_str,
                sig,
            ));
        }

        output
    }

    /// Get line context from cached file
    pub(crate) fn get_line_context(
        &self,
        file: &Path,
        line: usize,
        context_lines: usize,
    ) -> String {
        if let Some(lines) = self.file_cache.get(file) {
            let start = line.saturating_sub(context_lines + 1);
            let end = (line + context_lines).min(lines.len());
            let start = start.min(lines.len());
            let end = end.max(start).min(lines.len());
            lines[start..end].join("\n")
        } else {
            String::new()
        }
    }

    /// Get the outline of a file (all symbols, no bodies)
    pub fn file_outline(&self, file: &Path) -> String {
        let symbols = self.file_symbols(file);
        if symbols.is_empty() {
            return "No symbols found in file.".to_string();
        }

        let mut output = String::new();
        for symbol in symbols {
            let indent = if symbol.parent.is_some() { "  " } else { "" };
            let parent_str = symbol
                .parent
                .as_ref()
                .map(|p| format!("({p}) "))
                .unwrap_or_default();
            let sig = symbol
                .signature
                .as_ref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            output.push_str(&format!(
                "{}{}:{} {}{}{}\n",
                indent, symbol.line, symbol.kind, parent_str, symbol.name, sig,
            ));
        }
        output
    }
}
