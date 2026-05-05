//! Search functionality for messages and providers
#![allow(dead_code)]

/// Search state for message search
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MessageSearchState {
    pub is_active: bool,
    pub query: String,
    pub matches: Vec<usize>,
    pub current_match: usize,
}

impl MessageSearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start search mode
    pub fn start(&mut self) {
        self.is_active = true;
        self.query.clear();
        self.matches.clear();
        self.current_match = 0;
    }

    /// Exit search mode
    pub fn exit(&mut self) {
        self.is_active = false;
        self.query.clear();
        self.matches.clear();
        self.current_match = 0;
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn has_matches(&self) -> bool {
        !self.matches.is_empty()
    }

    /// Get number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// Get current match index (1-based)
    pub fn current_match_number(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current_match + 1
        }
    }

    /// Navigate to next search result
    pub fn next_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        self.current_match = (self.current_match + 1) % self.matches.len();
        Some(self.matches[self.current_match])
    }

    /// Navigate to previous search result
    pub fn prev_match(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }

        self.current_match = if self.current_match == 0 {
            self.matches.len() - 1
        } else {
            self.current_match - 1
        };
        Some(self.matches[self.current_match])
    }

    /// Add a character to the search query
    pub fn add_char(&mut self, c: char) {
        self.query.push(c);
    }

    /// Remove the last character from the search query
    pub fn remove_char(&mut self) {
        self.query.pop();
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
    }

    /// Get the search query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Clear matches and go to first result
    pub fn clear_matches(&mut self) {
        self.matches.clear();
        self.current_match = 0;
    }

    /// Set search matches and jump to first result
    pub fn set_matches(&mut self, matches: Vec<usize>) -> Option<usize> {
        self.matches = matches;
        self.current_match = 0;
        // Activate search state when matches are set
        self.is_active = true;

        if !self.matches.is_empty() {
            Some(self.matches[0])
        } else {
            None
        }
    }

    /// Get all match indices
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }
}

/// Perform case-insensitive search in text
///
pub fn search_texts<'a, I>(query: &str, texts: I) -> Vec<usize>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    texts
        .filter(|(_, text)| text.to_lowercase().contains(&query_lower))
        .map(|(i, _)| i)
        .collect()
}

/// Perform search and return first match index
///
pub fn perform_search<'a, I>(
    search_state: &mut MessageSearchState,
    query: &str,
    texts: I,
) -> Option<usize>
where
    I: Iterator<Item = (usize, &'a str)>,
{
    search_state.set_query(query.to_string());

    let matches = search_texts(query, texts);
    search_state.set_matches(matches)
}

/// Provider search state for filtering providers
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ProviderSearchState {
    pub is_active: bool,
    pub query: String,
    pub selected_index: usize,
}

impl ProviderSearchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start provider search mode
    pub fn start(&mut self) {
        self.is_active = true;
        self.query.clear();
        self.selected_index = 0;
    }

    /// Cancel provider search mode
    pub fn cancel(&mut self) {
        self.is_active = false;
        self.query.clear();
        self.selected_index = 0;
    }

    /// Check if provider search is active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Update search query
    pub fn update_query(&mut self, query: String) {
        self.query = query;
        self.selected_index = 0; // Reset selection when query changes
    }

    /// Add a character to the search query
    pub fn add_char(&mut self, c: char) {
        self.query.push(c);
        self.selected_index = 0;
    }

    /// Remove the last character from the search query
    pub fn remove_char(&mut self) {
        self.query.pop();
        self.selected_index = 0;
    }

    /// Get the search query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Move selection up
    pub fn move_up(&mut self, _max_index: usize) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self, max_index: usize) {
        if self.selected_index < max_index.saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    /// Get selected index
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Set selected index
    pub fn set_selected_index(&mut self, index: usize) {
        self.selected_index = index;
    }
}

/// Filter items by search query with fuzzy matching support
///
#[cfg(test)]
pub fn filter_items<'a, I, S>(query: &str, items: I) -> Vec<usize>
where
    I: Iterator<Item = (usize, &'a S)>,
    S: AsRef<str> + 'a,
{
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    items
        .filter(|(_, strings)| {
            let text = strings.as_ref().to_lowercase();
            text.contains(&query_lower)
        })
        .map(|(i, _)| i)
        .collect()
}

/// Multi-string search (search across multiple fields per item)
///
#[cfg(test)]
pub fn filter_items_multi<'a, I>(query: &str, items: I) -> Vec<usize>
where
    I: Iterator<Item = (usize, &'a [&'a str])>,
{
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    items
        .filter(|(_, strings)| {
            strings
                .iter()
                .any(|s| s.to_lowercase().contains(&query_lower))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Highlight search matches in text
///
#[cfg(test)]
pub fn highlight_matches(
    text: &str,
    query: &str,
    highlight_prefix: &str,
    highlight_suffix: &str,
) -> String {
    if query.is_empty() {
        return text.to_string();
    }

    let query_lower = query.to_lowercase();
    let text_lower = text.to_lowercase();

    let mut result = String::new();
    let mut last_pos = 0;

    while let Some(pos) = text_lower[last_pos..].find(&query_lower) {
        let abs_pos = last_pos + pos;

        // Add text before match
        result.push_str(&text[last_pos..abs_pos]);

        // Add highlighted match
        let match_len = query_lower.len();
        let match_end = abs_pos + match_len;

        result.push_str(highlight_prefix);
        if text.is_char_boundary(match_end) {
            result.push_str(&text[abs_pos..match_end]);
        } else {
            let matched_lower = &text_lower[abs_pos..match_end];
            let char_count = matched_lower.chars().count();
            let actual_end = text[abs_pos..]
                .char_indices()
                .nth(char_count)
                .map(|(i, _)| abs_pos + i)
                .unwrap_or_else(|| text.len());
            result.push_str(&text[abs_pos..actual_end]);
        }
        result.push_str(highlight_suffix);

        last_pos = match_end;
    }

    // Add remaining text
    result.push_str(&text[last_pos..]);

    result
}

/// Get unique search terms from a query (splits by whitespace)
///
#[cfg(test)]
pub fn parse_search_terms(query: &str) -> Vec<String> {
    use std::collections::HashSet;
    query
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_state_new() {
        let state = MessageSearchState::new();
        assert!(!state.is_active());
        assert!(state.query().is_empty());
        assert!(!state.has_matches());
        assert_eq!(state.match_count(), 0);
    }

    #[test]
    fn test_search_state_start_exit() {
        let mut state = MessageSearchState::new();

        state.start();
        assert!(state.is_active());

        state.exit();
        assert!(!state.is_active());
        assert!(state.query().is_empty());
        assert!(!state.has_matches());
    }

    #[test]
    fn test_search_state_navigation() {
        let mut state = MessageSearchState::new();
        state.set_matches(vec![1, 5, 10]);

        assert_eq!(state.current_match_number(), 1);
        assert_eq!(state.next_match(), Some(5));
        assert_eq!(state.current_match_number(), 2);

        assert_eq!(state.next_match(), Some(10));
        assert_eq!(state.current_match_number(), 3);

        // Wrap around
        assert_eq!(state.next_match(), Some(1));
        assert_eq!(state.current_match_number(), 1);

        // Previous
        assert_eq!(state.prev_match(), Some(10));
        assert_eq!(state.current_match_number(), 3);
    }

    #[test]
    fn test_search_state_navigation_no_matches() {
        let mut state = MessageSearchState::new();
        assert_eq!(state.next_match(), None);
        assert_eq!(state.prev_match(), None);
    }

    #[test]
    fn test_search_state_query_modification() {
        let mut state = MessageSearchState::new();

        state.add_char('a');
        state.add_char('b');
        assert_eq!(state.query(), "ab");

        state.remove_char();
        assert_eq!(state.query(), "a");

        state.set_query("test".to_string());
        assert_eq!(state.query(), "test");
    }

    #[test]
    fn test_search_texts() {
        let texts = [(0, "Hello world"), (1, "Goodbye world"), (2, "Hello Rust")];

        let results = search_texts("hello", texts.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![0, 2]);

        let results = search_texts("world", texts.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![0, 1]);

        // Case insensitive
        let results = search_texts("WORLD", texts.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![0, 1]);

        // No matches
        let results = search_texts("nonexistent", texts.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, Vec::<usize>::new());
    }

    #[test]
    fn test_perform_search() {
        let mut state = MessageSearchState::new();
        let texts = [(0, "Hello world"), (1, "Goodbye world"), (2, "Hello Rust")];

        let first_match = perform_search(&mut state, "hello", texts.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(first_match, Some(0));
        assert_eq!(state.match_count(), 2);
        assert!(state.is_active());
    }

    #[test]
    fn test_provider_search_state() {
        let mut state = ProviderSearchState::new();

        state.start();
        assert!(state.is_active());
        assert_eq!(state.selected_index(), 0);

        state.add_char('a');
        assert_eq!(state.query(), "a");
        assert_eq!(state.selected_index(), 0); // Reset on char add

        state.remove_char();
        assert_eq!(state.query(), "");

        state.move_down(10);
        assert_eq!(state.selected_index(), 1);

        state.move_up(10);
        assert_eq!(state.selected_index(), 0);

        state.cancel();
        assert!(!state.is_active());
    }

    #[test]
    fn test_filter_items() {
        let items = [
            (0, "Anthropic Claude"),
            (1, "OpenAI GPT"),
            (2, "Ollama Local"),
        ];

        let results = filter_items("anthropic", items.iter().map(|(i, t)| (*i, t)));
        assert_eq!(results, vec![0]);

        let results = filter_items("", items.iter().map(|(i, t)| (*i, t)));
        assert_eq!(results, Vec::<usize>::new());
    }

    #[test]
    fn test_filter_items_multi() {
        let items = [
            (0, &["Anthropic", "Claude", "Advanced"][..]),
            (1, &["OpenAI", "GPT", "Versatile"][..]),
            (2, &["Ollama", "Local", "No key"][..]),
        ];

        let results = filter_items_multi("anthropic", items.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![0]);

        let results = filter_items_multi("versatile", items.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![1]);

        let results = filter_items_multi("local", items.iter().map(|(i, t)| (*i, *t)));
        assert_eq!(results, vec![2]);
    }

    #[test]
    fn test_highlight_matches() {
        let text = "Hello world, hello Rust";
        let highlighted = highlight_matches(text, "hello", "[", "]");

        assert_eq!(highlighted, "[Hello] world, [hello] Rust");
    }

    #[test]
    fn test_highlight_matches_empty_query() {
        let text = "Hello world";
        let highlighted = highlight_matches(text, "", "[", "]");

        assert_eq!(highlighted, "Hello world");
    }

    #[test]
    fn test_parse_search_terms() {
        let terms = parse_search_terms("hello world test");
        assert_eq!(terms.len(), 3);
        assert!(terms.contains(&"hello".to_string()));
        assert!(terms.contains(&"world".to_string()));
        assert!(terms.contains(&"test".to_string()));
    }

    #[test]
    fn test_parse_search_terms_duplicates() {
        let terms = parse_search_terms("hello hello world");
        assert_eq!(terms.len(), 2); // Duplicates removed
    }

    #[test]
    fn test_parse_search_terms_whitespace() {
        let terms = parse_search_terms("  hello   world  ");
        assert_eq!(terms.len(), 2);
    }

    #[test]
    fn test_search_state_default() {
        let state = MessageSearchState::default();
        assert!(!state.is_active());
        assert!(state.query().is_empty());
        assert!(!state.has_matches());
    }

    #[test]
    fn test_provider_search_state_default() {
        let state = ProviderSearchState::default();
        assert!(!state.is_active());
        assert!(state.query().is_empty());
        assert_eq!(state.selected_index(), 0);
    }
}
