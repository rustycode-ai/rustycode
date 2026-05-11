//! Shared Fuzzy Matching Module

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// MATCH SCORE

/// Match relevance score for ranking search results
///
/// Higher scores indicate better matches. Scores are ordered from worst to best:
/// None < Substring < Prefix < Exact
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchScore {
    /// No match
    None = 0,
    /// Substring match (query found anywhere in text)
    Substring = 1,
    /// Prefix match (query at start of text)
    Prefix = 2,
    /// Exact match (query equals text)
    Exact = 3,
}

impl MatchScore {
    #[inline]
    pub fn is_match(self) -> bool {
        self != MatchScore::None
    }

    /// Get a numeric value for this score
    #[inline]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

// FUZZY MATCHER

/// Generic fuzzy matcher for searching collections of items
///
/// This matcher provides case-insensitive fuzzy matching with relevance scoring.
/// It can be used to search strings, names, paths, etc.
#[derive(Debug, Clone)]
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    pub fn new() -> Self {
        Self
    }

    /// Calculate match score for a query against a single text string
    ///
    pub fn match_score(&self, query: &str, text: &str) -> MatchScore {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        // Empty query matches everything
        if query.is_empty() {
            return MatchScore::Substring;
        }

        // Exact match
        if text_lower == query_lower {
            return MatchScore::Exact;
        }

        // Prefix match
        if text_lower.starts_with(&query_lower) {
            return MatchScore::Prefix;
        }

        // Substring match
        if text_lower.contains(&query_lower) {
            return MatchScore::Substring;
        }

        MatchScore::None
    }

    /// Calculate match score for a query against multiple text fields
    ///
    /// This is useful when you want to search across multiple properties
    /// (e.g., name and description). Returns the highest score from all fields.
    ///
    pub fn match_score_multi(&self, query: &str, fields: &[&str]) -> MatchScore {
        fields
            .iter()
            .map(|&field| self.match_score(query, field))
            .max()
            .unwrap_or(MatchScore::None)
    }

    /// Filter and index items by query using a scoring function
    ///
    pub fn filter_and_rank<T, F>(
        &self,
        _query: &str,
        items: &[T],
        score_fn: F,
    ) -> Vec<(usize, MatchScore)>
    where
        F: Fn(&T) -> MatchScore,
    {
        let mut matches: Vec<(usize, MatchScore)> = items
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let score = score_fn(item);
                if score.is_match() {
                    Some((idx, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (descending)
        matches.sort_by_key(|a| std::cmp::Reverse(a.1));

        matches
    }

    /// Highlight matching characters in text for UI display
    ///
    /// Returns a `Line` with matching portions highlighted in yellow/bold.
    ///
    pub fn highlight_matches(&self, text: &str, query: &str) -> Line<'_> {
        if query.is_empty() {
            return Line::from(text.to_string());
        }

        // Case-insensitive match using the same lowercased text for both
        // finding and offset calculation. We compute byte offsets on
        // text_lower but must translate them back to original `text`.
        //
        // SAFETY: to_lowercase() can change byte lengths for some Unicode
        // (e.g., German ß → ss), so we cannot reuse byte offsets from
        // text_lower on text directly. Instead, we walk char-by-char.

        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        if !text_lower.contains(&query_lower) {
            return Line::from(text.to_string());
        }

        let mut spans = Vec::new();
        let mut search_from_char = 0;

        let text_chars: Vec<char> = text.chars().collect();
        let text_lower_chars: Vec<char> = text_lower.chars().collect();
        let query_lower_chars: Vec<char> = query_lower.chars().collect();
        let query_char_len = query_lower_chars.len();

        while search_from_char + query_char_len <= text_lower_chars.len() {
            let mut found = None;
            for i in search_from_char..=text_lower_chars.len() - query_char_len {
                let slice: String = text_lower_chars[i..i + query_char_len].iter().collect();
                if slice == query_lower {
                    found = Some(i);
                    break;
                }
            }

            let Some(match_char_start) = found else { break };

            let match_char_end = match_char_start + query_char_len;

            let byte_start: usize = text_chars[..search_from_char]
                .iter()
                .map(|c| c.len_utf8())
                .sum();
            let byte_match_start: usize = text_chars[..match_char_start]
                .iter()
                .map(|c| c.len_utf8())
                .sum();
            let byte_match_end: usize = text_chars[..match_char_end]
                .iter()
                .map(|c| c.len_utf8())
                .sum();

            if byte_match_start > byte_start {
                spans.push(Span::raw(text[byte_start..byte_match_start].to_string()));
            }

            spans.push(Span::styled(
                text[byte_match_start..byte_match_end].to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));

            search_from_char = match_char_end;
        }

        let byte_remaining: usize = text_chars[..search_from_char]
            .iter()
            .map(|c| c.len_utf8())
            .sum();
        if byte_remaining < text.len() {
            spans.push(Span::raw(text[byte_remaining..].to_string()));
        }

        if spans.is_empty() {
            Line::from(text.to_string())
        } else {
            Line::from(spans)
        }
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

// TESTS

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_score_exact() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("test", "test"), MatchScore::Exact);
        assert_eq!(matcher.match_score("Test", "test"), MatchScore::Exact); // Case-insensitive
    }

    #[test]
    fn test_match_score_prefix() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("tes", "test"), MatchScore::Prefix);
        assert_eq!(matcher.match_score("Tes", "test"), MatchScore::Prefix);
    }

    #[test]
    fn test_match_score_substring() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("es", "test"), MatchScore::Substring);
        assert_eq!(matcher.match_score("st", "test"), MatchScore::Substring);
    }

    #[test]
    fn test_match_score_none() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("xyz", "test"), MatchScore::None);
    }

    #[test]
    fn test_match_score_empty_query() {
        let matcher = FuzzyMatcher::new();
        // Empty query matches everything (returns Substring as default)
        assert_eq!(matcher.match_score("", "test"), MatchScore::Substring);
    }

    #[test]
    fn test_match_score_multi() {
        let matcher = FuzzyMatcher::new();
        let fields = vec!["test", "example", "demo"];

        // Should match "test" exactly
        assert_eq!(
            matcher.match_score_multi("test", &fields),
            MatchScore::Exact
        );

        // Should match "test" as prefix
        assert_eq!(
            matcher.match_score_multi("tes", &fields),
            MatchScore::Prefix
        );

        // Should match "example" as substring
        assert_eq!(
            matcher.match_score_multi("xa", &fields),
            MatchScore::Substring
        );

        // No match in any field
        assert_eq!(matcher.match_score_multi("xyz", &fields), MatchScore::None);
    }

    #[test]
    fn test_filter_and_rank() {
        let matcher = FuzzyMatcher::new();
        let items = vec!["test", "testing", "example", "contest"];

        let results =
            matcher.filter_and_rank("test", &items, |item| matcher.match_score("test", item));

        // Should return all matches sorted by score
        assert!(results.len() >= 2);

        // First result should be exact match ("test")
        let (first_idx, first_score) = &results[0];
        assert_eq!(*first_score, MatchScore::Exact);
        assert_eq!(items[*first_idx], "test");
    }

    #[test]
    fn test_highlight_matches() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("test example", "es");

        // Should create spans with highlighted "es" portions
        let spans = line.spans;
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_matches_empty_query() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("test", "");

        // Should return original text unchanged
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn test_match_score_is_match() {
        assert!(MatchScore::Exact.is_match());
        assert!(MatchScore::Prefix.is_match());
        assert!(MatchScore::Substring.is_match());
        assert!(!MatchScore::None.is_match());
    }

    #[test]
    fn test_match_score_as_u8() {
        assert_eq!(MatchScore::None.as_u8(), 0);
        assert_eq!(MatchScore::Substring.as_u8(), 1);
        assert_eq!(MatchScore::Prefix.as_u8(), 2);
        assert_eq!(MatchScore::Exact.as_u8(), 3);
    }

    #[test]
    fn test_match_score_ordering() {
        assert!(MatchScore::Exact > MatchScore::Prefix);
        assert!(MatchScore::Prefix > MatchScore::Substring);
        assert!(MatchScore::Substring > MatchScore::None);
    }

    #[test]
    fn test_highlight_utf8_multibyte() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("héllo wörld", "lö");
        let spans = line.spans;
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_cjk() {
        let matcher = FuzzyMatcher::new();
        let line = matcher.highlight_matches("日本語テスト", "テスト");
        let spans = line.spans;
        assert!(!spans.is_empty());
        let rendered: String = spans.iter().map(|s| s.content.clone()).collect();
        assert!(rendered.contains("テスト"));
    }

    #[test]
    fn test_match_score_unicode() {
        let matcher = FuzzyMatcher::new();
        assert_eq!(matcher.match_score("café", "CAFÉ"), MatchScore::Exact);
        assert_eq!(matcher.match_score("cafe", "café"), MatchScore::None);
    }

    #[test]
    fn test_highlight_no_panic_on_mixed_byte_widths() {
        let matcher = FuzzyMatcher::new();
        let _ = matcher.highlight_matches("naïve café résumé", "ca");
        let _ = matcher.highlight_matches("🎉 party 🎊", "party");
        let _ = matcher.highlight_matches("mixαβγ", "βγ");
    }
}
